use anyhow::Result;

use bitcoin_consensus_encoding as encoding;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeDecoderError, CompactSizeEncoder,
    Decodable, Decoder, Decoder4, Encodable, Encoder2, Encoder4, SliceEncoder, VecDecoder,
};

use crate::padded_block::PaddedBlock;

pub struct Droplet {
    /// Droplet number
    pub num: usize,
    /// Indices (block heights) of blocks included in this droplet
    pub neighbors: Vec<Neighbor>,
    /// Size of encoded block in bytes
    pub block_size: usize,
    /// Size of encoded padded data in bytes
    pub data_size: usize,
    /// Padded block data
    data: Vec<u8>,
}

impl Droplet {
    pub fn new(num: usize, neighbors: Vec<Neighbor>, padded_block: PaddedBlock) -> Result<Self> {
        let block_size = padded_block.block_size;

        let data = padded_block.data;
        let data_size = data.len();

        Ok(Self {
            num,
            neighbors,
            block_size,
            data_size,
            data,
        })
    }

    #[allow(dead_code)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn as_block_bytes(&self) -> &[u8] {
        &self.data[0..self.block_size]
    }
}

/// Block number
#[derive(Clone)]
pub struct Neighbor(usize);

impl std::fmt::Debug for Neighbor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Neighbor {
    pub fn new(block: usize) -> Self {
        Self(block)
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`Neighbor`] type.
    pub struct NeighborEncoder(CompactSizeEncoder);
}

impl Encodable for Neighbor {
    type Encoder<'e> = NeighborEncoder;

    fn encoder(&self) -> Self::Encoder<'_> {
        NeighborEncoder(CompactSizeEncoder::new(self.0))
    }
}

impl Decodable for Neighbor {
    type Decoder = NeighborDecoder;

    fn decoder() -> Self::Decoder {
        NeighborDecoder(CompactSizeDecoder::new())
    }
}

/// The decoder for the [`Neighbor`] type.
pub struct NeighborDecoder(CompactSizeDecoder);

impl Decoder for NeighborDecoder {
    type Output = Neighbor;
    type Error = CompactSizeDecoderError;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        self.0.push_bytes(bytes)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(Neighbor(self.0.end()?))
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`Droplet`] type.
    pub struct DropletEncoder<'e>(Encoder4<CompactSizeEncoder, Encoder2<CompactSizeEncoder, SliceEncoder<'e, Neighbor>>, CompactSizeEncoder, Encoder2<CompactSizeEncoder, BytesEncoder<'e>>>);
}

impl Encodable for Droplet {
    type Encoder<'e>
        = DropletEncoder<'e>
    where
        Self: 'e;

    fn encoder(&self) -> Self::Encoder<'_> {
        let num = CompactSizeEncoder::new(self.num);

        let neighbors = Encoder2::new(
            CompactSizeEncoder::new(self.neighbors.len()),
            SliceEncoder::without_length_prefix(self.neighbors.as_ref()),
        );

        let block_size = CompactSizeEncoder::new(self.block_size);

        let data = Encoder2::new(
            CompactSizeEncoder::new(self.data.len()),
            BytesEncoder::without_length_prefix(self.data.as_ref()),
        );

        DropletEncoder(Encoder4::new(num, neighbors, block_size, data))
    }
}

/// The decoder for the [`Droplet`] type.
pub struct DropletDecoder {
    decoder: Option<
        Decoder4<CompactSizeDecoder, VecDecoder<Neighbor>, CompactSizeDecoder, ByteVecDecoder>,
    >,
    num: usize,
    neighbors: Vec<Neighbor>,
    block_size: usize,
    data: Vec<u8>,
}

impl Decodable for Droplet {
    type Decoder = DropletDecoder;

    fn decoder() -> Self::Decoder {
        let num_decoder = CompactSizeDecoder::new();
        let neighbors_decoder = VecDecoder::new();
        let block_size_decoder = CompactSizeDecoder::new();
        let data_decoder = ByteVecDecoder::new();

        Self::Decoder {
            decoder: Some(Decoder4::new(
                num_decoder,
                neighbors_decoder,
                block_size_decoder,
                data_decoder,
            )),
            num: 0,
            neighbors: Vec::default(),
            block_size: 0,
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

            let (num, neighbors, block_size, data) = decoder.end()?;
            self.num = num;
            self.neighbors = neighbors;
            self.block_size = block_size;
            self.data = data;

            self.decoder = None;
        }

        Ok(false)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(Droplet {
            num: self.num,
            neighbors: self.neighbors,
            block_size: self.block_size,
            data_size: self.data.len(),
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
