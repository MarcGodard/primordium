//! Scale 1 world: a tide pool of sequence polymers.
//!
//! Runs on scale-0 *surrogate* rates, never on atoms: backbone (Ka-Ka) bond
//! formation and breakage per environment come from the learned RateTable.
//! Matter closed per letter: free monomers + residues in chains is constant.
//!
//! Heredity is templated copying: each new letter is drawn with weight
//! availability * exp(pair_energy / kT), so fidelity is physics, worse when hot.
//! Copies need a dry spell (condensation); the duplex must melt (wet, warm)
//! before the template can be read again. Tides are the thermocycler.
//!
//! Catalysis is pool-wide here: a chain that cuts the Ka-Ka barrier speeds
//! every copy and ligation, so it helps rivals as much as itself. Compartments
//! (scale 2) are what make catalysts selfish enough to be selected.

use crate::cosmos::{Cosmos, EnvSample};
use crate::element::{KT, Table};
use crate::kinetics::{CH_BREAK, CH_FORM, bidx, pair};
use crate::polymer::{Function, LetterPhysics, N_LETTERS, Seq, express, seq_string};
use crate::rng::Rng;
use crate::surrogate::{Discovery, DiscoveryLog, RateTable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PoolParams {
    /// Pool floor elevation on the beach, 0..1. Below tide = wet.
    pub elevation: f64,
    /// Monomers of each letter in the pool (matter budget).
    pub monomers: [i64; N_LETTERS],
    /// Pool volume in lattice cells; turns counts into concentrations for mass action.
    pub cells: f64,
    /// Scale-0 sweeps per day, to convert learned per-sweep rates.
    pub sweeps_per_day: f64,
    /// Templated copy attempts per chain per day at unit monomer fraction.
    pub copy_rate: f64,
    /// Per-letter chance a copy stalls (processivity).
    pub stall: f64,
    /// Fraction of a residue's backbone bond protected when the chain is folded.
    pub fold_protect: f64,
    pub max_len: usize,
    /// Below this function confidence, fold for real (detail) and log.
    pub fold_below: f64,
}

impl Default for PoolParams {
    fn default() -> Self {
        Self {
            elevation: 0.56,
            monomers: [1500, 1500, 900, 900, 1500],
            cells: 20000.0,
            sweeps_per_day: 400.0,
            copy_rate: 6.0,
            stall: 0.03,
            fold_protect: 0.7,
            max_len: 40,
            fold_below: 0.9,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chain {
    pub id: u64,
    pub seq: Seq,
    pub parent: Option<u64>,
    pub born: f64,
    /// Bound to its last copy; must melt before it can template again.
    pub paired: bool,
    pub func: Function,
    pub conf: f64,
    pub generation: u32,
}

/// Scale-1 surrogate: sequence -> function. Exact cache hit, else the nearest
/// cached sequence of equal length by Hamming distance. Confidence = shared fraction.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionModel {
    cache: HashMap<(Seq, i32), Function>,
    by_len: HashMap<(usize, i32), Vec<Seq>>,
    pub folds: u64,
    pub exact: u64,
    pub nearest: u64,
}

/// kT bins of 0.05: fold and stability shift with temperature.
fn kt_bin(kt: f64) -> i32 {
    (kt / 0.05).round() as i32
}

impl FunctionModel {
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Detail: fold and memoize.
    pub fn fold(&mut self, seq: &[u8], kt: f64, ph: &LetterPhysics) -> Function {
        let kb = kt_bin(kt);
        let (_, f) = express(seq, ph, kb as f64 * 0.05, 1);
        self.cache.insert((seq.to_vec(), kb), f.clone());
        self.by_len.entry((seq.len(), kb)).or_default().push(seq.to_vec());
        self.folds += 1;
        f
    }

    /// Surrogate prediction and its confidence. None if nothing comparable is cached.
    pub fn predict(&self, seq: &[u8], kt: f64) -> Option<(Function, f64)> {
        let kb = kt_bin(kt);
        if let Some(f) = self.cache.get(&(seq.to_vec(), kb)) {
            return Some((f.clone(), 1.0));
        }
        let cands = self.by_len.get(&(seq.len(), kb))?;
        let mut best: Option<(&Seq, usize)> = None;
        for c in cands {
            let d = c.iter().zip(seq).filter(|(a, b)| a != b).count();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((c, d));
            }
        }
        let (c, d) = best?;
        let f = self.cache.get(&(c.clone(), kb))?.clone();
        Some((f, 1.0 - d as f64 / seq.len().max(1) as f64))
    }

    /// Returns function, confidence, and whether the real fold ran.
    pub fn get(&mut self, seq: &[u8], kt: f64, ph: &LetterPhysics, fold_below: f64) -> (Function, f64, bool) {
        match self.predict(seq, kt) {
            Some((f, c)) if c >= fold_below => {
                if c >= 1.0 {
                    self.exact += 1;
                } else {
                    self.nearest += 1;
                }
                (f, c, false)
            }
            _ => (self.fold(seq, kt, ph), 1.0, true),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PoolStats {
    pub copies: u64,
    pub mutations: u64,
    pub copied_letters: u64,
    pub breaks: u64,
    pub ligations: u64,
    pub melts: u64,
}

pub struct Pool {
    pub p: PoolParams,
    pub cosmos: Cosmos,
    pub rates: RateTable,
    pub ph: LetterPhysics,
    pub table: Table,
    pub chains: Vec<Chain>,
    pub free: [i64; N_LETTERS],
    pub t_days: f64,
    pub env: EnvSample,
    pub model: FunctionModel,
    pub discoveries: DiscoveryLog,
    pub stats: PoolStats,
    /// Best Ka-Ka barrier cut seen so far; beating it is a discovery.
    pub best_cut: f64,
    rng: Rng,
    next_id: u64,
}

const KA: u8 = 5;
/// Weak pairing in the dry is this much stronger: no water competing for the bond.
pub const DRY_PAIR_GAIN: f64 = 2.0;

impl Pool {
    pub fn new(p: PoolParams, cosmos: Cosmos, rates: RateTable, seed: u64) -> Self {
        let table = Table::new();
        let ph = LetterPhysics::new(&table);
        let env = cosmos.sample(0.25);
        let free = p.monomers;
        Self {
            p,
            cosmos,
            rates,
            ph,
            table,
            chains: Vec::new(),
            free,
            t_days: 0.25,
            env,
            model: FunctionModel::default(),
            discoveries: DiscoveryLog::default(),
            stats: PoolStats::default(),
            best_cut: 0.0,
            rng: Rng::new(seed),
            next_id: 1,
        }
    }

    pub fn wet(&self) -> bool {
        self.env.tide_level > self.p.elevation
    }

    /// Scale-0 features for the pool floor: same shape the rate table was trained on.
    pub fn features(&self) -> [f64; 4] {
        let depth = (self.env.tide_level - self.p.elevation).max(0.0);
        let wet = if self.wet() { 1.0 } else { 0.0 };
        crate::kinetics::features(self.env.kt, wet, self.env.visible * (-1.5 * depth).exp(), self.env.uv * (-8.0 * depth).exp())
    }

    /// Learned scale-0 rate for a backbone channel, per sweep per unit exposure.
    fn backbone_rate(&self, ch_base: usize) -> f64 {
        let ka = bidx(KA).unwrap();
        let ch = ch_base + pair(ka, ka);
        let x = self.features();
        self.rates.rate(&x, ch, self.rates.global_rate(ch), 1.0)
    }

    /// Pool catalysis of Ka-Ka bonds, kT of barrier cut. Diluted by chain count vs volume.
    pub fn backbone_cut(&self) -> f64 {
        let ka = bidx(KA).unwrap();
        let p = pair(ka, ka);
        let total: f64 = self.chains.iter().map(|c| c.func.catalysis.get(p).copied().unwrap_or(0.0)).sum();
        // saturating: many weak catalysts do not add past the best single site
        let best = self.chains.iter().map(|c| c.func.catalysis.get(p).copied().unwrap_or(0.0)).fold(0.0, f64::max);
        best.min(total * 200.0 / self.p.cells)
    }

    pub fn residues(&self) -> [i64; N_LETTERS] {
        let mut r = [0; N_LETTERS];
        for c in &self.chains {
            for &l in &c.seq {
                r[l as usize] += 1;
            }
        }
        r
    }

    fn spawn(&mut self, seq: Seq, parent: Option<u64>, generation: u32) {
        let kt = self.env.kt;
        let (func, conf, folded) = self.model.get(&seq, kt, &self.ph, self.p.fold_below);
        if folded {
            let ka = bidx(KA).unwrap();
            let cut = func.catalysis.get(pair(ka, ka)).copied().unwrap_or(0.0);
            if cut > self.best_cut + 0.05 {
                self.best_cut = cut;
                self.discoveries.log(
                    self.discoveries.entries.len(),
                    Discovery {
                        t_days: self.t_days,
                        scale: 1,
                        region: 0,
                        confidence: 0.0,
                        features: vec![cut, seq.len() as f64],
                        note: format!("new best backbone catalyst {} cuts {:.2} kT", seq_string(&seq), cut),
                    },
                );
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.chains.push(Chain { id, seq, parent, born: self.t_days, paired: false, func, conf, generation });
    }

    /// Advance `dt` days. Order: hydrolysis, melting, copying, ligation.
    pub fn step(&mut self, dt: f64) {
        self.env = self.cosmos.sample(self.t_days);
        let kt = self.env.kt;
        let sweeps = dt * self.p.sweeps_per_day;
        let cut = self.backbone_cut();
        let boost = (cut * KT / (kt * KT)).exp();
        let k_break = self.backbone_rate(CH_BREAK) * boost;
        let k_form = self.backbone_rate(CH_FORM) * boost;
        let wet = self.wet();

        // Hydrolysis / photolysis: per backbone bond, folded bonds partly shielded.
        // Pieces join the next step's population, so each chain is exposed once per step.
        for c in std::mem::take(&mut self.chains) {
            let shield = 1.0 - self.p.fold_protect * c.func.stability;
            let p_bond = 1.0 - (-k_break * shield * sweeps).exp();
            let bonds = c.seq.len().saturating_sub(1);
            let mut cuts: Vec<usize> = (1..=bonds).filter(|_| self.rng.chance(p_bond)).collect();
            if cuts.is_empty() {
                self.chains.push(c);
                continue;
            }
            self.stats.breaks += cuts.len() as u64;
            cuts.push(c.seq.len());
            let mut start = 0;
            for k in cuts {
                let piece = c.seq[start..k].to_vec();
                start = k;
                if piece.len() == 1 {
                    self.free[piece[0] as usize] += 1;
                } else {
                    self.spawn(piece, Some(c.id), c.generation);
                }
            }
        }

        // Melting: duplex strands separate in water with Boltzmann odds on mean pair energy.
        if wet {
            for c in self.chains.iter_mut().filter(|c| c.paired) {
                let mean_pair: f64 = c.seq.iter().map(|&l| self.ph.pair[l as usize].iter().cloned().fold(0.0, f64::max)).sum::<f64>() / c.seq.len() as f64;
                // cooperative: a few-residue core must open at once
                let p_melt = (-(mean_pair * 3.0) / kt).exp() * 20.0 * dt;
                if self.rng.chance(p_melt.min(1.0)) {
                    c.paired = false;
                    self.stats.melts += 1;
                }
            }
        }

        // Templated copying in the dry: needs unpaired template and monomers.
        if !wet {
            let total_free: i64 = self.free.iter().sum();
            let frac = total_free as f64 / self.p.monomers.iter().sum::<i64>() as f64;
            let n = self.chains.len();
            for ci in 0..n {
                if self.chains[ci].paired || !self.rng.chance((self.p.copy_rate * frac * boost * dt).min(1.0)) {
                    continue;
                }
                let template = self.chains[ci].seq.clone();
                let mut copy = Vec::with_capacity(template.len());
                let mut mutations = 0;
                // read 3' to 5': the copy is the reverse complement
                for &t in template.iter().rev() {
                    let mut w = [0.0; N_LETTERS];
                    let mut sum = 0.0;
                    for l in 0..N_LETTERS {
                        w[l] = self.free[l].max(0) as f64 * (DRY_PAIR_GAIN * self.ph.pair[t as usize][l] / kt).exp();
                        sum += w[l];
                    }
                    if sum <= 0.0 || self.rng.chance(self.p.stall) {
                        break;
                    }
                    let mut r = self.rng.f64() * sum;
                    let mut pick = 0;
                    for (l, &wl) in w.iter().enumerate() {
                        if r < wl {
                            pick = l;
                            break;
                        }
                        r -= wl;
                    }
                    let best = (0..N_LETTERS).max_by(|&a, &b| self.ph.pair[t as usize][a].total_cmp(&self.ph.pair[t as usize][b])).unwrap();
                    self.free[pick] -= 1;
                    copy.push(pick as u8);
                    if pick != best {
                        mutations += 1;
                        // a frayed end extends badly: stall odds grow with the pairing shortfall
                        let shortfall = DRY_PAIR_GAIN * (self.ph.pair[t as usize][best] - self.ph.pair[t as usize][pick]);
                        if self.rng.chance(1.0 - (-shortfall / kt).exp()) {
                            break;
                        }
                    }
                }
                self.stats.copied_letters += copy.len() as u64;
                if copy.len() >= 2 {
                    self.chains[ci].paired = true;
                    self.stats.copies += 1;
                    self.stats.mutations += mutations;
                    let (pid, gen_) = (self.chains[ci].id, self.chains[ci].generation);
                    self.spawn(copy, Some(pid), gen_ + 1);
                } else {
                    for l in copy {
                        self.free[l as usize] += 1;
                    }
                }
            }
        }

        // Ligation in the dry: monomer joins a chain end or a monomer (de novo dimer). Mass action on free counts.
        if !wet {
            let total_free: i64 = self.free.iter().sum();
            let conc = total_free as f64 / self.p.cells;
            let ends = 2.0 * self.chains.len() as f64 + total_free as f64;
            let expected = k_form * conc * ends * sweeps;
            let k = self.rng.poisson(expected);
            for _ in 0..k {
                let Some(l) = self.draw_free() else { break };
                let target = self.rng.below(self.chains.len() + total_free.max(1) as usize);
                if target < self.chains.len() && self.chains[target].seq.len() < self.p.max_len {
                    let c = self.chains.swap_remove(target);
                    let mut seq = c.seq.clone();
                    if self.rng.chance(0.5) {
                        seq.push(l);
                    } else {
                        seq.insert(0, l);
                    }
                    self.spawn(seq, Some(c.id), c.generation);
                } else if let Some(m) = self.draw_free() {
                    self.spawn(vec![l, m], None, 0);
                } else {
                    self.free[l as usize] += 1;
                    break;
                }
                self.stats.ligations += 1;
            }
        }

        // Chain-chain ligation in the dry: two ends condense with the same Ka-Ka rate.
        // Pairs of chains ~ n^2 / 2, each with 4 end-to-end orientations.
        if !wet && self.chains.len() >= 2 {
            let n = self.chains.len() as f64;
            let expected = k_form * 2.0 * n * (n - 1.0) / self.p.cells * sweeps;
            let k = self.rng.poisson(expected);
            for _ in 0..k {
                let len = self.chains.len();
                if len < 2 {
                    break;
                }
                let (i, j) = (self.rng.below(len), self.rng.below(len));
                if i == j || self.chains[i].seq.len() + self.chains[j].seq.len() > self.p.max_len {
                    continue;
                }
                let (hi, lo) = (i.max(j), i.min(j));
                let a = self.chains.swap_remove(hi);
                let b = self.chains.swap_remove(lo);
                let mut seq = a.seq.clone();
                seq.extend_from_slice(&b.seq);
                self.spawn(seq, Some(a.id), a.generation.max(b.generation));
                self.stats.ligations += 1;
            }
        }

        // Functions are temperature dependent: refresh a slice each step.
        if !self.chains.is_empty() {
            for _ in 0..(self.chains.len() / 20).max(1) {
                let ci = self.rng.below(self.chains.len());
                let seq = self.chains[ci].seq.clone();
                let (f, c, _) = self.model.get(&seq, kt, &self.ph, self.p.fold_below);
                self.chains[ci].func = f;
                self.chains[ci].conf = c;
            }
        }
        self.t_days += dt;
    }

    fn draw_free(&mut self) -> Option<u8> {
        let total: i64 = self.free.iter().sum();
        if total <= 0 {
            return None;
        }
        let mut r = self.rng.below(total as usize) as i64;
        for l in 0..N_LETTERS {
            if r < self.free[l] {
                self.free[l] -= 1;
                return Some(l as u8);
            }
            r -= self.free[l];
        }
        None
    }

    /// Most common sequences: the pool's "species".
    pub fn top_sequences(&self, n: usize) -> Vec<(String, usize)> {
        let mut m: HashMap<&[u8], usize> = HashMap::new();
        for c in &self.chains {
            *m.entry(&c.seq).or_default() += 1;
        }
        let mut v: Vec<(String, usize)> = m.into_iter().map(|(s, k)| (seq_string(s), k)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::CosmosParams;

    fn toy_rates() -> RateTable {
        let mut t = crate::kinetics::new_table();
        let ka = bidx(KA).unwrap();
        for kt in [0.9, 1.0, 1.1] {
            for wet in [0.0, 1.0] {
                let x = crate::kinetics::features(kt, wet, 0.3, 0.2);
                t.record_sample(&x);
                t.record(&x, CH_FORM + pair(ka, ka), 2.0, 100.0);
                t.record(&x, CH_BREAK + pair(ka, ka), if wet > 0.5 { 3.0 } else { 0.3 }, 10000.0);
            }
        }
        t
    }

    #[test]
    fn matter_closed_per_letter() {
        let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams::default()), toy_rates(), 3);
        let start = p.p.monomers;
        for _ in 0..400 {
            p.step(0.02);
            let r = p.residues();
            for l in 0..N_LETTERS {
                assert!(p.free[l] >= 0);
                assert_eq!(p.free[l] + r[l], start[l], "letter {l} leaked at t={}", p.t_days);
            }
        }
        assert!(p.stats.ligations > 0 && p.stats.copies > 0, "{:?}", p.stats);
    }

    #[test]
    fn copies_are_mostly_complementary() {
        let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams::default()), toy_rates(), 5);
        for _ in 0..400 {
            p.step(0.02);
        }
        let s = &p.stats;
        assert!(s.copied_letters > 100);
        let error = s.mutations as f64 / s.copied_letters as f64;
        assert!(error < 0.6, "copy error {error}");
    }
}
