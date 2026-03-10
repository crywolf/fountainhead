pub mod tmp_file_storage;

pub trait Storage<K, V> {
    type Error;

    fn insert(&mut self, key: K, value: V) -> Result<(), Self::Error>;
    fn get(&self, key: K) -> Result<V, Self::Error>;
    fn count(&self) -> usize;
    fn max_size(&self) -> usize;
}
