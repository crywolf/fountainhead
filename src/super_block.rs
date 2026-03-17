use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeEncoder, Decodable, Decoder,
    Decoder3, Encodable, Encoder, Encoder2, Encoder3, VecDecoder,
};

/// NOTE: 4_000_000 is the limit that can be decoded using [`bitcoin_consensus_encoding::ByteVecDecoder`]
pub const SUPERBLOCK_MAX_SIZE: usize = 4_000_000;

/// SuperBlock represents concatenated blocks (with padding)
#[derive(Debug, Clone, PartialEq)]
pub struct SuperBlock {
    /// Superblock number // TODO remove - unnecessary
    pub num: usize,
    /// Number of blocks included in this superblock
    block_count: usize,
    /// Concatenated consensus-encoded blocks
    pub encoded_blocks_bytes: Vec<u8>, // TODO pub
}

impl SuperBlock {
    pub fn new(num: usize) -> Self {
        Self {
            num,
            block_count: 0,
            encoded_blocks_bytes: Vec::with_capacity(SUPERBLOCK_MAX_SIZE),
        }
    }

    pub fn add(&mut self, encodable_block: EncodableBlock) -> Result<()> {
        let block_bytes = encoding::encode_to_vec(&encodable_block);

        // Concatenated block bytes, each block prefixed with compact-size length
        self.encoded_blocks_bytes.extend_from_slice(&block_bytes);

        self.block_count += 1;

        Ok(())
    }

    /// Byte length of currently encoded blocks in superblock
    pub fn size(&self) -> usize {
        self.encoded_blocks_bytes.len()
    }

    /// Returns amount of bytes that can be added to the superblock (size of the superblock is limited by [`SUPERBLOCK_SIZE`])
    pub fn available_space(&self) -> usize {
        SUPERBLOCK_MAX_SIZE - self.encoded_blocks_bytes.len()
    }

    /// Consensus-encode concatenated block bytes
    pub fn into_consensus_bytes(mut self) -> Vec<u8> {
        // encode as a vector of bytes with items count at the beginning
        let mut encoded_blocks_vec_with_count =
            Vec::with_capacity(self.encoded_blocks_bytes.len() + 10);

        let count_encoder = CompactSizeEncoder::new(self.block_count);
        let encoded_block_count = count_encoder.current_chunk();
        encoded_blocks_vec_with_count.extend_from_slice(encoded_block_count);

        encoded_blocks_vec_with_count.append(&mut self.encoded_blocks_bytes);

        encoded_blocks_vec_with_count
    }

    /// Consumes self and returns a vector of blocks
    pub fn into_blocks(self) -> anyhow::Result<Vec<Block>> {
        let mut blocks = Vec::new();
        dbg!(self.num, self.size());
        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(self.into_consensus_bytes().as_ref())
                .context("decode encoded blocks from superblock's consensus data")?;

        let encoded_blocks = encoded_blocks.into_vec();

        for enc_block in encoded_blocks {
            blocks.push(
                enc_block
                    .to_block()
                    .context("superblock: get block from encoded block")?,
            )
        }

        Ok(blocks)
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

impl<'a> std::ops::BitXorAssign<&'a SuperBlock> for SuperBlock {
    fn bitxor_assign(&mut self, rhs: &'a Self) {
        self.num ^= rhs.num;
        self.block_count ^= rhs.block_count;

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
    pub struct SuperBlockEncoder<'e>(Encoder3<CompactSizeEncoder, CompactSizeEncoder, Encoder2<CompactSizeEncoder, BytesEncoder<'e>>>);
}

impl Encodable for SuperBlock {
    type Encoder<'e>
        = SuperBlockEncoder<'e>
    where
        Self: 'e;

    fn encoder(&self) -> Self::Encoder<'_> {
        let num = CompactSizeEncoder::new(self.num);
        let block_count = CompactSizeEncoder::new(self.block_count);
        let encoded_blocks_bytes = Encoder2::new(
            CompactSizeEncoder::new(self.encoded_blocks_bytes.len()),
            BytesEncoder::without_length_prefix(self.encoded_blocks_bytes.as_ref()),
        );

        SuperBlockEncoder::new(Encoder3::new(num, block_count, encoded_blocks_bytes))
    }
}

/// The decoder for the [`SuperBlock`] type.
pub struct SuperBlockDecoder(Decoder3<CompactSizeDecoder, CompactSizeDecoder, ByteVecDecoder>);

impl Decodable for SuperBlock {
    type Decoder = SuperBlockDecoder;

    fn decoder() -> Self::Decoder {
        let num = CompactSizeDecoder::new();
        let block_count = CompactSizeDecoder::new();
        let encoded_blocks_bytes = ByteVecDecoder::new();

        SuperBlockDecoder(Decoder3::new(num, block_count, encoded_blocks_bytes))
    }
}

impl Decoder for SuperBlockDecoder {
    type Output = SuperBlock;
    type Error = anyhow::Error;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        Ok(self.0.push_bytes(bytes)?)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        let (num, block_count, encoded_blocks_bytes) = self.0.end()?;

        Ok(SuperBlock {
            num,
            block_count,
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
    #[allow(dead_code)]
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
#[derive(Debug, PartialEq)]
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
    fn test_superblock_basic_xor() {
        let mut shorter = SuperBlock {
            num: 41,
            block_count: 24259,
            encoded_blocks_bytes: Vec::from(b"Some data bytes"),
        };
        let longer = SuperBlock {
            num: 1100,
            block_count: 3,
            encoded_blocks_bytes: Vec::from(b"Some much longer data bytes"),
        };
        let mut longest = SuperBlock {
            num: 1100,
            block_count: 58962,
            encoded_blocks_bytes: Vec::from(b"Message with the longest data bytes"),
        };

        assert!(shorter.encoded_blocks_bytes.len() < longer.encoded_blocks_bytes.len());
        assert!(longer.encoded_blocks_bytes.len() < longest.encoded_blocks_bytes.len());

        assert_eq!(longest.size(), 35);
        assert_eq!(longest.available_space(), SUPERBLOCK_MAX_SIZE - 35);

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

        // encoded superblock it can be correctly encoded and decoded back
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

    #[test]
    fn test_superblock_xor() {
        let mut sb1 = SuperBlock::new(123);
        let block_data1 = vec![123_u8; 350];
        while sb1.available_space() > block_data1.len() {
            let block = EncodableBlock {
                data: block_data1.clone(),
            };
            sb1.add(block).unwrap();
        }

        let consensus_bytes = sb1.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb1 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        let block_count = sb1.block_count();
        let encoded_blocks1: EncodedBlocks =
            encoding::decode_from_slice(sb1.clone().into_consensus_bytes().as_ref()).unwrap();
        assert_eq!(encoded_blocks1.len(), block_count);

        ////////////////////////////////

        let mut sb2 = SuperBlock::new(223);
        let block_data2 = vec![222_u8; 423];
        while sb2.available_space() > block_data2.len() {
            let block = EncodableBlock {
                data: block_data2.clone(),
            };
            sb2.add(block).unwrap();
        }

        let consensus_bytes = sb2.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb2 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        let block_count = sb2.block_count();
        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(sb2.clone().into_consensus_bytes().as_ref()).unwrap();
        assert_eq!(encoded_blocks.len(), block_count);

        ////////////////////////////////

        let mut sb3 = SuperBlock::new(323);
        let block_data3 = vec![65_u8; 136588];
        while sb3.available_space() > block_data3.len() {
            let block = EncodableBlock {
                data: block_data3.clone(),
            };
            sb3.add(block).unwrap();
        }

        let consensus_bytes = sb3.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb3 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        let block_count = sb3.block_count();
        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(sb3.clone().into_consensus_bytes().as_ref()).unwrap();
        assert_eq!(encoded_blocks.len(), block_count);

        ////////////////////////////////

        let mut xored = SuperBlock::new(0);
        xored ^= &sb1;
        xored ^= &sb2;
        xored ^= &sb3;

        let max_size = [sb1.size(), sb2.size(), sb3.size()]
            .into_iter()
            .max()
            .unwrap();
        assert_eq!(xored.size(), max_size);

        let consensus_bytes = xored.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            xored == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        xored = back_from_bytes;

        // TODO this fails correctly, decoding XORed blocks
        // dbg!(xored.clone().into_consensus_bytes().len());
        // let block_count = xored.block_count();
        // let encoded_blocks: EncodedBlocks =
        //     encoding::decode_from_slice(xored.clone().into_consensus_bytes().as_ref()).unwrap();

        //   assert_eq!(encoded_blocks.len(), block_count);

        let mut decoded = xored.clone();
        decoded ^= sb3;
        decoded ^= sb2;

        let block_count = decoded.block_count();
        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(decoded.clone().into_consensus_bytes().as_ref()).unwrap();

        let decoded_blocks = encoded_blocks.into_vec();

        assert_eq!(decoded_blocks.len(), block_count);
        assert!(
            encoded_blocks1.into_vec() == decoded_blocks,
            "incorrectly decoded blocks"
        );

        assert_eq!(decoded_blocks.first().unwrap().data, block_data1);

        assert!(decoded == sb1, "incorrectly decoded superblock")
    }

    #[test]
    fn test_superblock_max_size() {
        let mut sb = SuperBlock::new(44444);
        // Compact-size of 1_000_000 takes 5 bytes
        let block_data = vec![213_u8; 1_000_000 - 5];

        while sb.available_space() > block_data.len() {
            let block = EncodableBlock {
                data: block_data.clone(),
            };
            sb.add(block).unwrap();
        }

        assert_eq!(sb.block_count(), 4);
        assert_eq!(sb.size(), SUPERBLOCK_MAX_SIZE);

        let consensus_bytes = sb.clone().encode_to_bytes();
        let decoded_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb == decoded_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        let encoded_blocks: EncodedBlocks =
            encoding::decode_from_slice(decoded_from_bytes.into_consensus_bytes().as_ref())
                .unwrap();

        assert_eq!(encoded_blocks.len(), sb.block_count());
        assert_eq!(encoded_blocks.0[0].size(), block_data.len());
    }
}
