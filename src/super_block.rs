use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeEncoder, Decodable, Decoder,
    Decoder4, Encodable, Encoder, Encoder2, Encoder4, VecDecoder,
};

/// NOTE: 4_000_000 is the limit that can be decoded using default [`bitcoin_consensus_encoding::CompactSizeDecoder`]
/// Limit can be changed by using [`bitcoin_consensus_encoding::CompactSizeDecoder::new_with_limit()`]
pub const SUPERBLOCK_SIZE: usize = 4_000_000;

/// SuperBlock represents concatenated blocks (with padding)
#[derive(Debug, Clone, PartialEq)]
pub struct SuperBlock {
    /// Superblock number // TODO remove - unnecessary
    pub num: usize,
    /// Number of blocks included in this superblock
    block_count: usize,
    /// Concatenated consensus-encoded blocks
    encoded_blocks_bytes: Vec<u8>,
    /// Length of encoded bytes
    bytes_length: usize,
}

impl SuperBlock {
    pub fn new(num: usize) -> Self {
        Self {
            num,
            block_count: 0,
            encoded_blocks_bytes: Vec::with_capacity(SUPERBLOCK_SIZE),
            bytes_length: 0,
        }
    }

    pub fn add(&mut self, encodable_block: EncodableBlock) -> Result<()> {
        let block_bytes = encoding::encode_to_vec(&encodable_block);

        // block bytes prefixed with compact-size length
        self.encoded_blocks_bytes.extend_from_slice(&block_bytes);
        self.bytes_length = self.encoded_blocks_bytes.len();

        self.block_count += 1;

        Ok(())
    }

    /// Byte length of currently encoded blocks in superblock
    pub fn size(&self) -> usize {
        // Add a reserve to encode compact-size length of the whole bytes vector.
        // 5 bytes is maximum realistic length of compact-size encoded number,
        // 5 bytes compact size can encode 65536 - 4294967295
        // We need to encode number of blocks in the vector and the total number of bytes.
        self.bytes_length + 2 * 5
    }

    /// Consensus-encode concatenated block bytes
    pub fn into_encoded_bytes(mut self) -> Vec<u8> {
        // encode as a vector of bytes with items count at the beginning
        let mut encoded_blocks_vec_with_count =
            Vec::with_capacity(self.encoded_blocks_bytes.len() + 10);

        let count_encoder = CompactSizeEncoder::new(self.block_count);
        let encoded_block_count = count_encoder.current_chunk();
        encoded_blocks_vec_with_count.extend_from_slice(encoded_block_count);

        encoded_blocks_vec_with_count.append(&mut self.encoded_blocks_bytes);

        encoded_blocks_vec_with_count
    }

    pub fn block_count(&self) -> usize {
        self.block_count
    }

    pub fn encode_to_bytes(self) -> Vec<u8> {
        encoding::encode_to_vec(&self)
    }

    pub fn decode_from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        encoding::decode_from_slice(bytes)
    }
}

impl std::ops::BitXorAssign for SuperBlock {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.num ^= rhs.num;
        self.block_count ^= rhs.block_count;
        self.bytes_length ^= rhs.bytes_length;

        let max_len = std::cmp::max(
            self.encoded_blocks_bytes.len(),
            rhs.encoded_blocks_bytes.len(),
        );

        let padding = max_len - self.encoded_blocks_bytes.len();
        self.encoded_blocks_bytes.append(&mut vec![0u8; padding]);

        for i in 0..rhs.encoded_blocks_bytes.len() {
            self.encoded_blocks_bytes[i] ^= rhs.encoded_blocks_bytes[i]
        }
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`SuperBlock`] type.
    pub struct SuperBlockEncoder<'e>(Encoder4<CompactSizeEncoder, CompactSizeEncoder, CompactSizeEncoder, Encoder2<CompactSizeEncoder, BytesEncoder<'e>>>);
}

impl Encodable for SuperBlock {
    type Encoder<'e>
        = SuperBlockEncoder<'e>
    where
        Self: 'e;

    fn encoder(&self) -> Self::Encoder<'_> {
        let num = CompactSizeEncoder::new(self.num);
        let block_count = CompactSizeEncoder::new(self.block_count);
        let bytes_length = CompactSizeEncoder::new(self.bytes_length);

        let encoded_blocks_bytes = Encoder2::new(
            CompactSizeEncoder::new(self.encoded_blocks_bytes.len()),
            BytesEncoder::without_length_prefix(self.encoded_blocks_bytes.as_ref()),
        );

        SuperBlockEncoder::new(Encoder4::new(
            num,
            block_count,
            bytes_length,
            encoded_blocks_bytes,
        ))
    }
}

/// The decoder for the [`SuperBlock`] type.
pub struct SuperBlockDecoder(
    Decoder4<CompactSizeDecoder, CompactSizeDecoder, CompactSizeDecoder, ByteVecDecoder>,
);

impl Decodable for SuperBlock {
    type Decoder = SuperBlockDecoder;

    fn decoder() -> Self::Decoder {
        let num = CompactSizeDecoder::new();
        let block_count = CompactSizeDecoder::new();
        //      let bytes_length = CompactSizeDecoder::new_with_limit(2 * SUPERBLOCK_SIZE);
        let bytes_length = CompactSizeDecoder::new_with_limit(SUPERBLOCK_SIZE + 15_000); // TODO
        let encoded_blocks_bytes = ByteVecDecoder::new();

        SuperBlockDecoder(Decoder4::new(
            num,
            block_count,
            bytes_length,
            encoded_blocks_bytes,
        ))
    }
}

impl Decoder for SuperBlockDecoder {
    type Output = SuperBlock;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        Ok(self.0.push_bytes(bytes)?)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        let (num, block_count, bytes_length, encoded_blocks_bytes) = self.0.end()?;

        Ok(SuperBlock {
            num,
            block_count,
            bytes_length,
            encoded_blocks_bytes,
        })
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
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

/// The decoder for the [`EncodedBlocks`] type.
pub struct EncodedBlocksDecoder(VecDecoder<EncodableBlock>);

impl Decodable for EncodedBlocks {
    type Decoder = EncodedBlocksDecoder;

    fn decoder() -> Self::Decoder {
        EncodedBlocksDecoder(VecDecoder::new())
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

/// Container for block serialized to Bitcoin wire format
#[derive(Debug)]
pub struct EncodableBlock {
    data: Vec<u8>,
}

impl EncodableBlock {
    pub fn new(block: Block) -> Self {
        let data = block.consensus_encode().expect("should be valid block");

        Self { data }
    }

    /// Returns size of consensus-encoded block data
    pub fn size(&self) -> usize {
        self.data.len()
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
        EncodableBlockDecoder(ByteVecDecoder::new())
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

    #[test]
    fn test_superblock_xor() {
        let mut shorter = SuperBlock {
            num: 41,
            block_count: 24259,
            encoded_blocks_bytes: Vec::from(b"Some data bytes"),
            bytes_length: 16,
        };
        let longer = SuperBlock {
            num: 1100,
            block_count: 3,
            encoded_blocks_bytes: Vec::from(b"Some much longer data bytes"),
            bytes_length: 27,
        };
        let mut longest = SuperBlock {
            num: 1100,
            block_count: 58962,
            encoded_blocks_bytes: Vec::from(b"Message with the longest data bytes"),
            bytes_length: 4_000_000,
        };

        assert!(shorter.encoded_blocks_bytes.len() < longer.encoded_blocks_bytes.len());
        assert!(longer.encoded_blocks_bytes.len() < longest.encoded_blocks_bytes.len());

        let mut xored = SuperBlock::new(0);

        // empty lhs becomes rhs
        xored ^= shorter.clone();
        assert_eq!(xored, shorter);

        // then becomes obfuscated by rhs
        xored ^= longer.clone();
        assert_ne!(xored, longer);

        // then becomes clear again by previous rhs
        xored ^= shorter.clone();
        assert_eq!(xored, longer);

        // longer lhs XORed by shorter rhs can be decoded
        let original = longest.clone();
        assert_ne!(original, shorter);
        assert_ne!(original, longer);

        // encoding
        longest ^= shorter.clone();
        assert_ne!(longest, original);

        longest ^= longer.clone();

        // it can be correctly encoded and decoded back
        let encoded = encoding::encode_to_vec(&longest);
        let decoded: SuperBlock = encoding::decode_from_slice(&encoded).unwrap();
        assert_eq!(decoded, longest);

        // decoding
        longest ^= shorter.clone();
        longest ^= longer.clone();

        assert_eq!(longest, original);

        /////////////////////////

        // shorter lhs XORed by longer rhs can be easily decoded,
        // because it will be zero-padded to the length of the rhs
        let original_shorter = shorter.clone();
        assert_ne!(original_shorter, longer);

        shorter ^= longer.clone();
        assert_ne!(shorter, original_shorter);

        shorter ^= longer.clone();
        assert_ne!(shorter, original_shorter);
        assert!(shorter.encoded_blocks_bytes.len() > original_shorter.encoded_blocks_bytes.len());
        assert_eq!(
            shorter.encoded_blocks_bytes[0..original_shorter.encoded_blocks_bytes.len()],
            original_shorter.encoded_blocks_bytes
        );
    }
}
