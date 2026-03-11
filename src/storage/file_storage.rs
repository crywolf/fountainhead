use std::{fs, io::BufReader, io::BufWriter};

use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;

use crate::{droplet::Droplet, storage::Storage};

pub struct FileStorage {
    dir: String,
    counter: usize,
    max_file_size: usize,
}

impl FileStorage {
    pub fn new(dir: &str, epoch: usize) -> Result<Self> {
        let epoch_dir = format!("{}/epoch{:06}", dir, epoch);
        fs::create_dir_all(&epoch_dir).context("FileStorage: create epoch dir to store files")?;

        let files_count = fs::read_dir(&epoch_dir)
            .with_context(|| format!("read dir {}", epoch_dir))?
            .count();

        Ok(Self {
            dir: epoch_dir,
            counter: files_count,
            max_file_size: 0,
        })
    }

    pub fn truncate(mut self) -> Result<()> {
        self.counter = 0;
        fs::remove_dir_all(self.dir).context("FileStorage: remove epoch dir")
    }

    fn filepath(&self, key: usize) -> String {
        let file_num = format!("{:06}", key);
        let file_path = format!("{}/drp{}.dat", self.dir, file_num);

        file_path
    }
}

impl Storage<usize, Droplet> for FileStorage {
    type Error = anyhow::Error;

    fn insert(&mut self, key: usize, droplet: Droplet) -> Result<()> {
        let filepath = self.filepath(key);
        let file = fs::File::create(filepath).context("FileStorage: create droplet file")?;
        let writer = BufWriter::new(file);
        self.counter += 1;

        let droplet_size = droplet.data_size();
        if droplet_size > self.max_file_size {
            self.max_file_size = droplet_size;
        }

        encoding::encode_to_writer(&droplet, writer)
            .context("FileStorage: write droplet into a file")
    }

    fn get(&self, key: usize) -> Result<Droplet> {
        let filepath = self.filepath(key);
        let file = fs::File::open(filepath).context("FileStorage: read droplet file")?;
        let reader = BufReader::new(file);

        let droplet = match encoding::decode_from_read(reader) {
            Ok(droplet) => droplet,
            Err(e) => anyhow::bail!("FileStorage: error decoding droplet file: {}", e),
        };

        Ok(droplet)
    }

    fn count(&self) -> usize {
        self.counter
    }

    fn max_size(&self) -> usize {
        self.max_file_size
    }
}
