//! Scale-4 surrogate: a cell as a lineage, not a chemistry. In lineage mode a cell's interior
//! chemistry stops; it grows its membrane and divides or bursts at rates learned from detailed
//! cells with the same (regulated) traits in the same conditions (temperature, flooding, Li
//! supply, cycle age, daylight). Growth and the daughter genome are paid from the cell and its
//! patch, and division (gated on size, as in detail) splits the cell in half, so matter stays
//! exact, a starved patch cannot breed and cells keep a size cycle (engulfing needs size gaps). The daughter genome takes
//! heritable bit errors at the learned per-division rate. Tides, radiation, drift, colonies,
//! engulfing and crowding still run for real.
use crate::cell::{Cell, World, pick_weighted, regulated_traits, regulation_signals, remap_products, round_flux};
use crate::polymer::N_LETTERS;
use crate::pool::Site;
use crate::rng::Rng;
use crate::surrogate::{Axis, RateTable};
use serde::{Deserialize, Serialize};

/// Fate table: divisions per cell-day, bursts per cell-day.
pub const L_DIV: usize = 0;
pub const L_BURST: usize = 1;
pub const L_CHANNELS: usize = 2;

const AGE_MAX: f64 = 20.0;
const RAW_MAX: f64 = 0.8;
/// Li's index among the letters (Hy, Li, Bo, Ox, Fl), and a scale putting its patch
/// concentration on the food axis.
const LI: usize = 1;
const LI_SCALE: f64 = 4.0;
/// kT span of the coast: polar winters reach ~0.80 (cells at 0.85 left the table every year).
const KT_LO: f64 = 0.78;
const KT_HI: f64 = 1.15;
const TRAIT_MAX: f64 = 0.75;
const DAYLIGHT_MAX: f64 = 1.3;

/// One detailed cell-step: the cell's features at its start and what happened to it.
#[derive(Clone, Debug, Default)]
pub struct LineageSample {
    pub x: [f64; 8],
    pub divided: bool,
    pub burst: bool,
    /// Heritable errors and bits copied at this step's division, by source letter.
    pub bit_errors: [u64; N_LETTERS],
    pub bits: [u64; N_LETTERS],
}

/// What a lineage-mode cell needs, factored so every table stays densely trained: a base
/// fate rate from conditions (kT, flooding, Li supply, cycle age, daylight), times a relative fitness from
/// heritable traits (polymerase, metabolism, membrane), plus copy fidelity over (kT,
/// polymerase). One 7-axis table had ~18k bins; cells kept landing in empty ones and sites
/// flipped back to detail daily. The price: trait effects cannot depend on climate.
/// Fidelity is learned at divisions and per source letter: radiation leaves non-canonical
/// letters that pair weakly and are miscopied ~10x as often, which partly undoes the damage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineageModel {
    pub base: RateTable,
    /// Events over base-expected events, per trait bin: relative fitness.
    pub fitness: RateTable,
    pub fidelity: RateTable,
}

impl Default for LineageModel {
    fn default() -> Self {
        let mut base = RateTable::new(
            vec![
                Axis::new("kT", KT_LO, KT_HI, 8),
                Axis::new("wet", 0.0, 1.0, 2),
                Axis::new("li_supply", 0.0, RAW_MAX, 4),
                Axis::new("age", 0.0, AGE_MAX, 8),
                Axis::new("daylight", 0.0, DAYLIGHT_MAX, 4),
            ],
            L_CHANNELS,
        );
        // divisions are a few per cell-week: a bin needs many cell-steps to hold a rate
        base.samples_for_confidence = 300.0;
        // traits sit mostly in 0.1..0.6: resolve them there, or selection blurs away
        let mut fitness =
            RateTable::new(vec![Axis::new("poly", 0.0, TRAIT_MAX, 8), Axis::new("synth", 0.0, TRAIT_MAX, 5), Axis::new("amph", 0.0, TRAIT_MAX, 6)], L_CHANNELS);
        fitness.samples_for_confidence = 300.0;
        let fidelity = RateTable::new(vec![Axis::new("kT", KT_LO, KT_HI, 10), Axis::new("poly", 0.0, TRAIT_MAX, 10)], N_LETTERS);
        Self { base, fitness, fidelity }
    }
}

fn base_x(x: &[f64]) -> [f64; 5] {
    [x[0], x[1], x[2], x[6], x[7]]
}

fn trait_x(x: &[f64]) -> [f64; 3] {
    [x[3], x[4], x[5]]
}

impl LineageModel {
    /// Record a batch of detailed cell-steps of length `dt` days.
    pub fn record(&mut self, samples: &[LineageSample], dt: f64) {
        let prior = self.priors();
        for s in samples {
            let (b, t) = (base_x(&s.x), trait_x(&s.x));
            self.base.record_sample(&b);
            self.fitness.record_sample(&t);
            for (ch, ev) in [(L_DIV, s.divided), (L_BURST, s.burst)] {
                self.base.record(&b, ch, ev as u8 as f64, dt);
                // relative fitness: events against what conditions alone predict
                let expected = self.base.rate(&b, ch, prior[ch], 1.0) * dt;
                self.fitness.record(&t, ch, ev as u8 as f64, expected);
            }
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
    /// One memory for all tables: a longer one for conditions alone (to remember seasons)
    /// scored worse over five warm-ups (95.8% vs 97.1% of ceiling).
    pub fn forget(&mut self, dt: f64, tau: f64) {
        let k = (-dt / tau).exp();
        self.base.decay(k);
        self.fitness.decay(k);
        self.fidelity.decay(k);
    }

    /// Pooled rates: base fate channels, fidelity per letter, then pooled relative fitness.
    pub fn priors(&self) -> Vec<f64> {
        let mut p: Vec<f64> = (0..L_CHANNELS).map(|ch| self.base.global_rate(ch)).collect();
        p.extend((0..N_LETTERS).map(|l| self.fidelity.global_rate(l)));
        p.extend((0..L_CHANNELS).map(|ch| self.fitness.global_rate(ch)));
        p
    }

    pub fn confidence(&self, x: &[f64]) -> f64 {
        self.base.confidence(&base_x(x)).min(self.fitness.confidence(&trait_x(x)))
    }

    /// (divisions/day, bursts/day, bit error per copied letter by source letter) at `x`.
    pub fn rates(&self, x: &[f64], prior: &[f64]) -> (f64, f64, [f64; N_LETTERS]) {
        let (div, burst) = self.fates(x, prior);
        (div, burst, self.bit_error(x, prior))
    }

    /// Bit error per copied letter, by source letter.
    pub fn bit_error(&self, x: &[f64], prior: &[f64]) -> [f64; N_LETTERS] {
        let chs: [(usize, f64); N_LETTERS] = std::array::from_fn(|l| (l, prior[L_CHANNELS + l]));
        let mut p_bit = [0.0; N_LETTERS];
        self.fidelity.rates_smooth(&fidelity_x(x), &chs, 1.0, &[1], &mut p_bit);
        p_bit.map(|p| p.clamp(0.0, 0.5))
    }

    /// (divisions/day, bursts/day) at `x`.
    pub fn fates(&self, x: &[f64], prior: &[f64]) -> (f64, f64) {
        let mut base = [0.0; 2];
        self.base.rates_smooth(&base_x(x), &[(L_DIV, prior[L_DIV]), (L_BURST, prior[L_BURST])], 1.0, &[], &mut base);
        // relative fitness shrinks toward 1 (no effect) where a trait bin is thin, and is taken
        // relative to its pooled value: early base rates run low, which would otherwise make
        // every trait look fitter than average
        let mut fit = [0.0; 2];
        let norm = [prior[L_CHANNELS + N_LETTERS], prior[L_CHANNELS + N_LETTERS + 1]];
        self.fitness.rates_smooth(&trait_x(x), &[(L_DIV, norm[0]), (L_BURST, norm[1])], 1.0, &[0, 1, 2], &mut fit);
        let rel = |ch: usize| if norm[ch] > 0.0 { fit[ch] / norm[ch] } else { 1.0 };
        ((base[0] * rel(0)).max(0.0), (base[1] * rel(1)).max(0.0))
    }
}

/// Lineage features from raw inputs: `li` is the Li (raw + loose) per cell volume around the
/// cell, `traits` the regulated (polymerase, metabolism, membrane), `age` days into the cycle.
pub fn lineage_x(kt: f64, wet: bool, li: f64, traits: (f64, f64, f64), age: f64, daylight: f64) -> [f64; 8] {
    [
        kt,
        if wet { 1.0 } else { 0.0 },
        (li * LI_SCALE).min(RAW_MAX),
        traits.0.min(TRAIT_MAX),
        traits.1.min(TRAIT_MAX),
        traits.2.min(TRAIT_MAX),
        age.clamp(0.0, AGE_MAX),
        daylight.clamp(0.0, DAYLIGHT_MAX),
    ]
}

fn fidelity_x(x: &[f64]) -> [f64; 2] {
    [x[0], x[3]]
}

impl World {
    /// Food and age saturate at the table edge (more of either changes little); temperature
    /// does not: an unseen climate is a discovery.
    /// `light` is what the cell's promoters sense (through neighbours' pigment, as detailed
    /// regulation sees it).
    pub fn lineage_features(&self, c: &Cell, info: &crate::cell::GenomeInfo, site: &Site, light: f64) -> [f64; 8] {
        // food is the scarce genome partner, not raw Ka: division waits on Li for the second
        // strand (shadow agreement 94.9% -> 98.2% of ceiling with this one change)
        let p = &self.patches[c.patch];
        let li = (p.raw[LI] + p.free[LI]) as f64 / self.cp.patch_cells;
        // traits as currently switched on, so promoters count in lineage fitness too
        let traits = regulated_traits(info, regulation_signals(c, light, site.env.kt, self.cp.colony_max));
        lineage_x(site.env.kt, site.wet, li, traits, self.t_days - c.born, self.daylight)
    }

    /// Mean confidence of the lineage table over this world's cells.
    pub fn lineage_confidence(&mut self, t: &LineageModel) -> f64 {
        // runs before this step's sample: refresh so the check sees today's light
        self.daylight = self.cosmos.sample(self.t_days.floor() + 0.5).visible;
        let cells = std::mem::take(&mut self.cells);
        let colony_shade = self.colony_shades(&cells);
        let mut sum = 0.0;
        for c in &cells {
            let info = self.info(&c.genome);
            let own = self.own_shade(c, &info);
            let (shade, sensed) = self.shades(c, own, &colony_shade);
            let site = self.patch_site(c.patch, shade);
            let light = self.patch_site(c.patch, sensed).x[2];
            sum += t.confidence(&self.lineage_features(c, &info, &site, light));
        }
        let n = cells.len();
        self.cells = cells;
        if n == 0 { 1.0 } else { sum / n as f64 }
    }

    /// One step with every cell as a lineage. `prior` holds the table's pooled rates.
    pub fn step_lineage(&mut self, dt: f64, t: &LineageModel, prior: &[f64]) {
        self.lineage_mode = true;
        self.env = self.cosmos.sample(self.t_days);
        self.daylight = self.cosmos.sample(self.t_days.floor() + 0.5).visible;
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        self.tides(dt, &mut rng);
        let mut cells = std::mem::take(&mut self.cells);
        let colony_shade = self.colony_shades(&cells);
        let mut born = Vec::new();
        let mut dead = Vec::new();
        for (ci, c) in cells.iter_mut().enumerate() {
            let info = self.info(&c.genome);
            let own = self.own_shade(c, &info);
            let (shade, sensed) = self.shades(c, own, &colony_shade);
            let site = self.patch_site(c.patch, shade);
            let light = self.patch_site(c.patch, sensed).x[2];
            let x = self.lineage_features(c, &info, &site, light);
            if c.template.is_none() {
                c.template = Some(c.genome.clone());
            }
            let (div, burst, p_bit) = t.rates(&x, prior);
            self.radiation(c, &site, dt, &mut rng);
            self.grow(c, div, dt, &mut rng);
            // the same floor as detail: a membrane too small to hold a cell bursts
            if c.area() < self.cp.burst_area || rng.chance((burst * dt).min(1.0)) {
                dead.push(ci);
                continue;
            }
            // a cell due to divide stays due until it can pay for the daughter's genome
            if c.ready || rng.chance((div * dt).min(1.0)) {
                c.ready = true;
                born.push((ci, p_bit));
            }
            if site.wet && c.colony.is_none() && rng.chance((self.cp.drift * dt).min(1.0)) {
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
            // the same size gate as detail: a cell that could not grow does not halve itself away
            let c = &cells[ci];
            if c.area() < (2 * c.birth_area).min(self.cp.divide_area) {
                continue;
            }
            if let Some(mut d) = self.split_cell(&mut cells[ci], p_bit, &mut rng) {
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
        self.predation(&mut cells, dt, &mut rng);
        self.colony_physics(&mut cells, dt, &mut rng);
        self.cells = cells;
        self.ctx.rng = rng;
        self.ctx.resolve(self.t_days);
        self.refresh_pending();
        self.t_days += dt;
    }

    /// Lineage growth: the membrane builds toward twice its birth size at the pace the
    /// division rate implies, letters in the membrane's own proportions, paid from the cell's
    /// loose monomers, then the patch's (then a raw letter plus raw Ka). Without it every lineage cell
    /// stayed full size, and engulfing (which needs a size gap) misfired.
    fn grow(&mut self, c: &mut Cell, div: f64, dt: f64, rng: &mut Rng) {
        if c.area() >= (2 * c.birth_area).min(self.cp.divide_area) {
            return;
        }
        let n = round_flux(c.birth_area as f64 * div * dt, rng).max(0);
        let p = &mut self.patches[c.patch];
        for _ in 0..n {
            let Some(l) = pick_weighted(&c.membrane, rng) else { return };
            // the cell's own loose monomers first (as detail's membrane assembly), so a meal
            // feeds growth; then the patch
            if c.free[l] > 0 {
                c.free[l] -= 1;
            } else if p.free[l] > 0 {
                p.free[l] -= 1;
            } else if p.raw[l] > 0 && p.raw_ka > 0 {
                p.raw[l] -= 1;
                p.raw_ka -= 1;
            } else {
                return;
            }
            c.membrane[l] += 1;
        }
    }

    /// Division as in detail: the cell splits its contents in half with a daughter whose
    /// genome is the cycle-start template copied with bit errors at `p_bit` (by source letter)
    /// and written in canonical letters, as two rounds of templated copying do. The new
    /// genome's letters come from the parent's loose monomers, then the patch; if neither can
    /// pay, the division waits.
    fn split_cell(&mut self, c: &mut Cell, p_bit: [f64; N_LETTERS], rng: &mut Rng) -> Option<Cell> {
        let mut genome = c.template.clone().unwrap_or_else(|| c.genome.clone());
        // canonical letter per bit: the fixed points of copying twice (Li 0, Fl 1)
        let canon: [u8; 2] =
            std::array::from_fn(|b| (0..N_LETTERS).find(|&l| self.comp[self.comp[l] as usize] as usize == l && self.bits[l] as usize == b).unwrap() as u8);
        let mut errors = 0;
        // tallied locally: a division that cannot be paid for is retried, not counted
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
        // pay for the genome before deriving its genes (a waiting cell retries every step)
        let mut need = [0i64; N_LETTERS];
        for &l in &genome {
            need[l as usize] += 1;
        }
        let p = &self.patches[c.patch];
        let (mut own, mut loose, mut raw) = ([0i64; N_LETTERS], [0i64; N_LETTERS], [0i64; N_LETTERS]);
        for l in 0..N_LETTERS {
            own[l] = need[l].min(c.free[l]);
            loose[l] = (need[l] - own[l]).min(p.free[l]);
            raw[l] = need[l] - own[l] - loose[l];
            if raw[l] > p.raw[l] {
                return None;
            }
        }
        if raw.iter().sum::<i64>() > p.raw_ka {
            return None;
        }
        let p = &mut self.patches[c.patch];
        for l in 0..N_LETTERS {
            c.free[l] -= own[l];
            p.free[l] -= loose[l];
            p.raw[l] -= raw[l];
        }
        p.raw_ka -= raw.iter().sum::<i64>();
        let info = self.info(&c.genome);
        let colony = self.stick(c, &info, rng);
        // a fork left from detail mode belongs to the cycle that just ended: back to monomers
        if let Some(f) = c.fork.take() {
            for l in f {
                c.free[l as usize] += 1;
            }
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
        let mut share = vec![0i64; c.products.len()];
        for (k, n) in c.products.iter_mut().enumerate() {
            share[k] = split(*n, rng);
            *n -= share[k];
        }
        let dinfo = self.info(&genome);
        let d_products = remap_products(&info, &share, &dinfo, &mut d_free);
        let id = self.ctx.next_id();
        c.generation += 1;
        c.born = self.t_days;
        c.birth_area = c.area().max(1);
        c.template = Some(c.genome.clone());
        self.stats.divisions += 1;
        self.stats.births += 1;
        self.stats.bits_inherited += genome.len() as u64;
        self.stats.bit_errors += errors;
        for (l, (e, n)) in by_letter.into_iter().enumerate() {
            self.stats.errors_by_letter[l] += e;
            self.stats.bits_by_letter[l] += n;
        }
        Some(Cell {
            id,
            parent: Some(c.id),
            born: self.t_days,
            generation: c.generation,
            genome,
            products: d_products,
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
            colony,
            mates: 0,
            y: (c.y + rng.f64() * 0.04 - 0.02).rem_euclid(1.0),
        })
    }
}
