//! Scale-4 bridge: a coastline of beaches. Sites run north to south; each is a scale-3 world.
use godot::prelude::*;
use primordium_core::coast::{Coast, CoastParams};
use primordium_core::cosmos::CosmosParams;
use primordium_core::hybrid::Mode;
use primordium_core::surrogate::RateTable;
use std::collections::VecDeque;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct CoastSim {
    coast: Option<Coast>,
    /// Sampled phenotype means per site: (t, then [poly, synth, amph, pigment, cells] per site).
    history: VecDeque<(f64, Vec<[f32; 5]>)>,
    base: Base<Node>,
}

#[godot_api]
impl CoastSim {
    #[func]
    fn start(&mut self, rates_path: GString, seed: i64) -> bool {
        let Ok(text) = std::fs::read_to_string(rates_path.to_string()) else { return false };
        let Ok(rates) = serde_json::from_str::<RateTable>(&text) else { return false };
        self.coast =
            Some(Coast::new(CoastParams { auto: true, ..Default::default() }, CosmosParams { seed: seed as u64, ..Default::default() }, rates, seed as u64));
        self.history.clear();
        true
    }

    #[func]
    fn set_auto(&mut self, on: bool) {
        if let Some(c) = self.coast.as_mut() {
            c.cp.auto = on;
            for s in c.sites.iter_mut() {
                s.world.cp.auto = on;
                if !on {
                    s.mode = Mode::Detail;
                }
            }
        }
    }

    #[func]
    fn step(&mut self, steps: i64, dt: f64) {
        for _ in 0..steps {
            let Some(c) = self.coast.as_mut() else { return };
            c.step(dt);
            // sampled inside the loop: a long warm-up still fills the evolution map
            let last = self.history.back().map_or(-1.0, |h| h.0);
            if c.t_days - last < 1.0 {
                continue;
            }
            let row = c
                .sites
                .iter_mut()
                .map(|s| {
                    let ph = s.world.phenotype_means();
                    [ph[0] as f32, ph[1] as f32, ph[2] as f32, ph[3] as f32, s.world.cells.len() as f32]
                })
                .collect();
            self.history.push_back((c.t_days, row));
            if self.history.len() > 1500 {
                self.history.pop_front();
            }
        }
    }

    #[func]
    fn site_count(&self) -> i64 {
        self.coast.as_ref().map_or(0, |c| c.sites.len() as i64)
    }

    /// Per site: latitude, tide range, local environment, population and phenotype means.
    #[func]
    fn sites(&mut self) -> VarArray {
        let mut arr = VarArray::new();
        let Some(c) = self.coast.as_mut() else { return arr };
        for s in c.sites.iter_mut() {
            let w = &mut s.world;
            let ph = w.phenotype_means();
            let e = w.env;
            let mut d = VarDictionary::new();
            d.set("latitude", s.latitude);
            d.set("tide_scale", s.tide_scale);
            d.set("temperature_k", e.temperature_k);
            d.set("tide_level", e.tide_level);
            d.set("visible", e.visible);
            d.set("uv", e.uv);
            d.set("elev_lo", w.cp.elev_lo);
            d.set("elev_hi", w.cp.elev_hi);
            d.set("cells", w.cells.len() as i64);
            d.set("surrogate", w.cells.iter().filter(|c| c.mode == Mode::Surrogate).count() as i64);
            d.set("max_generation", w.cells.iter().map(|c| c.generation).max().unwrap_or(0) as i64);
            d.set("poly", ph[0]);
            d.set("synth", ph[1]);
            d.set("amph", ph[2]);
            d.set("pigment", ph[3]);
            d.set("genome_len", ph[4]);
            d.set("divisions", w.stats.divisions as i64);
            d.set("eaten", w.stats.predations as i64);
            d.set("bit_error", w.stats.bit_errors as f64 / w.stats.bits_inherited.max(1) as f64);
            d.set("lineage", s.mode == Mode::Surrogate);
            let (colonial, size, adh) = w.colony_stats();
            d.set("colonial", colonial);
            d.set("colony_size", size);
            d.set("adhesion", adh);
            let reg = w.regulation_stats();
            d.set("reg", &PackedFloat32Array::from(&reg.map(|v| v as f32)[..]));
            d.set("confidence", s.conf);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Cells of one site: patch, y, radius, phenotype (genome info), generation, mode.
    #[func]
    fn site_cells(&mut self, k: i64) -> VarArray {
        let mut arr = VarArray::new();
        let Some(c) = self.coast.as_mut() else { return arr };
        let Some(s) = c.sites.get_mut(k as usize) else { return arr };
        let w = &mut s.world;
        let rows: Vec<_> =
            w.cells.iter().map(|c| (c.id, c.patch, c.y, c.area(), c.genome.clone(), c.generation, c.mode, c.colony.map_or(-1, |k| k as i64))).collect();
        for (id, patch, y, area, genome, generation, mode, colony) in rows {
            let info = w.info(&genome);
            let mut d = VarDictionary::new();
            d.set("id", id as i64);
            d.set("patch", patch as i64);
            d.set("y", y);
            d.set("r", 0.3 * (area.max(1) as f64).sqrt());
            d.set("poly", info.poly_cut);
            d.set("synth", info.synth_cut);
            d.set("amph", info.amph);
            d.set("pigment", info.pigment);
            d.set("generation", generation as i64);
            d.set("surrogate", mode == Mode::Surrogate);
            d.set("colony", colony);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Flattened history: per row t, then per site poly, synth, amph, pigment, cells.
    #[func]
    fn history(&self) -> PackedFloat32Array {
        let mut v = Vec::new();
        for (t, row) in &self.history {
            v.push(*t as f32);
            for s in row {
                v.extend_from_slice(s);
            }
        }
        PackedFloat32Array::from(v.as_slice())
    }

    /// Sky and tide as felt at site `k` (for the HUD).
    #[func]
    fn env(&self, k: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(c) = self.coast.as_ref() else { return d };
        let Some(s) = c.sites.get(k as usize) else { return d };
        let e = &s.world.env;
        for (key, v) in [
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
            d.set(key, v);
        }
        let cp = &s.world.cosmos.p;
        d.set("moon0_phase", (e.t_days / cp.moon_periods[0]).fract());
        d.set("moon1_phase", (e.t_days / cp.moon_periods[1]).fract());
        d.set("ledger_drift", 0i64);
        d
    }

    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(c) = self.coast.as_ref() else { return d };
        d.set("cells", c.cells() as i64);
        d.set("migrations", c.stats.migrations as i64);
        d.set("lineage_sites", c.sites.iter().filter(|s| s.mode == Mode::Surrogate).count() as i64);
        d.set("lifts", c.stats.lifts as i64);
        d.set("auto", c.cp.auto);
        d.set("surrogate", c.sites.iter().map(|s| s.world.cells.iter().filter(|c| c.mode == Mode::Surrogate).count()).sum::<usize>() as i64);
        let coast_disc = c.discoveries.entries.iter().rev().take(3).map(|e| (e.t_days, format!("day {:.1}: {}", e.t_days, e.note)));
        let mut disc: Vec<(f64, String)> = c
            .sites
            .iter()
            .flat_map(|s| {
                s.world.ctx.discoveries.entries.iter().rev().take(3).map(move |e| (e.t_days, format!("day {:.1} lat {:+.2}: {}", e.t_days, s.latitude, e.note)))
            })
            .chain(coast_disc)
            .collect();
        disc.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut arr = VarArray::new();
        for (_, s) in disc.into_iter().take(4) {
            arr.push(&s.to_variant());
        }
        d.set("discoveries", &arr);
        d
    }
}
