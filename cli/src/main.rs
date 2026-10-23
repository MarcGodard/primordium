//! Headless runner. `primordium chem --days=4 --seed=1 --every=0.25`
use primordium_core::chem::{ChemParams, ChemWorld};
use primordium_core::cosmos::CosmosParams;
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    let key = format!("--{name}=");
    args.iter().find_map(|a| a.strip_prefix(&key).and_then(|v| v.parse().ok())).unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("chem");
    match mode {
        "chem" => chem(&args),
        "fold" => fold(&args),
        "train" => train(&args),
        "shadow" => shadow(&args),
        "pool" => pool(&args),
        "fold-shadow" => fold_shadow(&args),
        "pond" => pond(&args),
        "pond-probe" => pond_probe(&args),
        "selection" => selection(&args),
        "pond-shadow" => pond_shadow(&args),
        _ => eprintln!("usage: primordium chem [--days=N] [--seed=N] [--every=D]"),
    }
}

fn chem(args: &[String]) {
    let days: f64 = arg(args, "days", 2.0);
    let seed: u64 = arg(args, "seed", 1);
    let every: f64 = arg(args, "every", 0.25);
    let mut w = ChemWorld::new(ChemParams::default(), CosmosParams { seed, ..Default::default() }, seed);
    let per = (every * w.p.sweeps_per_day).round() as u64;
    let total = (days * w.p.sweeps_per_day).round() as u64;
    let start = Instant::now();
    println!("atoms={} grid={}x{}", w.atoms.len(), w.p.width, w.p.height);
    for s in 1..=total {
        w.sweep();
        if s % per == 0 {
            let c = w.census();
            let e = w.env;
            let k = w.take_counts();
            println!(
                "t={:6.2}d T={:5.1}K vis={:.2} uv={:.2} tide={:.2} | mol={} bonds={} big={} exc={} hist={:?} | form={} brk={} exch={} uvb={} | drift={}",
                w.t_days, e.temperature_k, e.visible, e.uv, e.tide_level, c.molecules, c.bonds, c.largest, c.excited, c.size_hist,
                k.forms, k.breaks, k.exchanges, k.uv_breaks, w.ledger.drift()
            );
            println!("    top: {:?}", &c.top_species[..c.top_species.len().min(8)]);
        }
    }
    let secs = start.elapsed().as_secs_f64();
    println!("{} sweeps in {:.2}s = {:.0} sweeps/s", total, secs, total as f64 / secs);
}

fn fold(args: &[String]) {
    use primordium_core::element::Table;
    use primordium_core::kinetics::{BONDABLE, pair_members};
    use primordium_core::polymer::{LetterPhysics, express, parse_seq};
    let kt: f64 = arg(args, "kt", 1.0);
    let ph = LetterPhysics::new(&Table::new());
    println!("pair kT: {:?}", ph.pair);
    println!("hydropathy: {:?}", ph.hydropathy);
    for s in args.iter().skip(2).filter(|a| !a.starts_with("--")) {
        let seq = parse_seq(s);
        let (f, func) = express(&seq, &ph, kt, 1);
        let (p, cut) = func.best_catalysis();
        let (a, b) = pair_members(p);
        let kk = primordium_core::kinetics::pair(3, 3);
        let kl = primordium_core::kinetics::pair(1, 3);
        println!("    Ka-Ka cut {:.3}  Li-Ka cut {:.3}", func.catalysis[kk], func.catalysis[kl]);
        println!(
            "{s}: E={:.2} stab={:.3} amph={:.2} pig={:.2} best cut {:.2} kT on {}-{}",
            f.energy, func.stability, func.amphiphile, func.pigment, cut,
            primordium_core::element::NAMES[BONDABLE[a] as usize - 1], primordium_core::element::NAMES[BONDABLE[b] as usize - 1]
        );
    }
}

/// Learn scale-0 rates over `days` of a detailed run; writes data/rates0.json.
fn train(args: &[String]) {
    use primordium_core::hybrid;
    let days: f64 = arg(args, "days", 96.0);
    let seed: u64 = arg(args, "seed", 100);
    let mut w = ChemWorld::new(ChemParams::default(), CosmosParams { seed, ..Default::default() }, seed);
    let start = Instant::now();
    let table = hybrid::train(&mut w, days, |f| eprint!("\rtraining {:.0}%", f * 100.0));
    eprintln!();
    println!("trained {days} days in {:.1}s, bin coverage {:.1}%", start.elapsed().as_secs_f64(), table.coverage() * 100.0);
    std::fs::create_dir_all("data").unwrap();
    std::fs::write("data/rates0.json", serde_json::to_string(&table).unwrap()).unwrap();
}

/// Shadow mode: detailed vs all-surrogate copies of one world, compared on band bond counts.
fn shadow(args: &[String]) {
    use primordium_core::hybrid::Mode;
    use primordium_core::surrogate::{RateTable, agreement};
    let days: f64 = arg(args, "days", 4.0);
    let start_day: f64 = arg(args, "start", 1.0);
    let seed: u64 = arg(args, "seed", 1);
    let table: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut a = ChemWorld::new(ChemParams::default(), CosmosParams { seed, ..Default::default() }, seed);
    while a.t_days < start_day {
        a.sweep();
    }
    let mut b = a.clone();
    // twin: same state, different dice. Detail-vs-detail agreement is the ceiling any surrogate can reach.
    let mut twin = a.clone();
    twin.reseed(seed ^ 0xD1CE);
    b.hybrid.rates = Some(table);
    for k in 0..b.hybrid.hp.bands {
        b.restrict(k);
    }
    let bands = a.hybrid.hp.bands;
    let snapshot = |w: &ChemWorld| -> Vec<f64> {
        (0..bands)
            .flat_map(|k| {
                let s = if w.hybrid.mode[k] == Mode::Surrogate { w.hybrid.state[k].clone() } else { w.band_state_from_atoms(k) };
                s.bonds.into_iter().map(|v| v as f64).collect::<Vec<_>>()
            })
            .collect()
    };
    let initial = snapshot(&a);
    let per = (0.5 * a.p.sweeps_per_day) as u64;
    let total = (days * a.p.sweeps_per_day) as u64;
    let (mut ta, mut tb) = (0.0, 0.0);
    let (mut sum_acc, mut sum_null, mut sum_twin, mut n) = (0.0, 0.0, 0.0, 0.0);
    for s in 1..=total {
        let t0 = Instant::now();
        a.sweep();
        ta += t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        b.sweep();
        tb += t0.elapsed().as_secs_f64();
        twin.sweep();
        if s % per == 0 {
            let (sa, sb) = (snapshot(&a), snapshot(&b));
            let acc = agreement(&sa, &sb);
            let null = agreement(&sa, &initial);
            let floor = agreement(&sa, &snapshot(&twin));
            sum_twin += floor;
            sum_acc += acc;
            sum_null += null;
            n += 1.0;
            let conf: Vec<String> = b.hybrid.confidence.iter().map(|c| format!("{c:.2}")).collect();
            let detail = b.hybrid.mode.iter().filter(|m| **m == Mode::Detail).count();
            println!(
                "t={:6.2} T={:5.1}K tide={:.2} | bonds detail={} surrogate={} | agreement {:.1}% (twin {:.1}%, frozen-null {:.1}%) | conf [{}] detail-bands={} discoveries={}",
                a.t_days, a.env.temperature_k, a.env.tide_level, sa.iter().sum::<f64>(), sb.iter().sum::<f64>(), acc * 100.0, floor * 100.0, null * 100.0,
                conf.join(" "), detail, b.hybrid.discoveries.entries.len()
            );
        }
    }
    println!(
        "mean agreement {:.1}% | detail twin {:.1}% | frozen-null {:.1}% | relative to twin ceiling {:.1}% | speed detail {:.0} sweeps/s, hybrid {:.0} sweeps/s | ledger drift a={} b={} | lift shortfall {}",
        100.0 * sum_acc / n, 100.0 * sum_twin / n, 100.0 * sum_null / n, 100.0 * sum_acc / sum_twin, total as f64 / ta, total as f64 / tb, a.ledger.drift(), b.ledger.drift(), b.hybrid.lift_shortfall
    );
}

/// Scale-1 pool on learned scale-0 rates.
fn pool(args: &[String]) {
    use primordium_core::cosmos::Cosmos;
    use primordium_core::pool::{Pool, PoolParams};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 30.0);
    let seed: u64 = arg(args, "seed", 1);
    let dt: f64 = arg(args, "dt", 0.01);
    let every: f64 = arg(args, "every", 2.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams { seed, ..Default::default() }), rates, seed);
    let start = Instant::now();
    let steps = (days / dt) as u64;
    let per = (every / dt) as u64;
    for s in 1..=steps {
        p.step(dt);
        if s % per == 0 {
            let n = p.c.chains.len();
            let mean_len = p.c.chains.iter().map(|c| c.seq.len()).sum::<usize>() as f64 / n.max(1) as f64;
            let max_len = p.c.chains.iter().map(|c| c.seq.len()).max().unwrap_or(0);
            let max_gen = p.c.chains.iter().map(|c| c.generation).max().unwrap_or(0);
            let st = &p.c.stats;
            println!(
                "t={:6.1} T={:5.1}K {} | chains={} len mean {:.1} max {} gen {} | copies={} err={:.2} breaks={} lig={} | cut {:.2} best {:.2} | folds={} exact={} nn={}",
                p.t_days, p.env.temperature_k, if p.wet() { "wet" } else { "dry" }, n, mean_len, max_len, max_gen, st.copies,
                st.mutations as f64 / st.copied_letters.max(1) as f64, st.breaks, st.ligations, p.c.backbone_cut(), p.ctx.best_cut, p.ctx.model.folds, p.ctx.model.exact, p.ctx.model.nearest
            );
            println!("    top: {:?}", p.c.top_sequences(5));
        }
    }
    for d in p.ctx.discoveries.entries.iter().rev().take(5) {
        println!("discovery day {:.1}: {}", d.t_days, d.note);
    }
    println!("{:.1}s", start.elapsed().as_secs_f64());
}

/// Scale-1 surrogate accuracy: does a k-mutant share its parent's function?
/// Ceiling = same sequence, different annealing dice. Null = population-mean function.
fn fold_shadow(args: &[String]) {
    use primordium_core::element::Table;
    use primordium_core::polymer::{LetterPhysics, N_LETTERS, express_ensemble};
    use primordium_core::rng::Rng;
    use primordium_core::surrogate::agreement;
    let n: usize = arg(args, "n", 150);
    let k: usize = arg(args, "k", 1);
    let sm: f64 = arg(args, "steps", 1.0);
    let express = |s: &[u8], ph: &LetterPhysics, kt: f64, seed: u64| express_ensemble(s, ph, kt, seed, k, sm);
    let kt: f64 = arg(args, "kt", 1.0);
    let ph = LetterPhysics::new(&Table::new());
    let mut rng = Rng::new(9);
    for len in [8usize, 16, 24, 32] {
        let seqs: Vec<Vec<u8>> = (0..n).map(|_| (0..len).map(|_| rng.below(N_LETTERS) as u8).collect()).collect();
        let funcs: Vec<Vec<f64>> = seqs.iter().map(|s| express(s, &ph, kt, 1).1.as_vec()).collect();
        let dim = funcs[0].len();
        let mean: Vec<f64> = (0..dim).map(|k| funcs.iter().map(|f| f[k]).sum::<f64>() / n as f64).collect();
        let avg = |v: &mut dyn Iterator<Item = f64>| { let (s, c) = v.fold((0.0, 0.0), |a, x| (a.0 + x, a.1 + 1.0)); 100.0 * s / c };
        let ceiling = avg(&mut seqs.iter().zip(&funcs).map(|(s, f)| agreement(f, &express(s, &ph, kt, 77).1.as_vec())));
        let null = avg(&mut funcs.iter().map(|f| agreement(f, &mean)));
        let mut row = format!("len {len:2}: ceiling {ceiling:5.1}%  null {null:5.1}%  |");
        for d in 1..=3 {
            let acc = avg(&mut seqs.iter().zip(&funcs).map(|(s, f)| {
                let mut m = s.clone();
                for _ in 0..d {
                    let k = rng.below(len);
                    m[k] = ((m[k] as usize + 1 + rng.below(N_LETTERS - 1)) % N_LETTERS) as u8;
                }
                agreement(&express(&m, &ph, kt, 1).1.as_vec(), f)
            }));
            row += &format!("  {d}-mut {acc:5.1}%");
        }
        println!("{row}");
    }
}

/// Scale-2 pond of protocells.
fn pond(args: &[String]) {
    use primordium_core::cosmos::Cosmos;
    use primordium_core::hybrid::Mode;
    use primordium_core::pool::PoolParams;
    use primordium_core::protocell::{Pond, PondParams};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 60.0);
    let seed: u64 = arg(args, "seed", 1);
    let dt: f64 = arg(args, "dt", 0.01);
    let every: f64 = arg(args, "every", 5.0);
    let auto: bool = arg(args, "auto", false);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut p = Pond::new(PoolParams::default(), PondParams { auto, ..Default::default() }, Cosmos::new(CosmosParams { seed, ..Default::default() }), rates, seed);
    let start = Instant::now();
    let steps = (days / dt) as u64;
    let per = (every / dt) as u64;
    for s in 1..=steps {
        p.step(dt);
        if s % per == 0 {
            let n = p.vesicles.len();
            let mean_area = p.vesicles.iter().map(|v| v.area()).sum::<i64>() as f64 / n.max(1) as f64;
            let sur = p.vesicles.iter().filter(|v| v.mode == Mode::Surrogate).count();
            let st = &p.stats;
            let gens: u32 = p.vesicles.iter().map(|v| v.generation).max().unwrap_or(0);
            println!(
                "t={:6.1} T={:5.1}K {} | vesicles={} (sur {}) area {:.0} gen {} | nuc={} div={} burst={} ins={} ads={} | cut in {:.2} out {:.2} | err in {:.2} out {:.2} | outside chains {} | inside chains {} folds {} exact {} nn {} | {:.1}s",
                p.t_days, p.env.temperature_k, if p.wet() { "wet" } else { "dry" }, n, sur, mean_area, gens, st.nucleations, st.divisions, st.bursts,
                st.insertions, st.adsorptions, p.mean_vesicle_cut(), p.outside.backbone_cut(),
                { let s = p.total_interior_stats(); s.mutations as f64 / s.copied_letters.max(1) as f64 },
                p.outside.stats.mutations as f64 / p.outside.stats.copied_letters.max(1) as f64,
                p.outside.chains.len(),
                p.vesicles.iter().map(|v| v.interior.chains.len()).sum::<usize>(), p.ctx.model.folds, p.ctx.model.exact, p.ctx.model.nearest,
                start.elapsed().as_secs_f64()
            );
        }
    }
    for d in p.ctx.discoveries.entries.iter().rev().take(4) {
        println!("discovery day {:.1} scale {}: {}", d.t_days, d.scale, d.note);
    }
}

fn pond_probe(args: &[String]) {
    use primordium_core::cosmos::Cosmos;
    use primordium_core::pool::PoolParams;
    use primordium_core::protocell::{Pond, PondParams};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 20.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut p = Pond::new(PoolParams::default(), PondParams::default(), Cosmos::new(CosmosParams::default()), rates, 1);
    for _ in 0..(days / 0.01) as u64 {
        p.step(0.01);
    }
    let o = &p.outside;
    println!("outside: cells {:.0} chains {} free {:?} raw {:?} raw_ka {} cut {:.2} synth {:.2}", o.cells, o.chains.len(), o.free, o.raw, o.raw_ka, o.backbone_cut(), o.synthesis_cut());
    let mut vs: Vec<_> = p.vesicles.iter().collect();
    vs.sort_by_key(|v| -v.area());
    for v in vs.iter().take(8) {
        let i = &v.interior;
        println!(
            "v{} gen {} area {} birth {} cells {:.0} chains {} amph {:.2} free {} raw {} ka {} cut {:.2} synth {:.2} copies {} synthesized {}",
            v.id, v.generation, v.area(), v.birth_area, i.cells, i.chains.len(), v.amph_frac(0.3), i.free.iter().sum::<i64>(), i.raw.iter().sum::<i64>(), i.raw_ka,
            i.backbone_cut(), i.synthesis_cut(), i.stats.copies, i.stats.synthesized
        );
    }
    let amph = o.chains.iter().filter(|c| c.func.amphiphile >= 0.3).count();
    println!("outside amphiphiles {amph} of {}; amphiphile score dist: {:?}", o.chains.len(),
        { let mut h = [0; 5]; for c in &o.chains { h[((c.func.amphiphile * 5.0) as usize).min(4)] += 1; } h });
}

/// Selection gradient at scale 2: does interior catalysis predict vesicle growth?
/// Snapshot each vesicle's traits, run `window` days, score lineage growth (descendant membrane).
fn selection(args: &[String]) {
    use primordium_core::cosmos::Cosmos;
    use primordium_core::pool::PoolParams;
    use primordium_core::protocell::{Pond, PondParams};
    use primordium_core::surrogate::RateTable;
    use std::collections::HashMap;
    let warm: f64 = arg(args, "warm", 30.0);
    let window: f64 = arg(args, "window", 4.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut p = Pond::new(PoolParams::default(), PondParams::default(), Cosmos::new(CosmosParams::default()), rates, 1);
    while p.t_days < warm {
        p.step(0.01);
    }
    let traits: HashMap<u64, (f64, f64, f64, i64)> = p.vesicles.iter().map(|v| (v.id, (v.interior.backbone_cut(), v.interior.chains.len() as f64, v.osmotic(), v.area()))).collect();
    // root of each vesicle = ancestor present at snapshot
    let mut root: HashMap<u64, u64> = traits.keys().map(|&k| (k, k)).collect();
    let end = p.t_days + window;
    while p.t_days < end {
        p.step(0.01);
        for v in &p.vesicles {
            if !root.contains_key(&v.id) {
                if let Some(r) = v.parent.and_then(|q| root.get(&q).copied()) {
                    root.insert(v.id, r);
                }
            }
        }
    }
    let mut growth: HashMap<u64, f64> = HashMap::new();
    for v in &p.vesicles {
        if let Some(&r) = root.get(&v.id) {
            *growth.entry(r).or_default() += v.area() as f64;
        }
    }
    let rows: Vec<(f64, f64, f64, f64)> = traits.iter().map(|(id, t)| (t.0, t.1, t.2, growth.get(id).copied().unwrap_or(0.0) / t.3 as f64)).collect();
    let corr = |f: &dyn Fn(&(f64, f64, f64, f64)) -> f64| {
        let n = rows.len() as f64;
        let (mx, my) = (rows.iter().map(f).sum::<f64>() / n, rows.iter().map(|r| r.3).sum::<f64>() / n);
        let cov: f64 = rows.iter().map(|r| (f(r) - mx) * (r.3 - my)).sum();
        let vx: f64 = rows.iter().map(|r| (f(r) - mx).powi(2)).sum();
        let vy: f64 = rows.iter().map(|r| (r.3 - my).powi(2)).sum();
        cov / (vx * vy).sqrt().max(1e-12)
    };
    let survived = rows.iter().filter(|r| r.3 > 0.0).count();
    println!("{} vesicles at day {warm}; {survived} lineages alive after {window} days", rows.len());
    println!("r(interior backbone cut, lineage growth) = {:+.3}", corr(&|r| r.0));
    println!("r(interior chain count,  lineage growth) = {:+.3}", corr(&|r| r.1));
    println!("r(osmotic load,          lineage growth) = {:+.3}", corr(&|r| r.2));
}

/// Scale-2 shadow: from one mid-run state, compare detail, auto-surrogate and a reseeded
/// detail twin on population statistics (what the scale above consumes).
fn pond_shadow(args: &[String]) {
    use primordium_core::cosmos::Cosmos;
    use primordium_core::hybrid::Mode;
    use primordium_core::pool::PoolParams;
    use primordium_core::protocell::{Pond, PondParams};
    use primordium_core::surrogate::{RateTable, agreement};
    let warm: f64 = arg(args, "warm", 30.0);
    let days: f64 = arg(args, "days", 20.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut a = Pond::new(PoolParams::default(), PondParams::default(), Cosmos::new(CosmosParams::default()), rates, 1);
    while a.t_days < warm {
        a.step(0.01);
    }
    let mut b = a.clone();
    b.pp.auto = true;
    let mut twin = a.clone();
    twin.ctx.reseed(0xD1CE);
    let summary = |p: &Pond, divs0: u64| -> Vec<f64> {
        let n = p.vesicles.len().max(1) as f64;
        vec![
            p.vesicles.len() as f64,
            (p.stats.divisions - divs0) as f64,
            p.vesicles.iter().map(|v| v.area()).sum::<i64>() as f64 / n,
            p.vesicles.iter().map(|v| v.interior.chains.len()).sum::<usize>() as f64,
            p.vesicles.iter().map(|v| v.interior.free.iter().sum::<i64>()).sum::<i64>() as f64,
            p.outside.chains.len() as f64,
        ]
    };
    let d0 = a.stats.divisions;
    let (mut ta, mut tb) = (0.0, 0.0);
    let (mut acc, mut ceil, mut n) = (0.0, 0.0, 0.0);
    let end = a.t_days + days;
    let mut next_report = a.t_days + 2.0;
    while a.t_days < end {
        let t = Instant::now();
        a.step(0.01);
        ta += t.elapsed().as_secs_f64();
        let t = Instant::now();
        b.step(0.01);
        tb += t.elapsed().as_secs_f64();
        twin.step(0.01);
        if a.t_days >= next_report {
            next_report += 2.0;
            let (sa, sb, st) = (summary(&a, d0), summary(&b, d0), summary(&twin, d0));
            let (x, y) = (agreement(&sa, &sb), agreement(&sa, &st));
            acc += x;
            ceil += y;
            n += 1.0;
            let sur = b.vesicles.iter().filter(|v| v.mode == Mode::Surrogate).count();
            println!("t={:6.1} agreement surrogate {:.1}% twin {:.1}% | surrogate vesicles {sur}/{} | detail {:?} surrogate {:?}", a.t_days, x * 100.0, y * 100.0, b.vesicles.len(), sa.iter().map(|v| v.round()).collect::<Vec<_>>(), sb.iter().map(|v| v.round()).collect::<Vec<_>>());
        }
    }
    println!("mean agreement {:.1}% vs twin ceiling {:.1}% = {:.1}% of ceiling | time detail {:.1}s surrogate {:.1}s", 100.0 * acc / n, 100.0 * ceil / n, 100.0 * acc / ceil, ta, tb);
}
