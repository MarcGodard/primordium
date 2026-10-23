//! Scale 0: lattice artificial chemistry. Ground truth for everything above.
//!
//! Tidal-pool slice: x runs up a beach (elevation = x / width), y wraps along
//! the shore. Cells below the tide line are wet. One atom per cell; covalent
//! and weak bonds join 8-neighbours only, so a bond is also a spatial contact.
//!
//! Matter is closed: atoms are never created, destroyed or transmuted.
//! Energy flows through: photons in, heat out to an infinite bath. Ledger
//! invariant (exact, integer): stored + bath - photon_in == stored_at_start.
//! Hydrolysis, solvation and tidal drift are free-energy/kinetic terms of the
//! water, not stored energy, so they bias acceptance but never touch the ledger.

use crate::cosmos::{Cosmos, CosmosParams, EnvSample};
use crate::element::{Energy, KT, N_ELEMENTS, Table};
use crate::hybrid::{Hybrid, HybridParams, Mode};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const NONE: u32 = u32::MAX;
pub const MAX_BONDS: usize = crate::element::MAX_VALENCE as usize;

/// 8-neighbourhood, index order fixed: viewer and tests rely on it.
pub const DIRS: [(i32, i32); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Atom {
    pub z: u8,
    pub nb: u8,
    pub x: u16,
    pub y: u16,
    pub bonds: [u32; MAX_BONDS],
    pub weak: u32,
    /// Captured photon energy, milli-units. 0 = ground state.
    pub excite: Energy,
}

impl Atom {
    #[inline]
    pub fn bonded_to(&self, j: u32) -> bool {
        self.bonds[..self.nb as usize].contains(&j)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChemParams {
    pub width: u16,
    pub height: u16,
    /// Fraction of cells holding an atom.
    pub density: f64,
    /// Relative abundance of Z = 1..=8.
    pub composition: [f64; N_ELEMENTS],
    /// Monte-Carlo sweeps per simulated day. Sets the time-lapse.
    pub sweeps_per_day: f64,
    /// Activation barriers, kT at 300 K. Arrhenius: warmer reacts faster.
    pub barrier_form: f64,
    pub barrier_exchange: f64,
    /// Water's pull toward hydrolysis in wet cells, kT.
    pub hydrolysis: f64,
    /// Scales element hydropathy into contact free energy, kT per contact.
    pub solvation: f64,
    /// Move acceptance multiplier in dry cells: drying strands things.
    pub dry_mobility: f64,
    /// Drift bias, kT per lattice step at unit tide rate.
    pub flow_force: f64,
    /// Photon arrivals per atom per day at flux 1.0.
    pub vis_rate: f64,
    pub uv_rate: f64,
    pub cosmic_rate: f64,
    pub radio_rate: f64,
    /// Photon energies, kT.
    pub vis_energy: f64,
    pub uv_energy: f64,
    pub cosmic_energy: f64,
    pub radio_energy: f64,
    /// Attenuation per unit depth (beach-height units) of water.
    pub vis_depth: f64,
    pub uv_depth: f64,
    /// Per-sweep chance an excited atom dumps to heat / hands off to a bonded neighbour.
    pub excite_decay: f64,
    pub excite_transfer: f64,
}

impl Default for ChemParams {
    fn default() -> Self {
        Self {
            width: 192,
            height: 128,
            density: 0.33,
            //            Hy    Ne    Li    Bo    Ka    Ox    Fl    Ar
            composition: default_composition(),
            sweeps_per_day: 400.0,
            barrier_form: 3.0,
            barrier_exchange: 1.5,
            hydrolysis: 3.0,
            solvation: 0.6,
            dry_mobility: 0.08,
            flow_force: 0.35,
            vis_rate: 4.0,
            uv_rate: 0.15,
            cosmic_rate: 0.004,
            radio_rate: 1.0,
            vis_energy: 5.0,
            uv_energy: 12.0,
            cosmic_energy: 40.0,
            radio_energy: 0.3,
            vis_depth: 1.5,
            uv_depth: 8.0,
            excite_decay: 0.05,
            excite_transfer: 0.2,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub stored: Energy,
    pub bath: Energy,
    pub photon_in: Energy,
    pub stored_at_start: Energy,
}

impl Ledger {
    pub fn drift(&self) -> Energy {
        self.stored + self.bath - self.photon_in - self.stored_at_start
    }
}

/// Event tallies since the last `take_counts`. Feeds surrogate training.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Counts {
    pub moves: u64,
    pub forms: u64,
    pub breaks: u64,
    pub exchanges: u64,
    pub weak_forms: u64,
    pub weak_breaks: u64,
    pub photo_excite: u64,
    pub uv_breaks: u64,
    pub cosmic_hits: u64,
    pub excite_used: u64,
}

#[derive(Clone, Debug)]
pub struct ChemWorld {
    pub p: ChemParams,
    pub table: Table,
    pub cosmos: Cosmos,
    pub atoms: Vec<Atom>,
    pub(crate) grid: Vec<u32>,
    pub(crate) rng: Rng,
    pub t_days: f64,
    pub sweeps: u64,
    pub env: EnvSample,
    pub ledger: Ledger,
    pub counts: Counts,
    pub hybrid: Hybrid,
}

impl ChemWorld {
    pub fn new(p: ChemParams, cp: CosmosParams, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let (w, h) = (p.width as usize, p.height as usize);
        let mut grid = vec![NONE; w * h];
        let total: f64 = p.composition.iter().sum();
        let n = ((w * h) as f64 * p.density) as usize;
        let mut atoms = Vec::with_capacity(n);
        while atoms.len() < n {
            let c = rng.below(w * h);
            if grid[c] != NONE {
                continue;
            }
            let mut r = rng.f64() * total;
            let mut z = 1u8;
            for (k, &f) in p.composition.iter().enumerate() {
                if r < f {
                    z = k as u8 + 1;
                    break;
                }
                r -= f;
            }
            grid[c] = atoms.len() as u32;
            atoms.push(Atom { z, nb: 0, x: (c % w) as u16, y: (c / w) as u16, bonds: [NONE; 3], weak: NONE, excite: 0 });
        }
        let cosmos = Cosmos::new(cp);
        let t0 = 0.25; // dawn of day 0
        let env = cosmos.sample(t0);
        let mut world = Self {
            p,
            table: Table::new(),
            cosmos,
            atoms,
            grid,
            rng,
            t_days: t0,
            sweeps: 0,
            env,
            ledger: Ledger::default(),
            counts: Counts::default(),
            hybrid: Hybrid::default(),
        };
        world.hybrid = Hybrid::new(HybridParams::default(), world.atoms.len());
        world.ledger.stored = world.recompute_stored();
        world.ledger.stored_at_start = world.ledger.stored;
        world
    }

    #[inline]
    fn cell(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || x >= self.p.width as i32 {
            return None;
        }
        let h = self.p.height as i32;
        let y = y.rem_euclid(h);
        Some(y as usize * self.p.width as usize + x as usize)
    }

    #[inline]
    fn adjacent(&self, a: &Atom, x: i32, y: i32) -> bool {
        let dx = (a.x as i32 - x).abs();
        let h = self.p.height as i32;
        let dy = (a.y as i32 - y).rem_euclid(h);
        dx <= 1 && (dy <= 1 || dy == h - 1)
    }

    /// Elevation of column x, 0..1 up the beach.
    #[inline]
    pub fn elevation(&self, x: i32) -> f64 {
        (x as f64 + 0.5) / self.p.width as f64
    }

    #[inline]
    pub fn depth(&self, x: i32) -> f64 {
        self.env.tide_level - self.elevation(x)
    }

    #[inline]
    fn wet(&self, x: i32) -> bool {
        self.depth(x) > 0.0
    }

    pub fn atom_at(&self, x: i32, y: i32) -> Option<u32> {
        self.cell(x, y).map(|c| self.grid[c]).filter(|&a| a != NONE)
    }

    /// Ledger cross-check: stored energy rebuilt from state. Surrogate bands
    /// count their bond tallies instead of the frozen atoms' bonds.
    pub fn recompute_stored(&self) -> Energy {
        let mut s = self.recompute_atoms_stored();
        for b in 0..self.hybrid.hp.bands {
            if self.hybrid.mode[b] == Mode::Surrogate {
                s -= self.band_state_from_atoms(b).stored(&self.table);
            }
        }
        s + self.surrogate_stored()
    }

    fn recompute_atoms_stored(&self) -> Energy {
        let mut s = 0;
        for (i, a) in self.atoms.iter().enumerate() {
            s += a.excite;
            for &j in &a.bonds[..a.nb as usize] {
                if (j as usize) > i {
                    s -= self.table.bond[a.z as usize][self.atoms[j as usize].z as usize];
                }
            }
            if a.weak != NONE && (a.weak as usize) > i {
                s -= self.table.weak[a.z as usize][self.atoms[a.weak as usize].z as usize];
            }
        }
        s
    }

    /// Book a change of stored energy; the bath absorbs the opposite.
    #[inline]
    pub(crate) fn book(&mut self, d_stored: Energy) {
        self.ledger.stored += d_stored;
        self.ledger.bath -= d_stored;
    }

    #[inline]
    fn kt(&self) -> f64 {
        self.env.kt * KT
    }

    #[inline]
    fn accept(&mut self, d_energy: f64) -> bool {
        d_energy <= 0.0 || self.rng.chance((-d_energy / self.kt()).exp())
    }

    pub(crate) fn link(&mut self, i: u32, j: u32) {
        for (a, b) in [(i, j), (j, i)] {
            let at = &mut self.atoms[a as usize];
            at.bonds[at.nb as usize] = b;
            at.nb += 1;
        }
    }

    pub(crate) fn unlink(&mut self, i: u32, j: u32) {
        for (a, b) in [(i, j), (j, i)] {
            let at = &mut self.atoms[a as usize];
            let n = at.nb as usize;
            let k = at.bonds[..n].iter().position(|&x| x == b).expect("unlink of missing bond");
            at.bonds[k] = at.bonds[n - 1];
            at.bonds[n - 1] = NONE;
            at.nb -= 1;
        }
    }

    fn break_weak(&mut self, i: u32) {
        let j = self.atoms[i as usize].weak;
        if j == NONE {
            return;
        }
        let w = self.table.weak[self.atoms[i as usize].z as usize][self.atoms[j as usize].z as usize];
        self.atoms[i as usize].weak = NONE;
        self.atoms[j as usize].weak = NONE;
        self.book(w);
    }

    pub(crate) fn free_valence(&self, i: u32) -> u8 {
        let a = &self.atoms[i as usize];
        self.table.get(a.z).valence - a.nb
    }

    /// Advance one Monte-Carlo sweep (one move + one reaction attempt per atom on average).
    pub fn sweep(&mut self) {
        self.env = self.cosmos.sample(self.t_days);
        let n = self.hybrid.active.len();
        for _ in 0..n {
            let i = self.hybrid.active[self.rng.below(n)];
            self.try_move(i);
            self.try_react(i);
        }
        if self.sweeps % self.hybrid.hp.leap == 0 {
            self.step_surrogates();
        }
        self.photons();
        self.relax_excitations();
        self.sweeps += 1;
        self.t_days += 1.0 / self.p.sweeps_per_day;
    }

    /// Water contacts: wet empty neighbours of (x, y), with `vacant` counted as empty.
    fn water_contacts(&self, x: i32, y: i32, vacant: Option<usize>) -> i32 {
        let mut n = 0;
        for (dx, dy) in DIRS {
            if let Some(c) = self.cell(x + dx, y + dy) {
                if (self.grid[c] == NONE || Some(c) == vacant) && self.wet(x + dx) {
                    n += 1;
                }
            }
        }
        n
    }

    fn try_move(&mut self, i: u32) {
        let a = self.atoms[i as usize];
        let (dx, dy) = DIRS[self.rng.below(8)];
        let (ox, oy) = (a.x as i32, a.y as i32);
        let (nx, ny) = (ox + dx, oy + dy);
        let Some(nc) = self.cell(nx, ny) else { return };
        if self.grid[nc] != NONE || self.cell_frozen(nx) {
            return;
        }
        let ny = ny.rem_euclid(self.p.height as i32);
        let wet_old = self.wet(ox);
        // Either end dry thins the move both ways, so drying slows without biasing where atoms sit.
        if (!wet_old || !self.wet(nx)) && !self.rng.chance(self.p.dry_mobility) {
            return;
        }
        // Weak pairs are rigid like covalent bonds: only try_weak makes or breaks them,
        // since a move has no reverse that re-forms a pair (detailed balance).
        for &j in a.bonds[..a.nb as usize].iter().chain((a.weak != NONE).then_some(&a.weak)) {
            if !self.adjacent(&self.atoms[j as usize], nx, ny) {
                return;
            }
        }
        let mut d_e = 0.0;

        // Solvation (kinetic): E = -s * sum over atom-water contacts of hydropathy.
        let s = self.p.solvation * KT;
        let oc = self.cell(ox, oy).unwrap();
        let ha = self.table.get(a.z).hydropathy;
        let before = self.water_contacts(ox, oy, None);
        let after = self.water_contacts(nx, ny, Some(oc));
        d_e += -s * ha * (after - before) as f64;
        for (ddx, ddy) in DIRS {
            if wet_old {
                if let Some(b) = self.atom_at(ox + ddx, oy + ddy) {
                    if b != i {
                        d_e -= s * self.table.get(self.atoms[b as usize].z).hydropathy;
                    }
                }
            }
            if self.wet(nx) {
                if let Some(b) = self.atom_at(nx + ddx, ny + ddy) {
                    if b != i {
                        d_e += s * self.table.get(self.atoms[b as usize].z).hydropathy;
                    }
                }
            }
        }

        // Tidal drift: flooding pushes up-beach (+x), ebbing pulls down. Wet only.
        if wet_old {
            d_e -= self.p.flow_force * KT * self.env.tide_rate * dx as f64 * 0.5;
        }

        if !self.accept(d_e) {
            return;
        }
        self.grid[oc] = NONE;
        self.grid[nc] = i;
        let at = &mut self.atoms[i as usize];
        at.x = nx as u16;
        at.y = ny as u16;
        self.counts.moves += 1;
    }

    fn try_react(&mut self, i: u32) {
        let a = self.atoms[i as usize];
        let (dx, dy) = DIRS[self.rng.below(8)];
        let Some(j) = self.atom_at(a.x as i32 + dx, a.y as i32 + dy) else { return };
        if self.frozen(j) {
            return;
        }
        let b = self.atoms[j as usize];
        let kt = self.kt();
        if self.rng.chance(0.5) {
            self.try_weak(i, j);
            return;
        }
        let (za, zb) = (a.z as usize, b.z as usize);
        let wet = self.wet(a.x as i32) || self.wet(b.x as i32);
        let hydro = if wet { self.p.hydrolysis * KT } else { 0.0 };
        let arrhenius = |barrier: f64| (-barrier * KT / kt).exp();

        if a.bonded_to(j) {
            // Break. Excitation of either partner can pay the bill (photolysis).
            let e = self.table.bond[za][zb];
            let x = a.excite.max(b.excite);
            let need = e as f64 - hydro;
            let use_x = x > 0 && need > 0.0;
            let uphill = if use_x { need - x as f64 } else { need };
            if self.rng.chance(arrhenius(self.p.barrier_form)) && self.accept(uphill) {
                self.unlink(i, j);
                self.book(e);
                self.tally_break(i, j);
                if use_x {
                    let k = if a.excite >= b.excite { i } else { j };
                    let xe = self.atoms[k as usize].excite;
                    self.atoms[k as usize].excite = 0;
                    self.book(-xe);
                    self.counts.excite_used += 1;
                }
                self.counts.breaks += 1;
            }
            return;
        }

        let fa = self.free_valence(i);
        let fb = self.free_valence(j);
        let e_ab = self.table.bond[za][zb];
        // A weakly paired couple must unpair (try_weak) before going covalent: condense
        // from the paired state has no reverse into it.
        if e_ab == 0 || a.weak == j {
            return;
        }
        if fa > 0 && fb > 0 {
            // Condense. In water, dG = -E + hydrolysis.
            if self.rng.chance(arrhenius(self.p.barrier_form)) && self.accept(hydro - e_ab as f64) {
                self.link(i, j);
                self.book(-e_ab);
                self.tally_form(i, j);
                self.counts.forms += 1;
            }
        } else if fa > 0 && b.nb > 0 {
            // Exchange: b-c becomes a-b. Partner c must not be a.
            let c = b.bonds[self.rng.below(b.nb as usize)];
            // leaving partner may sit in a surrogate band: its cross-band bond is frozen
            if self.frozen(c) {
                return;
            }
            let e_bc = self.table.bond[zb][self.atoms[c as usize].z as usize];
            let d = (e_bc - e_ab) as f64;
            if self.rng.chance(arrhenius(self.p.barrier_exchange)) && self.accept(d) {
                self.unlink(j, c);
                self.link(i, j);
                self.book(e_bc - e_ab);
                self.tally_exchange(i, j, c);
                self.counts.exchanges += 1;
            }
        }
    }

    fn try_weak(&mut self, i: u32, j: u32) {
        let (a, b) = (self.atoms[i as usize], self.atoms[j as usize]);
        let w = self.table.weak[a.z as usize][b.z as usize];
        if a.weak == j {
            if self.accept(w as f64) {
                self.break_weak(i);
                self.counts.weak_breaks += 1;
            }
        } else if a.weak == NONE && b.weak == NONE && w > 0 && !a.bonded_to(j) {
            self.atoms[i as usize].weak = j;
            self.atoms[j as usize].weak = i;
            self.book(-w);
            self.counts.weak_forms += 1;
        }
    }

    /// Photons land on detail-band atoms only; surrogate bands carry light effects in their learned rates.
    fn photons(&mut self) {
        let n = self.hybrid.active.len();
        if n == 0 {
            return;
        }
        let dt = 1.0 / self.p.sweeps_per_day;
        let env = self.env;
        let p = self.p.clone();
        let to_e = |kt: f64| (kt * KT).round() as Energy;

        // Visible: excite. Absorption grows with bonding (pigments).
        const MAX_ABS: f64 = 1.6;
        let k = self.rng.poisson(p.vis_rate * env.visible * n as f64 * dt * MAX_ABS);
        for _ in 0..k {
            let i = self.hybrid.active[self.rng.below(n)] as usize;
            let a = self.atoms[i];
            let el = self.table.get(a.z);
            let absorb = el.absorb * (0.3 + a.nb as f64 / 3.0) * (-p.vis_depth * self.depth(a.x as i32).max(0.0)).exp();
            if !self.rng.chance(absorb / MAX_ABS) {
                continue;
            }
            let e = to_e(p.vis_energy);
            self.ledger.photon_in += e;
            if a.excite == 0 {
                self.atoms[i].excite = e;
                self.ledger.stored += e;
                self.counts.photo_excite += 1;
            } else {
                self.ledger.bath += e;
            }
        }

        // UV: snap one bond. Water shields.
        let k = self.rng.poisson(p.uv_rate * env.uv * n as f64 * dt);
        for _ in 0..k {
            let i = self.hybrid.active[self.rng.below(n)];
            let a = self.atoms[i as usize];
            if !self.rng.chance((-p.uv_depth * self.depth(a.x as i32).max(0.0)).exp()) {
                continue;
            }
            let e = to_e(p.uv_energy);
            self.ledger.photon_in += e;
            self.ledger.bath += e;
            if a.nb > 0 {
                let j = a.bonds[self.rng.below(a.nb as usize)];
                let eb = self.table.bond[a.z as usize][self.atoms[j as usize].z as usize];
                if eb <= e && !self.frozen(j) {
                    self.tally_break(i, j);
                    self.unlink(i, j);
                    self.book(eb);
                    self.counts.uv_breaks += 1;
                }
            }
        }

        // Cosmic: shatter what it can. Water does not shield.
        let k = self.rng.poisson(p.cosmic_rate * env.cosmic * n as f64 * dt);
        for _ in 0..k {
            let i = self.hybrid.active[self.rng.below(n)];
            let mut budget = to_e(p.cosmic_energy);
            self.ledger.photon_in += budget;
            self.ledger.bath += budget;
            self.counts.cosmic_hits += 1;
            let mut k = 0;
            while k < self.atoms[i as usize].nb as usize {
                let j = self.atoms[i as usize].bonds[k];
                let eb = self.table.bond[self.atoms[i as usize].z as usize][self.atoms[j as usize].z as usize];
                if eb > budget || self.frozen(j) {
                    k += 1;
                    continue;
                }
                budget -= eb;
                self.tally_break(i, j);
                self.unlink(i, j);
                self.book(eb);
            }
            if self.atoms[i as usize].weak != NONE && !self.frozen(self.atoms[i as usize].weak) {
                self.break_weak(i);
            }
        }

        // Radio: too soft for covalent bonds; can pop a weak pair.
        let k = self.rng.poisson(p.radio_rate * env.radio * n as f64 * dt);
        for _ in 0..k {
            let i = self.hybrid.active[self.rng.below(n)];
            let e = to_e(p.radio_energy);
            self.ledger.photon_in += e;
            self.ledger.bath += e;
            let a = self.atoms[i as usize];
            if a.weak != NONE && !self.frozen(a.weak) && self.table.weak[a.z as usize][self.atoms[a.weak as usize].z as usize] <= e {
                self.break_weak(i);
            }
        }
    }

    fn relax_excitations(&mut self) {
        let n = self.atoms.len();
        for i in 0..n {
            let x = self.atoms[i].excite;
            if x == 0 {
                continue;
            }
            if self.rng.chance(self.p.excite_decay) {
                self.atoms[i].excite = 0;
                self.book(-x);
            } else if self.atoms[i].nb > 0 && self.rng.chance(self.p.excite_transfer) {
                // Antenna hop to a bonded ground-state neighbour; stored unchanged.
                let a = self.atoms[i];
                let j = a.bonds[self.rng.below(a.nb as usize)] as usize;
                // frozen atoms carry no excitation: coarse-graining dropped it
                if self.atoms[j].excite == 0 && !self.frozen(j as u32) {
                    self.atoms[j].excite = x;
                    self.atoms[i].excite = 0;
                }
            }
        }
    }

    /// Fresh random stream, same state. Shadow mode uses it to measure the stochastic noise floor.
    pub fn reseed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    pub fn take_counts(&mut self) -> Counts {
        std::mem::take(&mut self.counts)
    }

    /// Molecules = covalent components. Returns per-atom molecule id.
    pub fn molecules(&self) -> Vec<u32> {
        let n = self.atoms.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut x: u32) -> u32 {
            while p[x as usize] != x {
                p[x as usize] = p[p[x as usize] as usize];
                x = p[x as usize];
            }
            x
        }
        for (i, a) in self.atoms.iter().enumerate() {
            for &j in &a.bonds[..a.nb as usize] {
                let (ri, rj) = (find(&mut parent, i as u32), find(&mut parent, j));
                if ri != rj {
                    parent[ri as usize] = rj;
                }
            }
        }
        (0..n as u32).map(|i| find(&mut parent, i)).collect()
    }

    pub fn census(&self) -> Census {
        let mol = self.molecules();
        let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
        for (i, &m) in mol.iter().enumerate() {
            members.entry(m).or_default().push(i as u32);
        }
        let mut species: HashMap<String, u32> = HashMap::new();
        let mut size_hist = vec![0u32; 9];
        let mut largest = 0;
        let mut bonds = 0;
        for atoms in members.values() {
            let mut counts = [0u32; N_ELEMENTS];
            for &i in atoms {
                counts[self.atoms[i as usize].z as usize - 1] += 1;
                bonds += self.atoms[i as usize].nb as u32;
            }
            largest = largest.max(atoms.len());
            let bucket = (usize::BITS - atoms.len().leading_zeros()) as usize - 1;
            size_hist[bucket.min(8)] += 1;
            species.entry(formula(&counts)).and_modify(|c| *c += 1).or_insert(1);
        }
        let mut top: Vec<(String, u32)> = species.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let excited = self.atoms.iter().filter(|a| a.excite > 0).count() as u32;
        let element_totals = {
            let mut t = [0u32; N_ELEMENTS];
            for a in &self.atoms {
                t[a.z as usize - 1] += 1;
            }
            t
        };
        // surrogate bands: frozen internal bonds are stale, their counts are the truth
        let mut bonds = bonds as i64 / 2;
        for b in 0..self.hybrid.hp.bands {
            if self.hybrid.mode[b] == Mode::Surrogate {
                bonds += self.hybrid.state[b].total_bonds() - self.band_state_from_atoms(b).total_bonds();
            }
        }
        Census {
            molecules: members.len() as u32,
            bonds: bonds as u32,
            largest: largest as u32,
            size_hist,
            top_species: top.into_iter().take(12).collect(),
            excited,
            element_totals,
        }
    }
}

/// Starting mix by element. The first 8 are this world's; heavier elements of a bigger table
/// get a share that falls with Z (heavier nuclei are rarer), nobles a tenth of that.
pub fn default_composition() -> [f64; N_ELEMENTS] {
    const FIRST: [f64; 8] = [0.30, 0.02, 0.05, 0.12, 0.22, 0.17, 0.10, 0.02];
    std::array::from_fn(|i| {
        if i < FIRST.len() {
            return FIRST[i];
        }
        let z = i as u8 + 1;
        let share = 0.08 * 0.75f64.powi((i - FIRST.len()) as i32);
        if crate::element::element(z).valence == 0 { share * 0.1 } else { share }
    })
}

pub fn formula(counts: &[u32; N_ELEMENTS]) -> String {
    use crate::element::NAMES;
    let mut s = String::new();
    // backbone first, like Hill notation puts carbon first; then by bonds made, the lightest
    // element (Hy) last among its peers as Hill puts hydrogen after carbon; nobles at the end
    let mut order: Vec<usize> = (1..=N_ELEMENTS).collect();
    let val = |z: usize| crate::element::element(z as u8).valence;
    order.sort_by_key(|&z| (z != crate::element::BACKBONE as usize, std::cmp::Reverse(val(z)), z == 1, z));
    for z in order {
        let c = counts[z - 1];
        if c == 0 {
            continue;
        }
        s.push_str(NAMES[z - 1]);
        if c > 1 {
            s.push_str(&c.to_string());
        }
    }
    s
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Census {
    pub molecules: u32,
    pub bonds: u32,
    pub largest: u32,
    /// Molecule counts by size bucket: 1, 2-3, 4-7, 8-15, ... 256+.
    pub size_hist: Vec<u32>,
    pub top_species: Vec<(String, u32)>,
    pub excited: u32,
    pub element_totals: [u32; N_ELEMENTS],
}

#[cfg(test)]
mod tests {
    /// Derived formula order reproduces the hand-written one for [2, 6]: Ka Bo Ox Li Fl Hy Ne Ar.
    #[test]
    fn formula_order_is_hill_like() {
        assert_eq!(formula(&[1; N_ELEMENTS]), "KaBoOxLiFlHyNeAr");
    }

    use super::*;

    fn small() -> ChemWorld {
        let p = ChemParams { width: 48, height: 32, ..Default::default() };
        ChemWorld::new(p, CosmosParams::default(), 7)
    }

    #[test]
    fn matter_and_energy_conserved() {
        let mut w = small();
        let before = w.census().element_totals;
        // run across noon so every photon band fires
        for _ in 0..600 {
            w.sweep();
        }
        assert_eq!(w.census().element_totals, before, "atoms created or destroyed");
        assert_eq!(w.ledger.stored, w.recompute_stored(), "stored energy bookkeeping");
        assert_eq!(w.ledger.drift(), 0, "energy ledger");
        assert!(w.ledger.photon_in > 0);
    }

    #[test]
    fn bonds_stay_symmetric_and_adjacent() {
        let mut w = small();
        for _ in 0..300 {
            w.sweep();
        }
        for (i, a) in w.atoms.iter().enumerate() {
            assert!(a.nb <= w.table.get(a.z).valence);
            for &j in &a.bonds[..a.nb as usize] {
                let b = &w.atoms[j as usize];
                assert!(b.bonded_to(i as u32));
                assert!(w.adjacent(b, a.x as i32, a.y as i32));
            }
            if a.weak != NONE {
                let b = &w.atoms[a.weak as usize];
                assert_eq!(b.weak, i as u32);
                assert!(w.adjacent(b, a.x as i32, a.y as i32), "weak partners adjacent");
                assert!(!a.bonded_to(a.weak), "weak and covalent never share a pair");
            }
            assert_eq!(w.atom_at(a.x as i32, a.y as i32), Some(i as u32));
        }
    }

    #[test]
    fn hybrid_round_trip_keeps_ledger_and_matter() {
        let mut w = small();
        let mut trainer = small();
        let table = crate::hybrid::train(&mut trainer, 2.0, |_| {});
        w.hybrid.rates = Some(table);
        // the frozen-atom assert below assumes no band lifts mid-test
        w.hybrid.hp.lift_below = 0.0;
        let before = w.census().element_totals;
        for _ in 0..100 {
            w.sweep();
        }
        for b in [1, 3, 5] {
            w.restrict(b);
        }
        assert_eq!(w.ledger.stored, w.recompute_stored());
        let frozen_before: Vec<(usize, Atom)> = w.atoms.iter().enumerate().filter(|(i, _)| w.frozen(*i as u32)).map(|(i, a)| (i, *a)).collect();
        assert!(!frozen_before.is_empty());
        for _ in 0..300 {
            w.sweep();
            assert_eq!(w.ledger.stored, w.recompute_stored(), "ledger during surrogate");
        }
        for (i, a) in &frozen_before {
            let now = &w.atoms[*i];
            let mut bonds_then = a.bonds[..a.nb as usize].to_vec();
            let mut bonds_now = now.bonds[..now.nb as usize].to_vec();
            bonds_then.sort();
            bonds_now.sort();
            assert_eq!((a.x, a.y, a.weak, a.excite, bonds_then), (now.x, now.y, now.weak, now.excite, bonds_now), "frozen atom {i} changed");
        }
        for b in [1, 3, 5] {
            w.lift(b);
        }
        assert_eq!(w.ledger.stored, w.recompute_stored(), "ledger after lift");
        assert_eq!(w.ledger.drift(), 0);
        assert_eq!(w.census().element_totals, before);
        for (i, a) in w.atoms.iter().enumerate() {
            assert!(a.nb <= w.table.get(a.z).valence);
            for &j in &a.bonds[..a.nb as usize] {
                assert!(w.atoms[j as usize].bonded_to(i as u32));
                assert!(w.adjacent(&w.atoms[j as usize], a.x as i32, a.y as i32));
            }
        }
    }

    #[test]
    fn chemistry_happens() {
        let mut w = small();
        for _ in 0..400 {
            w.sweep();
        }
        let c = w.census();
        assert!(c.bonds > 50, "bonds formed: {}", c.bonds);
        assert!(c.largest >= 4, "some molecule grew: {}", c.largest);
    }
}
