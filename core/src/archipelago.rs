//! Scale 5b: several small reefs (islands) under one sky, each with its own shore, linked only
//! by spores that drift off one island's deep edge and wash up on another. Isolation plus
//! different shores pull one founder's descendants apart: the start of species.
//!
//! Matter stays closed across the whole archipelago: a crossing spore takes its atoms with it.
use crate::cosmos::CosmosParams;
use crate::polymer::N_LETTERS;
use crate::reef::{Organism, Reef, ReefParams, ReefSeed};
use crate::rng::Rng;
use crate::surrogate::RateTable;

/// One island's shore: multipliers on the base reef's rates, and its slope.
#[derive(Clone, Debug)]
pub struct Island {
    pub name: &'static str,
    pub waves: f64,
    pub uv: f64,
    pub food: f64,
    /// Elevation of its deepest and highest rows (tide units).
    pub elev: (f64, f64),
}

/// Calm deep lagoon, surf-beaten point, sunlit shallow flats.
pub fn default_islands() -> Vec<Island> {
    vec![
        Island { name: "Lagoon", waves: 0.25, uv: 0.5, food: 1.0, elev: (0.0, 0.8) },
        Island { name: "Surf Point", waves: 4.0, uv: 1.0, food: 1.5, elev: (0.05, 0.95) },
        Island { name: "Sun Flats", waves: 1.0, uv: 4.0, food: 1.0, elev: (0.2, 1.0) },
    ]
}

#[derive(Clone, Debug)]
pub struct ArchParams {
    /// Island size in tiles (each smaller than the full reef).
    pub width: usize,
    pub depth: usize,
    /// Chance per day that a drifter over an island's deep edge is carried out to sea.
    pub crossing: f64,
    /// Days at sea before an arrival can settle.
    pub travel_days: f64,
}

impl Default for ArchParams {
    fn default() -> Self {
        Self { width: 16, depth: 14, crossing: 0.01, travel_days: 2.0 }
    }
}

/// Net genome divergence that counts as two kinds, and the days it must hold. Calibrated with
/// `primordium archipelago` over 6 seeds: the split is mostly a founder effect when an island is
/// first reached (islands settle in ~200-400 days and sit at 0.7-1.0%); x4 currents jump past
/// 1% at colonisation, then gene flow erodes them to 0.3-0.6% within ~100 days.
pub const SPLIT_GOAL: f64 = 0.008;
pub const SPLIT_HOLD: f64 = 150.0;

/// The best split, checked once a sim day (fast-forward included): how long one lineage on one
/// pair of islands has stayed past SPLIT_GOAL.
#[derive(Clone, Debug)]
pub struct SplitWatch {
    /// Last check: (lineage, island a, island b, divergence).
    pub last: Option<(u32, usize, usize, f64)>,
    /// Day the current pair rose past the goal and has stayed there since (-1: not now).
    pub since: f64,
    next: f64,
}

impl Default for SplitWatch {
    fn default() -> Self {
        Self { last: None, since: -1.0, next: 0.0 }
    }
}

pub struct Archipelago {
    pub ap: ArchParams,
    pub watch: SplitWatch,
    pub specs: Vec<Island>,
    pub islands: Vec<Reef>,
    /// Arrivals so far, [from][to].
    pub crossings: Vec<Vec<u64>>,
    rng: Rng,
}

impl Archipelago {
    /// Islands are `base` (the world's reef: its sea, its rates) with each spec's shore.
    pub fn new(ap: ArchParams, specs: Vec<Island>, base: ReefParams, cosmos: CosmosParams, rates: RateTable, seed_data: &ReefSeed, seed: u64) -> Self {
        let d = base;
        let islands = specs
            .iter()
            .enumerate()
            .map(|(k, s)| {
                // the ocean reservoir scales with the island so food per tile matches the reef
                let area = (ap.width * ap.depth) as f64 / (d.width * d.depth) as f64;
                let rp = ReefParams {
                    width: ap.width,
                    depth: ap.depth,
                    founders: 0,
                    elev_lo: s.elev.0,
                    elev_hi: s.elev.1,
                    drag: d.drag * s.waves,
                    uv_death: d.uv_death * s.uv,
                    uptake: d.uptake * s.food,
                    ocean_tiles: d.ocean_tiles * area,
                    max_cells: (d.max_cells as f64 * area) as usize,
                    ..d.clone()
                };
                Reef::new(rp, cosmos.clone(), rates.clone(), seed_data, seed.wrapping_add(k as u64 * 7919))
            })
            .collect();
        let n = specs.len();
        Self { ap, watch: SplitWatch::default(), specs, islands, crossings: vec![vec![0; n]; n], rng: Rng::new(seed ^ 0xA2C1) }
    }

    pub fn t_days(&self) -> f64 {
        self.islands.first().map_or(0.0, |r| r.t_days)
    }

    pub fn step(&mut self, dt: f64) {
        for r in &mut self.islands {
            r.step(dt);
        }
        let n = self.islands.len();
        if n < 2 {
            return;
        }
        let mut sea: Vec<(usize, Organism)> = Vec::new();
        for k in 0..n {
            for o in self.islands[k].emigrate(self.ap.crossing, dt) {
                sea.push((k, o));
            }
        }
        for (from, o) in sea {
            let to = (from + 1 + self.rng.below(n - 1)) % n;
            self.crossings[from][to] += 1;
            self.islands[to].immigrate(o, self.ap.travel_days);
        }
        self.watch_split();
    }

    /// Once a sim day: is the best split past the goal, and since when? Another lineage taking
    /// the lead starts its own count; the same lineage on another island pair keeps it (with two
    /// pairs close, sampling noise swaps the leader from day to day).
    fn watch_split(&mut self) {
        let t = self.t_days();
        if t < self.watch.next {
            return;
        }
        self.watch.next = t.floor() + 1.0;
        let b = self.best_split(30);
        let same = match (b, self.watch.last) {
            (Some(x), Some(y)) => x.0 == y.0,
            _ => false,
        };
        match b {
            Some(x) if x.3 >= SPLIT_GOAL => {
                if self.watch.since < 0.0 || !same {
                    self.watch.since = t;
                }
            }
            _ => self.watch.since = -1.0,
        }
        self.watch.last = b;
    }

    /// Days the current split has held past SPLIT_GOAL (0 when it is not past it).
    pub fn split_held(&self) -> f64 {
        if self.watch.since < 0.0 { 0.0 } else { self.t_days() - self.watch.since }
    }

    /// Release a creature on island `k` as a new lineage; the lineage number is shared by every
    /// island. Returns (lineage, spores placed).
    pub fn release(&mut self, k: usize, genome: &[u8], mass: [i64; N_LETTERS], n: usize) -> Option<(u32, usize)> {
        let next = self.islands.iter().map(|r| r.next_clade).max().unwrap_or(0);
        self.islands[k].next_clade = next;
        let got = self.islands[k].release(genome, mass, (0.0, 0.5), n)?;
        for r in &mut self.islands {
            r.next_clade = next + 1;
        }
        Some(got)
    }

    /// Matter over every island (closed: crossings only move it).
    pub fn matter(&self) -> ([i64; N_LETTERS], i64) {
        let mut m = [0i64; N_LETTERS];
        let mut ka = 0;
        for r in &self.islands {
            let (a, b) = r.matter();
            for l in 0..N_LETTERS {
                m[l] += a[l];
            }
            ka += b;
        }
        (m, ka)
    }

    /// Cells of lineage `clade` on each island.
    pub fn spread(&self, clade: u32) -> Vec<usize> {
        self.islands.iter().map(|r| r.organisms.iter().filter(|o| o.clade == clade && o.settled()).map(|o| o.cells.len()).sum()).collect()
    }

    /// How far apart lineage `clade` has grown between islands `a` and `b`: net genome
    /// divergence (mean letters differing per letter between islands, minus the mean within
    /// each), over up to `sample` settled bodies per island. None when either island has fewer
    /// than 3 bodies of it.
    pub fn divergence(&self, clade: u32, a: usize, b: usize, sample: usize) -> Option<f64> {
        fn pick(r: &Reef, clade: u32, sample: usize) -> Vec<&[u8]> {
            let all: Vec<&[u8]> = r.organisms.iter().filter(|o| o.clade == clade && o.settled()).map(|o| o.genome.as_slice()).collect();
            let step = (all.len() / sample.max(1)).max(1);
            all.into_iter().step_by(step).take(sample).collect()
        }
        let (ga, gb) = (pick(&self.islands[a], clade, sample), pick(&self.islands[b], clade, sample));
        if ga.len() < 3 || gb.len() < 3 {
            return None;
        }
        let mean = |x: &[&[u8]], y: &[&[u8]], same: bool| {
            let (mut s, mut n) = (0.0, 0usize);
            for (i, p) in x.iter().enumerate() {
                for (j, q) in y.iter().enumerate() {
                    if same && j <= i {
                        continue;
                    }
                    s += distance(p, q);
                    n += 1;
                }
            }
            if n == 0 { 0.0 } else { s / n as f64 }
        };
        Some(mean(&ga, &gb, false) - 0.5 * (mean(&ga, &ga, true) + mean(&gb, &gb, true)))
    }

    /// The lineage and island pair grown furthest apart: (clade, a, b, divergence).
    pub fn best_split(&self, sample: usize) -> Option<(u32, usize, usize, f64)> {
        let mut clades: Vec<u32> = self.islands.iter().flat_map(|r| r.organisms.iter().map(|o| o.clade)).collect();
        clades.sort_unstable();
        clades.dedup();
        let n = self.islands.len();
        let mut best: Option<(u32, usize, usize, f64)> = None;
        for c in clades {
            for a in 0..n {
                for b in a + 1..n {
                    if let Some(d) = self.divergence(c, a, b, sample) {
                        if best.is_none_or(|x| d > x.3) {
                            best = Some((c, a, b, d));
                        }
                    }
                }
            }
        }
        best
    }
}

/// Letters differing per letter (length difference counts as differing).
pub fn distance(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len().max(b.len()).max(1);
    let same = a.iter().zip(b).filter(|(x, y)| x == y).count();
    (n - same) as f64 / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spores crossing between islands carry their atoms: the archipelago's matter is exact,
    /// and the lineage reaches a second island.
    #[test]
    fn crossings_keep_matter_closed() {
        let dir = format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), crate::element::data_dir());
        let rates: RateTable = serde_json::from_str(&std::fs::read_to_string(format!("{dir}rates0.json")).unwrap()).unwrap();
        let rs: ReefSeed = serde_json::from_str(&std::fs::read_to_string(format!("{dir}reef_seed.json")).unwrap()).unwrap();
        let ap = ArchParams { width: 6, depth: 6, crossing: 2.0, travel_days: 0.5 };
        let mut a = Archipelago::new(ap, default_islands(), ReefParams::default(), CosmosParams::default(), rates, &rs, 3);
        let m0 = a.matter();
        let f = &rs.founders[0];
        let genome: Vec<u8> = f.genome.iter().map(|&l| a.islands[0].canon_letter(l)).collect();
        let (clade, _) = a.release(0, &genome, f.mass.map(|m| m.clamp(0, 40)), 20).expect("release");
        for _ in 0..3000 {
            a.step(0.02);
        }
        assert_eq!(a.matter(), m0, "matter changed across crossings");
        assert!(a.crossings.iter().flatten().sum::<u64>() > 0, "nothing crossed");
        assert!(a.spread(clade).iter().filter(|&&n| n > 0).count() >= 2, "lineage on one island only: {:?}", a.spread(clade));
    }
}
