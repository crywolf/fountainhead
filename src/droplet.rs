use anyhow::anyhow;
use bitcoin_consensus_encoding as encoding;

use encoding::{
    CompactSizeDecoder, CompactSizeDecoderError, CompactSizeEncoder, Decodable, Decoder, Decoder3,
    Encodable, Encoder2, Encoder3, SliceEncoder, VecDecoder,
};

use crate::super_block::{SuperBlock, SuperBlockDecoder, SuperBlockEncoder};

#[derive(Debug, Clone, PartialEq)]
pub struct Droplet {
    /// Droplet number
    pub num: usize,
    /// Indices (numbers) of superblocks encoded in this droplet
    neighbors: Vec<Neighbor>,
    /// Super block encoded (XORed) into this droplet
    xored_superblock: SuperBlock,
}

impl Droplet {
    /// Creates new droplet containing given superblock (possibly XORed) with some `neighbors`
    pub fn new(num: usize, neighbors: Vec<Neighbor>, superblock: SuperBlock) -> Self {
        Self {
            num,
            neighbors,
            xored_superblock: superblock,
        }
    }

    /// Returns `neighbors` XORed into the droplet
    pub fn neighbors(&self) -> Vec<Neighbor> {
        self.neighbors.clone()
    }

    /// Returns reference to data encoded (XORed) in the droplet
    pub fn xored_superblock(&self) -> &SuperBlock {
        &self.xored_superblock
    }

    /// Consumes self and returns data encoded (XORed) in the droplet
    pub fn into_xored_superblock(self) -> SuperBlock {
        self.xored_superblock
    }

    /// Returns the size of the encoded data
    pub fn data_size(&self) -> usize {
        self.xored_superblock.size()
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

        DropletEncoder::new(Encoder3::new(
            num,
            neighbors,
            self.xored_superblock.encoder(),
        ))
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
            .map_err(|e| anyhow!("DropletDecoder: push bytes: {:?}", e))
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        let (num, neighbors, superblock) = self
            .0
            .end()
            .map_err(|e| anyhow!("DropletDecoder: end: {:?}", e))?;

        Ok(Droplet {
            num,
            neighbors,
            xored_superblock: superblock,
        })
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}

#[cfg(test)]
mod tests {
    use crate::super_block::RawBlock;

    use super::*;

    #[test]
    fn test_droplet() {
        let block1 = Vec::from(b"Some block data");
        let block2 = Vec::from(b"Another block");
        let block3 = Vec::from(b"Last and longest block");
        let b1 = RawBlock::new(&block1);
        let b2 = RawBlock::new(&block2);
        let b3 = RawBlock::new(&block3);

        let mut sb = SuperBlock::new(50);
        sb.add(b1).unwrap();
        sb.add(b2).unwrap();
        sb.add(b3).unwrap();

        let neighbors = vec![Neighbor(3), Neighbor(19854)];
        let droplet = Droplet::new(231, neighbors, sb);

        let encoded_bytes = encoding::encode_to_vec(&droplet);
        let decoded: Droplet = encoding::decode_from_slice(&encoded_bytes).unwrap();

        assert_eq!(&decoded, &droplet);

        let expected_blocks = droplet.into_xored_superblock().into_blocks().unwrap();
        let decoded_blocks = decoded.into_xored_superblock().into_blocks().unwrap();

        assert_eq!(decoded_blocks, expected_blocks);
    }
}
