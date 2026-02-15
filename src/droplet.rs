use anyhow::{Context, Result};
use bitcoinkernel::Block;

use bitcoin_consensus_encoding::{
    self as encoding, CompactSizeDecoder, CompactSizeEncoder, Decoder, Encoder2,
};
use encoding::{BytesEncoder, Decodable, Encodable};

pub struct Droplet {
    pub num: i32,
    pub size: usize,
    data: Vec<u8>,
}

impl Droplet {
    pub fn new(num: i32, block: Block) -> Result<Self> {
        let data = block.consensus_encode().context("consensus_encode")?;
        let size = data.len();
        Ok(Self { num, size, data })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`Droplet`] type.
    pub struct DropletEncoder<'e>(Encoder2<CompactSizeEncoder, BytesEncoder<'e>>);
}

impl Encodable for Droplet {
    type Encoder<'e>
        = DropletEncoder<'e>
    where
        Self: 'e;

    fn encoder(&self) -> Self::Encoder<'_> {
        let num = CompactSizeEncoder::new(self.num as usize);
        let data = BytesEncoder::without_length_prefix(self.data.as_ref());

        DropletEncoder(Encoder2::new(num, data))
    }
}

#[derive(Default)]
pub struct DropletDecoder {
    num: usize,
    data: Vec<u8>,
}

impl Decodable for Droplet {
    type Decoder = DropletDecoder;

    fn decoder() -> Self::Decoder {
        Self::Decoder::default()
    }
}

impl Decoder for DropletDecoder {
    type Output = Droplet;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        let mut num_dec = CompactSizeDecoder::default();
        num_dec.push_bytes(bytes)?;
        self.num = num_dec.end()?;

        self.data.extend_from_slice(bytes);
        Ok(false)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(Droplet {
            num: self.num as i32,
            size: self.data.len(),
            data: self.data,
        })
    }

    fn read_limit(&self) -> usize {
        todo!()
    }
}
