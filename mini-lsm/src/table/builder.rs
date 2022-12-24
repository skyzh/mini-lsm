use anyhow::Result;
use bytes::BufMut;
use std::path::Path;

use super::{BlockMeta, FileObject, SsTable};
use crate::block::BlockBuilder;

pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: Vec<u8>,
    data: Vec<u8>,
    pub(super) meta: Vec<BlockMeta>,
    target_size: usize,
    block_size: usize,
}

impl SsTableBuilder {
    pub fn new(target_size: usize, block_size: usize) -> Self {
        Self {
            data: Vec::new(),
            meta: Vec::new(),
            first_key: Vec::new(),
            target_size,
            block_size,
            builder: BlockBuilder::new(block_size),
        }
    }

    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        if self.data.len() > self.target_size {
            return false;
        }

        if self.first_key.is_empty() {
            self.first_key = key.to_vec();
        }

        if self.builder.add(key, value) {
            return true;
        }
        // create a new block builder and append block data
        self.finish_block();

        // add the key-value pair to the next block
        assert!(self.builder.add(key, value));
        self.first_key = key.to_vec();

        true
    }

    fn finish_block(&mut self) {
        let builder = std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size));
        let encoded_block = builder.build().encode();
        self.meta.push(BlockMeta {
            offset: self.data.len(),
            first_key: std::mem::take(&mut self.first_key).into(),
        });
        self.data.extend(encoded_block);
    }

    pub fn build(mut self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.finish_block();
        let mut buf = self.data;
        let meta_offset = buf.len();
        BlockMeta::encode_block_meta(&self.meta, &mut buf);
        buf.put_u32(meta_offset as u32);
        let file = FileObject::create(path.as_ref(), buf)?;
        Ok(SsTable {
            file,
            block_metas: self.meta,
            block_meta_offset: meta_offset,
        })
    }
}
