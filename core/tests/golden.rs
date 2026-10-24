//! Golden trajectories for the default table: short seeded runs of every scale, fingerprinted.
//! Guards refactors that must not change behaviour (docs/bigger-table.md: the [2, 6] world stays
//! bit for bit). A change that is meant to alter trajectories updates the pins on purpose:
//! `GOLDEN_PRINT=1 cargo test --release --test golden -- --nocapture` prints the new values.
// The pins are trajectories of the [2, 6] world; another table has its own (none yet).
#![cfg(not(any(feature = "shells-266", feature = "shells-2666", feature = "shells-288")))]
use primordium_core::cell::{CellParams, World};
use primordium_core::chem::{ChemParams, ChemWorld};
use primordium_core::coast::{Coast, CoastParams};
use primordium_core::cosmos::{Cosmos, CosmosParams};
use primordium_core::pool::{Pool, PoolParams};
use primordium_core::protocell::{Pond, PondParams};
use primordium_core::reef::{Reef, ReefParams, ReefSeed};
use primordium_core::surrogate::RateTable;

/// FNV-1a over a value's Debug text: stable across runs, changes with any field.
fn fp(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn check(name: &str, got: u64, want: u64) {
    if std::env::var("GOLDEN_PRINT").is_ok() {
        println!("{name}: {got:#018x}");
    } else {
        assert_eq!(got, want, "{name} trajectory changed (GOLDEN_PRINT=1 to see new pins)");
    }
}

fn rates() -> RateTable {
    let dir = format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), primordium_core::element::data_dir());
    serde_json::from_str(&std::fs::read_to_string(format!("{dir}rates0.json")).unwrap()).unwrap()
}

#[test]
fn chem_golden() {
    let mut w = ChemWorld::new(ChemParams::default(), CosmosParams { seed: 1, ..Default::default() }, 1);
    for _ in 0..60 {
        w.sweep();
    }
    check("chem", fp(&format!("{:?}", w.atoms)), 0x2e4d54eb3e82dfd2);
}

#[test]
fn pool_golden() {
    let mut p = Pool::new(PoolParams::default(), Cosmos::new(CosmosParams::default()), rates(), 1);
    for _ in 0..150 {
        p.step(0.02);
    }
    check("pool", fp(&format!("{:?}", p.c.chains)), 0x3e5b38f1d8bdc738);
}

#[test]
fn pond_golden() {
    let mut p = Pond::new(PoolParams::default(), PondParams::default(), Cosmos::new(CosmosParams::default()), rates(), 1);
    for _ in 0..80 {
        p.step(0.02);
    }
    check("pond", fp(&format!("{:?}", p.vesicles)), 0xa68ae1ad6083937d);
}

#[test]
fn cell_golden() {
    let mut w = World::new(PoolParams::default(), CellParams::default(), Cosmos::new(CosmosParams::default()), rates(), 1, 40);
    for _ in 0..80 {
        w.step(0.02);
    }
    check("cell", fp(&format!("{:?}", w.cells)), 0x64c129c1e9766f14);
}

#[test]
fn coast_golden() {
    let cp = CoastParams { sites: 3, cells_per_site: 60, founders_per_site: 15, ..Default::default() };
    let mut c = Coast::new(cp, CosmosParams::default(), rates(), 1);
    for _ in 0..60 {
        c.step(0.02);
    }
    let cells: Vec<String> = c.sites.iter().map(|s| format!("{:?}", s.world.cells)).collect();
    check("coast", fp(&cells.join("|")), 0x667d2cd46c26274d);
}

#[test]
fn reef_golden() {
    let dir = format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), primordium_core::element::data_dir());
    let seed: ReefSeed = serde_json::from_str(&std::fs::read_to_string(format!("{dir}reef_seed.json")).unwrap()).unwrap();
    let rp = ReefParams { width: 10, depth: 10, founders: 60, ..Default::default() };
    let mut r = Reef::new(rp, CosmosParams::default(), rates(), &seed, 2);
    for _ in 0..1500 {
        r.step(0.02);
    }
    let bodies: Vec<String> = r.organisms.iter().map(|o| format!("{} {:?} {:?}", o.id, o.genome, o.cells.iter().map(|c| (c.pos, c.mass)).collect::<Vec<_>>())).collect();
    check("reef", fp(&bodies.join("|")), 0xcfece02aa680d3ad);
}
