#![cfg(test)]
#![allow(dead_code)]

use std::collections::HashMap;

/// Represents a single encoded symbol and its neighbors
#[derive(Clone, Debug)]
struct EncodedSymbol {
    id: usize,
    data: Vec<u8>,
    neighbors: Vec<usize>, // Indices of source symbols it depends on
}

/// Belief propagation decoder for Fountain codes
pub struct TestFountainDecoder {
    k: usize,                                   // Number of source symbols
    symbol_size: usize,                         // Size of each symbol in bytes
    encoded_symbols: Vec<EncodedSymbol>,        // Received encoded symbols
    recovered_symbols: HashMap<usize, Vec<u8>>, // Recovered source symbols
    symbol_counter: usize,                      // Counter for encoded symbol IDs
}

impl TestFountainDecoder {
    /// Create a new decoder for k source symbols of given size
    pub fn new(k: usize, symbol_size: usize) -> Self {
        TestFountainDecoder {
            k,
            symbol_size,
            encoded_symbols: Vec::new(),
            recovered_symbols: HashMap::new(),
            symbol_counter: 0,
        }
    }

    /// Add a received encoded symbol with its neighbors
    pub fn add_symbol(&mut self, neighbors: Vec<usize>, data: Vec<u8>) {
        if data.len() != self.symbol_size {
            panic!(
                "Symbol size mismatch: expected {}, got {}",
                self.symbol_size,
                data.len()
            );
        }

        self.encoded_symbols.push(EncodedSymbol {
            id: self.symbol_counter,
            data,
            neighbors,
        });
        self.symbol_counter += 1;
    }

    /// Decode using belief propagation (iterative message passing)
    pub fn decode(&mut self) -> Result<HashMap<usize, Vec<u8>>, String> {
        // Iteratively apply belief propagation until no more progress
        let mut iterations = 0;
        let max_iterations = self.encoded_symbols.len() * 10;

        loop {
            iterations += 1;

            if iterations > max_iterations {
                return Err(format!(
                    "Decoding failed to converge after {} iterations. \
                     Recovered {}/{} symbols",
                    max_iterations,
                    self.recovered_symbols.len(),
                    self.k
                ));
            }

            // Check if we've recovered all symbols
            if self.recovered_symbols.len() == self.k {
                return Ok(self.recovered_symbols.clone());
            }

            // Find degree-1 symbols (symbols that depend on exactly one unknown)
            let progress_before = self.recovered_symbols.len();

            // For each symbol, XOR out all known neighbors
            for symbol in self.encoded_symbols.iter() {
                let unknown_neighbors: Vec<usize> = symbol
                    .neighbors
                    .iter()
                    .copied()
                    .filter(|&neighbor_idx| !self.recovered_symbols.contains_key(&neighbor_idx))
                    .collect();

                // If exactly one unknown dependency remains
                if unknown_neighbors.len() == 1 {
                    let unknown_idx = unknown_neighbors[0];

                    // Skip if already recovered
                    if self.recovered_symbols.contains_key(&unknown_idx) {
                        continue;
                    }

                    // XOR out all known neighbors
                    let mut recovered = symbol.data.clone();
                    for &neighbor_idx in &symbol.neighbors {
                        if let Some(known_symbol) = self.recovered_symbols.get(&neighbor_idx) {
                            for (i, byte) in known_symbol.iter().enumerate() {
                                recovered[i] ^= byte;
                            }
                        }
                    }

                    self.recovered_symbols.insert(unknown_idx, recovered);
                }
            }

            // Check if we made progress
            if self.recovered_symbols.len() == progress_before {
                return Err(format!(
                    "Decoding stalled. Recovered {}/{} symbols",
                    self.recovered_symbols.len(),
                    self.k
                ));
            }
        }
    }

    /// Get a recovered source symbol by index
    pub fn get_symbol(&self, index: usize) -> Option<&Vec<u8>> {
        self.recovered_symbols.get(&index)
    }

    /// Get all recovered symbols
    pub fn get_recovered_symbols(&self) -> &HashMap<usize, Vec<u8>> {
        &self.recovered_symbols
    }

    /// Get recovery statistics
    pub fn recovery_stats(&self) -> (usize, usize) {
        (self.recovered_symbols.len(), self.k)
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;
    use crate::encoder::{distribution::RobustSoliton, test_encoder::TestFountainEncoder};

    const C: f64 = 0.06; // Constant factor (typically 0.03 to 0.1)
    const DELTA: f64 = 0.01; // Failure probability (typically 0.01 to 0.5)

    #[test]
    fn test_simple_single_symbol_recovery() {
        let mut decoder = TestFountainDecoder::new(1, 32);

        // Add an encoded symbol that depends only on source symbol 0
        let source_data = vec![42u8; 32];
        decoder.add_symbol(vec![0], source_data.clone());

        let result = decoder.decode();
        assert!(result.is_ok());

        let recovered = decoder.get_symbol(0).unwrap();
        assert_eq!(recovered, &source_data);
    }

    #[test]
    fn test_decoder_with_fountain_encoder() {
        // Create source symbols
        let source_symbols = vec![
            vec![1u8, 2, 3, 4, 5, 6, 7, 8],
            vec![10u8, 20, 30, 40, 50, 60, 70, 80],
            vec![12u8, 24, 36, 48, 52, 64, 76, 82],
            vec![100u8, 110, 120, 130, 140, 150, 160, 170],
            vec![200u8, 210, 220, 230, 240, 250, 251, 252],
        ];

        let k = source_symbols.len();
        let symbol_size = source_symbols[0].len();

        let c = 0.01;
        let delta = 0.01;

        // Encode using FountainEncoder
        let degree_distribution = RobustSoliton::new(k, c, delta);

        // Generate enough encoded symbols to recover all source symbols
        // With robust soliton, typically need k + overhead symbols
        let num_encoded = degree_distribution.min_encoded_symbols() + 1;

        let encoder = TestFountainEncoder::new(source_symbols.clone(), degree_distribution);
        let mut rng = rand::rng();

        let mut encoded_data = Vec::new();

        for _ in 0..num_encoded {
            let (indices, symbol) = encoder.generate_symbol(&mut rng);
            encoded_data.push((indices, symbol));
        }

        // Decode
        let mut decoder = TestFountainDecoder::new(k, symbol_size);

        for (neighbors, data) in encoded_data {
            decoder.add_symbol(neighbors, data);
        }

        let result = decoder.decode();
        assert!(result.is_ok(), "Decoding failed: {}", result.unwrap_err());

        // Verify recovered symbols match originals
        for (idx, original) in source_symbols.iter().enumerate() {
            let recovered = decoder.get_symbol(idx).expect("Symbol not recovered");
            assert_eq!(
                recovered, original,
                "Mismatch at symbol {}: expected {:?}, got {:?}",
                idx, original, recovered
            );
        }
    }

    #[test]
    fn test_decoder_with_larger_dataset() {
        // Create larger source symbols
        let source_symbols: Vec<Vec<u8>> = (0..10000).map(|i| vec![i as u8; 64]).collect();

        let k = source_symbols.len();
        let symbol_size = source_symbols[0].len();

        // Encode
        let degree_distribution = RobustSoliton::new(k, C, DELTA);

        // Generate enough encoded symbols to recover all source symbols (typically need k + small overhead)
        let num_encoded = degree_distribution.min_encoded_symbols();

        let encoder = TestFountainEncoder::new(source_symbols.clone(), degree_distribution);
        let mut rng = rand::rng();

        let mut encoded_data = Vec::new();

        for _ in 0..num_encoded {
            let (indices, symbol) = encoder.generate_symbol(&mut rng);
            encoded_data.push((indices, symbol));
        }

        // Decode
        let mut decoder = TestFountainDecoder::new(k, symbol_size);

        for (neighbors, data) in encoded_data {
            decoder.add_symbol(neighbors, data);
        }

        let result = decoder.decode();
        assert!(result.is_ok(), "Decoding failed: {}", result.unwrap_err());

        // Verify all symbols recovered
        let (recovered, total) = decoder.recovery_stats();
        assert_eq!(recovered, total, "Not all symbols recovered");

        // Verify correctness
        for (idx, original) in source_symbols.iter().enumerate() {
            let recovered = decoder.get_symbol(idx).expect("Symbol not recovered");
            assert_eq!(recovered, original, "Mismatch at symbol {}", idx);
        }
    }

    #[test]
    fn test_decoder_insufficient_symbols() {
        // Create source symbols
        let source_symbols = vec![
            vec![1u8, 2, 3, 4],
            vec![5u8, 6, 7, 8],
            vec![9u8, 10, 11, 12],
        ];

        let k = source_symbols.len();
        let symbol_size = source_symbols[0].len();

        // Encode
        let degree_distribution = RobustSoliton::new(k, C, DELTA);
        let encoder = TestFountainEncoder::new(source_symbols, degree_distribution);
        let mut rng = rand::rng();

        // Generate only k-1 encoded symbols (insufficient)
        let mut encoded_data = Vec::new();
        for _ in 0..(k - 1) {
            let (indices, symbol) = encoder.generate_symbol(&mut rng);
            encoded_data.push((indices, symbol));
        }

        // Try to decode
        let mut decoder = TestFountainDecoder::new(k, symbol_size);

        for (neighbors, data) in encoded_data {
            decoder.add_symbol(neighbors, data);
        }

        let result = decoder.decode();
        assert!(
            result.is_err(),
            "Should fail with insufficient symbols: {}",
            result.unwrap_err()
        );
    }
}
