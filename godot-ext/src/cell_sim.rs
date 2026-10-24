//! Scale-3 bridge: cells on the beach. Patch = elevation band; position within a patch is viewer-only.
use godot::prelude::*;
use primordium_core::cell::{CellParams, GenomeInfo, Signal, World};
use primordium_core::cosmos::Cosmos;
use primordium_core::hybrid::Mode;
use primordium_core::kinetics::{BONDABLE, N_PAIRS, bidx, pair, pair_members};
use primordium_core::polymer::{self, LETTERS, seq_string};
use primordium_core::pool::{KA, PoolParams};
use primordium_core::surrogate::RateTable;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct CellSim {
    world: Option<World>,
    folds: HashMap<Vec<u8>, Vec<(i32, i32)>>,
    /// Phenotype means sampled over time: (t, poly, synth, amph, pigment, genome len, cells).
    history: VecDeque<[f64; 7]>,
    base: Base<Node>,
}

/// Dominant role of a gene product: 0 polymerase, 1 metabolism, 2 membrane, 3 pigment, 4 none.
pub(crate) fn role(f: &polymer::Function) -> (i64, f64) {
    let ka = bidx(KA).unwrap();
    let poly = f.catalysis[pair(ka, ka)];
    let synth = LETTERS.iter().map(|&z| f.catalysis[pair(ka, bidx(z).unwrap())]).sum::<f64>() / LETTERS.len() as f64;
    let scores = [poly / 0.5, synth / 0.5, f.amphiphile / 0.4, f.pigment / 0.8];
    let (k, v) = scores.iter().cloned().enumerate().fold((4, 0.0), |a, (k, v)| if v > a.1 { (k, v) } else { a });
    if v < 0.3 { (4, v) } else { (k as i64, v) }
}

impl CellSim {
    fn info(&mut self, genome: &[u8]) -> Arc<GenomeInfo> {
        self.world.as_mut().unwrap().info(genome)
    }

    fn fold_of(&mut self, seq: &[u8]) -> Vec<(i32, i32)> {
        if let Some(f) = self.folds.get(seq) {
            return f.clone();
        }
        let w = self.world.as_ref().unwrap();
        let (f, _) = polymer::express_ensemble(seq, &w.ctx.ph, w.env.kt, 1, 1, 0.5);
        if self.folds.len() > 20000 {
            self.folds.clear();
        }
        self.folds.insert(seq.to_vec(), f.pos.clone());
        f.pos
    }
}

#[godot_api]
impl CellSim {
    /// Levers (see lever_panel.gd), x1/4..x4 of default: "mutation" (UV and cosmic-ray
    /// mutations per copied letter), "uv" (UV wear on membranes). False when unknown.
    #[func]
    fn set_lever(&mut self, name: GString, mult: f64) -> bool {
        let Some(w) = self.world.as_mut() else { return false };
        let d = CellParams::default();
        let m = mult.clamp(0.25, 4.0);
        match name.to_string().as_str() {
            "mutation" => {
                w.cp.uv_mutation = d.uv_mutation * m;
                w.cp.cosmic_mutation = d.cosmic_mutation * m;
            }
            "uv" => w.cp.uv_membrane = d.uv_membrane * m,
            _ => return false,
        }
        true
    }

    #[func]
    fn levers(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(w) = self.world.as_ref() else { return out };
        let d = CellParams::default();
        out.set("mutation", w.cp.uv_mutation / d.uv_mutation);
        out.set("uv", w.cp.uv_membrane / d.uv_membrane);
        out
    }

    #[func]
    fn start(&mut self, rates_path: GString, seed: i64, auto: bool) -> bool {
        let Ok(text) = std::fs::read_to_string(rates_path.to_string()) else { return false };
        let Ok(rates) = serde_json::from_str::<RateTable>(&text) else { return false };
        let cosmos = Cosmos::new(crate::elements_sim::cosmos(seed as u64));
        let r = crate::elements_sim::recipe();
        let mut cp = CellParams { auto, ..Default::default() };
        r.apply_cell(&mut cp);
        self.world = Some(World::new(PoolParams::default(), cp, cosmos, rates, seed as u64, 200));
        self.history.clear();
        true
    }

    #[func]
    fn set_auto(&mut self, on: bool) {
        if let Some(w) = self.world.as_mut() {
            w.cp.auto = on;
        }
    }

    #[func]
    fn step(&mut self, steps: i64, dt: f64) {
        let Some(w) = self.world.as_mut() else { return };
        for _ in 0..steps {
            w.step(dt);
        }
        let last = self.history.back().map_or(-1.0, |h| h[0]);
        if w.t_days - last >= 0.5 {
            let ph = w.phenotype_means();
            self.history.push_back([w.t_days, ph[0], ph[1], ph[2], ph[3], ph[4], w.cells.len() as f64]);
            if self.history.len() > 2000 {
                self.history.pop_front();
            }
        }
    }

    #[func]
    fn patches(&self) -> VarArray {
        let mut arr = VarArray::new();
        let Some(w) = self.world.as_ref() else { return arr };
        let mut count = vec![0i64; w.patches.len()];
        for c in &w.cells {
            count[c.patch] += 1;
        }
        for (k, p) in w.patches.iter().enumerate() {
            let mut d = VarDictionary::new();
            d.set("elevation", p.elevation);
            d.set("wet", w.env.tide_level > p.elevation);
            d.set("raw", p.raw_ka);
            d.set("cells", count[k]);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Per cell: position (patch, y), size, phenotype, state.
    #[func]
    fn cells(&mut self) -> VarArray {
        let mut arr = VarArray::new();
        let Some(w) = self.world.as_ref() else { return arr };
        let rows: Vec<(u64, usize, f64, i64, Vec<u8>, Mode, f64, u32, i64, f64, i64)> = w
            .cells
            .iter()
            .map(|c| {
                (
                    c.id,
                    c.patch,
                    c.y,
                    c.area(),
                    c.genome.clone(),
                    c.mode,
                    c.conf,
                    c.generation,
                    c.products.iter().sum::<i64>(),
                    c.fork.as_ref().map_or(0.0, |f| f.len() as f64 / c.genome.len().max(1) as f64),
                    c.colony.map_or(-1, |k| k as i64),
                )
            })
            .collect();
        for (id, patch, y, area, genome, mode, conf, generation, prods, fork, colony) in rows {
            let info = self.info(&genome);
            let mut d = VarDictionary::new();
            d.set("id", id as i64);
            d.set("patch", patch as i64);
            d.set("y", y);
            d.set("area", area);
            d.set("r", 0.3 * (area.max(1) as f64).sqrt());
            d.set("poly", info.poly_cut);
            d.set("synth", info.synth_cut);
            d.set("amph", info.amph);
            d.set("pigment", info.pigment);
            d.set("genes", info.genes.len() as i64);
            d.set("products", prods);
            d.set("fork", fork);
            d.set("surrogate", mode == Mode::Surrogate);
            d.set("confidence", conf);
            d.set("generation", generation as i64);
            d.set("colony", colony);
            d.set("adhesion", info.adhesion);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Genome, genes (span, role, expressed copies, product chain dict for the fold inset).
    #[func]
    fn cell_detail(&mut self, id: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(w) = self.world.as_ref() else { return d };
        let found = w.cells.iter().find(|c| c.id as i64 == id).or_else(|| w.cells.iter().find(|c| c.parent == Some(id as u64)));
        let Some(c) = found.cloned() else { return d };
        let bits = w.bits;
        let info = self.info(&c.genome);
        d.set("id", c.id as i64);
        d.set("generation", c.generation as i64);
        d.set("genome", &PackedByteArray::from(c.genome.as_slice()));
        let b: Vec<u8> = c.genome.iter().map(|&l| bits[l as usize]).collect();
        d.set("bits", &PackedByteArray::from(b.as_slice()));
        d.set("genome_text", seq_string(&c.genome));
        d.set("fork", c.fork.as_ref().map_or(-1, |f| f.len() as i64));
        d.set("mutations", c.mutations as i64);
        d.set("area", c.area());
        d.set("free", c.free.iter().sum::<i64>());
        d.set("patch", c.patch as i64);
        d.set("surrogate", c.mode == Mode::Surrogate);
        let names = primordium_core::element::NAMES;
        let mut genes = VarArray::new();
        for (k, g) in info.genes.iter().enumerate() {
            let fold = self.fold_of(&g.product);
            let pos: Vec<Vector2> = fold.iter().map(|&(x, y)| Vector2::new(x as f32, y as f32)).collect();
            let mut gd = VarDictionary::new();
            let (r, strength) = role(&g.func);
            gd.set("start", g.start as i64);
            let pr = g.promoter;
            let name = match pr.signal {
                Signal::None => String::new(),
                Signal::Light => "light".into(),
                Signal::Cold => "cold".into(),
                Signal::LiScarce => format!("{} short", polymer::LETTER_NAMES[primordium_core::cell::genetic_code().food as usize]),
                Signal::Mates => "mates".into(),
                Signal::Height => "height".into(),
                Signal::Exposure => "skin".into(),
            };
            // "light+" = on in light; "cold-" = off when cold
            gd.set("promoter", if name.is_empty() { name } else { format!("{name}{}", if pr.on_high { "+" } else { "-" }) });
            gd.set("end", g.end as i64);
            gd.set("role", r);
            gd.set("strength", strength);
            gd.set("copies", c.products.get(k).copied().unwrap_or(0));
            // chain dictionary in the shape the fold inset expects
            gd.set("id", k as i64);
            gd.set("seq", seq_string(&g.product));
            gd.set("letters", &PackedByteArray::from(g.product.as_slice()));
            gd.set("fold", &PackedVector2Array::from(pos.as_slice()));
            let mut top: Vec<(usize, f64)> = (0..N_PAIRS).map(|p| (p, g.func.catalysis[p])).collect();
            top.sort_by(|a, b| b.1.total_cmp(&a.1));
            let mut cat = VarArray::new();
            for (p, cut) in top.into_iter().take(3) {
                let (a, b) = pair_members(p);
                cat.push(&format!("{}-{} {:.2} kT", names[BONDABLE[a] as usize - 1], names[BONDABLE[b] as usize - 1], cut).to_variant());
            }
            gd.set("catalysis", &cat);
            gd.set("stability", g.func.stability);
            gd.set("amphiphile", g.func.amphiphile);
            gd.set("pigment", g.func.pigment);
            let f = polymer::Fold { pos: fold, energy: 0.0 };
            let mut sites = PackedVector2Array::new();
            for (i, j, _) in polymer::sites(&f) {
                sites.push(pos[i]);
                sites.push(pos[j]);
            }
            gd.set("sites", &sites);
            genes.push(&gd.to_variant());
        }
        d.set("genes", &genes);
        d
    }

    /// Engulfings of the last `days`: patch, predator y, prey y, age in days, predator id.
    #[func]
    fn meals(&self, days: f64) -> VarArray {
        let mut arr = VarArray::new();
        let Some(w) = self.world.as_ref() else { return arr };
        for &(t, patch, py, qy, id) in w.meals.iter().rev() {
            if w.t_days - t > days {
                break;
            }
            let mut d = VarDictionary::new();
            d.set("patch", patch as i64);
            d.set("y", py);
            d.set("prey_y", qy);
            d.set("age", w.t_days - t);
            d.set("id", id as i64);
            arr.push(&d.to_variant());
        }
        arr
    }

    #[func]
    fn follow(&self, id: i64) -> i64 {
        let Some(w) = self.world.as_ref() else { return -1 };
        if w.cells.iter().any(|c| c.id as i64 == id) {
            return id;
        }
        w.cells.iter().find(|c| c.parent == Some(id as u64)).map_or(-1, |c| c.id as i64)
    }

    /// History rows: t, polymerase, metabolism, membrane, pigment, genome length, cells.
    #[func]
    fn history(&self) -> PackedFloat32Array {
        let mut v = Vec::with_capacity(self.history.len() * 7);
        for h in &self.history {
            v.extend(h.iter().map(|&x| x as f32));
        }
        PackedFloat32Array::from(v.as_slice())
    }

    #[func]
    fn env(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(w) = self.world.as_ref() else { return d };
        let e = &w.env;
        for (k, v) in [
            ("t_days", e.t_days),
            ("day_phase", e.day_phase),
            ("sun_height", e.sun_height),
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
        let cp = &w.cosmos.p;
        d.set("moon0_phase", (e.t_days / cp.moon_periods[0]).fract());
        d.set("moon1_phase", (e.t_days / cp.moon_periods[1]).fract());
        d.set("elev_lo", w.cp.elev_lo);
        d.set("elev_hi", w.cp.elev_hi);
        d.set("ledger_drift", 0i64);
        d
    }

    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(w) = self.world.as_ref() else { return d };
        let s = &w.stats;
        d.set("cells", w.cells.len() as i64);
        d.set("surrogate", w.cells.iter().filter(|c| c.mode == Mode::Surrogate).count() as i64);
        d.set("divisions", s.divisions as i64);
        d.set("deaths", s.deaths as i64);
        d.set("eaten", s.predations as i64);
        d.set("max_generation", w.cells.iter().map(|c| c.generation).max().unwrap_or(0) as i64);
        d.set("radiation_mutations", s.radiation_mutations as i64);
        d.set("copy_error", s.copy_mutations as f64 / s.copied_letters.max(1) as f64);
        d.set("lifts", s.lifts as i64);
        d.set("genomes", s.new_genomes as i64);
        d.set("auto", w.cp.auto);
        let mut disc = VarArray::new();
        for e in w.ctx.discoveries.entries.iter().rev().take(5) {
            disc.push(&format!("day {:.1} s{}: {}", e.t_days, e.scale, e.note).to_variant());
        }
        d.set("discoveries", &disc);
        d
    }
}
