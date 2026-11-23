"""Tests of the laboratory (D52): plan expansion, year lines, the state machine,
the reading of a run's data, the curves, the series' aggregates, the compare
pages, the version record and the `run` command line.

    python3 -I tools/lab/lab_test.py

Standard library only. The end-to-end tests launch a shell script in place of
the vivarium binary, so they need `/bin/sh`; the version test needs `git`.
"""

import json
import os
import shutil
import stat
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lab  # noqa: E402
import panels  # noqa: E402
import reading  # noqa: E402

FAKE_VIVARIUM = r"""#!/bin/sh
# A vivarium that lives two years and writes a tiny report.
out=vivarium-report.html; sleep_s=0
while [ $# -gt 0 ]; do
  case "$1" in --out) out="$2"; shift;; --days) sleep_s="$2"; shift;; esac; shift
done
[ "$sleep_s" -gt 100 ] && sleep 30
echo "year 1: 4 alive, 1 dead so far, 2 s" >&2
echo "year 2: 3 alive, 2 dead so far, 4 s" >&2
printf '<html><table><tr><th>Year</th><th>Tribe</th><th>Population</th><th>Hunger</th></tr><tr><td>1</td><td>A</td><td>4</td><td>1</td></tr><tr><td>2</td><td>A</td><td>3</td><td>2</td></tr></table></html>' > "$out"
if [ -n "$FAKE_JSON" ]; then cp "$FAKE_JSON" "${out%.html}.json"; else
printf '{"meta":{},"years":[{"tribe":"A","year":1,"population":4,"deaths":{"hunger":1}},{"tribe":"A","year":2,"population":3,"deaths":{"hunger":2}}]}' > "${out%.html}.json"; fi
echo "3 alive, 2 dead" >&2
exit "${FAKE_EXIT:-0}"
"""


def fixture(seed=1, trials=True, fire=False):
    """A tiny data file written by hand after docs/10-vivarium-data.md: two tribes, two years;
    with `fire`, the keys of D55 (lot 4, PR 2) too."""
    def year(tribe, yr, pop_before, pop, births, deaths, trials_n, kept, copied, camp):
        total = sum(deaths.values())
        return {"tribe": tribe, "year": yr, "first_day": (yr - 1) * 50 + 1, "last_day": yr * 50,
                "population": pop, "population_before": pop_before,
                "growth_permille": (pop - pop_before) * 1000 // pop_before, "births": births,
                "deaths": {"illness": 0, "old_age": 0, "childbirth": 0, "hunger": 0, "thirst": 0, "cold": 0, **deaths},
                "deaths_total": total, "hunger_seasons": ["winter"] * deaths.get("hunger", 0),
                "intervals_days": [], "interval_days_mean": None, "fifteen": {"turned": 0, "alive": 0},
                "fertile_women": {"total": 1, "without_spouse": 0}, "trials": trials_n, "kept": kept, "copied": copied,
                "homes_moved": 0, "home_moves": [],
                "ways": {"minds": pop, "sleep_out": 1 if pop else 0, "median_worth_tiles": 32, "median_stock_kj": 15000} if pop else None,
                "camp": camp,
                "food": {"start_kj": 0, "gathered_kj": 100000 * yr, "eaten_kj": 90000 * yr, "spoiled_kj": 1000, "end_kj": 500,
                         "lost_kj": 0, "thin_hours": 48, "weak_hours": 24}}

    def person(pid, tribe, sex, born, died=None, trials_=(), lines=()):
        return {"id": pid, "name": f"person {pid}", "tribe": tribe, "sex": sex, "born": born,
                "born_day": 1 if born == "start" else born, "age_days_at_start": 7300 if born == "start" else None,
                "born_at": [10, 10], "died": died, "mother": None, "father": None, "spouses": [], "children": 0,
                "lost": [], "will": 50, "ways": {"worth_half_tiles": 32, "sleep_out": False, "stock_kj": 15000, "home": [10, 10]},
                "trials": list(trials_), "copied": 0, "thin_hours": 24, "weak_hours": 0, "gathered_kj": 5000,
                "meals": 10, "drinks": 20, "thoughts": 30,
                "lines": [{"day": 1, "tick": 0, "what": f"person {pid} of tribe {tribe} is born"}] + list(lines)}

    kept_trial = {"day": 10, "step": "wider", "words": "searches twice as wide", "judged_day": 15, "kept": True}
    lost_trial = {"day": 20, "step": "sleep_out", "words": "sleeps out", "judged_day": 25, "kept": False}
    n = 20
    d = {
        "meta": {"format": 1, "settings": {"world": "vivarium", "side": None, "people": 2, "children": 0, "infants": 0,
                                           "adult_years": [18, 40], "fed": False, "brain": True, "tribes": 2, "days": 100,
                                           "ticks_per_second": 1, "seed": seed, "trials": trials, "content": "content"},
                 "map_tiles": [64, 64], "camps": [[10, 10], [50, 50]], "days_per_year": 50, "ticks_per_day": 24,
                 "ticks": 2400, "rates": [{"name": "hunger a day", "rate": 100}],
                 "timing": {"wall_s": 1.5, "birth_s": 0.1, "run_date": "2026-11-22T15:04:00Z"}},
        "years": [year("A", 1, 2, 3, 1, {}, 2 if trials else 0, 1 if trials else 0, 0, [10, 10]),
                  year("A", 2, 3, 2, 0, {"hunger": 1}, 0, 0, 1 if trials else 0, [12, 11]),
                  year("B", 1, 2, 2, 0, {}, 0, 0, 0, [50, 50]),
                  year("B", 2, 2, 1, 0, {"cold": 1}, 0, 0, 0, [50, 50])],
        "found": [{"tribe": "A", "tried": {"wider": 1 if trials else 0, "sleep_out": 1 if trials else 0},
                   "kept": {"wider": 1 if trials else 0}, "copied": {"wider": 1 if trials else 0}},
                  {"tribe": "B", "tried": {}, "kept": {}, "copied": {}}],
        "people": [person(1, "A", "woman", "start", trials_=[kept_trial, lost_trial] if trials else []),
                   person(2, "A", "man", "start"),
                   person(3, "A", "man", 30, {"day": 70, "tick": 1680, "cause": "hunger", "season": "winter", "at": [11, 12]}),
                   person(4, "B", "woman", "start", {"day": 90, "tick": 2160, "cause": "cold", "season": "winter", "at": [51, 52]}),
                   person(5, "B", "man", "start")],
        "curves": {"points": n, "every": 5, "ticks": [i * 120 for i in range(n)], "days": [i * 5 for i in range(n)],
                   "alive": [4] * 6 + [5] * 8 + [4] * 4 + [3] * 2,
                   "alive_by_tribe": [[2] * 6 + [3] * 8 + [2] * 6, [2] * 18 + [1] * 2],
                   "thin_by_tribe": [[0] * n, [0] * n], "weak_by_tribe": [[0] * n, [0] * n],
                   "reserve_by_tribe": [[80 - i for i in range(n)], [70] * n],
                   "hunger": [20] * n, "thirst": [10] * n, "fatigue": [30] * n},
        "presence": [{"tribe": "A", "year": 1, "cell_tiles": 32, "cells": [[0, 0, 60]]},
                     {"tribe": "A", "year": 2, "cell_tiles": 32, "cells": [[0, 0, 40], [1, 0, 10]]},
                     {"tribe": "B", "year": 1, "cell_tiles": 32, "cells": [[1, 1, 50]]},
                     {"tribe": "B", "year": 2, "cell_tiles": 32, "cells": [[1, 1, 30]]}],
        "map": {"data_uri": "data:image/png;base64,iVBORw0KGgo=", "tiles_per_pixel": 1, "tiles": [64, 64], "pixels": [64, 64],
                "legend": {"camp": [220, 40, 40], "alive": [255, 255, 255], "dead": [0, 0, 0]}},
    }
    if fire:
        d["meta"]["settings"]["fire"] = True
        d["meta"]["memory"] = {"written": 5, "freed": 1, "held": 4}
        for y in d["years"]:
            a = y["tribe"] == "A"
            y["fire"] = {"nights": 40 if a else 0, "told": 120 if a else 0, "heard": 300 if a else 0, "dispelled": 2 if a else 0}
            y["knowledge"] = {"places": 30 * y["year"] if a else 5, "recorded": 20 if a else 0, "heard_only": 3 if a else 0,
                              "accesses": 40 if a else 0, "accesses_heard": 30 if a else 0}
            y["food"]["stored_kj"] = {"noon_mean": 400 * y["year"], "noon_min": 100, "end": 500}
            if y["ways"]:
                y["ways"]["stock_kj"] = {"q1": 15000, "median": 15000, "q3": 30000}
        d["found"][0]["heard"] = {"trials": 1, "copies": 1, "dispelled": 2}
        d["found"][0]["by_will"] = [{"band": "timid", "from": 0, "to": 20, "tried": 1, "kept": 1},
                                    {"band": "middling", "from": 21, "to": 50, "tried": 1, "kept": 0},
                                    {"band": "bold", "from": 51, "to": 100, "tried": 0, "kept": 0}]
        d["found"][1]["heard"] = {"trials": 0, "copies": 0, "dispelled": 0}
        d["found"][1]["by_will"] = []
        for p in d["people"]:
            p["will_start"] = 10 if p["id"] == 1 else 50
            p["believes"] = []
            for t in p["trials"]:
                t["will"] = 10
                t["heard"] = t["step"] == "wider"
        d["people"][0]["believes"] = [{"place": [16, 16], "record": 1, "seen": True, "by": 1, "day": 12},
                                      {"place": [32, 16], "record": 2, "seen": False, "by": 2, "day": 30}]
        d["people"][0]["lines"].append({"day": 16, "tick": 384, "what": "at the fire, person 1 says its searches twice as wide did it good; person 2 believed it"})
        d["people"][1]["lines"].append({"day": 40, "tick": 960, "what": "person 2 finds nothing where person 1 had said: the place is bare"})
        d["records"] = [{"id": 1, "kind": "place", "day": 12, "witness": 1, "refs": 1, "place": [16, 16], "states": [[0, 2]],
                         "way": None, "up": None, "kept": None, "will": None, "present": None, "told": None},
                        {"id": 2, "kind": "place", "day": 20, "witness": 2, "refs": 1, "place": [32, 16], "states": [[1, 1]],
                         "way": None, "up": None, "kept": None, "will": None, "present": None, "told": None},
                        {"id": 3, "kind": "place", "day": 40, "witness": 2, "refs": 1, "place": [32, 16], "states": [],
                         "way": None, "up": None, "kept": None, "will": None, "present": None, "told": None},
                        {"id": 4, "kind": "trial", "day": 15, "witness": 1, "refs": 2, "place": None, "states": None,
                         "way": "worth_half_tiles", "up": True, "kept": True, "will": 10, "present": None, "told": None}]
        d["hearth"] = [{"tribe": "A", "nights": [{"day": 15, "camp": [10, 10], "present": 2, "told": [{"record": 4, "by": 1}, {"record": 1, "by": 1}]},
                                                 {"day": 40, "camp": [10, 10], "present": 2, "told": [{"record": 3, "by": 2}]}]},
                       {"tribe": "B", "nights": []}]
        d["rumours"] = [{"day": 40, "tribe": "A", "body": 2, "place": [32, 16], "record": 2, "from": 1},
                        {"day": 60, "tribe": "A", "body": 3, "place": [32, 16], "record": 2, "from": 1}]
    return d


def fake_series(runs_dir, name, version, seeds=(1,), source="etalon-test", json_for=None):
    """A finished series on disk: serie.json and, for each run, the fixture's data when `json_for` says so."""
    out = os.path.join(runs_dir, name)
    os.makedirs(out, exist_ok=True)
    plan = {"question": "Q ?", "out": out, "lab": {"source": source, "created": "2026-11-22T16:00:00"},
            "runs": [{"name": "t", "args": "--world vivarium --people 2 --days 100", "seeds": list(seeds),
                      "with_and_without_trials": True}]}
    runs = lab.expand(plan)
    for r in runs:
        r.update(started=1.0, ended=5.0, wall=4.0, pid=1)
        with open(os.path.join(out, r["name"] + ".exit"), "w") as f:
            f.write("0")
        if json_for is None or r["name"] in json_for:
            with open(os.path.join(out, r["name"] + ".json"), "w") as f:
                json.dump(fixture(r["seed"], r["trials"]), f)
    lab.save_state(out, plan, runs, {"lab": {"pid": 1, "jobs": 2, "note": ""}, "version": version})
    return out


class Layout(unittest.TestCase):
    """The run page laid out (lab 5): the points, the tables, the chronicle, the card, the pieces."""

    def setUp(self):
        self.d = fixture(fire=True)
        self.sm = reading.summary(self.d)
        self.bt = reading.by_tribe(self.d)

    def all_points(self, d):
        sm, bt = reading.summary(d), reading.by_tribe(d)
        return {"top": reading.top_points(d, sm, bt), "gens": reading.people_points(d, sm, bt), "morts": reading.deaths_points(d, sm),
                "naissances": reading.births_points(d, sm, bt), "tous": reading.everyone_points(d, sm), "ou": reading.where_points(d, sm, bt),
                "savoir": reading.knowledge_points(d), "appris": reading.learnt_points(d, sm), "techniques": reading.techniques_points(d),
                "ose": reading.dared_points(d), "chronique": reading.chronicle_points(d, 1, reading.chronicle_events(d, 1))}

    def test_the_points_are_short_and_as_many_as_said(self):
        for d in (self.d, fixture(), fixture(trials=False)):
            for name, pts in self.all_points(d).items():
                lo, hi = reading.TOP_POINTS if name == "top" else reading.SECTION_POINTS
                self.assertTrue(lo <= len(pts) <= hi, (name, pts))
                for p in pts:
                    self.assertLessEqual(reading.point_words(p), reading.POINT_WORDS, (name, p))
                    self.assertTrue(p["b"], (name, p))
        self.assertEqual(reading.point_words(reading.point(f"{reading.num(12345)} vivants", "(+14 en 20 ans, A et B à égalité)")), 11)   # a figure is one word

    def test_the_points_come_from_the_data(self):
        top = [p["b"] for p in reading.top_points(self.d, self.sm, self.bt)]
        self.assertEqual(top[0], "3 vivants")
        self.assertEqual(top[1], "2 morts de manque")
        # a death of want less: the point says so, from the data
        d = fixture(fire=True)
        d["people"][2]["died"]["cause"] = "illness"
        d["people"][3]["died"]["cause"] = "illness"
        sm = reading.summary(d)
        self.assertEqual(reading.deaths_point(d, sm)["b"], "0 mort de manque")
        self.assertIn("2 de maladie", reading.deaths_point(d, sm)["t"])
        # the places of A against B: 60 against 5 in the last year
        self.assertEqual(reading.places_point(self.d, self.bt)["b"], "A connaît 12× plus de lieux")
        # the camps: A's camp moved by (2, 1) only, a stay; moved far, the point says when
        self.assertEqual(reading.camps_point(self.d, self.sm)["b"], "aucun camp n’a bougé")
        d["years"][1]["camp"] = [60, 60]
        self.assertEqual(reading.camps_point(d, sm)["b"], "camp de A déplacé 1 fois")
        self.assertEqual(reading.camps_point(d, sm)["t"], "(an 2)")
        # with a control, the gap of the food carried at noon
        ctrl = fixture(fire=True)
        for y in ctrl["years"]:
            y["food"]["stored_kj"]["noon_mean"] = 200 * y["year"]
        pts = reading.top_points(self.d, self.sm, self.bt, ctrl, "sans le feu")
        self.assertIn({"b": "+100 % de nourriture portée", "t": "à midi, avec le feu", "series": [800, 1600]}, pts)

    def test_the_sparkline_says_the_trend(self):
        self.assertEqual(reading.trend_of([1, 2, 5]), "up")
        self.assertEqual(reading.trend_of([100, 103]), "flat")
        self.assertEqual(reading.trend_of([5, None, 1]), "down")
        self.assertIsNone(reading.trend_of([5]))
        self.assertEqual(reading.spark([3]), "")
        s = reading.spark([1, 2, 5])
        self.assertIn("<polyline", s)
        self.assertIn("monte, de 1 à 5", s)
        html = reading.points_html([reading.point("personne 2 a osé 3 fois", "et gardé 1", [1, 3])])
        self.assertIn('data-person="2"', html)
        self.assertIn("↗", html)

    def test_a_table_has_no_empty_cell_and_sorts_by_its_keys(self):
        rows = [[(f"r{i}", i), (None if i % 2 else "x", None), (reading.fmt_thin(1000 * i), 1000 * i), ("", None)] for i in range(12)]
        t = reading.data_table("t1", [("Nom", False), ("Rien", False), ("Nombre", True), ("Plus", False, True)], rows, "vide")
        self.assertNotRegex(t, r"<td[^>]*></td>")
        self.assertIn(reading.DASH, t)
        self.assertIn('class="data short" id="t1"', t)
        self.assertIn("voir les 12", t)
        self.assertIn("plus de colonnes", t)
        self.assertIn('<th class="xc" scope=col>', t)
        self.assertIn('<td class="n" data-k="11000">11 000</td>', t)
        self.assertEqual(reading.data_table("t2", [("Nom", False)], [], "vide"), '<p class="muted">vide</p>')
        short = reading.data_table("t3", [("Nom", False)], [[("a", "a")]], "vide")
        self.assertNotIn("short", short)
        self.assertNotIn("<button type=button data-", short)
        deaths = reading.deaths_table_v2(self.d)
        self.assertIn('data-k="70"', deaths)   # the day, to sort by
        self.assertIn('data-person="3"', deaths)
        self.assertNotRegex(deaths, r"<td[^>]*></td>")
        for t in (reading.births_table_v2(self.d), reading.people_table_v2(self.d)):
            self.assertNotRegex(t, r"<td[^>]*></td>")

    def test_the_chronicle_groups_what_repeats_the_same_day(self):
        ev = [{"day": 4, "type": "rumeur", "tribe": "A", "who": [i], "text": "t", "order": 7} for i in range(3)]
        ev += [{"day": 4, "type": "mort-bg", "tribe": "A", "who": [9], "text": "m", "order": 0},
               {"day": 5, "type": "rumeur", "tribe": "B", "who": [1], "text": "t", "order": 7},
               {"day": 5, "type": "rumeur", "tribe": "B", "who": [2], "text": "t", "order": 7}]
        days = reading.chronicle_groups(ev)
        self.assertEqual([d for d, _ in days], [4, 5])
        self.assertEqual(days[0][1][0][0], "group")
        self.assertEqual(len(days[0][1][0][2]), 3)
        self.assertEqual(days[0][1][1][0], "one")
        self.assertEqual([kind for kind, *_ in days[1][1]], ["one", "one"])   # two the same day stay apart

    def test_the_chronicle_is_sent_a_year_at_a_time(self):
        events = reading.chronicle_events(self.d, 1)
        kinds = {e["type"] for e in events}
        self.assertTrue({"garde", "abandon", "essai", "lecon", "rumeur"} <= kinds, kinds)
        self.assertTrue(all(1 <= e["day"] <= 50 for e in events))
        self.assertIn("mort-want", {e["type"] for e in reading.chronicle_events(self.d, 2)})
        y = json.loads(reading.chronicle_json(self.d, 1))
        self.assertEqual(set(y), {"lead", "chips", "seasons"})
        self.assertIn('class="points"', y["lead"])
        self.assertEqual(json.loads(reading.chronicle_json(self.d, 99))["lead"], json.loads(reading.chronicle_json(self.d, 2))["lead"])
        page = reading.chronicle_html(self.d, 1, "/chronique/s/r")
        self.assertIn('data-chron="/chronique/s/r"', page)
        self.assertNotIn("<li class=day>", page)   # the year comes on demand
        self.assertIn('data-year="2"', page)

    def test_the_card_tells_a_life(self):
        card = reading.person_card(self.d, 1)
        for part in ('class="tr tA"', "personne 1", "<svg class=frise", "Ce qu’elle a essayé (2, 1 gardé)", "chercher plus loin",
                     "Ce qu’elle a raconté au feu (1)", "lui a fait du bien ; 1 personne l’a cru", "vivante à la fin"):
            self.assertIn(part, card)
        dead = reading.person_card(self.d, 3)
        self.assertIn("mort de faim", dead)
        self.assertIn("introuvable", reading.person_card(self.d, 99))

    def test_the_page_is_light_and_its_pieces_come_on_demand(self):
        page = reading.run_body({"name": "s", "version": None, "runs": []}, {"name": "r", "state": "fini", "seed": 1}, self.d)
        self.assertNotIn("data:image/png", page)
        self.assertNotIn('"positions"', page)
        self.assertIn('data-morceau="/morceau/s/r"', page)
        self.assertIn('data-fiche="/fiche/s/r"', page)
        carte = reading.run_piece(self.d, "carte")
        self.assertIn("data:image/png", carte)
        self.assertIn('"heat"', carte)
        self.assertIsNone(reading.run_piece(self.d, "rien"))

    def test_the_places_of_each_year_or_of_the_end(self):
        end = {"A": {(1, 1): True, (2, 1): False}, "B": {}}
        known, by_end = reading.known_places_by_year(self.d, end)
        self.assertTrue(by_end)
        self.assertEqual(known["1"]["A"], [[1, 1, 1], [2, 1, 0]])
        d = fixture(fire=True)
        d["years"][0]["knowledge"]["where"] = [[3, 4, 1], [5, 6, 0]]
        known, by_end = reading.known_places_by_year(d, end)
        self.assertFalse(by_end)
        self.assertEqual(known, {"1": {"A": [[3, 4, 1], [5, 6, 0]]}})

    def test_the_control_is_the_same_seed_without_the_fire_else_without_trials(self):
        runs = [{"name": "b-s1-feu", "base": "b", "seed": 1, "trials": True, "fire": True},
                {"name": "b-s1-sansfeu", "base": "b", "seed": 1, "trials": True, "fire": False},
                {"name": "b-s2-feu", "base": "b", "seed": 2, "trials": True, "fire": True},
                {"name": "t-s1-avec", "base": "t", "seed": 1, "trials": True, "fire": True},
                {"name": "t-s1-sans", "base": "t", "seed": 1, "trials": False, "fire": True}]
        s = {"runs": runs}
        self.assertEqual(reading.control_of(s, runs[0]), (runs[1], "sans le feu"))
        self.assertEqual(reading.control_of(s, runs[1]), (None, None))
        self.assertEqual(reading.control_of(s, runs[2]), (None, None))
        self.assertEqual(reading.control_of(s, runs[3]), (runs[4], "sans les épreuves"))

    def test_the_tribes_add_up_to_the_totals(self):
        for key in ("alive", "dead", "births", "trials"):
            self.assertEqual(sum(v[key] for v in self.bt.values()), self.sm[{"dead": "dead"}.get(key, key)], key)
        self.assertEqual(sum(v["rumours"] for v in self.bt.values()), 4)   # every year's dispelled, not the rumours kept one by one
        self.assertEqual(reading.tribe_side(self.d, "A"), "du nord-ouest")
        self.assertEqual(reading.tribe_side(self.d, "B"), "du sud-est")   # (50, 50) on 64, y down


class Expansion(unittest.TestCase):
    def test_seeds_times_with_and_without(self):
        runs = lab.expand({"runs": [{"name": "v", "args": "--world vivarium --people 25",
                                     "seeds": [1, 2], "with_and_without_trials": True}]})
        self.assertEqual([r["name"] for r in runs], ["v-s1-avec", "v-s1-sans", "v-s2-avec", "v-s2-sans"])
        self.assertEqual(runs[0]["args"], ["--world", "vivarium", "--people", "25", "--seed", "1"])
        self.assertEqual(runs[1]["args"][-3:], ["--seed", "1", "--no-trials"])
        self.assertEqual([r["trials"] for r in runs], [True, False, True, False])
        self.assertEqual([r["seed"] for r in runs], [1, 1, 2, 2])
        self.assertTrue(all(r["base"] == "v" for r in runs))

    def test_with_and_without_fire_doubles_again(self):
        runs = lab.expand({"runs": [{"name": "v", "args": "--world vivarium", "seeds": [1],
                                     "with_and_without_trials": True, "with_and_without_fire": True}]})
        self.assertEqual([r["name"] for r in runs], ["v-s1-avec-feu", "v-s1-avec-sansfeu", "v-s1-sans-feu", "v-s1-sans-sansfeu"])
        self.assertEqual([(r["trials"], r["fire"]) for r in runs], [(True, True), (True, False), (False, True), (False, False)])
        self.assertEqual(runs[1]["args"][-3:], ["--seed", "1", "--no-fire"])
        self.assertEqual(runs[3]["args"][-4:], ["--seed", "1", "--no-trials", "--no-fire"])
        alone = lab.expand({"runs": [{"name": "f", "seeds": [2], "with_and_without_fire": True}]})
        self.assertEqual([r["name"] for r in alone], ["f-s2-feu", "f-s2-sansfeu"])
        self.assertTrue(all(r["fire"] for r in lab.expand({"runs": [{"name": "d", "seeds": [1]}]})))

    def test_seeds_only(self):
        runs = lab.expand({"runs": [{"name": "p", "args": ["--world", "patch"], "seeds": [7]}]})
        self.assertEqual([(r["name"], r["args"]) for r in runs], [("p-s7", ["--world", "patch", "--seed", "7"])])

    def test_seed_in_args_wins_and_no_trials_in_args_is_a_control(self):
        runs = lab.expand({"runs": [{"name": "c", "args": "--seed 9 --no-trials", "seeds": [1, 2],
                                     "with_and_without_trials": True}]})
        self.assertEqual([r["name"] for r in runs], ["c-s9-avec", "c-s9-sans"])
        self.assertEqual(runs[1]["args"].count("--no-trials"), 1)
        self.assertEqual([r["trials"] for r in runs], [False, False])

    def test_default_seed_and_empty_plan(self):
        self.assertEqual(lab.expand({"runs": [{"name": "d"}]})[0]["args"], ["--seed", "1"])
        self.assertEqual(lab.expand({}), [])


class Commands(unittest.TestCase):
    def test_runs_side_by_side_get_no_helpers_and_a_run_alone_keeps_the_default(self):
        with tempfile.TemporaryDirectory() as d:
            plan = {"out": d, "runs": [{"name": "t", "args": "--world vivarium --people 2 --days 1",
                                        "seeds": [1, 2]}]}
            many = lab.Series(plan, 2, "/bin/true", d, version={})
            cmd = many.command(many.runs[0])
            self.assertEqual(cmd[cmd.index("--helpers") + 1], "0")
            self.assertEqual(cmd[-1], os.path.join(d, many.runs[0]["name"] + ".html"))
            one_at_a_time = lab.Series(plan, 1, "/bin/true", d, version={})
            self.assertNotIn("--helpers", one_at_a_time.command(one_at_a_time.runs[0]))
            said = {"out": d, "runs": [{"name": "t", "args": "--world vivarium --people 2 --helpers 2",
                                        "seeds": [1, 2]}]}
            kept = lab.Series(said, 2, "/bin/true", d, version={})
            cmd = kept.command(kept.runs[0])
            self.assertEqual(cmd.count("--helpers"), 1)
            self.assertEqual(cmd[cmd.index("--helpers") + 1], "2")


class Seeds(unittest.TestCase):
    def test_lists_ranges_and_mixes(self):
        self.assertEqual(lab.parse_seeds("1,2,3"), [1, 2, 3])
        self.assertEqual(lab.parse_seeds("1-4"), [1, 2, 3, 4])
        self.assertEqual(lab.parse_seeds(" 1 - 3, 7 ; 3"), [1, 2, 3, 7])
        self.assertEqual(lab.parse_seeds("5 6"), [5, 6])

    def test_refusals(self):
        for bad in ("", "a", "3-1", "1-5000", "1,,x"):
            with self.assertRaises(ValueError, msg=bad):
                lab.parse_seeds(bad)


class YearLines(unittest.TestCase):
    LOG = ("world born in 6 s\nyear 1: 48 alive, 2 dead so far, 31 s\n"
           "year 2: 45 alive, 7 dead so far, 63 s\nnoise\n44 alive, 9 dead, report written\n")

    def test_every_year_line_is_read(self):
        self.assertEqual(lab.year_lines(self.LOG), [(1, 48, 2, 31), (2, 45, 7, 63)])
        self.assertEqual(lab.year_lines(""), [])

    def test_final_counts_take_the_last_match(self):
        self.assertEqual(lab.final_counts(self.LOG), (44, 9))
        self.assertIsNone(lab.final_counts("nothing yet"))


class StateMachine(unittest.TestCase):
    def test_states(self):
        self.assertEqual(lab.run_state({}, None, False), "attend")
        self.assertEqual(lab.run_state({"started": 1.0}, None, True), "en cours")
        self.assertEqual(lab.run_state({"started": 1.0}, None, False), "perdu")
        self.assertEqual(lab.run_state({"started": 1.0}, "0\n", False), "fini")
        self.assertEqual(lab.run_state({"started": 1.0}, "101", False), "ÉCHEC(101)")
        self.assertEqual(lab.run_state({"started": 1.0, "stopped": True}, "-15", False), "arrêté")
        self.assertEqual(lab.run_state({"stopped": True}, None, False), "arrêté")

    def test_run_status_reads_the_files(self):
        with tempfile.TemporaryDirectory() as d:
            run = {"name": "r", "started": 100.0}
            with open(os.path.join(d, "r.log"), "w") as f:
                f.write("year 1: 4 alive, 1 dead so far, 2 s\n")
            s = lab.run_status(d, run, 110.0, True)
            self.assertEqual((s["state"], s["year"], s["alive"], s["dead"], s["elapsed"]), ("en cours", 1, 4, 1, 10))
            with open(os.path.join(d, "r.log"), "a") as f:
                f.write("3 alive, 2 dead\n")
            with open(os.path.join(d, "r.exit"), "w") as f:
                f.write("0")
            run["ended"] = 105.0
            s = lab.run_status(d, run, 110.0, False)
            self.assertEqual((s["state"], s["alive"], s["dead"], s["elapsed"]), ("fini", 3, 2, 5))


class Form(unittest.TestCase):
    FIELDS = {"name": "essai", "question": "Q ?", "world": "vivarium", "tribes": "2", "people": "25",
              "children": "3", "infants": "0", "ages": "18-40", "fed": "on", "days": "1000",
              "seeds": "1-3", "both": "on", "cadence": "1", "jobs": "4"}

    def test_plan_from_form_keeps_soul_runs_format(self):
        plan = lab.plan_from_form(self.FIELDS, "/runs", "/bin/viv", "/c", now=0)
        self.assertEqual(plan["out"], "/runs/essai")
        self.assertEqual(plan["binary"], "/bin/viv")
        run = plan["runs"][0]
        self.assertEqual(run["name"], "vivarium-25")
        self.assertEqual(run["args"], "--world vivarium --tribes 2 --people 25 --children 3 --fed --days 1000")
        self.assertEqual(run["seeds"], [1, 2, 3])
        self.assertTrue(run["with_and_without_trials"])
        self.assertFalse(run["with_and_without_fire"])
        self.assertEqual(plan["lab"]["jobs"], 4)
        self.assertEqual(len(lab.expand(plan)), 6)
        with_fire = lab.plan_from_form(dict(self.FIELDS, both_fire="on"), "/runs", "/bin/viv", "/c", now=0)
        self.assertTrue(with_fire["runs"][0]["with_and_without_fire"])
        self.assertEqual(len(lab.expand(with_fire)), 12)
        self.assertEqual(lab.form_from_plan(with_fire)["both_fire"], "on")
        self.assertEqual(lab.form_from_plan({"runs": [{"name": "x", "with_and_without_fire": True}]})["both_fire"], "on")
        self.assertEqual(lab.form_from_plan(plan)["both_fire"], "")

    def test_defaults_are_left_out_and_patch_side_is_kept(self):
        f = dict(lab.FORM_DEFAULTS, name="p", side="80", cadence="10")
        plan = lab.plan_from_form(f, "/runs", "/b", "/c")
        self.assertEqual(plan["runs"][0]["args"], "--world patch --side 80 --people 5 --days 10 --cadence 10")
        self.assertIsNone(plan["lab"]["jobs"])

    def test_refusals_in_french(self):
        for key, bad in (("name", "../x"), ("world", "mars"), ("tribes", "3"), ("ages", "30-20"),
                         ("days", "0"), ("seeds", ""), ("people", "x"), ("side", "4")):
            with self.assertRaises(ValueError, msg=key) as cm:
                lab.plan_from_form(dict(self.FIELDS, **{key: bad}), "/runs", "/b", "/c")
            self.assertTrue(cm.exception.args[0])

    def test_form_round_trip_and_soul_runs_plan(self):
        plan = lab.plan_from_form(self.FIELDS, "/runs", "/b", "/c")
        back = lab.form_from_plan(plan)
        for k, v in self.FIELDS.items():
            self.assertEqual(back[k], "1,2,3" if k == "seeds" else v, k)
        old = {"question": "q", "out": "~/runs/old",
               "runs": [{"name": "v25", "args": "--world vivarium --tribes 2 --people 25 --days 1000 --fed",
                         "seeds": [1, 2], "with_and_without_trials": True}]}
        f = lab.form_from_plan(old)
        self.assertEqual((f["name"], f["world"], f["tribes"], f["people"], f["days"], f["fed"], f["seeds"], f["both"]),
                         ("old", "vivarium", "2", "25", "1000", "on", "1,2", "on"))


class Reading(unittest.TestCase):
    def setUp(self):
        self.d = fixture()

    def test_summary_reads_the_cards(self):
        sm = reading.summary(self.d)
        self.assertEqual((sm["alive"], sm["dead"], sm["births"], sm["start"]), (3, 2, 1, 4))
        self.assertEqual((sm["deaths"]["hunger"], sm["deaths"]["cold"], sum(sm["deaths"].values())), (1, 1, 2))
        self.assertEqual((sm["trials"], sm["kept"], sm["copied"], sm["homes_moved"], sm["years"]), (2, 1, 1, 0, 2))
        self.assertEqual(reading.tribes(self.d), ["A", "B"])
        self.assertEqual(reading.causes(self.d)[:3], ["hunger", "thirst", "cold"])
        del self.d["years"]
        self.assertEqual(reading.causes(self.d), ["hunger", "cold"])

    def test_kinds_come_from_the_data_and_unknown_ones_keep_their_key(self):
        self.assertEqual(reading.kinds(self.d), ["wider", "sleep_out"])
        self.d["found"][1]["kept"]["tent"] = 1
        self.assertEqual(reading.kinds(self.d), ["wider", "sleep_out", "tent"])
        self.assertEqual(reading.kind_fr("tent"), "tent")
        self.assertIn("tent", reading.found_table(self.d, "B"))

    def test_the_story_of_techniques(self):
        disc = reading.discoveries(self.d, "A")
        self.assertEqual([(x["kind"], x["tried"], x["kept"], x["copied"], x["first"], x["holders"]) for x in disc],
                         [("wider", 1, 1, 1, (15, "person 1"), 1), ("sleep_out", 1, 0, 0, None, 0)])
        story = reading.story_html(self.d, "A")
        self.assertIn("La tribu A a tenté 2 essais ; 1 essai a passé le jugement", story)
        self.assertIn("Ce qui reste à la fin : <b>chercher plus loin</b> (1 vivant la porte)", story)
        self.assertIn("Essayé sans succès : dormir dehors (1 essai)", story)
        self.assertIn("2 vivants à la fin (2 esprits lus)", reading.culture_line(self.d, "A"))
        self.assertIn("La tribu B n’a rien essayé", reading.story_html(self.d, "B"))
        self.assertIn("étaient coupées", reading.story_html(fixture(trials=False), "A"))
        table = reading.found_table(self.d, "A")
        self.assertIn("<th>Gardée la première fois</th>", table)
        self.assertIn("<td>an 1, été, jour 15</td><td>personne 1</td><td class=n>1</td><td class=n>1</td>", table)
        self.assertIn("deux fois plus loin du camp", table)
        self.assertEqual(table.count("<tr>"), 2)                    # the header and the one kept kind
        self.assertEqual(reading.found_table(self.d, "B"), "")
        self.assertIn("jour 15, personne 1 : chercher plus loin, garde", reading.kept_list(self.d, "A"))

    def test_the_verdict_in_everyday_french(self):
        v = reading.verdict(self.d)
        self.assertEqual(v, "2 morts de manque : 1 de faim, 1 de froid, en hiver ; aucun mort de fond. "
                            "Après 2 ans, 3 des 4 du départ sont vivants, et l’enfant né en route n’a pas survécu. Une technique gardée : chercher plus loin.")
        self.assertTrue(reading.verdict(fixture(trials=False)).endswith("Les épreuves étaient coupées (c’est le témoin) : rien à garder."))
        for p in self.d["people"]:
            p["died"] = p["died"] or {"day": 95, "cause": "thirst", "season": "winter", "at": [1, 1]}
        self.assertTrue(reading.verdict(self.d).startswith("5 morts de manque : 1 de faim, 3 de soif, 1 de froid, en hiver ; aucun mort de fond. "
                                                          "Après 2 ans, plus personne : les 4 du départ sont morts, et l’enfant né en route aussi."))
        self.assertIn("Aucune technique", reading.verdict(self.d))
        alone = fixture()
        alone["people"] = alone["people"][:2]
        alone["years"] = alone["years"][:2]
        alone["found"] = alone["found"][:1]
        self.assertIn("Personne n’est mort.", reading.verdict(alone))

    def test_deaths_by_want_stand_apart_from_the_background(self):
        sm = reading.summary(self.d)
        self.assertEqual((sm["want"], sm["background"]), (2, 0))
        self.assertEqual(sm["want_by"], {"hunger": 1, "thirst": 0, "cold": 1})
        self.assertEqual(sm["background_by"], {"illness": 0, "old_age": 0, "childbirth": 0})
        # an illness, an old age, a cause the lab does not know: all background
        self.d["people"][1]["died"] = {"day": 60, "tick": 1440, "cause": "illness", "season": "summer", "at": [10, 10]}
        self.d["people"][4]["died"] = {"day": 80, "tick": 1920, "cause": "plague", "season": "autumn", "at": [50, 50]}
        self.d["people"][0]["died"] = {"day": 85, "tick": 2040, "cause": "old_age", "season": "autumn", "at": [50, 50]}
        sm = reading.summary(self.d)
        self.assertEqual((sm["want"], sm["background"], sm["dead"]), (2, 3, 5))
        self.assertEqual(sm["background_by"], {"illness": 1, "old_age": 1, "childbirth": 0, "plague": 1})
        self.assertTrue(reading.verdict(self.d, sm).startswith(
            "2 morts de manque : 1 de faim, 1 de froid, en hiver ; 3 morts de fond : maladie 1, vieillesse 1, plague 1. Après 2 ans, plus personne"))
        # the want deaths spread over seasons: "le plus souvent"
        self.d["people"][2]["died"]["season"] = "summer"
        self.d["people"][3]["died"]["season"] = "summer"
        self.d["people"][4]["died"]["cause"] = "thirst"
        self.d["people"][4]["died"]["season"] = "winter"
        self.assertTrue(reading.verdict(self.d).startswith("3 morts de manque : 1 de faim, 1 de soif, 1 de froid, le plus souvent en été ; 2 morts de fond : maladie 1, vieillesse 1."))
        # only background deaths
        for p in self.d["people"]:
            if p["died"] and reading.is_want(p["died"]["cause"]):
                p["died"]["cause"] = "illness"
        self.assertTrue(reading.verdict(self.d).startswith("Aucun mort de manque ; 5 morts de fond : maladie 4, vieillesse 1."))

    def test_the_stock_tile_reads_the_food_of_the_last_year(self):
        st = reading.stock(self.d)
        self.assertEqual((st["kj"], st["start_kj"], st["by_tribe"], st["year"]), (1000, 0, {"A": 500, "B": 500}, 2))
        self.assertAlmostEqual(st["days_per_person"], 1000 / 3 / 10000)
        tiles = reading.tiles_html(reading.summary(self.d))
        self.assertIn(f'<div class="tile stock"><div class=v>{reading.fmt(1000)}</div><div class=k>nourriture stockée (kJ)</div>', tiles)
        self.assertIn("≈ 0 jour de vivres par vivant · début de l’an 2 : 0 · A 500, B 500", tiles)
        other = reading.summary(fixture())
        other["stock"]["kj"] = 400
        self.assertIn(f'<div class="tile stock"><div class=v>{reading.fmt(1000)}<span class="arrow good" title="+600', reading.tiles_html(reading.summary(self.d), other))
        for y in self.d["years"]:
            del y["food"]
        sm = reading.summary(self.d)
        self.assertIsNone(sm["stock"])
        tiles = reading.tiles_html(sm, other)
        self.assertIn(f'<div class="tile stock"><div class="v text">{reading.NO_STOCK}</div>', tiles)
        self.assertIn("aucune clé <code>years[].food</code>", tiles)
        self.assertNotIn("arrow", tiles.split('class="tile stock"')[1].split("</div></div>")[0])

    def test_days_ages_and_places_in_words(self):
        self.assertEqual(reading.day_fr(self.d, 112), "an 3, printemps, jour 112")
        self.assertEqual(reading.day_fr(self.d, 50), "an 1, hiver, jour 50")
        self.assertEqual([reading.season_of(self.d, day) for day in (1, 13, 14, 26, 38, 39)],
                         ["spring", "spring", "summer", "autumn", "autumn", "winter"])
        self.assertEqual(reading.age_fr(self.d, 7300 + 99), "147 ans")      # game years of 50 days
        self.assertEqual(reading.age_fr(self.d, 40), "40 jours")
        self.assertEqual(reading.age_days(self.d, self.d["people"][2], 70), 40)   # born day 30, dead day 70
        self.assertEqual(reading.age_days(self.d, self.d["people"][0]), 7300 + 99)
        self.assertEqual(reading.where_fr(self.d, "A", [11, 12], 70), "au camp")
        self.assertEqual(reading.where_fr(self.d, "A", [200, 10], 70), "à 190 m du camp, vers l’est")
        self.assertEqual(reading.where_fr(self.d, "A", [12, 150], 100), "à 139 m du camp, vers le sud")
        self.assertEqual(reading.where_fr(self.d, "A", None), "-")

    def test_the_three_tables_of_the_run_page(self):
        deaths = reading.deaths_table(self.d)
        self.assertIn("<h2>Les morts</h2>", deaths)
        self.assertIn("<th>Qui</th><th>Quand</th><th class=n>Âge à la mort</th><th>De quoi</th><th>Où</th>", deaths)
        self.assertEqual(deaths.count("<tr>"), 3)
        self.assertIn("<td>personne 3 (A)</td><td>an 2, été, jour 70</td><td class=n>40 jours</td>", deaths)
        self.assertIn(">faim</span></td><td>au camp</td>", deaths)
        births = reading.births_table(self.d)
        self.assertIn("<h2>Les naissances</h2>", births)
        self.assertEqual(births.count("<tr>"), 2)
        self.assertIn("<td>personne 3 (A)</td><td>an 1, automne, jour 30</td><td>-</td><td>-</td><td><span class=dead>non</span> : mort de faim à 40 jours</td>", births)
        people = reading.people_end_table(self.d)
        self.assertIn("<h2>Les gens à la fin</h2>", people)
        self.assertEqual(people.count("<tr>"), 6)
        rows = people.split("<tr>")
        self.assertIn("vivante", rows[2])                 # the living first (rows[1] is the header)
        self.assertIn("<td>chercher plus loin</td>", rows[2])
        self.assertIn("<span class=dead>mort</span>, an 2, été, jour 70 (faim)", people)
        empty = fixture()
        empty["people"] = [p for p in empty["people"] if not p["died"] and p["born"] == "start"]
        self.assertIn("Personne n’est mort.", reading.deaths_table(empty))
        self.assertIn("Aucune naissance.", reading.births_table(empty))

    def test_tiles_count_the_techniques_and_point_the_arrows(self):
        sm = reading.summary(self.d)
        self.assertEqual((sm["techniques"], sm["alive_start"]), (1, 3))
        tiles = reading.tiles_html(sm)
        self.assertEqual(tiles.count('<div class="tile'), 6)
        self.assertEqual([c for c in ("alive", "want", "background", "births", "stock", "kept") if f'<div class="tile {c}">' in tiles],
                         ["alive", "want", "background", "births", "stock", "kept"])
        self.assertNotIn('<div class="tile dead">', tiles)
        self.assertIn('<div class=v>2</div><div class=k>morts de manque</div><div class=s><span class="cause" style="color:#c0392b">faim</span> 1 · '
                      '<span class="cause" style="color:#2980b9">soif</span> 0 · <span class="cause" style="color:#7f8c8d">froid</span> 1</div>', tiles)
        self.assertIn('<div class=v>0</div><div class=k>morts de fond</div><div class=s><span class="cause" style="color:#8e44ad">maladie</span> 0 · ', tiles)
        self.assertNotIn("arrow", tiles)
        other = reading.summary(fixture(trials=False))
        tiles = reading.tiles_html(sm, other)
        self.assertIn('class="arrow good" title="+1 par rapport à l’autre côté">↑ +1', tiles)
        other["want"] = 1
        self.assertIn('class="arrow bad"', reading.tiles_html(sm, other))      # one more death by want: worse
        other["want"], other["background"] = 2, 1
        self.assertIn('class="arrow good" title="-1 par rapport à l’autre côté">↓ -1', reading.tiles_html(sm, other))

    def test_the_fire_sections_read_the_keys_of_d55(self):
        d = fixture(fire=True)
        self.assertTrue(reading.has_fire_data(d))
        self.assertFalse(reading.has_fire_data(self.d))
        self.assertIn("avec feu", reading.settings_fr(d))
        self.assertIn("sans feu (témoin", reading.settings_fr({"meta": {"settings": {"fire": False}}}))
        sm = reading.summary(d)
        self.assertEqual((sm["heard_trials"], sm["heard_copies"], sm["dispelled"]), (1, 1, 2))
        self.assertEqual(sm["knowledge"]["places"], 65)
        self.assertEqual(sm["knowledge"]["by_tribe"], {"A": 60, "B": 5})
        self.assertAlmostEqual(sm["knowledge"]["heard_share"], 0.75)
        know = reading.knowledge_html(d)
        self.assertIn("<h2>Ce que la tribu sait</h2>", know)
        self.assertIn("65 lieux connus (tribu A 60, tribu B 5), dont 3 que personne de vivant n'a vus lui-même ; 75% des souvenirs", know)
        self.assertEqual(know.count("<svg"), 2)
        self.assertIn("<td class=n>40</td><td class=n>120</td><td class=n>300</td><td class=n>2</td>", know)
        dared = reading.dared_html(d)
        self.assertIn("<h2>Qui a osé quoi</h2>", dared)
        self.assertIn("<td>personne 1</td><td>A</td><td class=n>10 → 50</td><td class=n>2</td><td class=n>1</td><td class=n>1</td>", dared)
        self.assertIn("frileux (0–20)", dared)
        nights = reading.nights_html(d)
        self.assertIn("Tribu A, 2 nuits gardées au foyer", nights)
        self.assertIn("personne 1 : un essai gardé le jour 15 : la distance de recherche en plus", nights)
        self.assertIn("personne 1 : un lieu (16, 16), 1 sorte de nourriture, vu le jour 12", nights)
        self.assertIn("personne 2 : un lieu (32, 16), nu, vu le jour 40", nights)
        rum = reading.rumours_html(d)
        self.assertIn("an 1 : 1 rumeur dissipée", rum)
        self.assertIn("an 2 : 1 rumeur dissipée", rum)
        self.assertIn("jour 40, personne 2 va où personne 1 avait dit (32, 16) et ne trouve rien", rum)
        self.assertIn("aucune rumeur dissipée", reading.rumours_html(self.d))
        bel = reading.believes_html(d, d["people"][0])
        self.assertIn("Ce qu'il croit : 2 lieux, 1 vu, 1 entendu", bel)
        self.assertIn("<td>(32, 16)</td><td>entendu de personne 2</td><td class=n>30</td>", bel)
        self.assertIn("non : un souvenir plus récent le contredit", bel)
        self.assertIn("<td>(16, 16)</td><td>vu lui-même</td><td class=n>12</td><td>un lieu (16, 16), 1 sorte de nourriture, vu le jour 12</td><td>oui</td>", bel)
        self.assertEqual(reading.believes_html(d, d["people"][1]), "")
        page = reading.run_body({"name": "s", "version": None}, {"name": "r", "state": "fini", "seed": 1}, d)
        for section in ("Ce que la tribu sait", "Qui a osé quoi"):
            self.assertIn(section, page)
        annexes = reading.run_piece(d, "annexes")   # the raw tables come when the annexes open
        for section in ("<h3>Qui a dit quoi à qui", "<h3>Rumeurs", "Ce qu'il croit"):
            self.assertIn(section, annexes)
        for section in ("<h3>Qui a dit quoi à qui", "<h3>Rumeurs"):
            self.assertNotIn(section, page)
        for part in ('id="t-savoir"', 'id="t-ose"', "Rumeurs dissipées"):   # the run page's own short tables
            self.assertIn(part, page)
        self.assertIn("10 → 50", reading.people_table(d))
        self.assertIn("porté à midi moy. / min", reading.years_table(d))
        self.assertIn("<td class=n>400 / 100</td><td class=n>15\u202f000 / 15\u202f000 / 30\u202f000</td>", reading.years_table(d))
        old = reading.run_body({"name": "s", "version": None}, {"name": "r", "state": "fini", "seed": 1}, self.d)
        self.assertNotIn("Ce que la tribu sait", old)

    def test_settings_in_french(self):
        s = reading.settings_fr(self.d)
        self.assertIn("monde vivarium, 2 tribus, 2 adultes", s)
        self.assertIn("100 jours (2 ans), graine 1, avec épreuves", s)
        self.assertIn("sans épreuves (témoin)", reading.settings_fr(fixture(trials=False)))

    def test_tables_build(self):
        years = reading.years_table(self.d)
        self.assertIn("<th>faim</th>", years)
        self.assertEqual(years.count("<tr>"), 5)
        people = reading.people_table(self.d)
        self.assertEqual(people.count("<details>"), 5)
        self.assertIn("jour 70, faim (hiver)", people)
        self.assertEqual(reading.hours_to_days(36), 1.5)

    def test_map_draws_trail_heat_and_deaths(self):
        html_ = reading.map_svg(self.d, "m")
        svg = html_[html_.index("<svg"):html_.index("</svg>") + 6]
        self.assertIn('viewBox="0 0 64 64"', svg)
        self.assertEqual(svg.count("<g data-year="), 2)
        self.assertIn('data-year="1" display="none"', svg)
        self.assertNotIn('data-year="2" display="none"', svg)
        import xml.etree.ElementTree as ET
        ET.fromstring(svg)                                     # well-formed: it is drawn inline
        self.assertIn("points=\"10,10 10,10 12,11\"", svg)        # tribe A: start, year 1, year 2
        self.assertEqual(svg.count("data-camp-year="), 6)       # start and two years, two tribes
        self.assertEqual(svg.count("meurt de"), 2)
        self.assertIn('<g data-death="70"', svg)
        self.assertIn('<g class="dots"></g>', svg)
        self.assertIn('<input type="range" min="1" max="100" value="100"', html_)
        self.assertIn("an 2, hiver, jour 100", html_)
        self.assertIn(reading.NO_POSITIONS, html_)
        self.assertIn('data-heat checked', html_)                # without positions, the heatmap is shown
        payload = json.loads(html_[html_.index('<script type="application/json">') + 32:html_.rindex("</script>")])
        self.assertEqual((payload["days"], payload["dpy"], payload["lastYear"], payload["positions"]), (100, 50, 2, []))
        self.assertEqual(payload["tribe"]["4"], "B")
        self.assertEqual(payload["names"]["1"], "personne 1")
        self.d["map"] = {}
        self.assertIn(reading.NO_DATA, reading.map_svg(self.d, "m"))

    def test_map_reads_the_positions_fixture(self):
        # `positions`: one sample a day at noon, [[day, [[id, x, y], …]], …] (docs/10, being added)
        self.d["positions"] = [[2, [[1, 12, 10], [2, 11, 11], [5, 52, 50]]], [1, [[1, 10, 10], [2, 11, 10], [4, 50, 50], [5, 51, 50]]]]
        html_ = reading.map_svg(self.d, "m")
        self.assertNotIn(reading.NO_POSITIONS, html_)
        self.assertIn('data-heat>', html_)                      # with positions, the heatmap is an option, off
        payload = json.loads(html_[html_.index('<script type="application/json">') + 32:html_.rindex("</script>")])
        self.assertEqual([p[0] for p in payload["positions"]], [1, 2])           # sorted by day
        self.assertEqual(payload["positions"][1][1][0], [1, 12, 10])
        self.assertEqual(reading.positions_of({"positions": "nope"}), [])
        self.assertEqual(reading.positions_of({}), [])

    def test_store_reads_and_caches(self):
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "s"))
            store = reading.Store(d)
            self.assertIsNone(store.load("s", "r"))
            with open(os.path.join(d, "s", "r.json"), "w") as f:
                json.dump(self.d, f)
            self.assertEqual(store.load("s", "r")["meta"]["format"], 1)
            self.assertIs(store.load("s", "r"), store.load("s", "r"))
            with open(os.path.join(d, "s", "bad.json"), "w") as f:
                f.write("{not json")
            self.assertIsNone(store.load("s", "bad"))


class Curves(unittest.TestCase):
    def test_scale_maps_the_span_and_a_flat_span(self):
        x = reading.Scale(0, 100, 50, 650)
        self.assertEqual((x(0), x(50), x(100)), (50, 350, 650))
        y = reading.Scale(0, 40, 196, 12)
        self.assertEqual(y(0), 196)
        self.assertEqual(y(40), 12)
        self.assertAlmostEqual(y(20), 104)
        self.assertEqual(reading.Scale(5, 5, 0, 10)(5), 0)

    def test_nice_max_and_ticks(self):
        self.assertEqual([reading.nice_max(v) for v in (0, 1, 7, 23, 50, 170, 1000)], [1, 1, 10, 25, 50, 200, 1000])
        self.assertEqual(reading.y_ticks(8), [0, 2, 4, 6, 8])

    def test_charts_build_with_axes_legend_and_hover(self):
        d = fixture()
        pop = reading.population_chart(d).svg("pop")
        self.assertEqual(pop.count("<polyline"), 2)
        self.assertIn("tribu A", pop)
        self.assertIn("vivants", pop)
        self.assertIn("an 1, jour 50 : 3", pop)          # the hover title at the year's end
        bd = reading.births_deaths_chart(d).svg("bd")
        self.assertIn("an 1 : 1 naissance", bd)
        self.assertIn("an 2 : 1 mort de faim", bd)
        self.assertIn("an 2 : 1 mort de froid", bd)
        self.assertIn("essais gardés", reading.kept_chart(d).svg("k"))
        self.assertIn("réserve de graisse", reading.reserve_chart(d).svg("r"))
        self.assertEqual(reading.year_marks(d, None, "alive"), ([50], [5], ["an 1, jour 50 : 5"]))

    def test_the_want_chart_stacks_red_under_grey_per_tribe(self):
        d = fixture()
        d["years"][0]["deaths"]["illness"] = 2        # tribe A, year 1: two background deaths
        svg = reading.want_chart(d, wide=True).svg("morts par an")
        self.assertIn('viewBox="0 0 1000 150"', svg)
        self.assertIn('class="winter"', svg)
        self.assertEqual(svg.count(f'fill="{reading.WANT_COLOR}"'), 2)           # A hunger (year 2), B cold (year 2)
        self.assertEqual(svg.count(f'fill="{reading.BACKGROUND_COLOR}"'), 1)     # A illness (year 1)
        self.assertIn("<title>tribu A, an 1 : 0 mort de manque, 2 de fond (maladie 2)</title>", svg)
        self.assertIn("<title>tribu A, an 2 : 1 mort de manque, 0 de fond (faim 1)</title>", svg)
        self.assertIn("<title>tribu B, an 2 : 1 mort de manque, 0 de fond (froid 1)</title>", svg)
        self.assertIn(f'stroke="{reading.TRIBE_COLOR["A"]}"', svg)
        self.assertIn(f'stroke="{reading.TRIBE_COLOR["B"]}"', svg)
        # the same x scale as the population curve above it
        self.assertEqual(reading.want_chart(d, wide=True).x(70), reading.population_chart(d, wide=True).x(70))
        self.assertIn('viewBox="0 0 660 230"', reading.want_chart(d).svg(""))

    def test_two_runs_share_one_chart(self):
        a, b = fixture(), fixture(trials=False)
        b["curves"]["alive_by_tribe"][0] = [9] * 20
        ch = reading.population_chart(a, "A", reading.side_colors("a"))
        reading.population_chart(b, "B", reading.side_colors("b"), dash=True, chart=ch)
        self.assertEqual(ch.ytop, 10)
        self.assertEqual(len(ch.legend), 4)
        self.assertIn('stroke-dasharray', ch.svg())


class SeriesReading(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="lab-read-")
        self.v1 = {"commit": "abc1234", "branch": "feat/x", "dirty": False, "change": "v1 : le juge compte la soif"}
        self.v2 = {"commit": "def5678", "branch": "feat/y", "dirty": True, "change": "v2"}
        fake_series(self.dir, "etalon-test-1", self.v1, seeds=(1, 2))
        fake_series(self.dir, "etalon-test-2", self.v2, seeds=(1, 2), json_for=["t-s1-avec", "t-s1-sans", "t-s2-avec"])
        fake_series(self.dir, "autre", self.v1, source="autre-plan", json_for=[])
        self.lab = lab.Lab(self.dir, "/bin/true", self.dir, 2)
        self.series = self.lab.series()

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_series_carry_version_plan_and_signature(self):
        s = {x["name"]: x for x in self.series}
        self.assertEqual(s["etalon-test-1"]["version"]["commit"], "abc1234")
        self.assertEqual(s["etalon-test-1"]["plan_name"], "etalon-test-1"[:11])
        self.assertEqual(s["etalon-test-1"]["signature"], s["etalon-test-2"]["signature"])
        self.assertNotEqual(s["etalon-test-1"]["signature"], s["autre"]["signature"] + "x")
        self.assertEqual([r["seed"] for r in s["etalon-test-1"]["runs"]], [1, 1, 2, 2])
        self.assertEqual([r["trials"] for r in s["etalon-test-1"]["runs"]], [True, False, True, False])
        self.assertTrue(all(r["data"] for r in s["etalon-test-1"]["runs"]))
        self.assertEqual([r["data"] for r in s["autre"]["runs"]], [False, False])
        kin = reading.same_plan(s["etalon-test-1"], self.series)
        self.assertEqual([k["name"] for k in kin], ["etalon-test-2"])
        self.assertEqual(reading.same_plan(s["autre"], self.series), [])
        self.assertIn("abc1234", reading.version_html(self.v1))
        self.assertIn("+modifs locales", reading.version_html(self.v2))
        self.assertIn("inconnu", reading.version_html(None))

    def test_aggregates_mean_and_spread(self):
        s = next(x for x in self.series if x["name"] == "etalon-test-1")
        rows, agg = reading.series_numbers(s, self.lab.store)
        self.assertEqual(len(rows), 4)
        self.assertEqual((agg[True]["n"], agg[True]["mean"], agg[True]["min"], agg[True]["max"], agg[True]["sd"]), (2, 3.0, 3, 3, 0.0))
        self.assertEqual(reading.aggregates([2, 4, 6]), {"n": 3, "mean": 4.0, "min": 2, "max": 6, "sd": 2.0})
        self.assertIsNone(reading.aggregates([]))
        line = reading.key_line(s, self.lab.store)
        self.assertIn("4/4 runs lus", line)
        self.assertIn("avec épreuves : 3,0 vivants", line)
        self.assertIn("morts de manque : 8 (faim 4, froid 4) · morts de fond : 0 · 4 naissances", line)
        self.assertIn("4 naissances", line)
        autre = next(x for x in self.series if x["name"] == "autre")
        self.assertIn("aucune donnée lue", reading.key_line(autre, self.lab.store))

    def test_pages_build_and_a_run_without_data_says_so(self):
        s = {x["name"]: x for x in self.series}
        lire = reading.lire_body(self.series, self.lab.store)
        self.assertIn("etalon-test-1", lire)
        self.assertIn('action="/compare"', lire)
        serie = reading.serie_body(s["etalon-test-1"], self.series, self.lab.store)
        self.assertIn("Comparer avec la version…", serie)
        self.assertIn("def5678", serie)
        self.assertIn('<p class=vd>Avec essais, 3 vivants à la fin ; sans, 3 : pas de différence.</p>', serie)
        self.assertIn('<a class=rw href="/run/etalon-test-1/t-s1-avec">t · avec essais · avec le feu · graine 1</a>', serie)
        self.assertIn("<span class=rr>3 vivants · 2 morts de manque · 1 naissances", serie)
        self.assertIn("<h2>Morts de manque et de fond</h2><ul><li>avec essais : <b>2,0</b> morts de manque en moyenne sur 2 runs (de 2 à 2), et 0,0 morts de fond (de 0 à 0)</li>", serie)
        self.assertIn("<li>sans essais (témoin) : <b>3,0</b> vivants", serie)
        self.assertIn(">sans essais (témoin)</text>", serie)   # the curves named by what differs
        agg = {True: {"n": 2, "mean": 4.0, "want": {"mean": 1.0}}, False: {"n": 2, "mean": 2.0, "want": {"mean": 3.0}}}
        self.assertEqual(reading.series_verdict(agg, 4, 4), "Avec les épreuves, 4 vivants à la fin et 1 mort de manque en moyenne sur 2 runs ; "
                                                             "sans, 2 vivants et 3 morts de manque sur 2 runs. Les épreuves gardent 2 vivants de plus et épargnent 2 morts de manque.")
        agg[False]["want"]["mean"] = 0.5
        self.assertTrue(reading.series_verdict(agg, 4, 4).endswith("Les épreuves gardent 2 vivants de plus et ajoutent 0,5 mort de manque."))
        agg[True]["mean"] = 0.5
        self.assertTrue(reading.series_verdict(agg, 4, 4).endswith("Les épreuves coûtent 1,5 vivant et ajoutent 0,5 mort de manque."))
        agg[True]["mean"] = 2.0
        agg[True]["want"]["mean"] = 0.5
        self.assertTrue(reading.series_verdict(agg, 4, 4).endswith("Les épreuves ne changent rien."))
        self.assertIn("Population de tous les runs", serie)
        self.assertIn("écart-type", serie)
        run = reading.run_body(s["etalon-test-1"], s["etalon-test-1"]["runs"][0], self.lab.store.load("etalon-test-1", "t-s1-avec"))
        for part in ("<p class=hero>Après 2 ans, 3 vivants", '<ul class="points top">', "<div class=tiles6>",
                     "<h2>Les gens</h2>", "<h2>Où ils vivent</h2>", "<h2>Ce qu’ils ont appris</h2>", "<h2>La chronique</h2>", "<h2>Annexes</h2>",
                     "<h2>Les facteurs</h2>", "<h2>Les courbes</h2>", "data-maps6", "data-sm6", "<h3>Les morts</h3>", "<h3>Les naissances</h3>", "<h3>Tout le monde</h3>", "<h3>Les techniques</h3>",
                     'class="winter"', "vivants à la fin", 'data-lazy="annexes"', "<dialog id=fiche",
                     'aria-label="morts de manque et de fond par an"', "A à gauche de l’année, B à droite"):
            self.assertIn(part, run)
        self.assertLess(run.index('<h2>Les facteurs</h2>'), run.index('aria-label="morts de manque et de fond par an"'))
        order = [run.index(x) for x in ("<p class=hero>", '<ul class="points top">', "<div class=tiles6>", "<h2>Les gens</h2>", "<h3>Les morts</h3>",
                                        "<h2>Où ils vivent</h2>", "<h2>Ce qu’ils ont appris</h2>", "<h2>La chronique</h2>",
                                        "<h2>Annexes</h2>", "<details class=detail>")]
        self.assertEqual(order, sorted(order))
        empty = reading.run_body(s["autre"], s["autre"]["runs"][0], None)
        self.assertIn(reading.NO_DATA, empty)
        self.assertNotIn("La carte", empty)
        self.assertIn(reading.NO_DATA, reading.serie_body(s["autre"], self.series, self.lab.store))

    def test_compare_two_runs_highlights_the_differences(self):
        s = {x["name"]: x for x in self.series}
        da = self.lab.store.load("etalon-test-1", "t-s1-avec")
        db = self.lab.store.load("etalon-test-1", "t-s1-sans")
        body = reading.compare_runs_body(s["etalon-test-1"], s["etalon-test-1"]["runs"][0], da,
                                         s["etalon-test-1"], s["etalon-test-1"]["runs"][1], db)
        self.assertIn('class="arrow good" title="+1 par rapport à l’autre côté">↑ +1', body)    # one technique against none
        self.assertIn('class="arrow bad" title="-1 par rapport à l’autre côté">↓ -1', body)
        self.assertEqual(body.count("<svg"), 6)              # four charts, two maps
        self.assertEqual(body.count('<p class="verdict small">'), 2)
        self.assertEqual(body.count("Une technique gardée"), 1)
        self.assertEqual(body.count("Ce qui reste à la fin"), 1)
        self.assertEqual(body.count("<section class=tribe>"), 4)
        missing = reading.compare_runs_body(s["etalon-test-1"], s["etalon-test-1"]["runs"][0], da, s["autre"], s["autre"]["runs"][0], None)
        self.assertIn(reading.NO_DATA, missing)

    def test_compare_two_versions_pairs_the_runs(self):
        s = {x["name"]: x for x in self.series}
        pairs, left = reading.pair_runs(s["etalon-test-1"], s["etalon-test-2"])
        self.assertEqual([(a["name"], b["name"]) for a, b in pairs], [(n, n) for n in ("t-s1-avec", "t-s1-sans", "t-s2-avec", "t-s2-sans")])
        self.assertEqual(left, [])
        body = reading.compare_series_body(s["etalon-test-1"], s["etalon-test-2"], self.lab.store)
        self.assertIn("Comparer deux versions", body)
        self.assertIn("<p class=verdict>La version B contre la version A : avec épreuves, pareil et autant de morts de manque ; "
                      "sans épreuves, pareil et autant de morts de manque.</p>", body)
        self.assertIn("<th>vivants A → B</th><th>morts de manque</th><th>morts de fond</th><th>naissances</th>", body)
        self.assertIn("<h2>Morts de manque et de fond</h2>", body)
        agg_a = {True: {"mean": 3.0, "want": {"mean": 2.0}}}
        agg_b = {True: {"mean": 5.0, "want": {"mean": 0.5}}, False: {"mean": 1.0, "want": {"mean": 1.0}}}
        self.assertEqual(reading.compare_verdict(agg_a, agg_b), "La version B contre la version A : avec épreuves, 2 vivants de plus et 1,5 mort de manque de moins.")
        self.assertIn("graine par graine", body)
        self.assertEqual(body.count("face à face"), 4)
        self.assertIn("3 → - ", body)                        # t-s2-sans has no data in the second series
        self.assertIn("Les cultures à la fin", body)
        self.assertIn("abc1234", body)
        self.assertIn("def5678", body)
        other = reading.compare_series_body(s["etalon-test-1"], s["autre"], self.lab.store)
        self.assertIn("ne viennent pas du même plan", other)

    def test_routes_serve_the_reading_pages(self):
        import threading
        import urllib.request
        lab.Handler.lab = self.lab
        srv = lab.ThreadingHTTPServer(("127.0.0.1", 0), lab.Handler)
        lab.Handler.port = srv.server_address[1]
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        try:
            for path, code, needle in (("/lire", 200, "Lire / Comparer"), ("/serie/etalon-test-1", 200, "Comparer avec la version"),
                                       ("/run/etalon-test-1/t-s1-avec", 200, "Ce qu’ils ont appris"),
                                       ("/fiche/etalon-test-1/t-s1-avec?id=1", 200, "personne 1"),
                                       ("/fiche/etalon-test-1/t-s1-avec?id=x", 404, "introuvable"),
                                       ("/chronique/etalon-test-1/t-s1-avec?an=1", 200, '"seasons"'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=carte", 200, "data-map2"),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=annexes", 200, "Année par année"),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=rien", 404, "introuvable"),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=carte-base", 200, '"tribes"'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=carte-an&an=2", 200, '"year":2'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=relief", 200, '"known":false'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=nourriture", 200, '"cells"'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=courbes", 200, '"days"'),
                                       ("/morceau/etalon-test-1/t-s1-avec?quoi=annees", 200, "<table"),
                                       ("/api/resultats?serie=etalon-test-1", 200, '"verdict"'),
                                       ("/api/resultats?serie=..", 404, ""),
                                       ("/", 200, "Étalon"),
                                       ("/morceau/autre/t-s1-avec?quoi=carte", 404, "introuvable"),
                                       ("/run/autre/t-s1-avec", 200, reading.NO_DATA),
                                       ("/compare?a=etalon-test-1&b=etalon-test-2", 200, "Comparer deux versions"),
                                       ("/compare?a=etalon-test-1/t-s1-avec&b=etalon-test-2/t-s1-avec", 200, "Comparer deux runs"),
                                       ("/run/etalon-test-1/nope", 404, "introuvable"), ("/compare?a=../x&b=autre", 404, "invalide"),
                                       ("/serie/..", 404, "introuvable")):
                try:
                    r = urllib.request.urlopen(f"http://127.0.0.1:{lab.Handler.port}{path}")
                    status, body = r.status, r.read().decode()
                except urllib.error.HTTPError as e:
                    status, body = e.code, e.read().decode()
                self.assertEqual(status, code, path)
                self.assertIn(needle, body, path)
                self.assertNotIn("Traceback", body)
        finally:
            srv.shutdown()
            srv.server_close()


class Version(unittest.TestCase):
    def test_version_of_reads_git_or_says_inconnu(self):
        v = lab.version_of(None, "/nonexistent/vivarium", "rien")
        self.assertEqual((v["commit"], v["branch"], v["change"], v["worktree"], v["binary_mtime"]), ("inconnu", "inconnu", "rien", None, None))
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(lab.version_of(d, "/x")["commit"], "inconnu")
            if shutil.which("git") is None:
                self.skipTest("no git")
            env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t")
            import subprocess
            for cmd in (["git", "init", "-q", "-b", "feat/test"], ["git", "commit", "-q", "--allow-empty", "-m", "x"]):
                subprocess.run(cmd, cwd=d, env=env, check=True, capture_output=True)
            v = lab.version_of(d, "/x", "")
            self.assertEqual(v["branch"], "feat/test")
            self.assertEqual(len(v["commit"]), 7)
            self.assertFalse(v["dirty"])

    def test_signature_and_plan_name(self):
        p1 = {"runs": [{"name": "v", "args": "--people 2", "seeds": [1], "with_and_without_trials": True}]}
        p2 = {"runs": [{"name": "v", "args": ["--people", "2"], "seeds": [1], "with_and_without_trials": True}], "question": "autre"}
        self.assertEqual(lab.signature(p1), lab.signature(p2))
        self.assertNotEqual(lab.signature(p1), lab.signature({"runs": [{"name": "v", "args": "--people 3"}]}))
        self.assertEqual(lab.lab_plan_name({"lab": {"source": "etalon-x"}}, "s"), "etalon-x")
        self.assertEqual(lab.lab_plan_name({}, "s"), "s")

    def test_form_carries_the_change_sentence(self):
        plan = lab.plan_from_form(dict(lab.FORM_DEFAULTS, name="p", change="D50 b : la soif"), "/runs", "/b", "/c")
        self.assertEqual(plan["change"], "D50 b : la soif")
        self.assertEqual(lab.form_from_plan({"change": "x", "runs": [{"name": "r"}]})["change"], "x")

    def test_standard_plans_exist_and_expand(self):
        names = lab.Lab(tempfile.mkdtemp(prefix="lab-std-"), "/bin/true", "/c", 1).standard_plans()
        self.assertEqual(names, ["etalon-bande-25-5ans", "etalon-deux-tribus-25-20ans", "etalon-dix-graines"])
        for name, count in (("etalon-bande-25-5ans", 4), ("etalon-deux-tribus-25-20ans", 4), ("etalon-dix-graines", 40)):
            with open(os.path.join(lab.STANDARD_PLANS, name + ".json")) as f:
                plan = json.load(f)
            self.assertTrue(plan["question"])
            self.assertEqual(len(lab.expand(plan)), count, name)
            self.assertNotIn("out", plan)


class EndToEnd(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="lab-test-")
        self.binary = os.path.join(self.dir, "fake-vivarium")
        with open(self.binary, "w") as f:
            f.write(FAKE_VIVARIUM)
        os.chmod(self.binary, stat.S_IRWXU)
        self.runs = os.path.join(self.dir, "runs")
        self.lab = lab.Lab(self.runs, self.binary, self.dir, jobs=2)

    def tearDown(self):
        self.lab.stop_all()
        shutil.rmtree(self.dir, ignore_errors=True)

    def plan(self, name, days="3"):
        return lab.plan_from_form({"name": name, "world": "patch", "people": "2", "days": days,
                                   "seeds": "1,2", "both": ""}, self.runs, self.binary, self.dir)

    def wait(self, pred, timeout=10):
        t0 = time.time()
        while time.time() - t0 < timeout:
            if pred():
                return True
            time.sleep(0.05)
        return False

    def test_a_series_runs_to_the_synthesis(self):
        plan = self.plan("petite")
        self.lab.write_plan("petite", plan)
        self.assertEqual(self.lab.plans(), ["petite"])
        s = self.lab.launch(plan)
        self.assertTrue(self.wait(lambda: s.done))
        out = os.path.join(self.runs, "petite")
        for name in ("patch-2-s1", "patch-2-s2"):
            self.assertEqual(open(os.path.join(out, name + ".exit")).read(), "0")
            self.assertTrue(os.path.exists(os.path.join(out, name + ".html")))
        with open(os.path.join(out, "serie.json")) as f:
            state = json.load(f)
        self.assertEqual(state["lab"]["pid"], os.getpid())
        self.assertTrue(all("wall" in r for r in state["runs"]))
        series = self.lab.series()
        self.assertEqual(len(series), 1)
        self.assertEqual(series[0]["state"], "terminée")
        self.assertEqual([(r["state"], r["year"], r["alive"], r["dead"]) for r in series[0]["runs"]],
                         [("fini", 2, 3, 2)] * 2)
        self.assertTrue(series[0]["synthese"])
        synth = open(os.path.join(out, "synthese.html")).read()
        self.assertIn("A:3", synth)      # Population of tribe A, last year
        self.assertIn("Hunger", synth)
        with self.assertRaises(ValueError):
            self.lab.stop(os.path.join(self.runs, "ailleurs"))

    def test_run_command_line_attached_launches_a_standard_plan_with_its_version(self):
        plan_file = os.path.join(self.dir, "etalon-mini.json")
        with open(plan_file, "w") as f:
            json.dump({"question": "mini ?", "runs": [{"name": "m", "args": "--world patch --people 2 --days 3",
                                                        "seeds": [1], "with_and_without_trials": True}]}, f)
        fixture_file = os.path.join(self.dir, "fixture.json")
        with open(fixture_file, "w") as f:
            json.dump(fixture(), f)
        os.environ["FAKE_JSON"] = fixture_file
        try:
            code = lab.main(["run", plan_file, "--attached", "--change", "essai", "--binary", self.binary,
                             "--content", self.dir, "--runs-dir", self.runs, "--name", "mini-1", "--jobs", "2"])
        finally:
            os.environ.pop("FAKE_JSON", None)
        self.assertEqual(code, 0)
        self.assertTrue(os.path.exists(os.path.join(self.runs, "plans", "mini-1.json")))
        with open(os.path.join(self.runs, "mini-1", "serie.json")) as f:
            state = json.load(f)
        self.assertEqual(state["version"]["change"], "essai")
        self.assertEqual(state["version"]["commit"], "inconnu")
        self.assertEqual(state["plan"]["lab"]["source"], "etalon-mini")
        self.assertEqual([r["name"] for r in state["runs"]], ["m-s1-avec", "m-s1-sans"])
        series = lab.Lab(self.runs, self.binary, self.dir, 2).series()
        self.assertEqual(series[0]["state"], "terminée")
        self.assertEqual(series[0]["plan_name"], "etalon-mini")
        self.assertTrue(all(r["data"] for r in series[0]["runs"]))
        self.assertIn("2/2 runs lus", reading.key_line(series[0], reading.Store(self.runs)))
        self.assertEqual(lab.main(["run", os.path.join(self.dir, "absent.json"), "--attached", "--runs-dir", self.runs]), 2)

    def test_stop_kills_only_this_series_and_marks_it(self):
        slow = self.plan("lente", days="1000")
        s = self.lab.launch(slow)
        self.assertTrue(self.wait(lambda: s.pid_alive("patch-2-s1") and s.pid_alive("patch-2-s2")))
        with self.assertRaises(ValueError):
            self.lab.launch(slow)
        series = self.lab.series()[0]
        self.assertEqual(series["state"], "en cours")
        self.assertTrue(series["owned"])
        self.lab.stop(os.path.join(self.runs, "lente"))
        self.assertTrue(self.wait(lambda: s.done))
        self.assertFalse(lab.pid_exists(s.runs[0]["pid"]))
        series = self.lab.series()[0]
        self.assertEqual(series["state"], "arrêtée")
        self.assertEqual([r["state"] for r in series["runs"]], ["arrêté", "arrêté"])
        self.assertIn("2 arrêté(s)", series["final"])


class OnePage(unittest.TestCase):
    """Lab 6: the run page in one, the maps' pieces, the curves, the home and series cards."""

    def data(self):
        d = fixture(fire=True)
        d["meta"]["factors"] = [{"key": "thought.plant_sight_tiles", "value": 12}, {"key": "fire.hearth_nights", "value": 3},
                                {"key": "zzz.unknown", "value": [1, 2]}]
        d["relief"] = {"cell_tiles": 32, "size": [2, 2], "known": True, "dm": [100, 110, 120, 130]}
        d["bearers"] = {"chunk_tiles": 32, "margin_chunks": 2, "seasons": {"apple": [False, True, True, False]},
                        "kinds": ["apple"], "cells": [[0, 0, [0, 3, 5, 0], [[0, 5]]]]}
        return d

    def test_factor_cards_come_from_meta_factors_and_unknown_keys_stay_folded(self):
        page = panels.factors_html(self.data())
        self.assertIn("<h3 class=fth>Le monde</h3>", page)
        self.assertIn("class=allf", page)                      # every setting, folded
        self.assertIn("zzz.unknown", page)                     # an unnamed key keeps its key
        self.assertNotIn("zzz.unknown", page.split("class=allf")[0])
        self.assertIn("Tous les réglages du contenu (3)", page)

    def test_the_maps_pieces_answer_and_an_unknown_one_does_not(self):
        d = self.data()
        kinds = {}
        for what in ("annees", "fond", "carte-base", "carte-an", "relief", "nourriture", "courbes"):
            body, ctype = panels.piece(d, what, 1)
            kinds[what] = ctype
            if ctype.startswith("application/json"):
                json.loads(body)
        self.assertTrue(kinds["fond"].startswith("image/png"))
        self.assertTrue(kinds["annees"].startswith("text/html"))
        self.assertIsNone(panels.piece(d, "rien", 1))
        base = json.loads(panels.piece(d, "carte-base", 1)[0])
        self.assertEqual((base["tiles"], base["years"], base["dpy"], base["tribes"]), ([64, 64], 2, 50, ["A", "B"]))
        year = json.loads(panels.piece(d, "carte-an", 1)[0])
        self.assertEqual((year["year"], year["first"], year["last"]), (1, 1, 50))
        self.assertIn([3, 10, 10, 30], [b[:4] for b in year["births"]])      # person 3 born on day 30
        self.assertEqual(json.loads(panels.piece(d, "relief", 1)[0])["dm"], [100, 110, 120, 130])
        self.assertEqual(json.loads(panels.piece(d, "nourriture", 1)[0])["kinds"], ["apple"])
        old = fixture()                                         # a run made before lab 6: no relief, no food
        self.assertFalse(json.loads(panels.piece(old, "relief", 1)[0])["known"])
        self.assertEqual(json.loads(panels.piece(old, "nourriture", 1)[0])["cells"], [])

    def test_the_small_multiples_share_the_days_and_smooth_the_needs(self):
        c = panels.curves(self.data())
        self.assertEqual((c["dpy"], c["last"], c["tribes"]), (50, 100, ["A", "B"]))
        self.assertEqual(len(c["days"]), 20)
        page = panels.curves_html()
        self.assertIn("data-sm6", page)

    def test_the_chronicle_facts_mix_their_kinds_and_count_right(self):
        facts, total = panels.notable_facts(self.data(), limit=4)
        self.assertEqual(len(facts), 4)
        self.assertGreaterEqual(total, 4)
        every = panels.notable_facts(self.data(), limit=99)[0]
        self.assertEqual(len({f[1] for f in facts}), min(4, len({f[1] for f in every})))   # one of each kind before a second
        self.assertEqual([f[0] for f in facts], sorted(f[0] for f in facts))     # told in the order of days
        texts = " ".join(f[2] for f in panels.notable_facts(self.data(), limit=99)[0])
        self.assertIn("1 mort de faim chez A", texts)
        self.assertIn("B tombe à 1 vivant,", texts)                               # no plural on one

    def test_the_year_strip_names_its_colours_and_marks(self):
        strip = panels.year_strip(self.data())
        self.assertIn('title="tribu A"', strip)
        self.assertIn('title="tribu B"', strip)
        self.assertIn("★", panels.years_section(self.data()))
        self.assertIn('data-lazy="annees"', panels.years_section(self.data()))

    def test_the_pyramid_counts_the_living_and_the_families_fold(self):
        d = self.data()
        start = panels.pyramid(d, 1, "Au départ")
        self.assertIn("· 4 vivants", start)                    # person 3 is born on day 30
        self.assertIn("· 3 vivants", panels.pyramid(d, 95, "À la fin"))   # persons 3 and 4 are dead
        self.assertIn("id=ages", panels.ages_families_html(d))

    def test_purpose_summary_and_series_title(self):
        self.assertEqual(panels.purpose_summary("Le feu change-t-il la survie (avec le soir) ? Et puis le reste , voilà."),
                         "Le feu change-t-il la survie ?")
        self.assertEqual(panels.purpose_summary("Un but , clair : oui. Le reste."), "Un but, clair : oui.")
        self.assertLessEqual(len(panels.purpose_summary("mot " * 200)), 222)
        self.assertEqual(panels.series_title("x", {"title": "Mon titre"}), "Mon titre")
        self.assertEqual(panels.series_title("lab5-feu-20ans-20261122-165124"), "Lab 5 feu 20 ans")

    def test_variants_are_named_by_what_differs(self):
        self.assertEqual(panels.variant_labels([(True, True), (True, False)]), {(True, True): "avec le feu", (True, False): "sans le feu"})
        self.assertEqual(panels.variant_labels([(True, False), (False, False)])[(False, False)], "sans essais (témoin)")
        self.assertEqual(len(set(panels.variant_labels([(a, b) for a in (True, False) for b in (True, False)]).values())), 4)
        self.assertEqual(panels.variant_labels([(True, True)]), {(True, True): "tous les runs"})

    def test_series_results_with_two_and_four_variants(self):
        tmp = tempfile.mkdtemp(prefix="lab6-")
        try:
            os.makedirs(os.path.join(tmp, "s"))
            runs = []
            for i, (tr, fi) in enumerate(((True, True), (True, False), (False, True), (False, False))):
                d = fixture(seed=i + 1, trials=tr, fire=fi)
                d["meta"]["settings"]["fire"] = fi
                with open(os.path.join(tmp, "s", f"r{i}.json"), "w") as f:
                    json.dump(d, f)
                runs.append({"name": f"r{i}"})
            two = panels.series_results(tmp, {"name": "s", "runs": runs[:2]})
            self.assertTrue(two["verdict"].startswith("Avec le feu, "), two["verdict"])
            self.assertEqual(len(two["runs"]), 2)
            four = panels.series_results(tmp, {"name": "s", "runs": runs})
            self.assertTrue(four["verdict"].startswith("Le plus de vivants à la fin : "), four["verdict"])
            self.assertIn("<b>4</b> runs lus sur 5", panels.series_results(tmp, {"name": "s", "runs": runs + [{"name": "absent"}]})["points"][-1])
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def test_last_year_counts_read_the_data_file(self):
        tmp = tempfile.mkdtemp(prefix="lab6-")
        try:
            p = os.path.join(tmp, "r.json")
            with open(p, "w") as f:
                json.dump(fixture(), f)
            c = lab.last_year_counts(p)
            self.assertEqual((c["A"]["Population"], c["A"]["Hunger"], c["B"]["Cold"]), ("2", "1", "1"))
            self.assertEqual(lab.last_year_counts(os.path.join(tmp, "absent.json")), {})
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def test_the_title_field_goes_round_and_the_standard_plans_have_one(self):
        fields = dict(Form.FIELDS, title="Un titre")
        plan = lab.plan_from_form(fields, "/runs", "/b", "/c")
        self.assertEqual(plan["title"], "Un titre")
        self.assertEqual(lab.form_from_plan(plan)["title"], "Un titre")
        names = sorted(n[:-5] for n in os.listdir(lab.STANDARD_PLANS) if n.endswith(".json"))
        self.assertTrue(names)
        for name in names:
            self.assertTrue(lab.standard_plan(name).get("title"), name)


if __name__ == "__main__":
    unittest.main()
