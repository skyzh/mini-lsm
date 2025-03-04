// Copyright (c) 2022-2025 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::table::bloom::Bloom;
use crate::table::FileObject;
use anyhow::Result;
use bytes::{Buf, BufMut};
use std::path::Path;
use std::sync::Arc;

use super::{BlockMeta, SsTable};
use crate::{
    block::{Block, BlockBuilder},
    key::{KeyBytes, KeySlice, KeyVec},
    lsm_storage::BlockCache,
};

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: KeyVec,
    last_key: KeyVec,
    data: Vec<u8>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
    keys_encoded: Vec<u32>,
    max_ts: u64,
}

impl SsTableBuilder {
    /// Create a builder based on target block size.
    pub fn new(block_size: usize) -> Self {
        Self {
            builder: BlockBuilder::new(block_size),
            first_key: KeyVec::new(),
            last_key: KeyVec::new(),
            data: Vec::new(),
            meta: Vec::new(),
            block_size,
            keys_encoded: Vec::new(),
            max_ts: 0,
        }
    }

    /// Adds a key-value pair to SSTable.
    ///
    /// Note: You should split a new block when the current block is full.(`std::mem::replace` may
    /// be helpful here)
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        self.keys_encoded
            .push(farmhash::fingerprint32(key.key_ref()));
        self.max_ts = self.max_ts.max(key.ts());
        if self.first_key.is_empty() {
            self.first_key.append(key.key_ref());
            self.first_key.set_ts(key.ts())
        }

        if !self.builder.add(key, value) {
            self.detach_block();

            if !self.builder.add(key, value) {
                panic!("Adding is falling apart")
            }
        }

        self.last_key.clear();
        self.last_key.append(key.key_ref());
        self.last_key.set_ts(key.ts());
    }

    fn detach_block(&mut self) {
        // replace the old block builder with a fresh one
        let old_builder = std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size));
        let old_block = old_builder.build();

        // Get the first key by extracting the block and the last one in Self
        self.meta.push(BlockMeta {
            offset: self.data.len(),
            first_key: SsTableBuilder::first_key_extractor(&old_block),
            last_key: self.last_key.clone().into_key_bytes(),
        });

        let old_block_encoded = old_block.encode();
        self.data.extend(old_block_encoded.as_ref());
        let crc = crc32fast::hash(&old_block_encoded);
        self.data.extend_from_slice(&crc.to_be_bytes());
    }

    fn first_key_extractor(block: &Block) -> KeyBytes {
        let key_len = (&block.data[2..4]).get_u16() as usize;
        let mut key = KeyVec::new();
        key.append(&block.data[4..4 + key_len]);
        key.set_ts((&block.data[4 + key_len..4 + key_len + 8]).get_u64());
        key.into_key_bytes()
    }

    /// Get the estimated size of the SSTable.
    ///
    /// Since the data blocks contain much more data than meta blocks, just return the size of data
    /// blocks here.
    pub fn estimated_size(&self) -> usize {
        self.data.len()
    }

    /// Builds the SSTable and writes it to the given path. Use the `FileObject` structure to manipulate the disk objects.
    pub fn build(
        mut self,
        id: usize,
        block_cache: Option<Arc<BlockCache>>,
        path: impl AsRef<Path>,
    ) -> Result<SsTable> {
        // Encode the lastly built block
        self.detach_block();

        //Check if the table is empty
        if self.data.is_empty() {
            panic!("Cannot build an empty SSTable")
        }

        // Encode the SSTable before storage
        let len = self.data.len();
        BlockMeta::encode_block_meta(&self.meta, &mut self.data);
        self.data.put_u32(len as u32);

        let len_bloom = self.keys_encoded.len();
        let bloom = Some(Bloom::build_from_key_hashes(
            &self.keys_encoded,
            Bloom::bloom_bits_per_key(len_bloom, 0.01),
        ));

        let len_tot = self.data.len();
        bloom.as_ref().unwrap().encode(&mut self.data);
        self.data.put_u32(len_tot as u32);

        self.data.put_u64(self.max_ts);

        Ok(SsTable {
            file: FileObject::create(path.as_ref(), self.data)?,
            block_meta: self.meta,
            block_meta_offset: len,
            id,
            bloom,
            block_cache: None,
            first_key: self.first_key.into_key_bytes(),
            last_key: self.last_key.into_key_bytes(),
            max_ts: self.max_ts,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
