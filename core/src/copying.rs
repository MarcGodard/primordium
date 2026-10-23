//! Templated copying, one letter at a time: Boltzmann pairing (detail) or a learned model of it
//! (surrogate). The learned model holds only what pairing chemistry decides; the monomer mix is
//! applied at copy time, so one table covers any supply.
use crate::polymer::N_LETTERS;
use crate::rng::Rng;
use crate::surrogate::RateTable;

/// Channels per table: inverse selectivity per (template, partner), then fray-after-mismatch
/// (a pool copy stalls on a fray; a cell's polymerase clips the letter and retries).
pub const COPY_CHANNELS: usize = 2 * N_LETTERS * N_LETTERS;

fn sel_channel(t: usize, l: usize) -> usize {
    t * N_LETTERS + l
}

fn stall_channel(t: usize, l: usize) -> usize {
    N_LETTERS * N_LETTERS + t * N_LETTERS + l
}

/// Detail tallies, per (template, partner). Selectivity: wrong picks, and the summed supply
/// ratio free[partner] / free[right] over right picks; their ratio estimates 1 / selectivity
/// exactly in expectation (only letters with the right partner present count). Stalls: how
/// often a mismatch of that kind frays (pool: the copy ends; cell: the letter is clipped).
#[derive(Clone, Debug, Default)]
pub struct PairTally {
    pub picks: [[f64; N_LETTERS]; N_LETTERS],
    pub odds: [[f64; N_LETTERS]; N_LETTERS],
    pub stalls: [[f64; N_LETTERS]; N_LETTERS],
    pub mismatches: [[f64; N_LETTERS]; N_LETTERS],
}

impl PairTally {
    /// Add to `table` channels starting at `base`.
    pub fn record(&self, table: &mut RateTable, x: &[f64], base: usize) {
        for t in 0..N_LETTERS {
            for l in 0..N_LETTERS {
                if self.picks[t][l] > 0.0 || self.odds[t][l] > 0.0 {
                    table.record(x, base + sel_channel(t, l), self.picks[t][l], self.odds[t][l]);
                }
                if self.mismatches[t][l] > 0.0 {
                    table.record(x, base + stall_channel(t, l), self.stalls[t][l], self.mismatches[t][l]);
                }
            }
        }
    }
}

/// Learned pairing at one state: what the surrogate copies with.
#[derive(Clone, Copy, Debug)]
pub struct LearnedCopy {
    pub inv_sel: [[f64; N_LETTERS]; N_LETTERS],
    pub stall: [[f64; N_LETTERS]; N_LETTERS],
}

impl LearnedCopy {
    /// `prior` holds the table's pooled rate per channel (from `base`), for thin bins.
    pub fn at(table: &RateTable, x: &[f64], prior: &[f64], base: usize) -> Self {
        let mut m = LearnedCopy { inv_sel: [[0.0; N_LETTERS]; N_LETTERS], stall: [[0.0; N_LETTERS]; N_LETTERS] };
        for t in 0..N_LETTERS {
            for l in 0..N_LETTERS {
                let (s, k) = (sel_channel(t, l), stall_channel(t, l));
                m.inv_sel[t][l] = table.rate(x, base + s, prior[s], 1.0).max(0.0);
                m.stall[t][l] = table.rate(x, base + k, prior[k], 1.0).clamp(0.0, 1.0);
            }
        }
        m
    }

    pub fn priors(table: &RateTable, base: usize) -> Vec<f64> {
        (0..COPY_CHANNELS).map(|ch| table.global_rate(base + ch)).collect()
    }
}

#[derive(Clone, Copy)]
pub enum Copier<'a> {
    /// Boltzmann weights exp(gain * pair / kT). A mismatched end frays with 1 - exp(-shortfall / kT);
    /// `fray` (0..1) scales how often that ends in a stall or a clip (0: never handled).
    Pairing {
        gain: f64,
        kt: f64,
        pair: &'a [[f64; N_LETTERS]; N_LETTERS],
        fray: f64,
    },
    Learned(&'a LearnedCopy),
}

impl Copier<'_> {
    /// Choose the partner for template letter `t` from `free` (not decremented). Returns the
    /// pick and whether the copy stalls after it. `None` when no monomer is available.
    pub fn pick(&self, t: usize, right: usize, free: &[i64; N_LETTERS], tally: &mut PairTally, rng: &mut Rng) -> Option<(usize, bool)> {
        let mut w = [0.0; N_LETTERS];
        match *self {
            Copier::Pairing { gain, kt, pair, .. } => {
                for l in 0..N_LETTERS {
                    w[l] = free[l].max(0) as f64 * (gain * pair[t][l] / kt).exp();
                }
            }
            Copier::Learned(m) => {
                for l in 0..N_LETTERS {
                    w[l] = free[l].max(0) as f64 * m.inv_sel[t][l];
                }
                w[right] = free[right].max(0) as f64;
            }
        }
        let sum: f64 = w.iter().sum();
        if sum <= 0.0 {
            return None;
        }
        let mut r = rng.f64() * sum;
        // fallback when rounding walks r past the end: the right letter if present
        let mut pick = if w[right] > 0.0 { right } else { (0..N_LETTERS).rev().find(|&l| w[l] > 0.0).unwrap() };
        for (l, &wl) in w.iter().enumerate() {
            if r < wl {
                pick = l;
                break;
            }
            r -= wl;
        }
        let mut stall = false;
        if pick != right {
            stall = match *self {
                Copier::Pairing { gain, kt, pair, fray } => fray > 0.0 && rng.chance(fray * (1.0 - (-(gain * (pair[t][right] - pair[t][pick])) / kt).exp())),
                Copier::Learned(m) => m.stall[t][pick] > 0.0 && rng.chance(m.stall[t][pick]),
            };
        }
        if let Copier::Pairing { .. } = *self {
            if free[right] > 0 {
                if pick != right {
                    tally.picks[t][pick] += 1.0;
                } else {
                    for l in (0..N_LETTERS).filter(|&l| l != right) {
                        tally.odds[t][l] += free[l].max(0) as f64 / free[right] as f64;
                    }
                }
            }
            // count every mismatch, clipped or not: a pocketless cell's zero clip rate is data too
            if pick != right {
                tally.mismatches[t][pick] += 1.0;
                tally.stalls[t][pick] += stall as u8 as f64;
            }
        }
        Some((pick, stall))
    }
}
