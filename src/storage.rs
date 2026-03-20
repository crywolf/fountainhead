pub mod file_storage;
pub mod tmp_file_storage;

pub trait Storage<K, V> {
    type Error;

    /// Inserts an item to the storage
    fn insert(&mut self, key: &K, item: V) -> Result<(), Self::Error>;

    /// Returns the item corresponding to the key
    fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the number of items in the storage
    fn count(&self) -> usize;

    /// Returns the maximum size of the stored items
    fn max_size(&self) -> usize;
}
