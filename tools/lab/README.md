# lab — the laboratory: series of vivarium runs, created and followed from a page (D52)

`lab.py` is a small local web server, standard library only, that creates
series of `vivarium` runs (D41) from a form and follows them live. It is a
tool run by hand on the developer's machine; the game never runs it and
depends on nothing here. Nothing goes outward: it serves on `127.0.0.1`
only, opens no browser, and its pages load no external resource.

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
(runs at a time when a plan says nothing; defaults to the cores minus two).

Open the printed URL yourself. Ctrl-C (or `kill PID`) stops the lab and
ends the runs it launched; series launched elsewhere are untouched.

Two screens for now:

- **Nouvelle série**: the form. The question, the world (`vivarium` or a
  `patch` of the valley with its side), the tribes, the grown people,
  children and infants per camp, the ages, `--fed`, the days (the year of
  50 days is shown), the seeds (a list `1,2,3`, a range `1-10`, or both),
  "avec et sans épreuves" (two runs a seed, the second with `--no-trials`),
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
  "lab": {"form": {"...": "the form's fields, for Dupliquer"}, "jobs": 8, "created": "2026-11-22T15:04:00"}
}
```

Expansion: one run per seed, named `<name>-s<seed>`; with
`with_and_without_trials`, two per seed, `-avec` and `-sans` (the latter
with `--no-trials`). A `--seed` inside `args` wins over `seeds`; `args` may
be a list or a shell-like string. The `lab` key is the lab's own and
`soul-runs` ignores it; a plan without it (written by hand or by
`soul-runs`) is read all the same, the form being prefilled from its
first run's `args`.

## The output layout (shared with `soul-runs`)

```
<runs-dir>/plans/<name>.json      the plan
<runs-dir>/<name>/serie.json      {"plan", "runs": [{name, base, seed, trials, args,
                                   started, ended, wall, pid, stopped}], "lab": {pid, jobs, note}}
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
form to plan and back, and two end-to-end runs with a shell script in
place of the vivarium (a series to its synthesis; a stop that kills only
its own runs). Under a second.

## What PR 2 adds

The third screen, **Lire / Comparer**, once the vivarium writes its report
as JSON beside the HTML page: reports per series, tables and curves (alive,
deaths by cause, births, trials kept) across seeds, and comparisons between
series (with against without trials, one content value against another).
Reading parses the JSON, never the pages.
