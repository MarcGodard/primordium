# Running the chain on a bigger periodic table

Plan for scales 1+ on a table other than `[2, 6]`. Chemistry (scale 0) already runs on any table; everything from polymers up assumes 5 side-group letters and the Li/Fl heritable pair (`polymer.rs` asserts `N_LETTERS == 5`).

## Target

`[2, 6, 6]`: backbone Ka (valence 3), rival Ti, heritable alphabet Fl/Sa (2 letters). `[2, 8]` makes the backbone tetravalent (Ox), so chains branch: out until there is a branched polymer model.

## Approach

Compile-time first: a cargo feature picks `SHELLS`, and every hard-coded rule becomes a function of `Universe`, so the default table stays bit-for-bit identical. A runtime choice (player picks a universe in the game) needs ~280 `N_LETTERS`/`N_ELEMENTS` uses turned into vectors or max-size arrays (roughly 1500-2500 lines, risk to determinism); doing the rules first means that step later only changes storage.

## Rules to generalise

- **Letters** (`element.rs` `side_groups`, `Universe::new`): atoms with `0 < valence < backbone valence`. A rival (same valence as the backbone) would branch chains, so it is not a letter. No change for `[2, 6]`.
- **Genetic code** (`cell.rs` `CODE`): N letters, H heritable letters. Codon width `w` = smallest with `H^w >= N + 1`; codons `0..N` map to letters; of the rest, the last `max(1, H^w / 4)` are stops and the others are synonyms assigned from the food letter on. For `[2, 6]` this gives today's `[0,1,2,3,4,1,None,None]` exactly; for `[2, 6, 6]` w = 4 (16 codons, 4 stops). Built at run time next to `comp`/`bits`, because `bond_energies` is not const.
- **Heritable bits** (`cell.rs` `bit_table`, `codon`): canonical letters are the fixed points of copying twice (`comp[comp[x]] == x`), bit = rank. Codons read `w` base-H digits. Assert H == 2 at first.
- **Food letter** (`const LI` in `lineage.rs`, `reef.rs`; `free[1]` and founder genomes in `cell.rs`): the bit-0 canonical letter, also the synonym target, which is why it is the scarce one today. `canon: [u8; 2]` becomes `[u8; H]`.
- **polymer.rs**: drop the assert; residue characters from `LETTER_NAMES` initials; Hy caps by valence.
- **chem.rs**: `MAX_BONDS` from the largest valence; starting shares and formula order by role instead of by Z.
- **Literal indices** in tests, the CLI (`pair(3,3)`, the "Hy Li Bo Ox Fl" label), godot-ext (`pond_sim`, `pool_sim`) and scripts (`fold_inset.gd`, `pond_view.gd`, `atom.gdshader` palette size).

## Data per table

`rates0.json` (258 channels today, 1463 for `[2, 6, 6]`) and `reef_seed.json` record no table: add a `shells` field (serde default) and refuse a mismatch. Keep `data/` for `[2, 6]` and `data/shells-2-6-6/` for others behind one `data_dir()` helper, used by the CLI and exposed to Godot. Regenerate in order: `train` (more days until coverage matches), the shadow checks, `reef-seed`, `reef`.

## Before refactoring

No test pins a trajectory bit for bit. Add golden hashes of world state after short seeded runs (train, cells, coast, reef) and check that regenerated `rates0.json` and `reef_seed.json` match byte for byte. Existing guards: element tests (heritable `[3, 7]`), the exact code/translation test in `cell.rs`, copy error rate, fold and cosmos determinism, and every matter/energy ledger test.

## Cost in play

Genes get 4/3 longer with 4-digit codons, so the error threshold is harder to cross; the scale-0 rate table is 5.7x bigger, so training needs more days for the same coverage.
