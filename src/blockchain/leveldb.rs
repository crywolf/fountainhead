use std::cell::RefCell;

use anyhow::{Context, Result};
use bitcoinkernel::BlockHeader;
use rusty_leveldb::{DB, Options};

pub struct LevelDB {
    database: RefCell<DB>,
}

impl LevelDB {
    pub fn open(dir: &str) -> Result<Self> {
        let db_path = std::path::Path::new(dir).join("blocks").join("index");

        let options = Options {
            create_if_missing: false,
            ..Default::default()
        };

        let database = RefCell::new(DB::open(&db_path, options).context("open leveldb database")?);

        Ok(Self { database })
    }

    pub fn lookup_header(&self, block_hash: &[u8; 32]) -> Result<Option<BlockHeader>> {
        // LevelDB keys for block index entries follow a specific format
        // The key format is: 'b' + block_hash (in little-endian)
        let mut key = vec![b'b'];
        key.extend_from_slice(block_hash);

        match self.database.borrow_mut().get(&key) {
            Some(bytes) => {
                // First X bytes contain some metadata => ignore them, last 80 bytes are the header bytes
                let header = BlockHeader::new(&bytes[bytes.len() - 80..])
                    .context("create header from bytes")
                    .inspect_err(|e| log::error!("Error looking up block header: {}", e))?;
                Ok(Some(header))
            }
            None => Ok(None),
        }
    }
}
