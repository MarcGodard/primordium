# Progress

## Milestones

- [x] **M1 Scale 0 chemistry + environment + viewer.** Shell-rule elements, cosmos drivers, lattice chemistry with exact matter/energy ledger, Godot beach view with atom-anatomy zoom, orrery, census, inspect.
- [x] **M2 Surrogate framework + scale-0 surrogate.** Beach cut into 8 elevation bands; each runs lattice or mass-action counts (`hybrid.rs`, `kinetics.rs`). Rates learned per environment bin (`surrogate.rs` RateTable) from a year of detailed tallies (`primordium train` -> `data/rates0.json`). Confidence from bin coverage; low confidence lifts the band back to atoms and logs a discovery. Viewer: surrogate bands show their bond-type histogram; zooming in lifts visible bands to real atoms. Ledger exact through restrict/step/lift.
- [ ] **M3 Scale 1 polymers.** Backbone chains with side-group sequences; folding rule -> function (catalysis of specific reactions); template copying via weak pairing; surrogate sequence -> function.
- [ ] **M4 Scale 2 metabolism.** Amphiphile vesicles from hydropathy; reaction networks inside; growth/split on wet-dry cycles; surrogate env -> fluxes.
- [ ] **M5 Scale 3 cell.** Genome polymer read by evolved machinery, inheritance with copy errors, division, selection under the cosmos cycles. Godot zoom from cell down to atoms.

## Log

- 2026-10-23 M2 shadow mode (`primordium shadow`): surrogate agrees 86-88% with detail on per-band per-pair bond counts, while two detailed runs with different dice agree 87-89%: **98.8-98.9% of the predictable signal**, 7-10x faster. Near equilibrium a frozen-state guess scores similarly; the surrogate earns its keep when conditions change.
- 2026-10-23 Review fixed two detailed-balance bugs (dry gate one-directional; weak pairs destroyed without reverse).
- 2026-10-23 M1 landed. 8.1k atoms at ~2.2k sweeps/s release. Ledger drift 0 over runs.
