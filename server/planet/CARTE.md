# The planet's map: which mechanism lives where

## Rule of work for a round on the model

1. **Try small.** A change of a mechanism is tried on its bench scene
   (`banc`, under a second to a few seconds) and on two seeds (14 and 9),
   with the try build. Not on thirty seeds.
2. **The thirty seeds run once, at the end**, through the bilan
   (`tools/bilan.sh vNN OUT_DIR`), with the release build.
3. **The proof** (`tools/preuve.py`) says whether the worlds changed. A
   tooling change must leave it identical. A change of the model moves it:
   save the new reference (`--save`) in the same commit. The bilan has
   one line a section in the proof: a round that adds rows to the bilan
   moves the lines of the sections it touched, and only those.
4. **One archive of the page, no more**: `tools/page.py --previous NN`
   keeps the page before it as `carte-vNN.html` (the previous version
   only). Older archives are not regenerated, and no copy is made per try.
5. Read this map and the module of the mechanism you change. The two
   files of several thousand lines are gone: no module is over 800 lines.

## The commands

All from `server/`, with `CARGO_TARGET_DIR=~/.cache/soul-sim/target-planete`
and under `soul-cap`.

| What | Command | Measured (27 Nov 2026, 16 threads) |
|---|---|---|
| Does it compile | `cargo check -p planet --all-targets` | under 1 s after a one-line change |
| **Try**: a bench scene | `cargo run --profile essai -p planet --example banc -- rift` | build 3 s, a scene about 1 s, the six together 4 s |
| **Try**: two seeds, numbers only | `PLANET_OUT=dir PLANET_SEEDS=14,9 cargo run --profile essai -p planet --example planet` | build 3 s, two planets 5 s |
| Tests | `cargo test --profile essai -p planet --lib` | build 2 s, tests 11 s |
| **Final**: the bilan on 30 seeds | `planet/tools/bilan.sh v12 OUT_DIR` | 2 min (was 7 min on one thread) |
| **Final**: the identity proof | `planet/tools/preuve.py` | 3 min (planets with layers 1 min, bilan 2 min) |
| The page for seeds 14 and 9 | `planet/tools/page.py --out DIR --version 12 --previous 11` | 70 s |
| A sheet of 12 steps | `planet/tools/planche.py DIR 14 1 out.png` | 1 s |

The `essai` profile (workspace `Cargo.toml`) is the release profile at
`opt-level = 1` with incremental compilation, overflow checks and debug
assertions: the build of a one-line change is about twice as fast as
release, a planet about 15 % slower, and an integer overflow panics
instead of wrapping. The worlds are the same under every profile
(integers only). `cargo test` without `--lib` also builds the examples a
second time: use `--lib`.

`PLANET_THREADS=n` sets the threads of `planet`, `bilan` and `banc`
(default: every core). What they write does not depend on it, except the
seconds a planet takes.

## Where things are

`src/lib.rs` only declares the modules and re-exports their names.

| File | What it holds |
|---|---|
| `math.rs`, `noise.rs`, `hash.rs` | Fixed-point trigonometry and vectors; seeded draws and noise. |
| `sphere.rs` | The grid: `FACE`² × 6 cells of one area, neighbours, `cell_of`, `spread`, `quantile`. The cell count is a constant (`CELLS`). |
| `history/` | **The model**: the plates' history, one mechanism a module (below). |
| `today.rs` | Today's plates and the kinds of their boundaries (`Layout`, `Kind`, `classify`). |
| `mosaic.rs` | What the supercontinent's crust is made of at the start (`Mosaic`, version 13): old cores of many sizes (30 to 46 older plates over the sphere, borders warped by `CORE_WARP`) welded by belts 200 to 900 km wide; a belt's crust is youngest on its axis and grades to its cores' age at its edges. `start.rs` reads it (ages, old ranges and troughs); nothing else does. |
| `planet.rs` | `Planet`: the history, then heights, sea, landmasses, hollows; `at_step(t)`. |
| `site.rs`, `zoom.rs` | Our continent: its site on a subduction margin, its shape across the margin. |
| `tests.rs` | The tests. |
| `examples/planet.rs` | Draws seeds: maps, the page's layers (`PLANET_LAYERS`), `graine-N.json`. |
| `examples/bilan.rs` | The one measuring tool: a table over many seeds, a JSON per version. Sections: 1 flips, 2 rigid pieces, 3 plates, 4 crust kept, 5 water inside the continents, 6 crust and its plate, 7 rifts (with the troughs begun on plates carrying a crumb and the tension at a trough's start), 9 collisions (begun, one step only, stopped by their pile, how long they last, those between two real continents, the thickest pile, today's continents), 10 the engine (the step the plates start to move, trenches and ridges through time, the ocean floor's age, plates not born for lack of a name), 11 relief and sea (land above 2,000 and 4,000 m, the thickest crust, the oldest range still standing, crust under water), 8 seconds. |
| `examples/banc.rs` | The bench: small written scenes, one mechanism each, a sheet of their steps. |
| `tools/` | `preuve.py` (identity proof), `bilan.sh`, `page.py` + `carte.html` (the page), `planche.py` (sheets). |

## The settings

A world's character is `Settings` (`history/settings.rs`): each field is
documented there with its default. The lead's file `monde-defaut.ron`
(review folder) lists the same fields in French; `--settings file.ron`
reads it, a missing field takes its default.

A mechanism's own tuning constants sit at the top of its module (for
example `RIFT_WIDTH_KM` in `history/rifts.rs`, `WEAR` in
`history/wear.rs`). The draws' purposes (`P_*`) are in `history/mod.rs`.

## The history, a mechanism a module

`history/mod.rs` is the table of contents: `run` = `start::begin`, then
`advance` once per step, then `record::finish`. **`advance` lists the
mechanisms in the order a step runs them**: read it first.

The state they all work on is `World` (`history/state.rs`): the crust
(pieces of continent, bits of ocean floor), the grid's view of it
(`plate`, `rock`, `top`), the plates' turns, the mantle's heat, the
events' records and the layers kept for each step. What the mechanisms
of one step hand to one another is `world.step` (`Step`, same file).
Each mechanism is a function `fn name(world: &mut World)` that begins by
naming the parts of the world it uses.

| Order in a step | Function | Module | What it does |
|---|---|---|---|
| start | `begin` | `start.rs` | Supercontinent, first plates, ocean floor, relief, heat, plumes, first forces. Since version 14 the supercontinent's plates carry no floor: the ocean's plates reach its rim, their floor oldest far from their ridges, and a slab hangs wherever that floor meets the supercontinent (`hung`); those slabs pull from step 0, and the mantle starts cold around them. |
| 1 | `begin_step` | `crust.rs` | Clears what the last step left on each cell of crust. |
| 2 | `stopped` | `collisions.rs` | **A collision its pile has stopped** (version 15): the two plates the forces of the last step turned as one (`World::locked`, see step 21) are one plate from now on; the journal says `EndedWelded`. No count of quiet steps, no speed under which plates "weld", no block pushed out sideways (the escape script is gone). |
| 3 | `troughs` | `rifts.rs` | Rifts. **A trough begins where the tension across a plate is more than its section holds** (version 15, in place of "stretches at 1 mm a year on a plate with some continent"): the tension is how fast the two sides of a line would part times the drag they share (`forces::parting`: a small side is carried for little, so a crumb does not stretch); the section is every cell of the plate along the line, each holding as its lithosphere is strong (`holds`: a continent's crust by its thickness and the inverse of `weak`, the ocean floor by the square root of its age); `Settings::crust_holds` (300) is what a cell of the strongest crust holds. The same test, on the trough's own thinned line, says whether it goes on. Troughs grow cell by cell toward the weakest way through the crust `RIFT_AHEAD_KM` (368 km) ahead of each tip (`ahead`), stretch and thin it (each cell from the day it was reached), cut the plate once they cross it, then unzip; or are abandoned. `weak`: cold, warm mantle, a suture or failed rift of the history, the crust's age: so the mosaic's belts are weak and its cores strong. |
| 4 | `turn` | `crust.rs` | The plates turn: pieces change their turn, bits of floor move, and are sorted by cell. |
| 5 | `meet` | `collisions.rs` | Pieces that meet: crust run under another piece piles on it (suture): on the thinnest crust of that plate within `PILE_REACH` cells (4, some 370 km) of the crust above, on whatever piece of that plate, so a range squeezed on widens. No ceiling on a pile's thickness (version 15; 70 km until then, the rest "waited under"). |
| 6 | `sink_and_dock` | `trenches.rs` | Floor under a continent sinks (active margin); terranes and arcs dock. |
| 7 | `draw_floor` | `trenches.rs` | The grid's view; ocean under ocean (the older sinks), island arcs. |
| 8 | `holes_and_gaps` | `floor.rs` | Holes of the floor's sampling mended; a gap beside stretched crust filled with that crust's own width while any is left; other gaps between parting plates filled with new floor; rift margins. |
| 9 | `specks` | `plates.rs` | A lone cell of a plate joins the plate around it. |
| 10 | `piles_and_silt` | `wear.rs` | Piles flow and wear; their silt builds the margins out. Version 15: crust flows out from under a pile as the sixth power of what it has above a continent's 35 km (`THICK_SPREAD`: 6 ‰ of the difference a step at 50 km, 130 at 60, the whole at 70), worked out in `SPREAD_PARTS` parts of a step: a range keeps its root, a plateau is flat and widens. A pile loses `mountain_wear` (25 ‰, was 100) of its excess a step: half in 140 My. A km of extra crust stands 150 m higher (`ISOSTASY_M`, `crust.rs`; 100 until version 14). |
| 11 | `margins_give_way` | `trenches.rs` | **A trench is born** (version 14). The floor beside a continent of its own plate (a passive margin) bends down under its own weight: each shore cell keeps how far (`Rock::sag`), which grows by `SAG_M_MY` m a My times the floor's weight (`slab_weight` of its age: nothing under 10 My, full at 80), times how hard the floor pushes its continent along (read from the forces: the drag on the continent as it moves, less the forces on the continent itself, shared among the plate's shore cells; pressed as hard as a full slab pulls, twice as fast; pulled as hard, not at all). At `SAG_BREAK_M` the floor gives way: that ocean's floor on this plate becomes a plate (`BornOfTrench` in the journal) and a slab hangs where it gave way. An unpressed margin of a new ocean gives way about 160 My after the ocean opened; old floor that became a margin, after 100 My. |
| 12 | `one_piece` | `plates.rs` | A plate is one piece: parts cut off become plates or are captured, crumbs absorbed; plates parted. |
| 13 | `hotspots_mark` | `heat.rs` | Hotspots mark and lift the crust over them. |
| 14 | `compact` | `crust.rs` | Dead crust leaves the record; what the view hides is counted. |
| 15 | `margins_sink` | `rifts.rs` | The crust beside a new ocean sinks (a rifted margin). |
| 16 | `look_at` | `forces.rs` | What the plates do now: kinds of boundaries, margins (`look`). **A trench is a state** (version 14, `World::hung`): floor that sinks under another plate goes on sinking where a slab hung at the step before, there or a cell away, however slowly the plates close (it began at 2 mm a year, `FORCE_LEAST_UM`), and its slab pulls by its weight; the slab is lost where the plates part at 2 mm a year. So a trench lasts through a slow spell, moves with its plates, and runs on along a boundary a cell a step. |
| 17 | `warm_step` | `heat.rs` | The mantle's heat under the crust as it lies (`warm`). |
| 18 | `sea_and_weather` | `wear.rs` | The sea level; relief wears down and fills (`weather`). |
| 19 | `progress` | `collisions.rs` | Collisions begin, last, end: between continents only (a terrane or an arc that meets a continent docks, step 6). Each record (`Collision`) keeps its two continents' sizes, its dates (begun, ended), how it ended (`stopped`: by its pile), the middle of its front, the way it squeezed and its thickest crust. The piles at the fronts (`Step::piles`): the thickest crust at or a cell from each side of a cell where two plates' continents touch, as a weight (`forces::pile_weight`). |
| 20 | `write` | `record.rs` | The step's layers and its journal of events. |
| 21 | `next` | `forces.rs` | The forces and the plates' turns for the next step (`drive`): the crust that still joins two plates a rift has cut holds them there (`Bridge`); boundaries that open, lava floods. **A pile pushes back** (version 15, in place of the grip that took a share of the speed away): a force on the two plates, `Settings::pile_push` times the pile's weight (the difference of the squares of its thickness and a continent's) at each side of their front, against their closing only. While the forces press harder the plates close, slower; once the pile holds all they press with, the two turn as one (`Drive::locked`) if they are in a collision, or stop closing if a rift had cut them. |

Steps 1 to 15 do not run at step 0. Other things a module holds:
`pieces.rs` the rigid pieces (`Land`: lattice, `cover`, `kin`, `spread`),
the grid's view (`paint`, `seats`) and who rides over whom
(`precedence`); `crust.rs` the records a crust carries (`Rock`, `Bit`,
`Mark`, the named causes of a change of height); `plates.rs` the merge
of two plates (`absorb`) and the parts of a plate (`parts`);
`record.rs` the public records and `History`.

A plate's name is its id, below `MAXP` (64). A new plate takes a new
name while any is left; after that, the names of the plates that are
gone are given again (`World::free_names` and `name`, `state.rs`). A
plate that finds no name is not born and is counted
(`History::unnamed`, a row of the bilan that must be 0).

## The bench

`cargo run --profile essai -p planet --example banc -- [scene…]` runs
written scenes on the real model (`history::run_scene`: the model's own start and
steps, on continents and plates laid by hand), prints a line of numbers a step and writes
`banc-<scene>.png`, a sheet of its steps, in `PLANET_OUT` (default the
current folder). With no name, every scene.

| Scene | What is written | What it shows |
|---|---|---|
| `rift` | One continent of 3,000 km on its plate, the forces free, 32 steps. | Nothing tears for 19 steps: the lid's heat alone pulls less than the section holds. Once a trench is born at its margin (step 20) the tension is enough: a trough begins, grows, the plate is cut once it has crossed, the ocean unzips. |
| `collision` | Two continents driven at each other at 60 mm a year each for 6 steps, then left to the forces (`Scene::free_from`), 20 steps. | They meet and pile up; the slabs beside the front go on pulling, the pile grows to some 75-90 km, flows sideways and pushes back. The scene's two plates are the two hemispheres, so the pull never ends and this collision does not stop: a stop is seen in the worlds (the bilan's section 9). |
| `trench` | An ocean plate driven under a continent that stands still. | An active margin and its arc. |
| `arcs` | Two ocean plates driven at each other, no continent near. | The older floor sinks, island arcs grow. |
| `carried` | A continent carried 20 steps by its plate. | A rigid piece: its cells unchanged, and no trough (version 15: a continent its plate simply carries does not stretch). |
| `margin` | A continent of 2,800 km alone on its plate with the old floor around it, the forces free, 30 steps. | A trench is born: nothing sinks for 19 steps while the margin bends, the floor gives way at step 20 (one plate more), and the trench runs along the margin in the next steps. |

To add a scene, write a function like these in `examples/banc.rs` and
name it in `SCENES`. A scene is not "well under a second": it runs on
the planet's whole grid, whose cell count (`CELLS`, 60,000) and radius
are constants, so its time is the time of its 14 to 20 steps (about 1 s).
A smaller sphere would need `FACE` and `RADIUS_KM` chosen at build or run
time, which touches the grid's constants and the thresholds in km²: a
change of the model's code, not done in the tooling round.

## The journal and the page

The history keeps a journal (`History::journal`, `history/record.rs`:
`Event`, `What`): for each step, what happened to the plates, with its
cause and its place. A plate born (of which rift, of which margin giving way, cut off which plate), a plate ended (welded, crumb, enclosed,
no cell left), a plate that moves as one with another (`MovesWith`: the
map then draws them as one) or alone again, a rift begun or abandoned, a
collision begun, terranes docked, islands risen. Recording only: no
mechanism reads it.

`examples/planet.rs` writes it beside the layers (`h<seed>-journal.js`)
and the page shows it: in "Histoire", the list of the step's events in
French (a click marks the place), and the button "Frise des plaques", one
bar per plate drawn from its birth to its end. The address
`carte.html#histoire-9-12-frise` opens seed 9 at step 12 with the strip.
