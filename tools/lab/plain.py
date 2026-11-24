"""The series page and the run page in plain words (D52, amended 24 November 2026).

The project lead, 24 November 2026, on the series page of before: "Il n'y a rien qui va." The
series page is now its question, its answer and ONE table: a row a run, named by what differs, its
cells coloured against the run's own reference. The run page opens with a breadcrumb and one
summary line, then always the same parts in the same order, each opened by one plain sentence.
Everything is read from the runs' data files (`docs/10-vivarium-data.md`) and from `serie.json`
(its optional `note`, `ref` and `plan.answer`). Standard library only.
"""

import html
import json
import math
import os
import re
import statistics
import threading
import urllib.parse

import panels
import reading
import scarcity

h = html.escape
fmt = reading.fmt
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")   # lab.NAME_RE: the names of series and runs
NONE = "—"


# --- the measures of a run: what the series' table shows ----------------------------------------

def noon_spring_median(d):
    """The median distance of the bodies at noon to their tribe's camp of that year, in spring,
    in metres (tiles), on the longer axis; None when the data cannot say (no positions, no camps,
    no people's tribes). The camp of a year is `years[].camp` of that tribe, else the year before."""
    dpy = reading.year_days(d)
    quarter = max(1, dpy // 4)
    tribe = {p.get("id"): p.get("tribe") for p in d.get("people", []) if p.get("id") is not None}
    camps = {(y.get("tribe"), y.get("year")): y.get("camp") for y in d.get("years", []) if y.get("camp")}
    dist = []
    for entry in d.get("positions") or []:
        if not isinstance(entry, list) or len(entry) != 2:
            continue
        day, bodies = entry
        if ((day - 1) // quarter) % 4 != 0:   # season 0, spring, counted from the run's start
            continue
        yr = (day - 1) // dpy + 1
        for b in bodies:
            t = tribe.get(b[0])
            c = camps.get((t, yr)) or camps.get((t, yr - 1))
            if not c:
                continue
            dist.append(max(abs(b[1] - c[0]), abs(b[2] - c[1])))
    return round(statistics.median(dist)) if dist else None


def together_percent(d):
    """The fertile couples' dawns spent together, in percent (`nights_apart`); None without it."""
    na = d.get("nights_apart")
    if not isinstance(na, list):
        return None
    dawns = sum(x.get("dawns") or 0 for x in na)
    if not dawns:
        return None
    return round(100 * sum(x.get("together") or 0 for x in na) / dawns)


def measures(d):
    """A run's figures for the table; None where the data does not hold the measure."""
    sm = reading.summary(d)
    deaths = sm["deaths"]
    hc = d.get("home_changes")
    wall = ((d.get("meta") or {}).get("timing") or {}).get("wall_s")
    return {"start": sm["start"], "births": sm["births"], "deaths": sm["dead"], "hunger": deaths.get("hunger", 0),
            "alive": sm["alive"], "lived": sm["start"] + sm["births"], "together": together_percent(d),
            "noon_spring": noon_spring_median(d), "moves": len(hc) if isinstance(hc, list) else None,
            "wall": wall if isinstance(wall, (int, float)) and wall > 0 else None,
            "chain": scarcity.measures(d)}   # the second table, « La chaîne du manque » (scarcity.py)


_MEASURES = {}
_MEASURES_LOCK = threading.Lock()


def measures_of(store, serie, run):
    """A run's measures, worked out once per file and kept by its mtime; None without data."""
    path = store.path(serie, run)
    try:
        mtime = os.path.getmtime(path)
    except OSError:
        return None
    with _MEASURES_LOCK:
        hit = _MEASURES.get(path)
    if hit and hit[0] == mtime:
        return hit[1]
    d = store.load(serie, run)
    if d is None:
        return None
    m = measures(d)
    with _MEASURES_LOCK:
        _MEASURES[path] = (mtime, m)
    return m


# The table's columns: key, French header, the one-line explanation, the direction of better (+1
# more is better, -1 fewer is better, 0 neither) and the dead zone (an absolute floor and a share of
# the reference): a gap within it is noise and stays uncoloured.
COLUMNS = [
    ("start", "au départ", "les gens vivants à la première journée", 0, (0, 0)),
    ("births", "naissances", "les enfants nés pendant le run", 1, (2, 0.05)),
    ("deaths", "morts (dont faim)", "tous les morts du run ; entre parenthèses, ceux morts de faim", -1, (2, 0.05)),
    ("alive", "vivants à la fin", "les gens encore en vie à la dernière journée", 1, (2, 0.05)),
    ("lived", "ayant vécu en tout", "ceux du départ plus les naissances : tous ceux qui ont vécu à un moment du run", 1, (2, 0.05)),
    ("together", "couples ensemble à l’aube", "sur les matins où une femme pouvait concevoir et avait un mari vivant, "
     "la part où les deux se réveillaient l’un près de l’autre", 1, (3, 0)),
    ("noon_spring", "distance du camp à midi, au printemps", "à midi, au printemps, à quelle distance de son camp du cycle se tient "
     "chacun ; la valeur du milieu (la médiane), en mètres", 0, (0, 0)),
    ("moves", "déménagements", "combien de fois un adulte a changé de foyer", 0, (0, 0)),
]
HUNGER_ZONE = (2, 0.05)


def judge(value, ref, better, zone):
    """`good`, `bad` or '' for a value against its reference: '' when either is missing, when
    the column has no better side, or when the gap is within the dead zone."""
    if value is None or ref is None or not better:
        return ""
    gap = value - ref
    if abs(gap) <= max(zone[0], zone[1] * abs(ref)):
        return ""
    return "good" if gap * better > 0 else "bad"


def cell_value(key, m):
    v = m.get(key) if m else None
    if v is None:
        return NONE
    return f"{fmt(v)} %" if key == "together" else (f"{fmt(v)} m" if key == "noon_spring" else fmt(v))


def gap_words(v, r, unit=""):
    if v is None or r is None:
        return ""
    g = v - r
    return f"référence : {fmt(r)}{unit} ({'+' if g > 0 else ''}{fmt(g)})"


def title_attr(tip):
    return f' title="{h(tip)}"' if tip else ""


def cells_html(m, ref):
    """A row's cells, each coloured against `ref` (the reference's measures, or None)."""
    out = []
    for key, _head, _expl, better, zone in COLUMNS:
        unit = " %" if key == "together" else (" m" if key == "noon_spring" else "")
        if key == "deaths":
            v, hv = (m or {}).get("deaths"), (m or {}).get("hunger")
            r, hr = (ref or {}).get("deaths"), (ref or {}).get("hunger")
            cls, hcls = judge(v, r, -1, zone), judge(hv, hr, -1, HUNGER_ZONE)
            if v is None:
                out.append(f"<td class=n>{NONE}</td>")
                continue
            tip = " ; ".join(x for x in (gap_words(v, r) and "morts, " + gap_words(v, r),
                                         gap_words(hv, hr) and "de faim, " + gap_words(hv, hr)) if x)
            out.append(f'<td class=n{title_attr(tip)}><span class="{cls}">{fmt(v)}</span> '
                       f'<span class="hg {hcls}">({fmt(hv)})</span></td>')
            continue
        v, r = (m or {}).get(key), (ref or {}).get(key)
        cls = judge(v, r, better, zone)
        tip = gap_words(v, r, unit)
        out.append(f'<td class="n {cls}"{title_attr(tip)}>{h(cell_value(key, m))}</td>')
    return "".join(out)


# --- the names of the runs: their note, else what differs ------------------------------------------

SETTING_FR = {
    "trials": lambda v: "avec essais" if v else "sans essais",
    "fire": lambda v: "avec le feu" if v else "sans le feu",
    "fed": lambda v: "nourris sans limite" if v else "nourriture du monde",
    "brain": lambda v: "avec cerveau" if v else "sans cerveau",
    "moves": lambda v: "déménagements permis" if v else "sans déménagement",
    "decisions": lambda v: "avec décisions" if v else "sans décisions",
    "world": lambda v: f"monde {v}",
    "side": lambda v: f"côté {v}" if v else "sans côté",
    "tribes": lambda v: reading.plural(v, "tribu"),
    "people": lambda v: f"{fmt(v)} adultes par camp",
    "children": lambda v: f"{fmt(v)} enfants par camp",
    "infants": lambda v: f"{fmt(v)} nourrissons par camp",
    "adult_years": lambda v: f"âges {v[0]}–{v[1]}" if isinstance(v, list) and len(v) == 2 else f"âges {v}",
    "days": lambda v: f"{fmt(v)} journées",
    "content": lambda v: f"contenu « {v} »",
    "ticks_per_second": lambda v: f"cadence {fmt(v)}",
    "base": lambda v: str(v),
}


def setting_fr(key, value):
    """One setting in French words; a factor by its French name when the lab knows it."""
    if key.startswith("factor:"):
        k = key[len("factor:"):]
        name = panels.FACTOR_FR[k][1] if k in panels.FACTOR_FR else k
        return f"{name} {panels.value_fr(value)}"
    fn = SETTING_FR.get(key)
    return fn(value) if fn else f"{key} {value}"


def run_settings(r, d):
    """What a run is made of: `serie.json`'s words, then its data's settings and factors."""
    out = {"trials": r.get("trials", True), "fire": r.get("fire", True)}
    if r.get("base"):
        out["base"] = r["base"]
    meta = (d or {}).get("meta") or {}
    for k, v in (meta.get("settings") or {}).items():
        if k != "seed":
            out[k] = v
    for f in meta.get("factors") or []:
        if isinstance(f, dict) and f.get("key") is not None:
            out["factor:" + str(f["key"])] = f.get("value")
    return out


def seed_of(r, d):
    s = r.get("seed")
    if s is None and d:
        s = ((d.get("meta") or {}).get("settings") or {}).get("seed")
    return s


def built_names(runs, datas):
    """The runs' names built from what differs between them, plus the seed: `{run: name}`."""
    settings = {r["name"]: run_settings(r, datas.get(r["name"])) for r in runs}
    keys = []
    for st in settings.values():
        keys.extend(k for k in st if k not in keys)
    differ = [k for k in keys if len({json.dumps(st[k], sort_keys=True) for st in settings.values() if k in st}) > 1]
    out = {}
    for r in runs:
        st = settings[r["name"]]
        words = [setting_fr(k, st[k]) for k in differ if k in st]
        seed = seed_of(r, datas.get(r["name"]))
        if seed is not None:
            words.append(f"graine {seed}")
        out[r["name"]] = " · ".join(words) or r["name"]
    return out


def run_names(s, store):
    """Every run of the series named for a human eye: its `note` when it has one, else what differs
    between the series' runs (their settings) and its seed. The data is read only when a run lacks
    a note."""
    runs = s.get("runs") or []
    if all(r.get("note") for r in runs):
        return {r["name"]: noted(r["note"], r.get("seed")) for r in runs}
    datas = {r["name"]: store.load(s["name"], r["name"]) for r in runs} if store is not None else {}
    built = built_names(runs, datas)
    return {r["name"]: noted(r["note"], seed_of(r, datas.get(r["name"]))) if r.get("note") else built[r["name"]] for r in runs}


def noted(note, seed):
    """A run's note as its name, its seed added when the note does not say it (two seeds of one
    variant must not read alike)."""
    note = str(note).strip()
    return note + (f" · graine {seed}" if seed is not None and "graine" not in note else "")


def parse_ref(ref, serie):
    """A run's `ref` as `(series, run)`: `"series/run"`, or a run of the same series alone; None
    when it is not a valid name."""
    if not isinstance(ref, str):
        return None
    parts = [p for p in ref.strip().split("/") if p]
    if len(parts) == 1:
        parts = [serie] + parts
    if len(parts) != 2 or not all(NAME_RE.match(p) for p in parts):
        return None
    return parts[0], parts[1]


def read_serie_file(runs_dir, serie):
    """Another series' `serie.json`, for a reference's note and seed; {} when unreadable."""
    try:
        with open(os.path.join(runs_dir, serie, "serie.json"), encoding="utf-8") as f:
            st = json.load(f)
    except (OSError, ValueError):
        return {}
    return st if isinstance(st, dict) else {}


def ref_label(store, serie, run):
    """A reference in another series: `(name, where)`: its note (with its seed when the note does
    not say it), and the series' title and the run's name in small."""
    st = read_serie_file(store.runs_dir, serie)
    orig = next((r for r in st.get("runs") or [] if isinstance(r, dict) and r.get("name") == run), {})
    seed = orig.get("seed")
    note = orig.get("note")
    if note:
        name = noted(note, seed)
    else:
        name = run + (f" · graine {seed}" if seed is not None else "")
    return name, f"{panels.series_title(serie, st.get('plan'))} › {run}"


# --- the series page -------------------------------------------------------------------------------

def state_words(r):
    if r.get("state") == "en cours":
        return f"en cours, cycle {fmt(r.get('year'))}" if r.get("year") else "en cours"
    return {"attend": "pas encore lancé", "fini": ""}.get(r.get("state"), r.get("state") or "")


def series_rows(s, store):
    """The table's rows: the references first (each once, in the order the runs name them), then
    the series' runs. Each row: `{serie, run, name, where, ref_row, m, ref_m, state}`."""
    names = run_names(s, store)
    own = {r["name"]: r for r in s.get("runs") or []}
    refs, missing = [], []
    for r in s.get("runs") or []:
        p = parse_ref(r.get("ref"), s["name"])
        if p and p not in refs and p != (s["name"], r["name"]):
            refs.append(p)
        elif r.get("ref") and not p:
            missing.append(r["name"])
    rows = []
    for serie, run in refs:
        if serie == s["name"] and run in own:
            name, where = names[run], ""
            state = state_words(own[run])
        else:
            name, where = ref_label(store, serie, run)
            state = ""
        m = measures_of(store, serie, run)
        if m is None and not state:
            state = "introuvable"
        rows.append({"serie": serie, "run": run, "name": name, "where": where, "ref_row": True, "m": m, "state": state,
                     "ref": parse_ref(own[run].get("ref"), serie) if serie == s["name"] and run in own else None})
    for r in s.get("runs") or []:
        if (s["name"], r["name"]) in refs:
            continue
        rows.append({"serie": s["name"], "run": r["name"], "name": names[r["name"]], "where": "", "ref_row": False,
                     "m": measures_of(store, s["name"], r["name"]), "state": state_words(r),
                     "ref": parse_ref(r.get("ref"), s["name"])})
    for row in rows:
        ref = row.pop("ref")
        row["ref_m"] = measures_of(store, *ref) if ref and ref != (row["serie"], row["run"]) else None
        row["ref_name"] = next((x["name"] for x in rows if (x["serie"], x["run"]) == ref), None) if ref else None
    return rows, missing


TABLE_JS = """
document.querySelectorAll('tr[data-href]').forEach(tr => tr.addEventListener('click', e => {
  if (e.target.closest('a')) return;
  location.href = tr.dataset.href;
}));
"""


def series_table(s, store):
    rows, missing = series_rows(s, store)
    if not rows:
        return '<p class=muted>Aucun run dans cette série.</p>'
    head = "".join(f'<th class=n scope=col title="{h(expl)}">{h(name)}</th>' for _k, name, expl, _b, _z in COLUMNS)
    body = []
    for row in rows:
        url = reading.run_url(row["serie"], row["run"])
        mark = '<span class=refmk>référence</span>' if row["ref_row"] else ""
        sub = "".join(f'<span class=sub>{h(x)}</span>' for x in (row["where"], row["state"]) if x)
        vs = f' title="comparé à : {h(row["ref_name"])}"' if row["ref_name"] and row["ref_m"] else ""
        body.append(f'<tr data-href="{h(url)}" class="{"refrow" if row["ref_row"] else ""}">'
                    f'<th scope=row{vs}><a href="{h(url)}">{h(row["name"])}</a>{mark}{sub}</th>{cells_html(row["m"], row["ref_m"])}</tr>')
    legend = "".join(f"<li><b>{h(name)}</b> : {h(expl)}.</li>" for _k, name, expl, _b, _z in COLUMNS)
    warn = (f'<p class=caption>Référence illisible (il faut « série/run ») pour : {h(", ".join(missing))}.</p>' if missing else "")
    colours = ("<p class=caption>Chaque run est comparé à sa référence, la ligne marquée « référence » qu’il nomme : "
               "<span class=good>vert</span> mieux, <span class=bad>rouge</span> moins bien. Plus de naissances, de vivants et de couples "
               "ensemble, c’est mieux ; moins de morts, c’est mieux. Un écart de 5 % ou moins (2 au plus pour un petit nombre, "
               "3 points pour les couples) n’est pas coloré : c’est du bruit. La distance et les déménagements ne sont jamais colorés : "
               "ni plus ni moins n’est meilleur en soi. Survoler une case : la valeur de la référence. Cliquer une ligne : la page du run.</p>")
    return (f'<div class=tablebox><table class=plain><thead><tr><th scope=col>run</th>{head}</tr></thead>'
            f'<tbody>{"".join(body)}</tbody></table></div>{warn}{colours}<ul class=colhelp>{legend}</ul>'
            f'<script>{TABLE_JS}</script>')


def crumbs(items):
    """`Labo › … › …`: every item but the last a link."""
    parts = []
    for i, (label, url) in enumerate(items):
        parts.append(f'<a href="{h(url)}">{h(label)}</a>' if url and i < len(items) - 1 else f'<span>{h(label)}</span>')
    return '<nav class=crumbs aria-label="où suis-je">' + " › ".join(parts) + "</nav>"


def serie_body(s, store):
    """The series page: its question, its answer, its table; the computing time in small at the bottom."""
    title = s.get("title") or s["name"]
    state = "" if (s.get("state") or "").startswith("terminée") else f' <span class=etat>{h(s.get("state") or "")}</span>'
    answer = (s.get("answer") or "").strip()
    out = [crumbs([("Labo", "/"), (title, None)]), f"<h1>{h(title)}{state}</h1>",
           f'<p class=question>{h(s.get("question") or "Question : pas écrite")}</p>',
           f'<p class=answer>{h(answer)}</p>' if answer else '<p class="answer none">Réponse : pas encore écrite</p>',
           series_table(s, store), scarcity.chain_table(series_rows(s, store)[0])]
    walls = []
    for r in s.get("runs") or []:
        m = measures_of(store, s["name"], r["name"])
        if m and m.get("wall"):
            walls.append(f"{h(r['name'])} {h(reading.secs(m['wall']))}")
    foot = []
    if walls:
        foot.append("Temps de calcul de chaque run : " + " · ".join(walls) + ".")
    v = s.get("version") or {}
    if v.get("commit") or v.get("branch") or v.get("change"):
        foot.append("Version : " + h(" ".join(x for x in (v.get("commit") or "commit inconnu", f"(branche {v['branch']})" if v.get("branch") else "") if x))
                    + (f" — {h(v['change'])}" if v.get("change") else "") + ".")
    if foot:
        out.append('<p class=foot>' + "<br>".join(foot) + "</p>")
    return "".join(out)


# --- the run page ------------------------------------------------------------------------------------

DEATH_WORDS = {"hunger": "de faim", "thirst": "de soif", "cold": "de froid", "illness": "de maladie",
               "old_age": "de vieillesse", "childbirth": "en couches"}


def years_words(x):
    """`18`, `2,5` for a count of age-years."""
    r = round(x, 1)
    return fmt(int(r)) if r == int(r) else fmt(r)


def summary_line(d):
    """The run in one line: at the start, births, deaths by cause, alive at the end, lived in all,
    the simulated time in game days and age-years."""
    sm = reading.summary(d)
    by = [(c, n) for c, n in sorted(sm["deaths"].items(), key=lambda x: -x[1]) if n]
    causes = ", ".join(f"{fmt(n)} {DEATH_WORDS.get(c, 'de ' + reading.cause_fr(c))}" for c, n in by)
    days = reading.run_days(d)
    ay = days / reading.age_year_days(d) if days else 0
    parts = [f"<b>{fmt(sm['start'])}</b> au départ", f"<b>{fmt(sm['births'])}</b> naissance{'s' if sm['births'] > 1 else ''}",
             f"<b>{fmt(sm['dead'])}</b> mort{'s' if sm['dead'] > 1 else ''}" + (f" ({h(causes)})" if causes else ""),
             f"<b>{fmt(sm['alive'])}</b> vivant{'s' if sm['alive'] > 1 else ''} à la fin",
             f"<b>{fmt(sm['start'] + sm['births'])}</b> ayant vécu en tout",
             f"<b>{fmt(days)}</b> journées de jeu, soit <b>{years_words(ay)}</b> année{'s' if ay >= 2 else ''} d’âge"]
    return '<p class=sumline>' + " · ".join(parts) + "</p>"


def frise(d):
    """Births and deaths on one line of time: a birth a small mark above the line, a death a mark
    below in the colour of its cause, at its day; the winters in pale bands."""
    days = reading.run_days(d) or 1
    yd = reading.year_days(d)
    W, L, R, Hh = 1000, 50, 14, 170
    mid = Hh // 2
    x = reading.Scale(0, days, L, W - R)
    width = max(1, W - L - R)
    bin_days = max(1, math.ceil(days * 4 / width))
    ups, downs = {}, {}
    for p in d.get("people", []):
        born = p.get("born_day")
        if p.get("born") != "start" and isinstance(born, int):
            ups.setdefault((born - 1) // bin_days, []).append(p)
        died = p.get("died")
        if died and isinstance(died.get("day"), int):
            downs.setdefault((died["day"] - 1) // bin_days, []).append(p)
    if not ups and not downs:
        return '<p class=muted>Aucune naissance ni aucune mort pendant le run.</p>'
    peak = max([len(v) for v in ups.values()] + [len(v) for v in downs.values()] + [1])
    step = min(5.0, (mid - 14) / peak)
    mw = max(1.5, x(bin_days) - x(0) - 1)
    g = [f'<svg class="chart frise" viewBox="0 0 {W} {Hh}" role=img aria-label="naissances et morts sur une frise">']
    yr = 0
    while yr * yd < days:
        w0, w1 = yr * yd + math.ceil(yd * 3 / 4), min(days, (yr + 1) * yd)
        if w1 > w0:
            g.append(f'<rect class="winter" x="{x(w0):.1f}" y="6" width="{x(w1) - x(w0):.1f}" height="{Hh - 26}" fill="#dde6ee">'
                     f'<title>hiver du cycle {yr + 1}</title></rect>')
        yr += 1
    for yr in range(0, math.ceil(days / yd)):
        a, b = yr * yd, min((yr + 1) * yd, days)
        g.append(f'<line x1="{x(a):.1f}" x2="{x(a):.1f}" y1="6" y2="{Hh - 20}" stroke="#eee"/>')
        g.append(f'<text x="{(x(a) + x(b)) / 2:.1f}" y="{Hh - 6}" text-anchor="middle" font-size="11">cycle {yr + 1}</text>')
    g.append(f'<line x1="{L}" x2="{W - R}" y1="{mid}" y2="{mid}" stroke="#888"/>')
    g.append(f'<text x="{L - 6}" y="{mid - 6}" text-anchor="end" font-size="11">nés</text>'
             f'<text x="{L - 6}" y="{mid + 14}" text-anchor="end" font-size="11">morts</text>')

    def mark(b, i, up, p):
        px = x(b * bin_days)
        py = mid - (i + 1) * step if up else mid + i * step + 1
        if up:
            what = f"naissance de {reading.person_fr(p.get('id'))}, tribu {p.get('tribe')}"
            day, color = p.get("born_day"), reading.BIRTH_COLOR
        else:
            cause = (p.get("died") or {}).get("cause")
            what = f"mort {DEATH_WORDS.get(cause, 'de ' + reading.cause_fr(cause))} de {reading.person_fr(p.get('id'))}, tribu {p.get('tribe')}"
            day, color = p["died"]["day"], reading.CAUSE_COLOR.get(cause, "#666")
        return (f'<rect x="{px + .5:.1f}" y="{py:.1f}" width="{mw:.1f}" height="{max(1.0, step - .8):.1f}" fill="{color}">'
                f'<title>{h(reading.day_fr(d, day))} : {h(what)}</title></rect>')
    for b, ps in sorted(ups.items()):
        g.extend(mark(b, i, True, p) for i, p in enumerate(sorted(ps, key=lambda p: p.get("born_day") or 0)))
    for b, ps in sorted(downs.items()):
        g.extend(mark(b, i, False, p) for i, p in enumerate(sorted(ps, key=lambda p: p["died"]["day"])))
    g.append("</svg>")
    cs = [c for c in reading.causes(d) if any((p.get("died") or {}).get("cause") == c for p in d.get("people", []))]
    legend = (f'<i class=sw style="background:{reading.BIRTH_COLOR}"></i>naissance'
              + "".join(f' · <i class=sw style="background:{reading.CAUSE_COLOR.get(c, "#666")}"></i>mort {h(DEATH_WORDS.get(c, reading.cause_fr(c)))}'
                        for c in cs))
    return f'{"".join(g)}<p class=caption>{legend}. Survoler une marque : la journée et la personne.</p>'


APART_FR = [   # the causes of a night apart (`nights_apart[]`), in plain French
    ("homes_apart", "leurs deux foyers sont trop loin l’un de l’autre"),
    ("sleep_out", "l’un dort dehors, loin de chez lui, par habitude"),
    ("goal", "l’un est parti plusieurs journées vers un but lointain"),
    ("home_trial", "l’un essaie un nouveau foyer"),
    ("caught_out", "la nuit a surpris l’un loin de chez lui"),
]


def couples_html(d):
    """The couples: how many formed and parted, the mornings together, why some are apart, and at
    the mornings a woman could conceive, whether her husband was near."""
    seen, formed_start, formed_later = set(), 0, 0
    for p in d.get("people", []):
        for sp in p.get("spouses") or []:
            pair = tuple(sorted((p.get("id"), sp.get("id"))))
            if pair in seen:
                continue
            seen.add(pair)
            if (sp.get("day") or 1) <= 1:
                formed_start += 1
            else:
                formed_later += 1
    parted = sum((y.get("couples") or {}).get("parted") or 0 for y in d.get("years", []))
    again = sum((y.get("couples") or {}).get("taken_again") or 0 for y in d.get("years", []))
    has_couples = any(isinstance(y.get("couples"), dict) for y in d.get("years", []))
    out = [f"<p><b>{fmt(formed_start)}</b> couple{'s' if formed_start > 1 else ''} au départ, <b>{fmt(formed_later)}</b> formé{'s' if formed_later > 1 else ''} pendant le run"
           + (f" ; <b>{fmt(parted)}</b> séparation{'s' if parted > 1 else ''} (mari et femme restés trop longtemps loin l’un de l’autre), "
              f"<b>{fmt(again)}</b> remariage{'s' if again > 1 else ''} après un veuvage ou une séparation" if has_couples else "") + ".</p>"]
    na = d.get("nights_apart")
    if isinstance(na, list) and sum(x.get("dawns") or 0 for x in na):
        dawns = sum(x.get("dawns") or 0 for x in na)
        tog = sum(x.get("together") or 0 for x in na)
        out.append(f"<p>Les matins où une femme pouvait concevoir et avait un mari vivant : <b>{fmt(dawns)}</b> ; le couple se réveillait "
                   f"ensemble <b>{fmt(tog)}</b> fois, soit <b>{round(100 * tog / dawns)} %</b>.</p>")
        apart = [(k, words, sum(x.get(k) or 0 for x in na)) for k, words in APART_FR]
        if dawns - tog:
            out.append("<p>Les matins séparés, et pourquoi :</p><ul class=plainlist>"
                       + "".join(f"<li><b>{fmt(n)}</b> : {h(words)}</li>" for _k, words, n in apart if n) + "</ul>")
        ay = reading.age_year_days(d)
        rows = "".join(f'<tr><td class=n>{i + 1}</td><td class=n>{fmt(x.get("dawns") or 0)}</td><td class=n>{fmt(x.get("together") or 0)}</td>'
                       + "".join(f'<td class=n>{fmt(x.get(k) or 0)}</td>' for k, _w in APART_FR) + "</tr>" for i, x in enumerate(na))
        out.append(f'<details class=fold><summary>Année d’âge par année d’âge (une année d’âge : {fmt(ay)} journées de jeu)</summary><div class=scroll>'
                   '<table class=plain><thead><tr><th class=n>année d’âge</th><th class=n>matins</th><th class=n>ensemble</th>'
                   '<th class=n>foyers loin</th><th class=n>dort dehors</th><th class=n>but lointain</th><th class=n>essai de foyer</th>'
                   f'<th class=n>surpris par la nuit</th></tr></thead><tbody>{rows}</tbody></table></div></details>')
    else:
        out.append('<p class=muted>Les matins ensemble ou séparés ne sont pas mesurés dans ce run (il est plus ancien que cette mesure).</p>')
    bc = (d.get("bilan") or {}).get("couples") if isinstance(d.get("bilan"), dict) else None
    if isinstance(bc, dict) and sum(v or 0 for v in bc.values()):
        out.append(f"<p>Les journées où une femme pouvait tomber enceinte : <b>{fmt(bc.get('met') or 0)}</b> fois son mari était près d’elle et elle assez nourrie, "
                   f"<b>{fmt(bc.get('apart') or 0)}</b> fois il était trop loin, <b>{fmt(bc.get('thin') or 0)}</b> fois elle était trop maigre, "
                   f"<b>{fmt(bc.get('apart_and_thin') or 0)}</b> fois les deux.</p>")
    fams = panels.families(d)
    if fams:
        out.append(panels.families_fold(d, fams))
    return "".join(out)


def moves_html(d):
    """The homes changed: one line per departure (a day, a tribe, a place), who left and how far."""
    hc = d.get("home_changes")
    if not isinstance(hc, list):
        return '<p class=muted>Les changements de foyer ne sont pas mesurés dans ce run (il est plus ancien que cette mesure).</p>'
    if not hc:
        return "<p>Personne n’a changé de foyer.</p>"
    ts = reading.tribes(d)
    groups = {}
    for c in hc:
        t = c.get("tribe")
        letter = ts[t] if isinstance(t, int) and 0 <= t < len(ts) else t
        key = (c.get("day"), letter, tuple(c.get("to") or ()))
        groups.setdefault(key, []).append(c)
    alone = sum(1 for c in hc if not c.get("with"))
    out = [f"<p><b>{fmt(len(hc))}</b> changement{'s' if len(hc) > 1 else ''} de foyer, en <b>{fmt(len(groups))}</b> départ{'s' if len(groups) > 1 else ''} ; "
           f"{fmt(alone)} seul{'s' if alone > 1 else ''}, {fmt(len(hc) - alone)} avec d’autres.</p>"]
    lines = []
    for (day, t, _to), cs in sorted(groups.items(), key=lambda kv: (kv[0][0] or 0, str(kv[0][1]))):
        c = cs[0]
        a, b = c.get("from"), c.get("to")
        far = ""
        if a and b:
            dx, dy = b[0] - a[0], b[1] - a[1]
            dist = round(math.hypot(dx, dy))
            sector = int(((math.degrees(math.atan2(dy, dx)) + 360 + 22.5) % 360) // 45)
            far = f"à {fmt(dist)} m vers {reading.COMPASS[sector]}"
        who = ", ".join(reading.person_link(x.get("body")) for x in cs)
        lines.append(f'<li><span class=fd>{h(reading.day_fr(d, day))}</span> {reading.tribe_chip(t)} {who} '
                     f'{"part" if len(cs) == 1 else "partent"} {h(far)}</li>')
    shown, rest = lines[:15], lines[15:]
    out.append(f'<ul class=facts>{"".join(shown)}</ul>')
    if rest:
        out.append(f'<details class=fold><summary>Les {len(rest)} autres départs</summary><ul class=facts>{"".join(rest)}</ul></details>')
    return "".join(out)


PARTS = [("chaine", "La chaîne du manque"), ("carte", "La carte : où ils vont"), ("population", "La population au fil du temps"),
         ("frise", "Naissances et morts sur une frise"), ("ages", "Les âges"), ("couples", "Les couples"),
         ("demenagements", "Les déménagements"), ("chronique", "La chronique")]
LEADS = {
    "chaine": scarcity.LEAD,
    "carte": "Où chaque tribu a vécu, cycle par cycle : le curseur choisit le cycle, et chaque cadre ce qu’il montre "
             "(les endroits où l’on se tenait à midi, les trajets et les camps, la nourriture, l’eau, les morts et les naissances).",
    "population": "Combien de vivants compte chaque tribu, journée après journée ; les bandes pâles sont les hivers.",
    "frise": "Chaque naissance au-dessus de la ligne et chaque mort en dessous, à la journée où elle arrive, sur le même temps que la courbe.",
    "ages": "Combien d’hommes et de femmes de chaque âge vivent à la première journée et au dernier, par tranches de dix années d’âge.",
    "couples": "Combien de couples se forment ou se séparent, et si mari et femme se réveillent l’un près de l’autre : "
               "sans cela, pas d’enfant.",
    "demenagements": "Chaque fois qu’un adulte change de foyer : quand, qui, à quelle distance et dans quelle direction, seul ou avec d’autres.",
    "chronique": "Ce qui s’est passé, dans l’ordre : d’abord les faits qui comptent, puis, à déplier, chaque fait un par un.",
}


def part(key, inner):
    title = dict(PARTS)[key]
    return f'<section id={key} class=group><h2>{h(title)}</h2><p class=lead>{h(LEADS[key])}</p>{inner}</section>'


def rest_html(d):
    """What does not fit the seven parts, folded at the bottom: the settings, the other curves, the
    years, what they learnt, the tables."""
    down = lambda x: x.replace("<h3>", "<h4>").replace("</h3>", "</h4>").replace("<h2>", "<h3>").replace("</h2>", "</h3>")  # noqa: E731
    fire = reading.has_fire_data(d)
    inner = ['<p class=lead>Le reste des données du run, pour qui veut creuser : les réglages, les autres courbes, le cycle par cycle, '
             'ce qu’ils ont appris, les tableaux de chacun.</p>',
             "<h3>Les réglages</h3>" + panels.factors_html(d),
             "<h3>Les autres courbes</h3>" + panels.curves_html(),
             down(panels.years_section(d)),
             "<h3>Les morts, un par un</h3>" + reading.deaths_table_v2(d),
             "<h3>Les naissances, une par une</h3>" + reading.births_table_v2(d),
             "<h3>Tout le monde</h3>" + reading.people_table_v2(d),
             "<h3>Les techniques</h3>" + reading.techniques_html(d)]
    if (d.get("hands") or {}).get("tribes"):
        inner.append("<h3>Les mains</h3>" + reading.hands_html(d))
    if fire:
        inner.append("<h3>Ce que la tribu sait</h3>" + reading.knowledge_body(d))
        inner.append("<h3>Qui a osé quoi</h3>" + reading.dared_body(d))
        inner.append("<h3>Qui sait où est la nourriture</h3>" + reading.reputed_body(d))
    inner.append('<details class=detail><summary>Les tableaux bruts</summary><div data-lazy="annexes"><p class=muted>les annexes se chargent…</p></div></details>')
    return ('<section id=reste class=group><details class=rest><summary>Pour aller plus loin : réglages, autres courbes, savoir, tableaux</summary>'
            + "".join(inner) + "</details></section>")


def run_body(s, r, d, store=None, year=None):
    """The page of one run: the breadcrumb, one summary line, then the seven parts, always in the
    same order, and the rest folded at the bottom."""
    serie, name = s["name"], r["name"]
    names = run_names(s, store) if store is not None else {}
    label = names.get(name) or r.get("note") or name
    back = reading.serie_url(serie)
    head = [crumbs([("Labo", "/"), (s.get("title") or serie, back), (label, None)]),
            f'<p class=back><a class=button href="{back}">← retour à la série</a></p>',
            f'<h1>{h(label)}' + (f' <span class=etat>{h(state_words(r))}</span>' if state_words(r) else "") + "</h1>"]
    ref = parse_ref(r.get("ref"), serie)
    if ref:
        ref_name = names.get(ref[1]) if ref[0] == serie else None
        if ref_name is None and store is not None:
            ref_name = ref_label(store, *ref)[0]
        head.append(f'<p class=muted>Sa référence : <a href="{reading.run_url(*ref)}">{h(ref_name or "/".join(ref))}</a></p>')
    if d is None:
        live = f" Le run est en cours : {fmt(r.get('alive'))} vivants, {fmt(r.get('dead'))} morts d’après le log." if r.get("state") == "en cours" else ""
        return "".join(head) + f'<p class="err">{reading.NO_DATA}.{h(live)}</p>'
    head.append(summary_line(d))
    chron = reading.chronicle_html(d, year, f"/chronique/{urllib.parse.quote(serie)}/{urllib.parse.quote(name)}")
    if chron:
        chron = chron.replace("<h2>La chronique</h2>", f"<h2>La chronique</h2><p class=lead>{h(LEADS['chronique'])}</p>" + panels.chronicle_lead(d)
                              + "<details class=chronall><summary>Chaque fait, un par un, cycle par cycle (filtres, recherche)</summary>", 1)
        chron = chron[: chron.rindex("</section>")] + "</details></section>"
    else:
        chron = part("chronique", '<p class=muted>Pas de chronique : le run n’a pas de cycle.</p>')
    body = [part("chaine", scarcity.part_html(d)),
            part("carte", panels.maps_html()),
            part("population", reading.population_chart(d, wide=True).svg("vivants par tribu, journée après journée")),
            part("frise", frise(d)),
            part("ages", panels.pyramids_html(d)),
            part("couples", couples_html(d)),
            part("demenagements", moves_html(d)),
            chron, rest_html(d),
            '<dialog id=fiche class=fiche aria-label="fiche"><div class=in><button type=button class=close>fermer</button><div class=body></div></div></dialog>',
            f"<script>{reading.TABLE_JS}{reading.CHRON_JS}{reading.MAP2_JS}{reading.LAZY_JS}{reading.FICHE_JS}{reading.TOC_JS}"
            f"{panels.MAPS6_JS}{panels.SM6_JS}</script>"]
    toc = [(k, v.split(" : ")[0] if k == "carte" else v, False) for k, v in PARTS] + [("reste", "Pour aller plus loin", False)]
    base = f"{urllib.parse.quote(serie)}/{urllib.parse.quote(name)}"
    return ("".join(head) + f'<div class=runpage data-fiche="/fiche/{h(base)}" data-morceau="/morceau/{h(base)}">{reading.toc_html(toc)}'
            f'<div class=runmain>{"".join(body)}</div></div>')


PLAIN_CSS = """
.crumbs{font-size:.92em;color:var(--muted);margin:.2em 0 .3em}.crumbs a{color:var(--ink2);text-decoration:none}.crumbs a:hover{text-decoration:underline}
.crumbs span{color:var(--ink)}
p.back{margin:.2em 0 .6em}a.button{display:inline-block;padding:.25em .8em;border:1px solid var(--edge);border-radius:8px;color:var(--ink);
 text-decoration:none;background:var(--surface);font-size:.92em}a.button:hover{background:var(--zebra)}
.question{font-size:1.15em;line-height:1.5;max-width:62ch;margin:.3em 0 .4em}
.answer{font-size:1.05em;line-height:1.5;max-width:62ch;margin:0 0 1.4em;padding-left:.8em;border-left:3px solid var(--ink2)}
.answer.none{color:var(--muted);font-style:italic;border-left-color:var(--line)}
.tablebox{overflow-x:auto}
table.plain{width:100%;box-shadow:none;border:1px solid var(--line);border-radius:8px;font-size:.93em}
table.plain th[scope=col]{vertical-align:bottom;cursor:help}table.plain thead th:first-child{cursor:default}
table.plain tbody th{font-weight:600;text-align:left;padding:.5em .9em;color:var(--ink);border-bottom:0;min-width:14em;font-size:1em}
table.plain tbody tr:nth-child(even) th{background:var(--zebra)}
table.plain tbody th a{color:var(--ink);text-decoration:none}table.plain tbody tr[data-href]{cursor:pointer}
table.plain tbody tr[data-href]:hover th a{text-decoration:underline}table.plain tbody tr:hover td,table.plain tbody tr:hover th{background:var(--chip)}
table.plain .sub{display:block;font-weight:400;font-size:.82em;color:var(--muted)}
table.plain .refmk{display:inline-block;margin-left:.5em;font-size:.75em;font-weight:600;color:var(--ink2);border:1px solid var(--edge);
 border-radius:1em;padding:0 .5em;vertical-align:middle}
table.plain tbody tr.refrow+tr:not(.refrow) td,table.plain tbody tr.refrow+tr:not(.refrow) th{border-top:2px solid var(--edge)}
table.plain td.good,table.plain span.good{background:var(--good-bg);color:var(--good-ink)}table.plain td.bad,table.plain span.bad{background:var(--bad-bg);color:var(--bad-ink)}
table.plain span.good,table.plain span.bad{padding:0 .25em;border-radius:3px}table.plain .hg{color:var(--muted)}
.caption .good{background:var(--good-bg);color:var(--good-ink);padding:0 .3em;border-radius:3px}.caption .bad{background:var(--bad-bg);color:var(--bad-ink);padding:0 .3em;border-radius:3px}
ul.colhelp{font-size:.88em;color:var(--ink2);padding-left:1.1em;margin:.4em 0 1.2em;line-height:1.5;columns:2 24em;column-gap:2.5em}ul.colhelp li{break-inside:avoid;margin:0 0 .3em}
.foot{font-size:.8em;color:var(--muted);margin-top:2.5em;border-top:1px solid var(--line);padding-top:.6em}
.sumline{font-size:1.05em;line-height:1.6;margin:.4em 0 1.2em;max-width:60em}
p.lead{color:var(--ink2);margin:0 0 .8em;max-width:62ch}
i.sw{display:inline-block;width:.8em;height:.8em;border-radius:2px;margin:0 .3em 0 .1em;vertical-align:-.05em}
ul.plainlist{margin:.2em 0 .8em;padding-left:1.2em}
details.rest>summary{font-size:1.15em;font-weight:650;margin:.4em 0}
""" + scarcity.CSS
