//! Scale 5: a reef. Organisms are bodies of cells on a 3D lattice, rooted on a rocky shore that
//! runs from deep water (row 0) up to a dry dune, under the same sky and tides as every scale.
//!
//! A body cell is a scale-4 lineage cell: it grows and divides or bursts at the rates the coast
//! learned for its regulated traits in its conditions (`LineageModel`). What is new here is
//! geometry, which every cell feels through its own position:
//!   - light: water depth and every cell above it in its column shade it (pigment shades more)
//!   - food: monomers come in through faces open to water, then are shared through the body
//!   - waves: drag grows with body height, grip with the adhesion its rock-touching cells express;
//!     when drag wins the body tears and the pieces drift off (fragmentation)
//!   - grazing: a sticky-skinned body bites cells off smaller neighbours on its tile
//!
//! Gene regulation reads two body signals (height and exposure), so cells of one genome can
//! take different jobs by position. A dividing cell keeps its daughter when the adhesion it
//! expresses holds (as scale-3 colonies); a daughter that comes loose is a spore: it drifts,
//! settles and founds a new body, with a genome copied at the learned per-division bit error.
//! A body is one clone: somatic divisions copy the genome exactly, heredity's errors enter at
//! spores (the germline). Matter is closed over tiles, ocean and bodies, per letter and Ka.
use crate::cell::{BODY_HEIGHT, CellParams, GenomeInfo, Patch, Tide, World, pick_weighted, regulated_traits, round_flux, signals};
use crate::cosmos::{Cosmos, CosmosParams, EnvSample};
use crate::lineage::{LineageModel, lineage_x};
use crate::polymer::{N_LETTERS, Seq};
use crate::pool::{PoolParams, Site};
use crate::rng::Rng;
use crate::surrogate::{Discovery, DiscoveryLog, RateTable};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// Lattice cells per terrain tile side.
pub const TILE: i32 = 6;
/// Elevation (tide units) one cell of body height adds: a tall body reaches toward the surface.
pub const CELL_RISE: f64 = 0.012;
const DIRS: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
/// Founder body size cap, letters.
const FOUNDER_MASS: f64 = 120.0;
/// Li's index among the letters (Hy, Li, Bo, Ox, Fl).
const LI: usize = 1;

/// What the coast hands the reef: its learned lineage model (how a cell with given traits
/// fares in given conditions) and founder cells drawn from its evolved population.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReefSeed {
    pub days: f64,
    pub model: LineageModel,
    pub founders: Vec<Founder>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Founder {
    pub genome: Seq,
    pub latitude: f64,
    pub colonial: bool,
    /// What the cell is built of apart from its genome (membrane, products, junk), per letter.
    pub mass: [i64; N_LETTERS],
}

#[derive(Clone, Debug)]
pub struct ReefParams {
    /// Tiles along the shore and from deep water up the beach.
    pub width: usize,
    pub depth: usize,
    /// Elevation of the deepest and highest rows (tide units, as the beach patches).
    pub elev_lo: f64,
    pub elev_hi: f64,
    /// Height of rock outcrops and pools on the slope.
    pub roughness: f64,
    pub latitude: f64,
    /// Tile water volume in cells: sets concentrations, as a beach patch.
    pub tile_cells: f64,
    pub ocean_raw: f64,
    pub ocean_ka: f64,
    /// Ocean reservoir, in tiles' worth of ocean-strength water.
    pub ocean_tiles: f64,
    pub flush_rate: f64,
    pub washout_rate: f64,
    pub founders: usize,
    pub max_cells: usize,
    /// Monomers taken in per open wet face per day at one monomer per cell volume.
    pub uptake: f64,
    /// Store per cell, per letter, at which uptake of that letter stops (the body is saturated).
    pub store_cap: f64,
    /// Uptake gain per cell of height: the water just over the rock is drained by what lives
    /// there, flow above it is fresh (a boundary layer).
    pub boundary: f64,
    /// Elongation per unit of expressed membrane builder when a daughter is placed: a cell
    /// lengthened by membrane divides across its length, putting the daughter on top (up weighs
    /// 1 + elongate x membrane, sideways the inverse).
    pub elongate: f64,
    /// Cell deaths per day: steady turnover, plus per unit of UV reaching the cell (UV erodes
    /// membranes; the canopy's and the cell's own pigment shade it).
    pub turnover: f64,
    pub uv_death: f64,
    /// Light a plain cell blocks for those below it; pigment adds up to `max_shade`.
    pub base_shade: f64,
    pub max_shade: f64,
    /// Drag per cell per unit wave energy; grip per rock-touching cell, plain and per unit adhesion.
    pub drag: f64,
    pub base_grip: f64,
    pub grip: f64,
    /// Tears per day per unit of drag over grip.
    pub break_rate: f64,
    /// A daughter stays attached at expressed adhesion floor + span (none below the floor).
    pub stick_floor: f64,
    pub stick_span: f64,
    /// Days a spore or fragment drifts before it settles; adrift deaths per day.
    pub drift_days: f64,
    pub drift_death: f64,
    /// Grazing: skin adhesion past the floor bites, up to `graze_rate` bites per day; prey must
    /// have fewer cells than `graze_size` times the grazer's.
    pub graze_floor: f64,
    pub graze_span: f64,
    pub graze_rate: f64,
    pub graze_size: f64,
    /// Smallest body a grazer can bite, cells: a bite needs something to hold, so a freshly
    /// settled spore slips through (a size refuge for recruits).
    pub graze_min: usize,
    /// Body size at which the mates signal saturates.
    pub mates_full: f64,
}

impl Default for ReefParams {
    fn default() -> Self {
        let cp = CellParams::default();
        Self {
            width: 32,
            depth: 24,
            elev_lo: 0.05,
            elev_hi: 0.95,
            roughness: 0.12,
            latitude: 0.35,
            tile_cells: cp.patch_cells,
            ocean_raw: cp.ocean_raw,
            ocean_ka: cp.ocean_ka,
            ocean_tiles: 2000.0,
            flush_rate: cp.flush_rate,
            washout_rate: cp.washout_rate,
            founders: 240,
            max_cells: 24_000,
            uptake: 150.0,
            store_cap: 60.0,
            boundary: 0.4,
            elongate: 4.0,
            turnover: 0.005,
            uv_death: 0.04,
            base_shade: 0.1,
            max_shade: cp.max_shade,
            drag: 1.0,
            base_grip: 0.5,
            grip: 8.0,
            break_rate: 0.5,
            stick_floor: cp.stick_floor,
            stick_span: cp.stick_span,
            drift_days: 1.0,
            drift_death: 0.3,
            graze_floor: cp.eat_floor,
            graze_span: cp.eat_span,
            graze_rate: 6.0,
            graze_size: 0.6,
            graze_min: 2,
            mates_full: (cp.colony_max - 1) as f64,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BodyCell {
    /// Reef lattice position: x along shore, y up the beach, z cells above the rock.
    pub pos: [i32; 3],
    /// Everything but the cell's genome copy (membrane, products, stores), per letter.
    pub mass: [i64; N_LETTERS],
    pub birth_mass: i64,
    pub born: f64,
    pub ready: bool,
    /// Last step: expressed polymerase, metabolism, membrane, shade, adhesion (display, shading).
    pub expr: [f32; 5],
    /// Last step: light reaching the cell (0..1+), faces open to water, lineage features.
    pub light: f32,
    pub open: u8,
    pub x: [f64; 8],
}

impl BodyCell {
    fn total(&self) -> i64 {
        self.mass.iter().sum()
    }
}

#[derive(Clone, Debug)]
pub struct Organism {
    pub id: u64,
    pub parent: Option<u64>,
    pub genome: Seq,
    pub info: Arc<GenomeInfo>,
    pub generation: u32,
    pub born: f64,
    pub cells: Vec<BodyCell>,
    /// Monomers shared by the whole body: food comes in at the skin and is passed inward.
    pub store: [i64; N_LETTERS],
    /// Days left adrift (spores and torn-off pieces); 0 when settled on the rock.
    pub adrift: f64,
    /// Tile the organism floats over while adrift.
    pub drift_tile: usize,
    /// The founder this body descends from (by spores or fragments): its lineage.
    pub clade: u32,
}

impl Organism {
    pub fn settled(&self) -> bool {
        self.adrift <= 0.0 && self.drift_tile == usize::MAX
    }

    pub fn height(&self) -> i32 {
        self.cells.iter().map(|c| c.pos[2]).max().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReefStats {
    pub divisions: u64,
    pub spores: u64,
    pub settled: u64,
    pub lost_adrift: u64,
    pub tears: u64,
    pub fragments: u64,
    pub bites: u64,
    pub deaths: u64,
    pub bit_errors: u64,
    pub bits_inherited: u64,
    /// Cells last step whose conditions the coast's table barely covers.
    pub unsure_cells: u64,
}

pub struct Reef {
    pub rp: ReefParams,
    /// Reads genomes into genes and folded products (scale 1-3 machinery); runs no cells.
    pub interp: World,
    pub cosmos: Cosmos,
    pub env: EnvSample,
    pub daylight: f64,
    pub t_days: f64,
    pub tiles: Vec<Patch>,
    pub ocean: [i64; N_LETTERS],
    pub ocean_ka: i64,
    pub organisms: Vec<Organism>,
    pub model: LineageModel,
    prior: Vec<f64>,
    pub stats: ReefStats,
    pub discoveries: DiscoveryLog,
    /// Bites for display: (day, grazer id, bitten cell position), last ~200.
    pub meals: VecDeque<(f64, u64, [i32; 3])>,
    /// Occupied lattice sites of settled bodies.
    occ: HashMap<[i32; 3], u64>,
    canon: [u8; 2],
    next_id: u64,
    steps: u64,
    seed: u64,
    rng: Rng,
}

fn hash2(x: i64, y: i64, seed: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed;
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth value noise, 0..1.
fn noise(x: f64, y: f64, seed: u64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let v = |dx: i64, dy: i64| hash2(x0 as i64 + dx, y0 as i64 + dy, seed);
    let a = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let b = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    a + (b - a) * sy
}

/// Letter counts of a genome copy.
fn counts(genome: &[u8]) -> [i64; N_LETTERS] {
    let mut n = [0i64; N_LETTERS];
    for &l in genome {
        n[l as usize] += 1;
    }
    n
}

/// Split counts binomially: (kept, given).
fn halve(m: &mut [i64; N_LETTERS], rng: &mut Rng) -> [i64; N_LETTERS] {
    let mut d = [0i64; N_LETTERS];
    for l in 0..N_LETTERS {
        d[l] = (0..m[l]).filter(|_| rng.chance(0.5)).count() as i64;
        m[l] -= d[l];
    }
    d
}

/// Connected pieces of a body (face neighbours), as cell index lists.
fn pieces(cells: &[BodyCell]) -> Vec<Vec<usize>> {
    let at: HashMap<[i32; 3], usize> = cells.iter().enumerate().map(|(i, c)| (c.pos, i)).collect();
    let mut seen = vec![false; cells.len()];
    let mut out = Vec::new();
    for s in 0..cells.len() {
        if seen[s] {
            continue;
        }
        seen[s] = true;
        let mut piece = vec![s];
        let mut k = 0;
        while k < piece.len() {
            let p = cells[piece[k]].pos;
            for d in DIRS {
                if let Some(&j) = at.get(&[p[0] + d[0], p[1] + d[1], p[2] + d[2]]) {
                    if !seen[j] {
                        seen[j] = true;
                        piece.push(j);
                    }
                }
            }
            k += 1;
        }
        out.push(piece);
    }
    out
}

/// What one organism's cells read in the parallel step.
struct Shared<'a> {
    rp: &'a ReefParams,
    env: EnvSample,
    daylight: f64,
    t: f64,
    tiles: &'a [Patch],
    occ: &'a HashMap<[i32; 3], u64>,
    trans: &'a HashMap<[i32; 3], f64>,
    model: &'a LineageModel,
    prior: &'a [f64],
}

impl Reef {
    pub fn new(rp: ReefParams, cosmos: CosmosParams, rates: RateTable, seed_data: &ReefSeed, seed: u64) -> Self {
        let mut cp = cosmos.clone();
        cp.latitude = rp.latitude;
        let cosmos = Cosmos::new(cp);
        let interp = World::new(PoolParams::default(), CellParams { max_cells: 0, ..Default::default() }, cosmos.clone(), rates, seed, 0);
        let canon: [u8; 2] =
            std::array::from_fn(|b| (0..N_LETTERS).find(|&l| interp.comp[interp.comp[l] as usize] as usize == l && interp.bits[l] as usize == b).unwrap() as u8);
        let tiles: Vec<Patch> = (0..rp.depth)
            .flat_map(|ty| (0..rp.width).map(move |tx| (tx, ty)))
            .map(|(tx, ty)| {
                let f = ty as f64 / (rp.depth - 1).max(1) as f64;
                let (x, y) = (tx as f64, ty as f64);
                let rough = noise(x / 5.0, y / 5.0, seed) * 0.65 + noise(x / 2.0, y / 2.0, seed ^ 7) * 0.35 - 0.5;
                let elevation = (rp.elev_lo + (rp.elev_hi - rp.elev_lo) * f + rp.roughness * 2.0 * rough).clamp(0.0, 1.0);
                Patch {
                    elevation,
                    raw: [(rp.ocean_raw * rp.tile_cells) as i64; N_LETTERS],
                    raw_ka: (rp.ocean_ka * rp.tile_cells) as i64,
                    free: [0; N_LETTERS],
                }
            })
            .collect();
        let ocean = [(rp.ocean_raw * rp.tile_cells * rp.ocean_tiles) as i64; N_LETTERS];
        let ocean_ka = (rp.ocean_ka * rp.tile_cells * rp.ocean_tiles) as i64;
        let env = cosmos.sample(0.25);
        let mut r = Self {
            prior: seed_data.model.priors(),
            model: seed_data.model.clone(),
            rp,
            interp,
            cosmos,
            env,
            daylight: 0.0,
            t_days: 0.25,
            tiles,
            ocean,
            ocean_ka,
            organisms: Vec::new(),
            stats: ReefStats::default(),
            discoveries: DiscoveryLog::default(),
            meals: VecDeque::new(),
            occ: HashMap::new(),
            canon,
            next_id: 1,
            steps: 0,
            seed,
            rng: Rng::new(seed ^ 0x5EEF),
        };
        r.seed_founders(&seed_data.founders);
        r
    }

    /// Founders settle as single cells on the lower, usually flooded half of the shore. Their
    /// matter comes from the ocean.
    fn seed_founders(&mut self, founders: &[Founder]) {
        if founders.is_empty() {
            return;
        }
        let mut rng = std::mem::replace(&mut self.rng, Rng::new(0));
        let low: Vec<usize> = (0..self.tiles.len()).filter(|&k| self.tiles[k].elevation < 0.5).collect();
        if low.is_empty() {
            self.rng = rng;
            return;
        }
        for k in 0..self.rp.founders {
            let f = &founders[k % founders.len()];
            let genome: Seq = f.genome.iter().map(|&l| self.canon[self.interp.bits[l as usize] as usize]).collect();
            // big coast cells start as average reef cells: the founder is a spore, not a whole cell
            let raw_mass = f.mass.map(|m| m.max(0));
            let scale = (FOUNDER_MASS / raw_mass.iter().sum::<i64>().max(1) as f64).min(1.0);
            let mut mass = raw_mass.map(|m| (m as f64 * scale).round() as i64);
            let store = [0i64; N_LETTERS];
            // the ocean pays: genome copy plus body
            let need = counts(&genome);
            for l in 0..N_LETTERS {
                mass[l] = mass[l].min(self.ocean[l] - need[l]).max(0);
                self.ocean[l] -= need[l] + mass[l];
            }
            let total: i64 = need.iter().sum::<i64>() + mass.iter().sum::<i64>();
            self.ocean_ka -= total;
            let tile = low[rng.below(low.len())];
            let spot = self.free_spot(tile, &mut rng);
            let Some(pos) = spot else {
                // no room: the matter stays loose on the tile
                let t = &mut self.tiles[tile];
                for l in 0..N_LETTERS {
                    t.free[l] += need[l] + mass[l];
                }
                continue;
            };
            let info = self.info_now(&genome);
            let id = self.id();
            let cell = BodyCell { pos, birth_mass: mass.iter().sum::<i64>().max(1), mass, born: self.t_days, ready: false, expr: [0.0; 5], light: 0.0, open: 5, x: [0.0; 8] };
            self.occ.insert(pos, id);
            self.organisms.push(Organism {
                id,
                parent: None,
                genome,
                info,
                generation: 0,
                born: self.t_days,
                cells: vec![cell],
                store,
                adrift: 0.0,
                drift_tile: usize::MAX,
                clade: k as u32,
            });
        }
        self.rng = rng;
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id - 1
    }

    /// Genome -> genes and folded products, folding any new product now.
    fn info_now(&mut self, genome: &[u8]) -> Arc<GenomeInfo> {
        let i = self.interp.info(genome);
        if !i.pending {
            return i;
        }
        self.interp.ctx.resolve(self.t_days);
        self.interp.info(genome)
    }

    pub fn tile_index(&self, tx: i32, ty: i32) -> Option<usize> {
        (tx >= 0 && ty >= 0 && (tx as usize) < self.rp.width && (ty as usize) < self.rp.depth).then(|| ty as usize * self.rp.width + tx as usize)
    }

    fn tile_at(&self, pos: [i32; 3]) -> usize {
        let tx = pos[0].div_euclid(TILE).clamp(0, self.rp.width as i32 - 1);
        let ty = pos[1].div_euclid(TILE).clamp(0, self.rp.depth as i32 - 1);
        ty as usize * self.rp.width + tx as usize
    }

    fn in_bounds(&self, p: [i32; 3]) -> bool {
        p[2] >= 0 && p[0] >= 0 && p[1] >= 0 && p[0] < self.rp.width as i32 * TILE && p[1] < self.rp.depth as i32 * TILE
    }

    /// A free rock site (z = 0) on a tile, if one turns up in a few tries.
    fn free_spot(&self, tile: usize, rng: &mut Rng) -> Option<[i32; 3]> {
        let (tx, ty) = ((tile % self.rp.width) as i32, (tile / self.rp.width) as i32);
        (0..8).map(|_| [tx * TILE + rng.below(TILE as usize) as i32, ty * TILE + rng.below(TILE as usize) as i32, 0]).find(|p| !self.occ.contains_key(p))
    }

    /// Wave energy on a tile: breakers just below the waterline, a weaker swell deeper, the tidal
    /// current everywhere wet; storms (cloud) scale it all.
    pub fn wave_energy(&self, tile: usize) -> f64 {
        let d = self.env.tide_level - self.tiles[tile].elevation;
        if d <= 0.0 {
            return 0.0;
        }
        let surf = (-((d - 0.06) / 0.08).powi(2)).exp() + 0.15 * (-d / 0.3).exp();
        let storm = 0.4 + 2.0 * self.env.cloud * self.env.cloud;
        storm * surf + 0.3 * self.env.tide_rate.abs() * (-d / 0.1).exp()
    }

    /// Light passing every cell above each settled cell (pigment shades, plain cells a little).
    fn transmission(&self) -> HashMap<[i32; 3], f64> {
        let mut cols: HashMap<(i32, i32), Vec<(i32, f64)>> = HashMap::new();
        for o in self.organisms.iter().filter(|o| o.settled()) {
            for c in &o.cells {
                let shade = (self.rp.base_shade + c.expr[3] as f64).min(0.95);
                cols.entry((c.pos[0], c.pos[1])).or_default().push((c.pos[2], shade));
            }
        }
        let mut out = HashMap::with_capacity(self.occ.len());
        for ((x, y), mut col) in cols {
            col.sort_by(|a, b| b.0.cmp(&a.0));
            let mut t = 1.0;
            for (z, s) in col {
                out.insert([x, y, z], t);
                t *= 1.0 - s;
            }
        }
        out
    }

    pub fn step(&mut self, dt: f64) {
        self.env = self.cosmos.sample(self.t_days);
        self.daylight = self.cosmos.sample(self.t_days.floor() + 0.5).visible;
        self.interp.env = self.env;
        self.steps += 1;
        let mut rng = std::mem::replace(&mut self.rng, Rng::new(0));
        let tide = Tide { flush: self.rp.flush_rate, washout: self.rp.washout_rate, raw: self.rp.ocean_raw * self.rp.tile_cells, ka: self.rp.ocean_ka * self.rp.tile_cells };
        for p in self.tiles.iter_mut() {
            if self.env.tide_level > p.elevation {
                tide.mix(p, &mut self.ocean, &mut self.ocean_ka, dt, &mut rng);
            }
        }
        self.uptake(dt, &mut rng);
        let trans = self.transmission();
        let sh = Shared {
            rp: &self.rp,
            env: self.env,
            daylight: self.daylight,
            t: self.t_days,
            tiles: &self.tiles,
            occ: &self.occ,
            trans: &trans,
            model: &self.model,
            prior: &self.prior,
        };
        let salt = self.seed ^ self.steps.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let outs: Vec<(Vec<usize>, u64, Option<[f64; 8]>)> =
            self.organisms.par_iter_mut().map(|o| if o.settled() { live(o, &sh, dt, &mut Rng::new(salt ^ o.id)) } else { (Vec::new(), 0, None) }).collect();
        self.stats.unsure_cells = outs.iter().map(|o| o.1).sum();
        if let Some(x) = outs.iter().find_map(|o| o.2) {
            self.log_unsure(x);
        }
        let mut torn = Vec::new();
        for (k, (dead, _, _)) in outs.into_iter().enumerate() {
            if !dead.is_empty() {
                self.kill(k, &dead);
                torn.push(k);
            }
        }
        self.divide(&mut rng);
        torn.extend(self.waves(dt, &mut rng));
        torn.extend(self.graze(dt, &mut rng));
        torn.sort_unstable();
        torn.dedup();
        for k in torn {
            self.split_pieces(k, &mut rng);
        }
        self.drift(dt, &mut rng);
        self.crowd(&mut rng);
        // an empty body's leftover store goes back to its tile
        let mut i = 0;
        while i < self.organisms.len() {
            if self.organisms[i].cells.is_empty() {
                let o = self.organisms.swap_remove(i);
                let t = if o.drift_tile != usize::MAX { o.drift_tile } else { 0 };
                for l in 0..N_LETTERS {
                    self.tiles[t].free[l] += o.store[l];
                }
            } else {
                i += 1;
            }
        }
        self.rng = rng;
        self.t_days += dt;
    }

    fn log_unsure(&mut self, x: [f64; 8]) {
        let key = (x[0] * 20.0) as usize * 1000 + (x[7] * 10.0) as usize * 10 + x[1] as usize;
        self.discoveries.log(
            key,
            Discovery {
                t_days: self.t_days,
                scale: 5,
                region: 0,
                confidence: self.model.confidence(&x),
                features: x.to_vec(),
                note: format!(
                    "body cells in conditions the coast barely saw: kT {:.2}, {}, light {:.2} of noon",
                    x[0],
                    if x[1] > 0.5 { "under water" } else { "in the air" },
                    x[7]
                ),
            },
        );
    }

    /// Food enters through faces open to water and joins the body's shared store: loose monomers
    /// directly, raw atoms paired with Ka as fast as the body's metabolism allows.
    fn uptake(&mut self, dt: f64, rng: &mut Rng) {
        let tide = self.env.tide_level;
        for k in 0..self.organisms.len() {
            if !self.organisms[k].settled() {
                continue;
            }
            let t = self.tile_at(self.organisms[k].cells[0].pos);
            let elev = self.tiles[t].elevation;
            let o = &self.organisms[k];
            let faces: f64 =
                o.cells.iter().filter(|c| elev + c.pos[2] as f64 * CELL_RISE < tide).map(|c| c.open as f64 * (1.0 + self.rp.boundary * c.pos[2] as f64)).sum();
            if faces == 0.0 {
                continue;
            }
            let synth = o.cells.iter().map(|c| c.expr[1] as f64).sum::<f64>() / o.cells.len() as f64;
            let per = self.rp.uptake * faces / self.rp.tile_cells * dt;
            let cap = self.rp.store_cap * o.cells.len() as f64;
            let p = &mut self.tiles[t];
            let o = &mut self.organisms[k];
            for l in 0..N_LETTERS {
                // a saturated body stops taking a letter in: no hoarding what it cannot use
                let per = per * (1.0 - o.store[l] as f64 / cap).max(0.0);
                let n = round_flux(per * p.free[l] as f64, rng).clamp(0, p.free[l]);
                p.free[l] -= n;
                o.store[l] += n;
                // metabolism: a catalysed Ka + side atom pairing, at the body's mean metabolism
                let m = round_flux(per * synth * 4.0 * p.raw[l].min(p.raw_ka) as f64, rng).clamp(0, p.raw[l].min(p.raw_ka));
                p.raw[l] -= m;
                p.raw_ka -= m;
                o.store[l] += m;
            }
        }
    }

    /// Burst cells spill into their tile.
    fn kill(&mut self, k: usize, dead: &[usize]) {
        let mut dead = dead.to_vec();
        dead.sort_unstable();
        let g = counts(&self.organisms[k].genome);
        for &i in dead.iter().rev() {
            let c = self.organisms[k].cells.swap_remove(i);
            self.occ.remove(&c.pos);
            let t = self.tile_at(c.pos);
            for l in 0..N_LETTERS {
                self.tiles[t].free[l] += c.mass[l] + g[l];
            }
            self.stats.deaths += 1;
            if self.organisms[k].cells.is_empty() {
                // the last cell gone: the body's store spills where it stood
                let store = std::mem::take(&mut self.organisms[k].store);
                for l in 0..N_LETTERS {
                    self.tiles[t].free[l] += store[l];
                }
            }
        }
    }

    /// Ready cells that have doubled divide. The daughter's genome copy is paid from the body's
    /// store. Sticky enough: it stays, in a free neighbouring site. Otherwise it leaves as a spore
    /// carrying a copy with heritable errors at the learned rate.
    fn divide(&mut self, rng: &mut Rng) {
        let mut order: Vec<(usize, usize)> =
            self.organisms.iter().enumerate().filter(|(_, o)| o.settled()).flat_map(|(k, o)| (0..o.cells.len()).map(move |i| (k, i))).collect();
        order.retain(|&(k, i)| {
            let c = &self.organisms[k].cells[i];
            c.ready && c.total() >= 2 * c.birth_mass
        });
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        let mut spores = Vec::new();
        for (k, i) in order {
            let o = &self.organisms[k];
            let c = &o.cells[i];
            let adh = c.expr[4] as f64;
            let stick = rng.chance(((adh - self.rp.stick_floor) / self.rp.stick_span).clamp(0.0, 1.0));
            let genome = if stick {
                o.genome.clone()
            } else {
                let p_bit = self.model.bit_error(&c.x, &self.prior);
                let mut g = o.genome.clone();
                let mut errors = 0;
                for l in g.iter_mut() {
                    let mut b = self.interp.bits[*l as usize] as usize;
                    if rng.chance(p_bit[*l as usize]) {
                        b ^= 1;
                        errors += 1;
                    }
                    *l = self.canon[b];
                }
                self.stats.bit_errors += errors;
                self.stats.bits_inherited += g.len() as u64;
                g
            };
            let spot = if stick {
                let p = c.pos;
                // elongated cells divide along their length (up) and seldom sideways: membrane-rich
                // cells build columns, membrane-poor ones spread into crusts. The axis is chosen
                // first; a cell whose chosen side is taken waits (contact inhibition), so a cell
                // under others does not spill sideways into a mound.
                let e = 1.0 + self.rp.elongate * c.expr[2] as f64;
                let dirs: Vec<([i32; 3], f64)> = DIRS
                    .iter()
                    .map(|d| ([p[0] + d[0], p[1] + d[1], p[2] + d[2]], if d[2] > 0 { e } else if d[2] < 0 { 0.5 / e } else { 1.0 / e }))
                    .filter(|(q, _)| self.in_bounds(*q))
                    .collect();
                let total: f64 = dirs.iter().map(|f| f.1).sum();
                let mut r = rng.f64() * total;
                let mut pick = dirs[dirs.len() - 1].0;
                for (q, w) in &dirs {
                    if r < *w {
                        pick = *q;
                        break;
                    }
                    r -= w;
                }
                if self.occ.contains_key(&pick) {
                    continue;
                }
                Some(pick)
            } else {
                None
            };
            let need = counts(&genome);
            let o = &mut self.organisms[k];
            if (0..N_LETTERS).any(|l| o.store[l] < need[l]) {
                continue;
            }
            for l in 0..N_LETTERS {
                o.store[l] -= need[l];
            }
            let t = self.t_days;
            let c = &mut o.cells[i];
            let half = halve(&mut c.mass, rng);
            c.birth_mass = c.total().max(1);
            c.born = t;
            c.ready = false;
            let d = BodyCell { pos: c.pos, birth_mass: half.iter().sum::<i64>().max(1), mass: half, born: t, ready: false, expr: c.expr, light: c.light, open: 5, x: c.x };
            self.stats.divisions += 1;
            match spot {
                Some(q) => {
                    let id = o.id;
                    o.cells.push(BodyCell { pos: q, ..d });
                    self.occ.insert(q, id);
                }
                None => {
                    let home = self.tile_at(d.pos);
                    spores.push((k, genome, d, home));
                }
            }
        }
        for (k, genome, mut d, home) in spores {
            let info = self.info_now(&genome);
            let id = self.id();
            let (parent, generation, clade) = (self.organisms[k].id, self.organisms[k].generation + 1, self.organisms[k].clade);
            d.pos = [0, 0, 0];
            self.stats.spores += 1;
            self.organisms.push(Organism {
                id,
                parent: Some(parent),
                genome,
                info,
                generation,
                born: self.t_days,
                cells: vec![d],
                store: [0; N_LETTERS],
                adrift: self.rp.drift_days * (0.5 + rng.f64()),
                drift_tile: home,
                clade,
            });
        }
    }

    /// Waves pull on every cell, harder the higher it stands; rock-touching cells hold with the
    /// adhesion they express. When drag wins the body tears at a random height (or lets go of the rock).
    fn waves(&mut self, dt: f64, rng: &mut Rng) -> Vec<usize> {
        let mut torn = Vec::new();
        for k in 0..self.organisms.len() {
            let o = &self.organisms[k];
            if !o.settled() || o.cells.is_empty() {
                continue;
            }
            let e = self.wave_energy(self.tile_at(o.cells[0].pos));
            if e <= 0.0 {
                continue;
            }
            let drag: f64 = e * self.rp.drag * o.cells.iter().map(|c| 1.0 + c.pos[2] as f64 / 4.0).sum::<f64>();
            let grip: f64 = o.cells.iter().filter(|c| c.pos[2] == 0).map(|c| self.rp.base_grip + self.rp.grip * c.expr[4] as f64).sum();
            if drag <= grip || !rng.chance(1.0 - (-self.rp.break_rate * (drag / grip.max(1e-9) - 1.0) * dt).exp()) {
                continue;
            }
            self.stats.tears += 1;
            let h = o.height();
            if h == 0 {
                // the whole crust lets go and tumbles
                let tile = self.tile_at(o.cells[0].pos);
                for c in &self.organisms[k].cells {
                    self.occ.remove(&c.pos);
                }
                let o = &mut self.organisms[k];
                o.adrift = self.rp.drift_days * (0.5 + rng.f64());
                o.drift_tile = tile;
                self.stats.fragments += 1;
                continue;
            }
            // cells at or above the tear break off (`split_pieces` sets them adrift)
            let cut = 1 + rng.below(h as usize) as i32;
            self.tear(k, |c| c.pos[2] >= cut, rng);
            torn.push(k);
        }
        torn
    }

    /// Move the cells matching `off` out of body `k` into a new adrift organism (same genome),
    /// with a share of the store by cell count.
    fn tear(&mut self, k: usize, off: impl Fn(&BodyCell) -> bool, rng: &mut Rng) {
        let o = &mut self.organisms[k];
        let (gone, kept): (Vec<BodyCell>, Vec<BodyCell>) = std::mem::take(&mut o.cells).into_iter().partition(|c| off(c));
        o.cells = kept;
        if gone.is_empty() {
            return;
        }
        for c in &gone {
            self.occ.remove(&c.pos);
        }
        let tile = self.tile_at(gone[0].pos);
        let o = &mut self.organisms[k];
        let share = gone.len() as f64 / (gone.len() + o.cells.len()) as f64;
        let mut store = [0i64; N_LETTERS];
        for l in 0..N_LETTERS {
            store[l] = round_flux(o.store[l] as f64 * share, rng).clamp(0, o.store[l]);
            o.store[l] -= store[l];
        }
        let piece = Organism {
            id: 0,
            parent: Some(o.id),
            genome: o.genome.clone(),
            info: o.info.clone(),
            generation: o.generation,
            born: self.t_days,
            cells: gone,
            store,
            adrift: self.rp.drift_days * (0.5 + rng.f64()),
            drift_tile: tile,
            clade: o.clade,
        };
        let id = self.id();
        self.stats.fragments += 1;
        // a torn-off piece may itself be in several pieces: each floats on its own
        let parts = pieces(&piece.cells);
        if parts.len() == 1 {
            self.organisms.push(Organism { id, ..piece });
            return;
        }
        let n = piece.cells.len() as f64;
        let mut left = piece.store;
        for (j, part) in parts.iter().enumerate() {
            let mut store = [0i64; N_LETTERS];
            for l in 0..N_LETTERS {
                store[l] = if j + 1 == parts.len() { left[l] } else { round_flux(piece.store[l] as f64 * part.len() as f64 / n, rng).clamp(0, left[l]) };
                left[l] -= store[l];
            }
            let cells = part.iter().map(|&i| piece.cells[i].clone()).collect();
            let id = if j == 0 { id } else { self.id() };
            self.organisms.push(Organism { id, cells, store, info: piece.info.clone(), genome: piece.genome.clone(), ..piece.clone() });
        }
    }

    /// After cells are lost, a settled body may fall apart: the largest rock-touching piece stays,
    /// other rock-touching pieces become bodies of their own (same genome), loose pieces drift.
    fn split_pieces(&mut self, k: usize, rng: &mut Rng) {
        if k >= self.organisms.len() || !self.organisms[k].settled() || self.organisms[k].cells.is_empty() {
            return;
        }
        let parts = pieces(&self.organisms[k].cells);
        let cells = &self.organisms[k].cells;
        if parts.len() == 1 && cells.iter().any(|c| c.pos[2] == 0) {
            return;
        }
        let grounded: Vec<bool> = parts.iter().map(|p| p.iter().any(|&i| cells[i].pos[2] == 0)).collect();
        let keep = (0..parts.len()).filter(|&j| grounded[j]).max_by_key(|&j| parts[j].len());
        // positions, not indices: each tear reorders the body
        let sets: Vec<HashSet<[i32; 3]>> = parts.iter().map(|p| p.iter().map(|&i| cells[i].pos).collect()).collect();
        // with nothing on the rock, piece 0 stays in this body and floats off below
        let stay = keep.unwrap_or(0);
        for (j, set) in sets.iter().enumerate() {
            if j == stay {
                continue;
            }
            let before = self.organisms.len();
            self.tear(k, |c| set.contains(&c.pos), rng);
            if grounded[j] {
                // still on the rock: a new body in place, not a drifter
                for o in self.organisms[before..].iter_mut() {
                    o.adrift = 0.0;
                    o.drift_tile = usize::MAX;
                    for c in &o.cells {
                        self.occ.insert(c.pos, o.id);
                    }
                }
                self.stats.fragments -= 1;
            }
        }
        if keep.is_none() {
            // nothing touches the rock: the rest floats away too
            let tile = self.tile_at(self.organisms[k].cells[0].pos);
            for c in &self.organisms[k].cells {
                self.occ.remove(&c.pos);
            }
            let o = &mut self.organisms[k];
            o.adrift = self.rp.drift_days * (0.5 + rng.f64());
            o.drift_tile = tile;
            self.stats.fragments += 1;
        }
    }

    /// Bodies with sticky skin bite cells off smaller bodies on their tile. A bitten cell's
    /// matter joins the grazer's store as monomers. Returns the bitten bodies.
    fn graze(&mut self, dt: f64, rng: &mut Rng) -> Vec<usize> {
        // ordered: tiles draw from the shared dice in turn, and hash order would change the run
        let mut on_tile: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, o) in self.organisms.iter().enumerate() {
            if o.settled() && !o.cells.is_empty() {
                on_tile.entry(self.tile_at(o.cells[0].pos)).or_default().push(k);
            }
        }
        let mut bitten = Vec::new();
        for (_, ks) in on_tile {
            if ks.len() < 2 {
                continue;
            }
            for &k in &ks {
                let o = &self.organisms[k];
                let skin: Vec<f64> = o.cells.iter().filter(|c| c.open > 0).map(|c| c.expr[4] as f64).collect();
                if skin.is_empty() {
                    continue;
                }
                let adh = skin.iter().sum::<f64>() / skin.len() as f64;
                let f = ((adh - self.rp.graze_floor) / self.rp.graze_span).clamp(0.0, 1.0);
                let bites = rng.poisson(self.rp.graze_rate * f * dt);
                for _ in 0..bites {
                    let j = ks[rng.below(ks.len())];
                    let (big, small) = (self.organisms[k].cells.len() as f64, self.organisms[j].cells.len() as f64);
                    if j == k || small < self.rp.graze_min.max(1) as f64 || small >= big * self.rp.graze_size {
                        continue;
                    }
                    // a bite takes a skin cell
                    let prey = &self.organisms[j];
                    let skin: Vec<usize> = (0..prey.cells.len()).filter(|&i| prey.cells[i].open > 0).collect();
                    let i = if skin.is_empty() { rng.below(prey.cells.len()) } else { skin[rng.below(skin.len())] };
                    let g = counts(&prey.genome);
                    let c = self.organisms[j].cells.swap_remove(i);
                    self.occ.remove(&c.pos);
                    let id = self.organisms[k].id;
                    for l in 0..N_LETTERS {
                        self.organisms[k].store[l] += c.mass[l] + g[l];
                    }
                    if self.organisms[j].cells.is_empty() {
                        // eaten whole: its store goes down with the last bite
                        let store = std::mem::take(&mut self.organisms[j].store);
                        for l in 0..N_LETTERS {
                            self.organisms[k].store[l] += store[l];
                        }
                    }
                    self.stats.bites += 1;
                    self.meals.push_back((self.t_days, id, c.pos));
                    if self.meals.len() > 200 {
                        self.meals.pop_front();
                    }
                    bitten.push(j);
                }
            }
        }
        bitten
    }

    /// Drifters ride the water to neighbouring tiles, die at a steady rate, and settle when
    /// their time is up on a flooded tile with room (shape kept, base set on the rock).
    fn drift(&mut self, dt: f64, rng: &mut Rng) {
        let mut lost = Vec::new();
        for k in 0..self.organisms.len() {
            if self.organisms[k].settled() || self.organisms[k].cells.is_empty() {
                continue;
            }
            let mut t = self.organisms[k].drift_tile;
            if rng.chance((2.0 * dt).min(1.0)) {
                let (tx, ty) = ((t % self.rp.width) as i32, (t / self.rp.width) as i32);
                let d = DIRS[rng.below(4)];
                if let Some(n) = self.tile_index(tx + d[0], ty + d[1]) {
                    if self.tiles[n].elevation < self.env.tide_level {
                        t = n;
                    }
                }
            }
            self.organisms[k].drift_tile = t;
            self.organisms[k].adrift -= dt;
            if rng.chance((self.rp.drift_death * dt).min(1.0)) {
                lost.push(k);
                continue;
            }
            if self.organisms[k].adrift > 0.0 || self.tiles[t].elevation >= self.env.tide_level {
                continue;
            }
            let Some(spot) = self.free_spot(t, rng) else { continue };
            let o = &self.organisms[k];
            let p0 = o.cells[0].pos;
            let z0 = o.cells.iter().map(|c| c.pos[2]).min().unwrap();
            let moved: Vec<[i32; 3]> = o.cells.iter().map(|c| [spot[0] + c.pos[0] - p0[0], spot[1] + c.pos[1] - p0[1], c.pos[2] - z0]).collect();
            if moved.iter().any(|q| !self.in_bounds(*q) || self.occ.contains_key(q)) {
                continue;
            }
            let o = &mut self.organisms[k];
            for (c, q) in o.cells.iter_mut().zip(&moved) {
                c.pos = *q;
                self.occ.insert(*q, o.id);
            }
            o.adrift = 0.0;
            o.drift_tile = usize::MAX;
            self.stats.settled += 1;
        }
        for &k in lost.iter().rev() {
            let o = self.organisms.swap_remove(k);
            let g = counts(&o.genome);
            let p = &mut self.tiles[o.drift_tile];
            for l in 0..N_LETTERS {
                p.free[l] += o.store[l] + o.cells.iter().map(|c| c.mass[l] + g[l]).sum::<i64>();
            }
            self.stats.lost_adrift += 1;
            self.stats.deaths += o.cells.len() as u64;
        }
    }

    /// Past the cell cap, random cells die (a safety net; food and room limit growth first).
    fn crowd(&mut self, rng: &mut Rng) {
        // only settled cells can be culled: counting drifters too could never get under the cap
        let mut n: usize = self.organisms.iter().filter(|o| o.settled()).map(|o| o.cells.len()).sum();
        while n > self.rp.max_cells {
            let k = rng.below(self.organisms.len());
            if self.organisms[k].cells.is_empty() || !self.organisms[k].settled() {
                continue;
            }
            let i = rng.below(self.organisms[k].cells.len());
            self.kill(k, &[i]);
            self.split_pieces(k, rng);
            // a split can set pieces adrift too
            n = self.organisms.iter().filter(|o| o.settled()).map(|o| o.cells.len()).sum();
        }
    }

    pub fn cells(&self) -> usize {
        self.organisms.iter().map(|o| o.cells.len()).sum()
    }

    /// Per-letter matter and Ka over tiles, ocean and bodies.
    pub fn matter(&self) -> ([i64; N_LETTERS], i64) {
        let mut m = self.ocean;
        let mut ka = self.ocean_ka;
        for p in &self.tiles {
            for l in 0..N_LETTERS {
                m[l] += p.raw[l] + p.free[l];
                ka += p.free[l];
            }
            ka += p.raw_ka;
        }
        for o in &self.organisms {
            let g = counts(&o.genome);
            for l in 0..N_LETTERS {
                let n = o.store[l] + o.cells.iter().map(|c| c.mass[l] + g[l]).sum::<i64>();
                m[l] += n;
                ka += n;
            }
        }
        (m, ka)
    }
}

/// One settled body's step: each cell reads its light, food and position, switches its genes,
/// grows from the body's store and is due to divide or burst at the coast's learned rates.
/// Returns the burst cells, how many cells sat where the table is thin, and one such cell's features.
fn live(o: &mut Organism, sh: &Shared, dt: f64, rng: &mut Rng) -> (Vec<usize>, u64, Option<[f64; 8]>) {
    let n = o.cells.len();
    let width = sh.rp.width as i32;
    let depth = sh.rp.depth as i32;
    let tile_of = |p: [i32; 3]| {
        let tx = p[0].div_euclid(TILE).clamp(0, width - 1);
        let ty = p[1].div_euclid(TILE).clamp(0, depth - 1);
        ty as usize * width as usize + tx as usize
    };
    let home = &sh.tiles[tile_of(o.cells[0].pos)];
    let li = (home.raw[LI] + home.free[LI]) as f64 / sh.rp.tile_cells;
    let mates = (n - 1) as f64 / sh.rp.mates_full.max(1.0);
    let mut dead = Vec::new();
    let mut unsure = 0;
    let mut example = None;
    for (i, c) in o.cells.iter_mut().enumerate() {
        let p = c.pos;
        c.open = DIRS.iter().filter(|d| p[2] + d[2] >= 0 && !sh.occ.contains_key(&[p[0] + d[0], p[1] + d[1], p[2] + d[2]])).count() as u8;
        let above = sh.trans.get(&p).copied().unwrap_or(1.0);
        let elev = sh.tiles[tile_of(p)].elevation + p[2] as f64 * CELL_RISE;
        let site = Site::at(sh.env, elev, 1.0 - above);
        let light = site.x[2];
        let sig = signals(light, sh.env.kt, &o.store, mates, p[2] as f64 / BODY_HEIGHT, c.open as f64 / 6.0);
        let traits = regulated_traits(&o.info, sig);
        let levels: Vec<f64> = o.info.genes.iter().map(|g| g.promoter.level(sig)).collect();
        let total: f64 = levels.iter().sum();
        let pig: f64 = o.info.genes.iter().zip(&levels).filter(|(g, _)| g.func.pigment > 0.0).map(|(_, l)| l).sum();
        let shade = if total > 0.0 { sh.rp.max_shade * o.info.pigment * pig / total } else { 0.0 };
        let adh = o.info.genes.iter().zip(&levels).map(|(g, l)| g.func.adhesion * l).fold(0.0, f64::max);
        c.expr = [traits.0 as f32, traits.1 as f32, traits.2 as f32, shade as f32, adh as f32];
        c.light = light as f32;
        // growth integrates the day's light as it reaches this cell through water and canopy
        let depth_w = (sh.env.tide_level - elev).max(0.0);
        let daylight = sh.daylight * above * (-1.5 * depth_w).exp();
        c.x = lineage_x(sh.env.kt, site.wet, li, traits, sh.t - c.born, daylight);
        if sh.model.confidence(&c.x) < 0.2 {
            unsure += 1;
            example.get_or_insert(c.x);
        }
        let (div, burst) = sh.model.fates(&c.x, sh.prior);
        if c.total() < 2 * c.birth_mass {
            let k = round_flux(c.birth_mass as f64 * div * dt, rng).max(0);
            for _ in 0..k {
                let Some(l) = pick_weighted(&c.mass, rng) else { break };
                if o.store[l] == 0 {
                    break;
                }
                o.store[l] -= 1;
                c.mass[l] += 1;
            }
        }
        let uv = Site::at(sh.env, elev, 1.0 - above * (1.0 - shade)).x[3];
        if rng.chance(((burst + sh.rp.turnover + sh.rp.uv_death * uv) * dt).min(1.0)) {
            dead.push(i);
            continue;
        }
        if !c.ready && rng.chance((div * dt).min(1.0)) {
            c.ready = true;
        }
    }
    (dead, unsure, example)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (RateTable, ReefSeed) {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/");
        let rates = serde_json::from_str(&std::fs::read_to_string(format!("{dir}rates0.json")).unwrap()).unwrap();
        let seed = serde_json::from_str(&std::fs::read_to_string(format!("{dir}reef_seed.json")).unwrap()).unwrap();
        (rates, seed)
    }

    #[test]
    fn reef_matter_closed_and_bodies_grow() {
        let (rates, seed) = load();
        let rp = ReefParams { width: 10, depth: 10, founders: 60, ..Default::default() };
        let mut r = Reef::new(rp, CosmosParams::default(), rates, &seed, 2);
        let start = r.matter();
        // founders are paid from the ocean: the reef starts with exactly what tiles and ocean were given
        let rp = &r.rp;
        let per_letter = (rp.ocean_raw * rp.tile_cells) as i64 * (rp.width * rp.depth) as i64 + (rp.ocean_raw * rp.tile_cells * rp.ocean_tiles) as i64;
        assert!(start.0.iter().all(|&m| m == per_letter), "founders created or lost matter");
        for _ in 0..4000 {
            r.step(0.02);
        }
        assert_eq!(r.matter(), start);
        let biggest = r.organisms.iter().map(|o| o.cells.len()).max().unwrap_or(0);
        assert!(biggest > 1, "no body grew past one cell: test would pass vacuously");
        assert!(r.stats.divisions > 0 && r.stats.spores > 0, "no divisions or spores");
    }
}

impl Reef {
    /// Living lineages: (clade, cells), biggest first.
    pub fn clades(&self) -> Vec<(u32, usize)> {
        let mut by: BTreeMap<u32, usize> = BTreeMap::new();
        for o in &self.organisms {
            *by.entry(o.clade).or_default() += o.cells.len();
        }
        let mut v: Vec<(u32, usize)> = by.into_iter().filter(|c| c.1 > 0).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    /// Division of labour: (share of genes, over cells of multicellular bodies, switched by height
    /// or exposure; within-body variance of expressed adhesion and pigment shade over their
    /// variance across all those cells: 0 = every body uniform, 1 = bodies as mixed as the reef).
    pub fn labour(&self) -> (f64, f64) {
        let (mut body_genes, mut genes) = (0.0, 0.0);
        let mut all: Vec<[f64; 2]> = Vec::new();
        let mut within = 0.0;
        for o in self.organisms.iter().filter(|o| o.settled() && o.cells.len() > 1) {
            let n = o.cells.len() as f64;
            for g in &o.info.genes {
                genes += n;
                if matches!(g.promoter.signal, crate::cell::Signal::Height | crate::cell::Signal::Exposure) {
                    body_genes += n;
                }
            }
            let v: Vec<[f64; 2]> = o.cells.iter().map(|c| [c.expr[4] as f64, c.expr[3] as f64]).collect();
            for k in 0..2 {
                let m = v.iter().map(|x| x[k]).sum::<f64>() / n;
                within += v.iter().map(|x| (x[k] - m).powi(2)).sum::<f64>();
            }
            all.extend(v);
        }
        let n = all.len() as f64;
        let mut total = 0.0;
        for k in 0..2 {
            let m = all.iter().map(|x| x[k]).sum::<f64>() / n.max(1.0);
            total += all.iter().map(|x| (x[k] - m).powi(2)).sum::<f64>();
        }
        (body_genes / genes.max(1.0), if total > 0.0 { within / total } else { 0.0 })
    }
}
