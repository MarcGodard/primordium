//! Scale-5 bridge: the reef in 3D. Godot Y is up; lattice x runs along the shore (X) and lattice
//! y up the beach (-Z, away from the sea-side camera). One lattice cell = one world unit, and the
//! rock's elevation is scaled so a body cell rises exactly as far as `CELL_RISE` says it does.
use crate::cell_sim::role;
use godot::prelude::*;
use primordium_core::cell::Signal;
use primordium_core::cosmos::CosmosParams;
use primordium_core::goal::{CreatureGoal, Tracker, jobs};
use primordium_core::polymer::{N_LETTERS, seq_string};
use primordium_core::reef::{CELL_RISE, Organism, Reef, ReefParams, ReefSeed, TILE};
use primordium_core::surrogate::RateTable;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// World units per unit of elevation (tide units).
const VSCALE: f64 = 1.0 / CELL_RISE;

/// One row of `history` a day: see the field's doc.
fn push_history(r: &Reef, history: &mut VecDeque<[f32; 11]>) {
    let last = history.back().map_or(-1.0, |h| h[0] as f64);
    if r.t_days - last < 1.0 {
        return;
    }
    let settled: Vec<&Organism> = r.organisms.iter().filter(|o| o.settled()).collect();
    let multi: Vec<&&Organism> = settled.iter().filter(|o| o.cells.len() > 1).collect();
    let mut row = [0f32; 11];
    row[10] = r.clades().len() as f32;
    row[0] = r.t_days as f32;
    row[1] = r.cells() as f32;
    row[2] = settled.len() as f32;
    row[3] = multi.iter().map(|o| o.cells.len()).sum::<usize>() as f32 / multi.len().max(1) as f32;
    row[4] = settled.iter().map(|o| o.height()).max().unwrap_or(0) as f32;
    let n = settled.iter().map(|o| o.cells.len()).sum::<usize>().max(1) as f32;
    for o in &settled {
        for c in &o.cells {
            for k in 0..5 {
                row[5 + k] += c.expr[k] / n;
            }
        }
    }
    history.push_back(row);
    if history.len() > 2000 {
        history.pop_front();
    }
}

/// Goal tracker sample once a day at noon (light-switched genes are off at night). Returns
/// creatures counted, or None when it was not yet noon.
fn noon_sample(r: &Reef, goal: &CreatureGoal, tracker: &mut Tracker, next_noon: &mut f64) -> Option<usize> {
    if r.t_days < *next_noon {
        return None;
    }
    *next_noon = r.t_days.floor() + 1.5;
    tracker.sample(goal, r);
    Some(r.organisms.iter().filter(|o| goal.body_ok(o)).count())
}

/// What a reef looks like at one moment, for the fast-forward report.
#[derive(Clone, Default)]
struct Snapshot {
    t: f64,
    cells: usize,
    lineages: Vec<(u32, usize)>,
    biggest: usize,
    max_gen: u32,
    genomes: usize,
    traits: [f64; 5],
}

impl Snapshot {
    fn of(r: &Reef) -> Self {
        let mut traits = [0.0; 5];
        let mut n = 0.0;
        for c in r.organisms.iter().filter(|o| o.settled()).flat_map(|o| o.cells.iter()) {
            for k in 0..5 {
                traits[k] += c.expr[k] as f64;
            }
            n += 1.0;
        }
        let genomes: std::collections::HashSet<&Vec<u8>> = r.organisms.iter().map(|o| &o.genome).collect();
        Self {
            t: r.t_days,
            cells: r.cells(),
            lineages: r.clades(),
            biggest: r.organisms.iter().filter(|o| o.settled()).map(|o| o.cells.len()).max().unwrap_or(0),
            max_gen: r.organisms.iter().map(|o| o.generation).max().unwrap_or(0),
            genomes: genomes.len(),
            traits: traits.map(|v| v / f64::max(n, 1.0)),
        }
    }
}

/// The reef and everything sampled from it, moved to a worker thread while fast-forwarding.
struct Run {
    reef: Reef,
    history: VecDeque<[f32; 11]>,
    tracker: Tracker,
    next_noon: f64,
}

/// What the worker hands back: the run and a report of what changed.
struct FfDone {
    run: Run,
    before: Snapshot,
    after: Snapshot,
    peak_creatures: usize,
    creature_days: usize,
}

struct FastForward {
    handle: std::thread::JoinHandle<FfDone>,
    /// Days done, x1000 (the worker's progress for the bar).
    done: Arc<AtomicU64>,
    days: f64,
}

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct ReefSim {
    reef: Option<Reef>,
    /// Matter at start, per letter and Ka: the HUD shows any drift from it.
    start: ([i64; N_LETTERS], i64),
    /// Sampled each day: t, cells, bodies, mean body size of multicellular bodies, tallest, then
    /// mean expressed polymerase, metabolism, membrane, shade, adhesion, then living lineages.
    history: VecDeque<[f32; 11]>,
    goal: CreatureGoal,
    tracker: Tracker,
    /// Next noon the tracker samples (light-switched genes are off at night).
    next_noon: f64,
    /// Running fast-forward: the reef is on the worker thread (`reef` is None meanwhile).
    ff: Option<FastForward>,
    base: Base<Node>,
}

impl ReefSim {
    /// Rock height in world units at lattice point (x, y): elevations interpolated between tile
    /// centres, so the terrain mesh and the cells standing on it agree.
    fn surface(r: &Reef, x: f64, y: f64) -> f64 {
        let (w, d) = (r.rp.width as i32, r.rp.depth as i32);
        let fx = x / TILE as f64 - 0.5;
        let fy = y / TILE as f64 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let e = |i: i32, j: i32| r.tiles[(j.clamp(0, d - 1) * w + i.clamp(0, w - 1)) as usize].elevation;
        let (i, j) = (x0 as i32, y0 as i32);
        let a = e(i, j) + (e(i + 1, j) - e(i, j)) * tx;
        let b = e(i, j + 1) + (e(i + 1, j + 1) - e(i, j + 1)) * tx;
        (a + (b - a) * ty) * VSCALE
    }

    fn world_pos(r: &Reef, p: [i32; 3]) -> Vector3 {
        let (x, y) = (p[0] as f64 + 0.5, p[1] as f64 + 0.5);
        Vector3::new(x as f32, (Self::surface(r, x, y) + p[2] as f64 + 0.5) as f32, -y as f32)
    }

    fn sample(&mut self) {
        let Some(r) = self.reef.as_ref() else { return };
        push_history(r, &mut self.history);
    }

    fn sample_goal(&mut self) {
        let Some(r) = self.reef.as_ref() else { return };
        noon_sample(r, &self.goal, &mut self.tracker, &mut self.next_noon);
    }

    fn begin(&mut self, rates_path: GString, seed_path: GString, seed: i64, founders: Option<usize>) -> bool {
        // a fast-forward would hand its old reef back over the new one
        if self.ff.is_some() {
            return false;
        }
        let Ok(text) = std::fs::read_to_string(rates_path.to_string()) else { return false };
        let Ok(rates) = serde_json::from_str::<RateTable>(&text) else { return false };
        let Ok(text) = std::fs::read_to_string(seed_path.to_string()) else { return false };
        let Ok(rs) = serde_json::from_str::<ReefSeed>(&text) else { return false };
        let d = ReefParams::default();
        let rp = ReefParams { founders: founders.unwrap_or(d.founders), ..d };
        let r = Reef::new(rp, CosmosParams { seed: seed as u64, ..Default::default() }, rates, &rs, seed as u64);
        self.start = r.matter();
        self.reef = Some(r);
        self.history.clear();
        self.tracker = Tracker::default();
        self.next_noon = 0.5;
        true
    }
}

#[godot_api]
impl ReefSim {
    #[func]
    fn start(&mut self, rates_path: GString, seed_path: GString, seed: i64) -> bool {
        self.begin(rates_path, seed_path, seed, None)
    }

    /// An empty reef (no founders) for the player's own creatures: see `release`.
    #[func]
    fn start_garden(&mut self, rates_path: GString, seed_path: GString, seed: i64) -> bool {
        self.begin(rates_path, seed_path, seed, Some(0))
    }

    /// Release a kept creature (`genome` and one cell's `mass` as `organism` gave them) as `n`
    /// spores on the lower shore. Returns its new lineage number, or -1 when refused.
    #[func]
    fn release(&mut self, genome: PackedByteArray, mass: PackedInt64Array, n: i64) -> i64 {
        let Some(r) = self.reef.as_mut() else { return -1 };
        let mut m = [0i64; N_LETTERS];
        for (k, v) in mass.as_slice().iter().take(N_LETTERS).enumerate() {
            m[k] = *v;
        }
        // the ocean pays, so the ledger's baseline holds
        r.release(genome.as_slice(), m, (0.0, 0.5), n.max(1) as usize).map_or(-1, |(c, _)| c as i64)
    }

    /// The creature goal and how close the reef is: lineages qualifying now, the lineage with
    /// the most qualifying days in the window and that count, and the lineage and day that first
    /// bred true (-1 before). `near_*`: the best crawler by size then jobs, for progress.
    #[func]
    fn goal(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let g = &self.goal;
        d.set("min_cells", g.min_cells as i64);
        d.set("min_jobs", g.min_jobs as i64);
        d.set("window_days", g.window_days);
        d.set("need_days", g.need_days as i64);
        let Some(r) = self.reef.as_ref() else { return d };
        d.set("qualifying", r.organisms.iter().filter(|o| g.body_ok(o)).count() as i64);
        let (bl, bd) = self.tracker.best().map_or((-1, 0), |(c, n)| (c as i64, n as i64));
        d.set("best_lineage", bl);
        d.set("best_days", bd);
        let (wc, wd) = self.tracker.won.map_or((-1, -1.0), |(c, t)| (c as i64, t));
        d.set("won_lineage", wc);
        d.set("won_day", wd);
        let score = |o: &&Organism| (o.cells.len().min(g.min_cells) * 10 + jobs(o, g.split).min(g.min_jobs) * 1000, o.cells.len());
        if let Some(o) = r.organisms.iter().filter(|o| o.settled() && (!g.mobile || o.speed > 0.0)).max_by_key(score) {
            d.set("near_cells", o.cells.len() as i64);
            d.set("near_jobs", jobs(o, g.split) as i64);
        }
        d
    }

    /// Run `days` on a worker thread at `dt`; the reef is away (every query returns empty, `step`
    /// does nothing) until `poll_fast_forward` hands it back. False when busy or not started.
    #[func]
    fn fast_forward(&mut self, days: f64, dt: f64) -> bool {
        if self.ff.is_some() || days <= 0.0 || dt <= 0.0 {
            return false;
        }
        let Some(reef) = self.reef.take() else { return false };
        let mut run = Run { reef, history: std::mem::take(&mut self.history), tracker: std::mem::take(&mut self.tracker), next_noon: self.next_noon };
        let goal = self.goal.clone();
        let done = Arc::new(AtomicU64::new(0));
        let progress = done.clone();
        let handle = std::thread::spawn(move || {
            let before = Snapshot::of(&run.reef);
            let end = run.reef.t_days + days;
            let (mut peak, mut cdays) = (0, 0);
            while run.reef.t_days < end {
                run.reef.step(dt);
                push_history(&run.reef, &mut run.history);
                if let Some(n) = noon_sample(&run.reef, &goal, &mut run.tracker, &mut run.next_noon) {
                    peak = peak.max(n);
                    cdays += (n > 0) as usize;
                }
                progress.store(((days - (end - run.reef.t_days)) * 1000.0) as u64, Ordering::Relaxed);
            }
            let after = Snapshot::of(&run.reef);
            FfDone { run, before, after, peak_creatures: peak, creature_days: cdays }
        });
        self.ff = Some(FastForward { handle, done, days });
        true
    }

    /// Fast-forward progress 0..1, or -1 when none is running.
    #[func]
    fn fast_forward_progress(&self) -> f64 {
        self.ff.as_ref().map_or(-1.0, |f| (f.done.load(Ordering::Relaxed) as f64 / 1000.0 / f.days).min(1.0))
    }

    /// Once the worker is done: take the reef back and report what changed (empty while it runs).
    /// Report: days, cells/biggest/max_generation/genomes before and after, lineages before and
    /// after as [lineage, cells] pairs (biggest first), lost lineages, mean traits before and after
    /// (polymerase, metabolism, membrane, pigment, glue), the most creatures seen at one noon and
    /// on how many noons there were any.
    #[func]
    fn poll_fast_forward(&mut self) -> VarDictionary {
        let mut d = VarDictionary::new();
        if !self.ff.as_ref().is_some_and(|f| f.handle.is_finished()) {
            return d;
        }
        let f = self.ff.take().unwrap();
        let Ok(out) = f.handle.join() else {
            // the worker panicked: the reef is gone, say so rather than pretend
            d.set("failed", true);
            return d;
        };
        let (b, a) = (&out.before, &out.after);
        d.set("days", a.t - b.t);
        for (k, s) in [("before", b), ("after", a)] {
            let mut sd = VarDictionary::new();
            sd.set("t", s.t);
            sd.set("cells", s.cells as i64);
            sd.set("biggest", s.biggest as i64);
            sd.set("max_generation", s.max_gen as i64);
            sd.set("genomes", s.genomes as i64);
            let mut lin = PackedInt64Array::new();
            for (c, n) in &s.lineages {
                lin.push(*c as i64);
                lin.push(*n as i64);
            }
            sd.set("lineages", &lin);
            sd.set("traits", &PackedFloat32Array::from(s.traits.map(|v| v as f32).as_slice()));
            d.set(k, &sd);
        }
        let alive: std::collections::BTreeSet<u32> = a.lineages.iter().map(|l| l.0).collect();
        let lost: PackedInt64Array = b.lineages.iter().filter(|l| !alive.contains(&l.0)).map(|l| l.0 as i64).collect();
        d.set("lost", &lost);
        d.set("peak_creatures", out.peak_creatures as i64);
        d.set("creature_days", out.creature_days as i64);
        let run = out.run;
        self.reef = Some(run.reef);
        self.history = run.history;
        self.tracker = run.tracker;
        self.next_noon = run.next_noon;
        d
    }

    /// Environment levers for the player's world: a multiplier on one reef rate, against its
    /// default. "waves" drag, "grazers" bite rate, "food" uptake through open faces, "uv" UV death.
    /// Rates only, never amounts: matter stays closed. Clamped to 1/4..4. False when unknown or busy.
    #[func]
    fn set_lever(&mut self, name: GString, mult: f64) -> bool {
        let Some(r) = self.reef.as_mut() else { return false };
        let d = ReefParams::default();
        let m = mult.clamp(0.25, 4.0);
        match name.to_string().as_str() {
            "waves" => r.rp.drag = d.drag * m,
            "grazers" => r.rp.graze_rate = d.graze_rate * m,
            "food" => r.rp.uptake = d.uptake * m,
            "uv" => r.rp.uv_death = d.uv_death * m,
            _ => return false,
        }
        true
    }

    /// Current lever multipliers (see `set_lever`); empty while busy.
    #[func]
    fn levers(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(r) = self.reef.as_ref() else { return out };
        let d = ReefParams::default();
        out.set("waves", r.rp.drag / d.drag);
        out.set("grazers", r.rp.graze_rate / d.graze_rate);
        out.set("food", r.rp.uptake / d.uptake);
        out.set("uv", r.rp.uv_death / d.uv_death);
        out
    }

    #[func]
    fn busy(&self) -> bool {
        self.ff.is_some()
    }

    /// Bodies meeting the creature goal right now, as ids (markers and "next creature").
    #[func]
    fn creatures(&self) -> PackedInt64Array {
        let Some(r) = self.reef.as_ref() else { return PackedInt64Array::new() };
        r.organisms.iter().filter(|o| self.goal.body_ok(o)).map(|o| o.id as i64).collect()
    }

    /// One representative per living lineage, biggest lineage first: the garden saves these and
    /// releases them again on load. Each: lineage, cells, genome, mass of one cell.
    #[func]
    fn lineages(&self, max: i64) -> VarArray {
        let mut arr = VarArray::new();
        let Some(r) = self.reef.as_ref() else { return arr };
        for (clade, n) in r.clades().into_iter().take(max.max(0) as usize) {
            // a lineage alive only as drifting spores still counts
            let Some(o) = r.organisms.iter().filter(|o| o.clade == clade && !o.cells.is_empty()).max_by_key(|o| (o.settled(), o.cells.len())) else { continue };
            let mut d = VarDictionary::new();
            d.set("lineage", clade as i64);
            d.set("cells", n as i64);
            d.set("genome", &PackedByteArray::from(o.genome.as_slice()));
            d.set("mass", &PackedInt64Array::from(o.cells[0].mass.as_slice()));
            arr.push(&d.to_variant());
        }
        arr
    }

    #[func]
    fn step(&mut self, steps: i64, dt: f64) {
        for _ in 0..steps {
            let Some(r) = self.reef.as_mut() else { return };
            r.step(dt);
            self.sample();
            self.sample_goal();
        }
    }

    #[func]
    fn t_days(&self) -> f64 {
        self.reef.as_ref().map_or(0.0, |r| r.t_days)
    }

    /// (width, depth) in lattice units.
    #[func]
    fn extent(&self) -> Vector2 {
        self.reef.as_ref().map_or(Vector2::ZERO, |r| Vector2::new((r.rp.width as i32 * TILE) as f32, (r.rp.depth as i32 * TILE) as f32))
    }

    #[func]
    fn vscale(&self) -> f64 {
        VSCALE
    }

    /// Rock heights on a grid of (w+1) x (d+1) lattice corners, row by row up the beach.
    #[func]
    fn heights(&self) -> PackedFloat32Array {
        let Some(r) = self.reef.as_ref() else { return PackedFloat32Array::new() };
        let (w, d) = (r.rp.width as i32 * TILE, r.rp.depth as i32 * TILE);
        let mut v = Vec::with_capacity(((w + 1) * (d + 1)) as usize);
        for y in 0..=d {
            for x in 0..=w {
                v.push(Self::surface(r, x as f64, y as f64) as f32);
            }
        }
        PackedFloat32Array::from(v.as_slice())
    }

    /// Water surface height, world units.
    #[func]
    fn water_level(&self) -> f64 {
        self.reef.as_ref().map_or(0.0, |r| r.env.tide_level * VSCALE)
    }

    /// MultiMesh buffer (transform 12, colour 4, custom 4 per instance) for every cell. Colour =
    /// expressed polymerase, metabolism, membrane, shade; custom = adhesion, light reaching the cell,
    /// selected (1), adrift (0.5) or crawling (0.1-0.3 by speed), height in the body + 64 x lineage. Drifters float at the water surface.
    #[func]
    fn cell_buffer(&self, selected: i64) -> PackedFloat32Array {
        let Some(r) = self.reef.as_ref() else { return PackedFloat32Array::new() };
        let mut v = Vec::with_capacity(r.cells() * 20);
        let water = r.env.tide_level * VSCALE;
        for o in &r.organisms {
            // selected 1, crawling 0.1-0.3 by speed, else 0 (adrift is set below)
            let sel = if o.id as i64 == selected { 1.0 } else if o.speed > 0.0 { 0.1 + 0.1 * o.speed.min(2.0) } else { 0.0 };
            let (drift_at, adrift) = if o.settled() {
                (Vector3::ZERO, false)
            } else {
                let t = o.drift_tile;
                let (tx, ty) = ((t % r.rp.width) as f32, (t / r.rp.width) as f32);
                let bob = ((r.t_days * 7.0 + o.id as f64 * 0.37).sin() * 0.3) as f32;
                (Vector3::new((tx + 0.5) * TILE as f32, water as f32 + bob, -(ty + 0.5) * TILE as f32), true)
            };
            let p0 = o.cells.first().map_or([0, 0, 0], |c| c.pos);
            for c in &o.cells {
                let (pos, s) = if adrift {
                    let d = Vector3::new((c.pos[0] - p0[0]) as f32, (c.pos[2] - p0[2]) as f32, -(c.pos[1] - p0[1]) as f32);
                    (drift_at + d * 0.8, 0.38)
                } else {
                    // cells swell as they grow toward division
                    let f = (c.mass.iter().sum::<i64>() as f32 / c.birth_mass.max(1) as f32).clamp(1.0, 2.0);
                    // a lone cell is drawn small: the many single cells otherwise hide the bodies
                    let lone = if o.cells.len() == 1 { 0.6 } else { 1.0 };
                    (Self::world_pos(r, c.pos), (0.36 + 0.1 * f) * lone)
                };
                v.extend_from_slice(&[s, 0.0, 0.0, pos.x, 0.0, s, 0.0, pos.y, 0.0, 0.0, s, pos.z]);
                v.extend_from_slice(&[c.expr[0], c.expr[1], c.expr[2], c.expr[3]]);
                // height and lineage share a slot: z + 64 x clade. Exact in f32 (Forward+); the
                // Compatibility renderer stores custom data as half floats and would garble it
                v.extend_from_slice(&[c.expr[4], c.light, if adrift { 0.5 } else { sel }, c.pos[2] as f32 + 64.0 * o.clade as f32]);
            }
        }
        PackedFloat32Array::from(v.as_slice())
    }

    /// Nearest organism whose cell the ray passes within half a cell of; -1 for none.
    #[func]
    fn pick(&self, from: Vector3, dir: Vector3) -> i64 {
        let Some(r) = self.reef.as_ref() else { return -1 };
        let dir = dir.normalized();
        let mut best = (f32::MAX, -1i64);
        for o in r.organisms.iter().filter(|o| o.settled()) {
            for c in &o.cells {
                let p = Self::world_pos(r, c.pos) - from;
                let t = p.dot(dir);
                if t > 0.0 && t < best.0 && (p - dir * t).length() < 0.6 {
                    best = (t, o.id as i64);
                }
            }
        }
        best.1
    }

    /// The settled organism with the most cells; -1 for none.
    #[func]
    fn biggest(&self) -> i64 {
        let Some(r) = self.reef.as_ref() else { return -1 };
        r.organisms.iter().filter(|o| o.settled()).max_by_key(|o| o.cells.len()).map_or(-1, |o| o.id as i64)
    }

    /// Centre of an organism's body, world units.
    #[func]
    fn organism_center(&self, id: i64) -> Vector3 {
        let Some(r) = self.reef.as_ref() else { return Vector3::ZERO };
        let Some(o) = r.organisms.iter().find(|o| o.id as i64 == id && o.settled()) else { return Vector3::ZERO };
        let sum = o.cells.iter().fold(Vector3::ZERO, |a, c| a + Self::world_pos(r, c.pos));
        sum / o.cells.len().max(1) as f32
    }

    /// One organism: size, shape, lineage, genes with their switches, and its make-up by height
    /// (per layer: cells, then mean polymerase, metabolism, membrane, shade, adhesion).
    #[func]
    fn organism(&mut self, id: i64) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(r) = self.reef.as_ref() else { return d };
        let Some(o) = r.organisms.iter().find(|o| o.id as i64 == id) else { return d };
        d.set("id", o.id as i64);
        d.set("cells", o.cells.len() as i64);
        d.set("height", o.height() as i64 + 1);
        d.set("generation", o.generation as i64);
        d.set("lineage", o.clade as i64);
        d.set("age", r.t_days - o.born);
        d.set("adrift", !o.settled());
        d.set("speed", o.speed);
        d.set("genome_len", o.genome.len() as i64);
        d.set("genome_text", seq_string(&o.genome));
        d.set("genome", &PackedByteArray::from(o.genome.as_slice()));
        d.set("mass", &PackedInt64Array::from(o.cells.first().map_or(&[0; N_LETTERS], |c| &c.mass).as_slice()));
        d.set("jobs", jobs(o, self.goal.split) as i64);
        d.set("creature", self.goal.body_ok(o));
        d.set("store", o.store.iter().sum::<i64>());
        let tile = if o.settled() { r.tile_index(o.cells[0].pos[0] / TILE, o.cells[0].pos[1] / TILE).unwrap_or(0) } else { o.drift_tile };
        d.set("elevation", r.tiles[tile].elevation);
        d.set("depth", r.env.tide_level - r.tiles[tile].elevation);
        d.set("waves", r.wave_energy(tile));
        let mut layers = PackedFloat32Array::new();
        for z in 0..=o.height() {
            let cs: Vec<_> = o.cells.iter().filter(|c| c.pos[2] == z).collect();
            layers.push(cs.len() as f32);
            for k in 0..5 {
                layers.push(cs.iter().map(|c| c.expr[k]).sum::<f32>() / cs.len().max(1) as f32);
            }
        }
        d.set("layers", &layers);
        let mut genes = VarArray::new();
        for g in &o.info.genes {
            let mut gd = VarDictionary::new();
            let (k, strength) = role(&g.func);
            // adhesion has no slot in the scale-3 roles; show it where it dominates
            let k = if g.func.adhesion > 0.3 && g.func.adhesion > strength * 0.3 { 5 } else { k };
            gd.set("role", k);
            gd.set("strength", strength);
            gd.set("adhesion", g.func.adhesion);
            let p = g.promoter;
            let name = match p.signal {
                Signal::None => "",
                Signal::Light => "light",
                Signal::Cold => "cold",
                Signal::LiScarce => "Li short",
                Signal::Mates => "size",
                Signal::Height => "height",
                Signal::Exposure => "skin",
            };
            gd.set("promoter", if name.is_empty() { String::new() } else { format!("{name}{}", if p.on_high { "+" } else { "-" }) });
            genes.push(&gd.to_variant());
        }
        d.set("genes", &genes);
        d
    }

    /// Bites of the last `days`: position (world) and age in days, flattened as x, y, z, age.
    #[func]
    fn bites(&self, days: f64) -> PackedFloat32Array {
        let Some(r) = self.reef.as_ref() else { return PackedFloat32Array::new() };
        let mut v = Vec::new();
        for &(t, _, p) in r.meals.iter().rev() {
            if r.t_days - t > days {
                break;
            }
            let w = Self::world_pos(r, p);
            v.extend_from_slice(&[w.x, w.y, w.z, (r.t_days - t) as f32]);
        }
        PackedFloat32Array::from(v.as_slice())
    }

    /// Sky, tide and sun as felt on the reef (HUD and lighting).
    #[func]
    fn env(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(r) = self.reef.as_ref() else { return d };
        let e = &r.env;
        for (key, v) in [
            ("t_days", e.t_days),
            ("day_phase", e.day_phase),
            ("year_phase", e.year_phase),
            ("year", e.year),
            ("sun_height", e.sun_height),
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
        let cp = &r.cosmos.p;
        d.set("latitude", cp.latitude);
        d.set("declination", cp.obliquity * (std::f64::consts::TAU * e.year_phase).sin());
        d.set("moon0_phase", (e.t_days / cp.moon_periods[0]).fract());
        d.set("moon1_phase", (e.t_days / cp.moon_periods[1]).fract());
        let (m, ka) = r.matter();
        let drift: i64 = m.iter().zip(self.start.0).map(|(a, b)| (a - b).abs()).sum::<i64>() + (ka - self.start.1).abs();
        d.set("ledger_drift", drift);
        d
    }

    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(r) = self.reef.as_ref() else { return d };
        let settled: Vec<&Organism> = r.organisms.iter().filter(|o| o.settled()).collect();
        let st = &r.stats;
        d.set("cells", r.cells() as i64);
        d.set("bodies", settled.len() as i64);
        d.set("multicellular", settled.iter().filter(|o| o.cells.len() > 1).count() as i64);
        d.set("biggest", settled.iter().map(|o| o.cells.len()).max().unwrap_or(0) as i64);
        d.set("tallest", settled.iter().map(|o| o.height() + 1).max().unwrap_or(0) as i64);
        d.set("adrift", (r.organisms.len() - settled.len()) as i64);
        d.set("generation", r.organisms.iter().map(|o| o.generation).max().unwrap_or(0) as i64);
        d.set("spores", st.spores as i64);
        d.set("tears", st.tears as i64);
        d.set("bites", st.bites as i64);
        let crawl: Vec<&&Organism> = settled.iter().filter(|o| o.speed > 0.0).collect();
        d.set("crawlers", crawl.len() as i64);
        d.set("crawlers_multi", crawl.iter().filter(|o| o.cells.len() > 1).count() as i64);
        d.set("divisions", st.divisions as i64);
        d.set("bit_error", st.bit_errors as f64 / st.bits_inherited.max(1) as f64);
        d.set("unsure", st.unsure_cells as i64);
        let clades = r.clades();
        d.set("lineages", clades.len() as i64);
        d.set("founders", r.rp.founders as i64);
        let total = r.cells().max(1) as f64;
        let mut top = VarArray::new();
        for (c, n) in clades.iter().take(5) {
            top.push(&Vector2::new(*c as f32, (*n as f64 / total) as f32).to_variant());
        }
        d.set("top_lineages", &top);
        let mut arr = VarArray::new();
        for e in r.discoveries.entries.iter().rev().take(3) {
            arr.push(&format!("day {:.1}: {}", e.t_days, e.note).to_variant());
        }
        d.set("discoveries", &arr);
        d
    }

    /// Flattened daily history, 11 values per row (see `history` field).
    #[func]
    fn history(&self) -> PackedFloat32Array {
        let v: Vec<f32> = self.history.iter().flatten().copied().collect();
        PackedFloat32Array::from(v.as_slice())
    }
}
