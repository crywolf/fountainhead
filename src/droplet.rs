use anyhow::{Context, anyhow};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;

use encoding::{
    CompactSizeDecoder, CompactSizeDecoderError, CompactSizeEncoder, Decodable, Decoder, Decoder3,
    Encodable, Encoder2, Encoder3, SliceEncoder, VecDecoder,
};

use crate::super_block::{EncodedBlocks, SuperBlock, SuperBlockDecoder, SuperBlockEncoder};

#[derive(Clone)]
pub struct Droplet {
    /// Droplet number
    pub num: usize,
    /// Indices (numbers) of superblocks encoded in this droplet
    neighbors: Vec<Neighbor>,
    /// Super block
    superblock: SuperBlock,
}

impl Droplet {
    pub fn new(num: usize, neighbors: Vec<Neighbor>, superblock: SuperBlock) -> Self {
        Self {
            num,
            neighbors,
            superblock,
        }
    }

    pub fn neighbors(&self) -> Vec<Neighbor> {
        self.neighbors.clone()
    }

    pub fn superblock(&self) -> &SuperBlock {
        &self.superblock
    }

    pub fn into_superblock(self) -> SuperBlock {
        self.superblock
    }

    pub fn data_size(&self) -> usize {
        self.superblock.size()
    }

    /// Consumes the droplet and returns a vector of blocks
    pub fn into_blocks(self) -> anyhow::Result<Vec<Block>> {
        let mut blocks = Vec::new();

        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(self.superblock.into_encoded_bytes().as_ref())
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
}

/// Superblock number
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Neighbor(usize);

impl std::fmt::Debug for Neighbor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Neighbor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Neighbor {
    pub fn new(num: usize) -> Self {
        Self(num)
    }
}

impl From<usize> for Neighbor {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<Neighbor> for usize {
    fn from(value: Neighbor) -> Self {
        value.0
    }
}

impl From<&Neighbor> for usize {
    fn from(value: &Neighbor) -> Self {
        value.0
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
    pub struct DropletEncoder<'e>(Encoder3<CompactSizeEncoder, Encoder2<CompactSizeEncoder, SliceEncoder<'e, Neighbor>>,  SuperBlockEncoder<'e>>);
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

        DropletEncoder::new(Encoder3::new(num, neighbors, self.superblock.encoder()))
    }
}

/// The decoder for the [`Droplet`] type.
pub struct DropletDecoder(Decoder3<CompactSizeDecoder, VecDecoder<Neighbor>, SuperBlockDecoder>);

impl Decodable for Droplet {
    type Decoder = DropletDecoder;

    fn decoder() -> Self::Decoder {
        let num_decoder = CompactSizeDecoder::new();
        let neighbors_decoder = VecDecoder::new();
        let superblock_decoder = SuperBlock::decoder();

        DropletDecoder(Decoder3::new(
            num_decoder,
            neighbors_decoder,
            superblock_decoder,
        ))
    }
}

impl Decoder for DropletDecoder {
    type Output = Droplet;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        self.0
            .push_bytes(bytes)
            .map_err(|e| anyhow!("DropletDecoder: push bytes: {}", e))
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        let (num, neighbors, superblock) = self
            .0
            .end()
            .map_err(|e| anyhow!("DropletDecoder: end: {}", e))?;

        Ok(Droplet {
            num,
            neighbors,
            superblock,
        })
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}
