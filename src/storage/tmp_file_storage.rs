use std::{fs, io::BufReader, io::BufWriter, path::PathBuf};

use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use tempfile::{TempDir, tempdir};

use crate::{storage::Storage, super_block::SuperBlock};

pub struct TmpFileStorage {
    dir: TempDir,
    counter: usize,
    max_file_size: usize,
}

impl TmpFileStorage {
    pub fn new() -> Result<Self> {
        // Create a directory inside of `env::temp_dir()`
        let dir = tempdir()?;

        Ok(Self {
            dir,
            counter: 0,
            max_file_size: 0,
        })
    }

    pub fn truncate(mut self) -> Result<()> {
        self.counter = 0;
        Ok(self.dir.close()?)
    }

    fn filepath(&self, key: usize) -> PathBuf {
        self.dir.path().join(format!("sblk{:06}.dat", key))
    }
}

impl Storage<usize, SuperBlock> for TmpFileStorage {
    type Error = anyhow::Error;

    fn insert(&mut self, key: &usize, sb: SuperBlock) -> Result<()> {
        let filepath = self.filepath(*key);
        let tmp_file = fs::File::create(filepath).context("create superblock file")?;
        let writer = BufWriter::new(tmp_file);
        self.counter += 1;

        let superblock_size = sb.size();
        if superblock_size > self.max_file_size {
            self.max_file_size = superblock_size;
        }

        encoding::encode_to_writer(&sb, writer).context("write superblock into a file")
    }

    fn get(&self, key: &usize) -> Result<Option<SuperBlock>> {
        let filepath = self.filepath(*key);
        let tmp_file = fs::File::open(filepath).context("read superblock file");
        let tmp_file = match tmp_file {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let reader = BufReader::new(tmp_file);

        let sb = match encoding::decode_from_read(reader) {
            Ok(sb) => sb,
            Err(e) => anyhow::bail!("error decoding superblock file: {}", e),
        };

        Ok(Some(sb))
    }

    fn count(&self) -> usize {
        self.counter
    }

    fn max_size(&self) -> usize {
        self.max_file_size
    }
}
