# Primordium

A multiscale artificial-life simulator. Instead of simulating every atom of an organism, each scale of life gets two models:

1. a **detailed model** built from the scale below, and
2. a **surrogate**: a learned behaviour function that reproduces what the scale above needs, with a confidence score.

The scale above runs on surrogates. When evolution pushes something outside what a surrogate has seen, that piece drops back to the detailed model, the event is logged as a *discovery*, and the surrogate retrains. Zooming in the viewer uses the same switch, so zoom is real simulation on demand, not a picture.

The chemistry is invented, not ours: elements come from a shell rule with capacities `[2, 6]` instead of `[2, 8]`, so the best backbone atom is trivalent and chains carry one side group per link. Life here should look life-like and evolve differently.

## Scales

| Scale | Detailed model | Status |
|---|---|---|
| -1 Subatomic | Shell rule generates the periodic table | done (rule + viewer zoom) |
| 0 Chemistry | Lattice Monte-Carlo: bonds, exchange, weak pairing, photons, hydration | done |
| 1 Polymers | Sequence-defined chains, lattice folding, pocket catalysis, templated copying | done |
| 2 Protocells | Amphiphile vesicles with interior metabolism, growth, division | done |
| 3 Cells | Binary genome, genetic code, regulated expression, gated division, proofreading, radiation mutation | done |
| 4 Coast | Beaches along a latitude gradient, linked by along-shore currents; lineage surrogate | done (surrogate 96-98%, 2.3x faster) |
| 3-4 Colonies | Adhesive products keep daughters attached: anchored, monomer-sharing, shade-sharing colonies | done (first step to multicellularity) |
| 3-4 Ecology | Sticky cells engulf smaller ones; colonies resist | done (predation drives multicellularity) |
| 5 Reef | Multicellular bodies in 3D on a rocky shore: each cell a scale-4 lineage cell, its genes switched by its height and exposure; light through water and canopy, food through open faces, waves that tear, grazing, spores and fragments | detail done, 3D viewer; organism surrogate next |

Each scale's surrogate is measured against a "twin": the same detailed model with different dice. Agreement as a share of that ceiling (latest runs): scale 0 ~99% (10-day mean over 6 seeds), scale 1 95-104% (blend of cached relatives), scale 2 ~99-100%, scale 3 ~100%, scale 4 ~95% with engulfing (lineage mode, mean of five windows, 2.2x faster). A mean-field surrogate can sit at or above the ceiling because it carries less noise than a second detailed run.

## Findings so far

- A single fold of a long chain is mostly chance; function is only a property of a sequence as a fold-ensemble average, which makes reliable folding itself selectable.
- Eigen's error threshold appears unprompted: catalysts cannot be inherited until a polymerase pocket raises copy fidelity.
- Compartments alone do not select catalysts; division gated on genome copying does.
- The pairing chemistry forces a binary heritable alphabet, so cells need a genetic code to reach the 5-letter working alphabet.
- Proofreading that needs the polymerase pocket (clipping is the pocket's reverse reaction) makes polymerase quality the strongest target of selection.
- Copy errors come mostly from a scarce monomer: less crowded beaches copy 2-4x more faithfully, and cold polar beaches most of all.
- Over 2000 days the coast crosses the error threshold, and polymerases specialise by climate: they strengthen at the warm equator and wither at the cold poles, where cold alone keeps copying accurate.

- On the reef, bodies grow from single coast cells into mounds and columns of up to a few hundred cells within 200 days; shape is heritable because cells that express membrane builders divide upward.

## Growing the world

Everything is sized to run on one machine, and the knobs that make it bigger are parameters, not rewrites: reef size and cell cap (`ReefParams`), coast sites and cells per site (`CoastParams`), lattice size (`ChemParams`), and the fold ensemble size (`polymer::ENSEMBLE`). Each scale above chemistry runs on learned tables from the scale below, so a bigger world costs table lookups, not more chemistry. The periodic table falls out of a shell rule (`element.rs`); growing it past 8 elements is a planned step.

## Environment: change drives evolution

`core/src/cosmos.rs` gives every scale the same drivers as a pure function of time: day/night, an eccentric orbit with axial tilt (seasons), two moons plus solar tide (spring and neap tides on a beach slope, so wet/dry cycles), clouds, a star activity cycle with flares, a pulsar and a binary in radio, galactic cosmic rays, and rare supernovae.

## Run

```sh
cargo test
cargo run --release -p primordium -- chem --days=3
cargo build --release -p primordium-godot
godot --path godot          # Godot 4.7
```

Viewer: Tab cycles scales (chemistry, polymers, protocells, cells, coast, reef). The reef is 3D: right-drag orbits, shift- or middle-drag pans, left-click picks a body (its make-up by height and its genes' switches appear on the left), B jumps to the biggest body, O toggles the slow auto-orbit. Mouse wheel zooms, right-drag pans, left-click inspects and drills down (cell -> gene -> fold -> residue atoms), space pauses, 1-5 sets speed, A toggles automatic resolution.

Headless experiments: `train`, `shadow`, `pool`, `fold-shadow`, `pond`, `pond-shadow`, `selection`, `cells`, `cell-shadow`, `cell-selection`, `coast`, `coast-shadow`, `reef-seed` (evolve a coast and hand its lineage model and cells to the reef: `data/reef_seed.json`), `reef` (see `cli/src/main.rs`).
