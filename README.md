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
| 1 Polymers | Sequence-defined chains, folding, catalysis, templating | planned |
| 2 Metabolism | Reaction networks inside amphiphile compartments | planned |
| 3 Cell | Genome polymer, translation, membrane, division | planned |

## Environment: change drives evolution

`core/src/cosmos.rs` gives every scale the same drivers as a pure function of time: day/night, an eccentric orbit with axial tilt (seasons), two moons plus solar tide (spring and neap tides on a beach slope, so wet/dry cycles), clouds, a star activity cycle with flares, a pulsar and a binary in radio, galactic cosmic rays, and rare supernovae.

## Run

```sh
cargo test
cargo run --release -p primordium -- chem --days=3
cargo build --release -p primordium-godot
godot --path godot          # Godot 4.7
```

Viewer: mouse wheel zooms (zoom far in to see nuclei, shells and electrons), right-drag pans, left-click inspects an atom, space pauses, 1-5 sets speed, R reseeds.
