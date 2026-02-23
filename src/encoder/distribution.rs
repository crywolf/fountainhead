#![allow(dead_code)]

use rand::{Rng, RngExt};

/// [Robust Soliton Distribution](https://en.wikipedia.org/wiki/Soliton_distribution) for Fountain codes
pub struct RobustSoliton {
    /// Number of source symbols (ie. blocks in our case)
    k: usize,
    /// Cumulative Distribution Function
    cdf: Vec<f64>,
    /// Failure probability (typically 0.01 to 0.5)
    delta: f64,
    /// R = c*ln(K/δ)*√K
    r: f64,
    /// Normalization factor β = ∑ ρ(d) + θ(d)
    beta: f64,
}

impl RobustSoliton {
    #![allow(clippy::needless_range_loop)]

    /// Construct a new Robust Soliton Distribution for the given number of source symbols `k`,
    /// constant factor `c` and failure probability `delta`.
    ///
    /// Constant factor is typically 0.03 to 0.1, Failure probability typically 0.01 to 0.5.
    pub fn new(k: usize, c: f64, delta: f64) -> Self {
        // R = c*ln(K/δ)*√K
        let mut r = c * (k as f64 / delta).ln() * (k as f64).sqrt();
        if r < 1.0 {
            r = 1.0
        };

        let mut distribution = Self {
            k,
            cdf: Vec::new(),
            delta,
            r,
            beta: 0.0,
        };

        distribution.build_cdf();
        distribution
    }

    /// The number of encoded symbols required at the receiving end to ensure that the
    /// decoding can run to completion, with probability at least 1 - delta
    pub fn min_encoded_symbols(&self) -> usize {
        // The number of encoded symbols required at the receiving end to ensure that the
        // decoding can run to completion, with probability at least 1 - delta, is β*k
        (self.beta * self.k as f64).ceil() as usize
    }

    /// Build the cumulative distribution function
    fn build_cdf(&mut self) {
        let mut pmf = self.compute_pmf();

        // Normalize to ensure it sums to 1.0
        let sum: f64 = pmf.iter().sum();
        for p in &mut pmf {
            *p /= sum;
        }
        self.beta = sum;

        // Build CDF
        self.cdf = Vec::with_capacity(pmf.len());
        let mut cumsum = 0.0;
        for p in pmf {
            cumsum += p;
            self.cdf.push(cumsum);
        }
    }

    /// Compute the robust soliton probability mass function (PMF)
    fn compute_pmf(&self) -> Vec<f64> {
        // Ideal soliton distribution
        let ideal = self.ideal_soliton();

        let k = self.k;

        // Robust soliton parameters
        let delta = self.delta;
        let r = self.r;

        // While the ideal soliton distribution has a mode (or spike) at 2,
        // the effect of the extra component in the robust distribution is to add an additional spike at the value K/R.

        let mut pmf = vec![0.0; k + 1];

        // Dirac spike
        for d in 1..(k / r as usize) {
            pmf[d] += 1.0 / (d as f64 * r);
        }
        pmf[k / r as usize] = (r / k as f64) * (r / delta).ln();

        // Combine ideal soliton with Dirac spike
        for i in 1..=k {
            pmf[i] += ideal[i];
        }

        pmf
    }

    /// Compute the ideal soliton distribution
    fn ideal_soliton(&self) -> Vec<f64> {
        let k = self.k;

        let mut ideal = vec![0.0; k + 1];

        ideal[1] = 1.0 / k as f64;

        for d in 2..=k {
            ideal[d] = 1.0 / (d as f64 * (d - 1) as f64);
        }

        ideal
    }
}

impl rand::distr::Distribution<usize> for RobustSoliton {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let u = rng.random::<f64>();

        // Sample a value from the distribution using inverse transform sampling
        // Binary search to find the degree
        match self
            .cdf
            .binary_search_by(|cdf_val| cdf_val.partial_cmp(&u).unwrap_or(std::cmp::Ordering::Less))
        {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::distr::Distribution as _;

    #[test]
    fn test_cdf_properties() {
        let c = 0.2;
        let delta = 0.05;
        let distribution = RobustSoliton::new(100, c, delta);

        // CDF should be monotonically increasing
        for i in 1..distribution.cdf.len() {
            assert!(distribution.cdf[i] >= distribution.cdf[i - 1]);
        }

        // Last value should be close to 1.0
        assert!((distribution.cdf[distribution.cdf.len() - 1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_sampling() {
        let c = 0.2;
        let delta = 0.05;
        let distribution = RobustSoliton::new(100, c, delta);
        let mut rng = rand::rng();

        for _ in 0..1000 {
            let degree = distribution.sample(&mut rng);
            assert!((1..=100).contains(&degree)); // degree >= 1 && degree <= 100
        }
    }

    #[test]
    fn test_degree_distribution_1() {
        let k = 10000;
        let c = 0.2; // Constant factor (typically 0.03 to 0.1)
        let delta = 0.05; // Failure probability (typically 0.01 to 0.5)

        let distribution = RobustSoliton::new(k, c, delta);
        let mut rng = rand::rng();
        let mut histogram = vec![0; 10001];

        // Sample 100k times
        for _ in 0..100_000 {
            let degree = distribution.sample(&mut rng);
            histogram[degree] += 1;
        }

        assert_eq!(histogram[0], 0, "Degree 0 cannot occur");

        // Degree 2 should be most frequent (soliton property)
        let max_degree = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .map(|(idx, _)| idx)
            .unwrap();

        assert_eq!(max_degree, 2, "Degree 2 should be the most frequent");

        // based on paper by D.J.C. MacKay: http://switzernet.com/people/emin-gabrielyan/060112-capillary-references/ref/MacKay05.pdf
        assert_eq!(
            distribution.r.round(),
            244.0,
            "R for k=10000, c=0.2, delta=0.05 is not correct"
        );
        assert_eq!(
            (k as f64 / distribution.r).round(),
            41.0,
            "k/R for k=10000, c=0.2, delta=0.05 is not correct"
        );
        assert!(
            distribution.beta < 1.3,
            "beta={} for k=10000, c=0.2, delta=0.05 is not correct",
            distribution.beta
        );
    }

    #[test]
    fn test_degree_distribution_2() {
        let k = 10000;
        let c = 0.01;
        let delta = 0.5;

        let distribution = RobustSoliton::new(k, c, delta);
        let mut rng = rand::rng();
        let mut histogram = vec![0; 10001];

        // Sample 100k times
        for _ in 0..100_000 {
            let degree = distribution.sample(&mut rng);
            histogram[degree] += 1;
        }

        assert_eq!(histogram[0], 0, "Degree 0 cannot occur");

        // Degree 2 should be most frequent (soliton property)
        let max_degree = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .map(|(idx, _)| idx)
            .unwrap();

        assert_eq!(max_degree, 2, "Degree 2 should be the most frequent");

        // based on paper by D.J.C. MacKay: http://switzernet.com/people/emin-gabrielyan/060112-capillary-references/ref/MacKay05.pdf
        assert_eq!(
            distribution.r.round(),
            10.0,
            "R for k=10000, c=0.01, delta=0.5 is not correct"
        );
        assert_eq!(
            (k as f64 / distribution.r).round(),
            1010.0,
            "k/R for k=10000, c=0.01, delta=0.5 is not correct"
        );
        // assert!(
        //     distribution.beta < 1.01,
        //     "beta={} for k=10000, c=0.01, delta=0.5 is not correct",
        //     distribution.beta,
        // );
    }

    #[test]
    fn test_degree_distribution_3() {
        let k = 10000;
        let c = 0.03;
        let delta = 0.5;

        let distribution = RobustSoliton::new(k, c, delta);
        let mut rng = rand::rng();
        let mut histogram = vec![0; 10001];

        // Sample 100k times
        for _ in 0..100_000 {
            let degree = distribution.sample(&mut rng);
            histogram[degree] += 1;
        }

        assert_eq!(histogram[0], 0, "Degree 0 cannot occur");

        // Degree 2 should be most frequent (soliton property)
        let max_degree = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .map(|(idx, _)| idx)
            .unwrap();

        assert_eq!(max_degree, 2, "Degree 2 should be the most frequent");

        // based on paper by D.J.C. MacKay: http://switzernet.com/people/emin-gabrielyan/060112-capillary-references/ref/MacKay05.pdf
        assert_eq!(
            distribution.r.round(),
            30.0,
            "R for k=10000, c=0.03, delta=0.5 is not correct"
        );
        assert_eq!(
            (k as f64 / distribution.r).round(),
            337.0,
            "k/R for k=10000, c=0.03, delta=0.5 is not correct"
        );
        // assert!(
        //     distribution.beta < 1.03,
        //     "beta={} for k=10000, c=0.03, delta=0.5 is not correct",
        //     distribution.beta
        // );
    }

    #[test]
    fn test_degree_distribution_4() {
        let k = 10000;
        let c = 0.1;
        let delta = 0.5;

        let distribution = RobustSoliton::new(k, c, delta);
        let mut rng = rand::rng();
        let mut histogram = vec![0; 10001];

        // Sample 100k times
        for _ in 0..100_000 {
            let degree = distribution.sample(&mut rng);
            histogram[degree] += 1;
        }

        assert_eq!(histogram[0], 0, "Degree 0 cannot occur");

        // Degree 2 should be most frequent (soliton property)
        let max_degree = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .map(|(idx, _)| idx)
            .unwrap();

        assert_eq!(max_degree, 2, "Degree 2 should be the most frequent");

        // based on paper by D.J.C. MacKay: http://switzernet.com/people/emin-gabrielyan/060112-capillary-references/ref/MacKay05.pdf
        assert_eq!(
            distribution.r.round(),
            99.0,
            "R for k=10000, c=0.1, delta=0.5 is not correct"
        );
        assert_eq!(
            (k as f64 / distribution.r).round(),
            101.0,
            "k/R for k=10000, c=0.1, delta=0.5 is not correct"
        );
        assert!(
            distribution.beta < 1.105,
            "beta={} for k=10000, c=0.1, delta=0.5 is not correct",
            distribution.beta
        );
    }
}
