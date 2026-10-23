# Primordium

Multiscale artificial-life sim: invented chemistry up through the cell, each scale replaced by a learned behaviour function for the scale above. Rust core (`core/`), headless CLI (`cli/`), Godot 4.7 viewer (`godot/`) over a gdext bridge (`godot-ext/`). Personal fun project, not client work.

## Commits

**Standing permission: commit and push when you judge it a good time.** Overrides the workspace per-commit rule for this repo. Remote `origin` = github.com/MarcGodard/primordium, **public**: no secrets, no personal paths or client names in code, docs or commit bodies.

- Commit at coherent stopping points with the tree green: `cargo build`, `cargo test`, headless Godot load clean.

## Principles

- **Matter closed, energy flows through.** Atoms never created, destroyed or transmuted. Energy ledger is integer and exact: `stored + bath - photon_in == stored_at_start`. Any matter/energy path change carries a ledger test.
- **Environment is a pure function of time** (`cosmos.rs`). Every scale and the viewer sample the same drivers. Change drives evolution: keep drivers on incommensurate periods.
- **Every scale = detailed model + surrogate.** Surrogate returns a distribution plus confidence; low confidence falls back to detail and logs a discovery. Never ship a surrogate without a shadow-mode accuracy number.
- **Visuals derive from data.** No hand-made art per species/element; shaders read z, bonds, state.
- Must run on one local machine: prefer coarse lattices and surrogates to brute force.

## Running

- Tests: `cargo test`. Headless: `cargo run --release -p primordium -- chem --days=3`.
- Viewer: `cargo build --release -p primordium-godot` then `godot --path godot`. The `.gdextension` loads `target/release`.
- Captures (window opens briefly, exits itself): `godot --path godot --resolution 1600x960 -- --shot=../out/x.png --warm=500 --frames=15 [--zoom=Z --focus=X,Y]`. Output goes in `out/` (ignored). Starts in the hub; `--scale=K` opens view K first (0 elements .. 6 reef, before `--warm`), `--warp=K` plays the transition (frames of the warp only: content flags still act on the hub's first view), `--explain=1` opens the guide, `--pick=1` and `--lineage=1` are view-specific.

## Progress

`PROGRESS.md` is the source of truth for milestones done vs open. Update it when landing a milestone.
