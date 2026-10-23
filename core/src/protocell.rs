//! Scale 2: protocells. Vesicles of amphiphile chains enclosing their own
//! scale-1 chemistry, floating in a tide pond.
//!
//! Why compartments matter: in the open pond a catalyst helps every chain
//! equally. Inside a membrane it helps only its own vesicle, which then grows
//! and divides faster, and its daughters inherit it. Selection now has a unit.
//!
//! Matter closed over pond + every vesicle (interior + membrane), per letter
//! and for Ka. Membrane is counted as residues by letter (each carries one Ka).
//!
//! Detail = interior runs full scale-1 kinetics (Contents::step). Surrogate =
//! interior kinetics replaced by learned vesicle-level rates (copying, copy
//! fidelity, synthesis, breakage) binned over the vesicle's catalytic profile
//! and environment; chains (the genome) are kept, so heredity survives. Membrane
//! physics, permeation, division and damage run the same in both modes.

use crate::cosmos::{Cosmos, EnvSample};
use crate::hybrid::Mode;
use crate::polymer::N_LETTERS;
use crate::pool::{Chain, Contents, Ctx, PoolParams, PoolStats, Site};
use crate::rng::Rng;
use crate::surrogate::{Axis, Discovery, RateTable};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PondParams {
    /// Chains at or above this amphiphile score can join a membrane.
    pub amph_min: f64,
    /// Per amphiphile chain per day: insertion from inside / adsorption from outside.
    pub insert_rate: f64,
    pub adsorb_rate: f64,
    /// Interior cells per membrane residue^1.5 (sphere: V ~ A^1.5).
    pub vol_k: f64,
    /// Permeability, per membrane residue per day, for raw atoms and monomers.
    pub perm_raw: f64,
    pub perm_mono: f64,
    /// Divisions per day once a vesicle doubled its birth area, scaled up by tidal flow.
    pub div_rate: f64,
    /// Membrane residues lost per residue per day at unit UV.
    pub uv_damage: f64,
    /// Below this membrane size the vesicle bursts.
    pub burst_area: i64,
    /// New vesicles per day per (amphiphile chains)^2 / pond volume.
    pub nucleate_rate: f64,
    pub max_vesicles: usize,
    /// Fraction of incoming light pigment chains can block, at full pigment.
    pub max_shade: f64,
    pub auto: bool,
    pub lift_below: f64,
    pub restrict_above: f64,
    /// Ocean raw-atom concentration per cell (side atoms per letter, then Ka). The tide's food.
    pub ocean_raw: f64,
    pub ocean_ka: f64,
    /// Per day while flooded: fraction of the pond's raw-atom gap to the ocean closed.
    pub flush_rate: f64,
    /// Membrane residues hydrolysing back to pond monomers, per residue per day (wet).
    pub membrane_turnover: f64,
    /// Membrane size past which shape instability splits a vesicle regardless of birth size.
    pub max_area: i64,
    /// Per day while flooded: fraction of pond monomers and chains washed out to sea.
    pub washout_rate: f64,
}

impl Default for PondParams {
    fn default() -> Self {
        Self {
            amph_min: 0.3,
            insert_rate: 2.0,
            adsorb_rate: 0.5,
            vol_k: 1.2,
            perm_raw: 2.0,
            perm_mono: 0.3,
            div_rate: 2.0,
            uv_damage: 0.02,
            burst_area: 12,
            nucleate_rate: 400.0,
            max_vesicles: 160,
            max_shade: 0.8,
            auto: false,
            lift_below: 0.5,
            restrict_above: 0.85,
            ocean_raw: 0.05,
            ocean_ka: 0.25,
            flush_rate: 1.5,
            membrane_turnover: 0.02,
            max_area: 90,
            washout_rate: 0.15,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vesicle {
    pub id: u64,
    pub parent: Option<u64>,
    pub born: f64,
    pub generation: u32,
    pub interior: Contents,
    pub membrane: [i64; N_LETTERS],
    pub birth_area: i64,
    /// Display position in pond units; viewer-only physics.
    pub pos: (f64, f64),
    pub mode: Mode,
    pub conf: f64,
    pub divisions: u32,
}

impl Vesicle {
    pub fn area(&self) -> i64 {
        self.membrane.iter().sum()
    }

    /// Mean pigment of interior chains, 0..1.
    pub fn pigment(&self) -> f64 {
        let n = self.interior.chains.len();
        if n == 0 {
            return 0.0;
        }
        self.interior.chains.iter().map(|c| c.func.pigment).sum::<f64>() / n as f64
    }

    /// Osmotic load per cell from solutes the membrane holds back. Raw atoms cross
    /// freely and equalise, so they pull nothing; monomers leak slowly and count a
    /// little; chains are trapped and count fully. Making polymer is what swells a vesicle.
    pub fn osmotic(&self) -> f64 {
        let i = &self.interior;
        (i.chains.len() as f64 + 0.3 * i.free.iter().sum::<i64>() as f64) / i.cells.max(1.0)
    }

    pub fn amph_frac(&self, amph_min: f64) -> f64 {
        let n = self.interior.chains.len();
        if n == 0 {
            return 0.0;
        }
        self.interior.chains.iter().filter(|c| c.func.amphiphile >= amph_min).count() as f64 / n as f64
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PondStats {
    pub nucleations: u64,
    pub divisions: u64,
    pub bursts: u64,
    pub insertions: u64,
    pub adsorptions: u64,
    pub max_generation: u32,
    pub lifts: u64,
    pub restricts: u64,
    /// Events run by surrogate interiors (synthesis, decay, ligation, join, break, copy).
    pub surrogate_events: u64,
}

/// Vesicle surrogate channels.
pub const V_COPY: usize = 0;
pub const V_MUT: usize = 1;
pub const V_SYNTH: usize = 2;
pub const V_BREAK: usize = 3;
pub const V_LIG: usize = 4;
pub const V_DECAY: usize = 5;
/// Chain-chain joins: exposure ~ n(n-1)/cells, unlike monomer ligation.
pub const V_JOIN: usize = 6;
pub const V_CHANNELS: usize = 7;

pub fn vesicle_table() -> RateTable {
    RateTable::new(
        vec![
            Axis::new("kT", 0.85, 1.15, 4),
            Axis::new("wet", 0.0, 1.0, 2),
            Axis::new("backbone_cut", 0.0, 0.8, 4),
            Axis::new("synthesis_cut", 0.0, 0.8, 4),
            Axis::new("free_monomer_conc", 0.0, 0.6, 4),
            Axis::new("chains_per_kcell", 0.0, 40.0, 4),
        ],
        V_CHANNELS,
    )
}

pub fn vesicle_features(v: &Vesicle, site: &Site) -> [f64; 6] {
    let i = &v.interior;
    let cells = i.cells.max(1.0);
    [
        site.env.kt,
        if site.wet { 1.0 } else { 0.0 },
        i.backbone_cut(),
        i.synthesis_cut(),
        i.free.iter().sum::<i64>() as f64 / cells,
        i.chains.len() as f64 * 1000.0 / cells,
    ]
}

#[derive(Clone)]
pub struct Pond {
    pub ctx: Ctx,
    pub pp: PondParams,
    pub outside: Contents,
    pub vesicles: Vec<Vesicle>,
    pub cosmos: Cosmos,
    pub env: EnvSample,
    pub t_days: f64,
    pub stats: PondStats,
    pub model: RateTable,
    /// Record detailed vesicles into `model` (training on).
    pub learn: bool,
    /// Ocean reservoir of raw atoms (per letter, Ka): part of the closed matter ledger.
    pub ocean: [i64; N_LETTERS],
    pub ocean_ka: i64,
}

/// Move n units with stochastic rounding of an expected flux, clamped to what the source holds.
fn round_flux(expected: f64, rng: &mut Rng) -> i64 {
    let base = expected.trunc();
    let frac = expected - base;
    base as i64 + if rng.chance(frac.abs()) { frac.signum() as i64 } else { 0 }
}

impl Pond {
    pub fn new(p: PoolParams, pp: PondParams, cosmos: Cosmos, rates: crate::surrogate::RateTable, seed: u64) -> Self {
        let env = cosmos.sample(0.25);
        let outside = Contents { free: p.monomers, raw: p.raw, raw_ka: p.raw_ka, cells: p.cells, ..Default::default() };
        // ocean holds far more than the pond ever will: a buffer, not infinite
        let ocean = [(pp.ocean_raw * outside.cells * 200.0) as i64; N_LETTERS];
        let ocean_ka = (pp.ocean_ka * outside.cells * 200.0) as i64;
        Self { ctx: Ctx::new(p, rates, seed), pp, outside, vesicles: Vec::new(), cosmos, env, t_days: 0.25, stats: PondStats::default(), model: vesicle_table(), learn: true, ocean, ocean_ka }
    }

    pub fn site(&self) -> Site {
        Site::at(self.env, self.ctx.p.elevation, 0.0)
    }

    pub fn wet(&self) -> bool {
        self.env.tide_level > self.ctx.p.elevation
    }

    /// Total matter: per letter and Ka, pond plus every vesicle. Must never change.
    pub fn matter(&self) -> ([i64; N_LETTERS], i64) {
        let (mut m, mut ka) = self.outside.matter();
        for l in 0..N_LETTERS {
            m[l] += self.ocean[l];
        }
        ka += self.ocean_ka;
        for v in &self.vesicles {
            let (vm, vka) = v.interior.matter();
            for l in 0..N_LETTERS {
                m[l] += vm[l] + v.membrane[l];
            }
            ka += vka + v.area();
        }
        (m, ka)
    }

    fn vesicle_cells(&self, area: i64) -> f64 {
        self.pp.vol_k * (area.max(1) as f64).powf(1.5)
    }

    pub fn step(&mut self, dt: f64) {
        self.env = self.cosmos.sample(self.t_days);
        let site = self.site();
        self.outside.step(&mut self.ctx, &site, dt);
        let mut rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
        if site.wet {
            self.flush(dt, &mut rng);
        }

        let mut vs = std::mem::take(&mut self.vesicles);
        for v in vs.iter_mut() {
            let shade = self.pp.max_shade * v.pigment();
            let vsite = Site::at(self.env, self.ctx.p.elevation, shade);
            let x = vesicle_features(v, &vsite);
            v.conf = self.model.confidence(&x);
            self.resolve_mode(v, &x, &vsite);
            match v.mode {
                Mode::Detail => {
                    let before = v.interior.stats.clone();
                    let expo = surrogate_exposures(&v.interior);
                    self.ctx.rng = std::mem::replace(&mut rng, Rng::new(0));
                    v.interior.step(&mut self.ctx, &vsite, dt);
                    rng = std::mem::replace(&mut self.ctx.rng, Rng::new(0));
                    if self.learn {
                        record(&mut self.model, &x, &before, &v.interior.stats, &expo, dt);
                    }
                }
                Mode::Surrogate => {
                    let before = v.interior.stats.clone();
                    surrogate_step(&mut v.interior, &self.model, &x, &mut self.ctx, dt, &mut rng);
                    let a = &v.interior.stats;
                    self.stats.surrogate_events += (a.synthesized - before.synthesized) + (a.decayed - before.decayed) + (a.ligations - before.ligations) + (a.breaks - before.breaks) + (a.copies - before.copies);
                }
            }
            self.membrane_and_permeation(v, &vsite, dt, &mut rng);
        }

        // Bursts and divisions after every vesicle stepped.
        let mut next = Vec::with_capacity(vs.len());
        for mut v in vs {
            if v.area() < self.pp.burst_area {
                self.spill(v);
                self.stats.bursts += 1;
                continue;
            }
            let shear = 1.0 + site.env.tide_rate.abs();
            let ready = v.area() >= (2 * v.birth_area).min(self.pp.max_area);
            if ready && rng.chance((self.pp.div_rate * shear * dt).min(1.0)) {
                let (a, b) = self.divide(&mut v, &mut rng);
                next.push(a);
                next.push(b);
                continue;
            }
            next.push(v);
        }
        self.vesicles = next;
        self.adsorb(dt, &mut rng);
        self.nucleate(dt, &site, &mut rng);
        while self.vesicles.len() > self.pp.max_vesicles {
            // crowding: a random vesicle is torn apart. Killing the smallest would kill
            // fresh daughters and select against dividing.
            let k = rng.below(self.vesicles.len());
            let v = self.vesicles.swap_remove(k);
            self.spill(v);
            self.stats.bursts += 1;
        }
        self.ctx.rng = rng;
        self.ctx.resolve(self.t_days);
        self.outside.apply_folds(&self.ctx);
        for v in self.vesicles.iter_mut() {
            v.interior.apply_folds(&self.ctx);
        }
        self.t_days += dt;
    }

    fn resolve_mode(&mut self, v: &mut Vesicle, x: &[f64], site: &Site) {
        match v.mode {
            Mode::Surrogate if v.conf < self.pp.lift_below => {
                v.mode = Mode::Detail;
                self.stats.lifts += 1;
                let (bin, _) = self.model.bin(x);
                self.ctx.discoveries.log(
                    1_000_000 + bin,
                    Discovery {
                        t_days: site.t_days,
                        scale: 2,
                        region: v.id as usize,
                        confidence: v.conf,
                        features: x.to_vec(),
                        note: format!("protocell {} in untrained state (cut {:.2}, synth {:.2})", v.id, x[2], x[3]),
                    },
                );
            }
            Mode::Detail if self.pp.auto && v.conf >= self.pp.restrict_above => {
                v.mode = Mode::Surrogate;
                self.stats.restricts += 1;
                // surrogate does not model duplex state: release strands
                for c in v.interior.chains.iter_mut() {
                    c.paired = false;
                }
            }
            _ => {}
        }
    }

    fn membrane_and_permeation(&mut self, v: &mut Vesicle, site: &Site, dt: f64, rng: &mut Rng) {
        // Insertion: amphiphiles made inside join the membrane.
        let amph_min = self.pp.amph_min;
        let p_ins = (self.pp.insert_rate * dt).min(1.0);
        let mut k = 0;
        while k < v.interior.chains.len() {
            if v.interior.chains[k].func.amphiphile >= amph_min && rng.chance(p_ins) {
                let c = v.interior.chains.swap_remove(k);
                for &l in &c.seq {
                    v.membrane[l as usize] += 1;
                }
                self.stats.insertions += 1;
            } else {
                k += 1;
            }
        }

        // UV erodes the membrane into monomers that fall into the pond; pigment shades it.
        let area = v.area();
        let lose = round_flux(self.pp.uv_damage * site.x[3] * area as f64 * dt, rng).clamp(0, area);
        for _ in 0..lose {
            let l = pick_weighted(&v.membrane, rng);
            v.membrane[l] -= 1;
            self.outside.free[l] += 1;
        }

        // Turnover: wet membranes shed residues as monomers into the pond.
        if site.wet {
            let area = v.area();
            let shed = round_flux(self.pp.membrane_turnover * area as f64 * dt, rng).clamp(0, area);
            for _ in 0..shed {
                let l = pick_weighted(&v.membrane, rng);
                v.membrane[l] -= 1;
                self.outside.free[l] += 1;
            }
        }

        // Permeation: raw atoms and monomers diffuse down the gradient through the membrane.
        let area = v.area() as f64;
        let cells_in = self.vesicle_cells(v.area());
        v.interior.cells = cells_in;
        let cells_out = self.outside.cells.max(1.0);
        let moved = |inside: &mut i64, out: &mut i64, perm: f64, rng: &mut Rng| {
            let flux = perm * area * (*out as f64 / cells_out - *inside as f64 / cells_in) * dt;
            let n = round_flux(flux, rng).clamp(-*inside, *out);
            *inside += n;
            *out -= n;
        };
        for l in 0..N_LETTERS {
            moved(&mut v.interior.raw[l], &mut self.outside.raw[l], self.pp.perm_raw, rng);
            moved(&mut v.interior.free[l], &mut self.outside.free[l], self.pp.perm_mono, rng);
        }
        moved(&mut v.interior.raw_ka, &mut self.outside.raw_ka, self.pp.perm_raw, rng);
    }

    /// High water mixes the pond with the ocean: raw atoms relax toward ocean concentration.
    fn flush(&mut self, dt: f64, rng: &mut Rng) {
        let cells = self.outside.cells;
        let k = (self.pp.flush_rate * dt).min(1.0);
        for l in 0..N_LETTERS {
            let gap = self.pp.ocean_raw * cells - self.outside.raw[l] as f64;
            let n = round_flux(gap * k, rng).clamp(-self.outside.raw[l], self.ocean[l]);
            self.outside.raw[l] += n;
            self.ocean[l] -= n;
        }
        let gap = self.pp.ocean_ka * cells - self.outside.raw_ka as f64;
        let n = round_flux(gap * k, rng).clamp(-self.outside.raw_ka, self.ocean_ka);
        self.outside.raw_ka += n;
        self.ocean_ka -= n;
        // washout: the sea takes monomers and chains, which fall apart into raw atoms there
        let w = (self.pp.washout_rate * dt).min(1.0);
        for l in 0..N_LETTERS {
            let n = round_flux(self.outside.free[l] as f64 * w, rng).clamp(0, self.outside.free[l]);
            self.outside.free[l] -= n;
            self.ocean[l] += n;
            self.ocean_ka += n;
        }
        let mut k = 0;
        while k < self.outside.chains.len() {
            if rng.chance(w) {
                let c = self.outside.chains.swap_remove(k);
                for &l in &c.seq {
                    self.ocean[l as usize] += 1;
                }
                self.ocean_ka += c.seq.len() as i64;
            } else {
                k += 1;
            }
        }
    }

    /// Outside amphiphiles stick to vesicles in proportion to membrane area times
    /// osmotic tension (Chen, Roberts, Szostak 2004): a vesicle swollen by what it
    /// makes inside pulls membrane harder, so internal replication drives growth.
    fn adsorb(&mut self, dt: f64, rng: &mut Rng) {
        if self.vesicles.is_empty() {
            return;
        }
        let osm: Vec<f64> = self.vesicles.iter().map(|v| v.osmotic()).collect();
        let mean_osm = (osm.iter().sum::<f64>() / osm.len() as f64).max(1e-9);
        let pull: Vec<f64> = self.vesicles.iter().zip(&osm).map(|(v, o)| v.area() as f64 * (0.2 + o / mean_osm)).collect();
        let total_pull: f64 = pull.iter().sum();
        let total_area: f64 = self.vesicles.iter().map(|v| v.area() as f64).sum();
        let p = (self.pp.adsorb_rate * dt * (total_area / 2000.0).min(1.0)).min(1.0);
        let mut k = 0;
        while k < self.outside.chains.len() {
            if self.outside.chains[k].func.amphiphile >= self.pp.amph_min && rng.chance(p) {
                let c = self.outside.chains.swap_remove(k);
                let mut r = rng.f64() * total_pull;
                let mut target = self.vesicles.len() - 1;
                for (i, w) in pull.iter().enumerate() {
                    if r < *w {
                        target = i;
                        break;
                    }
                    r -= w;
                }
                for &l in &c.seq {
                    self.vesicles[target].membrane[l as usize] += 1;
                }
                self.stats.adsorptions += 1;
            } else {
                k += 1;
            }
        }
    }

    /// Enough outside amphiphiles meeting assemble a vesicle around a sample of pond water.
    fn nucleate(&mut self, dt: f64, site: &Site, rng: &mut Rng) {
        if self.vesicles.len() >= self.pp.max_vesicles {
            return;
        }
        let amph: Vec<usize> = (0..self.outside.chains.len()).filter(|&k| self.outside.chains[k].func.amphiphile >= self.pp.amph_min).collect();
        if amph.len() < 4 {
            return;
        }
        let expected = self.pp.nucleate_rate * (amph.len() as f64).powi(2) / self.outside.cells * dt;
        for _ in 0..rng.poisson(expected) {
            let amph: Vec<usize> = (0..self.outside.chains.len()).filter(|&k| self.outside.chains[k].func.amphiphile >= self.pp.amph_min).collect();
            if amph.len() < 4 || self.vesicles.len() >= self.pp.max_vesicles {
                break;
            }
            let mut membrane = [0i64; N_LETTERS];
            let mut take: Vec<usize> = (0..4).map(|_| amph[rng.below(amph.len())]).collect();
            take.sort_unstable();
            take.dedup();
            for &k in take.iter().rev() {
                let c = self.outside.chains.swap_remove(k);
                for &l in &c.seq {
                    membrane[l as usize] += 1;
                }
            }
            let area: i64 = membrane.iter().sum();
            if area < self.pp.burst_area {
                for l in 0..N_LETTERS {
                    self.outside.free[l] += membrane[l];
                }
                continue;
            }
            let cells = self.vesicle_cells(area);
            let frac = (cells / self.outside.cells).min(0.5);
            let mut interior = Contents { cells, ..Default::default() };
            let mut k = 0;
            while k < self.outside.chains.len() {
                if rng.chance(frac) {
                    interior.chains.push(self.outside.chains.swap_remove(k));
                } else {
                    k += 1;
                }
            }
            for l in 0..N_LETTERS {
                let n = round_flux(self.outside.free[l] as f64 * frac, rng).clamp(0, self.outside.free[l]);
                self.outside.free[l] -= n;
                interior.free[l] += n;
                let n = round_flux(self.outside.raw[l] as f64 * frac, rng).clamp(0, self.outside.raw[l]);
                self.outside.raw[l] -= n;
                interior.raw[l] += n;
            }
            let n = round_flux(self.outside.raw_ka as f64 * frac, rng).clamp(0, self.outside.raw_ka);
            self.outside.raw_ka -= n;
            interior.raw_ka += n;
            let id = self.ctx.next_id();
            self.vesicles.push(Vesicle {
                id,
                parent: None,
                born: site.t_days,
                generation: 0,
                interior,
                membrane,
                birth_area: area,
                pos: (rng.f64() * 220.0, rng.f64() * 140.0),
                mode: Mode::Detail,
                conf: 0.0,
                divisions: 0,
            });
            self.stats.nucleations += 1;
        }
    }

    fn spill(&mut self, v: Vesicle) {
        let Vesicle { interior, membrane, .. } = v;
        for l in 0..N_LETTERS {
            self.outside.free[l] += interior.free[l] + membrane[l];
            self.outside.raw[l] += interior.raw[l];
        }
        self.outside.raw_ka += interior.raw_ka;
        self.outside.chains.extend(interior.chains);
    }

    /// Binary fission: membrane and contents split at random (binomial), chains whole.
    fn divide(&mut self, v: &mut Vesicle, rng: &mut Rng) -> (Vesicle, Vesicle) {
        let mut halves = [Contents::default(), Contents::default()];
        let mut mem = [[0i64; N_LETTERS]; 2];
        for c in std::mem::take(&mut v.interior.chains) {
            halves[rng.below(2)].chains.push(c);
        }
        let split = |n: i64, rng: &mut Rng| -> i64 { (0..n).filter(|_| rng.chance(0.5)).count() as i64 };
        for l in 0..N_LETTERS {
            let a = split(v.interior.free[l], rng);
            halves[0].free[l] = a;
            halves[1].free[l] = v.interior.free[l] - a;
            let a = split(v.interior.raw[l], rng);
            halves[0].raw[l] = a;
            halves[1].raw[l] = v.interior.raw[l] - a;
            let a = split(v.membrane[l], rng);
            mem[0][l] = a;
            mem[1][l] = v.membrane[l] - a;
        }
        let a = split(v.interior.raw_ka, rng);
        halves[0].raw_ka = a;
        halves[1].raw_ka = v.interior.raw_ka - a;
        self.stats.divisions += 1;
        let generation = v.generation + 1;
        self.stats.max_generation = self.stats.max_generation.max(generation);
        let [h0, h1] = halves;
        let mk = |pond: &mut Pond, interior: Contents, membrane: [i64; N_LETTERS], dx: f64| {
            let area: i64 = membrane.iter().sum();
            let mut interior = interior;
            interior.cells = pond.vesicle_cells(area);
            Vesicle {
                id: pond.ctx.next_id(),
                parent: Some(v.id),
                born: pond.t_days,
                generation,
                interior,
                membrane,
                birth_area: area.max(1),
                pos: (v.pos.0 + dx, v.pos.1),
                mode: v.mode,
                conf: v.conf,
                divisions: 0,
            }
        };
        let a = mk(self, h0, mem[0], -2.0);
        let b = mk(self, h1, mem[1], 2.0);
        (a, b)
    }

    pub fn mean_vesicle_cut(&self) -> f64 {
        if self.vesicles.is_empty() {
            return 0.0;
        }
        self.vesicles.iter().map(|v| v.interior.backbone_cut()).sum::<f64>() / self.vesicles.len() as f64
    }

    pub fn total_interior_stats(&self) -> PoolStats {
        let mut s = PoolStats::default();
        for v in &self.vesicles {
            s.add(&v.interior.stats);
        }
        s
    }
}

fn pick_weighted(counts: &[i64; N_LETTERS], rng: &mut Rng) -> usize {
    let total: i64 = counts.iter().sum();
    let mut r = rng.below(total.max(1) as usize) as i64;
    for (l, &c) in counts.iter().enumerate() {
        if r < c {
            return l;
        }
        r -= c;
    }
    (0..N_LETTERS).rev().find(|&l| counts[l] > 0).unwrap_or(0)
}

/// Exposure of each surrogate channel at the start of a detailed step (per day).
/// Mass action, matching how the detailed model drives each process.
fn surrogate_exposures(c: &Contents) -> [f64; V_CHANNELS] {
    let residues: i64 = c.chains.iter().map(|ch| ch.seq.len() as i64).sum();
    let cells = c.cells.max(1.0);
    let free: i64 = c.free.iter().sum();
    let synth = c.raw_ka as f64 * c.raw.iter().sum::<i64>() as f64 / cells;
    let lig = free as f64 * (2.0 * c.chains.len() as f64 + free as f64) / cells;
    let n = c.chains.len() as f64;
    let join = 2.0 * n * (n - 1.0).max(0.0) / cells;
    [c.chains.len() as f64, 0.0, synth, residues as f64, lig, free as f64, join]
}

fn record(t: &mut RateTable, x: &[f64], before: &PoolStats, after: &PoolStats, expo: &[f64; V_CHANNELS], dt: f64) {
    t.record_sample(x);
    t.record(x, V_COPY, (after.copies - before.copies) as f64, expo[V_COPY] * dt);
    // fidelity: mutations per copied letter
    t.record(x, V_MUT, (after.mutations - before.mutations) as f64, (after.copied_letters - before.copied_letters) as f64);
    t.record(x, V_SYNTH, (after.synthesized - before.synthesized) as f64, expo[V_SYNTH] * dt);
    t.record(x, V_DECAY, (after.decayed - before.decayed) as f64, expo[V_DECAY] * dt);
    t.record(x, V_BREAK, (after.breaks - before.breaks) as f64, expo[V_BREAK] * dt);
    let joins = (after.joins - before.joins) as f64;
    t.record(x, V_LIG, (after.ligations - before.ligations) as f64 - joins, expo[V_LIG] * dt);
    t.record(x, V_JOIN, joins, expo[V_JOIN] * dt);
}

/// Learned interior: copies with learned fidelity, net synthesis, breakage. Matter closed.
fn surrogate_step(c: &mut Contents, t: &RateTable, x: &[f64], ctx: &mut Ctx, dt: f64, rng: &mut Rng) {
    let rate = |ch: usize| t.rate(x, ch, t.global_rate(ch), 1.0);
    let expo = surrogate_exposures(c);
    // synthesis: raw Ka + side atom -> monomer, letter drawn by raw availability
    let make = round_flux(rate(V_SYNTH) * expo[V_SYNTH] * dt, rng).max(0);
    for _ in 0..make {
        if c.raw_ka == 0 || c.raw.iter().sum::<i64>() == 0 {
            break;
        }
        let l = pick_weighted(&c.raw, rng);
        c.raw[l] -= 1;
        c.raw_ka -= 1;
        c.free[l] += 1;
        c.stats.synthesized += 1;
    }
    // decay: monomers fall apart into raw atoms
    let lose = round_flux(rate(V_DECAY) * expo[V_DECAY] * dt, rng).max(0);
    for _ in 0..lose {
        if c.free.iter().sum::<i64>() == 0 {
            break;
        }
        let l = pick_weighted(&c.free, rng);
        c.free[l] -= 1;
        c.raw[l] += 1;
        c.raw_ka += 1;
        c.stats.decayed += 1;
    }
    // ligation: a monomer joins a chain end, or two monomers make a dimer
    let joins = round_flux(rate(V_LIG) * expo[V_LIG] * dt, rng).max(0);
    for _ in 0..joins {
        let free: i64 = c.free.iter().sum();
        if free == 0 {
            break;
        }
        let l = pick_weighted(&c.free, rng);
        c.free[l] -= 1;
        let target = rng.below(c.chains.len() + free as usize);
        if target < c.chains.len() && c.chains[target].seq.len() >= ctx.p.max_len {
            // full-length chain cannot take a letter: no event, as in detail
            c.free[l] += 1;
            continue;
        }
        if target < c.chains.len() {
            let old = c.chains.swap_remove(target);
            let mut seq = old.seq.clone();
            seq.push(l as u8);
            let (func, conf, pending) = ctx.function_for(&seq, x[0]);
            let id = ctx.next_id();
            c.chains.push(Chain { id, seq, parent: Some(old.id), born: old.born, paired: false, func, conf, generation: old.generation, pending });
            c.stats.ligations += 1;
        } else if c.free.iter().sum::<i64>() > 0 {
            let m = pick_weighted(&c.free, rng);
            c.free[m] -= 1;
            let seq = vec![l as u8, m as u8];
            let (func, conf, pending) = ctx.function_for(&seq, x[0]);
            let id = ctx.next_id();
            c.chains.push(Chain { id, seq, parent: None, born: 0.0, paired: false, func, conf, generation: 0, pending });
            c.stats.ligations += 1;
        } else {
            c.free[l] += 1;
        }
    }
    // chain-chain joins
    for _ in 0..round_flux(rate(V_JOIN) * expo[V_JOIN] * dt, rng).max(0) {
        let len = c.chains.len();
        if len < 2 {
            break;
        }
        let (i, j) = (rng.below(len), rng.below(len));
        if i == j || c.chains[i].seq.len() + c.chains[j].seq.len() > ctx.p.max_len {
            continue;
        }
        let (hi, lo) = (i.max(j), i.min(j));
        let a = c.chains.swap_remove(hi);
        let b = c.chains.swap_remove(lo);
        let mut seq = a.seq.clone();
        seq.extend_from_slice(&b.seq);
        let (func, conf, pending) = ctx.function_for(&seq, x[0]);
        let id = ctx.next_id();
        c.chains.push(Chain { id, seq, parent: Some(a.id), born: a.born, paired: false, func, conf, generation: a.generation.max(b.generation), pending });
        c.stats.ligations += 1;
        c.stats.joins += 1;
    }
    // breakage: random backbone bonds; pieces of one letter become monomers
    let residues: i64 = c.chains.iter().map(|ch| ch.seq.len() as i64).sum();
    let breaks = round_flux(rate(V_BREAK) * residues as f64 * dt, rng).max(0);
    for _ in 0..breaks {
        if c.chains.is_empty() {
            break;
        }
        let k = rng.below(c.chains.len());
        if c.chains[k].seq.len() < 2 {
            continue;
        }
        let ch = c.chains.swap_remove(k);
        let cut = 1 + rng.below(ch.seq.len() - 1);
        c.stats.breaks += 1;
        for piece in [ch.seq[..cut].to_vec(), ch.seq[cut..].to_vec()] {
            if piece.len() == 1 {
                c.free[piece[0] as usize] += 1;
            } else {
                // a fragment has its own fold: never the parent's function
                let (func, conf, pending) = ctx.function_for(&piece, x[0]);
                let id = ctx.next_id();
                c.chains.push(Chain { id, seq: piece, parent: Some(ch.id), born: ch.born, paired: false, func, conf, generation: ch.generation, pending });
            }
        }
    }
    // copying: reverse complement with learned per-letter error, needs monomers
    let p_copy = (rate(V_COPY) * dt).min(1.0);
    let p_mut = rate(V_MUT).clamp(0.0, 1.0);
    let n = c.chains.len();
    for k in 0..n {
        if !rng.chance(p_copy) {
            continue;
        }
        let template = c.chains[k].seq.clone();
        let mut copy = Vec::with_capacity(template.len());
        for &tl in template.iter().rev() {
            let best = (0..N_LETTERS).max_by(|&a, &b| ctx.ph.pair[tl as usize][a].total_cmp(&ctx.ph.pair[tl as usize][b])).unwrap();
            let want = if rng.chance(p_mut) { (best + 1 + rng.below(N_LETTERS - 1)) % N_LETTERS } else { best };
            if c.free[want] == 0 {
                break;
            }
            c.free[want] -= 1;
            copy.push(want as u8);
        }
        if copy.len() < 2 {
            for l in copy {
                c.free[l as usize] += 1;
            }
            continue;
        }
        let (func, conf, pending) = ctx.function_for(&copy, x[0]);
        let parent = c.chains[k].id;
        let generation = c.chains[k].generation + 1;
        let id = ctx.next_id();
        c.chains.push(Chain { id, seq: copy, parent: Some(parent), born: 0.0, paired: false, func, conf, generation, pending });
        c.stats.copies += 1;
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
    fn pond_matter_closed_and_vesicles_form() {
        let mut pond = Pond::new(PoolParams::default(), PondParams::default(), Cosmos::new(CosmosParams::default()), rates(), 2);
        let start = pond.matter();
        for k in 0..700 {
            pond.step(0.01);
            if k % 50 == 0 {
                assert_eq!(pond.matter(), start, "matter at t={}", pond.t_days);
            }
        }
        assert_eq!(pond.matter(), start);
        assert!(pond.stats.nucleations > 0, "{:?}", pond.stats);
    }

    #[test]
    fn surrogate_mode_keeps_matter() {
        let mut pond = Pond::new(PoolParams::default(), PondParams { auto: true, ..Default::default() }, Cosmos::new(CosmosParams::default()), rates(), 3);
        let start = pond.matter();
        for _ in 0..900 {
            pond.step(0.01);
        }
        assert_eq!(pond.matter(), start);
        assert!(pond.stats.surrogate_events > 0, "surrogate never ran an event: test would pass vacuously");
    }
}
