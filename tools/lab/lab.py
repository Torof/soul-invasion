#!/usr/bin/env python3
"""The laboratory: create series of vivarium runs and follow them live (D52).

    python3 -I tools/lab/lab.py [--port 8765] [--runs-dir ~/.cache/soul-sim/runs]
                                [--binary PATH] [--content DIR] [--jobs N]

Serves three pages on 127.0.0.1 only: "Nouvelle série" (a form that writes a
plan and launches it), "En cours" (every series under the runs directory,
with its live table, refreshed every two seconds) and "Lire / Comparer"
(a run, a series, two runs or two versions face to face, read from the
vivarium's JSON by `reading.py`). Pure standard library, nothing outward,
no external resource in the pages.

    python3 -I tools/lab/lab.py run PLAN.json --change "what changed" [--worktree DIR]
                                [--binary PATH] [--content DIR] [--name NAME] [--attached]

launches a series from a plan file without the browser (the standard plans
live in `tools/lab/plans/`), detached from any server: the "En cours" screen
sees it through its `serie.json`. Every series records the version of the
binary it ran: the git commit and branch of `--worktree` (or the plan's
`worktree`), "inconnu" without one, and the `--change` sentence.

The plan format and the output layout are those of `soul-runs`, the local
launcher the lab absorbs: a plan is `<runs-dir>/plans/<name>.json`, a series
writes `<runs-dir>/<name>/` with `serie.json`, one `NAME.log`, `NAME.html`
and `NAME.exit` per run, and `synthese.html` at the end. The lab adds a `lab`
key to the plan (the form's fields, the jobs) that `soul-runs` ignores.

The runs belong to the lab process: it holds their PIDs and ends them only on
the user's button or when it exits. A series launched elsewhere (soul-runs,
an earlier lab) is shown from its files, never stopped from here.
"""

import argparse
import html
import json
import os
import re
import shlex
import signal
import subprocess
import sys
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:  # `-I` leaves the script's own folder out of the path
    sys.path.insert(0, HERE)
import reading  # noqa: E402

REPO = os.path.dirname(os.path.dirname(HERE))
HOME = os.path.expanduser("~")
DEFAULT_RUNS = os.path.join(HOME, ".cache", "soul-sim", "runs")
DEFAULT_BIN = os.path.join(HOME, ".cache", "soul-sim", "target", "release", "vivarium")
DEFAULT_CONTENT = os.path.join(REPO, "content")
DEFAULT_JOBS = max(1, (os.cpu_count() or 4) - 2)
DAYS_A_YEAR = 50
YEAR_RE = re.compile(r"year (\d+): (\d+) alive, (\d+) dead so far, (\d+) s")
FINAL_RE = re.compile(r"(\d+) alive, (\d+) dead")
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
KEEP = ["Population", "Births", "Illness", "Old age", "Childbirth", "Hunger",
        "Thirst", "Cold", "Trials", "Kept", "Copied", "Homes moved"]
BINARY_RETRY_S = 5.0
FORM_DEFAULTS = {
    "name": "", "question": "", "run_name": "", "world": "patch", "side": "50",
    "tribes": "", "people": "5", "children": "0", "infants": "0", "ages": "18-40",
    "fed": "", "days": "10", "seeds": "1", "both": "", "cadence": "1", "jobs": "",
    "change": "",
}
STANDARD_PLANS = os.path.join(HERE, "plans")


# --- the plan: expansion, seeds, form ---------------------------------------

def arg_list(a):
    """A run's args, given as a list or a shell-like string."""
    if a is None:
        return []
    return shlex.split(a) if isinstance(a, str) else [str(x) for x in a]


def expand(plan):
    """The runs a plan asks for: seeds × (with, without trials), soul-runs' rule.

    A `--seed` inside the args wins over the `seeds` list; `--no-trials` in the
    args makes every variant a control run.
    """
    runs = []
    for r in plan.get("runs", []):
        base = arg_list(r.get("args"))
        if "--seed" in base:
            i = base.index("--seed")
            seeds = [int(base[i + 1])]
            base = base[:i] + base[i + 2:]
        else:
            seeds = r.get("seeds") or [1]
        variants = [(False, "")]
        if r.get("with_and_without_trials"):
            variants = [(False, "-avec"), (True, "-sans")]
        for seed in seeds:
            for no_trials, tag in variants:
                args = base + ["--seed", str(seed)]
                if no_trials and "--no-trials" not in args:
                    args.append("--no-trials")
                runs.append({"name": f"{r['name']}-s{seed}{tag}", "base": r["name"],
                             "seed": seed, "trials": "--no-trials" not in args,
                             "args": args})
    return runs


def parse_seeds(text):
    """Seeds from "1,2,3", "1-10", "1 2 3" or a mix of them, in order, no doubles."""
    seeds = []
    for part in re.split(r"[,\s;]+", re.sub(r"\s*-\s*", "-", text.strip())):
        if not part:
            continue
        m = re.fullmatch(r"(\d+)\s*-\s*(\d+)", part)
        if m:
            lo, hi = int(m.group(1)), int(m.group(2))
            if lo > hi:
                raise ValueError(f"graines : {part} va à l'envers")
            if hi - lo >= 1000:
                raise ValueError(f"graines : {part}, c'est plus de mille runs")
            seeds.extend(range(lo, hi + 1))
        elif part.isdigit():
            seeds.append(int(part))
        else:
            raise ValueError(f"graines : {part!r} n'est pas un nombre ni une plage")
    out = []
    for s in seeds:
        if s not in out:
            out.append(s)
    if not out:
        raise ValueError("graines : il en faut au moins une")
    return out


def plan_from_form(fields, runs_dir, binary, content, now=None):
    """The plan (soul-runs format plus a `lab` key) a filled form asks for.

    Raises ValueError, in French, when a field is not acceptable.
    """
    f = dict(FORM_DEFAULTS)
    f.update({k: (v or "").strip() for k, v in fields.items() if k in FORM_DEFAULTS})
    name = f["name"]
    if not NAME_RE.match(name):
        raise ValueError("nom de série : lettres, chiffres, point, tiret, souligné (80 au plus)")
    if f["world"] not in ("vivarium", "patch"):
        raise ValueError("monde : vivarium ou patch")

    def number(key, lo, hi):
        text = f[key]
        if not text.isdigit() or not lo <= int(text) <= hi:
            raise ValueError(f"{key} : un entier de {lo} à {hi}, pas {text!r}")
        return int(text)

    side = number("side", 8, 4096)
    people = number("people", 1, 10000)
    children = number("children", 0, 10000)
    infants = number("infants", 0, 10000)
    days = number("days", 1, 1_000_000)
    cadence = number("cadence", 1, 100)
    if f["tribes"] and f["tribes"] not in ("1", "2"):
        raise ValueError("tribus : 1, 2 ou le défaut du monde")
    m = re.fullmatch(r"(\d+)-(\d+)", f["ages"])
    if not m or int(m.group(1)) > int(m.group(2)):
        raise ValueError("âges : LO-HI, par exemple 18-40")
    seeds = parse_seeds(f["seeds"])
    jobs = number("jobs", 1, 256) if f["jobs"] else None
    run_name = f["run_name"] or f"{f['world']}-{people}"
    if not NAME_RE.match(run_name):
        raise ValueError("nom des runs : lettres, chiffres, point, tiret, souligné")

    args = ["--world", f["world"]]
    if f["world"] == "patch" and side != 50:
        args += ["--side", str(side)]
    if f["tribes"]:
        args += ["--tribes", f["tribes"]]
    args += ["--people", str(people)]
    if children:
        args += ["--children", str(children)]
    if infants:
        args += ["--infants", str(infants)]
    if f["ages"] != "18-40":
        args += ["--ages", f["ages"]]
    if f["fed"]:
        args.append("--fed")
    args += ["--days", str(days)]
    if cadence != 1:
        args += ["--cadence", str(cadence)]

    f["seeds"] = ",".join(str(s) for s in seeds)
    f["fed"] = "on" if f["fed"] else ""
    f["both"] = "on" if f["both"] else ""
    created = time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(now))
    return {
        "question": f["question"],
        "change": f["change"],
        "binary": binary,
        "content": content,
        "out": os.path.join(runs_dir, name),
        "runs": [{"name": run_name, "args": " ".join(args), "seeds": seeds,
                  "with_and_without_trials": bool(f["both"])}],
        "lab": {"form": f, "jobs": jobs, "created": created},
    }


def form_from_plan(plan, name=""):
    """The form's fields for a plan: its own `lab.form`, else read off its first run."""
    f = dict(FORM_DEFAULTS)
    lab = plan.get("lab") or {}
    if isinstance(lab.get("form"), dict):
        f.update({k: str(v) for k, v in lab["form"].items() if k in FORM_DEFAULTS})
    else:
        f["question"] = plan.get("question", "")
        f["change"] = plan.get("change", "")
        runs = plan.get("runs") or [{}]
        r = runs[0]
        f["run_name"] = r.get("name", "")
        f["seeds"] = ",".join(str(s) for s in r.get("seeds") or [1])
        f["both"] = "on" if r.get("with_and_without_trials") else ""
        args = arg_list(r.get("args"))
        flags = {"--world": "world", "--side": "side", "--tribes": "tribes", "--people": "people",
                 "--children": "children", "--infants": "infants", "--ages": "ages",
                 "--days": "days", "--cadence": "cadence", "--seed": "seeds"}
        i = 0
        while i < len(args):
            if args[i] == "--fed":
                f["fed"] = "on"
            elif args[i] in flags and i + 1 < len(args):
                f[flags[args[i]]] = args[i + 1]
                i += 1
            i += 1
        if lab.get("jobs"):
            f["jobs"] = str(lab["jobs"])
    f["name"] = name or f.get("name") or os.path.basename(plan.get("out", "").rstrip("/"))
    return f


# --- the state of a run, from its files -----------------------------------

def year_lines(log):
    """Every `year N: A alive, D dead so far, S s` line, as (year, alive, dead, secs)."""
    return [tuple(int(x) for x in m.groups()) for m in YEAR_RE.finditer(log)]


def final_counts(log):
    """(alive, dead) from the final line of a finished run, or None."""
    m = None
    for m in FINAL_RE.finditer(log):
        pass
    return (int(m.group(1)), int(m.group(2))) if m else None


def run_state(run, exit_code, pid_alive):
    """The state shown for a run.

    `exit_code` is the text of NAME.exit or None; `pid_alive` says whether the
    process is still there (known for sure for the lab's own runs, read from
    the system for the others).
    """
    if run.get("stopped"):
        return "arrêté"
    if exit_code is not None:
        return "fini" if exit_code.strip() == "0" else f"ÉCHEC({exit_code.strip()})"
    if run.get("started"):
        return "en cours" if pid_alive else "perdu"
    return "attend"


def read_text(path):
    try:
        with open(path, errors="replace") as f:
            return f.read()
    except OSError:
        return None


def pid_exists(pid):
    if not pid:
        return False
    try:
        os.kill(int(pid), 0)
    except (OSError, ValueError):
        return False
    return True


def run_status(out, run, now, pid_alive):
    log = read_text(os.path.join(out, run["name"] + ".log")) or ""
    year = alive = dead = None
    for year, alive, dead, _secs in year_lines(log):
        pass
    code = read_text(os.path.join(out, run["name"] + ".exit"))
    state = run_state(run, code, pid_alive)
    if state == "fini":
        final = final_counts(log)
        if final:
            alive, dead = final
    started, ended = run.get("started"), run.get("ended")
    if ended and started:
        elapsed = ended - started
    elif started:
        elapsed = now - started
    else:
        elapsed = 0
    page = run["name"] + ".html"
    return {"name": run["name"], "state": state, "year": year, "alive": alive, "dead": dead,
            "elapsed": int(elapsed),
            "page": page if os.path.exists(os.path.join(out, page)) else None,
            "log": run["name"] + ".log" if log else None}


def load_state(out):
    try:
        with open(os.path.join(out, "serie.json")) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def save_state(out, plan, runs, extra=None):
    state = {"plan": plan, "runs": runs}
    if extra:
        state.update(extra)
    tmp = os.path.join(out, "serie.json.tmp")
    with open(tmp, "w") as f:
        json.dump(state, f, indent=1, ensure_ascii=False)
    os.replace(tmp, os.path.join(out, "serie.json"))


def git_word(worktree, *args):
    try:
        r = subprocess.run(["git", "-C", worktree, *args], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def version_of(worktree, binary, change=""):
    """The version a series ran: the commit and branch of the binary's worktree, the change sentence.

    `commit` and `branch` are "inconnu" without a worktree or when git says
    nothing; `dirty` tells whether the worktree had uncommitted changes.
    """
    v = {"commit": "inconnu", "branch": "inconnu", "dirty": False, "change": change or "",
         "worktree": os.path.abspath(os.path.expanduser(worktree)) if worktree else None,
         "binary": binary, "binary_mtime": None}
    try:
        v["binary_mtime"] = time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(os.path.getmtime(binary)))
    except OSError:
        pass
    if v["worktree"]:
        commit = git_word(v["worktree"], "rev-parse", "--short", "HEAD")
        branch = git_word(v["worktree"], "rev-parse", "--abbrev-ref", "HEAD")
        status = git_word(v["worktree"], "status", "--porcelain", "--untracked-files=no")
        if commit:
            v["commit"] = commit
        if branch:
            v["branch"] = branch
        v["dirty"] = bool(status)
    return v


def signature(plan):
    """What makes two series "the same plan": its expanded runs' names and args."""
    return json.dumps([(r["name"], r["args"]) for r in expand(plan)], ensure_ascii=False)


# --- the synthesis page (soul-runs' layout) -------------------------------

def parse_tables(page):
    """Last-year rows of the tribe table, by header names (missing columns tolerated)."""
    result = {}
    for t in re.findall(r"<table.*?</table>", page, re.S):
        heads = [html.unescape(h) for h in re.findall(r"<th>(.*?)</th>", t, re.S)]
        if "Year" not in heads or "Population" not in heads:
            continue
        last = {}
        for row in re.findall(r"<tr>(.*?)</tr>", t, re.S):
            cells = [html.unescape(re.sub(r"<.*?>", "", c))
                     for c in re.findall(r"<td>(.*?)</td>", row, re.S)]
            if len(cells) == len(heads):
                d = dict(zip(heads, cells))
                last[d.get("Tribe", "")] = d
        result = last
    return result


def synthesis(out, plan, runs, pid_alive=lambda name: False):
    now = time.time()
    h = html.escape
    rows, cols = [], []
    for r in runs:
        s = run_status(out, r, now, pid_alive(r["name"]))
        tribes = parse_tables(read_text(os.path.join(out, r["name"] + ".html")) or "")
        for k in KEEP:
            if any(k in t for t in tribes.values()) and k not in cols:
                cols.append(k)
        rows.append((r, s, tribes))
    head = "".join(f"<th>{h(c)}</th>" for c in cols)
    body = []
    for r, s, tribes in rows:
        link = f'<a href="{h(s["page"])}">page</a>' if s["page"] else "-"
        vals = ""
        for c in cols:
            parts = [f"{h(n)}:{h(t[c])}" for n, t in sorted(tribes.items()) if c in t]
            vals += f"<td>{' · '.join(parts) if parts else '-'}</td>"
        v = lambda x: "-" if x is None else x  # noqa: E731
        body.append(f"<tr><td>{h(r['name'])}</td><td>{r['seed']}</td>"
                    f"<td>{'avec' if r['trials'] else 'sans'}</td><td>{h(s['state'])}</td>"
                    f"<td>{v(s['alive'])}</td><td>{v(s['dead'])}</td><td>{s['elapsed']} s</td>"
                    f"<td>{link}</td>{vals}</tr>")
    q = h(plan.get("question") or "(pas de question dans le plan)")
    page = f"""<!doctype html><html lang="fr"><meta charset="utf-8"><title>Synthèse de la série</title>
<style>body{{font:14px sans-serif;margin:2em}}table{{border-collapse:collapse}}
td,th{{border:1px solid #999;padding:3px 8px;text-align:left}}pre{{background:#eee;padding:8px}}</style>
<h1>Synthèse de la série</h1>
<h2>Ce que la série demandait</h2><p>{q}</p>
<h2>Le plan</h2><pre>{h(json.dumps(plan, indent=1, ensure_ascii=False))}</pre>
<h2>Les runs</h2>
<p>Vivants et morts à la fin du run ; les compteurs sont ceux de la dernière année du tableau « The tribes, year by year » de chaque page, par tribu.</p>
<table><tr><th>Run</th><th>Graine</th><th>Épreuves</th><th>État</th><th>Vivants</th><th>Morts</th><th>Temps</th><th>Page</th>{head}</tr>
{''.join(body)}</table></html>"""
    with open(os.path.join(out, "synthese.html"), "w") as f:
        f.write(page)


# --- a series run by this process ------------------------------------------

class Series:
    """One plan, launched: `jobs` runs at a time, PIDs held here, files as soul-runs."""

    def __init__(self, plan, jobs, binary, content, version=None):
        self.plan = plan
        self.out = os.path.expanduser(plan["out"])
        self.binary = os.path.expanduser(plan.get("binary") or binary)
        self.content = os.path.expanduser(plan.get("content") or content)
        self.version = version or version_of(None, self.binary, plan.get("change", ""))
        self.jobs = max(1, int(jobs))
        self.runs = expand(plan)
        if not self.runs:
            raise ValueError("aucun run dans le plan")
        self.procs = {}
        self.lock = threading.Lock()
        self.stop_event = threading.Event()
        self.note = ""
        self.done = False
        self.thread = threading.Thread(target=self._loop, name=f"series {self.out}", daemon=True)

    def start(self):
        os.makedirs(self.out, exist_ok=True)
        for r in self.runs:
            for ext in (".exit", ".log", ".html"):
                try:
                    os.remove(os.path.join(self.out, r["name"] + ext))
                except OSError:
                    pass
        self._save()
        self.thread.start()

    def _save(self):
        save_state(self.out, self.plan, self.runs,
                   {"lab": {"pid": os.getpid(), "jobs": self.jobs, "note": self.note}, "version": self.version})

    def pid_alive(self, name):
        with self.lock:
            p = self.procs.get(name)
        return p is not None and p.poll() is None

    def stop(self):
        self.stop_event.set()

    def _launch(self, r):
        cmd = ([self.binary, "--content", self.content] + r["args"]
               + ["--out", os.path.join(self.out, r["name"] + ".html")])
        with open(os.path.join(self.out, r["name"] + ".log"), "w") as log:
            p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT,
                                 stdin=subprocess.DEVNULL, start_new_session=True)
        r["started"] = time.time()
        r["pid"] = p.pid
        with self.lock:
            self.procs[r["name"]] = p

    def _finish(self, r, code):
        r["ended"] = time.time()
        r["wall"] = r["ended"] - r["started"]
        with open(os.path.join(self.out, r["name"] + ".exit"), "w") as f:
            f.write(str(code))
        with self.lock:
            self.procs.pop(r["name"], None)

    def _loop(self):
        pending = list(self.runs)
        while (pending or self.procs) and not self.stop_event.is_set():
            while pending and len(self.procs) < self.jobs and not self.stop_event.is_set():
                if not os.access(self.binary, os.X_OK):
                    self.note = "le binaire du vivarium manque : nouvel essai dans 5 s"
                    self._save()
                    self.stop_event.wait(BINARY_RETRY_S)
                    continue
                try:
                    self._launch(pending[0])
                except OSError as e:
                    self.note = f"lancement impossible ({e.strerror or e}) : nouvel essai dans 5 s"
                    self._save()
                    self.stop_event.wait(BINARY_RETRY_S)
                    continue
                self.note = ""
                pending.pop(0)
                self._save()
            with self.lock:
                running = list(self.procs.items())
            for name, p in running:
                code = p.poll()
                if code is not None:
                    self._finish(next(r for r in self.runs if r["name"] == name), code)
                    self._save()
            self.stop_event.wait(0.2)
        if self.stop_event.is_set():
            self._terminate(pending)
        synthesis(self.out, self.plan, self.runs)
        self.done = True
        self._save()

    def _terminate(self, pending):
        with self.lock:
            running = list(self.procs.items())
        for _name, p in running:
            try:
                p.terminate()
            except OSError:
                pass
        deadline = time.time() + 5
        for name, p in running:
            try:
                p.wait(timeout=max(0.1, deadline - time.time()))
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
            r = next(r for r in self.runs if r["name"] == name)
            r["stopped"] = True
            self._finish(r, p.returncode)
        for r in pending:
            r["stopped"] = True
        self.note = "arrêtée par le bouton"


def lab_plan_name(plan, name):
    """The plan a series was made from: the standard plan's stem, else the series' own name."""
    lab = plan.get("lab") or {}
    return lab.get("source") or name


class Lab:
    """What the server knows: the runs directory, the defaults, the series it owns."""

    def __init__(self, runs_dir, binary, content, jobs, worktree=None):
        self.runs_dir = os.path.abspath(os.path.expanduser(runs_dir))
        self.plans_dir = os.path.join(self.runs_dir, "plans")
        self.binary = os.path.abspath(os.path.expanduser(binary))
        self.content = os.path.abspath(os.path.expanduser(content))
        self.jobs = jobs
        self.worktree = worktree
        self.store = reading.Store(self.runs_dir)
        self.owned = {}
        self.lock = threading.Lock()
        os.makedirs(self.plans_dir, exist_ok=True)

    # plans
    def plan_path(self, name):
        if not NAME_RE.match(name):
            raise ValueError("nom de plan invalide")
        return os.path.join(self.plans_dir, name + ".json")

    def plans(self):
        names = sorted(n[:-5] for n in os.listdir(self.plans_dir)
                       if n.endswith(".json") and NAME_RE.match(n[:-5]))
        return names

    def load_plan(self, name):
        with open(self.plan_path(name)) as f:
            return json.load(f)

    def write_plan(self, name, plan):
        path = self.plan_path(name)
        tmp = path + ".tmp"
        with open(tmp, "w") as f:
            json.dump(plan, f, indent=1, ensure_ascii=False)
        os.replace(tmp, path)

    def standard_plans(self):
        """The standard plans shipped in the repository (`tools/lab/plans/`), by name."""
        try:
            return sorted(n[:-5] for n in os.listdir(STANDARD_PLANS) if n.endswith(".json") and NAME_RE.match(n[:-5]))
        except OSError:
            return []

    def plan_from_file(self, path, name=None, change="", worktree=None, now=None):
        """A plan read from a file (a standard plan, or any plan), made a new series.

        The series is named `name`, else `<file stem>-<date and time>`, so every
        launch of a standard plan is a new series: the versions compare.
        """
        with open(path) as f:
            plan = json.load(f)
        if not isinstance(plan, dict) or not plan.get("runs"):
            raise ValueError(f"{path} : pas un plan (il faut une liste runs)")
        stem = os.path.splitext(os.path.basename(path))[0]
        name = name or f"{stem}-{time.strftime('%Y%m%d-%H%M%S', time.localtime(now))}"
        if not NAME_RE.match(name):
            raise ValueError("nom de série : lettres, chiffres, point, tiret, souligné (80 au plus)")
        plan["out"] = os.path.join(self.runs_dir, name)
        if change:
            plan["change"] = change
        if worktree:
            plan["worktree"] = os.path.abspath(os.path.expanduser(worktree))
        lab = plan.setdefault("lab", {})
        lab.setdefault("source", stem)
        lab["created"] = time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(now))
        return name, plan

    # series
    def launch(self, plan, change=None):
        out = os.path.abspath(os.path.expanduser(plan.get("out") or ""))
        if not out or os.path.dirname(out) != self.runs_dir:
            raise ValueError(f'le dossier "out" du plan doit être un sous-dossier direct de {self.runs_dir}')
        with self.lock:
            if self.is_running(out):
                raise ValueError("cette série tourne encore : arrêtez-la d'abord")
            jobs = (plan.get("lab") or {}).get("jobs") or self.jobs
            binary = os.path.expanduser(plan.get("binary") or self.binary)
            version = version_of(plan.get("worktree") or self.worktree, binary,
                                 change if change is not None else plan.get("change", ""))
            s = Series(plan, jobs, self.binary, self.content, version)
            s.start()
            self.owned[out] = s
        return s

    def is_running(self, out):
        s = self.owned.get(out)
        if s is not None and not s.done:
            return True
        st = load_state(out)
        if not st:
            return False
        for r in st["runs"]:
            if r.get("started") and not r.get("stopped") \
                    and not os.path.exists(os.path.join(out, r["name"] + ".exit")) \
                    and pid_exists(r.get("pid")):
                return True
        return False

    def stop(self, out):
        s = self.owned.get(out)
        if s is None:
            raise ValueError("cette série n'a pas été lancée par ce laboratoire")
        s.stop()

    def stop_all(self):
        with self.lock:
            series = list(self.owned.values())
        for s in series:
            s.stop()
        for s in series:
            s.thread.join(timeout=8)

    def find(self, serie, run=None):
        """A series by name (and one of its runs), from `series()`; None when absent."""
        for s in self.series():
            if s["name"] == serie:
                if run is None:
                    return s, None
                r = next((r for r in s["runs"] if r["name"] == run), None)
                return (s, r) if r else None
        return None

    def series(self):
        """Every series under the runs directory, newest first, with its live table."""
        now = time.time()
        out = []
        try:
            dirs = [os.path.join(self.runs_dir, d) for d in os.listdir(self.runs_dir)]
        except OSError:
            dirs = []
        for d in dirs:
            st = load_state(d)
            if not st or not isinstance(st.get("runs"), list):
                continue
            owned = self.owned.get(d)
            plan = st.get("plan") or {}
            runs = []
            for r in st["runs"]:
                alive = owned.pid_alive(r["name"]) if owned else pid_exists(r.get("pid"))
                runs.append(run_status(d, r, now, alive))
            states = [r["state"] for r in runs]
            ended = all(s not in ("attend", "en cours") for s in states)
            if any(s == "en cours" for s in states):
                state = "en cours" if owned else "en cours (hors labo)"
            elif not ended and owned and not owned.done:
                state = "en cours"
            elif not ended:
                state = "perdue"
            elif any(s == "arrêté" for s in states):
                state = "arrêtée"
            else:
                fails = sum(s.startswith("ÉCHEC") for s in states)
                state = "terminée" + (f" ({fails} échec{'s' if fails > 1 else ''})" if fails else "")
            starts = [r.get("started") for r in st["runs"] if r.get("started")]
            ends = [r.get("ended") for r in st["runs"] if r.get("ended")]
            if starts:
                elapsed = (max(ends) if ended and ends else now) - min(starts)
            else:
                elapsed = 0
            name = os.path.basename(d)
            synth = os.path.exists(os.path.join(d, "synthese.html"))
            lab = st.get("lab") or {}
            finished = sum(s == "fini" for s in states)
            final = (f"{len(runs)} run{'s' if len(runs) > 1 else ''}, {finished} fini{'s' if finished > 1 else ''}"
                     f"{', ' + str(sum(s.startswith('ÉCHEC') for s in states)) + ' en échec' if any(s.startswith('ÉCHEC') for s in states) else ''}"
                     f"{', ' + str(sum(s == 'arrêté' for s in states)) + ' arrêté(s)' if any(s == 'arrêté' for s in states) else ''}"
                     f", {int(elapsed)} s") if ended else ""
            version = st.get("version") or {"commit": "inconnu", "branch": "inconnu", "change": plan.get("change", "")}
            try:
                sig = signature(plan)
            except (KeyError, TypeError, ValueError):
                sig = None
            out.append({
                "name": name, "dir": name, "question": plan.get("question", ""),
                "version": version, "plan_name": lab_plan_name(plan, name), "signature": sig,
                "state": state, "owned": owned is not None, "elapsed": int(elapsed),
                "note": (owned.note if owned else lab.get("note", "")) or "",
                "jobs": lab.get("jobs"), "created": (plan.get("lab") or {}).get("created", ""),
                "sort": max(starts) if starts else os.path.getmtime(os.path.join(d, "serie.json")),
                "synthese": f"/runs/{urllib.parse.quote(name)}/synthese.html" if synth else None,
                "final": final,
                "runs": [dict(r, seed=orig.get("seed"), trials=orig.get("trials", True),
                              page=f"/runs/{urllib.parse.quote(name)}/{urllib.parse.quote(r['page'])}" if r["page"] else None,
                              log=f"/runs/{urllib.parse.quote(name)}/{urllib.parse.quote(r['log'])}" if r["log"] else None,
                              data=os.path.exists(os.path.join(d, r["name"] + ".json")),
                              read=f"/run/{urllib.parse.quote(name)}/{urllib.parse.quote(r['name'])}")
                         for r, orig in zip(runs, st["runs"])],
            })
        out.sort(key=lambda s: s["sort"], reverse=True)
        return out


# --- the pages ---------------------------------------------------------------

CSS = """
body{font:16px/1.5 system-ui,sans-serif;margin:0;background:#fbfbf9;color:#1f2328}
header{background:#fff;border-bottom:1px solid #e6e4df;padding:.7em 1.5em;display:flex;gap:1.6em;align-items:baseline}
header strong{color:#2f3e46;font-size:1.05em}
header a{color:#5a6670;text-decoration:none;font-weight:600}header a.on{color:#1f2328;border-bottom:2px solid #2f3e46}
header small{color:#9aa0a6;margin-left:auto}
main{padding:1.2em 1.6em 4em;max-width:1100px;margin:0 auto}
a{color:#1f6f8b}
h1{font-size:1.5em;font-weight:650;margin:.3em 0 .4em}h2{font-size:1.25em;font-weight:650;margin:2.2em 0 .25em}h3{font-size:1.05em;margin:1.6em 0 .3em}
.caption{color:#6b7280;font-size:.92em;margin:0 0 .8em}
.verdict{font-size:1.4em;line-height:1.45;margin:.8em 0 1em;max-width:44em}.verdict.small{font-size:1.1em;margin:.4em 0}
.q{color:#444;margin:.2em 0 .6em;white-space:pre-wrap}.muted{color:#8a8f98}.small{font-size:.9em}.settings{color:#555}
.etat{display:inline-block;padding:0 .6em;border-radius:1em;font-size:.75em;background:#eceae4;color:#444;vertical-align:middle;font-weight:500}
.etat.cours{background:#ffe9a8}.etat.fini{background:#d4ecd0}.etat.echec,.etat.perdu{background:#f6d3cf}.etat.arret{background:#e3dcf2}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(10em,1fr));gap:1em;margin:1em 0 1.5em}
.tile{background:#fff;border-radius:12px;padding:1em 1.2em;box-shadow:0 1px 3px rgba(0,0,0,.06)}
.tile .v{font-size:2.6em;font-weight:700;line-height:1.1;font-variant-numeric:tabular-nums}.tile .k{color:#4b5563;font-size:.95em;margin-top:.1em}
.tile .s{color:#8a8f98;font-size:.85em;margin-top:.35em}
.tile.alive .v{color:#2b7a3d}.tile.want .v{color:#b3261e}.tile.background .v{color:#6b7280}.tile.births .v{color:#3a8f4a}.tile.stock .v{color:#8a6d1f}.tile.kept .v{color:#c2571a}.tile .v.text{font-size:1.15em;line-height:2.5}
.tilelabel{font-weight:700;font-size:1.3em;align-self:center;color:#6b7280}
.arrow{font-size:.42em;font-weight:600;margin-left:.4em;vertical-align:middle;padding:.1em .5em;border-radius:1em}
.arrow.good{background:#d8efd3;color:#1f6b2a}.arrow.bad{background:#f6d3cf;color:#8a2a22}
.alive{color:#2b7a3d;font-weight:600}.dead{color:#b3261e;font-weight:600}.cause{font-weight:600}
table{border-collapse:collapse;font-size:.95em;background:#fff;border-radius:10px;overflow:hidden;box-shadow:0 1px 3px rgba(0,0,0,.06)}
table.clean{width:100%}
th{text-align:left;font-weight:600;color:#6b7280;font-size:.85em;padding:.7em .9em .5em;border-bottom:1px solid #e6e4df;background:#fff}
td{padding:.5em .9em;vertical-align:top;border:0}tbody tr:nth-child(even) td{background:#f5f4f0}
td.n,th.n{text-align:right;font-variant-numeric:tabular-nums}
.serie{background:#fff;border-radius:12px;padding:.9em 1.2em;margin:0 0 1em;box-shadow:0 1px 3px rgba(0,0,0,.06)}
.serie h2{margin:0 0 .2em;font-size:1.15em}
form.f{display:grid;grid-template-columns:max-content 1fr;gap:.5em 1em;align-items:center;max-width:820px;background:#fff;padding:1.2em;border-radius:12px;box-shadow:0 1px 3px rgba(0,0,0,.06)}
form.f label{font-weight:600}form.f .hint{grid-column:2;color:#6b7280;font-size:.85em;margin-top:-.3em}
form.f textarea{min-height:4em;width:100%;box-sizing:border-box}form.f input[type=text],form.f input[type=number]{max-width:14em}
form.f .wide{grid-column:1/3}
input,select,textarea{font:inherit}
button{font:inherit;padding:.35em 1em;border:1px solid #cfd3d8;border-radius:8px;background:#fff;cursor:pointer;color:#1f2328}
button:hover{background:#f3f2ee}
button.main{background:#2f3e46;color:#fff;border-color:#2f3e46}button.danger{background:#b4433a;color:#fff;border-color:#b4433a}
.warn{color:#8a4b00}.err{background:#f6d3cf;padding:.7em 1em;border-radius:8px}
.plans td form{display:inline;margin-right:.4em}.confirm{display:inline-block;margin-left:.6em}
.version code{background:#eceae4;padding:0 .35em;border-radius:4px;font-size:.9em}
.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(330px,1fr));gap:1em}
svg.chart{width:100%;height:auto;background:#fff;border-radius:12px;font-family:system-ui,sans-serif;box-shadow:0 1px 3px rgba(0,0,0,.06)}
.mapbox{background:#fff;border-radius:12px;padding:1em;box-shadow:0 1px 3px rgba(0,0,0,.06)}svg.map{width:100%;height:auto;display:block;border-radius:6px}
.timeline{display:flex;gap:1em;align-items:center;margin:.9em 0 .2em;flex-wrap:wrap}.timeline input[type=range]{flex:1;min-width:12em}
.timeline .when{font-weight:600;min-width:13em}.legend{color:#555;font-size:.92em;margin:.4em 0 0}
.cols{display:grid;grid-template-columns:1fr 1fr;gap:1em;align-items:start}.cols2{display:grid;grid-template-columns:1fr 1fr;gap:1.2em;align-items:start}
@media (max-width:900px){.cols,.cols2{grid-template-columns:1fr}}
section.tribe{background:#fff;border-radius:12px;padding:.9em 1.2em;margin:0 0 1em;box-shadow:0 1px 3px rgba(0,0,0,.06)}
section.tribe p{margin:.3em 0 .6em}section.tribe table{box-shadow:none;margin-top:.6em}
section.tribe ul,ol.kept{margin:.2em 0 .4em;padding-left:1.4em}ol.kept{font-size:.85em;max-height:22em;overflow:auto}
.scroll{overflow-x:auto}table.years,table.people{font-size:.85em}details summary{cursor:pointer;color:#2f3e46;font-weight:600}
details.detail{margin-top:2.5em;border-top:1px solid #e6e4df;padding-top:1em}details.detail>summary{font-size:1.25em;font-weight:650}
details.detail>summary::after{content:" — tout le reste : réglages, autres courbes, tableau des années, la vie de chacun";font-weight:400;color:#8a8f98;font-size:.75em}
ul.lines{font-size:.8em;columns:2;margin:.2em 0}.controls{background:#fff;border-radius:12px;padding:.7em 1.2em;margin:.6em 0 1em;box-shadow:0 1px 3px rgba(0,0,0,.06)}
form.inline{display:inline-block;margin:.2em 1em .2em 0}form.inline select{max-width:28em}
.plans input[type=text]{width:16em}
"""


def page(title, body, tab, port):
    nav = "".join(f'<a href="{href}" class="{"on" if tab == key else ""}">{label}</a>'
                  for key, href, label in (("cours", "/", "En cours"), ("nouvelle", "/nouvelle", "Nouvelle série"),
                                           ("lire", "/lire", "Lire / Comparer")))
    return f"""<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>{html.escape(title)} — laboratoire</title><style>{CSS}</style></head><body>
<header><strong>Laboratoire</strong>{nav}<small>127.0.0.1:{port}</small></header>
<main>{body}</main></body></html>"""


RUNNING_JS = """
const fmt = v => v === null || v === undefined ? '-' : v;
const cls = s => s === 'en cours' || s === 'attend' ? 'cours' : s === 'fini' || s.startsWith('terminée') ? 'fini'
  : s.startsWith('ÉCHEC') || s === 'perdu' || s === 'perdue' ? 'echec' : s.startsWith('arrêt') ? 'arret' : '';
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const secs = s => s >= 3600 ? Math.floor(s/3600)+' h '+Math.floor(s%3600/60)+' min' : s >= 60 ? Math.floor(s/60)+' min '+(s%60)+' s' : s+' s';
const pending = new Set();
function render(series) {
  const root = document.getElementById('series');
  if (!series.length) { root.innerHTML = '<p class="muted">Aucune série dans le dossier des runs. <a href="/nouvelle">Créer la première</a>.</p>'; return; }
  root.innerHTML = series.map(s => {
    const running = s.state === 'en cours' && s.owned;
    const stop = running ? (pending.has(s.dir)
      ? `<span class="confirm">Arrêter la série <b>${esc(s.name)}</b> et tuer ses runs ? <form method="post" action="/arreter" style="display:inline"><input type="hidden" name="serie" value="${esc(s.dir)}"><button class="danger">Oui, arrêter</button></form> <button data-cancel="${esc(s.dir)}">Non</button></span>`
      : `<button data-stop="${esc(s.dir)}" class="confirm">Arrêter</button>`) : '';
    const rows = s.runs.map(r => `<tr><td>${r.page ? `<a href="${r.page}">${esc(r.name)}</a>` : esc(r.name)}</td>
      <td><span class="etat ${cls(r.state)}">${esc(r.state)}</span></td><td class="n">${fmt(r.year)}</td><td class="n">${fmt(r.alive)}</td>
      <td class="n">${fmt(r.dead)}</td><td class="n">${secs(r.elapsed)}</td><td>${r.log ? `<a href="${r.log}">log</a>` : ''}</td></tr>`).join('');
    const final = s.final ? `<p><b>${esc(s.final)}</b>${s.synthese ? ` — <a href="${s.synthese}">synthèse</a>` : ''}</p>` : '';
    const note = s.note ? `<p class="warn">${esc(s.note)}</p>` : '';
    const meta = [s.created ? 'créée le ' + esc(s.created.replace('T', ' à ')) : '', s.jobs ? s.jobs + ' à la fois' : '',
      s.owned ? '' : 'lancée hors du laboratoire', !s.final ? secs(s.elapsed) : ''].filter(Boolean).join(' · ');
    return `<section class="serie"><h2>${esc(s.name)} <span class="etat ${cls(s.state)}">${esc(s.state)}</span>${stop}</h2>
      <p class="muted">${meta}</p>${s.question ? `<p class="q">${esc(s.question)}</p>` : ''}${note}${final}
      <table><tr><th>run</th><th>état</th><th>année</th><th>vivants</th><th>morts</th><th>temps</th><th></th></tr>${rows}</table></section>`;
  }).join('');
  root.querySelectorAll('[data-stop]').forEach(b => b.onclick = () => { pending.add(b.dataset.stop); render(last); });
  root.querySelectorAll('[data-cancel]').forEach(b => b.onclick = () => { pending.delete(b.dataset.cancel); render(last); });
}
let last = [];
async function tick() {
  try { const r = await fetch('/api/series'); last = await r.json(); render(last); document.getElementById('err').textContent = ''; }
  catch (e) { document.getElementById('err').textContent = 'Le laboratoire ne répond plus (' + e + ').'; }
}
tick(); setInterval(tick, 2000);
"""

FORM_JS = """
const days = document.getElementById('days'), years = document.getElementById('years');
const upd = () => { const d = parseInt(days.value, 10); years.textContent = isNaN(d) ? '' : '= ' + (d / 50).toFixed(d % 50 ? 2 : 0) + ' an(s) de 50 jours'; };
days.oninput = upd; upd();
const world = document.getElementById('world'), side = document.getElementById('side-row');
const sideLabel = document.getElementById('side-row-label');
const sw = () => { const d = world.value === 'patch' ? '' : 'none'; side.style.display = d; sideLabel.style.display = d; };
world.onchange = sw; sw();
const cad = document.getElementById('cadence'), cw = document.getElementById('cadence-warn');
const cu = () => { cw.textContent = cad.value === '1' ? '' : 'Cadence ' + cad.value + ' : le jeu tourne à 10, soit dix fois plus lent qu’à 1.'; };
cad.oninput = cu; cu();
"""


def running_page(port):
    body = ('<h1>Les séries</h1><p id="err" class="warn"></p><div id="series"><p class="muted">Chargement…</p></div>'
            f'<script>{RUNNING_JS}</script>')
    return page("En cours", body, "cours", port)


def form_page(lab, fields, port, error="", title="Nouvelle série"):
    h = html.escape
    f = dict(FORM_DEFAULTS)
    f.update(fields or {})
    if not f["name"]:
        f["name"] = time.strftime("%Y-%m-%d-")
    chk = lambda k: ' checked' if f.get(k) else ''  # noqa: E731
    sel = lambda k, v: ' selected' if f.get(k) == v else ''  # noqa: E731
    err = f'<p class="err">{h(error)}</p>' if error else ""
    rows = f"""
<label for="name">Nom de la série</label><input type="text" id="name" name="name" value="{h(f['name'])}" required>
<span class="hint">Le dossier des runs et le nom du plan : <code>{h(lab.runs_dir)}/&lt;nom&gt;/</code>, <code>plans/&lt;nom&gt;.json</code>.</span>
<label for="question">Question</label><textarea id="question" name="question" class="wide" style="grid-column:2">{h(f['question'])}</textarea>
<span class="hint">Ce que la série doit répondre ; recopiée en tête de la synthèse.</span>
<label for="change">Ce qui a changé</label><input type="text" id="change" name="change" value="{h(f['change'])}" style="max-width:100%" placeholder="D50 b : le juge compte la soif">
<span class="hint">Une phrase sur la version du vivarium qui tourne ; gardée avec son commit dans <code>serie.json</code>, affichée partout où la série paraît.</span>
<label for="world">Monde</label><select id="world" name="world"><option value="vivarium"{sel('world', 'vivarium')}>vivarium (zone 2 de la graine 14, 2 176 m)</option><option value="patch"{sel('world', 'patch')}>patch de la vallée</option></select>
<label for="side" id="side-row-label">Côté du patch</label><span id="side-row"><input type="number" id="side" name="side" min="8" value="{h(f['side'])}"> tuiles</span>
<label for="tribes">Tribus</label><select id="tribes" name="tribes"><option value=""{sel('tribes', '')}>défaut du monde (2 sur le vivarium, 1 sur un patch)</option><option value="1"{sel('tribes', '1')}>1</option><option value="2"{sel('tribes', '2')}>2</option></select>
<label for="people">Adultes par camp</label><input type="number" id="people" name="people" min="1" value="{h(f['people'])}">
<label for="children">Enfants par camp</label><input type="number" id="children" name="children" min="0" value="{h(f['children'])}">
<label for="infants">Nourrissons par camp</label><input type="number" id="infants" name="infants" min="0" value="{h(f['infants'])}">
<label for="ages">Âges des adultes</label><input type="text" id="ages" name="ages" value="{h(f['ages'])}" pattern="\\d+-\\d+">
<span class="hint">LO-HI en années, répartis ; 20-20 pour un couple de vingt ans.</span>
<label for="fed">Nourris sans limite</label><span><input type="checkbox" id="fed" name="fed"{chk('fed')}> <code>--fed</code> : la faim d'un jour vaut 0, personne ne maigrit</span>
<label for="days">Jours</label><span><input type="number" id="days" name="days" min="1" value="{h(f['days'])}"> <span id="years" class="muted"></span></span>
<label for="seeds">Graines</label><input type="text" id="seeds" name="seeds" value="{h(f['seeds'])}">
<span class="hint">Une liste (1,2,3) ou une plage (1-10), ou les deux (1-3,7). Un run par graine.</span>
<label for="both">Avec et sans épreuves</label><span><input type="checkbox" id="both" name="both"{chk('both')}> deux runs par graine : <code>-avec</code> et <code>-sans</code> (<code>--no-trials</code>, le témoin de D50)</span>
<label for="cadence">Cadence</label><span><input type="number" id="cadence" name="cadence" min="1" value="{h(f['cadence'])}"> ticks par seconde de jeu <span id="cadence-warn" class="warn"></span></span>
<span class="hint">1 par défaut ; le jeu tourne à 10, dix fois plus lent.</span>
<label for="jobs">Runs à la fois</label><input type="number" id="jobs" name="jobs" min="1" value="{h(f['jobs'])}" placeholder="{DEFAULT_JOBS}">
<span class="hint">Vide : {lab.jobs} (les cœurs moins deux).</span>
<label for="run_name">Nom des runs</label><input type="text" id="run_name" name="run_name" value="{h(f['run_name'])}" placeholder="monde-adultes">
<span class="hint">Chaque run s'appelle <code>&lt;nom&gt;-s&lt;graine&gt;[-avec|-sans]</code>.</span>
<span class="wide"><button class="main">Créer le plan et lancer la série</button> <span class="muted">binaire : <code>{h(lab.binary)}</code>, contenu : <code>{h(lab.content)}</code></span></span>
"""
    plans = lab.plans()
    plan_rows = "".join(
        f'<tr><td>{h(p)}</td><td>'
        f'<form method="post" action="/relancer"><input type="hidden" name="plan" value="{h(p)}">'
        f'<input type="text" name="change" placeholder="ce qui a changé"> <button>Relancer</button></form>'
        f'<form method="get" action="/nouvelle"><input type="hidden" name="plan" value="{h(p)}"><button>Dupliquer en changeant…</button></form>'
        f'</td></tr>' for p in plans)
    plans_html = (f'<h2>Les plans existants</h2><p class="muted">Relancer refait la série dans le même dossier (les anciens fichiers sont effacés) ; '
                  f'pour comparer deux versions, lancez plutôt un plan étalon ci-dessous, qui fait une série neuve à chaque fois.</p>'
                  f'<table class="plans"><tr><th>plan</th><th></th></tr>{plan_rows}</table>'
                  if plans else '<p class="muted">Aucun plan encore dans <code>plans/</code>.</p>')
    standard = lab.standard_plans()
    std_rows = "".join(
        f'<tr><td>{h(p)}</td><td>{h(standard_question(p))}</td><td>'
        f'<form method="post" action="/etalon"><input type="hidden" name="plan" value="{h(p)}">'
        f'<input type="text" name="change" placeholder="ce qui a changé" required> <button class="main">Lancer une série</button></form></td></tr>'
        for p in standard)
    std_html = (f'<h2>Les plans étalons</h2><p class="muted">Dans le dépôt, <code>tools/lab/plans/</code> : chaque lancement fait une série neuve '
                f'<code>&lt;plan&gt;-&lt;date&gt;</code> avec sa version, à comparer depuis « Lire / Comparer ». '
                f'En ligne de commande : <code>python3 -I tools/lab/lab.py run tools/lab/plans/&lt;plan&gt;.json --change "…" --worktree DIR</code>.</p>'
                f'<table class="plans"><tr><th>plan</th><th>question</th><th></th></tr>{std_rows}</table>') if standard else ""
    body = (f'<h1>{h(title)}</h1>{err}<form class="f" method="post" action="/creer">{rows}</form>'
            f'<script>{FORM_JS}</script>{std_html}{plans_html}')
    return page(title, body, "nouvelle", port)


def standard_question(name):
    try:
        with open(os.path.join(STANDARD_PLANS, name + ".json")) as f:
            return json.load(f).get("question", "")
    except (OSError, ValueError):
        return ""


def not_found(port, what="Page introuvable"):
    return page("Introuvable", f"<h1>{html.escape(what)}</h1><p><a href=\"/lire\">Lire / Comparer</a></p>", "lire", port)


def target(lab, spec):
    """`serie` or `serie/run` → (series, run or None), or a French reason."""
    parts = [p for p in (spec or "").split("/") if p]
    if not parts or len(parts) > 2 or not all(NAME_RE.match(p) for p in parts):
        return None, f"cible invalide : {spec!r}"
    found = lab.find(*parts)
    if not found:
        return None, f"introuvable : {spec}"
    return found, None


def compare_page(lab, a, b, port):
    ta, ea = target(lab, a)
    tb, eb = target(lab, b)
    if ea or eb:
        return page("Comparer", f'<h1>Comparer</h1><p class="err">{html.escape(ea or eb)}</p>', "lire", port), 404
    (sa, ra), (sb, rb) = ta, tb
    if ra is None and rb is None:
        body = reading.compare_series_body(sa, sb, lab.store)
    else:
        if ra is None:
            ra = sa["runs"][0] if sa["runs"] else None
        if rb is None:
            rb = sb["runs"][0] if sb["runs"] else None
        if ra is None or rb is None:
            return page("Comparer", '<h1>Comparer</h1><p class="err">une des séries n’a aucun run</p>', "lire", port), 404
        body = reading.compare_runs_body(sa, ra, lab.store.load(sa["name"], ra["name"]),
                                         sb, rb, lab.store.load(sb["name"], rb["name"]))
    return page("Comparer", body, "lire", port), 200


# --- the server ---------------------------------------------------------------

class Handler(BaseHTTPRequestHandler):
    lab = None
    port = 0
    server_version = "soul-lab/1"
    sys_version = ""

    def log_message(self, fmt, *args):
        if "/api/" in (args[0] if args else "") or "/runs/" in (args[0] if args else ""):
            return
        sys.stderr.write("lab: " + fmt % args + "\n")

    def _send(self, code, body, ctype="text/html; charset=utf-8", headers=()):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def _page(self, body, code=200):
        self._send(code, body, headers=[("Content-Security-Policy",
                                         "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; "
                                         "img-src data:; connect-src 'self'; form-action 'self'; base-uri 'none'")])

    def _redirect(self, where):
        self.send_response(303)
        self.send_header("Location", where)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        url = urllib.parse.urlsplit(self.path)
        q = urllib.parse.parse_qs(url.query)
        if url.path == "/":
            self._page(running_page(self.port))
        elif url.path == "/nouvelle":
            fields, error = {}, ""
            name = (q.get("plan") or [""])[0]
            if name:
                try:
                    fields = form_from_plan(self.lab.load_plan(name), "")
                    fields["name"] = ""
                except (OSError, ValueError):
                    error = f"plan {name!r} illisible"
            self._page(form_page(self.lab, fields, self.port, error,
                                 f"Nouvelle série, d'après {name}" if name else "Nouvelle série"))
        elif url.path == "/lire":
            self._page(page("Lire / Comparer", reading.lire_body(self.lab.series(), self.lab.store), "lire", self.port))
        elif url.path.startswith("/serie/"):
            found, err = target(self.lab, urllib.parse.unquote(url.path[len("/serie/"):]))
            if err or found[1] is not None:
                self._page(not_found(self.port, "Série introuvable"), 404)
            else:
                self._page(page(f"Série {found[0]['name']}", reading.serie_body(found[0], self.lab.series(), self.lab.store), "lire", self.port))
        elif url.path.startswith("/run/"):
            found, err = target(self.lab, urllib.parse.unquote(url.path[len("/run/"):]))
            if err or found[1] is None:
                self._page(not_found(self.port, "Run introuvable"), 404)
            else:
                s, r = found
                self._page(page(f"Run {r['name']}", reading.run_body(s, r, self.lab.store.load(s["name"], r["name"])), "lire", self.port))
        elif url.path == "/compare":
            body, code = compare_page(self.lab, (q.get("a") or [""])[0], (q.get("b") or [""])[0], self.port)
            self._page(body, code)
        elif url.path == "/api/series":
            self._send(200, json.dumps(self.lab.series(), ensure_ascii=False), "application/json; charset=utf-8")
        elif url.path.startswith("/runs/"):
            self._file(url.path[len("/runs/"):])
        else:
            self._page(page("Introuvable", "<h1>Page introuvable</h1>", "", self.port), 404)

    def do_HEAD(self):
        self.do_GET()

    def _file(self, rel):
        rel = urllib.parse.unquote(rel)
        path = os.path.realpath(os.path.join(self.lab.runs_dir, rel))
        root = os.path.realpath(self.lab.runs_dir)
        types = {".html": "text/html; charset=utf-8", ".log": "text/plain; charset=utf-8",
                 ".json": "application/json; charset=utf-8", ".exit": "text/plain; charset=utf-8",
                 ".txt": "text/plain; charset=utf-8", ".css": "text/css", ".svg": "image/svg+xml", ".png": "image/png"}
        ext = os.path.splitext(path)[1]
        if not path.startswith(root + os.sep) or ext not in types or not os.path.isfile(path):
            self._page(page("Introuvable", "<h1>Fichier introuvable</h1>", "", self.port), 404)
            return
        with open(path, "rb") as f:
            data = f.read()
        self._send(200, data, types[ext], [("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:")])

    def _same_origin(self):
        origin = self.headers.get("Origin") or self.headers.get("Referer") or ""
        if not origin:
            return True
        host = urllib.parse.urlsplit(origin).netloc
        return host in (f"127.0.0.1:{self.port}", f"localhost:{self.port}")

    def _fields(self):
        length = int(self.headers.get("Content-Length") or 0)
        if length > 200_000:
            raise ValueError("formulaire trop long")
        raw = self.rfile.read(length).decode(errors="replace")
        return {k: v[0] for k, v in urllib.parse.parse_qs(raw, keep_blank_values=True).items()}

    def do_POST(self):
        if not self._same_origin():
            self._page(page("Refusé", "<h1>Refusé</h1><p>Requête d'une autre origine.</p>", "", self.port), 403)
            return
        url = urllib.parse.urlsplit(self.path)
        try:
            fields = self._fields()
        except ValueError as e:
            self._page(page("Refusé", f"<h1>Refusé</h1><p>{html.escape(str(e))}</p>", "", self.port), 400)
            return
        try:
            if url.path == "/creer":
                plan = plan_from_form(fields, self.lab.runs_dir, self.lab.binary, self.lab.content)
                name = os.path.basename(plan["out"])
                if os.path.exists(self.lab.plan_path(name)) or os.path.exists(plan["out"]):
                    raise ValueError(f"le nom {name!r} existe déjà : relancez ce plan, ou choisissez un autre nom")
                self.lab.write_plan(name, plan)
                self.lab.launch(plan)
                self._redirect("/")
            elif url.path == "/relancer":
                name = fields.get("plan", "")
                try:
                    plan = self.lab.load_plan(name)
                except OSError:
                    raise ValueError(f"plan {name!r} introuvable") from None
                change = (fields.get("change") or "").strip()
                if change:
                    plan["change"] = change
                    self.lab.write_plan(name, plan)
                self.lab.launch(plan)
                self._redirect("/")
            elif url.path == "/etalon":
                name = fields.get("plan", "")
                if name not in self.lab.standard_plans():
                    raise ValueError(f"plan étalon {name!r} introuvable")
                change = (fields.get("change") or "").strip()
                if not change:
                    raise ValueError("dites ce qui a changé : la phrase est gardée avec la version de la série")
                serie, plan = self.lab.plan_from_file(os.path.join(STANDARD_PLANS, name + ".json"), change=change,
                                                      worktree=self.lab.worktree)
                self.lab.write_plan(serie, plan)
                self.lab.launch(plan)
                self._redirect("/")
            elif url.path == "/arreter":
                name = fields.get("serie", "")
                if not NAME_RE.match(name):
                    raise ValueError("nom de série invalide")
                self.lab.stop(os.path.join(self.lab.runs_dir, name))
                self._redirect("/")
            else:
                self._page(page("Introuvable", "<h1>Page introuvable</h1>", "", self.port), 404)
        except ValueError as e:
            if url.path == "/creer":
                self._page(form_page(self.lab, fields, self.port, str(e)), 400)
            else:
                self._page(page("Refusé", f"<h1>Impossible</h1><p>{html.escape(str(e))}</p><p><a href=\"/\">Retour</a></p>", "", self.port), 400)


def run_cli(argv):
    """`lab.py run PLAN --change "…"`: launch a series from a plan file, detached, without the server.

    The parent writes the plan under `<runs-dir>/plans/<name>.json`, starts a
    child `lab.py run … --attached` in its own session and returns at once; the
    child owns the runs to the end and writes `serie.json` (its pid under
    `lab.pid`), so the "En cours" screen sees the series through its files.
    `--attached` runs in the foreground instead (the tests, or waiting for it).
    """
    ap = argparse.ArgumentParser(prog="lab.py run", description="Launch a series from a plan file, without the browser.")
    ap.add_argument("plan", help="a plan file (tools/lab/plans/*.json, or any soul-runs plan)")
    ap.add_argument("--change", default="", help='what changed in the simulation, one sentence ("D50 b : le juge compte la soif")')
    ap.add_argument("--worktree", help="the git worktree the binary was built from: its commit and branch are recorded")
    ap.add_argument("--binary", help=f"the vivarium to run (default: the plan's, else {DEFAULT_BIN})")
    ap.add_argument("--content", help="the content directory (default: the plan's, else <worktree>/content, else the repository's)")
    ap.add_argument("--runs-dir", default=DEFAULT_RUNS)
    ap.add_argument("--name", help="the series' name (default: <plan stem>-<date>)")
    ap.add_argument("--jobs", type=int, help="runs at a time (default: the plan's, else the cores minus two)")
    ap.add_argument("--attached", action="store_true", help="run in the foreground until the series ends")
    a = ap.parse_args(argv)
    content = a.content
    if not content and a.worktree and os.path.isdir(os.path.join(os.path.expanduser(a.worktree), "content")):
        content = os.path.join(os.path.expanduser(a.worktree), "content")
    lab = Lab(a.runs_dir, a.binary or DEFAULT_BIN, content or DEFAULT_CONTENT, a.jobs or DEFAULT_JOBS, a.worktree)
    try:
        name, plan = lab.plan_from_file(a.plan, a.name, a.change, a.worktree)
    except (OSError, ValueError) as e:
        print(f"lab: {e}", file=sys.stderr)
        return 2
    if a.binary:
        plan["binary"] = os.path.abspath(os.path.expanduser(a.binary))
    if content:
        plan["content"] = os.path.abspath(os.path.expanduser(content))
    if a.jobs:
        plan.setdefault("lab", {})["jobs"] = a.jobs
    if os.path.exists(plan["out"]) and lab.is_running(plan["out"]):
        print(f"lab: la série {name} tourne encore", file=sys.stderr)
        return 1
    lab.write_plan(name, plan)
    if not a.attached:
        os.makedirs(plan["out"], exist_ok=True)
        cmd = [sys.executable, "-I", os.path.abspath(__file__), "run", lab.plan_path(name), "--attached",
               "--name", name, "--runs-dir", lab.runs_dir, "--change", a.change]
        if a.worktree:
            cmd += ["--worktree", a.worktree]
        with open(os.path.join(plan["out"], "lab.log"), "a") as log:
            p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, start_new_session=True)
        print(f"série {name} lancée (pid {p.pid}) : {plan['out']}\n  à lire : http://127.0.0.1:8765/serie/{urllib.parse.quote(name)}")
        return 0
    s = lab.launch(plan, a.change)

    def leave(*_):
        lab.stop_all()
        os._exit(1)

    signal.signal(signal.SIGINT, leave)
    signal.signal(signal.SIGTERM, leave)
    print(f"lab: série {name} ({len(s.runs)} runs, {s.jobs} à la fois) → {s.out}", file=sys.stderr)
    while not s.done:
        time.sleep(0.5)
    status = [run_status(s.out, r, time.time(), False)["state"] for r in s.runs]
    print(f"lab: série {name} terminée : {', '.join(status)}", file=sys.stderr)
    return 0 if all(st == "fini" for st in status) else 1


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == "run":
        return run_cli(argv[1:])
    ap = argparse.ArgumentParser(description="The laboratory: create and follow series of vivarium runs.")
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--runs-dir", default=DEFAULT_RUNS)
    ap.add_argument("--binary", default=DEFAULT_BIN)
    ap.add_argument("--content", default=DEFAULT_CONTENT)
    ap.add_argument("--jobs", type=int, default=DEFAULT_JOBS, help="runs at a time when the plan says nothing")
    ap.add_argument("--worktree", help="the git worktree the binary was built from, recorded as the version of every series launched here")
    a = ap.parse_args(argv)
    lab = Lab(a.runs_dir, a.binary, a.content, a.jobs, a.worktree)
    Handler.lab = lab
    Handler.port = a.port
    server = ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    server.daemon_threads = True

    def leave(*_):
        print("\nlab: stopping, my runs end with me.", file=sys.stderr)
        lab.stop_all()
        os._exit(0)

    signal.signal(signal.SIGINT, leave)
    signal.signal(signal.SIGTERM, leave)
    if not os.access(lab.binary, os.X_OK):
        print(f"lab: warning, no vivarium binary at {lab.binary} (a series would wait for it)", file=sys.stderr)
    print(f"Laboratoire : http://127.0.0.1:{a.port}/  (runs : {lab.runs_dir}, pid {os.getpid()}, Ctrl-C pour quitter)")
    sys.stdout.flush()
    server.serve_forever()
    return 0


if __name__ == "__main__":
    sys.exit(main())
