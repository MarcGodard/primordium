//! What counts as a creature: the campaign's end goal, read off the reef with no new physics.
//! Body = one organism; jobs = groups of its cells that express traits differently.
use crate::reef::{Organism, Reef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatureGoal {
    pub min_cells: usize,
    pub min_jobs: usize,
    /// In-body range a trait needs before it splits cells into jobs.
    pub split: f32,
    pub mobile: bool,
    /// Breeding true: a lineage with a qualifying body on `need_days` of the last `window_days`.
    /// Qualifying bodies come and go (tears, grazing, growth), so the test is how often, not how
    /// long. Unaided reefs' best lineage mostly sits at 1-14 of 100 but one seed hit 40 once;
    /// gardens of kept creatures peak at 13-23 and fall back. Counted only in the garden.
    pub window_days: f64,
    pub need_days: usize,
}

impl Default for CreatureGoal {
    fn default() -> Self {
        Self { min_cells: 20, min_jobs: 3, split: 0.15, mobile: true, window_days: 100.0, need_days: 15 }
    }
}

/// Distinct jobs in a body. Trait levels have no absolute "on" (some never pass 0.5), so jobs are
/// relative: a trait whose in-body range reaches `split` cuts the body at its midpoint, a cell's
/// job is its side of every cut. Each job needs two cells and a tenth of the body: one odd cell
/// is noise, not a division of labour.
pub fn jobs(o: &Organism, split: f32) -> usize {
    let mut cuts = Vec::new();
    for k in 0..5 {
        let (lo, hi) = o.cells.iter().fold((f32::MAX, f32::MIN), |a, c| (a.0.min(c.expr[k]), a.1.max(c.expr[k])));
        if hi - lo >= split {
            cuts.push((k, (lo + hi) / 2.0));
        }
    }
    let mut by: BTreeMap<u8, usize> = BTreeMap::new();
    for c in &o.cells {
        let job = cuts.iter().enumerate().fold(0u8, |b, (i, &(k, mid))| if c.expr[k] > mid { b | 1 << i } else { b });
        *by.entry(job).or_default() += 1;
    }
    let floor = (o.cells.len() / 10).max(2);
    by.values().filter(|&&n| n >= floor).count()
}

impl CreatureGoal {
    pub fn body_ok(&self, o: &Organism) -> bool {
        o.settled() && o.cells.len() >= self.min_cells && (!self.mobile || o.speed > 0.0) && jobs(o, self.split) >= self.min_jobs
    }
}

/// Days each lineage held a qualifying body, over the rolling window. Sample once a day at noon
/// (light-switched genes are off at night).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tracker {
    pub days: BTreeMap<u32, VecDeque<f64>>,
    /// Lineage and day it first bred true.
    pub won: Option<(u32, f64)>,
}

impl Tracker {
    /// Sample the reef; returns lineages qualifying now.
    pub fn sample(&mut self, goal: &CreatureGoal, r: &Reef) -> BTreeSet<u32> {
        let now: BTreeSet<u32> = r.organisms.iter().filter(|o| goal.body_ok(o)).map(|o| o.clade).collect();
        self.record(goal, r.t_days, &now);
        now
    }

    /// Log day `t` for the lineages qualifying now, drop days older than the window.
    pub fn record(&mut self, goal: &CreatureGoal, t: f64, now: &BTreeSet<u32>) {
        for &c in now {
            self.days.entry(c).or_default().push_back(t);
        }
        for d in self.days.values_mut() {
            while d.front().is_some_and(|&t0| t - t0 >= goal.window_days) {
                d.pop_front();
            }
        }
        self.days.retain(|_, d| !d.is_empty());
        if self.won.is_none() {
            if let Some((c, _)) = self.best().filter(|b| b.1 >= goal.need_days) {
                self.won = Some((c, t));
            }
        }
    }

    /// Lineage with the most qualifying days in the window, and how many.
    pub fn best(&self) -> Option<(u32, usize)> {
        self.days.iter().map(|(&c, d)| (c, d.len())).max_by_key(|b| b.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window forgets: a lineage needs `need_days` inside one window, not in total.
    #[test]
    fn window_counts_recent_days_only() {
        let g = CreatureGoal { window_days: 10.0, need_days: 3, ..Default::default() };
        let mut tr = Tracker::default();
        let seven = BTreeSet::from([7]);
        for t in [0.5, 12.5, 13.5] {
            tr.record(&g, t, &seven);
        }
        assert_eq!(tr.best(), Some((7, 2)), "day 0.5 should have left the window");
        assert!(tr.won.is_none());
        tr.record(&g, 14.5, &seven);
        assert_eq!(tr.won, Some((7, 14.5)));
        // a lineage that stops qualifying drops out once its days age past the window
        tr.record(&g, 30.0, &BTreeSet::new());
        assert_eq!(tr.best(), None);
    }
}
