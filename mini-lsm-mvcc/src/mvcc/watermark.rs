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

use std::collections::BTreeMap;

pub struct Watermark {
    readers: BTreeMap<u64, usize>,
}

impl Default for Watermark {
    fn default() -> Self {
        Self::new()
    }
}

impl Watermark {
    pub fn new() -> Self {
        Self {
            readers: BTreeMap::new(),
        }
    }

    pub fn add_reader(&mut self, ts: u64) {
        *self.readers.entry(ts).or_default() += 1;
    }

    pub fn remove_reader(&mut self, ts: u64) {
        let cnt = self.readers.get_mut(&ts).unwrap();
        *cnt -= 1;
        if *cnt == 0 {
            self.readers.remove(&ts);
        }
    }

    pub fn num_retained_snapshots(&self) -> usize {
        self.readers.len()
    }

    pub fn watermark(&self) -> Option<u64> {
        self.readers.first_key_value().map(|(ts, _)| *ts)
    }
}
