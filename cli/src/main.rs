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
