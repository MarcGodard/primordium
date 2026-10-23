//! Scale-2 bridge: pond of protocells. Vesicle layout (drift, spacing) is viewer-only state.
use godot::prelude::*;
use primordium_core::cosmos::{Cosmos, CosmosParams};
use primordium_core::hybrid::Mode;
use primordium_core::kinetics::{BONDABLE, N_PAIRS, pair_members};
use primordium_core::polymer::{self, LETTERS, seq_string};
use primordium_core::pool::{Chain, PoolParams};
use primordium_core::protocell::{Pond, PondParams};
use primordium_core::rng::Rng;
use primordium_core::surrogate::RateTable;
use std::collections::HashMap;

const AREA: (f32, f32) = (220.0, 140.0);

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct PondSim {
    pond: Option<Pond>,
    layout: HashMap<u64, (Vector2, Vector2)>,
    specks: HashMap<u64, Vector2>,
    folds: HashMap<Vec<u8>, Vec<(i32, i32)>>,
    rng: Option<Rng>,
    base: Base<Node>,
}

fn radius(area: i64) -> f32 {
    // membrane residues ~ sphere surface: r ~ sqrt(area)
    0.9 * (area.max(1) as f32).sqrt()
}

fn dominant_z(seq: &[u8]) -> f32 {
    let mut n = [0; LETTERS.len()];
    for &l in seq {
        n[l as usize] += 1;
    }
    LETTERS[(0..LETTERS.len()).max_by_key(|&k| n[k]).unwrap()] as f32
}

impl PondSim {
    fn fold_of(&mut self, seq: &[u8]) -> Vec<(i32, i32)> {
        if let Some(f) = self.folds.get(seq) {
            return f.clone();
        }
        let p = self.pond.as_ref().unwrap();
        let (f, _) = polymer::express_ensemble(seq, &p.ctx.ph, p.env.kt, 1, 1, 0.5);
        if self.folds.len() > 20000 {
            self.folds.clear();
        }
        self.folds.insert(seq.to_vec(), f.pos.clone());
        f.pos
    }

    fn sync_layout(&mut self) {
        let rng = self.rng.get_or_insert_with(|| Rng::new(7));
        let pond = self.pond.as_ref().unwrap();
        let wet = pond.wet();
        let mut next: HashMap<u64, (Vector2, Vector2)> = HashMap::with_capacity(pond.vesicles.len());
        for v in &pond.vesicles {
            let (pos, vel) = self.layout.get(&v.id).copied().unwrap_or_else(|| {
                let at = v.parent.and_then(|p| self.layout.get(&p)).map(|(p, _)| *p + Vector2::new(rng.f64() as f32 * 4.0 - 2.0, rng.f64() as f32 * 4.0 - 2.0));
                (at.unwrap_or(Vector2::new(rng.f64() as f32 * AREA.0, rng.f64() as f32 * AREA.1)), Vector2::ZERO)
            });
            let jiggle = if wet { 0.25 } else { 0.03 };
            let vel = vel * 0.85 + Vector2::new(rng.f64() as f32 - 0.5, rng.f64() as f32 - 0.5) * jiggle;
            next.insert(v.id, (pos, vel));
        }
        // soft spacing: membranes push apart
        let ids: Vec<(u64, f32)> = pond.vesicles.iter().map(|v| (v.id, radius(v.area()))).collect();
        for a in 0..ids.len() {
            for b in (a + 1)..ids.len() {
                let (pa, pb) = (next[&ids[a].0].0, next[&ids[b].0].0);
                let d = pb - pa;
                let min = ids[a].1 + ids[b].1 + 0.5;
                let dist = d.length();
                if dist < min && dist > 1e-3 {
                    let push = d / dist * (min - dist) * 0.25;
                    next.get_mut(&ids[a].0).unwrap().1 -= push;
                    next.get_mut(&ids[b].0).unwrap().1 += push;
                }
            }
        }
        for (pos, vel) in next.values_mut() {
            *pos += *vel;
            pos.x = pos.x.clamp(4.0, AREA.0 - 4.0);
            pos.y = pos.y.clamp(4.0, AREA.1 - 4.0);
        }
        self.layout = next;
        // free chains in the pond drift as specks
        let mut specks = HashMap::with_capacity(pond.outside.chains.len());
        for c in &pond.outside.chains {
            let p = self.specks.get(&c.id).copied().unwrap_or(Vector2::new(rng.f64() as f32 * AREA.0, rng.f64() as f32 * AREA.1));
            let p = p + Vector2::new(rng.f64() as f32 - 0.5, rng.f64() as f32 - 0.5) * if wet { 0.6 } else { 0.05 };
            specks.insert(c.id, Vector2::new(p.x.rem_euclid(AREA.0), p.y.rem_euclid(AREA.1)));
        }
        self.specks = specks;
    }

    fn chain_dict(&mut self, c: &Chain) -> VarDictionary {
        let fold = self.fold_of(&c.seq);
        let pos: Vec<Vector2> = fold.iter().map(|&(x, y)| Vector2::new(x as f32, y as f32)).collect();
        let mut d = VarDictionary::new();
        d.set("id", c.id as i64);
        d.set("seq", seq_string(&c.seq));
        d.set("letters", &PackedByteArray::from(c.seq.as_slice()));
        d.set("fold", &PackedVector2Array::from(pos.as_slice()));
        d.set("generation", c.generation as i64);
        d.set("stability", c.func.stability);
        d.set("amphiphile", c.func.amphiphile);
        d.set("pigment", c.func.pigment);
        d.set("confidence", c.conf);
        let mut top: Vec<(usize, f64)> = (0..N_PAIRS).map(|p| (p, c.func.catalysis.get(p).copied().unwrap_or(0.0))).collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        let names = primordium_core::element::NAMES;
        let mut cat = VarArray::new();
        for (p, cut) in top.into_iter().take(4) {
            let (a, b) = pair_members(p);
            cat.push(&format!("{}-{} {:.2} kT", names[BONDABLE[a] as usize - 1], names[BONDABLE[b] as usize - 1], cut).to_variant());
        }
        d.set("catalysis", &cat);
        let f = polymer::Fold { pos: fold, energy: 0.0 };
        let mut sites = PackedVector2Array::new();
        for (i, j, _) in polymer::sites(&f) {
            sites.push(pos[i]);
            sites.push(pos[j]);
        }
        d.set("sites", &sites);
        d
    }
}

#[godot_api]
impl PondSim {
    /// Levers (see lever_panel.gd), x1/4..x4 of default: "shear" (how readily a ripe vesicle
    /// splits in the tide's shear), "uv" (UV erosion of membranes). False when unknown.
    #[func]
    fn set_lever(&mut self, name: GString, mult: f64) -> bool {
        let Some(p) = self.pond.as_mut() else { return false };
        let d = PondParams::default();
        let m = mult.clamp(0.25, 4.0);
        match name.to_string().as_str() {
            "shear" => p.pp.div_rate = d.div_rate * m,
            "uv" => p.pp.uv_damage = d.uv_damage * m,
            _ => return false,
        }
        true
    }

    #[func]
    fn levers(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(p) = self.pond.as_ref() else { return out };
        let d = PondParams::default();
        out.set("shear", p.pp.div_rate / d.div_rate);
        out.set("uv", p.pp.uv_damage / d.uv_damage);
        out
    }

    #[func]
    fn start(&mut self, rates_path: GString, seed: i64, auto: bool) -> bool {
        let Ok(text) = std::fs::read_to_string(rates_path.to_string()) else { return false };
        let Ok(rates) = serde_json::from_str::<RateTable>(&text) else { return false };
        let cosmos = Cosmos::new(CosmosParams { seed: seed as u64, ..Default::default() });
        self.pond = Some(Pond::new(PoolParams::default(), PondParams { auto, ..Default::default() }, cosmos, rates, seed as u64));
        self.layout.clear();
        true
    }

    #[func]
    fn set_auto(&mut self, on: bool) {
        if let Some(p) = self.pond.as_mut() {
            p.pp.auto = on;
        }
    }

    #[func]
    fn step(&mut self, steps: i64, dt: f64) {
        let Some(p) = self.pond.as_mut() else { return };
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
    fn vesicles(&self) -> VarArray {
        let mut arr = VarArray::new();
        let Some(p) = self.pond.as_ref() else { return arr };
        for v in &p.vesicles {
            let (pos, _) = self.layout.get(&v.id).copied().unwrap_or_default();
            let mut d = VarDictionary::new();
            d.set("id", v.id as i64);
            d.set("pos", pos);
            d.set("r", radius(v.area()));
            d.set("area", v.area());
            d.set("chains", v.interior.chains.len() as i64);
            d.set("free", v.interior.free.iter().sum::<i64>());
            d.set("pigment", v.pigment());
            d.set("cut", v.interior.backbone_cut());
            d.set("osmotic", v.osmotic());
            d.set("surrogate", v.mode == Mode::Surrogate);
            d.set("confidence", v.conf);
            d.set("generation", v.generation as i64);
            // interior chain dots: dominant letter element per chain
            let mut z = PackedFloat32Array::new();
            for c in &v.interior.chains {
                z.push(dominant_z(&c.seq));
            }
            d.set("chain_z", &z);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Pond-floor specks for free chains: (x, y, element z) triples.
    #[func]
    fn specks(&self) -> PackedFloat32Array {
        let mut v = Vec::new();
        let Some(p) = self.pond.as_ref() else { return PackedFloat32Array::new() };
        for c in &p.outside.chains {
            if let Some(pos) = self.specks.get(&c.id) {
                v.extend_from_slice(&[pos.x, pos.y, dominant_z(&c.seq)]);
            }
        }
        PackedFloat32Array::from(v.as_slice())
    }

    #[func]
    fn vesicle_at(&self, p: Vector2) -> i64 {
        let Some(pond) = self.pond.as_ref() else { return -1 };
        for v in &pond.vesicles {
            if let Some((pos, _)) = self.layout.get(&v.id) {
                if pos.distance_to(p) <= radius(v.area()) + 0.5 {
                    return v.id as i64;
                }
            }
        }
        -1
    }

    /// Interior chains of a vesicle (follows the lineage to a daughter if it divided).
    #[func]
    fn interior(&mut self, id: i64) -> VarArray {
        let mut arr = VarArray::new();
        let Some(pond) = self.pond.as_ref() else { return arr };
        let v = pond.vesicles.iter().find(|v| v.id as i64 == id).or_else(|| pond.vesicles.iter().find(|v| v.parent == Some(id as u64)));
        let Some(v) = v else { return arr };
        let chains: Vec<Chain> = v.interior.chains.clone();
        for c in &chains {
            let d = self.chain_dict(c);
            arr.push(&d.to_variant());
        }
        arr
    }

    #[func]
    fn follow(&self, id: i64) -> i64 {
        let Some(pond) = self.pond.as_ref() else { return -1 };
        if pond.vesicles.iter().any(|v| v.id as i64 == id) {
            return id;
        }
        pond.vesicles.iter().find(|v| v.parent == Some(id as u64)).map_or(-1, |v| v.id as i64)
    }

    #[func]
    fn env(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(p) = self.pond.as_ref() else { return d };
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
        d.set("ledger_drift", 0i64);
        d
    }

    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(p) = self.pond.as_ref() else { return d };
        let s = &p.stats;
        d.set("vesicles", p.vesicles.len() as i64);
        d.set("surrogate", p.vesicles.iter().filter(|v| v.mode == Mode::Surrogate).count() as i64);
        d.set("nucleations", s.nucleations as i64);
        d.set("divisions", s.divisions as i64);
        d.set("bursts", s.bursts as i64);
        d.set("max_generation", p.vesicles.iter().map(|v| v.generation).max().unwrap_or(0) as i64);
        d.set("outside_chains", p.outside.chains.len() as i64);
        d.set("cut_in", p.mean_vesicle_cut());
        d.set("cut_out", p.outside.backbone_cut());
        let si = p.total_interior_stats();
        d.set("err_in", si.mutations as f64 / si.copied_letters.max(1) as f64);
        d.set("err_out", p.outside.stats.mutations as f64 / p.outside.stats.copied_letters.max(1) as f64);
        d.set("lifts", s.lifts as i64);
        d.set("auto", p.pp.auto);
        let mut disc = VarArray::new();
        for e in p.ctx.discoveries.entries.iter().rev().take(5) {
            disc.push(&format!("day {:.1} s{}: {}", e.t_days, e.scale, e.note).to_variant());
        }
        d.set("discoveries", &disc);
        d
    }
}
