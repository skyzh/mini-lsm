use std::path::Path;

use anyhow::Result;

pub struct Wal {}

impl Wal {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        unimplemented!()
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        Ok(())
    }
}
