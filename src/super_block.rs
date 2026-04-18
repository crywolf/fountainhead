use std::io::{Cursor, Read};

use anyhow::{Context, Result};
use bitcoin_consensus_encoding as encoding;
use bitcoinkernel::Block;
use bitcoinkernel::core::{BlockHashExt, BlockHeaderExt};
use encoding::CompactSizeDecoderError;

use encoding::{
    ByteVecDecoder, BytesEncoder, CompactSizeDecoder, CompactSizeEncoder, Decodable, Decoder,
    Decoder2, Decoder3, Encodable, Encoder2, Encoder4, SliceEncoder, VecDecoder,
};

pub const SUPERBLOCK_MAX_SIZE: usize = 6_000_000; // 4_000_000 * 1.5

/// SuperBlock represents concatenated blocks (with padding)
#[derive(Debug, Clone, PartialEq)]
pub struct SuperBlock {
    /// Superblock number
    num: usize,
    /// Number of blocks included in this superblock
    block_count: usize,
    /// Length of encoded bytes
    bytes_length: usize,
    /// Sizes of individual blocks
    block_sizes: Vec<Size>,
    /// Concatenated consensus-encoded blocks (raw block bytes)
    raw_bytes: Vec<u8>,
}

impl SuperBlock {
    /// Creates new superblock with the given number
    pub fn new(num: usize) -> Self {
        Self {
            num,
            block_count: 0,
            bytes_length: 0,
            block_sizes: Vec::new(),
            raw_bytes: Vec::with_capacity(SUPERBLOCK_MAX_SIZE),
        }
    }

    /// Ads new block to superblock
    pub fn add(&mut self, raw_block: RawBlock) -> Result<()> {
        let block_bytes = raw_block.raw_bytes;
        let block_size = block_bytes.len();

        if self.available_space() < block_size {
            anyhow::bail!(
                "Not enough space in superblock; needed: {}. available: {}",
                block_size,
                self.available_space(),
            );
        }

        self.block_sizes.push(Size(block_size));

        // Concatenated block bytes
        self.raw_bytes.extend_from_slice(&block_bytes);
        self.bytes_length = self.raw_bytes.len();

        self.block_count += 1;

        Ok(())
    }

    /// Byte length of currently included blocks in the superblock
    pub fn size(&self) -> usize {
        self.raw_bytes.len()
    }

    /// Returns amount of bytes that can be added to the superblock (size of the superblock is limited by [`SUPERBLOCK_MAX_SIZE`])
    pub fn available_space(&self) -> usize {
        SUPERBLOCK_MAX_SIZE - self.raw_bytes.len()
    }

    /// Returns block hashes of contained blocks
    pub fn block_hashes(&mut self) -> anyhow::Result<Vec<BlockHashesPair>> {
        self.crop_padding();

        let mut hashes = Vec::with_capacity(self.block_count());

        let mut bytes = Cursor::new(&self.raw_bytes);

        for size in self.block_sizes.iter() {
            let mut raw_bytes = vec![0u8; size.into()];
            bytes
                .read_exact(&mut raw_bytes)
                .context("read raw block bytes")?;

            let block = RawBlock::new(&raw_bytes).to_block()?;
            let block_header = block.header();

            let hash_pair = BlockHashesPair {
                current: block_header.hash().to_bytes(),
                previous: block_header.prev_hash().to_bytes(),
            };

            hashes.push(hash_pair);
        }

        Ok(hashes)
    }

    /// Consumes self and returns a vector of blocks
    pub fn into_blocks(mut self) -> anyhow::Result<Vec<RawBlock>> {
        self.crop_padding();

        let mut blocks = Vec::with_capacity(self.block_count());

        let mut bytes = Cursor::new(&self.raw_bytes);

        for size in self.block_sizes {
            let mut raw_bytes = vec![0u8; size.into()];
            bytes
                .read_exact(&mut raw_bytes)
                .context("read raw block bytes")?;

            let block = RawBlock::new(&raw_bytes);
            blocks.push(block);
        }

        Ok(blocks)
    }

    /// Removes extra zero-padding added to during XOR operation (vectors must have the same length)
    pub fn crop_padding(&mut self) {
        self.raw_bytes.truncate(self.bytes_length);
        self.block_sizes.truncate(self.block_count());
    }

    /// Returns number of blocks
    pub fn block_count(&self) -> usize {
        self.block_count
    }

    /// Encodes the superblock into a vector
    pub fn encode_to_bytes(self) -> Vec<u8> {
        encoding::encode_to_vec(&self)
    }

    /// Decodes the superblock from a byte slice
    pub fn decode_from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        encoding::decode_from_slice(bytes)
            .map_err(|e| anyhow::anyhow!("Failed to decode superblock from bytes: {e}"))
    }
}

impl std::ops::BitXorAssign for SuperBlock {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.num ^= rhs.num;
        self.block_count ^= rhs.block_count;
        self.bytes_length ^= rhs.bytes_length;

        let max_sizes_len = std::cmp::max(self.block_sizes.len(), rhs.block_sizes.len());
        let padding = max_sizes_len - self.block_sizes.len();
        self.block_sizes.append(&mut vec![Size(0); padding]);

        for i in 0..rhs.block_sizes.len() {
            self.block_sizes[i] ^= rhs.block_sizes[i]
        }

        let max_bytes_len = std::cmp::max(self.raw_bytes.len(), rhs.raw_bytes.len());
        let padding = max_bytes_len - self.raw_bytes.len();
        self.raw_bytes.append(&mut vec![0; padding]);

        for i in 0..rhs.raw_bytes.len() {
            self.raw_bytes[i] ^= rhs.raw_bytes[i]
        }
    }
}

impl<'a> std::ops::BitXorAssign<&'a SuperBlock> for SuperBlock {
    fn bitxor_assign(&mut self, rhs: &'a Self) {
        self.num ^= rhs.num;
        self.block_count ^= rhs.block_count;
        self.bytes_length ^= rhs.bytes_length;

        let max_sizes_len = std::cmp::max(self.block_sizes.len(), rhs.block_sizes.len());
        let padding = max_sizes_len - self.block_sizes.len();
        self.block_sizes.append(&mut vec![Size(0); padding]);

        for i in 0..rhs.block_sizes.len() {
            self.block_sizes[i] ^= rhs.block_sizes[i]
        }

        let max_bytes_len = std::cmp::max(self.raw_bytes.len(), rhs.raw_bytes.len());
        let padding = max_bytes_len - self.raw_bytes.len();
        self.raw_bytes.append(&mut vec![0; padding]);

        for i in 0..rhs.raw_bytes.len() {
            self.raw_bytes[i] ^= rhs.raw_bytes[i]
        }
    }
}

/// Container for block serialized to Bitcoin wire data format
#[derive(Debug, PartialEq)]
pub struct RawBlock {
    raw_bytes: Vec<u8>,
}

impl RawBlock {
    pub fn new(raw_bytes: &[u8]) -> Self {
        Self {
            raw_bytes: raw_bytes.to_vec(),
        }
    }

    /// Returns size of block data
    pub fn size(&self) -> usize {
        self.raw_bytes.len()
    }

    /// Returns [`bitcoinkernel::Block`]
    pub fn to_block(&self) -> Result<Block> {
        Block::new(&self.raw_bytes).context("new block from encodable block")
    }
}

/// Pair of block hash and previous block hash
#[derive(Clone, Copy)]
pub struct BlockHashesPair {
    current: [u8; 32],
    previous: [u8; 32],
}

impl BlockHashesPair {
    pub fn current(self) -> [u8; 32] {
        self.current
    }

    pub fn previous(self) -> [u8; 32] {
        self.previous
    }
}

/// Size of individual block
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size(usize);

impl From<usize> for Size {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<Size> for usize {
    fn from(value: Size) -> Self {
        value.0
    }
}

impl From<&Size> for usize {
    fn from(value: &Size) -> Self {
        value.0
    }
}

impl std::ops::BitXorAssign for Size {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0
    }
}

encoding::encoder_newtype! {
    /// The encoder for the [`Size`] type.
    pub struct SizeEncoder<'e>(CompactSizeEncoder);
}

impl Encodable for Size {
    type Encoder<'e> = SizeEncoder<'e>;

    fn encoder(&self) -> Self::Encoder<'_> {
        SizeEncoder::new(CompactSizeEncoder::new(self.0))
    }
}

impl Decodable for Size {
    type Decoder = SizeDecoder;

    fn decoder() -> Self::Decoder {
        SizeDecoder(CompactSizeDecoder::new_with_limit(usize::MAX))
    }
}

/// The decoder for the [`Size`] type.
pub struct SizeDecoder(CompactSizeDecoder);

impl Decoder for SizeDecoder {
    type Output = Size;
    type Error = CompactSizeDecoderError;

    fn push_bytes(&mut self, bytes: &mut &[u8]) -> std::result::Result<bool, Self::Error> {
        self.0.push_bytes(bytes)
    }

    fn end(self) -> std::result::Result<Self::Output, Self::Error> {
        Ok(Size(self.0.end()?))
    }

    fn read_limit(&self) -> usize {
        self.0.read_limit()
    }
}

type RawBytesEncoder<'e> = Encoder2<CompactSizeEncoder, BytesEncoder<'e>>;
type SizesEncoder<'e> = Encoder2<CompactSizeEncoder, SliceEncoder<'e, Size>>;

encoding::encoder_newtype! {
    /// The encoder for the [`SuperBlock`] type.
    pub struct SuperBlockEncoder<'e>(Encoder2<Encoder4<CompactSizeEncoder, CompactSizeEncoder, CompactSizeEncoder, SizesEncoder<'e>>, RawBytesEncoder<'e>>);
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

        let block_sizes = Encoder2::new(
            CompactSizeEncoder::new(self.block_sizes.len()),
            SliceEncoder::without_length_prefix(self.block_sizes.as_ref()),
        );

        let enc1 = Encoder4::new(num, block_count, bytes_length, block_sizes);

        let raw_bytes = Encoder2::new(
            CompactSizeEncoder::new(self.raw_bytes.len()),
            BytesEncoder::without_length_prefix(self.raw_bytes.as_ref()),
        );

        SuperBlockEncoder::new(Encoder2::new(enc1, raw_bytes))
    }
}

/// The decoder for the [`SuperBlock`] type.
pub struct SuperBlockDecoder(
    Decoder2<
        Decoder3<CompactSizeDecoder, CompactSizeDecoder, CompactSizeDecoder>,
        Decoder2<VecDecoder<Size>, ByteVecDecoder>,
    >,
);

impl Decodable for SuperBlock {
    type Decoder = SuperBlockDecoder;

    fn decoder() -> Self::Decoder {
        let num = CompactSizeDecoder::new_with_limit(usize::MAX);
        let block_count = CompactSizeDecoder::new_with_limit(usize::MAX);
        let bytes_length = CompactSizeDecoder::new_with_limit(usize::MAX);

        let block_sizes = VecDecoder::new();
        let raw_bytes = ByteVecDecoder::new_with_limit(SUPERBLOCK_MAX_SIZE);

        SuperBlockDecoder(Decoder2::new(
            Decoder3::new(num, block_count, bytes_length),
            Decoder2::new(block_sizes, raw_bytes),
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
        let ((num, block_count, bytes_length), (block_sizes, raw_bytes)) = self.0.end()?;

        Ok(SuperBlock {
            num,
            block_count,
            bytes_length,
            block_sizes,
            raw_bytes,
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
    fn test_superblock() {
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

        let encoded_bytes = encoding::encode_to_vec(&sb);
        let decoded: SuperBlock = encoding::decode_from_slice(&encoded_bytes).unwrap();

        assert_eq!(&decoded, &sb);

        let expected_blocks = sb.into_blocks().unwrap();
        let decoded_blocks = decoded.into_blocks().unwrap();

        assert_eq!(decoded_blocks, expected_blocks);
    }

    #[test]
    fn test_superblock_basic_xor() {
        let mut shorter = SuperBlock {
            num: 41,
            block_count: 24259,
            raw_bytes: Vec::from(b"Some data bytes"),
            bytes_length: 15,
            block_sizes: vec![Size(15)],
        };
        let longer = SuperBlock {
            num: 1100,
            block_count: 3,
            raw_bytes: Vec::from(b"Some much longer data bytes"),
            bytes_length: 27,
            block_sizes: vec![Size(27)],
        };
        let mut longest = SuperBlock {
            num: 1100,
            block_count: 58962,
            raw_bytes: Vec::from(b"Message with the longest data bytes"),
            bytes_length: 35,
            block_sizes: vec![Size(35)],
        };

        assert!(shorter.raw_bytes.len() < longer.raw_bytes.len());
        assert!(longer.raw_bytes.len() < longest.raw_bytes.len());

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
        assert!(shorter.raw_bytes.len() > original_shorter.raw_bytes.len());
        assert_eq!(
            shorter.raw_bytes[0..original_shorter.raw_bytes.len()],
            original_shorter.raw_bytes
        );
        shorter.crop_padding();
        assert_eq!(shorter, original_shorter);
    }

    #[test]
    fn test_superblock_xor() {
        let mut sb1 = SuperBlock::new(123);
        let block_data1 = vec![222_u8; 423];
        while sb1.available_space() > block_data1.len() {
            let block = RawBlock {
                raw_bytes: block_data1.clone(),
            };
            sb1.add(block).unwrap();
        }

        let consensus_bytes = sb1.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb1 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        assert_eq!(sb1.block_count(), sb1.block_sizes.len());

        ////////////////////////////////

        let mut sb2 = SuperBlock::new(223);
        let block_data2 = vec![123_u8; 350];
        while sb2.available_space() > block_data2.len() {
            let block = RawBlock {
                raw_bytes: block_data2.clone(),
            };
            sb2.add(block).unwrap();
        }

        let consensus_bytes = sb2.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb2 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        ////////////////////////////////

        let mut sb3 = SuperBlock::new(323);
        let block_data3 = vec![65_u8; 136588];
        while sb3.available_space() > block_data3.len() {
            let block = RawBlock {
                raw_bytes: block_data3.clone(),
            };
            sb3.add(block).unwrap();
        }

        let consensus_bytes = sb3.clone().encode_to_bytes();
        let back_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb3 == back_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

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

        let mut decoded = xored;
        decoded ^= sb3;
        decoded ^= sb2;

        assert!(decoded.block_sizes.len() > sb1.block_sizes.len());

        assert!(decoded.size() == sb1.size());

        // we need to crop padding, to have the same block_sizes vector!
        decoded.crop_padding();

        assert!(decoded.block_sizes == sb1.block_sizes);
        assert!(decoded.block_count() == sb1.block_count());

        assert!(decoded == sb1, "incorrectly decoded superblock");

        let block_count = decoded.block_count();
        let decoded_blocks = decoded.into_blocks().unwrap();
        assert_eq!(decoded_blocks.len(), block_count);
        assert_eq!(decoded_blocks.first().unwrap().raw_bytes, block_data1);
    }

    #[test]
    fn test_superblock_max_size() {
        let mut sb = SuperBlock::new(44444);
        let block_data = vec![213_u8; 1_000_000];

        while sb.available_space() >= block_data.len() {
            let block = RawBlock {
                raw_bytes: block_data.clone(),
            };
            sb.add(block).unwrap();
        }

        assert_eq!(sb.block_count(), 6);
        assert_eq!(sb.size(), SUPERBLOCK_MAX_SIZE);

        // cannot add more
        let r = sb.add(RawBlock {
            raw_bytes: "whatever".bytes().collect(),
        });
        assert!(r.is_err());

        let consensus_bytes = sb.clone().encode_to_bytes();
        let decoded_from_bytes = SuperBlock::decode_from_bytes(&consensus_bytes).unwrap();

        assert!(
            sb == decoded_from_bytes,
            "incorrectly decoded superblock from consensus bytes"
        );

        assert_eq!(
            decoded_from_bytes.into_blocks().unwrap()[0].size(),
            block_data.len()
        );
    }
}
