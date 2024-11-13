mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes};
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
        // get number of elements in the block
        let entry_offsets_len = (&data[data.len() - SIZEOF_U16..]).get_u16() as usize;
        let data_end = data.len() - SIZEOF_U16 - entry_offsets_len * SIZEOF_U16;
        let offsets_raw = &data[data_end..data.len() - SIZEOF_U16];
        // get offset array
        let offsets = offsets_raw
            .chunks(SIZEOF_U16)
            .map(|mut x| x.get_u16())
            .collect();
        // retrieve data
        let data = data[0..data_end].to_vec();
        Self { data, offsets }
    }

    pub fn print(&self) {
        let sep = "=".repeat(10);
        println!("{} {} {}", sep, format!("{:^10}", "Block"), sep);

        let mut first_key: &[u8] = &[];
        for (i, &offset) in self.offsets.iter().enumerate() {
            let entry_start = offset as usize;
            // Determine next offset or end of data for entry boundary
            let entry_end = if i + 1 < self.offsets.len() {
                self.offsets[i + 1] as usize
            } else {
                self.data.len()
            };

            // Extracting the entry data
            let entry_data = &self.data[entry_start..entry_end];

            // Decode the overlap
            let key_overlap = u16::from_be_bytes([entry_data[0], entry_data[1]]) as usize;

            // Decode the rest of the key length
            let rest_of_key_len = u16::from_be_bytes([entry_data[2], entry_data[3]]) as usize;

            // Decode the rest of the key
            let rest_of_key = &entry_data[4..4 + rest_of_key_len];

            if first_key.len() == 0 {
                first_key = rest_of_key;
            }
            let full_key = &mut first_key[..key_overlap].to_vec();
            full_key.extend_from_slice(rest_of_key);

            // Decode the value length
            let value_length_pos = 4 + rest_of_key_len;
            let value_length = u16::from_be_bytes([
                entry_data[value_length_pos],
                entry_data[value_length_pos + 1],
            ]) as usize;

            // Decode the value
            let value = &entry_data[value_length_pos + 2..value_length_pos + 2 + value_length];

            // Print key and value
            println!("{:<10}: {}", "Index", i);
            println!("{:<10}: {}", "Key", String::from_utf8_lossy(full_key));
            println!("{:<10}: {}", "Value", String::from_utf8_lossy(value));
            println!("{}", "-".repeat(20));
        }
        println!("Offsets: {:?}", self.offsets);
        println!("{} {} {}", sep, format!("{:^10}", "End Block"), sep);
    }
}
