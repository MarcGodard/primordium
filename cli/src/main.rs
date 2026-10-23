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
        "fold-kernel" => fold_kernel(&args),
        "pond" => pond(&args),
        "pond-probe" => pond_probe(&args),
        "selection" => selection(&args),
        "pond-shadow" => pond_shadow(&args),
        "cells" => cells(&args),
        "cell-selection" => cell_selection(&args),
        "cell-shadow" => cell_shadow(&args),
        "coast" => coast(&args),
        "coast-shadow" => coast_shadow(&args),
        "reef-seed" => reef_seed(&args),
        "reef" => reef(&args),
        "elements" => elements(&args),
        "reef-shadow" => reef_shadow(&args),
        "reef-goal" => reef_goal(&args),
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
                w.t_days,
                e.temperature_k,
                e.visible,
                e.uv,
                e.tide_level,
                c.molecules,
                c.bonds,
                c.largest,
                c.excited,
                c.size_hist,
                k.forms,
                k.breaks,
                k.exchanges,
                k.uv_breaks,
                w.ledger.drift()
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
            f.energy,
            func.stability,
            func.amphiphile,
            func.pigment,
            cut,
            primordium_core::element::NAMES[BONDABLE[a] as usize - 1],
            primordium_core::element::NAMES[BONDABLE[b] as usize - 1]
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
    // own dice for the surrogate too: shared dice would credit it with the detail run's noise
    b.reseed(seed ^ 0x5EED);
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
                a.t_days,
                a.env.temperature_k,
                a.env.tide_level,
                sa.iter().sum::<f64>(),
                sb.iter().sum::<f64>(),
                acc * 100.0,
                floor * 100.0,
                null * 100.0,
                conf.join(" "),
                detail,
                b.hybrid.discoveries.entries.len()
            );
        }
    }
    println!(
        "mean agreement {:.1}% | detail twin {:.1}% | frozen-null {:.1}% | relative to twin ceiling {:.1}% | speed detail {:.0} sweeps/s, hybrid {:.0} sweeps/s | ledger drift a={} b={} | lift shortfall {}",
        100.0 * sum_acc / n,
        100.0 * sum_twin / n,
        100.0 * sum_null / n,
        100.0 * sum_acc / sum_twin,
        total as f64 / ta,
        total as f64 / tb,
        a.ledger.drift(),
        b.ledger.drift(),
        b.hybrid.lift_shortfall
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
                p.t_days,
                p.env.temperature_k,
                if p.wet() { "wet" } else { "dry" },
                n,
                mean_len,
                max_len,
                max_gen,
                st.copies,
                st.mutations as f64 / st.copied_letters.max(1) as f64,
                st.breaks,
                st.ligations,
                p.c.backbone_cut(),
                p.ctx.best_cut,
                p.ctx.model.folds,
                p.ctx.model.exact,
                p.ctx.model.nearest
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
        let avg = |v: &mut dyn Iterator<Item = f64>| {
            let (s, c) = v.fold((0.0, 0.0), |a, x| (a.0 + x, a.1 + 1.0));
            100.0 * s / c
        };
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

/// Scale-1 surrogate shadow: fold relatives of a sequence into a FunctionModel, hide the
/// sequence itself, compare the model's prediction to its fold; ceiling = a reseeded fold.
fn fold_kernel(args: &[String]) {
    use primordium_core::element::Table;
    use primordium_core::polymer::{LetterPhysics, N_LETTERS, express};
    use primordium_core::pool::{FunctionModel, PoolParams, kt_bin};
    use primordium_core::rng::Rng;
    use primordium_core::surrogate::agreement;
    let n: usize = arg(args, "n", 60);
    let rel: usize = arg(args, "rel", 8);
    let kt: f64 = arg(args, "kt", 1.0);
    let ph = LetterPhysics::new(&Table::new());
    let mut rng = Rng::new(9);
    let kt = (kt / 0.05).round() * 0.05; // the model folds at its kT bin
    for len in [8usize, 16, 24, 32] {
        let (mut ceil, mut blend, mut near, mut used, mut folded) = (0.0, 0.0, 0.0, 0, 0);
        for _ in 0..n {
            let s: Vec<u8> = (0..len).map(|_| rng.below(N_LETTERS) as u8).collect();
            // relatives as an evolving population leaves them: 1- and 2-mutants
            let mut model = FunctionModel::default();
            let mut nearest: Option<(usize, Vec<f64>)> = None;
            for r in 0..rel {
                let mut m = s.clone();
                for _ in 0..1 + r % 2 {
                    let k = rng.below(len);
                    m[k] = ((m[k] as usize + 1 + rng.below(N_LETTERS - 1)) % N_LETTERS) as u8;
                }
                let d = m.iter().zip(&s).filter(|(a, b)| a != b).count();
                // the sim never refolds a cached sequence: a repeat would count twice in the blend
                if d == 0 || model.cached(&m, kt_bin(kt)).is_some() {
                    continue;
                }
                let f = model.fold(&m, kt, &ph).as_vec();
                if nearest.as_ref().is_none_or(|(nd, _)| d < *nd) {
                    nearest = Some((d, f));
                }
            }
            // the sim folds instead of predicting below its confidence gate
            let Some((p, conf)) = model.predict(&s, kt) else { continue };
            if conf < PoolParams::default().fold_below {
                folded += 1;
                continue;
            }
            let f = express(&s, &ph, kt, 1).1.as_vec();
            ceil += agreement(&f, &express(&s, &ph, kt, 77).1.as_vec());
            blend += agreement(&f, &p.as_vec());
            near += agreement(&f, &nearest.unwrap().1);
            used += 1;
        }
        if used == 0 {
            println!("len {len:2}: always folded ({folded} below the confidence gate)");
            continue;
        }
        println!(
            "len {len:2}: ceiling {:5.1}% | blend {:5.1}% vs nearest {:5.1}% of ceiling | {used} predicted, {folded} folded instead",
            100.0 * ceil / used as f64,
            100.0 * blend / ceil,
            100.0 * near / ceil
        );
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
    let mut p =
        Pond::new(PoolParams::default(), PondParams { auto, ..Default::default() }, Cosmos::new(CosmosParams { seed, ..Default::default() }), rates, seed);
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
                p.t_days,
                p.env.temperature_k,
                if p.wet() { "wet" } else { "dry" },
                n,
                sur,
                mean_area,
                gens,
                st.nucleations,
                st.divisions,
                st.bursts,
                st.insertions,
                st.adsorptions,
                p.mean_vesicle_cut(),
                p.outside.backbone_cut(),
                {
                    let s = p.total_interior_stats();
                    s.mutations as f64 / s.copied_letters.max(1) as f64
                },
                p.outside.stats.mutations as f64 / p.outside.stats.copied_letters.max(1) as f64,
                p.outside.chains.len(),
                p.vesicles.iter().map(|v| v.interior.chains.len()).sum::<usize>(),
                p.ctx.model.folds,
                p.ctx.model.exact,
                p.ctx.model.nearest,
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
    println!(
        "outside: cells {:.0} chains {} free {:?} raw {:?} raw_ka {} cut {:.2} synth {:.2}",
        o.cells,
        o.chains.len(),
        o.free,
        o.raw,
        o.raw_ka,
        o.backbone_cut(),
        o.synthesis_cut()
    );
    let mut vs: Vec<_> = p.vesicles.iter().collect();
    vs.sort_by_key(|v| -v.area());
    for v in vs.iter().take(8) {
        let i = &v.interior;
        println!(
            "v{} gen {} area {} birth {} cells {:.0} chains {} amph {:.2} free {} raw {} ka {} cut {:.2} synth {:.2} copies {} synthesized {}",
            v.id,
            v.generation,
            v.area(),
            v.birth_area,
            i.cells,
            i.chains.len(),
            v.amph_frac(0.3),
            i.free.iter().sum::<i64>(),
            i.raw.iter().sum::<i64>(),
            i.raw_ka,
            i.backbone_cut(),
            i.synthesis_cut(),
            i.stats.copies,
            i.stats.synthesized
        );
    }
    let amph = o.chains.iter().filter(|c| c.func.amphiphile >= 0.3).count();
    println!("outside amphiphiles {amph} of {}; amphiphile score dist: {:?}", o.chains.len(), {
        let mut h = [0; 5];
        for c in &o.chains {
            h[((c.func.amphiphile * 5.0) as usize).min(4)] += 1;
        }
        h
    });
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
    let traits: HashMap<u64, (f64, f64, f64, i64)> =
        p.vesicles.iter().map(|v| (v.id, (v.interior.backbone_cut(), v.interior.chains.len() as f64, v.osmotic(), v.area()))).collect();
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
    // own dice for the surrogate too: shared dice would credit it with the detail run's noise
    b.ctx.reseed(0x5EED);
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
            println!(
                "t={:6.1} agreement surrogate {:.1}% twin {:.1}% | surrogate vesicles {sur}/{} | detail {:?} surrogate {:?}",
                a.t_days,
                x * 100.0,
                y * 100.0,
                b.vesicles.len(),
                sa.iter().map(|v| v.round()).collect::<Vec<_>>(),
                sb.iter().map(|v| v.round()).collect::<Vec<_>>()
            );
        }
    }
    println!(
        "mean agreement {:.1}% vs twin ceiling {:.1}% = {:.1}% of ceiling | time detail {:.1}s surrogate {:.1}s",
        100.0 * acc / n,
        100.0 * ceil / n,
        100.0 * acc / ceil,
        ta,
        tb
    );
}

/// Scale-3 world of cells on the beach.
fn cells(args: &[String]) {
    use primordium_core::cell::{CellParams, World};
    use primordium_core::cosmos::Cosmos;
    use primordium_core::hybrid::Mode;
    use primordium_core::pool::PoolParams;
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 60.0);
    let seed: u64 = arg(args, "seed", 1);
    let dt: f64 = arg(args, "dt", 0.02);
    let every: f64 = arg(args, "every", 5.0);
    let auto: bool = arg(args, "auto", true);
    let founders: usize = arg(args, "founders", 200);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut w = World::new(
        PoolParams::default(),
        CellParams { auto, ..Default::default() },
        Cosmos::new(CosmosParams { seed, ..Default::default() }),
        rates,
        seed,
        founders,
    );
    let start = Instant::now();
    let per = (every / dt).round() as u64;
    for s in 1..=(days / dt) as u64 {
        w.step(dt);
        if s % per == 0 {
            let n = w.cells.len().max(1) as f64;
            let ph = w.phenotype_means();
            let sur = w.cells.iter().filter(|c| c.mode == Mode::Surrogate).count();
            let area = w.cells.iter().map(|c| c.area()).sum::<i64>() as f64 / n;
            let prods = w.cells.iter().map(|c| c.products.iter().sum::<i64>()).sum::<i64>() as f64 / n;
            let free = w.cells.iter().map(|c| c.free.iter().sum::<i64>()).sum::<i64>() as f64 / n;
            let raw = w.cells.iter().map(|c| c.raw_ka).sum::<i64>() as f64 / n;
            let forks = w.cells.iter().filter(|c| c.fork.is_some()).count();
            let maxgen = w.cells.iter().map(|c| c.generation).max().unwrap_or(0);
            let st = &w.stats;
            println!(
                "t={:6.1} T={:5.1}K | cells {} (sur {}) gen {} div {} death {} | area {:.0} prods {:.1} free {:.0} rawka {:.0} forks {} | poly {:.2} synth {:.2} amph {:.2} pig {:.2} len {:.0} genes {:.1} | letter err {:.3} bit err {:.4} radmut {} | {:.1}s",
                w.t_days,
                w.env.temperature_k,
                w.cells.len(),
                sur,
                maxgen,
                st.divisions,
                st.deaths,
                area,
                prods,
                free,
                raw,
                forks,
                ph[0],
                ph[1],
                ph[2],
                ph[3],
                ph[4],
                ph[5],
                st.copy_mutations as f64 / st.copied_letters.max(1) as f64,
                st.bit_errors as f64 / st.bits_inherited.max(1) as f64,
                st.radiation_mutations,
                start.elapsed().as_secs_f64()
            );
        }
    }
    for d in w.ctx.discoveries.entries.iter().rev().take(5) {
        println!("discovery day {:.1} scale {}: {}", d.t_days, d.scale, d.note);
    }
    let st = &w.stats;
    println!(
        "transcripts ok {} junk {} short {} ({} messenger letters) | products decayed {} inserted {}",
        st.tx_ok, st.tx_junk, st.tx_short, st.tx_letters, st.products_decayed, st.products_inserted
    );
}

/// Selection gradient at scale 3: does genome phenotype predict lineage growth (descendant count)?
fn cell_selection(args: &[String]) {
    use primordium_core::cell::{CellParams, World};
    use primordium_core::cosmos::Cosmos;
    use primordium_core::pool::PoolParams;
    use primordium_core::surrogate::RateTable;
    use std::collections::HashMap;
    let warm: f64 = arg(args, "warm", 60.0);
    let window: f64 = arg(args, "window", 10.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut w = World::new(PoolParams::default(), CellParams { auto: false, ..Default::default() }, Cosmos::new(CosmosParams::default()), rates, 1, 200);
    while w.t_days < warm {
        w.step(0.02);
    }
    let snap: Vec<(u64, [f64; 5])> = w
        .cells
        .iter()
        .map(|c| c.genome.clone())
        .zip(w.cells.iter().map(|c| c.id))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(g, id)| {
            let i = w.info(&g);
            (id, [i.poly_cut, i.synth_cut, i.amph, i.pigment, i.genes.len() as f64])
        })
        .collect();
    let mut root: HashMap<u64, u64> = snap.iter().map(|(id, _)| (*id, *id)).collect();
    let end = w.t_days + window;
    while w.t_days < end {
        w.step(0.02);
        for c in &w.cells {
            if !root.contains_key(&c.id) {
                if let Some(r) = c.parent.and_then(|p| root.get(&p).copied()) {
                    root.insert(c.id, r);
                }
            }
        }
    }
    let mut kids: HashMap<u64, f64> = HashMap::new();
    for c in &w.cells {
        if let Some(&r) = root.get(&c.id) {
            *kids.entry(r).or_default() += 1.0;
        }
    }
    let rows: Vec<([f64; 5], f64)> = snap.iter().map(|(id, t)| (*t, kids.get(id).copied().unwrap_or(0.0))).collect();
    let n = rows.len() as f64;
    let names = ["polymerase", "synthesis", "amphiphile", "pigment", "gene count"];
    println!("{} cells at day {warm}, lineages tracked {window} days", rows.len());
    for k in 0..5 {
        let mx = rows.iter().map(|r| r.0[k]).sum::<f64>() / n;
        let my = rows.iter().map(|r| r.1).sum::<f64>() / n;
        let cov: f64 = rows.iter().map(|r| (r.0[k] - mx) * (r.1 - my)).sum();
        let vx: f64 = rows.iter().map(|r| (r.0[k] - mx).powi(2)).sum();
        let vy: f64 = rows.iter().map(|r| (r.1 - my).powi(2)).sum();
        println!("r({:<11}, descendants) = {:+.3}", names[k], cov / (vx * vy).sqrt().max(1e-12));
    }
}

/// Scale-3 shadow: detail vs auto-surrogate vs reseeded detail twin, on population statistics.
fn cell_shadow(args: &[String]) {
    use primordium_core::cell::{CellParams, World};
    use primordium_core::cosmos::Cosmos;
    use primordium_core::hybrid::Mode;
    use primordium_core::pool::PoolParams;
    use primordium_core::surrogate::{RateTable, agreement};
    let warm: f64 = arg(args, "warm", 40.0);
    let days: f64 = arg(args, "days", 30.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").unwrap()).unwrap();
    let mut a = World::new(PoolParams::default(), CellParams { auto: false, ..Default::default() }, Cosmos::new(CosmosParams::default()), rates, 1, 200);
    while a.t_days < warm {
        a.step(0.02);
    }
    let mut b = a.clone();
    b.cp.auto = true;
    let mut twin = a.clone();
    twin.reseed(0xD1CE);
    // own dice for the surrogate too: shared dice would credit it with the detail run's noise
    b.reseed(0x5EED);
    let d0 = a.stats.divisions;
    let summary = |w: &mut World| -> Vec<f64> {
        let n = w.cells.len().max(1) as f64;
        let ph = w.phenotype_means();
        vec![
            w.cells.len() as f64,
            (w.stats.divisions - d0) as f64,
            w.cells.iter().map(|c| c.area()).sum::<i64>() as f64 / n,
            ph[0] * 100.0,
            ph[2] * 100.0,
            ph[5] * 10.0,
            ph[4],
        ]
    };
    let (mut acc, mut ceil, mut k) = (0.0, 0.0, 0.0);
    let (mut ta, mut tb) = (0.0, 0.0);
    let end = a.t_days + days;
    let mut next = a.t_days + 3.0;
    while a.t_days < end {
        let t = Instant::now();
        a.step(0.02);
        ta += t.elapsed().as_secs_f64();
        let t = Instant::now();
        b.step(0.02);
        tb += t.elapsed().as_secs_f64();
        twin.step(0.02);
        if a.t_days >= next {
            next += 3.0;
            let (sa, sb, st) = (summary(&mut a), summary(&mut b), summary(&mut twin));
            let (x, y) = (agreement(&sa, &sb), agreement(&sa, &st));
            acc += x;
            ceil += y;
            k += 1.0;
            let sur = b.cells.iter().filter(|c| c.mode == Mode::Surrogate).count();
            println!(
                "t={:6.1} surrogate {:.1}% twin {:.1}% | surrogate cells {sur}/{} | detail {:?} surrogate {:?}",
                a.t_days,
                x * 100.0,
                y * 100.0,
                b.cells.len(),
                sa.iter().map(|v| v.round()).collect::<Vec<_>>(),
                sb.iter().map(|v| v.round()).collect::<Vec<_>>()
            );
        }
    }
    println!(
        "mean agreement {:.1}% vs twin ceiling {:.1}% = {:.1}% of ceiling | time detail {:.1}s surrogate {:.1}s",
        100.0 * acc / k,
        100.0 * ceil / k,
        100.0 * acc / ceil,
        ta,
        tb
    );
}

/// Scale-4 coastline: per-site populations and phenotypes along a latitude gradient.
fn coast(args: &[String]) {
    use primordium_core::coast::{Coast, CoastParams};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 200.0);
    let every: f64 = arg(args, "every", 50.0);
    let sites: usize = arg(args, "sites", 12);
    let seed: u64 = arg(args, "seed", 1);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut c = Coast::new(CoastParams { sites, ..Default::default() }, CosmosParams { seed, ..Default::default() }, rates, seed);
    let lineage_after: f64 = arg(args, "lineage-after", f64::INFINITY);
    let start = Instant::now();
    let m0 = c.matter();
    let mut next = every;
    let mut switched: Option<(f64, Instant)> = None;
    while c.t_days < days {
        if switched.is_none() && c.t_days >= lineage_after {
            c.set_mode(primordium_core::hybrid::Mode::Surrogate);
            c.cp.lift_below = 0.0;
            println!(
                "day {:.1}: all sites to lineage mode after {:.1}s; folds {} predictions {} genomes {} divisions {}",
                c.t_days,
                start.elapsed().as_secs_f64(),
                c.sites.iter().map(|s| s.world.ctx.model.folds).sum::<u64>(),
                c.sites.iter().map(|s| s.world.ctx.model.nearest).sum::<u64>(),
                c.sites.iter().map(|s| s.world.stats.new_genomes).sum::<u64>(),
                c.sites.iter().map(|s| s.world.stats.divisions).sum::<u64>()
            );
            switched = Some((c.t_days, Instant::now()));
        }
        c.step(0.02);
        if c.t_days >= next {
            next += every;
            let (di, dn): (f64, usize) = c.sites.iter().map(|s| s.world.pigment_differentiation()).fold((0.0, 0), |a, (d, n)| (a.0 + d * n as f64, a.1 + n));
            println!(
                "t={:6.1} cells {} migrations {} | pigment differentiation in colonies {:.2} ({} colonial cells) | {:.1}s",
                c.t_days,
                c.cells(),
                c.stats.migrations,
                di / dn.max(1) as f64,
                dn,
                start.elapsed().as_secs_f64()
            );
            for s in c.sites.iter_mut() {
                let w = &mut s.world;
                let ph = w.phenotype_means();
                let (colonial, size, adh) = w.colony_stats();
                let reg = w.regulation_stats();
                let st = &w.stats;
                println!(
                    "   lat {:+5.2} tide x{:.2} T {:5.1}K | cells {:3} gen {:3} | poly {:.2} synth {:.2} amph {:.2} pig {:.2} len {:3.0} | bit err {:.3} | adh {adh:.3} colonial {:3.0}% size {size:4.1} | genes on light {:.2} cold {:.2} Li {:.2} mates {:.2}, pigment light-on {:.2} | eaten {}",
                    s.latitude,
                    s.tide_scale,
                    w.env.temperature_k,
                    w.cells.len(),
                    w.cells.iter().map(|c| c.generation).max().unwrap_or(0),
                    ph[0],
                    ph[1],
                    ph[2],
                    ph[3],
                    ph[4],
                    st.bit_errors as f64 / st.bits_inherited.max(1) as f64,
                    colonial * 100.0,
                    reg[1],
                    reg[2],
                    reg[3],
                    reg[4],
                    reg[7],
                    st.predations
                );
            }
        }
    }
    if let Some((d0, t0)) = switched {
        println!(
            "lineage mode: {:.1} days in {:.1}s; folds {} predictions {} genomes {} divisions {}",
            c.t_days - d0,
            t0.elapsed().as_secs_f64(),
            c.sites.iter().map(|s| s.world.ctx.model.folds).sum::<u64>(),
            c.sites.iter().map(|s| s.world.ctx.model.nearest).sum::<u64>(),
            c.sites.iter().map(|s| s.world.stats.new_genomes).sum::<u64>(),
            c.sites.iter().map(|s| s.world.stats.divisions).sum::<u64>()
        );
    }
    println!("matter drift: {}", if c.matter() == m0 { "0" } else { "NONZERO" });
}

/// Scale-4 shadow: detailed coast vs all sites as lineages vs a reseeded detailed twin,
/// on per-site population, division and phenotype statistics.
fn coast_shadow(args: &[String]) {
    use primordium_core::coast::{Coast, CoastParams};
    use primordium_core::hybrid::Mode;
    use primordium_core::surrogate::{RateTable, agreement};
    let warm: f64 = arg(args, "warm", 60.0);
    let days: f64 = arg(args, "days", 40.0);
    let sites: usize = arg(args, "sites", 12);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut a = Coast::new(CoastParams { sites, ..Default::default() }, CosmosParams::default(), rates, 1);
    let t0 = Instant::now();
    while a.t_days < warm {
        a.step(0.02);
    }
    println!("warm-up {warm} days in {:.1}s, lineage table coverage {:.1}%", t0.elapsed().as_secs_f64(), a.model.base.coverage() * 100.0);
    let lift: f64 = arg(args, "lift", 0.5);
    let learn: bool = arg(args, "learn", true);
    a.learn = false;
    let mut b = a.clone();
    b.set_mode(Mode::Surrogate);
    b.cp.auto = true;
    b.cp.lift_below = lift;
    // a site that falls back to detail keeps training the table, as in a normal run
    b.learn = learn;
    b.reseed(0x5EED);
    let mut twin = a.clone();
    twin.reseed(0xD1CE);
    let d0: Vec<u64> = a.sites.iter().map(|s| s.world.stats.divisions).collect();
    let errs = |c: &Coast| {
        c.sites.iter().fold((0u64, 0u64, 0u64), |acc, s| {
            (acc.0 + s.world.stats.bit_errors, acc.1 + s.world.stats.bits_inherited, acc.2 + s.world.stats.radiation_mutations)
        })
    };
    let (ea0, eb0) = (errs(&a), errs(&b));
    if std::env::var("DIAG").is_ok() {
        let e = errs(&a);
        println!("detail lifetime bit error {:.4}", e.0 as f64 / e.1.max(1) as f64);
        let (e1, n1) = a.sites.iter().fold((0, 0), |acc, s| (acc.0 + s.world.stats.bit_errors_on1, acc.1 + s.world.stats.ones_inherited));
        let (et, nt) = (e.0, e.1);
        println!("   detail spectrum: error on 1-bits {:.4}, on 0-bits {:.4}", e1 as f64 / n1.max(1) as f64, (et - e1) as f64 / (nt - n1).max(1) as f64);
        let mut el = [0u64; 5];
        let mut bl = [0u64; 5];
        for s in &a.sites {
            for l in 0..5 {
                el[l] += s.world.stats.errors_by_letter[l];
                bl[l] += s.world.stats.bits_by_letter[l];
            }
        }
        let letters: Vec<u8> = a.sites[6].world.cells.iter().flat_map(|c| c.genome.clone()).collect();
        let comp: Vec<usize> = (0..5).map(|l| letters.iter().filter(|&&g| g as usize == l).count()).collect();
        println!(
            "   by source letter (Hy Li Bo Ox Fl): error {:?} share {:?} | genome letters now {:?}",
            (0..5).map(|l| format!("{:.3}", el[l] as f64 / bl[l].max(1) as f64)).collect::<Vec<_>>(),
            bl,
            comp
        );
        let f = |v: Vec<f64>| v.iter().map(|x| format!("{x:.3}")).collect::<Vec<_>>().join(" ");
        for (k, name) in [(0, "poly"), (1, "synth"), (2, "amph")] {
            println!("   {name:5} relative division rate [{}]", f(b.model.fitness.marginal(k, 0)));
        }
        println!("   age   divisions/day [{}]", f(b.model.base.marginal(3, 0)));
    }
    let m0 = b.matter();
    let summary = |c: &mut Coast| -> Vec<f64> {
        let mut v = Vec::new();
        for (k, s) in c.sites.iter_mut().enumerate() {
            let ph = s.world.phenotype_means();
            let (colonial, _, adh) = s.world.colony_stats();
            v.extend([
                s.world.cells.len() as f64,
                (s.world.stats.divisions - d0[k]) as f64,
                ph[0] * 100.0,
                ph[1] * 100.0,
                ph[2] * 100.0,
                ph[3] * 100.0,
                ph[4],
                colonial * 100.0,
                adh * 100.0,
            ]);
        }
        v
    };
    const STATS: usize = 9;
    let (mut acc, mut ceil, mut n) = (0.0, 0.0, 0.0);
    let (mut ta, mut tb) = (0.0, 0.0);
    let end = a.t_days + days;
    let mut next = a.t_days + 5.0;
    let mut predicted = vec![0.0; sites];
    let diag = std::env::var("DIAG").is_ok();
    while a.t_days < end {
        if diag {
            // what the lineage model would predict for detail's own cells
            let prior = b.model.priors();
            for (k, s) in a.sites.iter_mut().enumerate() {
                let w = &mut s.world;
                let cells = w.cells.clone();
                for c in &cells {
                    let info = w.info(&c.genome);
                    let site = w.patch_site(c.patch, 0.0);
                    let x = w.lineage_features(c, &info, &site, site.x[2]);
                    predicted[k] += b.model.rates(&x, &prior).0 * 0.02;
                }
            }
        }
        let t = Instant::now();
        a.step(0.02);
        ta += t.elapsed().as_secs_f64();
        let t = Instant::now();
        b.step(0.02);
        tb += t.elapsed().as_secs_f64();
        twin.step(0.02);
        if a.t_days >= next {
            next += 5.0;
            let (sa, sb, st) = (summary(&mut a), summary(&mut b), summary(&mut twin));
            let (x, y) = (agreement(&sa, &sb), agreement(&sa, &st));
            acc += x;
            ceil += y;
            n += 1.0;
            let lin = b.sites.iter().filter(|s| s.mode == Mode::Surrogate).count();
            let div = |v: &[f64]| v.chunks(STATS).map(|c| c[1]).sum::<f64>();
            if std::env::var("DIAG").is_ok() {
                let col = |v: &[f64], j: usize| v.chunks(STATS).map(|c| c[j]).collect::<Vec<_>>();
                let names = ["cells", "divs", "poly", "synth", "amph", "pig", "len", "colonial", "adh"];
                let row: Vec<String> = (0..STATS)
                    .map(|j| format!("{} {:.1}/{:.1}", names[j], 100.0 * agreement(&col(&sa, j), &col(&sb, j)), 100.0 * agreement(&col(&sa, j), &col(&st, j))))
                    .collect();
                println!("   {}", row.join("  "));
                let ready = b.sites.iter().map(|s| s.world.cells.iter().filter(|c| c.ready).count()).sum::<usize>();
                println!("   lineage cells waiting to pay for a daughter: {ready}/{}", b.cells());
                println!(
                    "   divisions per site: detail {:?}\n                       lineage {:?}\n      model on detail cells {:?}",
                    col(&sa, 1),
                    col(&sb, 1),
                    predicted.iter().map(|p| p.round()).collect::<Vec<_>>()
                );
                let mean = |v: &[f64], j: usize| col(v, j).iter().sum::<f64>() / sites as f64;
                println!(
                    "   coast means detail/lineage/twin: poly {:.1}/{:.1}/{:.1} amph {:.1}/{:.1}/{:.1} len {:.0}/{:.0}/{:.0}",
                    mean(&sa, 2),
                    mean(&sb, 2),
                    mean(&st, 2),
                    mean(&sa, 4),
                    mean(&sb, 4),
                    mean(&st, 4),
                    mean(&sa, 6),
                    mean(&sb, 6),
                    mean(&st, 6)
                );
            }
            println!(
                "t={:6.1} lineage {:.1}% twin {:.1}% | lineage sites {lin}/{} | divisions detail {} lineage {} twin {}",
                a.t_days,
                x * 100.0,
                y * 100.0,
                b.sites.len(),
                div(&sa),
                div(&sb),
                div(&st)
            );
        }
    }
    let drift = if b.matter() == m0 { "0" } else { "NONZERO" };
    let (ea, eb) = (errs(&a), errs(&b));
    println!(
        "bit error per division: detail {:.4} lineage {:.4} | radiation mutations detail {} lineage {}",
        (ea.0 - ea0.0) as f64 / (ea.1 - ea0.1).max(1) as f64,
        (eb.0 - eb0.0) as f64 / (eb.1 - eb0.1).max(1) as f64,
        ea.2 - ea0.2,
        eb.2 - eb0.2
    );
    for d in b.discoveries.entries.iter().take(5) {
        println!("discovery day {:.1}: {}", d.t_days, d.note);
    }
    println!(
        "mean agreement {:.1}% vs twin ceiling {:.1}% = {:.1}% of ceiling | time detail {:.1}s lineage {:.1}s ({:.1}x) | lifts {} | matter drift {}",
        100.0 * acc / n,
        100.0 * ceil / n,
        100.0 * acc / ceil,
        ta,
        tb,
        ta / tb.max(1e-9),
        b.stats.lifts,
        drift
    );
}

/// Evolve a coast, then hand its lineage model and a sample of its cells (colonial first) to the reef.
fn reef_seed(args: &[String]) {
    use primordium_core::coast::{Coast, CoastParams};
    use primordium_core::reef::{Founder, ReefSeed};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 200.0);
    let seed: u64 = arg(args, "seed", 1);
    let n: usize = arg(args, "founders", 96);
    let out: String = arg(args, "out", "data/reef_seed.json".to_string());
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let mut c = Coast::new(CoastParams::default(), CosmosParams { seed, ..Default::default() }, rates, seed);
    let start = Instant::now();
    while c.t_days < days {
        c.step(0.02);
        if (c.t_days / 20.0).fract() < 0.02 / 20.0 {
            println!("day {:.0}: {} cells, {:.0}s", c.t_days, c.cells(), start.elapsed().as_secs_f64());
        }
    }
    let mut pool: Vec<Founder> = Vec::new();
    for s in c.sites.iter_mut() {
        let lat = s.latitude;
        let cells = s.world.cells.clone();
        for cell in &cells {
            // the body a cell is built of (membrane, products, junk), not its pantry of loose and raw atoms
            let info = s.world.info(&cell.genome);
            let mut mass = cell.membrane;
            for l in 0..mass.len() {
                mass[l] += cell.junk[l];
            }
            for (g, &k) in info.genes.iter().zip(&cell.products) {
                for &l in &g.product {
                    mass[l as usize] += k;
                }
            }
            pool.push(Founder { genome: cell.genome.clone(), latitude: lat, colonial: cell.colony.is_some(), mass });
        }
    }
    // spread over the coast: every k-th cell, colonial ones first
    pool.sort_by_key(|f| !f.colonial);
    let step = (pool.len() / n.max(1)).max(1);
    let founders: Vec<Founder> = pool.into_iter().step_by(step).take(n).collect();
    let colonial = founders.iter().filter(|f| f.colonial).count();
    let seed = ReefSeed { days: c.t_days, model: c.model.clone(), founders };
    std::fs::write(&out, serde_json::to_string(&seed).unwrap()).unwrap();
    println!("wrote {out}: {} founders ({colonial} colonial) after {:.0} days, {:.0}s", seed.founders.len(), c.t_days, start.elapsed().as_secs_f64());
}

fn reef(args: &[String]) {
    use primordium_core::reef::{Reef, ReefParams, ReefSeed, TILE};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 60.0);
    let every: f64 = arg(args, "every", 5.0);
    let seed: u64 = arg(args, "seed", 1);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let rs: ReefSeed = serde_json::from_str(&std::fs::read_to_string("data/reef_seed.json").expect("run reef-seed first")).unwrap();
    let start = Instant::now();
    let rp = ReefParams { graze_min: arg(args, "graze-min", ReefParams::default().graze_min), ..Default::default() };
    let mut r = Reef::new(rp, CosmosParams { seed, ..Default::default() }, rates, &rs, seed);
    println!("built in {:.1}s: {} founders", start.elapsed().as_secs_f64(), r.organisms.len());
    let mut sig = std::collections::BTreeMap::new();
    for o in &r.organisms {
        for g in &o.info.genes {
            *sig.entry(format!("{:?}", g.promoter.signal)).or_insert(0) += 1;
        }
    }
    println!("founder promoters: {sig:?}");
    let m0 = r.matter();
    // report at noon: light-switched genes are all off at midnight
    let mut next = every - 0.5;
    while r.t_days < days {
        r.step(0.02);
        if r.t_days >= next {
            next += every;
            let settled: Vec<_> = r.organisms.iter().filter(|o| o.settled()).collect();
            let mut sizes: Vec<usize> = settled.iter().map(|o| o.cells.len()).collect();
            sizes.sort_unstable();
            let multi = sizes.iter().filter(|&&n| n > 1).count();
            let big = sizes.last().copied().unwrap_or(0);
            let med = sizes.get(sizes.len() / 2).copied().unwrap_or(0);
            let tall = settled.iter().map(|o| o.height()).max().unwrap_or(0);
            let mean = |k: usize| {
                let (s, n) = settled.iter().flat_map(|o| o.cells.iter()).fold((0.0, 0), |a, c| (a.0 + c.expr[k] as f64, a.1 + 1));
                s / n.max(1) as f64
            };
            let st = &r.stats;
            println!(
                "t={:5.1} cells {:5} bodies {:4} (multi {:3}, median {:2}, biggest {:3}, tallest {:2}) adrift {:3} | expr poly {:.2} synth {:.2} amph {:.2} shade {:.2} adh {:.2} | div {} spores {} settled {} lost {} tears {} frags {} bites {} | bit err {:.3} unsure {} | ledger {} | {:.1}s",
                r.t_days, r.cells(), settled.len(), multi, med, big, tall, r.organisms.len() - settled.len(),
                mean(0), mean(1), mean(2), mean(3), mean(4),
                st.divisions, st.spores, st.settled, st.lost_adrift, st.tears, st.fragments, st.bites,
                st.bit_errors as f64 / st.bits_inherited.max(1) as f64, st.unsure_cells,
                if r.matter() == m0 { "exact" } else { "DRIFT" }, start.elapsed().as_secs_f64()
            );
            let (body_genes, diff) = r.labour();
            let genomes: std::collections::HashSet<&Vec<u8>> = r.organisms.iter().map(|o| &o.genome).collect();
            // where each big lineage lives: mean rock elevation and depth under mean tide
            let mut zones = Vec::new();
            for (clade, n) in r.clades().into_iter().take(5) {
                let cells: Vec<_> = r.organisms.iter().filter(|o| o.clade == clade && o.settled()).flat_map(|o| o.cells.iter()).collect();
                let elev = cells.iter().map(|c| r.tiles[r.tile_index(c.pos[0] / TILE, c.pos[1] / TILE).unwrap_or(0)].elevation).sum::<f64>() / cells.len().max(1) as f64;
                let h = cells.iter().map(|c| c.pos[2] as f64).sum::<f64>() / cells.len().max(1) as f64;
                zones.push(format!("#{clade} {:.0}% elev {elev:.2} z {h:.1}", 100.0 * n as f64 / r.cells().max(1) as f64));
            }
            println!("        lineages {}: {}", r.clades().len(), zones.join(" | "));
            let mobile: Vec<_> = r.organisms.iter().filter(|o| o.settled() && o.speed > 0.0).collect();
            let multi_mobile = mobile.iter().filter(|o| o.cells.len() > 1).count();
            let fastest = mobile.iter().map(|o| o.speed).fold(0.0f32, f32::max);
            println!("        crawlers {} ({} multicellular), fastest {:.2} cells/day, steps {}", mobile.len(), multi_mobile, fastest, r.stats.moves);
            let spore_bodies = r.organisms.iter().filter(|o| o.settled() && o.generation > 0 && o.cells.len() > 1).count();
            println!("        multicellular bodies founded by spores {spore_bodies}");
            let max_gen = r.organisms.iter().map(|o| o.generation).max().unwrap_or(0);
            let mean_gen = r.organisms.iter().map(|o| o.generation as f64 * o.cells.len() as f64).sum::<f64>() / r.cells().max(1) as f64;
            println!(
                "        body-signal genes {:.1}%  within-body differentiation {:.2}  genomes {}  generation max {max_gen} cell-mean {mean_gen:.1}",
                body_genes * 100.0,
                diff,
                genomes.len()
            );
        }
    }
    for d in r.discoveries.entries.iter().take(5) {
        println!("  discovery day {:.1}: {}", d.t_days, d.note);
    }
}

/// Preview a periodic table under other shell capacities: elements, the backbone atom the
/// rule picks, side-group letters, each letter's pairing partner and which letters survive
/// copying twice (the heritable alphabet a genome in that universe would be written in).
fn elements(args: &[String]) {
    use primordium_core::element::{NAME_POOL, Universe, shell_total};
    let Ok(shells) = arg(args, "shells", "2,6".to_string()).split(',').map(|v| v.trim().parse::<u8>()).collect::<Result<Vec<u8>, _>>() else {
        println!("--shells takes capacities like 2,6,6");
        return;
    };
    let n = shell_total(&shells);
    if n > NAME_POOL.len() {
        println!("{n} elements: more than NAME_POOL holds");
        return;
    }
    let u = Universe::new(&shells);
    println!("shells {shells:?}: {n} elements");
    println!("  Z  name shell valence   chi   hydropathy  role");
    for (e, r) in u.elements.iter().zip(&u.roles) {
        println!("{:3}  {:4} {:5} {:7} {:6.2} {:8.2}     {r:?}", e.z, e.name, e.shell, e.valence, e.chi, e.hydropathy);
    }
    if u.backbone == 0 {
        println!("no element bonds: no chemistry");
        return;
    }
    let b = u.get(u.backbone);
    println!("backbone: {} (valence {})", b.name, b.valence);
    if b.valence < 3 {
        println!("  no atom can hold a chain plus a side group: no readable polymers");
        return;
    } else if b.valence > 3 {
        println!("  valence {}: chains carry {} side groups per link, sequences branch", b.valence, b.valence - 2);
    }
    if u.letters.is_empty() {
        println!("no side groups: chains carry no sequence");
        return;
    }
    let name = |z: u8| u.get(z).name;
    println!("side-group letters: {}", u.letters.iter().map(|&z| name(z)).collect::<Vec<_>>().join(" "));
    for (&l, &p) in u.letters.iter().zip(&u.partner) {
        println!("  {} pairs with {}", name(l), name(p));
    }
    let h: Vec<&str> = u.heritable.iter().map(|&z| name(z)).collect();
    println!("heritable alphabet (letters that survive repeated double copies): {} ({} letters, {:.1} bits each)", h.join(" "), h.len(), (h.len() as f64).log2());
}

/// How reproducible the reef is: the same state run on with different dice, compared on what
/// the scale above reads (`Reef::summary`), averaged over noon samples. The ceiling any reef
/// surrogate is scored against.
fn reef_shadow(args: &[String]) {
    use primordium_core::reef::{Reef, ReefParams, ReefSeed};
    use primordium_core::surrogate::{RateTable, agreement};
    let warm: f64 = arg(args, "warm", 200.0);
    let days: f64 = arg(args, "days", 60.0);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let rs: ReefSeed = serde_json::from_str(&std::fs::read_to_string("data/reef_seed.json").expect("run reef-seed first")).unwrap();
    let start = Instant::now();
    let mut a = Reef::new(ReefParams::default(), CosmosParams::default(), rates, &rs, 1);
    while a.t_days < warm {
        a.step(0.02);
    }
    let mut b = a.clone();
    b.reseed(0xD1CE);
    let (mut sum, mut n) = (0.0, 0);
    let mut next = a.t_days.floor() + 1.5;
    let end = a.t_days + days;
    while a.t_days < end {
        a.step(0.02);
        b.step(0.02);
        if a.t_days >= next {
            next += 1.0;
            let g = agreement(&a.summary(), &b.summary());
            sum += g;
            n += 1;
            if n % 10 == 0 {
                println!("t={:6.1} twin agreement {:.1}%  a {:?}", a.t_days, g * 100.0, a.summary().iter().map(|v| v.round()).collect::<Vec<_>>());
            }
        }
    }
    println!("mean twin agreement {:.1}% over {n} noons ({warm}-day warm-up, {:.0}s)", sum / n.max(1) as f64 * 100.0, start.elapsed().as_secs_f64());
}

/// Calibrate the creature goal on an untouched reef: how big, how many jobs and how mobile the
/// best bodies get with nobody steering, and how long lineages hold the default goal.
fn reef_goal(args: &[String]) {
    use primordium_core::goal::{CreatureGoal, Tracker, jobs};
    use primordium_core::reef::{Reef, ReefParams, ReefSeed};
    use primordium_core::surrogate::RateTable;
    let days: f64 = arg(args, "days", 2000.0);
    let seed: u64 = arg(args, "seed", 1);
    let rates: RateTable = serde_json::from_str(&std::fs::read_to_string("data/rates0.json").expect("run train first")).unwrap();
    let rs: ReefSeed = serde_json::from_str(&std::fs::read_to_string("data/reef_seed.json").expect("run reef-seed first")).unwrap();
    let mut r = Reef::new(ReefParams::default(), CosmosParams { seed, ..Default::default() }, rates, &rs, seed);
    let goal = CreatureGoal::default();
    let mut tr = Tracker::default();
    let mut next = 99.5;
    while r.t_days < days {
        r.step(0.02);
        if r.t_days >= next {
            next += 100.0;
            tr.sample(&goal, &r);
            let multi: Vec<_> = r.organisms.iter().filter(|o| o.settled() && o.cells.len() > 1).collect();
            let big = |n: usize| multi.iter().filter(|o| o.cells.len() >= n).count();
            let jobs_of = |min: usize| multi.iter().filter(|o| o.cells.len() >= min).map(|o| jobs(o)).max().unwrap_or(0);
            let mob = |min: usize| multi.iter().filter(|o| o.cells.len() >= min && o.speed > 0.0).count();
            let ok = |n: usize, j: usize| multi.iter().filter(|o| o.cells.len() >= n && jobs(o) >= j && o.speed > 0.0).count();
            println!(
                "t={:5.0} bodies>=10 {:3} >=20 {:3} >=40 {:3} | max jobs (>=10) {} (>=20) {} | mobile >=10 {:3} >=20 {:3} | goal(20,2) {:3} (20,3) {:3} (10,3) {:3} | best run {:4.0} won {:?}",
                r.t_days, big(10), big(20), big(40), jobs_of(10), jobs_of(20), mob(10), mob(20), ok(20, 2), ok(20, 3), ok(10, 3), tr.best_run(r.t_days), tr.won
            );
        }
    }
}
