#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use super::Block;

pub struct BlockIterator {}

impl BlockIterator {
    fn new(block: Arc<Block>) -> Self {
        unimplemented!()
    }

    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        unimplemented!()
    }

    pub fn create_and_seek_to_key(block: Arc<Block>, key: &[u8]) -> Self {
        unimplemented!()
    }

    pub fn key(&self) -> &[u8] {
        unimplemented!()
    }

    pub fn value(&self) -> &[u8] {
        unimplemented!()
    }

    pub fn is_valid(&self) -> bool {
        unimplemented!()
    }

    pub fn seek_to_first(&mut self) {
        unimplemented!()
    }

    pub fn seek_to_last(&mut self) {
        unimplemented!()
    }

    pub fn len(&self) -> usize {
        unimplemented!()
    }

    pub fn is_empty(&self) -> bool {
        unimplemented!()
    }

    pub fn seek_to(&mut self, idx: usize) {
        unimplemented!()
    }

    pub fn next(&mut self) {
        unimplemented!()
    }

    pub fn seek_to_key(&mut self, key: &[u8]) {
        unimplemented!()
    }
}
