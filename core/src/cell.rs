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
use crate::polymer::{self, Function, LETTERS, N_LETTERS, Seq, seq_string};
use crate::pool::{Ctx, DRY_PAIR_GAIN, FIDELITY_PER_CUT, KA, PoolParams, Site};
use crate::rng::Rng;
use crate::surrogate::{Axis, Discovery, RateTable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Letters per codon: the fewest binary digits that name every working letter plus a stop.
pub const CODON: usize = codon_len(N_LETTERS);
/// Codons in the table: `1 << CODON`.
pub const CODONS: usize = 1 << CODON;

const fn codon_len(n: usize) -> usize {
    let mut w = 1;
    while (1usize << w) < n + 1 {
        w += 1;
    }
    w
}

/// The genetic code this table's chemistry implies, derived once from the elements.
#[derive(Clone, Debug)]
pub struct GeneticCode {
    /// The heritable letters by bit: what any letter becomes after enough double copies.
    pub canon: [u8; 2],
    /// Bit-0 letter: the synonym codons code for it, so products need it most and it runs short
    /// first (Li in the [2, 6] world).
    pub food: u8,
    /// Codon (CODON bits, first letter most significant) -> product letter; None = stop.
    pub table: [Option<u8>; CODONS],
}

impl GeneticCode {
    /// Codons 0..N name the letters in order, the last quarter (at least one) are stops, any
    /// left between are synonyms, from the food letter on. For [2, 6]: [h, l, b, o, f, l, -, -].
    pub fn derive(comp: &[u8; N_LETTERS]) -> Self {
        let canon = canonical(comp);
        let food = canon[0];
        // every letter keeps a codon: stops give way when the table is nearly full
        let stops = (CODONS / 4).max(1).min(CODONS - N_LETTERS);
        let mut table = [None; CODONS];
        for (k, slot) in table.iter_mut().enumerate().take(CODONS - stops) {
            *slot = Some(if k < N_LETTERS { k as u8 } else { ((food as usize + k - N_LETTERS) % N_LETTERS) as u8 });
        }
        Self { canon, food, table }
    }
}

/// Heritable letters: iterate double copying to its fixed set. Copying pairs each letter with
/// its strongest partner; scales 1+ need exactly two survivors (a binary genome).
fn canonical(comp: &[u8; N_LETTERS]) -> [u8; 2] {
    let mut alive: Vec<u8> = (0..N_LETTERS as u8).collect();
    loop {
        let mut next: Vec<u8> = alive.iter().map(|&x| comp[comp[x as usize] as usize]).collect();
        next.sort_unstable();
        next.dedup();
        if next == alive {
            break;
        }
        alive = next;
    }
    assert!(alive.len() == 2, "heritable alphabet of {} letters: scales 1+ read binary genomes", alive.len());
    [alive[0], alive[1]]
}

/// This world's genetic code (its table of elements decides it).
pub fn genetic_code() -> &'static GeneticCode {
    static CODE: std::sync::OnceLock<GeneticCode> = std::sync::OnceLock::new();
    CODE.get_or_init(|| GeneticCode::derive(&complement_of(&polymer::LetterPhysics::new(&crate::element::Table::new()))))
}
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
    /// Ocean strength per letter, x `ocean_raw`: the world recipe's element abundances (a scarce
    /// element is a thin letter in every sea above it). 1 = the default table's sea.
    #[serde(with = "crate::arrays")]
    pub sea: [f64; N_LETTERS],
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
    /// Furthest a drifting cell moves along the shore per drift (y is 0..1, wraps).
    pub drift_along: f64,
    pub genome_max: usize,
    /// Adhesion below `stick_floor` never holds a daughter; at floor + span it always does.
    pub stick_floor: f64,
    pub stick_span: f64,
    /// Colony size past which a random member breaks away.
    pub colony_max: usize,
    /// Loose monomers flowing toward the colony average, fraction per day.
    pub share_rate: f64,
    /// Engulfing: adhesion past `eat_floor` lets a cell grab a smaller one on its patch, at up
    /// to `eat_rate` attempts per day (full at floor + span) scaled by patch crowding.
    pub eat_floor: f64,
    pub eat_span: f64,
    pub eat_rate: f64,
    /// Prey must be smaller than this fraction of the predator's membrane.
    pub eat_size: f64,
    /// Each colony mate makes a member this much harder to pull away (per mate, exp).
    pub colony_shield: f64,
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
            sea: [1.0; N_LETTERS],
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
            drift_along: 0.15,
            genome_max: 160,
            stick_floor: 0.2,
            stick_span: 0.2,
            colony_max: 16,
            share_rate: 2.0,
            eat_floor: 0.25,
            eat_span: 0.15,
            eat_rate: 2.0,
            eat_size: 0.6,
            colony_shield: 0.5,
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
    /// Read from the bits just upstream of the gene's leading stop codon.
    #[serde(default)]
    pub promoter: Promoter,
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
    /// Best stickiness among the genes' products.
    #[serde(default)]
    pub adhesion: f64,
    pub pending: bool,
}

/// Exact complement under scale-1 pairing: each letter's strongest partner.
pub fn complement_table(ctx: &Ctx) -> [u8; N_LETTERS] {
    complement_of(&ctx.ph)
}

fn complement_of(ph: &polymer::LetterPhysics) -> [u8; N_LETTERS] {
    let mut c = [0u8; N_LETTERS];
    for (t, slot) in c.iter_mut().enumerate() {
        *slot = (0..N_LETTERS).max_by(|&a, &b| ph.pair[t][a].total_cmp(&ph.pair[t][b])).unwrap() as u8;
    }
    c
}

/// Heritable bit of each letter: what it becomes after two rounds of copying (in [2, 6]: Li 0,
/// Fl 1).
pub fn bit_table(comp: &[u8; N_LETTERS]) -> [u8; N_LETTERS] {
    let canon = canonical(comp);
    let mut b = [0u8; N_LETTERS];
    for x in 0..N_LETTERS {
        let twice = comp[comp[x] as usize];
        debug_assert!(canon.contains(&twice), "double copy lands in the binary alphabet");
        b[x] = (twice == canon[1]) as u8;
    }
    b
}

fn codon(genome: &[u8], at: usize, bits: &[u8; N_LETTERS]) -> usize {
    (0..CODON).fold(0, |c, i| c << 1 | bits[genome[at + i] as usize] as usize)
}

/// Genes: reading-frame runs of non-stop codons, MIN..=MAX codons long. Spans in letters.
pub fn gene_spans(genome: &[u8], bits: &[u8; N_LETTERS]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let code = &genetic_code().table;
    let codons = genome.len() / CODON;
    let mut start = 0;
    for k in 0..=codons {
        let stop = k == codons || code[codon(genome, CODON * k, bits)].is_none();
        if stop {
            let n = k - start;
            if (MIN_GENE_CODONS..=MAX_GENE_CODONS).contains(&n) {
                out.push((CODON * start, CODON * k));
            }
            start = k + 1;
        }
    }
    out
}

/// What a promoter listens to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    /// Constitutive: always on.
    #[default]
    None,
    /// The light reaching the cell (after water and shade).
    Light,
    /// Cold: temperature below the comfortable range.
    Cold,
    /// The cell's own loose Li running short.
    LiScarce,
    /// How many colony mates the cell has (0 alone, 1 at the colony size cap).
    Mates,
    /// Height in a body (scale 5; 0 at the holdfast, 1 at `BODY_HEIGHT` cells up). A free cell sits at 0.
    Height,
    /// Faces open to water (scale 5; 1 on the skin, 0 deep inside). A free cell is all skin.
    Exposure,
}

/// Gene regulation: a signal, a sign (on when the signal is high, or low) and a gain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Promoter {
    pub signal: Signal,
    pub on_high: bool,
    pub gain: f64,
}

/// Body height at which the height signal saturates, in cells.
pub const BODY_HEIGHT: f64 = 8.0;

/// Signal strengths a promoter reads: light, cold, Li scarcity, mates, height, exposure.
pub type Signals = [f64; 6];

/// Bits before a gene's preceding stop codon: [signal x3, sign, gain]. Genes too close to the
/// genome start have no room for a promoter and stay constitutive. Promoter bits may overlap
/// the previous gene's last codon, so rewiring one can recode the other.
pub const PROMOTER_BITS: usize = 5;

pub fn promoter(genome: &[u8], start: usize, bits: &[u8; N_LETTERS]) -> Promoter {
    if start < CODON + PROMOTER_BITS {
        return Promoter::default();
    }
    let b = |k: usize| bits[genome[start - CODON - PROMOTER_BITS + k] as usize];
    // code 7 is unassigned and reads as constitutive
    let signal = match (b(0), b(1), b(2)) {
        (0, 0, 1) => Signal::Light,
        (0, 1, 0) => Signal::Cold,
        (0, 1, 1) => Signal::LiScarce,
        (1, 0, 0) => Signal::Mates,
        (1, 0, 1) => Signal::Height,
        (1, 1, 0) => Signal::Exposure,
        _ => Signal::None,
    };
    Promoter { signal, on_high: b(3) == 1, gain: if b(4) == 1 { 10.0 } else { 4.0 } }
}

impl Promoter {
    /// Expression level 0..1 at signal strengths (`Signals`), each 0..1.
    pub fn level(&self, sig: Signals) -> f64 {
        let s = match self.signal {
            Signal::None => return 1.0,
            Signal::Light => sig[0],
            Signal::Cold => sig[1],
            Signal::LiScarce => sig[2],
            Signal::Mates => sig[3],
            Signal::Height => sig[4],
            Signal::Exposure => sig[5],
        };
        let x = if self.on_high { s - 0.5 } else { 0.5 - s };
        1.0 / (1.0 + (-self.gain * x).exp())
    }
}

/// Signal strengths 0..1 a cell's promoters read: light reaching it, cold, its own Li running
/// short (Li's share of its loose monomers), and its colony mates (`Cell::mates` over the
/// cap's `colony_max - 1`). A beach cell has no body: height 0, all skin.
pub fn regulation_signals(c: &Cell, light: f64, kt: f64, colony_max: usize) -> Signals {
    signals(light, kt, &c.free, c.mates as f64 / colony_max.saturating_sub(1).max(1) as f64, 0.0, 1.0)
}

/// Signals from their raw inputs (`free`: the loose monomers whose Li share is sensed).
pub fn signals(light: f64, kt: f64, free: &[i64; N_LETTERS], mates: f64, height: f64, exposure: f64) -> Signals {
    let free_total: i64 = free.iter().sum();
    let li_share = if free_total > 0 { free[genetic_code().food as usize] as f64 / free_total as f64 } else { 0.0 };
    [
        light.clamp(0.0, 1.0),
        ((1.05 - kt) / 0.2).clamp(0.0, 1.0),
        (1.0 - li_share / 0.3).clamp(0.0, 1.0),
        mates.clamp(0.0, 1.0),
        height.clamp(0.0, 1.0),
        exposure.clamp(0.0, 1.0),
    ]
}

/// Traits as the genes are switched under `signals`: (polymerase, metabolism, membrane), each
/// the best gene's strength times its expression level. Genotype potential alone hides what
/// regulation does to fitness.
pub fn regulated_traits(info: &GenomeInfo, signals: Signals) -> (f64, f64, f64) {
    let ka = bidx(KA).unwrap();
    let (mut poly, mut synth, mut amph) = (0.0f64, 0.0f64, 0.0f64);
    for g in &info.genes {
        let on = g.promoter.level(signals);
        poly = poly.max(g.func.catalysis[pair(ka, ka)] * on);
        let s = LETTERS.iter().map(|&z| g.func.catalysis[pair(ka, bidx(z).unwrap())]).sum::<f64>() / N_LETTERS as f64;
        synth = synth.max(s * on);
        amph = amph.max(g.func.amphiphile * on);
    }
    (poly, synth, amph)
}

/// Translate a gene span codon by codon.
pub fn translate(gene: &[u8], bits: &[u8; N_LETTERS]) -> Seq {
    let code = &genetic_code().table;
    (0..gene.len() / CODON).filter_map(|k| code[codon(gene, CODON * k, bits)]).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub id: u64,
    pub parent: Option<u64>,
    /// Day the current cell cycle began (birth, or the parent's last division).
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
    /// Lineage mode: due to divide, waiting for its patch to afford a daughter.
    #[serde(default)]
    pub ready: bool,
    /// Lineage mode: the genome as it was when this cycle began. A detailed cell copies its
    /// genome during the cycle, so radiation damage the parent takes afterwards is not
    /// inherited; lineage daughters copy this snapshot to keep that. Information, not matter.
    #[serde(default)]
    pub template: Option<Seq>,
    /// Colony this cell is stuck in (daughters of sticky cells stay attached).
    #[serde(default)]
    pub colony: Option<u64>,
    /// Colony mates at the last colony update (a regulation signal).
    #[serde(default)]
    pub mates: u32,
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
    /// Heritable errors on parent 1-bits, and parent 1-bits inherited: the spectrum's bias.
    #[serde(default)]
    pub bit_errors_on1: u64,
    #[serde(default)]
    pub ones_inherited: u64,
    /// Heritable errors and bits inherited by the source letter copied (radiation leaves
    /// non-canonical letters; they pair weakly and are copied worse).
    #[serde(default)]
    pub errors_by_letter: [u64; N_LETTERS],
    #[serde(default)]
    pub bits_by_letter: [u64; N_LETTERS],
    pub tx_ok: u64,
    pub tx_junk: u64,
    pub tx_short: u64,
    pub products_decayed: u64,
    pub products_inserted: u64,
    #[serde(default)]
    pub colony_breakaways: u64,
    #[serde(default)]
    pub shared_monomers: u64,
    #[serde(default)]
    pub predations: u64,
}

pub fn cell_table() -> RateTable {
    let mut t = RateTable::new(
        // selectivity is pairing chemistry: temperature and the polymerase actually present
        vec![Axis::new("kT", 0.85, 1.15, 12), Axis::new("poly", 0.0, 1.0, 10)],
        COPY_CHANNELS,
    );
    // a cell-step copies only a few letters; mismatches per (template, partner) are rare, so a
    // bin needs many steps before its odds are worth trusting
    t.samples_for_confidence = 400.0;
    t
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
    /// Today's noon light (season and latitude, plus that noon's cloud): growth integrates light
    /// over a cycle, while instantaneous light swings with day and night.
    pub daylight: f64,
    /// Set while the world steps as lineages (`step_lineage`): expression then follows the
    /// genes, not frozen product stocks.
    pub lineage_mode: bool,
    /// Engulfings for display: (day, patch, predator y, prey y, predator id), last ~200.
    pub meals: std::collections::VecDeque<(f64, usize, f64, f64, u64)>,
    /// When set, detailed steps log each cell's lineage features and fate (scale-4 training).
    pub lineage_log: Option<Vec<crate::lineage::LineageSample>>,
}

/// Tidal mixing of a flooded patch with the ocean: raw atoms relax toward the ocean's
/// concentration (`raw`, `ka`: the patch's share at ocean strength), loose monomers wash out.
pub(crate) struct Tide {
    pub flush: f64,
    pub washout: f64,
    /// Raw atoms per letter the tide pulls each patch toward.
    pub raw: [f64; N_LETTERS],
    pub ka: f64,
}

impl Tide {
    pub(crate) fn mix(&self, p: &mut Patch, ocean: &mut [i64; N_LETTERS], ocean_ka: &mut i64, dt: f64, rng: &mut Rng) {
        let k = (self.flush * dt).min(1.0);
        let w = (self.washout * dt).min(1.0);
        for l in 0..N_LETTERS {
            let n = round_flux((self.raw[l] - p.raw[l] as f64) * k, rng).clamp(-p.raw[l], ocean[l]);
            p.raw[l] += n;
            ocean[l] -= n;
            let out = round_flux(p.free[l] as f64 * w, rng).clamp(0, p.free[l]);
            p.free[l] -= out;
            ocean[l] += out;
            *ocean_ka += out;
        }
        let n = round_flux((self.ka - p.raw_ka as f64) * k, rng).clamp(-p.raw_ka, *ocean_ka);
        p.raw_ka += n;
        *ocean_ka -= n;
        // loose monomers on the floor fall apart into raw atoms over time (wet)
        for l in 0..N_LETTERS {
            let n = round_flux(p.free[l] as f64 * 0.2 * dt, rng).clamp(0, p.free[l]);
            p.free[l] -= n;
            p.raw[l] += n;
            p.raw_ka += n;
        }
    }
}

pub(crate) fn round_flux(expected: f64, rng: &mut Rng) -> i64 {
    let base = expected.trunc();
    let frac = expected - base;
    base as i64 + if rng.chance(frac.abs()) { frac.signum() as i64 } else { 0 }
}

pub(crate) fn pick_weighted(counts: &[i64; N_LETTERS], rng: &mut Rng) -> Option<usize> {
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
pub(crate) fn remap_products(old: &GenomeInfo, counts: &[i64], new: &GenomeInfo, free: &mut [i64; N_LETTERS]) -> Vec<i64> {
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

/// Chance a frayed end is clipped before it is extended: clipping is the polymerase pocket's
/// reverse reaction, sped by the same cut (e^(cut/kT) - 1 over e^(cut/kT)).
fn clip_odds(poly: f64, kt: f64) -> f64 {
    1.0 - (-poly / kt).exp()
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
                let raw = std::array::from_fn(|l| (cp.ocean_raw * cp.sea[l] * cp.patch_cells) as i64);
                Patch { elevation, raw, raw_ka: (cp.ocean_ka * cp.patch_cells) as i64, free: [0; N_LETTERS] }
            })
            .collect();
        let ocean = std::array::from_fn(|l| (cp.ocean_raw * cp.sea[l] * cp.patch_cells * 400.0) as i64);
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
            lineage_log: None,
            lineage_mode: false,
            meals: Default::default(),
            daylight: 0.0,
        };
        w.seed_founders(founders);
        w
    }

    /// Founders: random genomes of 3-5 random genes with spacers. Their matter comes from the ocean.
    fn seed_founders(&mut self, n: usize) {
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        for _ in 0..n {
            // binary genome in the heritable letters. Genes of random sense codons, each closed by
            // the first stop codon.
            let gc = genetic_code();
            let letter_of = |b: usize| gc.canon[b];
            let spell = |cd: usize| (0..CODON).rev().map(move |i| letter_of(cd >> i & 1));
            let stop = gc.table.iter().position(|c| c.is_none()).unwrap();
            let mut genome = Vec::new();
            for _ in 0..(3 + rng.below(3)) {
                for _ in 0..(6 + rng.below(10)) {
                    let mut cd = rng.below(CODONS);
                    while gc.table[cd].is_none() {
                        cd = rng.below(CODONS);
                    }
                    genome.extend(spell(cd));
                }
                genome.extend(spell(stop));
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
            // take matter from the ocean so the ledger closes; a sea too thin in some letter (the
            // world recipe) skips founders it cannot pay for rather than going negative
            let take: [i64; N_LETTERS] = std::array::from_fn(|l| need[l] + membrane[l] + free[l] + genome.iter().filter(|&&g| g as usize == l).count() as i64);
            if (0..N_LETTERS).any(|l| self.ocean[l] < take[l]) || self.ocean_ka < take.iter().sum::<i64>() {
                continue;
            }
            for l in 0..N_LETTERS {
                self.ocean[l] -= take[l];
                self.ocean_ka -= take[l];
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
                ready: false,
                template: None,
                colony: None,
                mates: 0,
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
            genes.push(Gene { start: s, end: e, promoter: promoter(genome, s, &self.bits), product, func });
        }
        let ka = bidx(KA).unwrap();
        let poly_cut = genes.iter().map(|g| g.func.catalysis[pair(ka, ka)]).fold(0.0, f64::max);
        let synth_cut =
            genes.iter().map(|g| LETTERS.iter().map(|&z| g.func.catalysis[pair(ka, bidx(z).unwrap())]).sum::<f64>() / N_LETTERS as f64).fold(0.0, f64::max);
        let amph = genes.iter().map(|g| g.func.amphiphile).fold(0.0, f64::max);
        let pigment = genes.iter().map(|g| g.func.pigment).fold(0.0, f64::max);
        let adhesion = genes.iter().map(|g| g.func.adhesion).fold(0.0, f64::max);
        let info = Arc::new(GenomeInfo { genes, poly_cut, synth_cut, amph, pigment, adhesion, pending });
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
        self.lineage_mode = false;
        self.env = self.cosmos.sample(self.t_days);
        self.daylight = self.cosmos.sample(self.t_days.floor() + 0.5).visible;
        self.sel_prior = LearnedCopy::priors(&self.model, 0);
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        self.tides(dt, &mut rng);

        let mut cells = std::mem::take(&mut self.cells);
        let mut born = Vec::new();
        let mut dead = Vec::new();
        let logging = self.lineage_log.is_some();
        let colony_shade = self.colony_shades(&cells);
        let mut samples = Vec::new();
        for (ci, c) in cells.iter_mut().enumerate() {
            let info = self.info(&c.genome);
            debug_assert_eq!(c.products.len(), info.genes.len());
            let own = self.own_shade(c, &info);
            let (shade, sensed_shade) = self.shades(c, own, &colony_shade);
            let site = self.patch_site(c.patch, shade);
            let light = self.patch_site(c.patch, sensed_shade).x[2];
            if logging {
                samples.push(crate::lineage::LineageSample { x: self.lineage_features(c, &info, &site, light), ..Default::default() });
            }
            let x = Self::features(c, &info, site.env.kt);
            // a lineage-mode division queued before this cell came back to chemistry
            c.ready = false;
            c.conf = self.model.confidence(&x);
            self.resolve_mode(c, &x, &site);
            // shared membrane physics (permeation, turnover, abiotic assembly) runs in both modes;
            // the surrogate learns only what the detailed interior adds on top of it
            self.exchange(c, &site, dt, &mut rng);
            let mut tally = PairTally::default();
            self.interior_step(c, &info, &site, light, &x, dt, &mut tally, &mut rng);
            self.radiation(c, &site, dt, &mut rng);
            if c.mode == Mode::Detail && self.learn {
                self.model.record_sample(&x);
                tally.record(&mut self.model, &x, 0);
            }
            if c.area() < self.cp.burst_area {
                dead.push(ci);
                if logging {
                    samples[ci].burst = true;
                }
                continue;
            }
            // division: genome copied and membrane doubled
            if c.fork.as_ref().is_some_and(|f| f.len() == c.genome.len()) && c.area() >= (2 * c.birth_area).min(self.cp.divide_area) {
                born.push(ci);
            }
            // colony cells are stuck to the substrate and each other: no drifting off alone
            if site.wet && c.colony.is_none() && rng.chance((self.cp.drift * dt).min(1.0)) {
                let np = if rng.chance(0.5) { c.patch.saturating_sub(1) } else { (c.patch + 1).min(self.cp.patches - 1) };
                c.patch = np;
                // the same water carries it along the shore too: without this, clones spread only
                // across patches at their birth y and the shore fills in stripes
                c.y = (c.y + (rng.f64() - 0.5) * 2.0 * self.cp.drift_along).rem_euclid(1.0);
            }
        }
        // divide (indices stable: we only append)
        let mut daughters = Vec::new();
        for &ci in &born {
            let (e0, b0) = (self.stats.errors_by_letter, self.stats.bits_by_letter);
            if let Some(d) = self.divide(&mut cells[ci], &mut rng) {
                daughters.push(d);
                if logging {
                    let s = &mut samples[ci];
                    s.divided = true;
                    s.bit_errors = std::array::from_fn(|l| self.stats.errors_by_letter[l] - e0[l]);
                    s.bits = std::array::from_fn(|l| self.stats.bits_by_letter[l] - b0[l]);
                }
            }
        }
        if let Some(log) = self.lineage_log.as_mut() {
            log.extend(samples);
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
        self.predation(&mut cells, dt, &mut rng);
        self.colony_physics(&mut cells, dt, &mut rng);
        self.cells = cells;
        self.ctx.rng = rng;
        self.ctx.resolve(self.t_days);
        self.refresh_pending();
        self.t_days += dt;
    }

    /// Re-derive phenotypes whose product folds just landed; log record polymerases.
    pub(crate) fn refresh_pending(&mut self) {
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
    pub(crate) fn expressed_fraction(&self, c: &Cell, info: &GenomeInfo, f: impl Fn(&Gene) -> bool) -> f64 {
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
        // attempts, not letters: a clipped mismatch costs the polymerase a turn
        let mut budget = max_letters;
        for &t in template.iter().rev() {
            let right = self.comp[t as usize] as usize;
            loop {
                // kinetic proofreading: with the right partner absent a polymerase stalls and
                // waits rather than misincorporate; the copy resumes when supply returns
                if budget == 0 || free[right] <= 0 {
                    return (out, errors);
                }
                budget -= 1;
                let Some((pick, frayed)) = how.pick(t as usize, right, free, tally, rng) else { return (out, errors) };
                // a mismatched end frays; the pocket that joins backbone bonds also clips them
                // (catalysis runs both ways): the wrong letter goes back to the pool and the
                // polymerase tries again. Without a pocket a frayed end is extended anyway
                if frayed {
                    continue;
                }
                free[pick] -= 1;
                if pick != right {
                    errors += 1;
                }
                out.push(pick as u8);
                break;
            }
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
    #[allow(clippy::too_many_arguments)]
    fn interior_step(&mut self, c: &mut Cell, info: &GenomeInfo, site: &Site, light: f64, x_feat: &[f64], dt: f64, tally: &mut PairTally, rng: &mut Rng) {
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
            None => Copier::Pairing { gain, kt, pair: &self.ctx.ph.pair, fray: clip_odds(poly, kt) },
        };

        let signals = regulation_signals(c, light, kt, self.cp.colony_max);

        // transcription: each gene makes product copies; any copy error makes junk
        for (k, g) in info.genes.iter().enumerate() {
            // regulated genes follow their signal: expression costs monomers, so making a
            // product only when it pays (pigment in sunlight, say) is an advantage
            let n = rng.poisson(self.cp.tx_rate * supply * boost * g.promoter.level(signals) * dt);
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
            // copy errors are measured against the genome as the copy began: radiation the
            // parent takes afterwards is its own damage, not a copying mistake
            c.template = Some(c.genome.clone());
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
    pub(crate) fn radiation(&mut self, c: &mut Cell, site: &Site, dt: f64, rng: &mut Rng) {
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
            None => Copier::Pairing {
                gain: DRY_PAIR_GAIN * (1.0 + FIDELITY_PER_CUT * poly),
                kt: self.env.kt,
                pair: &self.ctx.ph.pair,
                fray: clip_odds(poly, self.env.kt),
            },
        };
        let (mut genome, errs) = self.copy_strand(&fork, &mut c.free, how, usize::MAX, &mut PairTally::default(), rng);
        if genome.len() < fork.len() {
            for &l in &genome {
                c.free[l as usize] += 1;
            }
            c.fork = Some(fork);
            return None;
        }
        // adhesion as expressed before the parent's products are split with the daughter
        let colony = self.stick(c, &info, rng);
        // antisense strand is recycled into monomers
        for &l in &fork {
            c.free[l as usize] += 1;
        }
        self.stats.copied_letters += genome.len() as u64;
        self.stats.copy_mutations += errs as u64;
        let source = c.template.take().unwrap_or_else(|| c.genome.clone());
        let same = genome.len().min(source.len());
        self.stats.bits_inherited += same as u64;
        self.stats.bit_errors += (0..same).filter(|&i| self.bits[genome[i] as usize] != self.bits[source[i] as usize]).count() as u64;
        for i in 0..same {
            let l = source[i] as usize;
            self.stats.bits_by_letter[l] += 1;
            self.stats.errors_by_letter[l] += (self.bits[genome[i] as usize] != self.bits[l]) as u64;
        }
        let ones: Vec<usize> = (0..same).filter(|&i| self.bits[source[i] as usize] == 1).collect();
        self.stats.ones_inherited += ones.len() as u64;
        self.stats.bit_errors_on1 += ones.iter().filter(|&&i| self.bits[genome[i] as usize] == 0).count() as u64;
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
        // both halves start a new cycle: `born` is the start of the current one
        c.born = self.t_days;
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
            ready: false,
            template: None,
            y: (c.y + rng.f64() * 0.04 - 0.02).rem_euclid(1.0),
            colony,
            mates: 0,
        })
    }

    /// How much of each gene a cell has on hand, 0..1 per gene. Detail: its product copies
    /// (full at 3). Lineage mode never re-makes products, so frozen stocks only erode with
    /// each mutation; there each gene's steady expression under current signals stands in.
    fn expression(&self, c: &Cell, info: &GenomeInfo) -> Vec<f64> {
        if self.lineage_mode {
            let signals = regulation_signals(c, self.patch_site(c.patch, 0.0).x[2], self.env.kt, self.cp.colony_max);
            info.genes.iter().map(|g| g.promoter.level(signals)).collect()
        } else {
            (0..info.genes.len()).map(|k| (c.products.get(k).copied().unwrap_or(0) as f64 / 3.0).min(1.0)).collect()
        }
    }

    /// Pigment shade a cell makes for itself: the best pigment times pigment genes' share of
    /// what the cell expresses.
    pub(crate) fn own_shade(&self, c: &Cell, info: &GenomeInfo) -> f64 {
        if self.lineage_mode {
            // same form as detail's product-copy share, with expression levels for copies
            let e = self.expression(c, info);
            let total: f64 = e.iter().sum();
            if total <= 0.0 {
                return 0.0;
            }
            let pig: f64 = info.genes.iter().zip(&e).filter(|(g, _)| g.func.pigment > 0.0).map(|(_, l)| l).sum();
            return self.cp.max_shade * info.pigment * pig / total;
        }
        self.cp.max_shade * info.pigment * self.expressed_fraction(c, info, |g| g.func.pigment > 0.0)
    }

    /// Per colony: summed member shade and member count.
    pub(crate) fn colony_shades(&mut self, cells: &[Cell]) -> HashMap<u64, (f64, f64)> {
        let mut acc: HashMap<u64, (f64, f64)> = HashMap::new();
        for c in cells {
            if let Some(k) = c.colony {
                let info = self.info(&c.genome);
                let e = acc.entry(k).or_default();
                e.0 += self.own_shade(c, &info);
                e.1 += 1.0;
            }
        }
        acc
    }

    /// (shade against UV, shade a cell's light sensing sees). UV protection is communal: the
    /// colony's mean pigment cover. Sensing sees only the neighbours' pigment, not the cell's
    /// own: a pigment maker among shaded mates still sees full sun, the mates see dimness.
    pub(crate) fn shades(&self, c: &Cell, own: f64, colonies: &HashMap<u64, (f64, f64)>) -> (f64, f64) {
        match c.colony.and_then(|k| colonies.get(&k)) {
            Some(&(sum, n)) if n > 1.0 => (sum / n, (sum - own) / (n - 1.0)),
            _ => (own, 0.0),
        }
    }

    /// Whether a daughter stays stuck to its parent: odds rise with the adhesion the parent
    /// actually expresses. Returns the shared colony id (the parent joins it too).
    pub(crate) fn stick(&mut self, c: &mut Cell, info: &GenomeInfo, rng: &mut Rng) -> Option<u64> {
        let e = self.expression(c, info);
        let adh = info.genes.iter().zip(&e).map(|(g, l)| g.func.adhesion * l).fold(0.0, f64::max);
        let p = ((adh - self.cp.stick_floor) / self.cp.stick_span).clamp(0.0, 1.0);
        if !rng.chance(p) {
            return None;
        }
        Some(*c.colony.get_or_insert_with(|| self.ctx.next_id()))
    }

    /// Engulfing: a big sticky cell grabs a smaller cell on its patch and takes in all of it
    /// (residues come apart into its loose monomers, raw atoms stay raw): matter exact.
    /// Colony mates hold on to each other, so colonies are harder prey, and a cell never eats
    /// its own colony.
    pub(crate) fn predation(&mut self, cells: &mut Vec<Cell>, dt: f64, rng: &mut Rng) {
        let n = cells.len();
        if n < 2 {
            return;
        }
        let mut by_patch: Vec<Vec<usize>> = vec![Vec::new(); self.patches.len()];
        for (i, c) in cells.iter().enumerate() {
            by_patch[c.patch].push(i);
        }
        let mut colony_size: HashMap<u64, f64> = HashMap::new();
        for c in cells.iter() {
            if let Some(k) = c.colony {
                *colony_size.entry(k).or_default() += 1.0;
            }
        }
        let mut eaten = vec![false; n];
        let mut meals: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if eaten[i] {
                continue;
            }
            let info = self.info(&cells[i].genome);
            let e = self.expression(&cells[i], &info);
            let adh = info.genes.iter().zip(&e).map(|(g, l)| g.func.adhesion * l).fold(0.0, f64::max);
            let drive = ((adh - self.cp.eat_floor) / self.cp.eat_span).clamp(0.0, 1.0);
            if drive <= 0.0 {
                continue;
            }
            let mates = &by_patch[cells[i].patch];
            let crowd = (mates.len() as f64 / (self.cp.max_cells as f64 / self.patches.len() as f64)).min(1.0);
            if !rng.chance((self.cp.eat_rate * drive * crowd * dt).min(1.0)) {
                continue;
            }
            let j = mates[rng.below(mates.len())];
            let (pi, pj) = (&cells[i], &cells[j]);
            if j == i || eaten[j] || (pi.colony.is_some() && pi.colony == pj.colony) || pj.area() as f64 > self.cp.eat_size * pi.area() as f64 {
                continue;
            }
            let mates_of_prey = pj.colony.and_then(|k| colony_size.get(&k)).map_or(0.0, |n| n - 1.0);
            if rng.chance(1.0 - (-self.cp.colony_shield * mates_of_prey).exp()) {
                continue;
            }
            eaten[j] = true;
            meals.push((i, j));
        }
        if meals.is_empty() {
            return;
        }
        for &(i, j) in &meals {
            self.meals.push_back((self.t_days, cells[i].patch, cells[i].y, cells[j].y, cells[i].id));
            if self.meals.len() > 200 {
                self.meals.pop_front();
            }
            let prey = cells[j].clone();
            let pinfo = self.info(&prey.genome);
            let p = &mut cells[i];
            for &l in prey.genome.iter().chain(prey.fork.iter().flatten()) {
                p.free[l as usize] += 1;
            }
            for (g, &k) in pinfo.genes.iter().zip(&prey.products) {
                for &l in &g.product {
                    p.free[l as usize] += k;
                }
            }
            for l in 0..N_LETTERS {
                p.free[l] += prey.free[l] + prey.junk[l] + prey.membrane[l];
                p.raw[l] += prey.raw[l];
            }
            p.raw_ka += prey.raw_ka;
            self.stats.predations += 1;
            self.stats.deaths += 1;
        }
        let mut gone: Vec<usize> = meals.iter().map(|&(_, j)| j).collect();
        gone.sort_unstable();
        for &j in gone.iter().rev() {
            cells.swap_remove(j);
        }
    }

    /// Colonies: oversized ones shed a member; members' loose monomers flow toward the
    /// colony mean. Transfers are whole monomers from donors to the needy, matter exact.
    pub(crate) fn colony_physics(&mut self, cells: &mut [Cell], dt: f64, rng: &mut Rng) {
        let mut members: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, c) in cells.iter().enumerate() {
            if let Some(k) = c.colony {
                members.entry(k).or_default().push(i);
            }
        }
        for c in cells.iter_mut() {
            c.mates = 0;
        }
        let k_share = (self.cp.share_rate * dt).min(1.0);
        let mut ids: Vec<u64> = members.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let mut m = members.remove(&id).unwrap();
            while m.len() > self.cp.colony_max {
                let j = m.swap_remove(rng.below(m.len()));
                cells[j].colony = None;
                self.stats.colony_breakaways += 1;
            }
            for &i in &m {
                cells[i].mates = m.len() as u32 - 1;
            }
            if m.len() < 2 {
                // mates died or broke away: a colony of one is a free cell again
                for &i in &m {
                    cells[i].colony = None;
                }
                continue;
            }
            for l in 0..N_LETTERS {
                let total: i64 = m.iter().map(|&i| cells[i].free[l]).sum();
                let mean = total as f64 / m.len() as f64;
                let mut pot = 0i64;
                for &i in &m {
                    let give = ((cells[i].free[l] as f64 - mean).max(0.0) * k_share).floor() as i64;
                    cells[i].free[l] -= give;
                    pot += give;
                }
                let need: f64 = m.iter().map(|&i| (mean - cells[i].free[l] as f64).max(0.0)).sum();
                if pot == 0 || need <= 0.0 {
                    if pot > 0 {
                        cells[m[0]].free[l] += pot;
                    }
                    continue;
                }
                let mut left = pot;
                for &i in &m {
                    let want = (mean - cells[i].free[l] as f64).max(0.0);
                    let take = ((want / need) * pot as f64).floor() as i64;
                    let take = take.min(left);
                    cells[i].free[l] += take;
                    left -= take;
                }
                // rounding remainder to the neediest member
                if left > 0 {
                    let &i = m.iter().min_by_key(|&&i| cells[i].free[l]).unwrap();
                    cells[i].free[l] += left;
                }
                self.stats.shared_monomers += pot as u64;
            }
        }
    }

    pub(crate) fn spill(&mut self, c: Cell) {
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
    pub(crate) fn tides(&mut self, dt: f64, rng: &mut Rng) {
        let tide = Tide { flush: self.cp.flush_rate, washout: self.cp.washout_rate, raw: std::array::from_fn(|l| self.cp.ocean_raw * self.cp.sea[l] * self.cp.patch_cells), ka: self.cp.ocean_ka * self.cp.patch_cells };
        for p in self.patches.iter_mut() {
            if self.env.tide_level > p.elevation {
                tide.mix(p, &mut self.ocean, &mut self.ocean_ka, dt, rng);
            }
        }
    }

    /// Fresh dice, same state: shadow mode's detail twin.
    pub fn reseed(&mut self, seed: u64) {
        self.ctx.reseed(seed);
    }

    /// (share of cells in colonies of 2+, mean size of those colonies, mean genome adhesion).
    pub fn colony_stats(&mut self) -> (f64, f64, f64) {
        let mut sizes: HashMap<u64, usize> = HashMap::new();
        for c in &self.cells {
            if let Some(k) = c.colony {
                *sizes.entry(k).or_default() += 1;
            }
        }
        let colonial: usize = sizes.values().filter(|&&n| n >= 2).sum();
        let groups = sizes.values().filter(|&&n| n >= 2).count();
        let genomes: Vec<Seq> = self.cells.iter().map(|c| c.genome.clone()).collect();
        let adh: f64 = genomes.iter().map(|g| self.info(g).adhesion).sum();
        let n = self.cells.len().max(1) as f64;
        (colonial as f64 / n, if groups > 0 { colonial as f64 / groups as f64 } else { 0.0 }, adh / n)
    }

    /// Share of genes (over all cells) on each signal, in `Signal` order (none, light, cold,
    /// Li scarcity, mates, height, exposure); then the share of pigment-making genes switched on by light.
    pub fn regulation_stats(&mut self) -> [f64; 8] {
        let genomes: Vec<Seq> = self.cells.iter().map(|c| c.genome.clone()).collect();
        let mut by = [0.0; 7];
        let (mut pig, mut pig_light) = (0.0f64, 0.0f64);
        for g in &genomes {
            let info = self.info(g);
            for gene in &info.genes {
                by[gene.promoter.signal as usize] += 1.0;
                if gene.func.pigment > 0.6 {
                    pig += 1.0;
                    if gene.promoter.signal == Signal::Light && gene.promoter.on_high {
                        pig_light += 1.0;
                    }
                }
            }
        }
        let total: f64 = by.iter().sum::<f64>().max(1.0);
        let mut out = [0.0; 8];
        for (o, b) in out.iter_mut().zip(by) {
            *o = b / total;
        }
        out[7] = pig_light / pig.max(1.0);
        out
    }

    /// Division-of-labour index for pigment: mean variance of members' pigment expression
    /// within colonies over the variance across all cells. Clonal colonies sit below 1;
    /// members taking different jobs push it up. Also returns the colonial cell count used.
    pub fn pigment_differentiation(&self) -> (f64, usize) {
        let frac = |c: &Cell| -> f64 {
            let Some(info) = self.genomes.get(&c.genome) else { return 0.0 };
            self.expressed_fraction(c, info, |g| g.func.pigment > 0.6)
        };
        let all: Vec<f64> = self.cells.iter().map(frac).collect();
        let var = |v: &[f64]| {
            let m = v.iter().sum::<f64>() / v.len().max(1) as f64;
            v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len().max(1) as f64
        };
        let total = var(&all);
        let mut groups: HashMap<u64, Vec<f64>> = HashMap::new();
        for (c, f) in self.cells.iter().zip(&all) {
            if let Some(k) = c.colony {
                groups.entry(k).or_default().push(*f);
            }
        }
        let (mut within, mut n) = (0.0, 0usize);
        for g in groups.values().filter(|g| g.len() >= 2) {
            within += var(g) * g.len() as f64;
            n += g.len();
        }
        if n == 0 || total <= 0.0 { (0.0, n) } else { (within / n as f64 / total, n) }
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
    /// The derived code is the one the [2, 6] world was written with: codons 0-4 the letters in
    /// order, 5 a synonym for Li (the food letter), 6-7 stops; Li and Fl the heritable pair.
    // pins below are values of the [2, 6] world
    #[cfg(not(feature = "shells-266"))]
    #[test]
    fn derived_code_matches_the_original() {
        let gc = genetic_code();
        assert_eq!(CODON, 3);
        assert_eq!(gc.table, [Some(0), Some(1), Some(2), Some(3), Some(4), Some(1), None, None]);
        assert_eq!(gc.canon, [1, 4]);
        assert_eq!(gc.food, 1);
    }

    use super::*;
    use crate::cosmos::CosmosParams;

    fn rates() -> RateTable {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), crate::element::data_file("rates0.json"))).unwrap()).unwrap()
    }

    // pins below are values of the [2, 6] world
    #[cfg(not(feature = "shells-266"))]
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
        for _ in 0..4000 {
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

    #[test]
    fn colonies_form_share_and_keep_matter() {
        // every expressed sticky product holds: colonies must form, and sharing is exact
        let cp = CellParams { stick_floor: 0.0, stick_span: 0.01, max_cells: 120, ..Default::default() };
        let mut w = World::new(PoolParams::default(), cp, Cosmos::new(CosmosParams::default()), rates(), 11, 60);
        let start = w.matter();
        for _ in 0..1500 {
            w.step(0.02);
        }
        assert_eq!(w.matter(), start);
        let (colonial, size, _) = w.colony_stats();
        assert!(colonial > 0.2 && size >= 2.0, "colonies did not form: {colonial} {size}");
        assert!(w.stats.shared_monomers > 0, "colony members never shared: test would pass vacuously");
    }

    #[test]
    fn engulfing_keeps_matter() {
        // sticky cells everywhere and eager to eat: predation must happen, matter must hold
        let cp = CellParams { eat_floor: 0.0, eat_span: 0.01, eat_rate: 20.0, eat_size: 1.0, max_cells: 120, ..Default::default() };
        let mut w = World::new(PoolParams::default(), cp, Cosmos::new(CosmosParams::default()), rates(), 13, 80);
        let start = w.matter();
        for _ in 0..800 {
            w.step(0.02);
        }
        assert_eq!(w.matter(), start);
        assert!(w.stats.predations > 0, "no cell was eaten: test would pass vacuously");
    }
}
