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

mod builder;
mod iterator;

use anyhow::{Context, Result, ensure};
pub use builder::BlockBuilder;
use bytes::{BufMut, Bytes};
pub use iterator::BlockIterator;

pub(crate) const SIZEOF_U16: usize = std::mem::size_of::<u16>();

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted
/// key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    pub fn encode(&self) -> Bytes {
        let mut buf = self.data.clone();
        let offsets_len = self.offsets.len();
        for offset in &self.offsets {
            buf.put_u16(*offset);
        }
        // Adds number of elements at the end of the block
        buf.put_u16(offsets_len as u16);
        buf.into()
    }

    pub fn decode(data: &[u8]) -> Self {
        Self::decode_checked(data).expect("invalid block encoding")
    }

    pub(crate) fn decode_checked(data: &[u8]) -> Result<Self> {
        // get number of elements in the block
        ensure!(data.len() >= SIZEOF_U16, "block footer is truncated");
        let entry_offsets_len =
            u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]]) as usize;
        ensure!(entry_offsets_len > 0, "block has no entries");
        let offsets_size = entry_offsets_len
            .checked_mul(SIZEOF_U16)
            .context("block offset table is too large")?;
        let footer_size = offsets_size
            .checked_add(SIZEOF_U16)
            .context("block footer is too large")?;
        ensure!(footer_size <= data.len(), "block offset table is truncated");
        let data_end = data.len() - footer_size;
        let offsets_raw = &data[data_end..data.len() - SIZEOF_U16];
        // get offset array
        let offsets: Vec<u16> = offsets_raw
            .chunks(SIZEOF_U16)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
            .collect();
        ensure!(
            offsets[0] == 0,
            "first block entry must start at offset zero"
        );
        ensure!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "block entry offsets are not strictly increasing"
        );
        ensure!(
            offsets.iter().all(|offset| usize::from(*offset) < data_end),
            "block entry offset is outside the data section"
        );

        let mut first_key_len = None;
        for (idx, offset) in offsets.iter().enumerate() {
            let entry_start = usize::from(*offset);
            let entry_end = offsets
                .get(idx + 1)
                .map_or(data_end, |offset| usize::from(*offset));
            let entry = &data[entry_start..entry_end];
            ensure!(entry.len() >= 14, "block entry header is truncated");
            let overlap = u16::from_be_bytes([entry[0], entry[1]]) as usize;
            let key_len = u16::from_be_bytes([entry[2], entry[3]]) as usize;
            if idx == 0 {
                ensure!(overlap == 0, "first block key has a nonzero overlap");
                first_key_len = Some(key_len);
            } else {
                ensure!(
                    overlap <= first_key_len.context("block is missing its first key")?,
                    "block key overlap exceeds the first key"
                );
            }
            let key_end = 4usize
                .checked_add(key_len)
                .context("block key length overflow")?;
            let value_len_offset = key_end
                .checked_add(std::mem::size_of::<u64>())
                .context("block timestamp offset overflow")?;
            let value_len_end = value_len_offset
                .checked_add(SIZEOF_U16)
                .context("block value header overflow")?;
            ensure!(
                value_len_end <= entry.len(),
                "block key, timestamp, or value length is truncated"
            );
            let value_len =
                u16::from_be_bytes([entry[value_len_offset], entry[value_len_offset + 1]]) as usize;
            let entry_len = value_len_end
                .checked_add(value_len)
                .context("block value length overflow")?;
            ensure!(entry_len == entry.len(), "block value length is invalid");
        }
        // retrieve data
        let data = data[0..data_end].to_vec();
        Ok(Self { data, offsets })
    }
}
