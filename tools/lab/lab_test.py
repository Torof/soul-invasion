"""Tests of the laboratory (D52): plan expansion, year lines, the state machine.

    python3 -I tools/lab/lab_test.py

Standard library only. The two end-to-end tests launch a shell script in place
of the vivarium binary, so they need `/bin/sh`.
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
echo "3 alive, 2 dead" >&2
exit "${FAKE_EXIT:-0}"
"""


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
