# Primordium

A multiscale artificial-life simulator. Instead of simulating every atom of an organism, each scale of life gets two models:

1. a **detailed model** built from the scale below, and
2. a **surrogate**: a learned behaviour function that reproduces what the scale above needs, with a confidence score.

The scale above runs on surrogates. When evolution pushes something outside what a surrogate has seen, that piece drops back to the detailed model, the event is logged as a *discovery*, and the surrogate retrains. Zooming in the viewer uses the same switch, so zoom is real simulation on demand, not a picture.

The chemistry is invented, not ours: elements come from a shell rule with capacities `[2, 6]` instead of `[2, 8]`, so the best backbone atom is trivalent and chains carry one side group per link. Life here should look life-like and evolve differently.

## Scales

| Scale | Detailed model | Status |
|---|---|---|
| -1 Subatomic | Shell rule generates the periodic table | done (rule, 3D periodic table screen, universe previews) |
| 0 Chemistry | Lattice Monte-Carlo: bonds, exchange, weak pairing, photons, hydration | done |
| 1 Polymers | Sequence-defined chains, lattice folding, pocket catalysis, templated copying | done |
| 2 Protocells | Amphiphile vesicles with interior metabolism, growth, division | done |
| 3 Cells | Binary genome, genetic code, regulated expression, gated division, proofreading, radiation mutation | done |
| 4 Coast | Beaches along a latitude gradient, linked by along-shore currents; lineage surrogate | done (surrogate 96-98%, 2.3x faster) |
| 3-4 Colonies | Adhesive products keep daughters attached: anchored, monomer-sharing, shade-sharing colonies | done (first step to multicellularity) |
| 3-4 Ecology | Sticky cells engulf smaller ones; colonies resist | done (predation drives multicellularity) |
| 5 Reef | Multicellular bodies in 3D on a rocky shore: each cell a scale-4 lineage cell, its genes switched by its height and exposure; light through water and canopy, food through open faces, waves that tear, grazing, spores and fragments | detail done, 3D viewer; organism surrogate next |
| Your world | An empty reef the player stocks with creatures kept from reef runs | done (saved between sessions) |

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

Everything is sized to run on one machine, and the knobs that make it bigger are parameters, not rewrites: reef size and cell cap (`ReefParams`), coast sites and cells per site (`CoastParams`), lattice size (`ChemParams`), the fold ensemble size (`polymer::ENSEMBLE`). Each scale above chemistry runs on learned tables from the scale below, so a bigger world costs table lookups, not more chemistry. The planet is Earth-sized (`CosmosParams::planet_radius_km`), so distances between coast sites are real.

The periodic table is one line: `element::SHELLS`. Element count, names, bond energies, which elements bond, the backbone atom and the side-group letters are all derived from it at compile time. Preview a bigger table before switching:

```sh
cargo run --release -p primordium -- elements --shells=2,6,6
```

What a third shell of 6 would do (from the explorer): 14 elements, Ka keeps the backbone (Ti is a trivalent rival), 10 side-group letters, and the heritable alphabet that survives copying becomes Fl/Sa instead of Li/Fl. With `[2, 8, 8]` (our universe's octets) the backbone turns tetravalent and chains branch. Chemistry (scale 0) runs on any table; scales 1+ are written for 5 letters and a binary genome (genetic code `cell::CODE`, the Li/Fl roles), and a compile-time check in `polymer.rs` says so. Generalising those is the next step to a bigger table (plan: `docs/bigger-table.md`); after any change, retrain `data/rates0.json` (`primordium train`) and `data/reef_seed.json` (`primordium reef-seed`).

## Play

The viewer opens on a hub: the planet with each scale orbiting it as a small world, plus your own world. It is a campaign: each scale has a goal measured by its own simulation (build a 12-atom molecule, a chain that copies itself, a bubble that divides, ...), and reaching it opens the next world and writes a codex page (J). At the reef the goal is to find a creature, a crawling body of 20+ cells whose cells do 3+ different jobs (gold rings mark them, G jumps to one) and keep it (K). Kept creatures are released into your world, an empty reef where they compete and keep evolving, saved between sessions; N starts a new reef run to find more. The last goal is a creature that breeds true: one lineage in your world producing creatures on 15 of the last 100 days. T fast-forwards 100 days (Shift+T 500) and reports what evolution did in words: which lineages rose and fell, what the bodies now do differently and why. Your world keeps evolving while the game is closed and tells you what happened when you come back. Each day brings an expedition (Shift+N at the reef): a reef seeded by the date, the same for every player that day. Kept creatures get species names (genus by lineage, so relatives share one) and live in the codex gallery as 3D bodies with smooth skins. E in the hub opens every world for free exploring.

## Environment: change drives evolution

`core/src/cosmos.rs` gives every scale the same drivers as a pure function of time: day/night, an eccentric orbit with axial tilt (seasons), two moons plus solar tide (spring and neap tides on a beach slope, so wet/dry cycles), clouds, a star activity cycle with flares, a pulsar and a binary in radio, galactic cosmic rays, and rare supernovae.

## Run

```sh
cargo test
cargo run --release -p primordium -- chem --days=3
cargo build --release -p primordium-godot
godot --path godot          # Godot 4.7
```

Viewer: it opens on a **hub**: the planet in space with the seven scales orbiting it as small worlds. Hover one for what it is, click (or press 1-7) to dive in through a warp. In any scale, the bar along the top jumps to any other scale, Tab / Shift+Tab climb up and down one scale, M returns to the hub, and **H** (or F1) opens a guide: what you are looking at, what the colours mean, and the real-world idea behind it.

The first scale is this world's periodic table in 3D: click an element for its atom (nucleus, electron shells, the empty slots that make it bond), its role in life here and its bond strengths with every other element; the buttons preview other universes (a third shell, our octets) and what their life would be built from. 2D scales: mouse wheel zooms, right-drag pans, left-click inspects and drills down (cell -> gene -> fold -> residue atoms), space pauses, 1-5 sets speed, A toggles automatic resolution. V on the cell scale switches its beach to 3D. The reef is 3D: right-drag orbits, shift- or middle-drag pans, left-click picks a body (its make-up by height and its genes' switches appear on the left), B jumps to the biggest body, C colours by lineage, O toggles the slow auto-orbit.

Headless experiments: `train`, `shadow`, `pool`, `fold-shadow`, `pond`, `pond-shadow`, `selection`, `cells`, `cell-shadow`, `cell-selection`, `coast`, `coast-shadow`, `reef-seed` (evolve a coast and hand its lineage model and cells to the reef: `data/reef_seed.json`), `reef` (see `cli/src/main.rs`).
