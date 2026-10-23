//! Scale 4: a coastline of beaches. Each site is a scale-3 world under the same sky at its own
//! latitude and tidal range, so seasons, day length, UV and exposure differ along the shore.
//! Along-shore currents carry cells between neighbouring sites at high water; a migrating cell
//! takes its matter with it, so the coast's matter is closed.
use crate::cell::{CellParams, World};
use crate::cosmos::{Cosmos, CosmosParams};
use crate::pool::PoolParams;
use crate::rng::Rng;
use crate::surrogate::RateTable;
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct CoastParams {
    pub sites: usize,
    /// Latitudes spanned, radians (south to north).
    pub lat_lo: f64,
    pub lat_hi: f64,
    /// Tidal range multiplier at the ends vs the middle of the coast (funnelled bays).
    pub tide_spread: f64,
    /// Chance per day a cell on a flooded patch rides the current to a neighbouring site.
    pub current: f64,
    pub cells_per_site: usize,
    pub founders_per_site: usize,
}

impl Default for CoastParams {
    fn default() -> Self {
        Self { sites: 12, lat_lo: -1.1, lat_hi: 1.1, tide_spread: 1.8, current: 0.02, cells_per_site: 200, founders_per_site: 40 }
    }
}

#[derive(Clone)]
pub struct Site {
    pub latitude: f64,
    pub tide_scale: f64,
    pub world: World,
}

#[derive(Clone, Default, Debug)]
pub struct CoastStats {
    pub migrations: u64,
}

#[derive(Clone)]
pub struct Coast {
    pub cp: CoastParams,
    pub sites: Vec<Site>,
    pub t_days: f64,
    pub stats: CoastStats,
    rng: Rng,
}

impl Coast {
    pub fn new(cp: CoastParams, cosmos: CosmosParams, rates: RateTable, seed: u64) -> Self {
        let n = cp.sites;
        let sites = (0..n)
            .map(|k| {
                let f = if n > 1 { k as f64 / (n - 1) as f64 } else { 0.5 };
                let latitude = cp.lat_lo + (cp.lat_hi - cp.lat_lo) * f;
                // bays at both ends funnel the tide; the open middle has the smallest range
                let tide_scale = 1.0 + (cp.tide_spread - 1.0) * (2.0 * f - 1.0).powi(2);
                let mut c = cosmos.clone();
                c.latitude = latitude;
                c.tide_amps = c.tide_amps.map(|a| a * tide_scale);
                let cell = CellParams { max_cells: cp.cells_per_site, ..Default::default() };
                let world = World::new(
                    PoolParams::default(),
                    cell,
                    Cosmos::new(c),
                    rates.clone(),
                    seed.wrapping_mul(1000).wrapping_add(k as u64),
                    cp.founders_per_site,
                );
                Site { latitude, tide_scale, world }
            })
            .collect();
        Self { cp, sites, t_days: 0.25, stats: CoastStats::default(), rng: Rng::new(seed ^ 0xC0A5) }
    }

    pub fn step(&mut self, dt: f64) {
        self.sites.par_iter_mut().for_each(|s| s.world.step(dt));
        self.migrate(dt);
        self.t_days += dt;
    }

    /// Cells on flooded patches drift along shore to the same patch of a neighbouring site.
    fn migrate(&mut self, dt: f64) {
        let p = (self.cp.current * dt).min(1.0);
        let n = self.sites.len();
        if n < 2 {
            return;
        }
        let mut moving = Vec::new();
        for k in 0..n {
            let w = &mut self.sites[k].world;
            let tide = w.env.tide_level;
            let mut i = 0;
            while i < w.cells.len() {
                let wet = w.patches[w.cells[i].patch].elevation < tide;
                if wet && self.rng.chance(p) {
                    let to = if k == 0 {
                        1
                    } else if k == n - 1 || self.rng.chance(0.5) {
                        k - 1
                    } else {
                        k + 1
                    };
                    moving.push((to, w.cells.swap_remove(i)));
                } else {
                    i += 1;
                }
            }
        }
        self.stats.migrations += moving.len() as u64;
        for (to, cell) in moving {
            let w = &mut self.sites[to].world;
            w.cells.push(cell);
        }
    }

    /// Per-letter matter and Ka over every site (cells, patches, oceans).
    pub fn matter(&mut self) -> (Vec<i64>, i64) {
        let mut m = Vec::new();
        let mut ka = 0;
        for s in self.sites.iter_mut() {
            let (sm, sk) = s.world.matter();
            if m.is_empty() {
                m = sm.to_vec();
            } else {
                for (a, b) in m.iter_mut().zip(sm) {
                    *a += b;
                }
            }
            ka += sk;
        }
        (m, ka)
    }

    pub fn cells(&self) -> usize {
        self.sites.iter().map(|s| s.world.cells.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> RateTable {
        serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/rates0.json")).unwrap()).unwrap()
    }

    #[test]
    fn coast_matter_closed_and_cells_migrate() {
        let cp = CoastParams { sites: 4, cells_per_site: 80, founders_per_site: 20, current: 0.5, ..Default::default() };
        let mut c = Coast::new(cp, CosmosParams::default(), rates(), 3);
        let start = c.matter();
        for _ in 0..300 {
            c.step(0.02);
        }
        assert_eq!(c.matter(), start);
        assert!(c.stats.migrations > 0, "no cell ever migrated: test would pass vacuously");
    }
}
