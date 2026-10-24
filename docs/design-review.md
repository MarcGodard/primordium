# Design review: from a tour of scales to a game of consequences

2026-10-23. Written after a play session. The player changed the element mix, rebuilt the scales above and could not tell whether anything changed; found the archipelago small; and found most goals were "search and wait".

## What the game is today

Nine worlds, each a real model, each with a goal that opens the next:

| Scale | Goal | What the player decides | Can it fail? |
|---|---|---|---|
| Elements | inspect three roles, try another table | the recipe (abundances) | no |
| Chemistry | a 12-atom molecule | the sky (orbit, tilt, tides, flares) | no |
| Polymers | a copy lineage 5 deep | pool height, size | no |
| Protocells | a lineage 4 divisions deep | shear, UV | no |
| Cells | 20 generations | mutation, UV wear | no |
| Coast | 30 cells carried between beaches | current | no |
| Reef | find and keep a creature | none (search) | no |
| Archipelago | a living web on every island | currents | no |
| Your world | a lineage that breeds true | levers, which creatures to keep | no |

The models are honest: matter is exact, the sky is shared, the recipe and sky reach every scale. But the game around them has three gaps.

### 1. Choices change the environment, never the lineage

The recipe and the sky set each scale's conditions (atom mix, sea per letter, temperature, UV, tides). What lives there does not come from below:

- cells and the coast start from random genomes built fresh from the genetic code;
- the reef and the archipelago start from `data/reef_seed.json`, genomes from one precomputed coast run on the default world.

So nothing the player evolved is carried up. A thin sea makes the reef poorer (measured: Li x0.2 holds a third of the cells), but the founders are the same as in every other world, and a good or bad run at the cells scale changes nothing above it. This is why changing the recipe "did not seem to matter": the effect is real but only environmental, and the game never shows it as a comparison.

### 2. Goals are thresholds you wait for

Every goal is a count that rises on its own. Settings exist on most scales but no goal needs them: the default always wins if you wait, and the reef goal is now a search key. There is no time budget, no way to lose, and so no reason to tune or to go back.

### 3. The worlds are small and the pacing strobes

- The archipelago holds about 7 x 1000 cells: islands, not a world.
- At the small scales a whole day passes in under a second, so light, tide and colour flash. Correct, but tiring.

## The core loop to build

> Each scale inherits what the player produced below, has a few real settings to tune, has a goal with a time budget and a way to fail, and passes up a result whose quality depends on the tuning. A poor result makes the next scale hard or impossible: go back a scale or two and do better.

### Carry-up: the player's own material moves up

| From | To | What carries | Model hook |
|---|---|---|---|
| Chemistry | Polymers | the monomers chemistry actually built, per letter (not the raw recipe) | pool `monomers` / `raw` from the chem census |
| Polymers | Protocells | the best copier and catalyst sequences | pond seeds its chains from them |
| Protocells | Cells | the genomes inside the winning vesicles, read through the genetic code | cell founders from those sequences instead of random genes |
| Cells | Coast | the surviving cell genomes | coast founders per site |
| Coast | Reef, archipelago | the coast's survivors (with the learned lineage table) | `ReefSeed.founders` from the player's coast run, not the shipped file |
| Reef | Archipelago, your world | kept creatures (already) | as now |

Each hand-off is saved with the campaign, so going back and redoing a scale replaces what it passes up.

### Stakes: a budget and a way to fail at every scale

| Scale | Tune | Goal (with a day budget) | Fails when |
|---|---|---|---|
| Chemistry | sky, temperature | enough monomers of every letter life needs | too hot: bonds break; too cold or dark: nothing forms |
| Polymers | pool height, size, UV | a copier whose error rate is under the error threshold for its length | error catastrophe: the lineage melts back into monomers |
| Protocells | shear, UV, membrane letters | a vesicle line that divides and keeps its copier | bubbles burst faster than they grow |
| Cells | mutation, UV wear | a population past N generations with working genes | extinction, or genes lost to mutation |
| Coast | current | lineages living on most climates | climates left empty |
| Reef | waves, grazers, food | a creature (now with settings that change how often they arise) | founders too weak to make bodies |
| Archipelago | currents, which islands to seed | a living web held on every island | an island's web collapses past recovery |

A failure shows a card that names the cause and the scale to revisit ("your cells' genomes copy with 30% error: a better copier comes from Polymers").

### Pacing and visuals

- Smooth the strobe: past about half a day per second, draw light and tide as their running average, so the scene breathes instead of flashing. The model still runs the real cycle.
- A time curve across scales: chemistry and polymers run fast (minutes of sim time per real second are seconds of chemistry), slowing towards real-life days at the reef; after your world, speed up again (and, later, more planets).

### A world, not a few islands

The project's own rule applies: a scale above runs as the learned behaviour of the one below. Islands in view run in detail; the rest run as lineage-level surrogates (counts per lineage and kind, learned from detailed islands, falling back to detail when unsure), like coast sites already do. That allows tens of islands and far more creatures at the same cost.

### Replayability

- A run summary per campaign: recipe, sky, how far it got, where it failed, its best creature. Compare runs.
- Hard recipes as challenges (a hydrogen world, a world short of the food letter), and the 14-element table as a new-game-plus.
- The daily expedition already gives a shared seed; extend it to a daily recipe.

## Order of work

1. Strobe smoothing (small, visible now).
2. Carry-up coast to reef and archipelago: the player's coast survivors replace the shipped founders. Then cells to coast, then protocells to cells, polymers to protocells, chemistry to polymers.
3. A budget and a failure state per scale, with go-back cards, calibrated with the CLI like every goal so far.
4. Recipe and sky readouts: each scale shows its numbers against the default world.
5. Archipelago at world size with surrogate islands.
6. Run summaries and challenge recipes.
