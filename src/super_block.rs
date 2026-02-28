use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeEncoder, Decodable, Decoder, Encodable, Encoder,
    Encoder2, VecDecoder,
};

pub const DEFAULT_SUPERBLOCK_SIZE: usize = 10 * 1024; // TODO !!!!

/// SuperBlock represents concatenated blocks (with padding)
pub struct SuperBlock {
    padded_size: usize,
    /// Number of blocks included in this superblock
    block_count: usize,
    /// Concatenated consensus-encoded blocks
    encoded_blocks_bytes: Vec<u8>,
}

impl SuperBlock {
    pub fn new(size: usize) -> Self {
        Self {
            padded_size: size,
            block_count: 0,
            encoded_blocks_bytes: Vec::with_capacity(DEFAULT_SUPERBLOCK_SIZE),
        }
    }

    pub fn add(&mut self, block: Block) -> Result<()> {
        let block_bytes = encoding::encode_to_vec(&EncodableBlock::new(block));

        // block bytes prefixed with compact-size length
        self.encoded_blocks_bytes.extend_from_slice(&block_bytes);

        self.block_count += 1;

        Ok(())
    }

    /// Byte length of currently encoded blocks in superblock
    pub fn len(&self) -> usize {
        // Add a reserve to encode compact-size length of the whole bytes vector.
        // 5 bytes is maximum realistic length of compact-size encoded number,
        // 5 bytes compact size can encode 65536 - 4294967295
        // We need to encode number of blocks in the vector and the total number of bytes.
        self.encoded_blocks_bytes.len() + 2 * 5
    }

    /// Add padding at the end of concatenated block bytes and consensus-encode them
    pub fn into_encoded_bytes(mut self) -> Vec<u8> {
        // encode as a vector of bytes with items count at the beginning
        let mut encoded_blocks_vec_with_count = Vec::with_capacity(self.padded_size);

        let count_encoder = CompactSizeEncoder::new(self.block_count);
        let encoded_block_count = count_encoder.current_chunk();
        encoded_blocks_vec_with_count.extend_from_slice(encoded_block_count);

        encoded_blocks_vec_with_count.append(&mut self.encoded_blocks_bytes);

        // adaptive zero-padding
        debug_assert!(
            self.padded_size >= encoded_blocks_vec_with_count.len(),
            "superblock size too small - padding_limit: {}, encoded_blocks_vec_with_count.len(): {}",
            self.padded_size,
            encoded_blocks_vec_with_count.len()
        );

        let padding_length = self.padded_size - encoded_blocks_vec_with_count.len();

        encoded_blocks_vec_with_count.append(&mut vec![0u8; padding_length]);

        encoded_blocks_vec_with_count
    }

    pub fn block_count(&self) -> usize {
        self.block_count
    }
}

/// Decodable collection of [`EncodableBlock`]s
pub struct EncodedBlocks(Vec<EncodableBlock>);

impl EncodedBlocks {
    #[expect(dead_code)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn into_vec(self) -> Vec<EncodableBlock> {
        self.0
    }
}

impl Decodable for EncodedBlocks {
    type Decoder = EncodedBlocksDecoder;

    fn decoder() -> Self::Decoder {
        EncodedBlocksDecoder(VecDecoder::default())
    }
}

impl Decoder for EncodedBlocksDecoder {
    type Output = EncodedBlocks;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        self.0
            .push_bytes(bytes)
            .context("EncodedBlocksDecoder: push_bytes()")
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(EncodedBlocks(
            self.0.end().context("EncodedBlocksDecoder: end()")?,
        ))
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}

/// The decoder for the [`EncodedBlocks`] type.
pub struct EncodedBlocksDecoder(VecDecoder<EncodableBlock>);

/// Container for block bytes
#[derive(Debug)]
pub struct EncodableBlock {
    data: Vec<u8>,
}

impl EncodableBlock {
    fn new(block: Block) -> Self {
        let data = block.consensus_encode().expect("should be valid block");

        Self { data }
    }

    pub fn to_block(&self) -> Result<Block> {
        Block::new(&self.data).context("new block from encodable block")
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`EncodableBlock`] type.
    pub struct BlockEncoder<'e>(Encoder2<CompactSizeEncoder, BytesEncoder<'e>>);
}

impl Encodable for EncodableBlock {
    type Encoder<'e> = BlockEncoder<'e>;

    fn encoder(&self) -> Self::Encoder<'_> {
        let block_encoder = Encoder2::new(
            CompactSizeEncoder::new(self.data.len()),
            BytesEncoder::without_length_prefix(&self.data),
        );

        BlockEncoder::new(block_encoder)
    }
}

impl Decodable for EncodableBlock {
    type Decoder = EncodableBlockDecoder;

    fn decoder() -> Self::Decoder {
        EncodableBlockDecoder(ByteVecDecoder::default())
    }
}

/// The decoder for the [`EncodableBlock`] type.
pub struct EncodableBlockDecoder(ByteVecDecoder);

impl Decoder for EncodableBlockDecoder {
    type Output = EncodableBlock;
    type Error = bitcoin_consensus_encoding::ByteVecDecoderError;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        self.0.push_bytes(bytes)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(EncodableBlock {
            data: self.0.end()?,
        })
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encodable_block() {
        let input_data = Vec::from(b"Some block data");
        let eb = EncodableBlock {
            data: input_data.clone(),
        };

        let encoded = encoding::encode_to_vec(&eb);

        let decoded: EncodableBlock = encoding::decode_from_slice(&encoded).unwrap();
        assert_eq!(decoded.data, input_data);
    }
}
