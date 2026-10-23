# Progress

## Milestones

- [x] **M1 Scale 0 chemistry + environment + viewer.** Shell-rule elements, cosmos drivers, lattice chemistry with exact matter/energy ledger, Godot beach view with atom-anatomy zoom, orrery, census, inspect.
- [x] **M2 Surrogate framework + scale-0 surrogate.** Beach cut into 8 elevation bands; each runs lattice or mass-action counts (`hybrid.rs`, `kinetics.rs`). Rates learned per environment bin (`surrogate.rs` RateTable) from a year of detailed tallies (`primordium train` -> `data/rates0.json`). Confidence from bin coverage; low confidence lifts the band back to atoms and logs a discovery. Viewer: surrogate bands show their bond-type histogram; zooming in lifts visible bands to real atoms. Ledger exact through restrict/step/lift.
- [x] **M3 Scale 1 polymers.** Ka-backbone chains with 5 side-group letters (`polymer.rs`): lattice folding scored with scale-0 energies, function read off the fold ensemble (pocket catalysis per scale-0 bond pair, temperature-dependent stability, amphiphile, pigment). Tide pool (`pool.rs`) runs on learned scale-0 Ka-Ka rates: templated copying in the dry with physics-derived fidelity, melting in the wet, hydrolysis, ligation. Function surrogate = cache + Hamming nearest neighbour with confidence. Viewer scale 1 (Tab): pool of folds, close-up with pockets, click residue for its atoms.
- [ ] **M4 Scale 2 metabolism.** Amphiphile vesicles from hydropathy; reaction networks inside; growth/split on wet-dry cycles; surrogate env -> fluxes.
- [ ] **M5 Scale 3 cell.** Genome polymer read by evolved machinery, inheritance with copy errors, division, selection under the cosmos cycles. Godot zoom from cell down to atoms.

## Log

- 2026-10-23 M3: single folds are dice for len >= 16 (same sequence agrees with itself ~37%); function as an 8-fold ensemble mean lifts the ceiling to ~70% and 1-mutant neighbours predict at ~88-96% of it. Copy error ~25-29% per letter: heredity only for short motifs until catalysts improve fidelity. Backbone catalysis is pool-wide, so no selection for it yet: that needs compartments (M4).
- 2026-10-23 M2 shadow mode (`primordium shadow`): surrogate agrees 86-88% with detail on per-band per-pair bond counts, while two detailed runs with different dice agree 87-89%: **98.8-98.9% of the predictable signal**, 7-10x faster. Near equilibrium a frozen-state guess scores similarly; the surrogate earns its keep when conditions change.
- 2026-10-23 Review fixed two detailed-balance bugs (dry gate one-directional; weak pairs destroyed without reverse).
- 2026-10-23 M1 landed. 8.1k atoms at ~2.2k sweeps/s release. Ledger drift 0 over runs.
