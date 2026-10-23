//! Scale-4 surrogate: a cell as a lineage, not a chemistry. In lineage mode a cell keeps its
//! contents frozen and divides or bursts at rates learned from detailed cells with the same
//! genes in the same conditions (temperature, flooding, food, age). Division clones the cell's
//! matter out of its patch, so matter stays exact and a starved patch cannot breed; the
//! daughter genome takes heritable bit errors at the learned per-division rate. Tides,
//! radiation, drift between patches and crowding still run for real.
use crate::cell::{Cell, World, remap_products};
use crate::polymer::N_LETTERS;
use crate::pool::Site;
use crate::rng::Rng;
use crate::surrogate::{Axis, RateTable};

/// Fate table: divisions per cell-day, bursts per cell-day.
pub const L_DIV: usize = 0;
pub const L_BURST: usize = 1;
pub const L_CHANNELS: usize = 2;

const AGE_MAX: f64 = 20.0;
const RAW_MAX: f64 = 0.8;
const TRAIT_MAX: f64 = 0.75;
/// Fate-table axes that are heritable traits: rates interpolate along them (see `rate_smooth`).
const TRAIT_AXES: [usize; 3] = [3, 4, 5];

/// One detailed cell-step: the cell's features at its start and what happened to it.
#[derive(Clone, Debug, Default)]
pub struct LineageSample {
    pub x: [f64; 7],
    pub divided: bool,
    pub burst: bool,
    /// Heritable errors and bits copied at this step's division, by source letter.
    pub bit_errors: [u64; N_LETTERS],
    pub bits: [u64; N_LETTERS],
}

/// What a lineage-mode cell needs: its fate over the full feature set, and copy fidelity over
/// (kT, polymerase) only. Fidelity is learned at divisions, which are rare per bin; on the full
/// grid most bins fell back to a pooled rate skewed by the error-prone early days (1.5x too many
/// errors). Fidelity is per source letter: radiation leaves non-canonical letters that pair
/// weakly and are miscopied ~10x as often, which partly undoes the damage.
#[derive(Clone, Debug)]
pub struct LineageModel {
    pub fate: RateTable,
    pub fidelity: RateTable,
}

impl Default for LineageModel {
    fn default() -> Self {
        let mut fate = RateTable::new(
            vec![
                Axis::new("kT", 0.85, 1.15, 4),
                Axis::new("wet", 0.0, 1.0, 2),
                Axis::new("raw_conc", 0.0, RAW_MAX, 3),
                // traits sit mostly in 0.1..0.6: resolve them there, or selection blurs away
                Axis::new("poly", 0.0, TRAIT_MAX, 6),
                Axis::new("synth", 0.0, TRAIT_MAX, 4),
                Axis::new("amph", 0.0, TRAIT_MAX, 5),
                Axis::new("age", 0.0, AGE_MAX, 5),
            ],
            L_CHANNELS,
        );
        // divisions are a few per cell-week: a bin needs many cell-steps to hold a rate
        fate.samples_for_confidence = 300.0;
        let fidelity = RateTable::new(vec![Axis::new("kT", 0.85, 1.15, 8), Axis::new("poly", 0.0, TRAIT_MAX, 10)], N_LETTERS);
        Self { fate, fidelity }
    }
}

impl LineageModel {
    /// Record a batch of detailed cell-steps of length `dt` days.
    pub fn record(&mut self, samples: &[LineageSample], dt: f64) {
        for s in samples {
            self.fate.record_sample(&s.x);
            self.fate.record(&s.x, L_DIV, s.divided as u8 as f64, dt);
            self.fate.record(&s.x, L_BURST, s.burst as u8 as f64, dt);
            if s.bits.iter().any(|&b| b > 0) {
                let f = fidelity_x(&s.x);
                self.fidelity.record_sample(&f);
                for l in 0..N_LETTERS {
                    if s.bits[l] > 0 {
                        self.fidelity.record(&f, l, s.bit_errors[l] as f64, s.bits[l] as f64);
                    }
                }
            }
        }
    }

    /// Evolution moves the rates (copy error keeps falling as polymerases improve): old data
    /// fades with time constant `tau` days, so the table tracks the present.
    pub fn forget(&mut self, dt: f64, tau: f64) {
        let k = (-dt / tau).exp();
        self.fate.decay(k);
        self.fidelity.decay(k);
    }

    /// Pooled rates: fate channels, then fidelity.
    pub fn priors(&self) -> Vec<f64> {
        let mut p: Vec<f64> = (0..L_CHANNELS).map(|ch| self.fate.global_rate(ch)).collect();
        p.extend((0..N_LETTERS).map(|l| self.fidelity.global_rate(l)));
        p
    }

    pub fn confidence(&self, x: &[f64]) -> f64 {
        self.fate.confidence(x)
    }

    /// (divisions/day, bursts/day, bit error per copied letter by source letter) at `x`.
    fn rates(&self, x: &[f64], prior: &[f64]) -> (f64, f64, [f64; N_LETTERS]) {
        let f = fidelity_x(x);
        let p_bit = std::array::from_fn(|l| self.fidelity.rate_smooth(&f, l, prior[L_CHANNELS + l], 1.0, &[1]).clamp(0.0, 0.5));
        (
            self.fate.rate_smooth(x, L_DIV, prior[L_DIV], 1.0, &TRAIT_AXES).max(0.0),
            self.fate.rate_smooth(x, L_BURST, prior[L_BURST], 1.0, &TRAIT_AXES).max(0.0),
            p_bit,
        )
    }
}

fn fidelity_x(x: &[f64]) -> [f64; 2] {
    [x[0], x[3]]
}

impl World {
    /// Food and age saturate at the table edge (more of either changes little); temperature
    /// does not: an unseen climate is a discovery.
    pub fn lineage_features(&self, c: &Cell, info: &crate::cell::GenomeInfo, site: &Site) -> [f64; 7] {
        let raw = self.patches[c.patch].raw_ka as f64 / self.cp.patch_cells;
        [
            site.env.kt,
            if site.wet { 1.0 } else { 0.0 },
            raw.min(RAW_MAX),
            info.poly_cut.min(TRAIT_MAX),
            info.synth_cut.min(TRAIT_MAX),
            info.amph.min(TRAIT_MAX),
            (self.t_days - c.born).clamp(0.0, AGE_MAX),
        ]
    }

    /// Mean confidence of the lineage table over this world's cells.
    pub fn lineage_confidence(&mut self, t: &LineageModel) -> f64 {
        let cells = std::mem::take(&mut self.cells);
        let mut sum = 0.0;
        for c in &cells {
            let info = self.info(&c.genome);
            let site = self.patch_site(c.patch, 0.0);
            sum += t.confidence(&self.lineage_features(c, &info, &site));
        }
        let n = cells.len();
        self.cells = cells;
        if n == 0 { 1.0 } else { sum / n as f64 }
    }

    /// One step with every cell as a lineage. `prior` holds the table's pooled rates.
    pub fn step_lineage(&mut self, dt: f64, t: &LineageModel, prior: &[f64]) {
        self.env = self.cosmos.sample(self.t_days);
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        self.tides(dt, &mut rng);
        let mut cells = std::mem::take(&mut self.cells);
        let mut born = Vec::new();
        let mut dead = Vec::new();
        for (ci, c) in cells.iter_mut().enumerate() {
            let info = self.info(&c.genome);
            let shade = self.cp.max_shade * info.pigment * self.expressed_fraction(c, &info, |g| g.func.pigment > 0.0);
            let site = self.patch_site(c.patch, shade);
            let x = self.lineage_features(c, &info, &site);
            if c.template.is_none() {
                c.template = Some(c.genome.clone());
            }
            let (div, burst, p_bit) = t.rates(&x, prior);
            self.radiation(c, &site, dt, &mut rng);
            if rng.chance((burst * dt).min(1.0)) {
                dead.push(ci);
                continue;
            }
            // a cell due to divide stays due until its patch can pay for the daughter
            if c.ready || rng.chance((div * dt).min(1.0)) {
                c.ready = true;
                born.push((ci, p_bit));
            }
            if site.wet && rng.chance((self.cp.drift * dt).min(1.0)) {
                c.patch = if rng.chance(0.5) { c.patch.saturating_sub(1) } else { (c.patch + 1).min(self.cp.patches - 1) };
            }
        }
        // when the patch runs short, who pays first must be luck, not list position: daughters
        // sit at the back, so a fixed order would favour old lineages over fitter new ones
        for i in (1..born.len()).rev() {
            born.swap(i, rng.below(i + 1));
        }
        let mut daughters = Vec::new();
        for (ci, p_bit) in born {
            if let Some(mut d) = self.clone_cell(&mut cells[ci], p_bit, &mut rng) {
                cells[ci].ready = false;
                d.template = Some(d.genome.clone());
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

    /// A daughter with the parent's contents, its genome copied with bit errors at `p_bit` (by
    /// source letter) and written in canonical letters, as two rounds of templated copying do.
    /// Its matter comes from the parent's patch: residues from loose monomers first, then raw
    /// atoms; if the patch cannot pay for a whole cell, there is no division.
    fn clone_cell(&mut self, c: &mut Cell, p_bit: [f64; N_LETTERS], rng: &mut Rng) -> Option<Cell> {
        let info = self.info(&c.genome);
        let mut genome = c.template.clone().unwrap_or_else(|| c.genome.clone());
        let mut errors = 0;
        // canonical letter per bit: the fixed points of copying twice (Li 0, Fl 1)
        let canon: [u8; 2] =
            std::array::from_fn(|b| (0..N_LETTERS).find(|&l| self.comp[self.comp[l] as usize] as usize == l && self.bits[l] as usize == b).unwrap() as u8);
        // tallied locally: a division the patch cannot pay for is retried, not counted
        let mut by_letter = [(0u64, 0u64); N_LETTERS];
        for g in genome.iter_mut() {
            let src = *g as usize;
            let mut bit = self.bits[src] as usize;
            if rng.chance(p_bit[src]) {
                bit ^= 1;
                errors += 1;
                by_letter[src].0 += 1;
            }
            by_letter[src].1 += 1;
            *g = canon[bit];
        }
        let dinfo = self.info(&genome);
        let mut free = c.free;
        let products = if genome == c.genome { c.products.clone() } else { remap_products(&info, &c.products, &dinfo, &mut free) };
        let d = Cell {
            id: 0,
            parent: Some(c.id),
            born: self.t_days,
            generation: c.generation + 1,
            genome,
            products,
            junk: c.junk,
            membrane: c.membrane,
            birth_area: c.area().max(1),
            free,
            raw: c.raw,
            raw_ka: c.raw_ka,
            fork: None,
            patch: c.patch,
            mode: c.mode,
            conf: c.conf,
            mutations: 0,
            ready: false,
            template: None,
            y: (c.y + rng.f64() * 0.04 - 0.02).rem_euclid(1.0),
        };
        let (m, _) = d.matter(&dinfo);
        let p = &mut self.patches[c.patch];
        // bound letters carry a Ka each: a loose monomer pays for one, else a raw letter + raw Ka
        let mut from_free = [0i64; N_LETTERS];
        let mut from_raw = [0i64; N_LETTERS];
        let mut ka_needed = d.raw_ka;
        for l in 0..N_LETTERS {
            let bound = m[l] - d.raw[l];
            from_free[l] = bound.min(p.free[l]);
            from_raw[l] = bound - from_free[l] + d.raw[l];
            ka_needed += bound - from_free[l];
            if from_raw[l] > p.raw[l] {
                return None;
            }
        }
        if ka_needed > p.raw_ka {
            return None;
        }
        for l in 0..N_LETTERS {
            p.free[l] -= from_free[l];
            p.raw[l] -= from_raw[l];
        }
        p.raw_ka -= ka_needed;
        let id = self.ctx.next_id();
        c.generation += 1;
        c.born = self.t_days;
        c.template = Some(c.genome.clone());
        // a fork left from detail mode belongs to the cycle that just ended: back to monomers
        if let Some(f) = c.fork.take() {
            for l in f {
                c.free[l as usize] += 1;
            }
        }
        self.stats.divisions += 1;
        self.stats.births += 1;
        self.stats.bits_inherited += d.genome.len() as u64;
        self.stats.bit_errors += errors;
        for (l, (e, n)) in by_letter.into_iter().enumerate() {
            self.stats.errors_by_letter[l] += e;
            self.stats.bits_by_letter[l] += n;
        }
        Some(Cell { id, ..d })
    }
}
