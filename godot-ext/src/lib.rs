//! GDExtension bridge. Godot owns visuals only; all state lives in primordium-core.
use godot::prelude::*;
use primordium_core::chem::{ChemParams, ChemWorld, NONE};
use primordium_core::cosmos::CosmosParams;
use primordium_core::element::{KT, N_ELEMENTS};
use primordium_core::hybrid::Mode;
use primordium_core::surrogate::RateTable;

mod pool_sim;

struct PrimordiumExt;

#[gdextension]
unsafe impl ExtensionLibrary for PrimordiumExt {}

#[derive(GodotClass)]
#[class(base=Node)]
pub struct ChemSim {
    world: ChemWorld,
    base: Base<Node>,
}

#[godot_api]
impl INode for ChemSim {
    fn init(base: Base<Node>) -> Self {
        Self { world: ChemWorld::new(ChemParams::default(), CosmosParams::default(), 1), base }
    }
}

/// Wrapped y distance > 1 means the bond crosses the periodic seam: skip drawing it.
fn seam(ay: u16, by: u16) -> bool {
    (ay as i32 - by as i32).abs() > 1
}

#[godot_api]
impl ChemSim {
    #[func]
    fn reset(&mut self, seed: i64, width: i64, height: i64) {
        let p = ChemParams { width: width as u16, height: height as u16, ..Default::default() };
        self.world = ChemWorld::new(p, CosmosParams { seed: seed as u64, ..Default::default() }, seed as u64);
    }

    #[func]
    fn step(&mut self, sweeps: i64) {
        for _ in 0..sweeps {
            self.world.sweep();
        }
    }

    /// Load a trained scale-0 rate table (from `primordium train`). Enables surrogate bands.
    #[func]
    fn load_rates(&mut self, path: GString) -> bool {
        let Ok(text) = std::fs::read_to_string(path.to_string()) else { return false };
        let Ok(t) = serde_json::from_str::<RateTable>(&text) else { return false };
        self.world.hybrid.rates = Some(t);
        true
    }

    #[func]
    fn set_auto(&mut self, on: bool) {
        self.world.hybrid.hp.auto = on;
    }

    /// Pin a band to detail (zoomed in) or release it to the resolution manager.
    #[func]
    fn pin_band(&mut self, band: i64, pinned: bool) {
        let b = band as usize;
        if b < self.world.hybrid.pinned.len() {
            self.world.hybrid.pinned[b] = pinned;
            if pinned {
                self.world.lift(b);
            }
        }
    }

    #[func]
    fn bands(&self) -> VarArray {
        let mut arr = VarArray::new();
        for b in 0..self.world.hybrid.hp.bands {
            let (x0, x1) = self.world.band_columns(b);
            let mut d = VarDictionary::new();
            let surrogate = self.world.hybrid.mode[b] == Mode::Surrogate;
            d.set("x0", x0 as i64);
            d.set("x1", x1 as i64);
            d.set("surrogate", surrogate);
            d.set("pinned", self.world.hybrid.pinned[b]);
            d.set("confidence", self.world.hybrid.confidence[b]);
            let bonds = if surrogate { self.world.hybrid.state[b].total_bonds() } else { self.world.band_state_from_atoms(b).total_bonds() };
            d.set("bonds", bonds);
            if surrogate {
                // pair histogram: what the surrogate knows about this band
                let s = &self.world.hybrid.state[b];
                let mut pairs = VarArray::new();
                for (p, &n) in s.bonds.iter().enumerate() {
                    let (pa, pb) = primordium_core::kinetics::pair_members(p);
                    let name = |k: usize| primordium_core::element::NAMES[primordium_core::kinetics::BONDABLE[k] as usize - 1];
                    let mut e = VarDictionary::new();
                    e.set("name", format!("{}-{}", name(pa), name(pb)));
                    e.set("n", n);
                    e.set("za", primordium_core::kinetics::BONDABLE[pa] as i64);
                    e.set("zb", primordium_core::kinetics::BONDABLE[pb] as i64);
                    pairs.push(&e.to_variant());
                }
                d.set("pairs", &pairs);
            }
            arr.push(&d.to_variant());
        }
        arr
    }

    #[func]
    fn discoveries(&self) -> VarArray {
        let mut arr = VarArray::new();
        for e in self.world.hybrid.discoveries.entries.iter().rev().take(6) {
            arr.push(&format!("day {:.1} band {} conf {:.2} T {:.0}K: {}", e.t_days, e.region, e.confidence, e.features[0] * 300.0, e.note).to_variant());
        }
        arr
    }

    #[func]
    fn hybrid_stats(&self) -> VarDictionary {
        let h = &self.world.hybrid;
        let mut d = VarDictionary::new();
        d.set("lifts", h.lifts as i64);
        d.set("restricts", h.restricts as i64);
        d.set("discoveries", h.discoveries.entries.len() as i64);
        d.set("active_atoms", h.active.len() as i64);
        d.set("has_rates", h.rates.is_some());
        d
    }

    #[func]
    fn grid_size(&self) -> Vector2i {
        Vector2i::new(self.world.p.width as i32, self.world.p.height as i32)
    }

    #[func]
    fn atom_count(&self) -> i64 {
        self.world.atoms.len() as i64
    }

    /// MultiMesh 2D buffer: per instance Transform2D (8 floats), color (4), custom (4).
    /// custom = (z, bonds made, excitation in kT, per-atom phase 0..1).
    #[func]
    fn instance_buffer(&self) -> PackedFloat32Array {
        let atoms = &self.world.atoms;
        let mut v = Vec::with_capacity(atoms.len() * 16);
        for (i, a) in atoms.iter().enumerate() {
            let (x, y) = (a.x as f32 + 0.5, a.y as f32 + 0.5);
            v.extend_from_slice(&[1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y]);
            v.extend_from_slice(&[1.0, 1.0, 1.0, 1.0]);
            let phase = ((i as u32).wrapping_mul(2654435761) >> 8) as f32 / (1u32 << 24) as f32;
            // frozen (surrogate band) atoms flagged by +10 on the bond count
            let nb = a.nb as f32 + if self.world.frozen(i as u32) { 10.0 } else { 0.0 };
            v.extend_from_slice(&[a.z as f32, nb, a.excite as f32 / KT as f32, phase]);
        }
        PackedFloat32Array::from(v.as_slice())
    }

    /// Covalent bonds as segment endpoint pairs, cell-centre coordinates.
    #[func]
    fn bond_lines(&self) -> PackedVector2Array {
        let atoms = &self.world.atoms;
        let mut v = Vec::new();
        for (i, a) in atoms.iter().enumerate() {
            for &j in &a.bonds[..a.nb as usize] {
                let b = &atoms[j as usize];
                // frozen bonds are stale: the surrogate's counts are the truth there
                if (j as usize) > i && !seam(a.y, b.y) && !self.world.frozen(i as u32) && !self.world.frozen(j) {
                    v.push(Vector2::new(a.x as f32 + 0.5, a.y as f32 + 0.5));
                    v.push(Vector2::new(b.x as f32 + 0.5, b.y as f32 + 0.5));
                }
            }
        }
        PackedVector2Array::from(v.as_slice())
    }

    #[func]
    fn weak_lines(&self) -> PackedVector2Array {
        let atoms = &self.world.atoms;
        let mut v = Vec::new();
        for (i, a) in atoms.iter().enumerate() {
            if a.weak != NONE && (a.weak as usize) > i {
                let b = &atoms[a.weak as usize];
                if !seam(a.y, b.y) {
                    v.push(Vector2::new(a.x as f32 + 0.5, a.y as f32 + 0.5));
                    v.push(Vector2::new(b.x as f32 + 0.5, b.y as f32 + 0.5));
                }
            }
        }
        PackedVector2Array::from(v.as_slice())
    }

    #[func]
    fn env(&self) -> VarDictionary {
        let e = &self.world.env;
        let mut d = VarDictionary::new();
        d.set("t_days", e.t_days);
        d.set("day_phase", e.day_phase);
        d.set("year_phase", e.year_phase);
        d.set("year", e.year);
        d.set("sun_height", e.sun_height);
        d.set("orbit_distance", e.orbit_distance);
        d.set("cloud", e.cloud);
        d.set("visible", e.visible);
        d.set("uv", e.uv);
        d.set("radio", e.radio);
        d.set("cosmic", e.cosmic);
        d.set("temperature_k", e.temperature_k);
        d.set("tide_level", e.tide_level);
        d.set("tide_rate", e.tide_rate);
        d.set("star_activity", e.star_activity);
        d.set("flare", e.flare);
        d.set("supernova", e.supernova);
        let cp = &self.world.cosmos.p;
        d.set("year_days", cp.year_days);
        d.set("moon0_phase", (e.t_days / cp.moon_periods[0]).fract());
        d.set("moon1_phase", (e.t_days / cp.moon_periods[1]).fract());
        d.set("ledger_drift", self.world.ledger.drift());
        d.set("photon_in", self.world.ledger.photon_in as f64 / KT);
        d
    }

    #[func]
    fn census(&self) -> VarDictionary {
        let c = self.world.census();
        let mut d = VarDictionary::new();
        d.set("molecules", c.molecules);
        d.set("bonds", c.bonds);
        d.set("largest", c.largest);
        d.set("excited", c.excited);
        let mut top = VarArray::new();
        for (f, n) in &c.top_species {
            top.push(&format!("{f} x{n}").to_variant());
        }
        d.set("top", &top);
        d
    }

    /// Element table for legends and shaders.
    #[func]
    fn elements(&self) -> VarArray {
        let mut arr = VarArray::new();
        for z in 1..=N_ELEMENTS as u8 {
            let e = self.world.table.get(z);
            let mut d = VarDictionary::new();
            d.set("z", e.z);
            d.set("name", e.name);
            d.set("valence", e.valence);
            d.set("shell", e.shell);
            d.set("outer", e.outer);
            d.set("chi", e.chi);
            d.set("hydropathy", e.hydropathy);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Hover readout for the atom at a cell, plus its molecule's formula and size.
    #[func]
    fn atom_info(&self, x: i64, y: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(i) = self.world.atom_at(x as i32, y as i32) else { return d };
        let a = self.world.atoms[i as usize];
        let e = self.world.table.get(a.z);
        d.set("name", e.name);
        d.set("z", a.z);
        d.set("bonds", a.nb);
        d.set("valence", e.valence);
        d.set("excite", a.excite as f64 / KT);
        d.set("weak", a.weak != NONE);
        // molecule walk: BFS over covalent bonds
        let mut seen = vec![i];
        let mut k = 0;
        while k < seen.len() && seen.len() < 4096 {
            let at = self.world.atoms[seen[k] as usize];
            for &j in &at.bonds[..at.nb as usize] {
                if !seen.contains(&j) {
                    seen.push(j);
                }
            }
            k += 1;
        }
        let mut counts = [0u32; N_ELEMENTS];
        for &j in &seen {
            counts[self.world.atoms[j as usize].z as usize - 1] += 1;
        }
        d.set("molecule", primordium_core::chem::formula(&counts));
        d.set("molecule_size", seen.len() as i64);
        d
    }
}
