//! Adaptive resolution for scale 0: the beach is cut into elevation bands,
//! each running either the lattice (detail) or the mass-action surrogate.
//!
//! restrict(band): atoms freeze in place, internal bonds collapse to counts.
//! lift(band): counts are written back onto the frozen atoms by breaking and
//!   forming real bonds between adjacent atoms. Geometry can make a target
//!   count unreachable; the shortfall is `lift_shortfall`, part of the 20%.
//! Ledger stays exact: surrogate bands contribute their counted bond energy,
//! and every lift edit is booked like any other bond event.
//!
//! Bonds that cross a band edge are frozen while either side is surrogate.

use crate::chem::ChemWorld;
use crate::element::Energy;
use crate::kinetics::{self, BandState, CH_BREAK, CH_FORM, N_PAIRS, Tally, bidx, exch_channel, pair, pair_members};
use crate::rng::Rng;
use crate::surrogate::{Discovery, DiscoveryLog, RateTable};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Detail,
    Surrogate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridParams {
    pub bands: usize,
    /// Sweeps per surrogate tau-leap and per tally flush.
    pub leap: u64,
    /// Below this confidence a surrogate band lifts to detail and logs a discovery.
    pub lift_below: f64,
    /// At or above this, an unpinned detail band may restrict (when auto is on).
    pub restrict_above: f64,
    pub auto: bool,
}

impl Default for HybridParams {
    fn default() -> Self {
        Self { bands: 8, leap: 10, lift_below: 0.5, restrict_above: 0.85, auto: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Hybrid {
    pub hp: HybridParams,
    pub mode: Vec<Mode>,
    pub state: Vec<BandState>,
    /// Forced to detail (viewer zoom, experiments).
    pub pinned: Vec<bool>,
    pub confidence: Vec<f64>,
    /// Atoms the lattice steps (those in detail bands).
    pub active: Vec<u32>,
    pub tallies: Option<Vec<Tally>>,
    pub rates: Option<RateTable>,
    pub discoveries: DiscoveryLog,
    pub lift_shortfall: i64,
    pub lifts: u64,
    pub restricts: u64,
    pub(crate) rng: Option<Rng>,
}

impl Hybrid {
    pub fn new(hp: HybridParams, n_atoms: usize) -> Self {
        let b = hp.bands;
        Self {
            mode: vec![Mode::Detail; b],
            state: vec![BandState::default(); b],
            pinned: vec![false; b],
            confidence: vec![1.0; b],
            active: (0..n_atoms as u32).collect(),
            rng: Some(Rng::new(0x5EED)),
            hp,
            ..Default::default()
        }
    }
}

impl ChemWorld {
    #[inline]
    pub fn band_of(&self, x: u16) -> usize {
        x as usize * self.hybrid.hp.bands / self.p.width as usize
    }

    #[inline]
    pub fn frozen(&self, i: u32) -> bool {
        self.hybrid.mode[self.band_of(self.atoms[i as usize].x)] == Mode::Surrogate
    }

    #[inline]
    pub fn cell_frozen(&self, x: i32) -> bool {
        self.hybrid.mode[self.band_of(x as u16)] == Mode::Surrogate
    }

    pub fn band_columns(&self, b: usize) -> (u16, u16) {
        let (w, n) = (self.p.width as usize, self.hybrid.hp.bands);
        (((b * w).div_ceil(n)) as u16, (((b + 1) * w).div_ceil(n)) as u16)
    }

    /// Features the surrogate sees for a band: kT, wet fraction, light and UV after water attenuation.
    pub fn band_features(&self, b: usize) -> [f64; 4] {
        let (x0, x1) = self.band_columns(b);
        let n = (x1 - x0) as f64;
        let (mut wet, mut vis, mut uv) = (0.0, 0.0, 0.0);
        for x in x0..x1 {
            let d = self.depth(x as i32).max(0.0);
            if self.depth(x as i32) > 0.0 {
                wet += 1.0;
            }
            vis += (-self.p.vis_depth * d).exp();
            uv += (-self.p.uv_depth * d).exp();
        }
        kinetics::features(self.env.kt, wet / n, self.env.visible * vis / n, self.env.uv * uv / n)
    }

    /// Count a band's internal bonds and free slots from the atoms.
    pub fn band_state_from_atoms(&self, b: usize) -> BandState {
        let (x0, x1) = self.band_columns(b);
        let mut s = BandState { cells: ((x1 - x0) as usize * self.p.height as usize) as f64, ..Default::default() };
        for (i, a) in self.atoms.iter().enumerate() {
            if self.band_of(a.x) != b {
                continue;
            }
            let Some(ia) = bidx(a.z) else { continue };
            s.free[ia] += (self.table.get(a.z).valence - a.nb) as i64;
            for &j in &a.bonds[..a.nb as usize] {
                let o = &self.atoms[j as usize];
                if (j as usize) > i && self.band_of(o.x) == b {
                    s.bonds[pair(ia, bidx(o.z).unwrap())] += 1;
                }
            }
        }
        s
    }

    pub(crate) fn rebuild_active(&mut self) {
        let active: Vec<u32> = (0..self.atoms.len() as u32).filter(|&i| !self.frozen(i)).collect();
        self.hybrid.active = active;
    }

    pub fn enable_tally(&mut self) {
        self.hybrid.tallies = Some(vec![Tally::default(); self.hybrid.hp.bands]);
    }

    /// Hook from the lattice: one event on `ch`, attributed to the band of atom i.
    /// Skipped unless both atoms share a band, matching what band state counts.
    #[inline]
    pub(crate) fn tally(&mut self, ch: usize, i: u32, j: u32) {
        if self.hybrid.tallies.is_none() {
            return;
        }
        let (bi, bj) = (self.band_of(self.atoms[i as usize].x), self.band_of(self.atoms[j as usize].x));
        if bi == bj {
            self.hybrid.tallies.as_mut().unwrap()[bi].events[ch] += 1.0;
        }
    }

    pub(crate) fn tally_form(&mut self, i: u32, j: u32) {
        if self.hybrid.tallies.is_some() {
            let p = pair(bidx(self.atoms[i as usize].z).unwrap(), bidx(self.atoms[j as usize].z).unwrap());
            self.tally(CH_FORM + p, i, j);
        }
    }

    pub(crate) fn tally_break(&mut self, i: u32, j: u32) {
        if self.hybrid.tallies.is_some() {
            let p = pair(bidx(self.atoms[i as usize].z).unwrap(), bidx(self.atoms[j as usize].z).unwrap());
            self.tally(CH_BREAK + p, i, j);
        }
    }

    /// a attacks b, c leaves. Attributed to a's band; c must share it too.
    pub(crate) fn tally_exchange(&mut self, a: u32, b: u32, c: u32) {
        if self.hybrid.tallies.is_some() && self.band_of(self.atoms[c as usize].x) == self.band_of(self.atoms[a as usize].x) {
            let z = |k: u32| bidx(self.atoms[k as usize].z).unwrap();
            let ch = exch_channel(z(a), z(b), z(c));
            self.tally(ch, a, b);
        }
    }

    /// Move tallies into the rate table, exposure from a state snapshot times sweeps.
    pub fn flush_tallies(&mut self, table: &mut RateTable, sweeps: f64) {
        let Some(tallies) = self.hybrid.tallies.take() else { return };
        for (b, t) in tallies.iter().enumerate() {
            if self.hybrid.mode[b] != Mode::Detail {
                continue;
            }
            let x = self.band_features(b);
            let s = self.band_state_from_atoms(b);
            table.record_sample(&x);
            for ch in 0..kinetics::N_CHANNELS {
                let e = s.exposure(ch) * sweeps;
                if e > 0.0 || t.events[ch] > 0.0 {
                    table.record(&x, ch, t.events[ch], e);
                }
            }
        }
        self.enable_tally();
    }

    pub fn restrict(&mut self, b: usize) {
        if self.hybrid.mode[b] == Mode::Surrogate {
            return;
        }
        // excitations do not survive coarse-graining: dump to heat
        for i in 0..self.atoms.len() {
            if self.band_of(self.atoms[i].x) == b && self.atoms[i].excite > 0 {
                let x = self.atoms[i].excite;
                self.atoms[i].excite = 0;
                self.book(-x);
            }
        }
        self.hybrid.state[b] = self.band_state_from_atoms(b);
        self.hybrid.mode[b] = Mode::Surrogate;
        self.hybrid.restricts += 1;
        self.rebuild_active();
    }

    pub fn lift(&mut self, b: usize) {
        if self.hybrid.mode[b] == Mode::Detail {
            return;
        }
        let target = self.hybrid.state[b].clone();
        // ledger: band was counted at target.stored(); atoms now carry the real bonds
        let counted = target.stored(&self.table);
        self.hybrid.mode[b] = Mode::Detail;
        let mut rng = self.hybrid.rng.take().unwrap();
        let actual = self.band_state_from_atoms(b);
        let atoms_stored = actual.stored(&self.table);
        // switching accounting from counts to atoms is not an event: settle the difference
        self.book(atoms_stored - counted);
        let internal = |w: &ChemWorld, i: usize, j: u32| w.band_of(w.atoms[j as usize].x) == b && (j as usize) > i;

        // breaks first, they free valence for forms
        for p in 0..N_PAIRS {
            let surplus = actual.bonds[p] - target.bonds[p];
            if surplus <= 0 {
                continue;
            }
            let mut cands = Vec::new();
            for (i, a) in self.atoms.iter().enumerate() {
                if self.band_of(a.x) != b || bidx(a.z).is_none() {
                    continue;
                }
                for &j in &a.bonds[..a.nb as usize] {
                    if internal(self, i, j) && pair(bidx(a.z).unwrap(), bidx(self.atoms[j as usize].z).unwrap()) == p {
                        cands.push((i as u32, j));
                    }
                }
            }
            shuffle(&mut cands, &mut rng);
            for &(i, j) in cands.iter().take(surplus as usize) {
                let e = self.table.bond[self.atoms[i as usize].z as usize][self.atoms[j as usize].z as usize];
                self.unlink(i, j);
                self.book(e);
            }
        }
        for p in 0..N_PAIRS {
            let mut deficit = target.bonds[p] - actual.bonds[p];
            if deficit <= 0 {
                continue;
            }
            let (pa, pb) = pair_members(p);
            let mut cands = Vec::new();
            for (i, a) in self.atoms.iter().enumerate() {
                if self.band_of(a.x) != b || bidx(a.z) != Some(pa) {
                    continue;
                }
                for (dx, dy) in crate::chem::DIRS {
                    if let Some(j) = self.atom_at(a.x as i32 + dx, a.y as i32 + dy) {
                        let o = &self.atoms[j as usize];
                        if self.band_of(o.x) == b && bidx(o.z) == Some(pb) && (pa != pb || j as usize > i) {
                            cands.push((i as u32, j));
                        }
                    }
                }
            }
            shuffle(&mut cands, &mut rng);
            for (i, j) in cands {
                if deficit == 0 {
                    break;
                }
                let (a, o) = (self.atoms[i as usize], self.atoms[j as usize]);
                if self.free_valence(i) == 0 || self.free_valence(j) == 0 || a.bonded_to(j) || a.weak == j {
                    continue;
                }
                self.link(i, j);
                self.book(-self.table.bond[a.z as usize][o.z as usize]);
                deficit -= 1;
            }
            self.hybrid.lift_shortfall += deficit;
        }
        self.hybrid.rng = Some(rng);
        self.hybrid.lifts += 1;
        self.rebuild_active();
    }

    /// Advance surrogate bands one leap; lift any whose confidence collapsed.
    pub(crate) fn step_surrogates(&mut self) {
        let Some(rates) = self.hybrid.rates.take() else { return };
        let mut rng = self.hybrid.rng.take().unwrap();
        for b in 0..self.hybrid.hp.bands {
            let x = self.band_features(b);
            let conf = rates.confidence(&x);
            self.hybrid.confidence[b] = conf;
            match self.hybrid.mode[b] {
                Mode::Surrogate if conf < self.hybrid.hp.lift_below || self.hybrid.pinned[b] => {
                    if !self.hybrid.pinned[b] {
                        let (bin, _) = rates.bin(&x);
                        self.hybrid.discoveries.log(
                            bin,
                            Discovery {
                                t_days: self.t_days,
                                scale: 0,
                                region: b,
                                confidence: conf,
                                features: x.to_vec(),
                                note: "chemistry outside trained conditions".into(),
                            },
                        );
                    }
                    self.hybrid.rng = Some(rng);
                    self.lift(b);
                    rng = self.hybrid.rng.take().unwrap();
                }
                Mode::Surrogate => {
                    let mut s = std::mem::take(&mut self.hybrid.state[b]);
                    let d = kinetics::step_band(&mut s, &rates, &self.table, &x, self.hybrid.hp.leap as f64, &mut rng);
                    self.hybrid.state[b] = s;
                    self.book(d);
                }
                Mode::Detail if self.hybrid.hp.auto && !self.hybrid.pinned[b] && conf >= self.hybrid.hp.restrict_above => {
                    self.restrict(b);
                }
                Mode::Detail => {}
            }
        }
        self.hybrid.rng = Some(rng);
        self.hybrid.rates = Some(rates);
    }

    /// Stored energy of the surrogate bands' counted bonds (ledger cross-check).
    pub(crate) fn surrogate_stored(&self) -> Energy {
        (0..self.hybrid.hp.bands).filter(|&b| self.hybrid.mode[b] == Mode::Surrogate).map(|b| self.hybrid.state[b].stored(&self.table)).sum()
    }

    pub fn surrogate_bonds(&self) -> i64 {
        (0..self.hybrid.hp.bands).filter(|&b| self.hybrid.mode[b] == Mode::Surrogate).map(|b| self.hybrid.state[b].total_bonds()).sum()
    }
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        v.swap(i, rng.below(i + 1));
    }
}

/// Run a detailed world with tallies on and learn a rate table. `days` should span a year for kT coverage.
pub fn train(world: &mut ChemWorld, days: f64, mut on_progress: impl FnMut(f64)) -> RateTable {
    let mut table = kinetics::new_table();
    world.enable_tally();
    let leap = world.hybrid.hp.leap;
    let total = (days * world.p.sweeps_per_day) as u64;
    for s in 1..=total {
        world.sweep();
        if s % leap == 0 {
            world.flush_tallies(&mut table, leap as f64);
        }
        if s % (total / 20).max(1) == 0 {
            on_progress(s as f64 / total as f64);
        }
    }
    world.hybrid.tallies = None;
    table
}
