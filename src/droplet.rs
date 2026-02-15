use anyhow::{Context, Result};
use bitcoinkernel::Block;

use bitcoin_consensus_encoding as encoding;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeEncoder, Decodable, Decoder,
    Decoder2, Encodable, Encoder2,
};

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
    pub struct DropletEncoder<'e>(Encoder2<CompactSizeEncoder, Encoder2<CompactSizeEncoder, BytesEncoder<'e>>>);
}

impl Encodable for Droplet {
    type Encoder<'e>
        = DropletEncoder<'e>
    where
        Self: 'e;

    fn encoder(&self) -> Self::Encoder<'_> {
        let num = CompactSizeEncoder::new(self.num as usize);

        let data = Encoder2::new(
            CompactSizeEncoder::new(self.data.len()),
            BytesEncoder::without_length_prefix(self.data.as_ref()),
        );

        DropletEncoder(Encoder2::new(num, data))
    }
}

pub struct DropletDecoder {
    decoder: Option<Decoder2<CompactSizeDecoder, ByteVecDecoder>>,
    num: usize,
    data: Vec<u8>,
}

impl Decodable for Droplet {
    type Decoder = DropletDecoder;

    fn decoder() -> Self::Decoder {
        let num_decoder = CompactSizeDecoder::new();
        let data_decoder = ByteVecDecoder::new();
        Self::Decoder {
            decoder: Some(Decoder2::new(num_decoder, data_decoder)),
            num: 0,
            data: Vec::default(),
        }
    }
}

impl Decoder for DropletDecoder {
    type Output = Droplet;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        if let Some(mut decoder) = self.decoder.take() {
            if decoder.push_bytes(bytes)? {
                self.decoder = Some(decoder);
                return Ok(true);
            }

            let (num, data) = decoder.end()?;
            self.num = num;
            self.data = data;

            self.decoder = None;
        }

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
        if let Some(decoder) = &self.decoder {
            decoder.read_limit()
        } else {
            0
        }
    }
}
