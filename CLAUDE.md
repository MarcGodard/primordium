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
- Captures (window opens briefly, exits itself): `godot --path godot --resolution 1600x960 -- --shot=../out/x.png --warm=500 --frames=15 [--zoom=Z --focus=X,Y]`. Output goes in `out/` (ignored). Starts in the hub; `--scale=K` opens view K first (0 elements .. 6 reef, 7 archipelago, 8 your world; before `--warm`), `--warp=K` plays the transition (frames of the warp only: content flags still act on the hub's first view), `--explain=1` opens the guide, `--codex=1` the codex, `--explore=1` opens every world, `--pick=1`, `--keep=1` (reef: keep the picked body), `--run=N` (reef: restart as run N), `--hunt=STEPS` (reef: run until a creature exists and select it), `--ff=DAYS` (reef: fast-forward and show the report), `--levers=1` or `--levers=waves:0.25` (garden: lever panel), `--welcome=1` and `--graduate=K` (show those cards), `--expedition=1` (reef: today's expedition), `--skins=1` (reef: skins on every body), `--traits=1` (reef: trait chart), `--orbit=YAW,PITCH` (3D camera, radians), `--island=K` (archipelago: show island K) and `--lineage=1` are view-specific. Flags act in order, so `--scale=6 --warm=N --pick=1 --keep=1 --scale=8 --warm=M` tests the garden. Captures never write the campaign save.

## Other periodic tables

- `--features shells-266` (on `primordium`, `primordium-godot`) builds the 14-element world. Use its own target dir (`CARGO_TARGET_DIR=target/t266`) so it never overwrites the default build the viewer loads. Its data lives in `data/shells-2-6-6/` (`element::data_dir`); tests that pin [2, 6] values and `core/tests/golden.rs` run on the default build only.

## Progress

`PROGRESS.md` is the source of truth for milestones done vs open. Update it when landing a milestone.
