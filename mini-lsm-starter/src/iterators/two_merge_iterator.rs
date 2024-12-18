use anyhow::Result;

use super::StorageIterator;

#[derive(PartialEq, Debug)]
enum Current {
    A = 0,
    B = 1,
}

/// Merges two iterators of different types into one. If the two iterators have the same key, only
/// produce the key once and prefer the entry from A.
pub struct TwoMergeIterator<A: StorageIterator, B: StorageIterator> {
    a: A,
    b: B,
    current: Current,
}

impl<
        A: 'static + StorageIterator,
        B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
    > TwoMergeIterator<A, B>
{
    pub fn create(a: A, b: B) -> Result<Self> {
        let current = if !a.is_valid() || !b.is_valid() {
            if a.is_valid() {
                Current::A
            } else {
                Current::B
            }
        } else if a.key() > b.key() {
            Current::B
        } else {
            Current::A
        };
        Ok(Self { a, b, current })
    }
}

impl<
        A: 'static + StorageIterator,
        B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
    > StorageIterator for TwoMergeIterator<A, B>
{
    type KeyType<'a> = A::KeyType<'a>;

    fn key(&self) -> Self::KeyType<'_> {
        if self.current == Current::A {
            self.a.key()
        } else {
            self.b.key()
        }
    }

    fn value(&self) -> &[u8] {
        if self.current == Current::A {
            self.a.value()
        } else {
            self.b.value()
        }
    }

    fn is_valid(&self) -> bool {
        if self.current == Current::A {
            self.a.is_valid()
        } else {
            self.b.is_valid()
        }
    }

    fn next(&mut self) -> Result<()> {
        if self.a.is_valid() && self.b.is_valid() && self.a.key() == self.b.key() {
            self.a.next()?;
            self.b.next()?;
        } else if self.current == Current::A {
            self.a.next()?;
        } else {
            self.b.next()?;
        }

        if !self.a.is_valid() || !self.b.is_valid() {
            self.current = if self.a.is_valid() {
                Current::A
            } else {
                Current::B
            };

            return Ok(());
        }

        self.current = if self.a.key() <= self.b.key() {
            Current::A
        } else {
            Current::B
        };

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.a.num_active_iterators() + self.b.num_active_iterators()
    }
}
