#![allow(dead_code)]

use rand::distr::Distribution as _;
use rand::{Rng, RngExt};

pub use crate::compression::distribution::RobustSoliton;

mod distribution;

/// Fountain code encoder using robust soliton distribution
pub struct FountainEncoder {
    k: usize,                     // Number of source symbols (ie. blocks in our case)
    source_symbols: Vec<Vec<u8>>, // Source data
    degree_distribution: RobustSoliton,
}

impl FountainEncoder {
    pub fn new(source_symbols: Vec<Vec<u8>>, degree_distribution: RobustSoliton) -> Self {
        let k = source_symbols.len();

        Self {
            k,
            source_symbols,
            degree_distribution,
        }
    }

    /// Generate an encoded symbol (repair symbol) by XORing random source symbols
    pub fn generate_symbol<R: Rng>(&self, rng: &mut R) -> (Vec<usize>, Vec<u8>) {
        // To generate a droplet in an epoch, a node first randomly
        // samples a degree d ∈ {1, 2, . . . , k} using the degree distribution
        let degree = self.degree_distribution.sample(rng);

        // Randomly select `degree` source symbols
        let mut neighbors = Vec::new();
        let mut selected = vec![false; self.k];

        while neighbors.len() < degree {
            let neighbor = rng.random_range(0..self.k);
            if !selected[neighbor] {
                selected[neighbor] = true;
                neighbors.push(neighbor);
            }
        }

        // XOR the selected symbols
        let symbol_len = self.source_symbols[0].len();
        let mut encoded = vec![0u8; symbol_len];

        for &neighbor in &neighbors {
            for (i, byte) in self.source_symbols[neighbor].iter().enumerate() {
                encoded[i] ^= byte;
            }
        }

        (neighbors, encoded)
    }
}

#[cfg(test)]
mod encoder_tests {
    use super::*;

    #[test]
    fn test_encoder_generation() {
        let source = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]];

        let c = 0.02; // Constant factor (typically 0.03 to 0.1)
        let delta = 0.05; // Failure probability (typically 0.01 to 0.5)

        let degree_distribution = RobustSoliton::new(source.len(), c, delta);
        let encoder = FountainEncoder::new(source, degree_distribution);
        let mut rng = rand::rng();

        for _ in 0..4 {
            let (indices, symbol) = encoder.generate_symbol(&mut rng);
            assert!(!indices.is_empty());
            assert_eq!(symbol.len(), 4);
        }
    }
}
