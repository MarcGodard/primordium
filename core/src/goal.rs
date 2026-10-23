//! What counts as a creature: the campaign's end goal, read off the reef with no new physics.
//! Body = one organism; jobs = distinct on/off expression patterns held by a real share of it.
use crate::reef::{Organism, Reef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Trait counted "on" in a cell above this expression.
pub const ON: f32 = 0.5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatureGoal {
    pub min_cells: usize,
    pub min_jobs: usize,
    pub mobile: bool,
    /// Days the lineage must hold a qualifying body, hands-off.
    pub hold_days: f64,
}

impl Default for CreatureGoal {
    fn default() -> Self {
        Self { min_cells: 20, min_jobs: 3, mobile: true, hold_days: 500.0 }
    }
}

/// A cell's job: which of the five traits it has switched on, as bits.
pub fn job(expr: &[f32; 5]) -> u8 {
    expr.iter().enumerate().fold(0, |b, (k, &e)| if e > ON { b | 1 << k } else { b })
}

/// Distinct jobs in a body, each held by at least two cells and a tenth of the body: one odd
/// cell is noise, not a division of labour.
pub fn jobs(o: &Organism) -> usize {
    let mut by: BTreeMap<u8, usize> = BTreeMap::new();
    for c in &o.cells {
        *by.entry(job(&c.expr)).or_default() += 1;
    }
    let floor = (o.cells.len() / 10).max(2);
    by.values().filter(|&&n| n >= floor).count()
}

impl CreatureGoal {
    pub fn body_ok(&self, o: &Organism) -> bool {
        o.settled() && o.cells.len() >= self.min_cells && (!self.mobile || o.speed > 0.0) && jobs(o) >= self.min_jobs
    }
}

/// Follows which lineages hold a qualifying body and since when. A gap resets the clock.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tracker {
    /// clade -> day its current unbroken run began
    pub since: BTreeMap<u32, f64>,
    /// clade and day it first held for `hold_days`
    pub won: Option<(u32, f64)>,
}

impl Tracker {
    /// Sample the reef; returns lineages qualifying now.
    pub fn sample(&mut self, goal: &CreatureGoal, r: &Reef) -> BTreeSet<u32> {
        let now: BTreeSet<u32> = r.organisms.iter().filter(|o| goal.body_ok(o)).map(|o| o.clade).collect();
        self.since.retain(|c, _| now.contains(c));
        for &c in &now {
            let t0 = *self.since.entry(c).or_insert(r.t_days);
            if self.won.is_none() && r.t_days - t0 >= goal.hold_days {
                self.won = Some((c, r.t_days));
            }
        }
        now
    }

    /// Longest current run, days.
    pub fn best_run(&self, t_days: f64) -> f64 {
        self.since.values().map(|t0| t_days - t0).fold(0.0, f64::max)
    }
}
