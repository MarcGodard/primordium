//! Scale-agnostic surrogate machinery: learned rate tables with confidence,
//! a discovery log, and a shadow-mode accuracy metric.
//!
//! A surrogate is a behaviour function learned from the detailed model one
//! scale down. It must know where it has data: `confidence` falls off where
//! training never went, and the caller drops to the detailed model there.
//! That drop is a *discovery*: the world went somewhere the surrogate had not.

use serde::{Deserialize, Serialize};

/// Linear bins over [lo, hi]. Queries outside the range clamp to the edge
/// bin but count as out of range for confidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Axis {
    pub name: String,
    pub lo: f64,
    pub hi: f64,
    pub bins: usize,
}

impl Axis {
    pub fn new(name: &str, lo: f64, hi: f64, bins: usize) -> Self {
        Self { name: name.into(), lo, hi, bins }
    }

    fn locate(&self, x: f64) -> (usize, bool) {
        let f = (x - self.lo) / (self.hi - self.lo);
        let inside = (0.0..=1.0).contains(&f);
        let b = (f * self.bins as f64).floor().clamp(0.0, (self.bins - 1) as f64) as usize;
        (b, inside)
    }
}

/// Mass-action rate table: rate[bin][channel] = events / exposure.
/// Exposure is whatever the channel's rate multiplies (pair counts, sweeps).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateTable {
    pub axes: Vec<Axis>,
    pub channels: usize,
    events: Vec<f64>,
    exposure: Vec<f64>,
    /// Training samples that landed in each bin.
    samples: Vec<u32>,
    /// Samples for full confidence (1 - 1/e at this count).
    pub samples_for_confidence: f64,
}

impl RateTable {
    pub fn new(axes: Vec<Axis>, channels: usize) -> Self {
        let n: usize = axes.iter().map(|a| a.bins).product();
        Self { axes, channels, events: vec![0.0; n * channels], exposure: vec![0.0; n * channels], samples: vec![0; n], samples_for_confidence: 6.0 }
    }

    pub fn n_bins(&self) -> usize {
        self.samples.len()
    }

    /// Flat bin index and whether every coordinate was in range.
    pub fn bin(&self, x: &[f64]) -> (usize, bool) {
        let mut idx = 0;
        let mut inside = true;
        for (a, &v) in self.axes.iter().zip(x) {
            let (b, ok) = a.locate(v);
            idx = idx * a.bins + b;
            inside &= ok;
        }
        (idx, inside)
    }

    pub fn record_sample(&mut self, x: &[f64]) {
        let (b, _) = self.bin(x);
        self.samples[b] += 1;
    }

    pub fn record(&mut self, x: &[f64], ch: usize, events: f64, exposure: f64) {
        let (b, _) = self.bin(x);
        self.events[b * self.channels + ch] += events;
        self.exposure[b * self.channels + ch] += exposure;
    }

    /// Pooled rate across all bins: prior for thin cells.
    pub fn global_rate(&self, ch: usize) -> f64 {
        let (mut e, mut x) = (0.0, 0.0);
        for b in 0..self.n_bins() {
            e += self.events[b * self.channels + ch];
            x += self.exposure[b * self.channels + ch];
        }
        if x > 0.0 { e / x } else { 0.0 }
    }

    /// Shrinks toward the global rate with pseudo-exposure `prior_weight`.
    pub fn rate(&self, x: &[f64], ch: usize, prior: f64, prior_weight: f64) -> f64 {
        let (b, _) = self.bin(x);
        let e = self.events[b * self.channels + ch];
        let xp = self.exposure[b * self.channels + ch];
        (e + prior * prior_weight) / (xp + prior_weight)
    }

    /// 0..1. Zero outside the trained range; saturates with samples in bin.
    pub fn confidence(&self, x: &[f64]) -> f64 {
        let (b, inside) = self.bin(x);
        if !inside {
            return 0.0;
        }
        1.0 - (-(self.samples[b] as f64) / self.samples_for_confidence).exp()
    }

    /// Fraction of bins with any data. Coverage, not accuracy.
    pub fn coverage(&self) -> f64 {
        self.samples.iter().filter(|&&s| s > 0).count() as f64 / self.n_bins() as f64
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub t_days: f64,
    pub scale: u8,
    pub region: usize,
    pub confidence: f64,
    pub features: Vec<f64>,
    pub note: String,
}

/// Fallback events. Deduped per (scale, feature bin) so a long excursion is one entry.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscoveryLog {
    pub entries: Vec<Discovery>,
    seen: std::collections::HashSet<(u8, usize)>,
}

impl DiscoveryLog {
    /// Returns true if this was new.
    pub fn log(&mut self, bin: usize, d: Discovery) -> bool {
        if self.seen.insert((d.scale, bin)) {
            self.entries.push(d);
            true
        } else {
            false
        }
    }

    /// Bin learned: allow a future excursion there to be logged again.
    pub fn forget_bin(&mut self, scale: u8, bin: usize) {
        self.seen.remove(&(scale, bin));
    }
}

/// Shadow-mode agreement, 0..1: 1 - L1(a - b) / L1(a). Compares what the scale above consumes.
pub fn agreement(detailed: &[f64], surrogate: &[f64]) -> f64 {
    let diff: f64 = detailed.iter().zip(surrogate).map(|(a, b)| (a - b).abs()).sum();
    let tot: f64 = detailed.iter().map(|a| a.abs()).sum();
    if tot == 0.0 { if diff == 0.0 { 1.0 } else { 0.0 } } else { (1.0 - diff / tot).max(0.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_tracks_coverage() {
        let mut t = RateTable::new(vec![Axis::new("kt", 0.0, 1.0, 4)], 1);
        assert_eq!(t.confidence(&[0.1]), 0.0);
        for _ in 0..30 {
            t.record_sample(&[0.1]);
        }
        assert!(t.confidence(&[0.1]) > 0.9);
        assert_eq!(t.confidence(&[0.9]), 0.0);
        assert_eq!(t.confidence(&[1.5]), 0.0, "out of range is never confident");
    }

    #[test]
    fn agreement_metric() {
        assert_eq!(agreement(&[10.0, 10.0], &[10.0, 10.0]), 1.0);
        assert!((agreement(&[10.0, 10.0], &[9.0, 11.0]) - 0.9).abs() < 1e-12);
    }
}
