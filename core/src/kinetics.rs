//! Scale 0 surrogate: a band of lattice collapsed to mass-action counts.
//!
//! State per band = free valence slots per element + internal bond counts per
//! element pair. Rates are learned from tallies of the detailed model, binned
//! over environment features. What the lattice knows that this does not:
//! which atom holds which slot, molecule shapes, spatial correlation. That
//! loss is what shadow mode measures.

use crate::element::{Energy, Table};
use crate::rng::Rng;
use crate::surrogate::{Axis, RateTable};
use serde::{Deserialize, Serialize};

/// Bondable elements (nobles excluded), as Z.
pub const BONDABLE: [u8; 6] = [1, 3, 4, 5, 6, 7];
pub const NB: usize = BONDABLE.len();
pub const N_PAIRS: usize = NB * (NB + 1) / 2;

pub const CH_FORM: usize = 0;
pub const CH_BREAK: usize = N_PAIRS;
pub const CH_EXCH: usize = 2 * N_PAIRS;
/// form per pair, break per pair, exchange per (attacker a, shared b, leaving c).
pub const N_CHANNELS: usize = 2 * N_PAIRS + NB * NB * NB;

/// Z -> bondable index, None for nobles.
pub fn bidx(z: u8) -> Option<usize> {
    BONDABLE.iter().position(|&b| b == z)
}

pub fn pair(a: usize, b: usize) -> usize {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    lo * NB - lo * (lo + 1) / 2 + hi
}

pub fn pair_members(p: usize) -> (usize, usize) {
    for a in 0..NB {
        for b in a..NB {
            if pair(a, b) == p {
                return (a, b);
            }
        }
    }
    unreachable!()
}

pub fn exch_channel(a: usize, b: usize, c: usize) -> usize {
    CH_EXCH + (a * NB + b) * NB + c
}

/// Environment features a band's rates depend on: kT, wet fraction, visible, uv.
pub fn features(kt: f64, wet: f64, visible: f64, uv: f64) -> [f64; 4] {
    [kt, wet, visible, uv]
}

pub fn new_table() -> RateTable {
    RateTable::new(
        vec![
            Axis::new("kT", 0.85, 1.15, 6),
            Axis::new("wet", 0.0, 1.0, 3),
            Axis::new("visible", 0.0, 1.2, 4),
            Axis::new("uv", 0.0, 1.6, 3),
        ],
        N_CHANNELS,
    )
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BandState {
    pub cells: f64,
    pub free: [i64; NB],
    pub bonds: [i64; N_PAIRS],
}

impl BandState {
    /// Mean-field exposure of a channel, per cell. Multiply by rate and sweeps.
    pub fn exposure(&self, ch: usize) -> f64 {
        let c = self.cells.max(1.0);
        if ch < CH_BREAK {
            let (a, b) = pair_members(ch - CH_FORM);
            let pairs = if a == b { (self.free[a] * (self.free[a] - 1)).max(0) as f64 / 2.0 } else { (self.free[a] * self.free[b]) as f64 };
            pairs / c
        } else if ch < CH_EXCH {
            self.bonds[ch - CH_BREAK] as f64
        } else {
            let k = ch - CH_EXCH;
            let (a, b, cc) = (k / (NB * NB), (k / NB) % NB, k % NB);
            let orient = if b == cc { 2.0 } else { 1.0 };
            self.free[a] as f64 * self.bonds[pair(b, cc)] as f64 * orient / c
        }
    }

    pub fn total_bonds(&self) -> i64 {
        self.bonds.iter().sum()
    }

    /// Bond energy stored in this band's internal bonds (negative = bound).
    pub fn stored(&self, t: &Table) -> Energy {
        let mut s = 0;
        for p in 0..N_PAIRS {
            let (a, b) = pair_members(p);
            s -= self.bonds[p] * t.bond[BONDABLE[a] as usize][BONDABLE[b] as usize];
        }
        s
    }
}

/// Events tallied in the detailed model, per band, since the last flush.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tally {
    pub events: Vec<f64>,
}

impl Default for Tally {
    fn default() -> Self {
        Self { events: vec![0.0; N_CHANNELS] }
    }
}

/// Advance a band by `sweeps` with tau-leaping. Returns the change in stored energy.
pub fn step_band(s: &mut BandState, table: &RateTable, chem: &Table, x: &[f64], sweeps: f64, rng: &mut Rng) -> Energy {
    let before = s.stored(chem);
    // random channel order: clamping should not systematically favour low indices
    let mut order: Vec<usize> = (0..N_CHANNELS).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.below(i + 1));
    }
    for ch in order {
        let expo = s.exposure(ch);
        if expo <= 0.0 {
            continue;
        }
        let r = table.rate(x, ch, table.global_rate(ch), 1.0);
        let k = rng.poisson(r * expo * sweeps) as i64;
        if k == 0 {
            continue;
        }
        if ch < CH_BREAK {
            let (a, b) = pair_members(ch);
            let cap = if a == b { s.free[a] / 2 } else { s.free[a].min(s.free[b]) };
            let k = k.min(cap);
            s.free[a] -= k;
            s.free[b] -= k;
            s.bonds[ch] += k;
        } else if ch < CH_EXCH {
            let p = ch - CH_BREAK;
            let (a, b) = pair_members(p);
            let k = k.min(s.bonds[p]);
            s.bonds[p] -= k;
            s.free[a] += k;
            s.free[b] += k;
        } else {
            let q = ch - CH_EXCH;
            let (a, b, c) = (q / (NB * NB), (q / NB) % NB, q % NB);
            let k = k.min(s.free[a]).min(s.bonds[pair(b, c)]);
            s.bonds[pair(b, c)] -= k;
            s.bonds[pair(a, b)] += k;
            s.free[a] -= k;
            s.free[c] += k;
        }
    }
    s.stored(chem) - before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_index_is_a_bijection() {
        let mut seen = vec![false; N_PAIRS];
        for a in 0..NB {
            for b in 0..NB {
                let p = pair(a, b);
                assert_eq!(p, pair(b, a));
                let (x, y) = pair_members(p);
                assert_eq!((x.min(y), x.max(y)), (a.min(b), a.max(b)));
                seen[p] = true;
            }
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn step_conserves_valence() {
        let mut t = new_table();
        let x = features(1.0, 1.0, 0.5, 0.2);
        t.record_sample(&x);
        for ch in 0..N_CHANNELS {
            t.record(&x, ch, 1.0, 10.0);
        }
        let mut s = BandState { cells: 1000.0, free: [100, 20, 30, 80, 60, 40], bonds: [0; N_PAIRS] };
        let slots = |s: &BandState| s.free.iter().sum::<i64>() + 2 * s.total_bonds();
        let before = slots(&s);
        let mut rng = Rng::new(3);
        for _ in 0..50 {
            step_band(&mut s, &t, &Table::new(), &x, 1.0, &mut rng);
            assert!(s.free.iter().all(|&f| f >= 0) && s.bonds.iter().all(|&b| b >= 0));
        }
        assert_eq!(slots(&s), before);
    }
}
