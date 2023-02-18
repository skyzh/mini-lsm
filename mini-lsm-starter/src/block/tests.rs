use std::sync::Arc;

use super::builder::BlockBuilder;
use super::iterator::BlockIterator;
use super::*;

#[test]
fn block_build_single_key() {
    let mut builder = BlockBuilder::new(16);
    assert!(builder.add(b"233", b"233333"));
    builder.build();
}

#[test]
#[should_panic]
fn empty_block_fails_to_build() {
    let builder = BlockBuilder::new(16);
    builder.build();
}

#[test]
#[should_panic]
fn cannot_add_gigantic_key() {
    let mut builder = BlockBuilder::new(16);
    let _ = builder.add(&[0u8; u16::MAX as usize + 1], b"11");
}

#[test]
#[should_panic]
fn cannot_add_empty_key() {
    let mut builder = BlockBuilder::new(16);
    let _ = builder.add(b"", b"11");
}

#[test]
#[should_panic]
fn cannot_add_gigantic_value() {
    let mut builder = BlockBuilder::new(16);
    let _ = builder.add(b"11", &[0u8; u16::MAX as usize + 1]);
}

#[test]
fn block_build_full() {
    // set a block size just a bit too small to fit both entries
    let mut builder = BlockBuilder::new(21);
    assert!(builder.add(b"11", b"11"));
    assert!(!builder.add(b"22", b"22"));
    builder.build();

    // set a block size exactly large enough to fit both entries
    let mut builder = BlockBuilder::new(22);
    assert!(builder.add(b"11", b"11"));
    assert!(builder.add(b"22", b"22"));
    builder.build();
}

fn key_of(idx: usize) -> Vec<u8> {
    format!("key_{:03}", idx * 5).into_bytes()
}

fn value_of(idx: usize) -> Vec<u8> {
    format!("value_{:010}", idx).into_bytes()
}

fn num_of_keys() -> usize {
    100
}

fn generate_block() -> Block {
    let mut builder = BlockBuilder::new(10000);
    for idx in 0..num_of_keys() {
        let key = key_of(idx);
        let value = value_of(idx);
        assert!(builder.add(&key[..], &value[..]));
    }
    builder.build()
}

#[test]
fn block_build_all() {
    generate_block();
}

#[test]
fn block_encode_decode_idempotence() {
    let block = generate_block();
    let encoded = block.encode();
    let decoded_block = Block::decode(&encoded);
    assert_eq!(block.offsets, decoded_block.offsets);
    assert_eq!(block.data, decoded_block.data);
}

#[test]
fn test_encode() {
    let mut bb = BlockBuilder::new(4000);
    assert!(bb.add(b"key1", b"mergez"));
    assert!(bb.add(b"key2", b"sausage"));
    let block = bb.build();

    let encoded = block.encode();
    assert_eq!(
        encoded,
        as_bytes(b"\0\x04key1\0\x06mergez\0\x04key2\0\x07sausage\0\0\0\x0e\0\x02")
    );
}

fn as_bytes(x: &[u8]) -> Bytes {
    Bytes::copy_from_slice(x)
}

#[test]
fn test_block_iterator() {
    let block = Arc::new(generate_block());
    let mut iter = BlockIterator::create_and_seek_to_first(block);
    for _ in 0..5 {
        for i in 0..num_of_keys() {
            let key = iter.key();
            let value = iter.value();
            assert_eq!(
                key,
                key_of(i),
                "expected key: {:?}, actual key: {:?}",
                as_bytes(&key_of(i)),
                as_bytes(key)
            );
            assert_eq!(
                value,
                value_of(i),
                "expected value: {:?}, actual value: {:?}",
                as_bytes(&value_of(i)),
                as_bytes(value)
            );
            iter.next();
        }
        iter.seek_to_first();
    }
}

#[test]
fn test_block_seek_key() {
    let block = Arc::new(generate_block());
    let mut iter = BlockIterator::create_and_seek_to_key(block, &key_of(0));
    for offset in 1..=5 {
        for i in 0..num_of_keys() {
            let key = iter.key();
            let value = iter.value();
            assert_eq!(
                key,
                key_of(i),
                "expected key: {:?}, actual key: {:?}",
                as_bytes(&key_of(i)),
                as_bytes(key)
            );
            assert_eq!(
                value,
                value_of(i),
                "expected value: {:?}, actual value: {:?}",
                as_bytes(&value_of(i)),
                as_bytes(value)
            );
            iter.seek_to_key(&format!("key_{:03}", i * 5 + offset).into_bytes());
        }
        iter.seek_to_key(b"k");
    }
}
