pub struct Config {
    pub droplets_dir: String,
    pub input_data_dir: String,
    pub output_data_dir: String,
    /// How many blocks are produced in an epoch.
    /// An epoch is defined as the time required for the blockchain to grow by `k` blocks (e.g., `k` = 10000).
    pub blocks_per_epoch: usize,
    /// Number of epochs to encode
    pub epochs_to_encode: usize,
    /// Number of worker threads for block validation
    pub worker_threads: i32,
}
