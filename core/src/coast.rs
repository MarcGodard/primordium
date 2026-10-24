//! Scale 4: a coastline of beaches. Each site is a scale-3 world under the same sky at its own
//! latitude and tidal range, so seasons, day length, UV and exposure differ along the shore.
//! Along-shore currents carry cells between neighbouring sites at high water; a migrating cell
//! takes its matter with it, so the coast's matter is closed.
use crate::cell::{CellParams, World};
use crate::cosmos::{Cosmos, CosmosParams};
use crate::hybrid::Mode;
use crate::lineage::{LineageModel, LineageSample};
use crate::pool::PoolParams;
use crate::rng::Rng;
use crate::surrogate::{Discovery, RateTable};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct CoastParams {
    pub sites: usize,
    /// Latitudes spanned, radians (south to north).
    pub lat_lo: f64,
    pub lat_hi: f64,
    /// Tidal range multiplier at the ends vs the middle of the coast (funnelled bays).
    pub tide_spread: f64,
    /// Along-shore current, m/s. A flooded cell reaches the next site at this speed over the
    /// ground distance between sites on an Earth-sized planet (~1270 km for 12 sites).
    pub current_m_s: f64,
    pub cells_per_site: usize,
    pub founders_per_site: usize,
    /// Every site's sea: the world recipe (CellParams::sea, and x `ocean_ka`).
    pub sea: [f64; crate::polymer::N_LETTERS],
    pub sea_ka: f64,
    /// Sites switch between detail and lineage mode on their own (by lineage-table confidence).
    pub auto: bool,
    pub lift_below: f64,
    pub restrict_above: f64,
    /// Lineage-table memory, days: evolution moves the rates, old evidence fades.
    pub memory_days: f64,
    /// Minimum days a site stays in a mode after switching (unless it leaves the trained range).
    pub dwell_days: f64,
}

impl Default for CoastParams {
    fn default() -> Self {
        Self {
            sites: 12,
            lat_lo: -1.1,
            lat_hi: 1.1,
            tide_spread: 1.8,
            // a typical coastal current; at Earth's size it carries ~2% of flooded cells a day
            current_m_s: 0.3,
            cells_per_site: 200,
            founders_per_site: 40,
            sea: [1.0; crate::polymer::N_LETTERS],
            sea_ka: 1.0,
            auto: false,
            lift_below: 0.5,
            restrict_above: 0.85,
            memory_days: 30.0,
            dwell_days: 5.0,
        }
    }
}

#[derive(Clone)]
pub struct Site {
    pub latitude: f64,
    pub tide_scale: f64,
    pub world: World,
    /// Detail: cells run their chemistry. Surrogate: cells are lineages (`lineage.rs`).
    pub mode: Mode,
    pub conf: f64,
    /// Day of the last mode switch.
    pub since: f64,
}

#[derive(Clone, Default, Debug)]
pub struct CoastStats {
    pub migrations: u64,
    pub lifts: u64,
    pub restricts: u64,
}

#[derive(Clone)]
pub struct Coast {
    pub cp: CoastParams,
    pub sites: Vec<Site>,
    pub t_days: f64,
    pub stats: CoastStats,
    /// Lineage behaviour learned from every detailed site: one coast-wide table.
    pub model: LineageModel,
    pub learn: bool,
    pub discoveries: crate::surrogate::DiscoveryLog,
    /// Ground distance between neighbouring sites, km.
    pub spacing_km: f64,
    steps: u64,
    rng: Rng,
}

impl Coast {
    pub fn new(cp: CoastParams, cosmos: CosmosParams, rates: RateTable, seed: u64) -> Self {
        let n = cp.sites;
        let spacing_km = if n > 1 { cosmos.meridian_km(cp.lat_hi, cp.lat_lo) / (n - 1) as f64 } else { f64::INFINITY };
        let sites = (0..n)
            .map(|k| {
                let f = if n > 1 { k as f64 / (n - 1) as f64 } else { 0.5 };
                let latitude = cp.lat_lo + (cp.lat_hi - cp.lat_lo) * f;
                // bays at both ends funnel the tide; the open middle has the smallest range
                let tide_scale = 1.0 + (cp.tide_spread - 1.0) * (2.0 * f - 1.0).powi(2);
                let mut c = cosmos.clone();
                c.latitude = latitude;
                c.tide_amps = c.tide_amps.map(|a| a * tide_scale);
                let d = CellParams::default();
                let cell = CellParams { max_cells: cp.cells_per_site, sea: cp.sea, ocean_ka: d.ocean_ka * cp.sea_ka, ..d };
                let world = World::new(
                    PoolParams::default(),
                    cell,
                    Cosmos::new(c),
                    rates.clone(),
                    seed.wrapping_mul(1000).wrapping_add(k as u64),
                    cp.founders_per_site,
                );
                Site { latitude, tide_scale, world, mode: Mode::Detail, conf: 0.0, since: f64::NEG_INFINITY }
            })
            .collect();
        Self {
            cp,
            sites,
            t_days: 0.25,
            stats: CoastStats::default(),
            model: LineageModel::default(),
            learn: true,
            discoveries: Default::default(),
            spacing_km,
            steps: 0,
            rng: Rng::new(seed ^ 0xC0A5),
        }
    }

    pub fn step(&mut self, dt: f64) {
        self.resolve_modes(dt);
        let prior = self.model.priors();
        let (model, learn) = (&self.model, self.learn);
        let logs: Vec<Vec<LineageSample>> = self
            .sites
            .par_iter_mut()
            .map(|s| match s.mode {
                Mode::Detail => {
                    s.world.lineage_log = learn.then(Vec::new);
                    s.world.step(dt);
                    s.world.lineage_log.take().unwrap_or_default()
                }
                Mode::Surrogate => {
                    s.world.step_lineage(dt, model, &prior);
                    Vec::new()
                }
            })
            .collect();
        // forget only while new evidence arrives: with every site in lineage mode nothing is
        // recorded, and fading anyway would wash the table out to its prior at full confidence
        if self.learn && logs.iter().any(|l| !l.is_empty()) {
            self.model.forget(dt, self.cp.memory_days);
        }
        for log in &logs {
            self.model.record(log, dt);
        }
        self.migrate(dt);
        self.t_days += dt;
    }

    /// Put every site in one mode (shadow runs, experiments). Auto may move them later.
    pub fn set_mode(&mut self, mode: Mode) {
        for s in self.sites.iter_mut() {
            s.mode = mode;
            s.since = self.t_days;
        }
    }

    /// Confidence is checked about once a day: it moves slowly, and scoring every cell is not free.
    fn resolve_modes(&mut self, dt: f64) {
        let per_day = (1.0 / dt).round().max(1.0) as u64;
        self.steps += 1;
        if (self.steps - 1) % per_day != 0 {
            return;
        }
        let (t, dwell) = (self.t_days, self.cp.dwell_days);
        for (k, s) in self.sites.iter_mut().enumerate() {
            s.conf = s.world.lineage_confidence(&self.model);
            // a switch holds for `dwell` days: seasonal dips into thin bins otherwise flip sites
            // back and forth daily. Leaving the trained range altogether still lifts at once.
            let settled = t - s.since >= dwell;
            match s.mode {
                Mode::Surrogate if s.conf < self.cp.lift_below && (settled || s.conf < 0.05) => {
                    s.mode = Mode::Detail;
                    s.since = t;
                    self.stats.lifts += 1;
                    self.discoveries.log(
                        4_000_000 + k,
                        Discovery {
                            t_days: self.t_days,
                            scale: 4,
                            region: k,
                            confidence: s.conf,
                            features: vec![s.latitude, s.world.env.temperature_k],
                            note: format!(
                                "site {:+.0} deg left known conditions ({:.1} K): back to cell chemistry",
                                s.latitude.to_degrees(),
                                s.world.env.temperature_k
                            ),
                        },
                    );
                }
                Mode::Detail if self.cp.auto && settled && s.conf >= self.cp.restrict_above => {
                    s.mode = Mode::Surrogate;
                    s.since = t;
                    self.stats.restricts += 1;
                }
                _ => {}
            }
        }
    }

    /// Cells on flooded patches drift along shore to the same patch of a neighbouring site.
    fn migrate(&mut self, dt: f64) {
        // km travelled in dt over the distance to the next site
        let p = (self.cp.current_m_s * 86.4 * dt / self.spacing_km).min(1.0);
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
                // colony cells hold on to the beach and each other: the current takes loners
                let wet = w.patches[w.cells[i].patch].elevation < tide && w.cells[i].colony.is_none();
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

    /// Fresh dice everywhere, same state: shadow runs.
    pub fn reseed(&mut self, seed: u64) {
        for (k, s) in self.sites.iter_mut().enumerate() {
            s.world.reseed(seed.wrapping_mul(31).wrapping_add(k as u64));
        }
        self.rng = Rng::new(seed ^ 0xC0A5);
    }

    pub fn cells(&self) -> usize {
        self.sites.iter().map(|s| s.world.cells.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> RateTable {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), crate::element::data_file("rates0.json"))).unwrap()).unwrap()
    }

    #[test]
    fn coast_matter_closed_and_cells_migrate() {
        let cp = CoastParams { sites: 4, cells_per_site: 80, founders_per_site: 20, current_m_s: 10.0, ..Default::default() };
        let mut c = Coast::new(cp, CosmosParams::default(), rates(), 3);
        let start = c.matter();
        for _ in 0..300 {
            c.step(0.02);
        }
        assert_eq!(c.matter(), start);
        assert!(c.stats.migrations > 0, "no cell ever migrated: test would pass vacuously");
    }

    #[test]
    fn lineage_mode_keeps_matter_and_breeds() {
        let cp = CoastParams { sites: 3, cells_per_site: 60, founders_per_site: 20, lift_below: 0.0, ..Default::default() };
        let mut c = Coast::new(cp, CosmosParams::default(), rates(), 5);
        for _ in 0..1500 {
            c.step(0.02);
        }
        c.set_mode(Mode::Surrogate);
        let start = c.matter();
        let d0: u64 = c.sites.iter().map(|s| s.world.stats.divisions).sum();
        for _ in 0..500 {
            c.step(0.02);
        }
        let d1: u64 = c.sites.iter().map(|s| s.world.stats.divisions).sum();
        assert!(d1 > d0, "no lineage division: test would pass vacuously");
        assert_eq!(c.matter(), start);
    }
}
