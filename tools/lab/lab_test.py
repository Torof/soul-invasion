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
[ -n "$FAKE_JSON" ] && cp "$FAKE_JSON" "${out%.html}.json"
echo "3 alive, 2 dead" >&2
exit "${FAKE_EXIT:-0}"
"""


def fixture(seed=1, trials=True):
    """A tiny data file written by hand after docs/10-vivarium-data.md: two tribes, two years."""
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
    return {
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
        self.assertEqual(plan["lab"]["jobs"], 4)
        self.assertEqual(len(lab.expand(plan)), 6)

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
        self.assertEqual(v, "Après 2 ans, 3 des 4 du départ sont vivants, et l’enfant né en route n’a pas survécu. "
                            "Ils ont surtout manqué de nourriture (1 des 2 morts), en hiver. Une technique gardée : chercher plus loin.")
        self.assertTrue(reading.verdict(fixture(trials=False)).endswith("Les épreuves étaient coupées (c’est le témoin) : rien à garder."))
        for p in self.d["people"]:
            p["died"] = p["died"] or {"day": 95, "cause": "thirst", "season": "winter", "at": [1, 1]}
        self.assertTrue(reading.verdict(self.d).startswith("Après 2 ans, plus personne : les 4 du départ sont morts, et l’enfant né en route aussi. "
                                                          "Ils ont surtout manqué d’eau (3 des 5 morts), en hiver."))
        self.assertIn("Aucune technique", reading.verdict(self.d))
        alone = fixture()
        alone["people"] = alone["people"][:2]
        alone["years"] = alone["years"][:2]
        alone["found"] = alone["found"][:1]
        self.assertIn("Personne n’est mort.", reading.verdict(alone))

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
        self.assertEqual(tiles.count('<div class="tile'), 4)
        self.assertNotIn("arrow", tiles)
        other = reading.summary(fixture(trials=False))
        tiles = reading.tiles_html(sm, other)
        self.assertIn('class="arrow good" title="+1 par rapport à l’autre côté">↑ +1', tiles)
        other["dead"] = 1
        self.assertIn('class="arrow bad"', reading.tiles_html(sm, other))      # one more dead: worse

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
        self.assertIn('<p class=verdict>Avec les épreuves, 3 vivants à la fin en moyenne sur 2 runs ; sans, 3 sur 2 runs. Les épreuves ne changent rien.</p>', serie)
        self.assertIn("Population de tous les runs", serie)
        self.assertIn("écart-type", serie)
        run = reading.run_body(s["etalon-test-1"], s["etalon-test-1"]["runs"][0], self.lab.store.load("etalon-test-1", "t-s1-avec"))
        for part in ("<p class=verdict>Après 2 ans, 3 des 4", '<div class="tile alive">', "<h2>La carte</h2>", 'class="winter"',
                     "<h2>La population</h2>", "<h2>Ce qu’ils ont trouvé</h2>", "<h2>Les morts</h2>", "<h2>Les naissances</h2>",
                     "<h2>Les gens à la fin</h2>", "<summary>Le détail</summary>", "Année par année", "La vie de chacun (5)"):
            self.assertIn(part, run)
        order = [run.index(x) for x in ("<p class=verdict>", "<div class=tiles>", "<h2>La carte</h2>", "<h2>La population</h2>",
                                        "<h2>Ce qu’ils ont trouvé</h2>", "<h2>Les morts</h2>", "<h2>Les naissances</h2>",
                                        "<h2>Les gens à la fin</h2>", "<details class=detail>")]
        self.assertEqual(order, sorted(order))
        self.assertLess(run.index("Année par année"), run.index("</details>"))
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
        self.assertIn("<p class=verdict>La version B contre la version A : avec épreuves, pareil ; sans épreuves, pareil.</p>", body)
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
                                       ("/run/etalon-test-1/t-s1-avec", 200, "Ce qu’ils ont trouvé"),
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
        for name, count in (("etalon-bande-25-5ans", 2), ("etalon-deux-tribus-25-20ans", 2), ("etalon-dix-graines", 20)):
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


if __name__ == "__main__":
    unittest.main()
