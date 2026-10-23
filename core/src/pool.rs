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
use crate::element::Table;
use crate::kinetics::{CH_BREAK, CH_FORM, bidx, pair};
use crate::polymer::{Function, LETTERS, LetterPhysics, N_LETTERS, Seq, express, seq_string};
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
    /// Side-group atoms and Ka atoms not yet joined into monomers.
    pub raw: [i64; N_LETTERS],
    pub raw_ka: i64,
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
            raw: [600, 600, 400, 400, 600],
            raw_ka: 2600,
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
    /// Waiting for its batched fold (kT bin it was born at); function is a placeholder until then.
    #[serde(default)]
    pub pending: Option<i32>,
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

const NEIGHBOUR_WINDOW: usize = 256;

/// kT bins of 0.05: fold and stability shift with temperature.
pub fn kt_bin(kt: f64) -> i32 {
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
        self.insert(seq, kb, f.clone());
        f
    }

    pub fn cached(&self, seq: &[u8], kb: i32) -> Option<&Function> {
        self.cache.get(&(seq.to_vec(), kb))
    }

    fn insert(&mut self, seq: &[u8], kb: i32, f: Function) {
        self.cache.insert((seq.to_vec(), kb), f);
        // neighbour search scans only recent folds per bin: relatives of a new
        // mutant are recent, and an unbounded scan made lookups grow with history
        let recent = self.by_len.entry((seq.len(), kb)).or_default();
        if recent.len() >= NEIGHBOUR_WINDOW {
            recent.remove(0);
        }
        recent.push(seq.to_vec());
        self.folds += 1;
    }

    /// Surrogate answer if confident enough, else None (caller queues a real fold).
    pub fn lookup(&mut self, seq: &[u8], kt: f64, fold_below: f64) -> Option<(Function, f64)> {
        match self.predict(seq, kt) {
            Some((f, c)) if c >= fold_below => {
                if c >= 1.0 {
                    self.exact += 1;
                } else {
                    self.nearest += 1;
                }
                Some((f, c))
            }
            _ => None,
        }
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
    pub synthesized: u64,
    pub decayed: u64,
    /// Chain-chain ligations (subset of `ligations`): their own mass action, n^2.
    #[serde(default)]
    pub joins: u64,
}

impl PoolStats {
    pub fn add(&mut self, o: &PoolStats) {
        self.copies += o.copies;
        self.mutations += o.mutations;
        self.copied_letters += o.copied_letters;
        self.breaks += o.breaks;
        self.ligations += o.ligations;
        self.melts += o.melts;
        self.synthesized += o.synthesized;
        self.decayed += o.decayed;
        self.joins += o.joins;
    }
}

pub const KA: u8 = 5;
/// Weak pairing in the dry is this much stronger: no water competing for the bond.
pub const DRY_PAIR_GAIN: f64 = 2.0;
/// Extra pairing discrimination per kT of backbone catalysis: a polymerase pocket
/// pins the incoming monomer against the template (escape from Eigen's error threshold).
pub const FIDELITY_PER_CUT: f64 = 8.0;

/// Rules and caches shared by every body of water (pond, vesicle interiors).
#[derive(Clone)]
pub struct Ctx {
    pub p: PoolParams,
    pub rates: RateTable,
    pub ph: LetterPhysics,
    pub table: Table,
    pub model: FunctionModel,
    pub discoveries: DiscoveryLog,
    /// Best Ka-Ka barrier cut seen so far; beating it is a discovery.
    pub best_cut: f64,
    pub rng: Rng,
    next_id: u64,
    /// Sequences awaiting a real fold this step, deduplicated.
    pending: std::collections::HashSet<(Seq, i32)>,
}

impl Ctx {
    /// Function for a new chain: surrogate if confident, else queue a fold and return a placeholder.
    pub fn function_for(&mut self, seq: &[u8], kt: f64) -> (Function, f64, Option<i32>) {
        if let Some((f, c)) = self.model.lookup(seq, kt, self.p.fold_below) {
            return (f, c, None);
        }
        let kb = kt_bin(kt);
        self.pending.insert((seq.to_vec(), kb));
        (Function { catalysis: vec![0.0; crate::kinetics::N_PAIRS], ..Default::default() }, 0.0, Some(kb))
    }

    /// Fold every queued sequence in parallel (pure function), memoize, log record catalysts.
    pub fn resolve(&mut self, t_days: f64) {
        use rayon::prelude::*;
        if self.pending.is_empty() {
            return;
        }
        let jobs: Vec<(Seq, i32)> = self.pending.drain().collect();
        let ph = &self.ph;
        let done: Vec<(Seq, i32, Function)> = jobs.into_par_iter().map(|(s, kb)| {
            let (_, f) = express(&s, ph, kb as f64 * 0.05, 1);
            (s, kb, f)
        }).collect();
        let ka = bidx(KA).unwrap();
        for (s, kb, f) in done {
            let cut = f.catalysis.get(pair(ka, ka)).copied().unwrap_or(0.0);
            if cut > self.best_cut + 0.05 {
                self.best_cut = cut;
                let n = self.discoveries.entries.len();
                self.discoveries.log(
                    n,
                    Discovery { t_days, scale: 1, region: 0, confidence: 0.0, features: vec![cut, s.len() as f64], note: format!("new best backbone catalyst {} cuts {:.2} kT", seq_string(&s), cut) },
                );
            }
            self.model.insert(&s, kb, f);
        }
    }

    pub fn new(p: PoolParams, rates: RateTable, seed: u64) -> Self {
        let table = Table::new();
        let ph = LetterPhysics::new(&table);
        Self { p, rates, ph, table, model: FunctionModel::default(), discoveries: DiscoveryLog::default(), best_cut: 0.0, rng: Rng::new(seed), next_id: 1, pending: Default::default() }
    }

    /// Fresh dice, same state: shadow mode's detail twin.
    pub fn reseed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id - 1
    }

    /// Learned scale-0 rate for a bond channel (CH_FORM or CH_BREAK base) between Ka and `z`.
    pub fn ka_rate(&self, ch_base: usize, z: u8, x: &[f64]) -> f64 {
        let ch = ch_base + pair(bidx(KA).unwrap(), bidx(z).unwrap());
        self.rates.rate(x, ch, self.rates.global_rate(ch), 1.0)
    }
}

/// Where a body of water sits and what it feels this step.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub env: EnvSample,
    pub wet: bool,
    /// Scale-0 features (kT, wet, light, uv) the rate table was trained on.
    pub x: [f64; 4],
    pub t_days: f64,
}

impl Site {
    pub fn at(env: EnvSample, elevation: f64, shade: f64) -> Self {
        let depth = (env.tide_level - elevation).max(0.0);
        let wet = env.tide_level > elevation;
        let x = crate::kinetics::features(env.kt, if wet { 1.0 } else { 0.0 }, env.visible * (-1.5 * depth).exp() * (1.0 - shade), env.uv * (-8.0 * depth).exp() * (1.0 - shade));
        Self { env, wet, x, t_days: env.t_days }
    }
}

/// One body of water: chains, free monomers, and raw atoms monomers are made from.
/// Matter per letter: raw + free + residues. Ka: raw_ka + all free + all residues.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Contents {
    pub chains: Vec<Chain>,
    pub free: [i64; N_LETTERS],
    /// Side-group atoms not yet on a Ka.
    pub raw: [i64; N_LETTERS],
    pub raw_ka: i64,
    /// Volume, lattice cells.
    pub cells: f64,
    pub stats: PoolStats,
}

/// Monomer concentration at which copying runs at its nominal rate.
const REF_MONOMER_CONC: f64 = 0.3;

impl Contents {
    pub fn residues(&self) -> [i64; N_LETTERS] {
        let mut r = [0; N_LETTERS];
        for c in &self.chains {
            for &l in &c.seq {
                r[l as usize] += 1;
            }
        }
        r
    }

    /// Per-letter totals including raw atoms; plus the Ka total. Matter-ledger view.
    pub fn matter(&self) -> ([i64; N_LETTERS], i64) {
        let r = self.residues();
        let mut m = [0; N_LETTERS];
        for l in 0..N_LETTERS {
            m[l] = self.raw[l] + self.free[l] + r[l];
        }
        (m, self.raw_ka + self.free.iter().sum::<i64>() + r.iter().sum::<i64>())
    }

    /// Catalysis of scale-0 pair `p` by this body's chains, kT barrier cut.
    /// Saturates at the best single site; dilution by volume.
    pub fn cut(&self, p: usize) -> f64 {
        let mut total = 0.0;
        let mut best: f64 = 0.0;
        for c in &self.chains {
            let v = c.func.catalysis.get(p).copied().unwrap_or(0.0);
            total += v;
            best = best.max(v);
        }
        best.min(total * 200.0 / self.cells.max(1.0))
    }

    pub fn backbone_cut(&self) -> f64 {
        let ka = bidx(KA).unwrap();
        self.cut(pair(ka, ka))
    }

    /// Mean catalysis over Ka-letter (monomer synthesis) pairs.
    pub fn synthesis_cut(&self) -> f64 {
        let ka = bidx(KA).unwrap();
        LETTERS.iter().map(|&z| self.cut(pair(ka, bidx(z).unwrap()))).sum::<f64>() / N_LETTERS as f64
    }

    /// `paired`: born annealed to a partner strand (a fresh copy, or a piece of a duplex).
    pub fn spawn(&mut self, ctx: &mut Ctx, site: &Site, seq: Seq, parent: Option<u64>, generation: u32, paired: bool) {
        let (func, conf, pending) = ctx.function_for(&seq, site.env.kt);
        let id = ctx.next_id();
        self.chains.push(Chain { id, seq, parent, born: site.t_days, paired, func, conf, generation, pending });
    }

    /// Fill placeholders from the cache after Ctx::resolve.
    pub fn apply_folds(&mut self, ctx: &Ctx) {
        for c in self.chains.iter_mut() {
            if let Some(kb) = c.pending {
                if let Some(f) = ctx.model.cached(&c.seq, kb) {
                    c.func = f.clone();
                    c.conf = 1.0;
                    c.pending = None;
                }
            }
        }
    }

    fn draw_free(&mut self, rng: &mut Rng) -> Option<u8> {
        let total: i64 = self.free.iter().sum();
        if total <= 0 {
            return None;
        }
        let mut r = rng.below(total as usize) as i64;
        for l in 0..N_LETTERS {
            if r < self.free[l] {
                self.free[l] -= 1;
                return Some(l as u8);
            }
            r -= self.free[l];
        }
        None
    }

    /// Advance `dt` days. Order: metabolism, hydrolysis, melting, copying, ligation.
    pub fn step(&mut self, ctx: &mut Ctx, site: &Site, dt: f64) {
        let mut rng = std::mem::replace(&mut ctx.rng, Rng::new(0));
        self.step_inner(ctx, site, dt, &mut rng);
        ctx.rng = rng;
    }

    fn step_inner(&mut self, ctx: &mut Ctx, site: &Site, dt: f64, rng: &mut Rng) {
        let kt = site.env.kt;
        let sweeps = dt * ctx.p.sweeps_per_day;
        let cells = self.cells.max(1.0);
        let boost = (self.backbone_cut() / kt).exp();
        let x = site.x;
        // A pocket binds small reactants; it can trim a chain end (the exact reverse of
        // adding a monomer) but cannot swallow a mid-chain bond. Internal bonds stay uncatalysed.
        let k_break_inner = ctx.ka_rate(CH_BREAK, KA, &x);
        let k_break_end = k_break_inner * boost;
        let k_form = ctx.ka_rate(CH_FORM, KA, &x) * boost;
        let wet = site.wet;

        // Metabolism: Ka + side atom <-> monomer, at learned scale-0 rates for that pair.
        // A catalyst of the pair speeds both ways (it cannot move the equilibrium).
        let ka = bidx(KA).unwrap();
        for (l, &z) in LETTERS.iter().enumerate() {
            let cat = (self.cut(pair(ka, bidx(z).unwrap())) / kt).exp();
            let make = rng.poisson(ctx.ka_rate(CH_FORM, z, &x) * cat * (self.raw_ka * self.raw[l]) as f64 / cells * sweeps) as i64;
            let make = make.min(self.raw_ka).min(self.raw[l]);
            self.raw_ka -= make;
            self.raw[l] -= make;
            self.free[l] += make;
            let lose = (rng.poisson(ctx.ka_rate(CH_BREAK, z, &x) * cat * self.free[l] as f64 * sweeps) as i64).min(self.free[l]);
            self.free[l] -= lose;
            self.raw[l] += lose;
            self.raw_ka += lose;
            self.stats.synthesized += make as u64;
            self.stats.decayed += lose as u64;
        }

        // Hydrolysis / photolysis: per backbone bond, folded bonds partly shielded.
        // Pieces join the next step's population, so each chain is exposed once per step.
        for c in std::mem::take(&mut self.chains) {
            let shield = 1.0 - ctx.p.fold_protect * c.func.stability;
            let p_inner = 1.0 - (-k_break_inner * shield * sweeps).exp();
            let p_end = 1.0 - (-k_break_end * shield * sweeps).exp();
            let bonds = c.seq.len().saturating_sub(1);
            let mut cuts: Vec<usize> = (1..=bonds).filter(|&b| rng.chance(if b == 1 || b == bonds { p_end } else { p_inner })).collect();
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
                    self.spawn(ctx, site, piece, Some(c.id), c.generation, c.paired);
                }
            }
        }

        // Melting: duplex strands separate in water with Boltzmann odds on mean pair energy.
        if wet {
            for c in self.chains.iter_mut().filter(|c| c.paired) {
                let mean_pair: f64 = c.seq.iter().map(|&l| ctx.ph.pair[l as usize].iter().cloned().fold(0.0, f64::max)).sum::<f64>() / c.seq.len() as f64;
                // cooperative: a few-residue core must open at once
                let p_melt = (-(mean_pair * 3.0) / kt).exp() * 20.0 * dt;
                if rng.chance(p_melt.min(1.0)) {
                    c.paired = false;
                    self.stats.melts += 1;
                }
            }
        }

        // Templated copying in the dry: needs unpaired template and monomers.
        if !wet {
            let total_free: i64 = self.free.iter().sum();
            let frac = (total_free as f64 / cells / REF_MONOMER_CONC).min(1.0);
            let gain = DRY_PAIR_GAIN * (1.0 + FIDELITY_PER_CUT * self.backbone_cut());
            let n = self.chains.len();
            for ci in 0..n {
                if self.chains[ci].paired || !rng.chance((ctx.p.copy_rate * frac * boost * dt).min(1.0)) {
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
                        w[l] = self.free[l].max(0) as f64 * (gain * ctx.ph.pair[t as usize][l] / kt).exp();
                        sum += w[l];
                    }
                    if sum <= 0.0 || rng.chance(ctx.p.stall) {
                        break;
                    }
                    let mut r = rng.f64() * sum;
                    // fallback: last available letter, in case rounding walks r past the end
                    let mut pick = (0..N_LETTERS).rev().find(|&l| w[l] > 0.0).unwrap();
                    for (l, &wl) in w.iter().enumerate() {
                        if r < wl {
                            pick = l;
                            break;
                        }
                        r -= wl;
                    }
                    let best = (0..N_LETTERS).max_by(|&a, &b| ctx.ph.pair[t as usize][a].total_cmp(&ctx.ph.pair[t as usize][b])).unwrap();
                    self.free[pick] -= 1;
                    copy.push(pick as u8);
                    if pick != best {
                        mutations += 1;
                        // a frayed end extends badly: stall odds grow with the pairing shortfall
                        let shortfall = gain * (ctx.ph.pair[t as usize][best] - ctx.ph.pair[t as usize][pick]);
                        if rng.chance(1.0 - (-shortfall / kt).exp()) {
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
                    // the copy is the other strand of the same duplex: both must melt first
                    self.spawn(ctx, site, copy, Some(pid), gen_ + 1, true);
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
            let conc = total_free as f64 / cells;
            let ends = 2.0 * self.chains.len() as f64 + total_free as f64;
            let k = rng.poisson(k_form * conc * ends * sweeps);
            for _ in 0..k {
                let free_now: i64 = self.free.iter().sum();
                let Some(l) = self.draw_free(rng) else { break };
                let target = rng.below(self.chains.len() + free_now.max(1) as usize);
                if target < self.chains.len() && self.chains[target].seq.len() >= ctx.p.max_len {
                    // full-length chain cannot take another letter: no event
                    self.free[l as usize] += 1;
                    continue;
                }
                if target < self.chains.len() {
                    let c = self.chains.swap_remove(target);
                    let mut seq = c.seq.clone();
                    if rng.chance(0.5) {
                        seq.push(l);
                    } else {
                        seq.insert(0, l);
                    }
                    self.spawn(ctx, site, seq, Some(c.id), c.generation, c.paired);
                } else if let Some(m) = self.draw_free(rng) {
                    self.spawn(ctx, site, vec![l, m], None, 0, false);
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
            let k = rng.poisson(k_form * 2.0 * n * (n - 1.0) / cells * sweeps);
            for _ in 0..k {
                let len = self.chains.len();
                if len < 2 {
                    break;
                }
                let (i, j) = (rng.below(len), rng.below(len));
                if i == j || self.chains[i].seq.len() + self.chains[j].seq.len() > ctx.p.max_len {
                    continue;
                }
                let (hi, lo) = (i.max(j), i.min(j));
                let a = self.chains.swap_remove(hi);
                let b = self.chains.swap_remove(lo);
                let mut seq = a.seq.clone();
                seq.extend_from_slice(&b.seq);
                self.spawn(ctx, site, seq, Some(a.id), a.generation.max(b.generation), a.paired || b.paired);
                self.stats.ligations += 1;
                self.stats.joins += 1;
            }
        }

        // Functions are temperature dependent: refresh a slice each step.
        if !self.chains.is_empty() {
            for _ in 0..(self.chains.len() / 20).max(1) {
                let ci = rng.below(self.chains.len());
                if self.chains[ci].pending.is_some() {
                    continue;
                }
                let seq = self.chains[ci].seq.clone();
                let (f, c, pending) = ctx.function_for(&seq, kt);
                if pending.is_none() {
                    self.chains[ci].func = f;
                    self.chains[ci].conf = c;
                } else {
                    // keep the old function until the new temperature's fold lands
                    self.chains[ci].pending = pending;
                }
            }
        }
    }

    /// Most common sequences: the "species".
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

/// A single open tide pool: one Contents at a fixed beach elevation.
pub struct Pool {
    pub ctx: Ctx,
    pub c: Contents,
    pub cosmos: Cosmos,
    pub env: EnvSample,
    pub t_days: f64,
}

impl Pool {
    pub fn new(p: PoolParams, cosmos: Cosmos, rates: RateTable, seed: u64) -> Self {
        let env = cosmos.sample(0.25);
        let c = Contents { free: p.monomers, raw: p.raw, raw_ka: p.raw_ka, cells: p.cells, ..Default::default() };
        Self { ctx: Ctx::new(p, rates, seed), c, cosmos, env, t_days: 0.25 }
    }

    pub fn site(&self) -> Site {
        Site::at(self.env, self.ctx.p.elevation, 0.0)
    }

    pub fn wet(&self) -> bool {
        self.env.tide_level > self.ctx.p.elevation
    }

    pub fn step(&mut self, dt: f64) {
        self.env = self.cosmos.sample(self.t_days);
        let site = self.site();
        self.c.step(&mut self.ctx, &site, dt);
        self.ctx.resolve(self.t_days);
        self.c.apply_folds(&self.ctx);
        self.t_days += dt;
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
                for z in LETTERS {
                    let kz = pair(ka, bidx(z).unwrap());
                    t.record(&x, CH_FORM + kz, 1.0, 100.0);
                    t.record(&x, CH_BREAK + kz, 0.1, 10000.0);
                }
            }
        }
        t
    }

    #[test]
    fn matter_closed_per_letter() {
        let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams::default()), toy_rates(), 3);
        let start = p.c.matter();
        for _ in 0..400 {
            p.step(0.02);
            for l in 0..N_LETTERS {
                assert!(p.c.free[l] >= 0 && p.c.raw[l] >= 0);
            }
            assert_eq!(p.c.matter(), start, "matter leaked at t={}", p.t_days);
        }
        let s = &p.c.stats;
        assert!(s.ligations > 0 && s.copies > 0 && s.synthesized > 0 && s.decayed > 0, "{s:?}");
    }

    #[test]
    fn copying_waits_for_melting() {
        // pin the pool dry: without a wet phase nothing melts, so each template copies once
        let mut p = Pool::new(PoolParams { elevation: 2.0, ..Default::default() }, Cosmos::new(CosmosParams::default()), toy_rates(), 4);
        for _ in 0..300 {
            p.step(0.02);
        }
        assert_eq!(p.c.stats.melts, 0);
        assert!(p.c.stats.copies > 0);
        // copies are born paired, so a copy never templates before a melt: no generation 2
        let max_gen = p.c.chains.iter().map(|c| c.generation).max().unwrap_or(0);
        assert!(max_gen <= 1, "copy of a copy without melting: generation {max_gen}");
    }

    #[test]
    fn copies_are_mostly_complementary() {
        let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams::default()), toy_rates(), 5);
        for _ in 0..400 {
            p.step(0.02);
        }
        let s = &p.c.stats;
        assert!(s.copied_letters > 100);
        let error = s.mutations as f64 / s.copied_letters as f64;
        assert!(error < 0.6, "copy error {error}");
    }
}
