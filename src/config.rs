pub struct Config {
    pub droplets_dir: String,
    pub input_data_dir: String,
    pub output_data_dir: String,
    /// Number of worker threads for block validation
    pub worker_threads: i32,
}
