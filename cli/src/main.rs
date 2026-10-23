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
