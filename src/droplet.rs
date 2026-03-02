use anyhow::Context;
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeDecoderError, CompactSizeEncoder,
    Decodable, Decoder, Decoder3, Encodable, Encoder2, Encoder3, SliceEncoder, VecDecoder,
};

use crate::super_block::{EncodedBlocks, SuperBlock};

pub struct Droplet {
    /// Droplet number
    pub num: usize,
    /// Indices of suuperblocks included in this droplet
    pub neighbors: Vec<Neighbor>,
    /// Padded super block data
    data: Vec<u8>,
}

impl Droplet {
    pub fn new(num: usize, neighbors: Vec<Neighbor>, superblock: SuperBlock) -> Self {
        let data = superblock.into_encoded_bytes();

        Self {
            num,
            neighbors,
            data,
        }
    }

    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn to_blocks(&self) -> anyhow::Result<Vec<Block>> {
        let mut blocks = Vec::new();

        let encoded_blocks: EncodedBlocks = encoding::decode_from_slice(&self.data)
            .context("decode encoded blocks from droplet data")?;

        let encoded_blocks = encoded_blocks.into_vec();

        for enc_block in encoded_blocks {
            blocks.push(
                enc_block
                    .to_block()
                    .context("droplet: get block from encoded block")?,
            )
        }

        Ok(blocks)
    }

    pub fn encode_to_bytes(&self) -> Vec<u8> {
        encoding::encode_to_vec(self)
    }

    pub fn decode_from_bytes(encoded_droplet: &[u8]) -> anyhow::Result<Self> {
        encoding::decode_from_slice(encoded_droplet)
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
    pub struct NeighborEncoder<'e>(CompactSizeEncoder);
}

impl Encodable for Neighbor {
    type Encoder<'e> = NeighborEncoder<'e>;

    fn encoder(&self) -> Self::Encoder<'_> {
        NeighborEncoder::new(CompactSizeEncoder::new(self.0))
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
    pub struct DropletEncoder<'e>(Encoder3<CompactSizeEncoder, Encoder2<CompactSizeEncoder, SliceEncoder<'e, Neighbor>>,  Encoder2<CompactSizeEncoder, BytesEncoder<'e>>>);
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

        let data = Encoder2::new(
            CompactSizeEncoder::new(self.data.len()),
            BytesEncoder::without_length_prefix(self.data.as_ref()),
        );

        DropletEncoder::new(Encoder3::new(num, neighbors, data))
    }
}

/// The decoder for the [`Droplet`] type.
pub struct DropletDecoder(Decoder3<CompactSizeDecoder, VecDecoder<Neighbor>, ByteVecDecoder>);

impl Decodable for Droplet {
    type Decoder = DropletDecoder;

    fn decoder() -> Self::Decoder {
        let num_decoder = CompactSizeDecoder::new();
        let neighbors_decoder = VecDecoder::new();
        let data_decoder = ByteVecDecoder::new();

        DropletDecoder(Decoder3::new(num_decoder, neighbors_decoder, data_decoder))
    }
}

impl Decoder for DropletDecoder {
    type Output = Droplet;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        Ok(self.0.push_bytes(bytes)?)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        let (num, neighbors, data) = self.0.end()?;

        Ok(Droplet {
            num,
            neighbors,
            data,
        })
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}
