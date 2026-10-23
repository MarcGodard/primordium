//! Scale 3: cells. A genome is one long chain in the same chemistry.
//!
//! Why a genetic code: a genome is copied twice (sense -> antisense -> sense) and
//! each letter is replaced by its best pairing partner. Under this chemistry's
//! pairing table only Li <-> Fl is an involution; Hy, Ox and Bo get rewritten
//! into Li/Fl within two copies. So the heritable alphabet is binary, and the
//! 5-letter working alphabet has to be reached through a code: 3-bit codons map
//! to product letters (two codons stop). Genes are reading-frame runs between stops.
//! Products are translated chains that fold (scale 1) into functions that run the cell:
//!   - polymerase: backbone (Ka-Ka) catalysis -> faster, more faithful copying
//!   - metabolism: Ka-letter catalysis -> monomers from raw atoms
//!   - amphiphile: membrane builders -> growth
//!   - pigment: shade against UV, which erodes membranes and mutates genomes
//! Division waits for a full genome copy: daughters inherit whole genomes with
//! copy errors. Radiation adds point mutations. That is heredity with a gate,
//! which scale 2 lacked.
//!
//! World: the beach as elevation patches. Each patch is a niche (wet/dry
//! rhythm, UV, depth) with its own raw-atom pool, fed and washed by tides from
//! an ocean reservoir. Matter closed over patches + cells + ocean, per letter and Ka.
//!
//! Detail = per-gene product kinetics. Surrogate = learned cell-level rates
//! (growth, replication speed, fidelity) over the genome's phenotype and the
//! patch environment; product counts freeze until the cell is lifted again.

use crate::copying::{COPY_CHANNELS, Copier, LearnedCopy, PairTally};
use crate::cosmos::{Cosmos, EnvSample};
use crate::hybrid::Mode;
use crate::kinetics::{CH_BREAK, bidx, pair};
use crate::polymer::{Function, LETTERS, N_LETTERS, Seq, seq_string};
use crate::pool::{Ctx, DRY_PAIR_GAIN, FIDELITY_PER_CUT, KA, PoolParams, Site};
use crate::rng::Rng;
use crate::surrogate::{Axis, Discovery, RateTable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Codon (3 bits, first letter most significant) -> product letter; None = stop.
/// Letters: 0 h, 1 l, 2 b, 3 o, 4 f.
pub const CODE: [Option<u8>; 8] = [Some(0), Some(1), Some(2), Some(3), Some(4), Some(1), None, None];
pub const MIN_GENE_CODONS: usize = 4;
pub const MAX_GENE_CODONS: usize = 40;
/// Product copies each founder inherits from its protocell ancestry.
const FOUNDER_COPIES: i64 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellParams {
    pub patches: usize,
    /// Elevation range the patches span (0 sea floor .. 1 dune).
    pub elev_lo: f64,
    pub elev_hi: f64,
    /// Patch volume in cells: sets raw-atom concentrations.
    pub patch_cells: f64,
    pub ocean_raw: f64,
    pub ocean_ka: f64,
    pub flush_rate: f64,
    pub washout_rate: f64,
    /// Transcripts per gene per day at full monomer supply, before polymerase boost.
    pub tx_rate: f64,
    /// Genome letters copied per day at full supply, before polymerase boost.
    pub rep_speed: f64,
    /// Product residues per amphiphile product inserted per day.
    pub insert_rate: f64,
    pub amph_min: f64,
    /// Abiotic membrane growth: residues per day per unit raw concentration.
    pub abiotic_membrane: f64,
    pub membrane_turnover: f64,
    pub uv_membrane: f64,
    /// Point mutations per genome letter per day at unit UV / cosmic.
    pub uv_mutation: f64,
    pub cosmic_mutation: f64,
    pub max_shade: f64,
    pub perm_raw: f64,
    pub vol_k: f64,
    pub burst_area: i64,
    /// Membrane size past which a cell with a finished genome copy divides regardless of birth size.
    pub divide_area: i64,
    pub max_cells: usize,
    /// Chance per day a cell drifts to a neighbouring patch (wet only).
    pub drift: f64,
    pub genome_max: usize,
    pub auto: bool,
    pub lift_below: f64,
    pub restrict_above: f64,
}

impl Default for CellParams {
    fn default() -> Self {
        Self {
            patches: 10,
            elev_lo: 0.30,
            elev_hi: 0.75,
            patch_cells: 4000.0,
            ocean_raw: 0.1,
            ocean_ka: 0.5,
            flush_rate: 1.5,
            washout_rate: 0.15,
            tx_rate: 10.0,
            rep_speed: 100.0,
            insert_rate: 2.0,
            amph_min: 0.3,
            abiotic_membrane: 6.0,
            membrane_turnover: 0.03,
            uv_membrane: 0.03,
            uv_mutation: 0.002,
            cosmic_mutation: 0.0004,
            max_shade: 0.8,
            // a generation needs ~300 monomers; at patch Ka 0.25/cell this imports ~100/day
            perm_raw: 15.0,
            vol_k: 1.2,
            burst_area: 10,
            divide_area: 80,
            max_cells: 500,
            drift: 0.3,
            genome_max: 160,
            auto: true,
            lift_below: 0.5,
            restrict_above: 0.85,
        }
    }
}

/// One gene: genome span and its folded product.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gene {
    pub start: usize,
    pub end: usize,
    /// Translated product of the gene's codons: what expression makes.
    pub product: Seq,
    pub func: Function,
}

/// Everything derived from a genome alone: genes and the phenotype they encode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenomeInfo {
    pub genes: Vec<Gene>,
    /// Phenotype if every gene's product is present: best cuts and max scores.
    pub poly_cut: f64,
    pub synth_cut: f64,
    pub amph: f64,
    pub pigment: f64,
    pub pending: bool,
}

/// Exact complement under scale-1 pairing: each letter's strongest partner.
pub fn complement_table(ctx: &Ctx) -> [u8; N_LETTERS] {
    let mut c = [0u8; N_LETTERS];
    for (t, slot) in c.iter_mut().enumerate() {
        *slot = (0..N_LETTERS).max_by(|&a, &b| ctx.ph.pair[t][a].total_cmp(&ctx.ph.pair[t][b])).unwrap() as u8;
    }
    c
}

/// Heritable bit of each letter: what it becomes after two rounds of copying (Li 0, Fl 1).
pub fn bit_table(comp: &[u8; N_LETTERS]) -> [u8; N_LETTERS] {
    let (l, f) = (1u8, 4u8);
    let mut b = [0u8; N_LETTERS];
    for x in 0..N_LETTERS {
        let twice = comp[comp[x] as usize];
        debug_assert!(twice == l || twice == f, "double copy lands in the binary alphabet");
        b[x] = (twice == f) as u8;
    }
    b
}

fn codon(genome: &[u8], at: usize, bits: &[u8; N_LETTERS]) -> usize {
    (bits[genome[at] as usize] as usize) << 2 | (bits[genome[at + 1] as usize] as usize) << 1 | bits[genome[at + 2] as usize] as usize
}

/// Genes: reading-frame runs of non-stop codons, MIN..=MAX codons long. Spans in letters.
pub fn gene_spans(genome: &[u8], bits: &[u8; N_LETTERS]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let codons = genome.len() / 3;
    let mut start = 0;
    for k in 0..=codons {
        let stop = k == codons || CODE[codon(genome, 3 * k, bits)].is_none();
        if stop {
            let n = k - start;
            if (MIN_GENE_CODONS..=MAX_GENE_CODONS).contains(&n) {
                out.push((3 * start, 3 * k));
            }
            start = k + 1;
        }
    }
    out
}

/// Translate a gene span codon by codon.
pub fn translate(gene: &[u8], bits: &[u8; N_LETTERS]) -> Seq {
    (0..gene.len() / 3).filter_map(|k| CODE[codon(gene, 3 * k, bits)]).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub id: u64,
    pub parent: Option<u64>,
    pub born: f64,
    pub generation: u32,
    pub genome: Seq,
    /// Copies of each gene's product present (detail mode).
    pub products: Vec<i64>,
    /// Residues of defective transcripts, by letter: degraded back to monomers.
    pub junk: [i64; N_LETTERS],
    pub membrane: [i64; N_LETTERS],
    pub birth_area: i64,
    pub free: [i64; N_LETTERS],
    pub raw: [i64; N_LETTERS],
    pub raw_ka: i64,
    /// Genome copy in progress (letters built so far).
    pub fork: Option<Seq>,
    pub patch: usize,
    pub mode: Mode,
    pub conf: f64,
    pub mutations: u32,
    /// Display position along the shore (0..1), viewer-only.
    pub y: f64,
}

impl Cell {
    pub fn area(&self) -> i64 {
        self.membrane.iter().sum()
    }

    pub fn cells(&self, vol_k: f64) -> f64 {
        vol_k * (self.area().max(1) as f64).powf(1.5)
    }

    /// Matter view: per letter, plus Ka (every residue and monomer carries one).
    pub fn matter(&self, info: &GenomeInfo) -> ([i64; N_LETTERS], i64) {
        let mut m = [0i64; N_LETTERS];
        let mut ka = self.raw_ka;
        let add = |l: u8, n: i64, m: &mut [i64; N_LETTERS], ka: &mut i64| {
            m[l as usize] += n;
            *ka += n;
        };
        for &l in &self.genome {
            add(l, 1, &mut m, &mut ka);
        }
        if let Some(f) = &self.fork {
            for &l in f {
                add(l, 1, &mut m, &mut ka);
            }
        }
        for (g, &n) in info.genes.iter().zip(&self.products) {
            for &l in &g.product {
                add(l, n, &mut m, &mut ka);
            }
        }
        for l in 0..N_LETTERS {
            m[l] += self.junk[l] + self.membrane[l] + self.free[l] + self.raw[l];
            ka += self.junk[l] + self.membrane[l] + self.free[l];
        }
        (m, ka)
    }
}

/// A patch of beach: raw atoms and loose monomers at one elevation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Patch {
    pub elevation: f64,
    pub raw: [i64; N_LETTERS],
    pub raw_ka: i64,
    pub free: [i64; N_LETTERS],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CellStats {
    pub births: u64,
    pub deaths: u64,
    pub divisions: u64,
    pub copy_mutations: u64,
    pub copied_letters: u64,
    pub radiation_mutations: u64,
    pub lifts: u64,
    pub restricts: u64,
    pub new_genomes: u64,
    /// Messenger letters copied in transcription (kept apart from replication's copied_letters).
    pub tx_letters: u64,
    /// Heritable errors: daughter genome bits that differ from the parent's (what selection sees).
    pub bit_errors: u64,
    pub bits_inherited: u64,
    pub tx_ok: u64,
    pub tx_junk: u64,
    pub tx_short: u64,
    pub products_decayed: u64,
    pub products_inserted: u64,
}

pub fn cell_table() -> RateTable {
    RateTable::new(
        // selectivity is pairing chemistry: temperature and the polymerase actually present
        vec![Axis::new("kT", 0.85, 1.15, 12), Axis::new("poly", 0.0, 1.0, 10)],
        COPY_CHANNELS,
    )
}

#[derive(Clone)]
pub struct World {
    pub ctx: Ctx,
    pub cp: CellParams,
    pub cosmos: Cosmos,
    pub env: EnvSample,
    pub t_days: f64,
    pub patches: Vec<Patch>,
    pub cells: Vec<Cell>,
    pub ocean: [i64; N_LETTERS],
    pub ocean_ka: i64,
    pub genomes: HashMap<Seq, Arc<GenomeInfo>>,
    pub comp: [u8; N_LETTERS],
    pub bits: [u8; N_LETTERS],
    pub stats: CellStats,
    pub model: RateTable,
    pub learn: bool,
    /// Best polymerase seen in a genome so far; beating it is a discovery.
    pub best_poly: f64,
    /// Re-deriving phenotypes after folds land: not new genomes.
    refreshing: bool,
    /// Pooled copy model per channel, refreshed each step: the prior for thin bins.
    sel_prior: Vec<f64>,
}

fn round_flux(expected: f64, rng: &mut Rng) -> i64 {
    let base = expected.trunc();
    let frac = expected - base;
    base as i64 + if rng.chance(frac.abs()) { frac.signum() as i64 } else { 0 }
}

fn pick_weighted(counts: &[i64; N_LETTERS], rng: &mut Rng) -> Option<usize> {
    let total: i64 = counts.iter().sum();
    if total <= 0 {
        return None;
    }
    let mut r = rng.below(total as usize) as i64;
    for (l, &c) in counts.iter().enumerate() {
        if r < c {
            return Some(l);
        }
        r -= c;
    }
    None
}

/// Copy speed with no polymerase expressed, relative to nominal.
pub const UNAIDED_COPY: f64 = 0.15;
/// Speed added per unit of (exp(cut/kT) - 1): ~0.45 kT of polymerase gives ~1.3x nominal.
pub const POLY_SPEEDUP: f64 = 2.0;

/// Fraction of mismatched letters a transcript tolerates and still works (neutral
/// network). At baseline fidelity (~25% error) about half pass, so a better
/// polymerase raises the yield of every gene: that is what makes it selectable.
pub const TX_TOLERANCE: f64 = 0.2;

/// Carry product copies across a genome change: a gene whose product sequence still
/// exists keeps its copies; the rest dissolve into `free`. Matter exact.
fn remap_products(old: &GenomeInfo, counts: &[i64], new: &GenomeInfo, free: &mut [i64; N_LETTERS]) -> Vec<i64> {
    let mut out = vec![0i64; new.genes.len()];
    for (g, &n) in old.genes.iter().zip(counts) {
        if n == 0 {
            continue;
        }
        if let Some(k) = new.genes.iter().position(|h| h.product == g.product) {
            out[k] += n;
        } else {
            for &l in &g.product {
                free[l as usize] += n;
            }
        }
    }
    out
}

/// Polymerase catalysis actually present: best expressed product, full strength from 3 copies.
fn present_poly(c: &Cell, info: &GenomeInfo) -> f64 {
    let ka = bidx(KA).unwrap();
    info.genes.iter().zip(&c.products).map(|(g, &n)| g.func.catalysis[pair(ka, ka)] * (n as f64 / 3.0).min(1.0)).fold(0.0, f64::max)
}

/// Swap mismatched letters of `got` for `want` via the monomer pool. All or nothing.
fn repair(got: &mut Seq, want: &[u8], free: &mut [i64; N_LETTERS]) -> bool {
    let mut need = [0i64; N_LETTERS];
    for (g, w) in got.iter().zip(want) {
        if g != w {
            need[*w as usize] += 1;
        }
    }
    if (0..N_LETTERS).any(|l| free[l] < need[l]) {
        return false;
    }
    for (g, w) in got.iter_mut().zip(want) {
        if g != w {
            free[*w as usize] -= 1;
            free[*g as usize] += 1;
            *g = *w;
        }
    }
    true
}

impl World {
    pub fn new(p: PoolParams, cp: CellParams, cosmos: Cosmos, rates: RateTable, seed: u64, founders: usize) -> Self {
        let ctx = Ctx::new(p, rates, seed);
        let comp = complement_table(&ctx);
        let bits = bit_table(&comp);
        let env = cosmos.sample(0.25);
        let patches: Vec<Patch> = (0..cp.patches)
            .map(|k| {
                let elevation = cp.elev_lo + (cp.elev_hi - cp.elev_lo) * k as f64 / (cp.patches - 1).max(1) as f64;
                let raw = [(cp.ocean_raw * cp.patch_cells) as i64; N_LETTERS];
                Patch { elevation, raw, raw_ka: (cp.ocean_ka * cp.patch_cells) as i64, free: [0; N_LETTERS] }
            })
            .collect();
        let ocean = [(cp.ocean_raw * cp.patch_cells * 400.0) as i64; N_LETTERS];
        let ocean_ka = (cp.ocean_ka * cp.patch_cells * 400.0) as i64;
        let mut w = Self {
            ctx,
            cp,
            cosmos,
            env,
            t_days: 0.25,
            patches,
            cells: Vec::new(),
            ocean,
            ocean_ka,
            genomes: HashMap::new(),
            comp,
            bits,
            stats: CellStats::default(),
            model: cell_table(),
            learn: true,
            best_poly: 0.0,
            refreshing: false,
            sel_prior: vec![0.0; COPY_CHANNELS],
        };
        w.seed_founders(founders);
        w
    }

    /// Founders: random genomes of 3-5 random genes with spacers. Their matter comes from the ocean.
    fn seed_founders(&mut self, n: usize) {
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        for _ in 0..n {
            // binary genome: Li = 0, Fl = 1. Genes of random sense codons, each closed by a stop.
            let letter_of = |b: usize| if b == 1 { 4u8 } else { 1u8 };
            let mut genome = Vec::new();
            for _ in 0..(3 + rng.below(3)) {
                for _ in 0..(6 + rng.below(10)) {
                    let mut cd = rng.below(8);
                    while CODE[cd].is_none() {
                        cd = rng.below(8);
                    }
                    genome.extend([letter_of(cd >> 2 & 1), letter_of(cd >> 1 & 1), letter_of(cd & 1)]);
                }
                genome.extend([letter_of(1), letter_of(1), letter_of(0)]); // 110: stop
            }
            let membrane = [6i64; N_LETTERS];
            let free = [20i64; N_LETTERS];
            // founders descend from protocells: they start holding a few copies of each product
            let mut need = [0i64; N_LETTERS];
            for (s0, e0) in gene_spans(&genome, &self.bits) {
                for l in translate(&genome[s0..e0], &self.bits) {
                    need[l as usize] += FOUNDER_COPIES;
                }
            }
            // take matter from the ocean so the ledger closes
            for l in 0..N_LETTERS {
                let n = need[l] + membrane[l] + free[l] + genome.iter().filter(|&&g| g as usize == l).count() as i64;
                self.ocean[l] -= n;
                self.ocean_ka -= n;
            }
            let patch = rng.below(self.cp.patches);
            let id = self.ctx.next_id();
            let n_genes = gene_spans(&genome, &self.bits).len();
            let cell = Cell {
                id,
                parent: None,
                born: self.t_days,
                generation: 0,
                genome,
                products: vec![FOUNDER_COPIES; n_genes],
                junk: [0; N_LETTERS],
                membrane,
                birth_area: 30,
                free,
                raw: [0; N_LETTERS],
                raw_ka: 0,
                fork: None,
                patch,
                mode: Mode::Detail,
                conf: 0.0,
                mutations: 0,
                y: rng.f64(),
            };
            self.cells.push(cell);
        }
        self.ctx.rng = rng;
        self.express_all();
    }

    /// Genome -> genes and phenotype. Product functions come from the scale-1
    /// function model; unseen products queue a batched fold (resolved at end of step).
    pub fn info(&mut self, genome: &[u8]) -> Arc<GenomeInfo> {
        if let Some(i) = self.genomes.get(genome) {
            if !i.pending {
                return i.clone();
            }
        }
        let kt = self.env.kt;
        let mut genes = Vec::new();
        let mut pending = false;
        for (s, e) in gene_spans(genome, &self.bits) {
            let product: Seq = translate(&genome[s..e], &self.bits);
            let (func, _, p) = self.ctx.function_for(&product, kt);
            pending |= p.is_some();
            genes.push(Gene { start: s, end: e, product, func });
        }
        let ka = bidx(KA).unwrap();
        let poly_cut = genes.iter().map(|g| g.func.catalysis[pair(ka, ka)]).fold(0.0, f64::max);
        let synth_cut =
            genes.iter().map(|g| LETTERS.iter().map(|&z| g.func.catalysis[pair(ka, bidx(z).unwrap())]).sum::<f64>() / N_LETTERS as f64).fold(0.0, f64::max);
        let amph = genes.iter().map(|g| g.func.amphiphile).fold(0.0, f64::max);
        let pigment = genes.iter().map(|g| g.func.pigment).fold(0.0, f64::max);
        let info = Arc::new(GenomeInfo { genes, poly_cut, synth_cut, amph, pigment, pending });
        if self.genomes.insert(genome.to_vec(), info.clone()).is_none() && !self.refreshing {
            self.stats.new_genomes += 1;
        }
        info
    }

    /// Make sure every cell's product vector matches its genome's gene count.
    fn express_all(&mut self) {
        let genomes: Vec<Seq> = self.cells.iter().map(|c| c.genome.clone()).collect();
        for (k, g) in genomes.iter().enumerate() {
            let info = self.info(g);
            let c = &mut self.cells[k];
            assert_eq!(c.products.len(), info.genes.len(), "products track the genome's genes");
        }
    }

    /// Total matter across patches, cells and ocean. Must never change.
    pub fn matter(&mut self) -> ([i64; N_LETTERS], i64) {
        let mut m = self.ocean;
        let mut ka = self.ocean_ka;
        for p in &self.patches {
            for l in 0..N_LETTERS {
                m[l] += p.raw[l] + p.free[l];
            }
            ka += p.raw_ka + p.free.iter().sum::<i64>();
        }
        let genomes: Vec<Seq> = self.cells.iter().map(|c| c.genome.clone()).collect();
        for (k, g) in genomes.iter().enumerate() {
            let info = self.info(g);
            let (cm, cka) = self.cells[k].matter(&info);
            for l in 0..N_LETTERS {
                m[l] += cm[l];
            }
            ka += cka;
        }
        (m, ka)
    }

    pub fn patch_site(&self, k: usize, shade: f64) -> Site {
        Site::at(self.env, self.patches[k].elevation, shade)
    }

    fn features(c: &Cell, info: &GenomeInfo, kt: f64) -> [f64; 2] {
        [kt, present_poly(c, info)]
    }

    pub fn step(&mut self, dt: f64) {
        self.env = self.cosmos.sample(self.t_days);
        self.sel_prior = LearnedCopy::priors(&self.model, 0);
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        self.tides(dt, &mut rng);

        let mut cells = std::mem::take(&mut self.cells);
        let mut born = Vec::new();
        let mut dead = Vec::new();
        for (ci, c) in cells.iter_mut().enumerate() {
            let info = self.info(&c.genome);
            debug_assert_eq!(c.products.len(), info.genes.len());
            let shade = self.cp.max_shade * info.pigment * self.expressed_fraction(c, &info, |g| g.func.pigment > 0.0);
            let site = self.patch_site(c.patch, shade);
            let x = Self::features(c, &info, site.env.kt);
            c.conf = self.model.confidence(&x);
            self.resolve_mode(c, &x, &site);
            // shared membrane physics (permeation, turnover, abiotic assembly) runs in both modes;
            // the surrogate learns only what the detailed interior adds on top of it
            self.exchange(c, &site, dt, &mut rng);
            let mut tally = PairTally::default();
            self.interior_step(c, &info, &site, &x, dt, &mut tally, &mut rng);
            self.radiation(c, &site, dt, &mut rng);
            if c.mode == Mode::Detail && self.learn {
                self.model.record_sample(&x);
                tally.record(&mut self.model, &x, 0);
            }
            if c.area() < self.cp.burst_area {
                dead.push(ci);
                continue;
            }
            // division: genome copied and membrane doubled
            if c.fork.as_ref().is_some_and(|f| f.len() == c.genome.len()) && c.area() >= (2 * c.birth_area).min(self.cp.divide_area) {
                born.push(ci);
            }
            if site.wet && rng.chance((self.cp.drift * dt).min(1.0)) {
                let np = if rng.chance(0.5) { c.patch.saturating_sub(1) } else { (c.patch + 1).min(self.cp.patches - 1) };
                c.patch = np;
            }
        }
        // divide (indices stable: we only append)
        let mut daughters = Vec::new();
        for &ci in &born {
            if let Some(d) = self.divide(&mut cells[ci], &mut rng) {
                daughters.push(d);
            }
        }
        cells.extend(daughters);
        dead.sort_unstable();
        for &ci in dead.iter().rev() {
            let c = cells.swap_remove(ci);
            self.spill(c);
            self.stats.deaths += 1;
        }
        while cells.len() > self.cp.max_cells {
            let k = rng.below(cells.len());
            let c = cells.swap_remove(k);
            self.spill(c);
            self.stats.deaths += 1;
        }
        self.cells = cells;
        self.ctx.rng = rng;
        self.ctx.resolve(self.t_days);
        self.refresh_pending();
        self.t_days += dt;
    }

    /// Re-derive phenotypes whose product folds just landed; log record polymerases.
    fn refresh_pending(&mut self) {
        let stale: Vec<Seq> = self.genomes.iter().filter(|(_, i)| i.pending).map(|(g, _)| g.clone()).collect();
        self.refreshing = true;
        for g in stale {
            self.genomes.remove(&g);
            let info = self.info(&g);
            if !info.pending && info.poly_cut > self.best_poly + 0.05 {
                self.best_poly = info.poly_cut;
                let n = self.ctx.discoveries.entries.len();
                self.ctx.discoveries.log(
                    2_000_000 + n,
                    Discovery {
                        t_days: self.t_days,
                        scale: 3,
                        region: 0,
                        confidence: 0.0,
                        features: vec![info.poly_cut, g.len() as f64],
                        note: format!("genome with polymerase {:.2} kT ({} genes, {} letters)", info.poly_cut, info.genes.len(), g.len()),
                    },
                );
            }
        }
        self.refreshing = false;
        if self.genomes.len() > 50_000 {
            // drop genomes no living cell carries (cells are back in place here, unlike mid-step)
            let alive: std::collections::HashSet<&Seq> = self.cells.iter().map(|c| &c.genome).collect();
            self.genomes.retain(|g, _| alive.contains(g));
        }
    }

    /// Fraction of the cell's expressed product copies whose gene satisfies `f`.
    fn expressed_fraction(&self, c: &Cell, info: &GenomeInfo, f: impl Fn(&Gene) -> bool) -> f64 {
        let total: i64 = c.products.iter().sum();
        if total == 0 {
            return 0.0;
        }
        info.genes.iter().zip(&c.products).filter(|(g, _)| f(g)).map(|(_, &n)| n).sum::<i64>() as f64 / total as f64
    }

    fn resolve_mode(&mut self, c: &mut Cell, x: &[f64], site: &Site) {
        match c.mode {
            Mode::Surrogate if c.conf < self.cp.lift_below => {
                c.mode = Mode::Detail;
                self.stats.lifts += 1;
                let (bin, _) = self.model.bin(x);
                self.ctx.discoveries.log(
                    3_000_000 + bin,
                    Discovery {
                        t_days: site.t_days,
                        scale: 3,
                        region: c.patch,
                        confidence: c.conf,
                        features: x.to_vec(),
                        note: format!("cell copying in untrained state (kT {:.3}, polymerase {:.2})", x[0], x[1]),
                    },
                );
            }
            Mode::Detail if self.cp.auto && c.conf >= self.cp.restrict_above => {
                c.mode = Mode::Surrogate;
                self.stats.restricts += 1;
            }
            _ => {}
        }
    }

    /// Raw atoms permeate; membranes turn over and erode under UV into the patch.
    fn exchange(&mut self, c: &mut Cell, site: &Site, dt: f64, rng: &mut Rng) {
        let p = &mut self.patches[c.patch];
        let area = c.area() as f64;
        let cin = c.cells(self.cp.vol_k);
        let cout = self.cp.patch_cells;
        let perm = self.cp.perm_raw;
        let flow = |inside: &mut i64, out: &mut i64, rng: &mut Rng| {
            let n = round_flux(perm * area * (*out as f64 / cout - *inside as f64 / cin) * dt, rng).clamp(-*inside, *out);
            *inside += n;
            *out -= n;
        };
        for l in 0..N_LETTERS {
            flow(&mut c.raw[l], &mut p.raw[l], rng);
        }
        flow(&mut c.raw_ka, &mut p.raw_ka, rng);
        let shade = site.x[3];
        let lose = round_flux((self.cp.membrane_turnover + self.cp.uv_membrane * shade) * area * dt, rng).clamp(0, c.area());
        for _ in 0..lose {
            let Some(l) = pick_weighted(&c.membrane, rng) else {
                break;
            };
            c.membrane[l] -= 1;
            p.free[l] += 1;
        }
        // membrane assembly from monomers the cell's metabolism made: growth runs on its own chemistry
        let mono_conc = c.free.iter().sum::<i64>() as f64 / cin;
        let grow = round_flux(self.cp.abiotic_membrane * mono_conc * dt * area.sqrt(), rng).max(0);
        for _ in 0..grow {
            let Some(l) = pick_weighted(&c.free, rng) else {
                break;
            };
            c.free[l] -= 1;
            c.membrane[l] += 1;
        }
    }

    fn learned_copy(&self, x: &[f64]) -> LearnedCopy {
        LearnedCopy::at(&self.model, x, &self.sel_prior, 0)
    }

    /// Templated copy of `template` (reverse complement) using the cell's monomers.
    /// Stops when a needed monomer is missing. Returns (copy, errors); detail copies add to `tally`.
    fn copy_strand(&self, template: &[u8], free: &mut [i64; N_LETTERS], how: Copier, max_letters: usize, tally: &mut PairTally, rng: &mut Rng) -> (Seq, u32) {
        let mut out = Vec::new();
        let mut errors = 0;
        for &t in template.iter().rev().take(max_letters) {
            let right = self.comp[t as usize] as usize;
            // kinetic proofreading: with the right partner absent a polymerase stalls and
            // waits rather than misincorporate; the copy resumes when supply returns
            if free[right] <= 0 {
                break;
            }
            let Some((pick, _)) = how.pick(t as usize, right, free, tally, rng) else { break };
            free[pick] -= 1;
            if pick != right {
                errors += 1;
            }
            out.push(pick as u8);
        }
        (out, errors)
    }

    /// Raw Ka + side atom -> monomer, catalysed by expressed synthesis products. Runs in both
    /// modes: it is cheap, and its per-letter output decides which partner a polymerase lacks.
    fn metabolism(&mut self, c: &mut Cell, info: &GenomeInfo, site: &Site, dt: f64, rng: &mut Rng) {
        let kt = site.env.kt;
        let x = site.x;
        let cells_in = c.cells(self.cp.vol_k);
        let presence = |c: &Cell, k: usize| (c.products[k] as f64 / 3.0).min(1.0);
        let ka = bidx(KA).unwrap();
        for (l, &z) in LETTERS.iter().enumerate() {
            let p = pair(ka, bidx(z).unwrap());
            let cut = info.genes.iter().enumerate().map(|(k, g)| g.func.catalysis[p] * presence(c, k)).fold(0.0, f64::max);
            let rate = self.ctx.ka_rate(crate::kinetics::CH_FORM, z, &x) * (cut / kt).exp();
            let make = round_flux(rate * (c.raw_ka * c.raw[l]) as f64 / cells_in * dt * self.ctx.p.sweeps_per_day, rng).clamp(0, c.raw_ka.min(c.raw[l]));
            c.raw_ka -= make;
            c.raw[l] -= make;
            c.free[l] += make;
        }
    }

    /// The cell's interior. Detail copies messengers and genome letter by letter with Boltzmann
    /// pairing; surrogate mode keeps every count-level process (metabolism, expression, decay,
    /// insertion, fork progress, stalls) and draws only copy fidelity from the learned table.
    fn interior_step(&mut self, c: &mut Cell, info: &GenomeInfo, site: &Site, x_feat: &[f64], dt: f64, tally: &mut PairTally, rng: &mut Rng) {
        let learned = c.mode == Mode::Surrogate;
        let model = learned.then(|| self.learned_copy(x_feat));
        let kt = site.env.kt;
        let x = site.x;
        let cells_in = c.cells(self.cp.vol_k);
        let poly = present_poly(c, info);

        self.metabolism(c, info, site, dt, rng);

        let supply = (c.free.iter().sum::<i64>() as f64 / cells_in / 0.3).min(1.0);
        let gain = DRY_PAIR_GAIN * (1.0 + FIDELITY_PER_CUT * poly);
        // copying a long template unaided is slow; a polymerase pocket makes it practical
        let boost = UNAIDED_COPY + POLY_SPEEDUP * ((poly / kt).exp() - 1.0);
        let how = match &model {
            Some(m) => Copier::Learned(m),
            None => Copier::Pairing { gain, kt, pair: &self.ctx.ph.pair, stall_on_mismatch: false },
        };

        // transcription: each gene makes product copies; any copy error makes junk
        for (k, g) in info.genes.iter().enumerate() {
            let n = rng.poisson(self.cp.tx_rate * supply * boost * dt);
            for _ in 0..n {
                // messenger: antisense copy of the gene, read back in sense order, then recycled
                let template: Seq = c.genome[g.start..g.end].to_vec();
                let (msg, _) = self.copy_strand(&template, &mut c.free, how, usize::MAX, tally, rng);
                // separate counter: messenger letters must not dilute the replication error rate
                self.stats.tx_letters += msg.len() as u64;
                for &l in &msg {
                    c.free[l as usize] += 1;
                }
                if msg.len() < template.len() {
                    self.stats.tx_short += 1;
                    continue;
                }
                // the antisense messenger's bits are the complement's bits; decode in sense order
                let sense: Seq = msg.iter().rev().map(|&a| self.comp[a as usize]).collect();
                let mut prod = translate(&sense, &self.bits);
                // product letters come from the monomer pool
                let mut have = c.free;
                let affordable = prod.iter().all(|&l| {
                    have[l as usize] -= 1;
                    have[l as usize] >= 0
                });
                if !affordable {
                    self.stats.tx_short += 1;
                    continue;
                }
                for &l in &prod {
                    c.free[l as usize] -= 1;
                }
                let errs = prod.iter().zip(&g.product).filter(|(a, b)| a != b).count();
                // most substitutions are neutral: a near-copy works if repair can swap the
                // wrong letters for right ones through the monomer pool (matter stays exact)
                let tolerable = prod.len() == g.product.len() && errs as f64 <= TX_TOLERANCE * g.product.len() as f64;
                if tolerable && repair(&mut prod, &g.product, &mut c.free) {
                    c.products[k] += 1;
                    self.stats.tx_ok += 1;
                } else {
                    self.stats.tx_junk += 1;
                    for &l in &prod {
                        c.junk[l as usize] += 1;
                    }
                }
            }
        }

        // product and junk decay at the learned backbone hydrolysis rate
        let k_break = self.ctx.ka_rate(CH_BREAK, KA, &x) * self.ctx.p.sweeps_per_day * dt;
        for (k, g) in info.genes.iter().enumerate() {
            let p = 1.0 - (-k_break * g.product.len() as f64 * (1.0 - 0.7 * g.func.stability)).exp();
            let lost = (0..c.products[k]).filter(|_| rng.chance(p)).count() as i64;
            c.products[k] -= lost;
            self.stats.products_decayed += lost as u64;
            for &l in &g.product {
                c.free[l as usize] += lost;
            }
        }
        for l in 0..N_LETTERS {
            let lost = round_flux(c.junk[l] as f64 * (k_break * 3.0).min(1.0), rng).clamp(0, c.junk[l]);
            c.junk[l] -= lost;
            c.free[l] += lost;
        }

        // membrane growth: amphiphile products insert whole
        let p_ins = (self.cp.insert_rate * dt).min(1.0);
        for (k, g) in info.genes.iter().enumerate() {
            if g.func.amphiphile < self.cp.amph_min {
                continue;
            }
            let ins = (0..c.products[k]).filter(|_| rng.chance(p_ins)).count() as i64;
            c.products[k] -= ins;
            self.stats.products_inserted += ins as u64;
            for &l in &g.product {
                c.membrane[l as usize] += ins;
            }
        }

        // genome replication: start once the membrane has grown, then copy at polymerase speed
        if c.fork.is_none() && c.area() as f64 >= (1.5 * c.birth_area as f64).min(0.75 * self.cp.divide_area as f64) {
            c.fork = Some(Vec::new());
        }
        if let Some(mut fork) = c.fork.take() {
            if fork.len() < c.genome.len() {
                let letters = round_flux(self.cp.rep_speed * supply * boost * dt, rng).max(0) as usize;
                // copy the next chunk: the fork reads the genome from its 3' end
                let done = fork.len();
                let remaining = &c.genome[..c.genome.len() - done];
                let (chunk, errs) = self.copy_strand(remaining, &mut c.free, how, letters, tally, rng);
                self.stats.copied_letters += chunk.len() as u64;
                self.stats.copy_mutations += errs as u64;
                fork.extend(chunk);
            }
            c.fork = Some(fork);
        }
    }

    /// UV (shaded by pigment, blocked by water) and cosmic rays mutate the genome in place.
    fn radiation(&mut self, c: &mut Cell, site: &Site, dt: f64, rng: &mut Rng) {
        let rate = self.cp.uv_mutation * site.x[3] + self.cp.cosmic_mutation * site.env.cosmic;
        let n = rng.poisson(rate * c.genome.len() as f64 * dt);
        if n == 0 {
            return;
        }
        let old_info = self.info(&c.genome);
        for _ in 0..n {
            let at = rng.below(c.genome.len());
            let old = c.genome[at] as usize;
            let new = (old + 1 + rng.below(N_LETTERS - 1)) % N_LETTERS;
            // matter: swap one letter for another through the cell's monomer pool
            if c.free[new] == 0 {
                continue;
            }
            c.free[new] -= 1;
            c.free[old] += 1;
            c.genome[at] = new as u8;
            c.mutations += 1;
            self.stats.radiation_mutations += 1;
        }
        let info = self.info(&c.genome);
        c.products = remap_products(&old_info, &c.products, &info, &mut c.free);
    }

    /// Second-strand synthesis then fission. The fork holds the antisense strand; the
    /// daughter's sense strand is a real templated copy of it at the cell's fidelity.
    /// If monomers run short the copy is undone and division waits (None).
    fn divide(&mut self, c: &mut Cell, rng: &mut Rng) -> Option<Cell> {
        let fork = c.fork.take().unwrap();
        let info = self.info(&c.genome);
        let poly = present_poly(c, &info);
        let model = (c.mode == Mode::Surrogate).then(|| self.learned_copy(&Self::features(c, &info, self.env.kt)));
        let how = match &model {
            Some(m) => Copier::Learned(m),
            None => {
                Copier::Pairing { gain: DRY_PAIR_GAIN * (1.0 + FIDELITY_PER_CUT * poly), kt: self.env.kt, pair: &self.ctx.ph.pair, stall_on_mismatch: false }
            }
        };
        let (mut genome, errs) = self.copy_strand(&fork, &mut c.free, how, usize::MAX, &mut PairTally::default(), rng);
        if genome.len() < fork.len() {
            for &l in &genome {
                c.free[l as usize] += 1;
            }
            c.fork = Some(fork);
            return None;
        }
        // antisense strand is recycled into monomers
        for &l in &fork {
            c.free[l as usize] += 1;
        }
        self.stats.copied_letters += genome.len() as u64;
        self.stats.copy_mutations += errs as u64;
        let same = genome.len().min(c.genome.len());
        self.stats.bits_inherited += same as u64;
        self.stats.bit_errors += (0..same).filter(|&i| self.bits[genome[i] as usize] != self.bits[c.genome[i] as usize]).count() as u64;
        if genome.len() > self.cp.genome_max {
            for &l in &genome[self.cp.genome_max..] {
                c.free[l as usize] += 1;
            }
            genome.truncate(self.cp.genome_max);
        }
        let split = |n: i64, rng: &mut Rng| (0..n).filter(|_| rng.chance(0.5)).count() as i64;
        let mut d_membrane = [0i64; N_LETTERS];
        let mut d_free = [0i64; N_LETTERS];
        let mut d_raw = [0i64; N_LETTERS];
        let mut d_junk = [0i64; N_LETTERS];
        for l in 0..N_LETTERS {
            d_membrane[l] = split(c.membrane[l], rng);
            c.membrane[l] -= d_membrane[l];
            d_free[l] = split(c.free[l], rng);
            c.free[l] -= d_free[l];
            d_raw[l] = split(c.raw[l], rng);
            c.raw[l] -= d_raw[l];
            d_junk[l] = split(c.junk[l], rng);
            c.junk[l] -= d_junk[l];
        }
        let d_ka = split(c.raw_ka, rng);
        c.raw_ka -= d_ka;
        // products split; the daughter keeps those its own genome still encodes
        let mut share = vec![0i64; info.genes.len()];
        for (k, n) in c.products.iter_mut().enumerate() {
            share[k] = split(*n, rng);
            *n -= share[k];
        }
        c.birth_area = c.area().max(1);
        c.generation += 1;
        self.stats.divisions += 1;
        self.stats.births += 1;
        let dinfo = self.info(&genome);
        let d_products = remap_products(&info, &share, &dinfo, &mut d_free);
        let id = self.ctx.next_id();
        Some(Cell {
            id,
            parent: Some(c.id),
            born: self.t_days,
            generation: c.generation,
            products: d_products,
            genome,
            junk: d_junk,
            membrane: d_membrane,
            birth_area: d_membrane.iter().sum::<i64>().max(1),
            free: d_free,
            raw: d_raw,
            raw_ka: d_ka,
            fork: None,
            patch: c.patch,
            mode: c.mode,
            conf: c.conf,
            mutations: 0,
            y: (c.y + rng.f64() * 0.04 - 0.02).rem_euclid(1.0),
        })
    }

    fn spill(&mut self, c: Cell) {
        let info = self.info(&c.genome);
        let p = &mut self.patches[c.patch];
        for &l in &c.genome {
            p.free[l as usize] += 1;
        }
        if let Some(f) = &c.fork {
            for &l in f {
                p.free[l as usize] += 1;
            }
        }
        for (g, &n) in info.genes.iter().zip(&c.products) {
            for &l in &g.product {
                p.free[l as usize] += n;
            }
        }
        for l in 0..N_LETTERS {
            p.free[l] += c.junk[l] + c.membrane[l] + c.free[l];
            p.raw[l] += c.raw[l];
        }
        p.raw_ka += c.raw_ka;
    }

    /// High water mixes each flooded patch with the ocean; monomers wash out and fall apart.
    fn tides(&mut self, dt: f64, rng: &mut Rng) {
        let k = (self.cp.flush_rate * dt).min(1.0);
        let w = (self.cp.washout_rate * dt).min(1.0);
        let cells = self.cp.patch_cells;
        for p in self.patches.iter_mut() {
            if self.env.tide_level <= p.elevation {
                continue;
            }
            for l in 0..N_LETTERS {
                let n = round_flux((self.cp.ocean_raw * cells - p.raw[l] as f64) * k, rng).clamp(-p.raw[l], self.ocean[l]);
                p.raw[l] += n;
                self.ocean[l] -= n;
                let out = round_flux(p.free[l] as f64 * w, rng).clamp(0, p.free[l]);
                p.free[l] -= out;
                self.ocean[l] += out;
                self.ocean_ka += out;
            }
            let n = round_flux((self.cp.ocean_ka * cells - p.raw_ka as f64) * k, rng).clamp(-p.raw_ka, self.ocean_ka);
            p.raw_ka += n;
            self.ocean_ka -= n;
            // loose monomers on the floor fall apart into raw atoms over time (wet)
            for l in 0..N_LETTERS {
                let n = round_flux(p.free[l] as f64 * 0.2 * dt, rng).clamp(0, p.free[l]);
                p.free[l] -= n;
                p.raw[l] += n;
                p.raw_ka += n;
            }
        }
    }

    /// Fresh dice, same state: shadow mode's detail twin.
    pub fn reseed(&mut self, seed: u64) {
        self.ctx.reseed(seed);
    }

    /// Population means of the phenotype, for watching evolution.
    pub fn phenotype_means(&mut self) -> [f64; 6] {
        let genomes: Vec<Seq> = self.cells.iter().map(|c| c.genome.clone()).collect();
        let n = genomes.len().max(1) as f64;
        let mut m = [0.0; 6];
        for g in &genomes {
            let i = self.info(g);
            m[0] += i.poly_cut;
            m[1] += i.synth_cut;
            m[2] += i.amph;
            m[3] += i.pigment;
            m[4] += g.len() as f64;
            m[5] += i.genes.len() as f64;
        }
        m.map(|v| v / n)
    }

    pub fn genome_string(g: &[u8]) -> String {
        seq_string(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::CosmosParams;

    fn rates() -> RateTable {
        serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/rates0.json")).unwrap()).unwrap()
    }

    #[test]
    fn code_is_heritable_and_genes_parse() {
        let ctx = Ctx::new(PoolParams::default(), crate::kinetics::new_table(), 1);
        let comp = complement_table(&ctx);
        let bits = bit_table(&comp);
        // Li and Fl are each other's partner: the only involution, so they carry the bits
        assert_eq!((comp[1], comp[4]), (4, 1));
        assert_eq!((bits[1], bits[4]), (0, 1));
        // 4 sense codons (001 l, 100 f, 011 o, 000 h) then stop 110, then 2 codons (too short)
        let g = crate::polymer::parse_seq(concat!("llf", "fll", "lff", "lll", "ffl", "lfl", "fll"));
        assert_eq!(gene_spans(&g, &bits), vec![(0, 12)]);
        assert_eq!(translate(&g[0..12], &bits), crate::polymer::parse_seq("lfoh"));
    }

    #[test]
    fn world_matter_closed() {
        let mut w = World::new(PoolParams::default(), CellParams { auto: false, ..Default::default() }, Cosmos::new(CosmosParams::default()), rates(), 5, 60);
        let start = w.matter();
        for k in 0..600 {
            w.step(0.02);
            if k % 50 == 0 {
                assert_eq!(w.matter(), start, "t={}", w.t_days);
            }
        }
        assert_eq!(w.matter(), start);
        assert!(w.stats.divisions > 0, "{:?}", w.stats);
    }

    #[test]
    fn surrogate_cells_keep_matter() {
        let mut w = World::new(PoolParams::default(), CellParams::default(), Cosmos::new(CosmosParams::default()), rates(), 6, 60);
        let start = w.matter();
        let mut sur = 0;
        for _ in 0..800 {
            w.step(0.02);
            sur += w.cells.iter().filter(|c| c.mode == Mode::Surrogate).count();
        }
        assert_eq!(w.matter(), start);
        assert!(sur > 0, "surrogate never engaged");
    }

    #[test]
    fn learned_copying_matches_detail_error_rate() {
        let cp = CellParams { auto: false, ..Default::default() };
        let mut w = World::new(PoolParams::default(), cp, Cosmos::new(CosmosParams::default()), rates(), 7, 120);
        for _ in 0..1500 {
            w.step(0.02);
        }
        let err_rate = |w: &mut World| {
            let (e0, l0) = (w.stats.copy_mutations, w.stats.copied_letters);
            for _ in 0..1000 {
                w.step(0.02);
            }
            (w.stats.copy_mutations - e0) as f64 / (w.stats.copied_letters - l0).max(1) as f64
        };
        let mut sur = w.clone();
        sur.reseed(99);
        sur.learn = false;
        // out-of-range moments lift cells; auto puts them back, as in the shadow run
        sur.cp.auto = true;
        for c in sur.cells.iter_mut() {
            c.mode = Mode::Surrogate;
        }
        let (d, s) = (err_rate(&mut w), err_rate(&mut sur));
        let n_sur = sur.cells.iter().filter(|c| c.mode == Mode::Surrogate).count();
        assert!(n_sur * 10 >= sur.cells.len() * 9, "trained cells stayed in detail: {n_sur}/{}", sur.cells.len());
        assert!(d > 0.01 && (s - d).abs() < 0.2 * d, "detail error {d:.4} vs learned {s:.4}");
    }
}
