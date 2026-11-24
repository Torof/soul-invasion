# lab — the laboratory: series of vivarium runs, created, followed, read and compared (D52)

`lab.py` is a small local web server, standard library only, that creates
series of `vivarium` runs (D41) from a form, follows them live, and reads
their data (`reading.py`: a run, a series, two runs or two versions face to
face). It is a tool run by hand on the developer's machine; the game never
runs it and depends on nothing here. Nothing goes outward: it serves on
`127.0.0.1` only, opens no browser, and its pages load no external resource.

The division of labour, in the lead's words (22 November 2026): the AI
session launches the series after every change of the simulation, the lead
reads and compares. So every series carries its **version** (the git
commit and branch of the binary's worktree, and one sentence of what
changed), and comparing means first **the same plan under two versions**.

## Start it

```sh
python3 -I tools/lab/lab.py
# Laboratoire : http://127.0.0.1:8765/  (runs : ~/.cache/soul-sim/runs, pid …)
```

Options: `--port 8765`, `--runs-dir ~/.cache/soul-sim/runs` (where plans and
series live), `--binary ~/.cache/soul-sim/target/release/vivarium` (the
vivarium to launch; the lab never builds it: a series whose binary is
missing or being rebuilt waits and retries every five seconds),
`--content <repo>/content` (the content directory given to every run;
defaults to the content of the checkout holding `lab.py`), `--jobs N`
(runs at a time when a plan says nothing; defaults to the cores minus two),
`--worktree DIR` (the git worktree the binary was built from: its commit
and branch are recorded as the version of every series launched here;
without it the version reads "inconnu").

## Launch from the command line (the AI session's way)

```sh
python3 -I tools/lab/lab.py run tools/lab/plans/etalon-bande-25-5ans.json \
    --change "D50 b : le juge compte la soif" --worktree ~/.cache/soul-sim/wt-x \
    --binary ~/.cache/soul-sim/target/release/vivarium [--content DIR] [--runs-dir DIR] [--name NAME] [--jobs N]
```

Reads any plan file (the standard plans below, or a `soul-runs` plan),
names the series `<plan stem>-<date and time>` (or `--name`), writes it
under `<runs-dir>/plans/`, records the version (`git rev-parse` in
`--worktree`, or the plan's `worktree` field; `--change` is the sentence)
and starts the series **detached**: a child `lab.py run … --attached` owns
the runs to their end and writes `serie.json`, the parent prints the folder
and the URL and returns. A server's "En cours" screen sees the series
through its files ("en cours (hors labo)"), and "Lire / Comparer" reads it
once its runs have written their data. `--content` defaults to
`<worktree>/content` when a worktree is given. `--attached` runs in the
foreground until the end (exit code 0 when every run finished).

### The standard plans, `tools/lab/plans/`

- `etalon-bande-25-5ans.json`: one band of 25 at camp A (15 adults, 7
  children, 3 infants), 250 days, with and without trials, seed 1 (two runs,
  about three minutes each).
- `etalon-deux-tribus-25-20ans.json`: two tribes of 25 adults, 1,000 days,
  seed 1, with and without (two runs, about an hour each).
- `etalon-dix-graines.json`: the same over seeds 1 to 10 (twenty runs), for
  when the lightening makes them cheap.

After every change of the simulation, the AI session runs the first two with
`--change "…"` and the lead compares the versions from the series' page.
The form's "Les plans étalons" table launches the same plans from the
browser (a change sentence is required there too).

Open the printed URL yourself. Ctrl-C (or `kill PID`) stops the lab and
ends the runs it launched; series launched elsewhere are untouched.

Three screens:

- **Nouvelle série**: the form. The question, the world (`vivarium` or a
  `patch` of the valley with its side), the tribes, the grown people,
  children and infants per camp, the ages, `--fed`, the days (the year of
  50 days is shown), the seeds (a list `1,2,3`, a range `1-10`, or both),
  "avec et sans épreuves" (two runs a seed, the second with `--no-trials`),
  "avec et sans feu" (two more a seed, `-feu` and `-sansfeu`, the latter
  with `--no-fire`: the control of D55, imitating by proximity as before
  the fire),
  the cadence (1 by default; 10 is the game's and ten times slower) and the
  runs at a time. It writes `<runs-dir>/plans/<name>.json` and launches the
  series. Under the form, every existing plan has **Relancer** (run it
  again, same folder, old run files removed) and **Dupliquer en
  changeant…** (prefill the form from it).
- **En cours**: every series found under the runs directory, newest first,
  with its live table (run, state, year, alive, dead, elapsed), refreshed
  every two seconds from `/api/series`. A running series launched by this
  lab has **Arrêter**, with the confirmation in the page; it kills that
  series' own processes only. A finished series shows its final line and
  links to `synthese.html` and to each run's page and log.
- **Lire / Comparer**: the series, newest first, each with its state, its
  version and change sentence, and a line of key numbers (runs read, alive
  at the end with and without trials, deaths by want with their causes,
  background deaths, births); a form with two selects to compare any two
  series or runs. Then:
  - `/serie/<serie>`: the question, the version, a one-sentence verdict
    (the mean alive and the mean deaths by want, with and without trials,
    and what the trials change), "**Comparer avec la
    version…**" (the other series made from the same plan: same plan name or
    identical runs once expanded, newest first, with their version and
    sentence), a second control for two runs of the series, a table with one
    row per run (seed, with/without, state, alive, deaths by want, background
    deaths, births, kept/trials, time), the mean, min, max and standard
    deviation of alive at the end for "with" and "without", the same for the
    deaths by want and the background deaths, and the population curves of
    every run overlaid
    (with in green, without in ochre).
  - `/run/<serie>/<name>`: **the page one understands at a glance** (22
    November 2026, after the lead's reading of the first screens). From top
    to bottom: the header (series, version, seed, with/without, links);
    **the verdict**, three sentences in everyday French, the deaths by want
    first since they are the brain's figure (22 November 2026: since #314
    most deaths are the background curve of D49, so "alive at the end" no
    longer measures the brain): "3 morts de manque : 1 de faim, 2 de froid,
    en automne ; 24 morts de fond : maladie 22, vieillesse 2. Après 5 ans, 9
    des 25 du départ sont vivants, et 2 des 4 enfants nés en route. Une
    technique gardée : chercher plus loin."; **six tiles** with one big
    number each (alive in green; **deaths by want** in red, hunger, thirst
    and cold underneath; **background deaths** in grey, illness, old age and
    childbirth underneath, and any cause the lab does not know yet; births;
    **the tribe's stored food** in kJ, what the living carried at the end
    of the run's last year, `years[].food.end_kj` summed over the tribes,
    with the days of food it makes per living person at 10,000 kJ a day,
    the year's start and each tribe's share, or "pas dans les données" when
    no year carries `food`; techniques carried at the end in orange);
    **the map of movements over time**: the
    vivarium's picture with a day slider and play/pause, the living as
    dots coloured by tribe with their trail of the last days (from the
    JSON key `positions`, one sample a day at noon; "pas de positions dans
    ce run" when the key is missing, the slider then moving the camp and
    the deaths only), the dead as a cross where they fell from the day
    they die, the camp at every year's end joined by its path, the day
    shown as "cycle 3, printemps, journée 112" (a game day is « journée », four seasons a « cycle », « an » only for ages, real time says « réel »), and the year's presence heatmap
    as an option; **the population curve** alone, the whole width, years
    marked and winters shaded, and under it, on the same x scale, **the
    deaths of each year per tribe** as a small bar stacked in two, the
    deaths by want in red under the background deaths in grey, the bar's
    outline in the tribe's colour (A left of the year, B right), the
    causes on hover; **"Ce qu'ils ont trouvé"**: per tribe, the
    techniques told in sentences, then a small table (technique, first
    kept when and by whom, copied by, carried at the end, what it
    changes); the three tables **"Les morts"** (who, when, age at death,
    cause, where from the camp), **"Les naissances"** (who, when, mother,
    father, alive at the end?) and **"Les gens à la fin"** (name, tribe,
    sex, fate, age at the end or at death, children, techniques carried),
    each with a title, a one-line caption, full-word headers, zebra rows
    and no borders; and, folded under **"Le détail"**: the settings, the
    cultures, the other curves (births and deaths by cause, trials kept,
    fat reserve), the kept trials by day, the yearly table and the life
    lines of everyone. Ages are in game years of `days_per_year` days.
  - `/compare?a=…&b=…`: a one-sentence verdict (B against A), then two
    series (run by run, paired by seed and
    with/without: the key numbers, alive, deaths by want, background
    deaths, births, kept and each cause, with the differences highlighted,
    the aggregates of alive and of the two kinds of deaths, the population
    curves overlaid, the cultures side by side, a link to each pair's
    face-to-face page; the verdict gives B against A in alive and in deaths
    by want) or two runs (each side's verdict, the six tiles with a green
    or red arrow for the difference, fewer deaths by want and more stored
    food being better, the four curves overlaid, A solid and B dashed, the maps side by side,
    the discoveries side by side).

**Since lab 6 (23 November 2026, D52 amended)** the pages above are laid out
again; where this list and the pages differ, the pages are right:

- `/` (**Les séries**): one compact card a series, closed by default: its
  title, the day, launch → end, the number of runs and the first sentence of
  its purpose ("but non écrit" when there is none). A click opens it: the
  verdict and two or three points (from `/api/resultats?serie=`, read once
  and cached by the files' times), the whole purpose, what changed, the
  version, the links and the runs. Sorting, filtering by state, a search and
  grouping by day; the standard plans at the end. A plan's `title` (the form's
  **Titre**, and each standard plan's) names its series; without one, the
  series' name without its date stamp.
- `/serie/<serie>`: the same head, the runs as cards, the aggregates and the
  population curves by variant, named by what differs between the runs
  ("avec le feu / sans le feu", "avec essais / sans essais (témoin)", or the
  four short names when both vary).
- `/run/<serie>/<name>`: the vivarium's page folded into the lab's.
  **Les facteurs** (cards by theme from `meta.factors`, every setting folded);
  **Les gens** with the age pyramids at the start and the end and the
  families as folded cards; **Les courbes** as small multiples sharing one
  hairline (a click enlarges one); **Nourriture et tribus, cycle par cycle**
  (big figures, trend points, a strip of the years with ★ most food, ▼
  least, ▲ most births, ✕ most deaths by want; the table folded and loaded on
  demand); **Où ils vivent**, two canvas maps side by side, each among six
  kinds (terrain and relief, food by season, water, noons, routes and camps,
  deaths and births) sharing one year slider, pan and zoom, with hover and a
  click on a person opening their card; **La chronique** with its trends and
  ten notable facts, one of each kind in turn, the whole chronicle folded.
  The heavy pieces come from `/morceau/<serie>/<run>?quoi=`: `fond` (the
  terrain PNG), `carte-base`, `carte-an&an=N`, `courbes`, `relief`,
  `nourriture`, `annees`. Colours always come with a word or a sign.

**Since 24 November 2026 (D52 amended again, the pages in plain words)** the
series and run pages are laid out once more (`plain.py`); where the lists
above and the pages differ, the pages are right:

- `/serie/<serie>`: a breadcrumb, the question (`plan.question`), the answer
  under it (`plan.answer`, or "Réponse : pas encore écrite"), then **one
  table**, a row a run: au départ, naissances, morts (dont faim), vivants à
  la fin, ayant vécu en tout, couples ensemble à l'aube, distance du camp à
  midi au printemps, déménagements, each header explained in a tooltip and in
  a legend under the table. A row is named by the run's `note` (with its seed
  when the note lacks it), else by the settings that differ between the runs,
  and the seed. A run's `ref` (`"series/run"`) puts that run first, marked
  "référence", and colours the run's cells against it: green better, red
  worse, a gap of 5 % or less (2 at least, 3 points for the couples) left
  uncoloured; the distance and the moves are never coloured. A click on a
  row opens the run. The computing time of each run is in small at the bottom.
- `/run/<serie>/<name>`: the breadcrumb "Labo › series › run" and "← retour
  à la série", one summary line (at the start, births, deaths by cause,
  alive at the end, lived in all, the simulated days and age-years), then
  always the same parts, each opened by one plain sentence: the map, the
  population, births and deaths on a frieze, the ages, the couples, the homes
  changed, the chronicle; the rest folded at the bottom ("Pour aller plus
  loin").

The vivarium's own page is now minimal (settings, end counts, the map, the
timing); `--page full` writes the long page of before.

Everything on these pages is read from the runs' `NAME.json` (the
vivarium's data, `docs/10-vivarium-data.md`), never from their pages. The
curves are inline SVG drawn by Python; the only script drives the map's
day slider (and the heatmap's year). The look is shared by every page:
light background, system font, no cell borders, colours that mean
something (green alive, red hunger, blue thirst, grey cold, orange with
trials, blue without). A run without data (an old vivarium, a run still going) shows
"pas de données, relancer avec le vivarium récent" and its live line. The
kinds of steps and the causes of death are read from the data: a new kind
appears by itself, named by its key until the lab learns its French.

## The plan format (shared with `soul-runs`)

```json
{
  "question": "Do the trials change survival over ten seeds?",
  "binary": "~/.cache/soul-sim/target/release/vivarium",
  "content": "~/.cache/soul-sim/wt/content",
  "out": "~/.cache/soul-sim/runs/essai2-dix-graines",
  "runs": [
    {"name": "vivarium-25", "args": "--world vivarium --tribes 2 --people 25 --days 1000",
     "seeds": [1, 2, 3], "with_and_without_trials": true}
  ],
  "change": "D50 b : le juge compte la soif",
  "worktree": "~/.cache/soul-sim/wt-x",
  "lab": {"form": {"...": "the form's fields, for Dupliquer"}, "jobs": 8, "created": "2026-11-22T15:04:00",
          "source": "etalon-bande-25-5ans"}
}
```

`change`, `worktree` and `lab.source` (the standard plan a series was made
from) are the lab's own too; `soul-runs` ignores them.

Expansion: one run per seed, named `<name>-s<seed>`; with
`with_and_without_trials`, two per seed, `-avec` and `-sans` (the latter
with `--no-trials`); with `with_and_without_fire`, twice again, `-feu`
and `-sansfeu` (the latter with `--no-fire`). A `--seed` inside `args`
wins over `seeds`; `args` may
be a list or a shell-like string. The `lab` key is the lab's own and
`soul-runs` ignores it; a plan without it (written by hand or by
`soul-runs`) is read all the same, the form being prefilled from its
first run's `args`.

## The output layout (shared with `soul-runs`)

```
<runs-dir>/plans/<name>.json      the plan
<runs-dir>/<name>/serie.json      {"plan" (optional "answer"), "runs": [{name, base, seed, trials, args,
                                   started, ended, wall, pid, stopped, optional note and ref}], "lab": {pid, jobs, note},
                                   "version": {commit, branch, dirty, change, worktree, binary, binary_mtime}}
<runs-dir>/<name>/NAME.json       the vivarium's data (docs/10), what "Lire / Comparer" reads
<runs-dir>/<name>/NAME.log        the run's stderr and stdout (`year N: A alive, D dead so far, S s`)
<runs-dir>/<name>/NAME.html       the vivarium's report (`--out`)
<runs-dir>/<name>/NAME.exit       the exit code, written when the run ends
<runs-dir>/<name>/synthese.html   written when the series ends: the question, the plan,
                                   one row per run with the last year's counters per tribe
```

A run's state is read from these files alone: `attend` (not started),
`en cours`, `fini`, `ÉCHEC(code)`, `arrêté` (by the button or the lab's
exit), `perdu` (started by a process that is gone, no exit code). A
`soul-runs --watch DIR` still reads a series the lab launched.

## Tests

```sh
python3 -I tools/lab/lab_test.py
```

Standard library `unittest`: the plan expansion (seeds × with/without),
the seeds field, the parsing of the year lines, the state machine, the
form to plan and back, the reading of a run's data (a fixture written by
hand after `docs/10`), the kinds read from the data, the story and
table of techniques, the verdict sentence, the deaths by want apart from
the background (an unknown cause counts as background), the stored food
and its missing-key tile, days, ages and places in words, the three
tables of the run page, the tiles and their arrows, the curves' scaling,
the map and its `positions` fixture, the series' aggregates, the series' table (the runs' names,
the reference colouring, the new columns, the old runs without their keys), the run page's parts, the
compare pages, the version record (a temporary git repository), the
standard plans, the routes, and three end-to-end runs with a shell script
in place of the vivarium (a series to its synthesis; the `run` command
line; a stop that kills only its own runs). About two seconds.
