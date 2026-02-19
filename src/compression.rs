#![allow(dead_code)]
use rand::distr::Distribution as _;
use rand::{Rng, RngExt};

use crate::compression::distribution::RobustSoliton;

mod distribution;

/// Fountain code encoder using robust soliton distribution
pub struct FountainEncoder {
    k: usize,                     // Number of source symbols (ie. blocks in our case)
    source_symbols: Vec<Vec<u8>>, // Source data
    degree_distribution: RobustSoliton,
}

impl FountainEncoder {
    pub fn new(source_symbols: Vec<Vec<u8>>) -> Self {
        let k = source_symbols.len();

        let c = 0.2; // Constant factor (typically 0.03 to 0.1)
        let delta = 0.05; // Failure probability (typically 0.01 to 0.5)

        Self {
            k,
            source_symbols,
            degree_distribution: RobustSoliton::new(k, c, delta),
        }
    }

    /// Generate an encoded symbol (repair symbol) by XORing random source symbols
    pub fn generate_symbol<R: Rng>(&self, rng: &mut R) -> (Vec<usize>, Vec<u8>) {
        // To generate a droplet in an epoch, a node first randomly
        // samples a degree d ∈ {1, 2, . . . , k} using the degree distribution
        let degree = self.degree_distribution.sample(rng);

        // Randomly select `degree` source symbols
        let mut indices = Vec::new();
        let mut selected = vec![false; self.k];

        while indices.len() < degree {
            let idx = rng.random_range(0..self.k);
            if !selected[idx] {
                selected[idx] = true;
                indices.push(idx);
            }
        }

        // XOR the selected symbols
        let symbol_len = self.source_symbols[0].len();
        let mut encoded = vec![0u8; symbol_len];

        for &idx in &indices {
            for (i, byte) in self.source_symbols[idx].iter().enumerate() {
                encoded[i] ^= byte;
            }
        }

        (indices, encoded)
    }
}

#[cfg(test)]
mod encoder_tests {
    use super::*;

    #[test]
    fn test_encoder_generation() {
        let source = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]];

        let encoder = FountainEncoder::new(source);
        let mut rng = rand::rng();

        for _ in 0..10 {
            let (indices, symbol) = encoder.generate_symbol(&mut rng);
            assert!(!indices.is_empty());
            assert_eq!(symbol.len(), 4);
        }
    }
}
