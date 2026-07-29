// Copyright (c) 2022-2026 Alex Chi Z
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

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{BufMut, Bytes};
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the course
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        assert!(self.offsets.len() <= u16::MAX as usize);
        let mut encoded =
            Vec::with_capacity(self.data.len() + (self.offsets.len() + 1) * size_of::<u16>());
        encoded.extend_from_slice(&self.data);
        for offset in &self.offsets {
            encoded.put_u16(*offset);
        }
        encoded.put_u16(self.offsets.len() as u16);
        encoded.into()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        assert!(data.len() >= size_of::<u16>(), "block footer is missing");
        let count = u16::from_be_bytes(data[data.len() - 2..].try_into().unwrap()) as usize;
        let footer_len = (count + 1)
            .checked_mul(size_of::<u16>())
            .expect("block footer length overflow");
        assert!(
            footer_len <= data.len(),
            "block offset table is out of bounds"
        );
        let data_end = data.len() - footer_len;
        let mut offsets = Vec::with_capacity(count);
        for chunk in data[data_end..data.len() - 2].chunks_exact(2) {
            offsets.push(u16::from_be_bytes(chunk.try_into().unwrap()));
        }
        assert_eq!(offsets.len(), count);
        if let Some(first) = offsets.first() {
            assert_eq!(*first, 0, "first block entry must start at offset zero");
        }
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "block offsets must be strictly increasing"
        );
        assert!(
            offsets.iter().all(|offset| (*offset as usize) < data_end),
            "block entry offset is outside the data section"
        );
        Self {
            data: data[..data_end].to_vec(),
            offsets,
        }
    }
}
