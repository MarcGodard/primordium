//! Scale-1 bridge: tide pool of polymers. Layout (where chains drift) is viewer-only state.
use godot::prelude::*;
use primordium_core::cosmos::Cosmos;
use primordium_core::element::NAMES;
use primordium_core::kinetics::{BONDABLE, N_PAIRS, pair_members};
use primordium_core::polymer::{self, LETTER_NAMES, LETTERS, seq_string};
use primordium_core::pool::{Pool, PoolParams};
use primordium_core::rng::Rng;
use primordium_core::surrogate::RateTable;
use std::collections::HashMap;

/// Pool size in display units; one residue = 1 unit.
const AREA: (f32, f32) = (220.0, 140.0);

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct PoolSim {
    pool: Option<Pool>,
    layout: HashMap<u64, (Vector2, Vector2)>,
    folds: HashMap<Vec<u8>, Vec<(i32, i32)>>,
    rng: Option<Rng>,
    base: Base<Node>,
}

impl PoolSim {
    fn fold_of(&mut self, seq: &[u8]) -> Vec<(i32, i32)> {
        if let Some(f) = self.folds.get(seq) {
            return f.clone();
        }
        let p = self.pool.as_ref().unwrap();
        let (f, _) = polymer::express_ensemble(seq, &p.ctx.ph, p.env.kt, 1, 1, 0.5);
        if self.folds.len() > 20000 {
            self.folds.clear();
        }
        self.folds.insert(seq.to_vec(), f.pos.clone());
        f.pos
    }

    fn sync_layout(&mut self) {
        let rng = self.rng.get_or_insert_with(|| Rng::new(42));
        let pool = self.pool.as_ref().unwrap();
        let mut next = HashMap::with_capacity(pool.c.chains.len());
        let wet = pool.wet();
        for c in &pool.c.chains {
            let (mut pos, mut vel) = self.layout.get(&c.id).copied().unwrap_or_else(|| {
                // new chains appear beside their parent, else anywhere
                let at = c.parent.and_then(|p| self.layout.get(&p)).map(|(p, _)| *p + Vector2::new(rng.f64() as f32 * 6.0 - 3.0, rng.f64() as f32 * 6.0 - 3.0));
                (at.unwrap_or(Vector2::new(rng.f64() as f32 * AREA.0, rng.f64() as f32 * AREA.1)), Vector2::ZERO)
            });
            // brownian drift; water moves things, a dry pool nearly freezes them
            let jiggle = if wet { 0.35 } else { 0.04 };
            vel = vel * 0.9 + Vector2::new(rng.f64() as f32 - 0.5, rng.f64() as f32 - 0.5) * jiggle;
            pos += vel;
            pos.x = pos.x.rem_euclid(AREA.0);
            pos.y = pos.y.rem_euclid(AREA.1);
            next.insert(c.id, (pos, vel));
        }
        self.layout = next;
    }

    fn centroid(fold: &[(i32, i32)]) -> Vector2 {
        let n = fold.len().max(1) as f32;
        let (sx, sy) = fold.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 as f32, a.1 + p.1 as f32));
        Vector2::new(sx / n, sy / n)
    }
}

fn pair_name(p: usize) -> String {
    let (a, b) = pair_members(p);
    format!("{}-{}", NAMES[BONDABLE[a] as usize - 1], NAMES[BONDABLE[b] as usize - 1])
}

#[godot_api]
impl PoolSim {
    /// Needs the scale-0 rate table: the pool runs on its learned rates.
    #[func]
    fn start(&mut self, rates_path: GString, seed: i64) -> bool {
        let Ok(text) = std::fs::read_to_string(rates_path.to_string()) else { return false };
        let Ok(rates) = serde_json::from_str::<RateTable>(&text) else { return false };
        let cosmos = Cosmos::new(crate::elements_sim::cosmos(seed as u64));
        let mut pp = PoolParams::default();
        crate::elements_sim::recipe().apply_pool(&mut pp);
        self.pool = Some(Pool::new(pp, cosmos, rates, seed as u64));
        self.layout.clear();
        true
    }

    /// Environment levers, multipliers on the default pool (x1/4..x4, height x0.6..x1.6): "height"
    /// how high on the beach the pool sits (the tide floods it less often: longer dry spells, where
    /// chains copy), "size" its volume (a small pool concentrates the same monomers). Matter is
    /// untouched. False when unknown.
    #[func]
    fn set_lever(&mut self, name: GString, mult: f64) -> bool {
        let Some(p) = self.pool.as_mut() else { return false };
        let d = PoolParams::default();
        match name.to_string().as_str() {
            "height" => p.ctx.p.elevation = d.elevation * mult.clamp(0.6, 1.6),
            "size" => {
                let v = d.cells * mult.clamp(0.25, 4.0);
                p.ctx.p.cells = v;
                p.c.cells = v;
            }
            _ => return false,
        }
        true
    }

    #[func]
    fn levers(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(p) = self.pool.as_ref() else { return out };
        let d = PoolParams::default();
        out.set("height", p.ctx.p.elevation / d.elevation);
        out.set("size", p.c.cells / d.cells);
        out
    }

    #[func]
    fn step(&mut self, steps: i64, dt: f64) {
        let Some(p) = self.pool.as_mut() else { return };
        for _ in 0..steps {
            p.step(dt);
        }
        self.sync_layout();
    }

    #[func]
    fn area(&self) -> Vector2 {
        Vector2::new(AREA.0, AREA.1)
    }

    #[func]
    fn residue_count(&self) -> i64 {
        self.pool.as_ref().map_or(0, |p| p.c.chains.iter().map(|c| c.seq.len() as i64).sum())
    }

    /// MultiMesh 2D buffer, one instance per residue: custom = (side-group Z, 2 backbone bonds, highlight, phase).
    #[func]
    fn instance_buffer(&mut self, selected: i64) -> PackedFloat32Array {
        let Some(pool) = self.pool.as_ref() else { return PackedFloat32Array::new() };
        let chains: Vec<(u64, Vec<u8>)> = pool.c.chains.iter().map(|c| (c.id, c.seq.clone())).collect();
        let mut v = Vec::new();
        for (id, seq) in chains {
            let fold = self.fold_of(&seq);
            let c = Self::centroid(&fold);
            let at = self.layout.get(&id).map(|x| x.0).unwrap_or_default();
            let hl = if id as i64 == selected { 1.0 } else { 0.0 };
            for (k, (&l, &(x, y))) in seq.iter().zip(&fold).enumerate() {
                let p = at + Vector2::new(x as f32, y as f32) - c;
                v.extend_from_slice(&[1.0, 0.0, 0.0, p.x, 0.0, 1.0, 0.0, p.y]);
                v.extend_from_slice(&[1.0, 1.0, 1.0, 1.0]);
                v.extend_from_slice(&[LETTERS[l as usize] as f32, 2.0, hl, (id as f32 * 0.37 + k as f32 * 0.11).fract()]);
            }
        }
        PackedFloat32Array::from(v.as_slice())
    }

    #[func]
    fn backbone_lines(&mut self) -> PackedVector2Array {
        let Some(pool) = self.pool.as_ref() else { return PackedVector2Array::new() };
        let chains: Vec<(u64, Vec<u8>)> = pool.c.chains.iter().map(|c| (c.id, c.seq.clone())).collect();
        let mut v = Vec::new();
        for (id, seq) in chains {
            let fold = self.fold_of(&seq);
            let c = Self::centroid(&fold);
            let at = self.layout.get(&id).map(|x| x.0).unwrap_or_default();
            for w in fold.windows(2) {
                v.push(at + Vector2::new(w[0].0 as f32, w[0].1 as f32) - c);
                v.push(at + Vector2::new(w[1].0 as f32, w[1].1 as f32) - c);
            }
        }
        PackedVector2Array::from(v.as_slice())
    }

    /// Chain whose centre is nearest `p` within `radius`, or -1.
    #[func]
    fn chain_at(&self, p: Vector2, radius: f32) -> i64 {
        let mut best = (-1i64, radius * radius);
        for (id, (pos, _)) in &self.layout {
            let d = pos.distance_squared_to(p);
            if d < best.1 {
                best = (*id as i64, d);
            }
        }
        best.0
    }

    #[func]
    fn chain_info(&mut self, id: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(pool) = self.pool.as_ref() else { return d };
        // chains get new ids when they grow, break or join: follow the lineage to a descendant
        let found = pool
            .c
            .chains
            .iter()
            .find(|c| c.id as i64 == id)
            .or_else(|| pool.c.chains.iter().filter(|c| c.parent == Some(id as u64)).max_by_key(|c| c.seq.len()));
        let Some(c) = found.cloned() else { return d };
        let id = c.id as i64;
        let fold = self.fold_of(&c.seq);
        let pos: Vec<Vector2> = fold.iter().map(|&(x, y)| Vector2::new(x as f32, y as f32)).collect();
        d.set("id", id);
        d.set("seq", seq_string(&c.seq));
        d.set("letters", &PackedByteArray::from(c.seq.as_slice()));
        d.set("fold", &PackedVector2Array::from(pos.as_slice()));
        d.set("generation", c.generation as i64);
        d.set("parent", c.parent.map_or(-1, |p| p as i64));
        d.set("age_days", self.pool.as_ref().unwrap().t_days - c.born);
        d.set("stability", c.func.stability);
        d.set("amphiphile", c.func.amphiphile);
        d.set("pigment", c.func.pigment);
        d.set("confidence", c.conf);
        d.set("paired", c.paired);
        let mut top: Vec<(usize, f64)> = (0..N_PAIRS).map(|p| (p, c.func.catalysis.get(p).copied().unwrap_or(0.0))).collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut cat = VarArray::new();
        for (p, cut) in top.into_iter().take(6) {
            cat.push(&format!("{} {:.2} kT", pair_name(p), cut).to_variant());
        }
        d.set("catalysis", &cat);
        let ka = primordium_core::kinetics::bidx(primordium_core::element::BACKBONE).unwrap();
        d.set("backbone_cut", c.func.catalysis.get(primordium_core::kinetics::pair(ka, ka)).copied().unwrap_or(0.0));
        let f = polymer::Fold { pos: fold, energy: 0.0 };
        let mut sites = PackedVector2Array::new();
        for (i, j, _) in polymer::sites(&f) {
            sites.push(pos[i]);
            sites.push(pos[j]);
        }
        d.set("sites", &sites);
        d
    }

    /// The atoms of one residue: the backbone atom plus its side group, which carries an Hy cap on
    /// every bond beyond the one to the backbone.
    #[func]
    fn residue_atoms(letter: i64) -> VarArray {
        let mut arr = VarArray::new();
        let z = LETTERS[letter as usize];
        let mut add = |z: u8, x: f32, y: f32, nb: i64| {
            let mut d = VarDictionary::new();
            d.set("z", z as i64);
            d.set("pos", Vector2::new(x, y));
            d.set("bonds", nb);
            arr.push(&d.to_variant());
        };
        let bb = primordium_core::element::BACKBONE;
        add(bb, 0.0, 0.0, primordium_core::element::MAX_VALENCE as i64);
        let v = primordium_core::element::element(z).valence as i64;
        add(z, 0.0, 1.0, v);
        // caps fan out below the side atom, each bonded to it alone
        for k in 1..v {
            add(1, (k as f32 - v as f32 * 0.5) * 0.9, 2.0, 1);
        }
        arr
    }

    #[func]
    fn letter_name(&self, letter: i64) -> GString {
        GString::from(LETTER_NAMES[letter as usize])
    }

    #[func]
    fn env(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(p) = self.pool.as_ref() else { return d };
        let e = &p.env;
        for (k, v) in [
            ("t_days", e.t_days),
            ("day_phase", e.day_phase),
            ("year_phase", e.year_phase),
            ("year", e.year),
            ("temperature_k", e.temperature_k),
            ("tide_level", e.tide_level),
            ("tide_rate", e.tide_rate),
            ("visible", e.visible),
            ("uv", e.uv),
            ("radio", e.radio),
            ("cosmic", e.cosmic),
            ("cloud", e.cloud),
            ("star_activity", e.star_activity),
            ("flare", e.flare),
            ("supernova", e.supernova),
            ("orbit_distance", e.orbit_distance),
        ] {
            d.set(k, v);
        }
        let cp = &p.cosmos.p;
        d.set("moon0_phase", (e.t_days / cp.moon_periods[0]).fract());
        d.set("moon1_phase", (e.t_days / cp.moon_periods[1]).fract());
        d.set("wet", p.wet());
        d.set("elevation", p.ctx.p.elevation);
        d.set("ledger_drift", 0i64);
        d
    }

    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(p) = self.pool.as_ref() else { return d };
        let n = p.c.chains.len();
        d.set("chains", n as i64);
        d.set("mean_len", p.c.chains.iter().map(|c| c.seq.len()).sum::<usize>() as f64 / n.max(1) as f64);
        d.set("max_len", p.c.chains.iter().map(|c| c.seq.len()).max().unwrap_or(0) as i64);
        d.set("max_generation", p.c.chains.iter().map(|c| c.generation).max().unwrap_or(0) as i64);
        d.set("copies", p.c.stats.copies as i64);
        d.set("copy_error", p.c.stats.mutations as f64 / p.c.stats.copied_letters.max(1) as f64);
        d.set("free_monomers", p.c.free.iter().sum::<i64>());
        d.set("backbone_cut", p.c.backbone_cut());
        d.set("best_cut", p.ctx.best_cut);
        d.set("folds", p.ctx.model.folds as i64);
        d.set("exact", p.ctx.model.exact as i64);
        d.set("nearest", p.ctx.model.nearest as i64);
        let mut disc = VarArray::new();
        for e in p.ctx.discoveries.entries.iter().rev().take(5) {
            disc.push(&format!("day {:.1}: {}", e.t_days, e.note).to_variant());
        }
        d.set("discoveries", &disc);
        d
    }
}
