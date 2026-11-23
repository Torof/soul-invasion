"""Read a run, read a series, compare: the "Lire / Comparer" screen (D52, PR 2).

Everything here reads the vivarium's data file (`NAME.json`, laid out in
`docs/10-vivarium-data.md`) and never its page. The pages are built
server-side, in French, as plain HTML with inline SVG curves and the map's
own picture; the only script is the one that picks the year of the
presence heatmap. Standard library only, nothing outward.

The kinds of steps and the causes of death are read from the data, never
from a fixed list: a kind the lab does not know is shown by its key, so a
future way (a tent, keeping water, fire) appears by itself.
"""

import html
import json
import math
import os
import statistics
import urllib.parse

# French words for what the data names by a key; an unknown key is shown as is.
KIND_FR = {
    "wider": "chercher plus loin", "narrower": "chercher moins loin",
    "sleep_out": "dormir dehors", "sleep_home": "dormir au camp",
    "store_more": "stocker davantage", "store_less": "stocker moins",
    "home_moved": "déplacer la maison",
}
CAUSE_FR = {"illness": "maladie", "old_age": "vieillesse", "childbirth": "couches",
            "hunger": "faim", "thirst": "soif", "cold": "froid"}
CAUSE_ORDER = ["hunger", "thirst", "cold", "illness", "childbirth", "old_age"]
CAUSE_COLOR = {"hunger": "#c0392b", "thirst": "#2980b9", "cold": "#7f8c8d", "illness": "#8e44ad",
               "childbirth": "#d35400", "old_age": "#95a5a6"}
SEASON_FR = {"winter": "hiver", "spring": "printemps", "summer": "été", "autumn": "automne"}
SEX_FR = {"woman": "femme", "man": "homme"}
HOW_FR = {"trial": "essai", "copy": "copie", "reverted": "abandon"}
TRIBE_COLOR = {"A": "#1f6f8b", "B": "#c2571a"}
OTHER_COLORS = ["#2e8b57", "#8e44ad", "#b7950b", "#444"]
SIDE_COLOR = {"a": "#1f6f8b", "b": "#c2571a"}       # the two sides of a comparison
TRIALS_COLOR = {True: "#c2571a", False: "#1f6f8b"}  # orange with trials, blue without
BIRTH_COLOR = "#3a8f4a"
ALIVE_COLOR, DEAD_COLOR, KEPT_COLOR = "#2b7a3d", "#b3261e", "#c2571a"
# Deaths by want are the brain's figure (22 November 2026); the rest is the background curve (D49):
# illness, old age, childbirth, and any cause the lab does not know yet.
WANT_CAUSES = ("hunger", "thirst", "cold")
BACKGROUND_CAUSES = ("illness", "old_age", "childbirth")
WANT_COLOR, BACKGROUND_COLOR = "#b3261e", "#8a8f98"
NO_STOCK = "pas dans les données"
KJ_PER_PERSON_DAY = 10000      # what a person eats in a day (docs/10)
NO_DATA = "pas de données, relancer avec le vivarium récent"
NO_POSITIONS = "pas de positions dans ce run"
SEASON_ALL = ["spring", "summer", "autumn", "winter"]   # the year cut in four, spring first (time.rs)
SEASON_IN = {"winter": "en hiver", "spring": "au printemps", "summer": "en été", "autumn": "en automne"}
# What a technique changes, in one clause, for the techniques' table and the verdict.
KIND_CHANGE = {
    "wider": "on cherche la nourriture deux fois plus loin du camp",
    "narrower": "on cherche la nourriture deux fois moins loin, près du camp",
    "sleep_out": "on dort là où la nuit tombe au lieu de rentrer au camp",
    "sleep_home": "on rentre dormir au camp",
    "store_more": "on garde plus de nourriture en réserve avant de se reposer",
    "store_less": "on garde moins de nourriture en réserve avant de se reposer",
    "home_moved": "on installe la maison ailleurs",
}
# The verdict's phrase for the main cause of death: (plural subject, singular subject).
CAUSE_VERDICT = {
    "hunger": ("ont manqué de nourriture", "a manqué de nourriture"),
    "thirst": ("ont manqué d’eau", "a manqué d’eau"),
    "cold": ("sont morts de froid", "est mort de froid"),
    "illness": ("ont été emportés par la maladie", "a été emporté par la maladie"),
    "childbirth": ("sont mortes en couches", "est morte en couches"),
    "old_age": ("sont morts de vieillesse", "est mort de vieillesse"),
}
COMPASS = ["l’est", "le sud-est", "le sud", "le sud-ouest", "l’ouest", "le nord-ouest", "le nord", "le nord-est"]

h = html.escape


def fmt(n):
    """A number for a French eye: `12 345`, `-` for None, one decimal for floats."""
    if n is None:
        return "-"
    if isinstance(n, float):
        return f"{n:,.1f}".replace(",", " ").replace(".", ",")
    return f"{n:,}".replace(",", " ")


def secs(s):
    s = int(s or 0)
    if s >= 3600:
        return f"{s // 3600} h {s % 3600 // 60} min"
    if s >= 60:
        return f"{s // 60} min {s % 60} s"
    return f"{s} s"


def kind_fr(key):
    return KIND_FR.get(key, key)


def cause_fr(key):
    return CAUSE_FR.get(key, key)


def person_fr(name_or_id):
    """`person 14` → `personne 14`; an id alone → `personne N`."""
    if isinstance(name_or_id, int):
        return f"personne {name_or_id}"
    return str(name_or_id).replace("person ", "personne ")


def plural(n, word, suffix="s"):
    """`3 morts`, `1 mort`, `0 mort`."""
    return f"{fmt(n)} {word}{suffix if (n or 0) > 1 else ''}"


def season_of(d, day):
    """The season of a day of the run: the year is cut in four, spring first (time.rs)."""
    yd = year_days(d)
    doy = (int(day) - 1) % yd
    return SEASON_ALL[min(3, doy * 4 // yd)]


def day_fr(d, day):
    """`an 3, printemps, jour 112` for a day of the run; `-` for None."""
    if day is None:
        return "-"
    yd = year_days(d)
    return f"an {(int(day) - 1) // yd + 1}, {SEASON_FR[season_of(d, day)]}, jour {fmt(int(day))}"


def age_days(d, p, at_day=None):
    """A person's age in days on `at_day` (the run's last day by default), or None when unknown."""
    if at_day is None:
        at_day = run_days(d)
    if p.get("born") == "start":
        start = p.get("age_days_at_start")
        if start is None:
            return None
        return start + at_day - 1
    born = p.get("born_day") or p.get("born")
    if not isinstance(born, int):
        return None
    return max(0, at_day - born)


def age_fr(d, days_old):
    """`32 ans`, `1 an`, `23 jours` (under a year), in game years of `days_per_year`."""
    if days_old is None:
        return "-"
    yd = year_days(d)
    if days_old < yd:
        return plural(days_old, "jour")
    return plural(days_old // yd, "an")


def run_days(d):
    """The run's last day: the settings' days, else the last year's last day."""
    days = ((d.get("meta") or {}).get("settings") or {}).get("days")
    if days:
        return int(days)
    return max((y.get("last_day") or 0 for y in d.get("years", [])), default=0)


def camp_at(d, tribe, day=None):
    """The tribe's camp at `day`: the start camp, then the camp at the end of each year passed."""
    ts = tribes(d)
    camps = (d.get("meta") or {}).get("camps") or []
    i = ts.index(tribe) if tribe in ts else 0
    camp = camps[i] if i < len(camps) else None
    for y in years_of(d, tribe):
        if day is not None and (y.get("last_day") or y.get("year", 0) * year_days(d)) > day:
            break
        if y.get("camp"):
            camp = y["camp"]
    return camp


def where_fr(d, tribe, at, day=None):
    """`à 140 m du camp, vers le nord-est`, or `au camp` within ten metres; `-` without a place."""
    if not at:
        return "-"
    camp = camp_at(d, tribe, day)
    if not camp:
        return f"en ({at[0]}, {at[1]})"
    dx, dy = at[0] - camp[0], at[1] - camp[1]
    dist = math.hypot(dx, dy)
    if dist < 10:
        return "au camp"
    sector = int(((math.degrees(math.atan2(dy, dx)) + 360 + 22.5) % 360) // 45)
    return f"à {fmt(int(round(dist)))} m du camp, vers {COMPASS[sector]}"


# --- the data of one run -------------------------------------------------------

class Store:
    """Reads the runs' JSON files under the runs directory, cached by mtime."""

    def __init__(self, runs_dir):
        self.runs_dir = runs_dir
        self.cache = {}

    def path(self, serie, run):
        return os.path.join(self.runs_dir, serie, run + ".json")

    def load(self, serie, run):
        """The run's data, or None when the file is missing or unreadable."""
        path = self.path(serie, run)
        try:
            mtime = os.path.getmtime(path)
        except OSError:
            self.cache.pop(path, None)
            return None
        hit = self.cache.get(path)
        if hit and hit[0] == mtime:
            return hit[1]
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError):
            return None
        if not isinstance(data, dict) or not isinstance(data.get("meta"), dict):
            return None
        if len(self.cache) > 64:
            self.cache.clear()
        self.cache[path] = (mtime, data)
        return data


def tribes(d):
    """The tribes' letters, in the order of the camps."""
    seen = []
    for y in d.get("years", []):
        if y.get("tribe") not in seen:
            seen.append(y["tribe"])
    for p in d.get("people", []):
        if p.get("tribe") not in seen:
            seen.append(p["tribe"])
    return seen


def tribe_color(t, i=0):
    return TRIBE_COLOR.get(t, OTHER_COLORS[i % len(OTHER_COLORS)])


def kinds(d):
    """Every kind of step the data names, in the data's order, known ones first."""
    out = []
    for f in d.get("found", []):
        for table in ("tried", "kept", "copied"):
            for k in (f.get(table) or {}):
                if k not in out:
                    out.append(k)
    for p in d.get("people", []):
        for t in p.get("trials") or []:
            if t.get("step") not in out:
                out.append(t["step"])
    known = [k for k in KIND_FR if k in out]
    return known + [k for k in out if k not in KIND_FR]


def causes(d):
    """Every cause of death the data names, in a fixed order, unknown ones last."""
    out = set()
    for y in d.get("years", []):
        out.update((y.get("deaths") or {}).keys())
    for p in d.get("people", []):
        if p.get("died"):
            out.add(p["died"].get("cause"))
    out.discard(None)
    return [c for c in CAUSE_ORDER if c in out] + sorted(c for c in out if c not in CAUSE_ORDER)


def years_of(d, tribe=None):
    ys = [y for y in d.get("years", []) if tribe is None or y.get("tribe") == tribe]
    return sorted(ys, key=lambda y: (y.get("year", 0), y.get("tribe", "")))


def last_year(d):
    ys = d.get("years", [])
    return max((y.get("year", 0) for y in ys), default=0)


def summary(d):
    """The cards of a run: alive, dead by cause, births, trials, kept, copied, homes moved."""
    people = d.get("people", [])
    deaths = {}
    for p in people:
        died = p.get("died")
        if died:
            deaths[died.get("cause", "?")] = deaths.get(died.get("cause", "?"), 0) + 1
    found = d.get("found", [])
    totals = {t: sum(sum((f.get(t) or {}).values()) for f in found) for t in ("tried", "kept", "copied")}
    cs = causes(d)
    return {
        "alive": sum(1 for p in people if not p.get("died")),
        "dead": sum(deaths.values()),
        "deaths": {c: deaths.get(c, 0) for c in cs},
        "want": sum(n for c, n in deaths.items() if c in WANT_CAUSES),
        "background": sum(n for c, n in deaths.items() if c not in WANT_CAUSES),
        "want_by": {c: deaths.get(c, 0) for c in WANT_CAUSES},
        "background_by": {c: deaths.get(c, 0) for c in BACKGROUND_CAUSES + tuple(c for c in cs if c not in WANT_CAUSES + BACKGROUND_CAUSES)},
        "stock": stock(d),
        "births": sum(1 for p in people if p.get("born") != "start"),
        "start": sum(1 for p in people if p.get("born") == "start"),
        "trials": totals["tried"], "kept": totals["kept"], "copied": totals["copied"],
        "homes_moved": sum(y.get("homes_moved") or 0 for y in d.get("years", [])),
        "techniques": sum(1 for n in carried(d).values() if n),
        "heard_trials": sum((f.get("heard") or {}).get("trials") or 0 for f in found),
        "heard_copies": sum((f.get("heard") or {}).get("copies") or 0 for f in found),
        "dispelled": sum((f.get("heard") or {}).get("dispelled") or 0 for f in found),
        "knowledge": knowledge_at_end(d),
        "alive_start": sum(1 for p in people if not p.get("died") and p.get("born") == "start"),
        "years": last_year(d),
        "days": (d.get("meta") or {}).get("settings", {}).get("days"),
    }


def is_want(cause):
    return cause in WANT_CAUSES


def stock(d):
    """The tribe's stored food: what the living carried at the end of the run's last year
    (`years[].food.end_kj`, summed over the tribes), with the year's start, the share of each tribe
    and the days of food it makes per living person. None when no year carries `food`."""
    ys = [y for y in years_of(d) if y.get("year") == last_year(d) and y.get("food")]
    if not ys:
        return None
    end = sum(y["food"].get("end_kj") or 0 for y in ys)
    alive = sum(y.get("population") or 0 for y in ys)
    return {"kj": end, "start_kj": sum(y["food"].get("start_kj") or 0 for y in ys),
            "by_tribe": {y["tribe"]: y["food"].get("end_kj") or 0 for y in ys},
            "days_per_person": end / alive / KJ_PER_PERSON_DAY if alive else None, "year": last_year(d)}


def last_kept(p):
    """The kind of a person's last kept trial: the technique it carries; None without."""
    kept = [t for t in p.get("trials") or [] if t.get("kept")]
    if not kept:
        return None
    return max(kept, key=lambda t: t.get("judged_day") or t.get("day") or 0).get("step")


def carried(d, tribe=None):
    """Per kind, how many living carry it at the end (their last kept trial), one tribe or all."""
    out = {}
    for p in d.get("people", []):
        if p.get("died") or (tribe is not None and p.get("tribe") != tribe):
            continue
        k = last_kept(p)
        if k is not None:
            out[k] = out.get(k, 0) + 1
    return out


def settings_fr(d):
    """The run's settings in one French line."""
    s = (d.get("meta") or {}).get("settings") or {}
    parts = [f"monde {s.get('world', '?')}" + (f" (côté {s['side']})" if s.get("side") else ""),
             f"{s.get('tribes', '?')} tribu{'s' if (s.get('tribes') or 0) > 1 else ''}",
             f"{s.get('people', '?')} adultes, {s.get('children', 0)} enfants, {s.get('infants', 0)} nourrissons par camp"]
    if s.get("adult_years"):
        parts.append(f"âges {s['adult_years'][0]}-{s['adult_years'][1]}")
    days = s.get("days") or 0
    dpy = (d.get("meta") or {}).get("days_per_year") or 50
    parts.append(f"{fmt(days)} jours ({days / dpy:g} an{'s' if days >= 2 * dpy else ''})")
    parts.append(f"graine {s.get('seed', '?')}")
    parts.append("avec épreuves" if s.get("trials", True) else "sans épreuves (témoin)")
    if "fire" in s:
        parts.append("avec feu" if s.get("fire") else "sans feu (témoin : l'imitation d'avant le feu)")
    if s.get("fed"):
        parts.append("nourris sans limite")
    if not s.get("brain", True):
        parts.append("sans cerveau")
    if (s.get("ticks_per_second") or 1) != 1:
        parts.append(f"cadence {s['ticks_per_second']}")
    if s.get("content"):
        parts.append(f"contenu « {s['content']} »")
    return ", ".join(parts)


# --- the curves: inline SVG, drawn here ---------------------------------------

class Scale:
    """A linear scale from a data span to a pixel span (the test checks it)."""

    def __init__(self, lo, hi, p0, p1):
        self.lo, self.hi, self.p0, self.p1 = lo, hi, p0, p1

    def __call__(self, v):
        if self.hi == self.lo:
            return self.p0
        return self.p0 + (v - self.lo) * (self.p1 - self.p0) / (self.hi - self.lo)


def nice_max(v):
    """A round top for an axis: 0 → 1, 7 → 8, 23 → 25, 170 → 200."""
    if v <= 0:
        return 1
    mag = 10 ** math.floor(math.log10(v))
    for k in (1, 2, 2.5, 5, 10):
        if v <= k * mag:
            return k * mag
    return 10 * mag


def y_ticks(top, n=4):
    step = top / n
    return [step * i for i in range(n + 1)]


class Chart:
    """A small SVG chart: axes in French, a legend, lines and bars."""

    W, H, L, R, T, B = 660, 230, 50, 14, 12, 34

    def __init__(self, xlo, xhi, ytop, xlabel, ylabel, year_days=50, pct=False, size=None, winters=False):
        if size:
            self.W, self.H = size
        self.xlabel, self.ylabel = xlabel, ylabel
        self.year_days = year_days
        self.x = Scale(xlo, xhi, self.L, self.W - self.R)
        self.ytop = nice_max(ytop) if not pct else 100
        self.y = Scale(0, self.ytop, self.H - self.B, self.T)
        self.parts = []
        self.legend = []
        self.xlo, self.xhi = xlo, xhi
        self.winters = winters

    def line(self, xs, ys, color, name=None, dash=False, width=1.6):
        pts = " ".join(f"{self.x(x):.1f},{self.y(y):.1f}" for x, y in zip(xs, ys) if y is not None)
        if pts:
            d = ' stroke-dasharray="6 4"' if dash else ""
            self.parts.append(f'<polyline fill="none" stroke="{color}" stroke-width="{width}"{d} points="{pts}"/>')
        if name:
            self.legend.append((color, name, dash))
        return self

    def hover(self, xs, ys, titles, color):
        """Invisible dots with a title each: the value on hover."""
        for x, y, t in zip(xs, ys, titles):
            if y is None:
                continue
            self.parts.append(f'<circle cx="{self.x(x):.1f}" cy="{self.y(y):.1f}" r="5" fill="{color}" '
                              f'fill-opacity="0.15"><title>{h(t)}</title></circle>')
        return self

    def bar(self, x0, x1, y0, y1, color, title=None, stroke=None):
        px0, px1 = self.x(x0), self.x(x1)
        py0, py1 = self.y(y0), self.y(y1)
        top, bottom = min(py0, py1), max(py0, py1)
        s = f' stroke="{stroke}" stroke-width="1"' if stroke else ""
        t = f"<title>{h(title)}</title>" if title else ""
        self.parts.append(f'<rect x="{px0 + 1:.1f}" y="{top:.1f}" width="{max(1, px1 - px0 - 2):.1f}" '
                          f'height="{max(0.5, bottom - top):.1f}" fill="{color}"{s}>{t}</rect>')
        return self

    def legend_item(self, color, name, dash=False):
        self.legend.append((color, name, dash))
        return self

    def svg(self, title=""):
        g = [f'<svg class="chart" viewBox="0 0 {self.W} {self.H}" role="img" aria-label="{h(title)}">']
        # winters shaded: the last quarter of every year
        if self.winters:
            yd = self.year_days
            yr = 0
            while yr * yd < self.xhi:
                w0, w1 = max(self.xlo, yr * yd + math.ceil(yd * 3 / 4) + 1), min(self.xhi, (yr + 1) * yd)
                if w1 > w0:
                    g.append(f'<rect class="winter" x="{self.x(w0):.1f}" y="{self.T}" width="{self.x(w1) - self.x(w0):.1f}" '
                             f'height="{self.H - self.B - self.T}" fill="#dde6ee"><title>hiver de l’an {yr + 1}</title></rect>')
                yr += 1
        # grid and y axis
        for v in y_ticks(self.ytop):
            py = self.y(v)
            g.append(f'<line x1="{self.L}" x2="{self.W - self.R}" y1="{py:.1f}" y2="{py:.1f}" stroke="#ddd"/>')
            g.append(f'<text x="{self.L - 6}" y="{py + 4:.1f}" text-anchor="end" font-size="11">{fmt(v if v != int(v) else int(v))}</text>')
        # x axis in years
        yd = self.year_days
        first = math.floor(self.xlo / yd) if self.xlo > 0 else 0
        n_years = max(1, math.ceil(self.xhi / yd))
        every = 1 if n_years <= 12 else 2 if n_years <= 30 else 5
        for yr in range(first, n_years + 1):
            day = yr * yd
            if day < self.xlo or day > self.xhi + yd:
                continue
            px = self.x(min(day, self.xhi))
            g.append(f'<line x1="{px:.1f}" x2="{px:.1f}" y1="{self.T}" y2="{self.H - self.B}" stroke="#eee"/>')
            if yr % every == 0:
                g.append(f'<text x="{px:.1f}" y="{self.H - self.B + 14}" text-anchor="middle" font-size="11">{yr}</text>')
        g.append(f'<text x="{(self.L + self.W - self.R) / 2:.0f}" y="{self.H - 4}" text-anchor="middle" font-size="11" fill="#555">{h(self.xlabel)}</text>')
        g.append(f'<text transform="translate(12,{(self.T + self.H - self.B) / 2:.0f}) rotate(-90)" text-anchor="middle" font-size="11" fill="#555">{h(self.ylabel)}</text>')
        g.append(f'<line x1="{self.L}" x2="{self.L}" y1="{self.T}" y2="{self.H - self.B}" stroke="#888"/>')
        g.append(f'<line x1="{self.L}" x2="{self.W - self.R}" y1="{self.H - self.B}" y2="{self.H - self.B}" stroke="#888"/>')
        g.extend(self.parts)
        # legend, top right
        lx = self.W - self.R - 6
        for i, (color, name, dash) in enumerate(self.legend[:8]):
            ly = self.T + 6 + i * 14
            d = ' stroke-dasharray="4 3"' if dash else ""
            g.append(f'<text x="{lx - 22}" y="{ly + 4}" text-anchor="end" font-size="11">{h(name)}</text>')
            g.append(f'<line x1="{lx - 18}" x2="{lx}" y1="{ly}" y2="{ly}" stroke="{color}" stroke-width="3"{d}/>')
        g.append("</svg>")
        return "".join(g)


def year_days(d):
    return (d.get("meta") or {}).get("days_per_year") or 50


def year_marks(d, tribe_index, curve):
    """(days, values, titles) at every year's end along a sampled curve, for the hover dots."""
    c = d.get("curves") or {}
    days = c.get("days") or []
    vals = c.get(curve) or []
    if tribe_index is not None:
        vals = vals[tribe_index] if tribe_index < len(vals) else []
    if not days or not vals:
        return [], [], []
    yd = year_days(d)
    xs, ys, ts = [], [], []
    last = -1
    for day, v in zip(days, vals):
        yr = day // yd if day % yd == 0 else None
        if yr and yr != last:
            xs.append(day)
            ys.append(v)
            ts.append(f"an {yr}, jour {day} : {fmt(v)}")
            last = yr
    return xs, ys, ts


def population_chart(d, label=None, color_of=None, dash=False, chart=None, hover=True, wide=False):
    """Alive per tribe over the days; `color_of(tribe, i)` picks the colour. `wide`: the run
    page's form, the whole width, years marked and winters shaded."""
    c = d.get("curves") or {}
    days = c.get("days") or []
    by_tribe = c.get("alive_by_tribe") or []
    ts = tribes(d)
    top = max((max(v) for v in by_tribe if v), default=1)
    if chart is None:
        chart = Chart(0, max(days[-1] if days else 1, year_days(d)), top, "années", "vivants", year_days(d),
                      size=(1000, 260) if wide else None, winters=wide)
        if wide:
            chart.legend_item("#dde6ee", "hivers")
    else:
        chart.ytop = nice_max(max(chart.ytop, top))
        chart.y = Scale(0, chart.ytop, chart.H - chart.B, chart.T)
    for i, vals in enumerate(by_tribe):
        t = ts[i] if i < len(ts) else f"tribu {i + 1}"
        color = color_of(t, i) if color_of else tribe_color(t, i)
        name = f"{label + ' · ' if label else ''}tribu {t}"
        chart.line(days[:len(vals)], vals, color, name, dash)
        if hover:
            xs, ys, titles = year_marks(d, i, "alive_by_tribe")
            chart.hover(xs, ys, [f"tribu {t}, {x}" for x in titles], color)
    return chart


def reserve_chart(d, label=None, color_of=None, dash=False, chart=None):
    c = d.get("curves") or {}
    days = c.get("days") or []
    by_tribe = c.get("reserve_by_tribe") or []
    ts = tribes(d)
    if chart is None:
        chart = Chart(0, max(days[-1] if days else 1, year_days(d)), 100, "années", "réserve de graisse, % du plein",
                      year_days(d), pct=True)
    for i, vals in enumerate(by_tribe):
        t = ts[i] if i < len(ts) else f"tribu {i + 1}"
        color = color_of(t, i) if color_of else tribe_color(t, i)
        chart.line(days[:len(vals)], vals, color, f"{label + ' · ' if label else ''}tribu {t}", dash)
        xs, ys, titles = year_marks(d, i, "reserve_by_tribe")
        chart.hover(xs, ys, [f"tribu {t}, {x} %" for x in titles], color)
    return chart


def births_deaths_chart(d, label=None, chart=None, slot=(0, 1), tag=""):
    """Per year, all tribes: a births bar and a deaths bar stacked by cause.

    `slot` = (index, count) places this run's bars within the year when two
    runs share one chart.
    """
    yd = year_days(d)
    cs = causes(d)
    per_year = {}
    for y in years_of(d):
        e = per_year.setdefault(y["year"], {"births": 0, "deaths": {}})
        e["births"] += y.get("births") or 0
        for c, n in (y.get("deaths") or {}).items():
            e["deaths"][c] = e["deaths"].get(c, 0) + n
    n_years = max(per_year, default=1)
    top = max([e["births"] for e in per_year.values()] + [sum(e["deaths"].values()) for e in per_year.values()] + [1])
    if chart is None:
        chart = Chart(0, n_years * yd, top, "années", "naissances et morts dans l'année", yd)
        chart.legend_item(BIRTH_COLOR, "naissances")
        for c in cs:
            chart.legend_item(CAUSE_COLOR.get(c, "#333"), f"morts : {cause_fr(c)}")
    else:
        chart.ytop = nice_max(max(chart.ytop, top))
        chart.y = Scale(0, chart.ytop, chart.H - chart.B, chart.T)
        chart.xhi = max(chart.xhi, n_years * yd)
        chart.x = Scale(0, chart.xhi, chart.L, chart.W - chart.R)
    k, n = slot
    for yr, e in sorted(per_year.items()):
        x0 = (yr - 1) * yd + k * yd / n
        w = yd / n
        pre = f"{label} · " if label else ""
        chart.bar(x0 + w * 0.08, x0 + w * 0.48, 0, e["births"], BIRTH_COLOR,
                  f"{pre}an {yr} : {e['births']} naissance{'s' if e['births'] > 1 else ''}")
        base = 0
        for c in cs:
            v = e["deaths"].get(c, 0)
            if v:
                chart.bar(x0 + w * 0.52, x0 + w * 0.92, base, base + v, CAUSE_COLOR.get(c, "#333"),
                          f"{pre}an {yr} : {v} mort{'s' if v > 1 else ''} de {cause_fr(c)}", stroke="#fff")
                base += v
    return chart


def want_chart(d, wide=False):
    """Per year and per tribe, a small bar stacked in two: the deaths by want (red) under the
    background deaths (grey); the bar's outline is the tribe's colour. `wide`: the run page's form,
    the same width and x scale as the population curve above it."""
    yd = year_days(d)
    ts = tribes(d)
    per = {}
    for y in years_of(d):
        dd = y.get("deaths") or {}
        want = sum(n for c, n in dd.items() if is_want(c))
        per[(y.get("year", 0), y.get("tribe"))] = (want, sum(dd.values()) - want, dd)
    n_years = max((yr for yr, _ in per), default=1)
    days = (d.get("curves") or {}).get("days") or []
    top = max([w + b for w, b, _ in per.values()] + [1])
    chart = Chart(0, max(days[-1] if days else 1, yd), top, "années", "morts dans l’année", yd,
                  size=(1000, 150) if wide else None, winters=wide)     # the population curve's x scale
    n = max(1, len(ts))
    for (yr, t), (want, background, dd) in sorted(per.items(), key=lambda kv: (kv[0][0], str(kv[0][1]))):
        i = ts.index(t) if t in ts else 0
        x0 = (yr - 1) * yd + i * yd / n
        w = yd / n
        words = ", ".join(f"{cause_fr(c)} {fmt(v)}" for c, v in dd.items() if v)
        title = f"tribu {t}, an {yr} : {plural(want, 'mort')} de manque, {fmt(background)} de fond" + (f" ({words})" if words else "")
        stroke = tribe_color(t, i)
        x1 = min(x0 + w * 0.88, chart.xhi)       # a last year cut short by the run's end
        if want:
            chart.bar(x0 + w * 0.12, x1, 0, want, WANT_COLOR, title, stroke=stroke)
        if background:
            chart.bar(x0 + w * 0.12, x1, want, want + background, BACKGROUND_COLOR, title, stroke=stroke)
    return chart


def kept_chart(d, label=None, color_of=None, chart=None, slot=(0, 1)):
    """Trials kept per year per tribe, as bars; the title tells trials, kept, copied."""
    yd = year_days(d)
    ts = tribes(d)
    rows = years_of(d)
    n_years = max((y["year"] for y in rows), default=1)
    top = max([y.get("kept") or 0 for y in rows] + [1])
    if chart is None:
        chart = Chart(0, n_years * yd, top, "années", "essais gardés dans l'année", yd)
    else:
        chart.ytop = nice_max(max(chart.ytop, top))
        chart.y = Scale(0, chart.ytop, chart.H - chart.B, chart.T)
    k, n = slot
    nt = max(1, len(ts))
    for y in rows:
        i = ts.index(y["tribe"]) if y["tribe"] in ts else 0
        color = color_of(y["tribe"], i) if color_of else tribe_color(y["tribe"], i)
        w = yd / n / nt
        x0 = (y["year"] - 1) * yd + k * yd / n + i * w
        pre = f"{label} · " if label else ""
        chart.bar(x0 + w * 0.1, x0 + w * 0.9, 0, y.get("kept") or 0, color,
                  f"{pre}tribu {y['tribe']}, an {y['year']} : {y.get('trials') or 0} essais, "
                  f"{y.get('kept') or 0} gardés, {y.get('copied') or 0} copiés")
    for i, t in enumerate(ts):
        chart.legend_item(color_of(t, i) if color_of else tribe_color(t, i), f"{label + ' · ' if label else ''}tribu {t}")
    return chart


# --- the map ------------------------------------------------------------------

MAP_JS = """
document.querySelectorAll('[data-map]').forEach(m => {
  const data = JSON.parse(m.querySelector('script[type="application/json"]').textContent);
  const days = data.days, dpy = data.dpy, seasons = ['printemps', 'été', 'automne', 'hiver'];
  const slider = m.querySelector('input[type=range]'), when = m.querySelector('.when'), play = m.querySelector('[data-play]');
  const heat = m.querySelector('[data-heat]'), groups = m.querySelectorAll('g[data-year]');
  const deaths = m.querySelectorAll('[data-death]'), camps = m.querySelectorAll('[data-camp-year]');
  const dots = m.querySelector('g.dots'), r = data.r, pos = data.positions || [];
  const colour = id => data.colors[data.tribe[id]] || '#444';
  const sampleAt = day => { let k = -1; for (let i = 0; i < pos.length && pos[i][0] <= day; i++) k = i; return k; };
  function show(day) {
    const year = Math.floor((day - 1) / dpy) + 1, doy = (day - 1) % dpy;
    when.textContent = 'an ' + year + ', ' + seasons[Math.min(3, Math.floor(doy * 4 / dpy))] + ', jour ' + day;
    deaths.forEach(g => g.style.display = +g.dataset.death <= day ? 'inline' : 'none');
    const passed = day >= days ? data.lastYear : year - 1;
    camps.forEach(c => c.style.display = +c.dataset.campYear <= passed ? 'inline' : 'none');
    const shown = Math.min(year, data.lastYear);
    groups.forEach(g => g.style.display = heat.checked && +g.dataset.year === shown ? 'inline' : 'none');
    if (!pos.length) return;
    const k = sampleAt(day);
    let out = '';
    if (k >= 0) {
      const trail = {};
      for (let i = Math.max(0, k - 4); i < k; i++) for (const [id, x, y] of pos[i][1]) (trail[id] = trail[id] || []).push(x + ',' + y);
      for (const [id, x, y] of pos[k][1]) {
        const c = colour(id);
        if (trail[id]) out += `<polyline fill="none" stroke="${c}" stroke-opacity=".45" stroke-width="${r / 3}" points="${trail[id].join(' ')} ${x},${y}"/>`;
        out += `<circle cx="${x}" cy="${y}" r="${r}" fill="${c}" stroke="#fff" stroke-width="${r / 4}"><title>${data.names[id] || id}, jour ${pos[k][0]}</title></circle>`;
      }
    }
    dots.innerHTML = out;
  }
  slider.oninput = () => show(+slider.value);
  heat.onchange = () => show(+slider.value);
  let timer = null;
  const stop = () => { clearInterval(timer); timer = null; play.textContent = '▶ Lire'; };
  play.onclick = () => {
    if (timer) return stop();
    if (+slider.value >= days) slider.value = 1;
    play.textContent = '❚❚ Pause';
    timer = setInterval(() => { if (+slider.value >= days) return stop(); slider.value = +slider.value + 1; show(+slider.value); }, 100);
  };
  show(+slider.value);
});
"""


def positions_of(d):
    """The daily positions `[[day, [[id, x, y], …]], …]`, sorted by day; `[]` when the key is missing."""
    pos = d.get("positions")
    if not isinstance(pos, list):
        return []
    return sorted((p for p in pos if isinstance(p, list) and len(p) == 2 and isinstance(p[1], list)), key=lambda p: p[0])


def map_svg(d, uid, side="", show_deaths=True):
    """The map of movements over time: the vivarium's picture with a day slider (play/pause),
    the living as dots coloured by tribe (from `positions`, when the run has them) with their
    trail of the last days, the dead as a small cross where they fell from the day they die,
    the camp's path year by year, and, as an option, the year's presence heatmap."""
    m = d.get("map") or {}
    uri = m.get("data_uri")
    tiles = m.get("tiles") or (d.get("meta") or {}).get("map_tiles")
    if not uri or not tiles or not str(uri).startswith("data:image/png;base64,"):
        return f'<p class="muted">{NO_DATA} (pas de carte dans le fichier)</p>'
    W, Hh = tiles
    fs = max(8, W / 60)
    ts = tribes(d)
    days = run_days(d) or 1
    pos = positions_of(d)
    parts = [f'<svg viewBox="0 0 {W} {Hh}" class="map" role="img" aria-label="carte du run{" " + h(side) if side else ""}">',
             f'<image href="{h(uri)}" x="0" y="0" width="{W}" height="{Hh}" style="image-rendering:pixelated"/>']
    # presence heatmap, one group a year, the last one shown without script
    presence = d.get("presence") or []
    years = sorted({p.get("year") for p in presence if p.get("year") is not None})
    peak = max((c[2] for p in presence for c in (p.get("cells") or [])), default=1)
    for yr in years:
        hidden = "" if yr == years[-1] else ' display="none"'
        parts.append(f'<g data-year="{yr}"{hidden}>')
        for p in presence:
            if p.get("year") != yr:
                continue
            color = tribe_color(p.get("tribe"), ts.index(p["tribe"]) if p.get("tribe") in ts else 0)
            cell = p.get("cell_tiles") or 32
            for cx, cy, n in p.get("cells") or []:
                o = 0.12 + 0.75 * min(1, n / peak)
                parts.append(f'<rect x="{cx * cell}" y="{cy * cell}" width="{cell}" height="{cell}" fill="{color}" '
                             f'fill-opacity="{o:.2f}"><title>tribu {h(p["tribe"])}, an {yr} : {n} présence(s) à midi</title></rect>')
        parts.append("</g>")
    # the camps' path, one a tribe: the whole path light, a marker at every year's end
    for i, t in enumerate(ts):
        color = tribe_color(t, i)
        pts = [(y["year"], y["camp"]) for y in years_of(d, t) if y.get("camp")]
        camps = (d.get("meta") or {}).get("camps") or []
        if i < len(camps):
            pts.insert(0, (0, camps[i]))
        if len(pts) > 1:
            parts.append(f'<polyline fill="none" stroke="{color}" stroke-width="{fs / 5:.2f}" stroke-opacity="0.7" stroke-dasharray="{fs / 3:.1f} {fs / 4:.1f}" '
                         f'points="{" ".join(f"{p[0]},{p[1]}" for _, p in pts)}"/>')
        for yr, (x, y) in pts:
            parts.append(f'<g data-camp-year="{yr}"><rect x="{x - fs / 3:.1f}" y="{y - fs / 3:.1f}" width="{fs / 1.5:.1f}" height="{fs / 1.5:.1f}" '
                         f'transform="rotate(45 {x} {y})" fill="{color}" stroke="#fff" stroke-width="{fs / 10:.2f}">'
                         f'<title>tribu {h(t)}, {"départ" if yr == 0 else f"fin de l’an {yr}"} : camp en ({x}, {y})</title></rect>'
                         f'<text x="{x + fs * 0.45:.1f}" y="{y - fs * 0.3:.1f}" font-size="{fs:.1f}" fill="{color}" '
                         f'stroke="#fff" stroke-width="{fs / 6:.2f}" paint-order="stroke" font-weight="700">{yr}</text></g>')
    # the deaths' places, each shown from its day
    if show_deaths:
        for p in d.get("people", []):
            died = p.get("died")
            if died and died.get("at"):
                x, y = died["at"]
                r = fs * 0.45
                parts.append(f'<g data-death="{died.get("day") or 0}" stroke="#111" stroke-width="{fs / 7:.2f}" stroke-linecap="round">'
                             f'<line x1="{x - r}" y1="{y - r}" x2="{x + r}" y2="{y + r}"/>'
                             f'<line x1="{x - r}" y1="{y + r}" x2="{x + r}" y2="{y - r}"/>'
                             f'<title>{h(person_fr(p.get("name", p.get("id"))))} ({h(p.get("tribe", ""))}) meurt de {h(cause_fr(died.get("cause", "?")))}, '
                             f'{h(day_fr(d, died.get("day")))}</title></g>')
    parts.append('<g class="dots"></g></svg>')
    data = {"days": days, "dpy": year_days(d), "lastYear": years[-1] if years else last_year(d), "r": round(fs * 0.4, 1),
            "positions": pos,
            "tribe": {str(p.get("id")): p.get("tribe") for p in d.get("people", [])},
            "colors": {t: tribe_color(t, i) for i, t in enumerate(ts)},
            "names": {str(p.get("id")): person_fr(p.get("name", p.get("id"))) for p in d.get("people", [])}}
    payload = json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    who = ", ".join(f'<span style="color:{tribe_color(t, i)}">● tribu {h(t)}</span>' for i, t in enumerate(ts))
    legend = (f"{who} : les vivants ce jour-là, avec leur trace des derniers jours · × un mort, là où il est tombé · "
              f"◆ le camp à la fin de chaque année (le chiffre est l’année), relié par son chemin.")
    if not pos:
        legend = (f'<b>{NO_POSITIONS}</b> : les vivants ne sont pas dessinés jour par jour (le vivarium récent les écrit) ; '
                  f'le curseur fait défiler le camp et les morts au fil des jours. {who} · × un mort, là où il est tombé · '
                  f'◆ le camp à la fin de chaque année (le chiffre est l’année).')
    controls = (f'<p class="timeline"><button type="button" data-play>▶ Lire</button> '
                f'<input type="range" min="1" max="{days}" value="{days}" step="1" aria-label="jour"> '
                f'<span class="when">{h(day_fr(d, days))}</span></p>'
                f'<p class="muted small"><label><input type="checkbox" data-heat{"" if pos else " checked"}> '
                f'Montrer aussi où ils ont le plus été dans l’année affichée (plus foncé : plus souvent)</label></p>')
    return (f'<div class="mapbox" data-map="{h(uid)}">{"".join(parts)}{controls}'
            f'<p class="legend">{legend}</p><script type="application/json">{payload}</script></div>')


# --- the discoveries, told as a story of techniques --------------------------

def discoveries(d, tribe):
    """What a tribe found: per kind, tried/kept/copied, the first keeping, who holds it at the end."""
    found = next((f for f in d.get("found", []) if f.get("tribe") == tribe), {})
    people = [p for p in d.get("people", []) if p.get("tribe") == tribe]
    living = [p for p in people if not p.get("died")]
    out = []
    for k in kinds(d):
        tried = (found.get("tried") or {}).get(k, 0)
        kept = (found.get("kept") or {}).get(k, 0)
        copied = (found.get("copied") or {}).get(k, 0)
        if not (tried or kept or copied):
            continue
        first = None
        for p in people:
            for t in p.get("trials") or []:
                if t.get("step") == k and t.get("kept"):
                    day = t.get("judged_day") or t.get("day")
                    if first is None or day < first[0]:
                        first = (day, p.get("name", p.get("id")))
        holders = 0
        for p in living:
            kept_trials = [t for t in p.get("trials") or [] if t.get("kept")]
            if kept_trials and max(kept_trials, key=lambda t: t.get("judged_day") or t.get("day")).get("step") == k:
                holders += 1
        out.append({"kind": k, "tried": tried, "kept": kept, "copied": copied, "first": first, "holders": holders})
    return out


def culture_line(d, tribe):
    """The tribe's culture at the end, in one sentence, from the last year's ways."""
    ys = years_of(d, tribe)
    if not ys:
        return "aucune année lue"
    last = ys[-1]
    w = last.get("ways")
    alive = last.get("population")
    if not w:
        return f"{fmt(alive)} vivant{'s' if (alive or 0) > 1 else ''} à la fin, aucun esprit lu"
    minds = w.get("minds") or 0
    return (f"{fmt(alive)} vivant{'s' if (alive or 0) > 1 else ''} à la fin ({minds} esprit{'s' if minds > 1 else ''} lu{'s' if minds > 1 else ''}) : "
            f"{w.get('sleep_out', 0)} dorment dehors, {minds - (w.get('sleep_out') or 0)} au camp ; "
            f"la valeur d'une plante est divisée par deux à {fmt(w.get('median_worth_tiles'))} tuiles (médiane) ; "
            f"{fmt(w.get('median_stock_kj'))} kJ gardés avant le repos (médiane)"
            + (f" ; camp en ({last['camp'][0]}, {last['camp'][1]})" if last.get("camp") else ""))


def story_html(d, tribe):
    """The techniques of a tribe told in plain French sentences."""
    found = next((f for f in d.get("found", []) if f.get("tribe") == tribe), {})
    tried = sum((found.get("tried") or {}).values())
    kept = sum((found.get("kept") or {}).values())
    copied = sum((found.get("copied") or {}).values())
    disc = discoveries(d, tribe)
    s = (d.get("meta") or {}).get("settings") or {}
    many = len(tribes(d)) > 1
    who = f"La tribu {h(tribe)}" if many else "La bande"
    if not s.get("trials", True):
        return f"<p>{who} n’a rien cherché : les épreuves étaient coupées (c’est le témoin).</p>"
    if not tried:
        return f"<p>{who} n’a rien essayé.</p>"
    lines = [f"<p>{who} a tenté {plural(tried, 'essai')} ; {plural(kept, 'essai')} {'ont' if kept > 1 else 'a'} passé le jugement et "
             f"{'ont' if kept > 1 else 'a'} été gardé{'s' if kept > 1 else ''}, et {fmt(copied)} fois quelqu’un a copié la façon d’un voisin.</p>"]
    held = sorted([x for x in disc if x["holders"]], key=lambda x: -x["holders"])
    if held:
        items = ", ".join(f"<b>{h(kind_fr(x['kind']))}</b> ({plural(x['holders'], 'vivant')} la port{'e' if x['holders'] == 1 else 'ent'})"
                          for x in held)
        lines.append(f"<p>Ce qui reste à la fin : {items}.</p>")
    elif kept:
        lines.append("<p>Rien n’est resté : les façons gardées en route ont toutes été abandonnées ou leurs porteurs sont morts.</p>")
    else:
        lines.append("<p>Les recherches n’ont rien apporté : aucun essai n’a passé le jugement.</p>")
    dropped = [x for x in disc if x["kept"] and not x["holders"]]
    if dropped and held:
        lines.append("<p>Gardé un temps puis abandonné : " + ", ".join(h(kind_fr(x["kind"])) for x in dropped) + ".</p>")
    lost = [x for x in disc if not x["kept"] and x["tried"]]
    if lost:
        lines.append("<p>Essayé sans succès : " + ", ".join(
            f"{h(kind_fr(x['kind']))} ({plural(x['tried'], 'essai')}"
            + (f", mais {plural(x['copied'], 'copie')} d’un voisin qui l’avait déjà" if x["copied"] else "") + ")"
            for x in lost) + ".</p>")
    return "".join(lines)


def found_table(d, tribe):
    """The techniques' small table: technique, first kept when and by whom, copied, carried, what it changes."""
    disc = [x for x in discoveries(d, tribe) if x["kept"] or x["copied"]]
    if not disc:
        return ""
    rows = []
    for x in sorted(disc, key=lambda x: (-x["holders"], -x["kept"])):
        first = f"{h(day_fr(d, x['first'][0]))}</td><td>{h(person_fr(x['first'][1]))}" if x["first"] else '<span class=muted>jamais gardée</span></td><td>-'
        rows.append(f"<tr><td><b>{h(kind_fr(x['kind']))}</b></td><td>{first}</td><td class=n>{fmt(x['copied'])}</td>"
                    f"<td class=n>{fmt(x['holders'])}</td><td class=muted>{h(KIND_CHANGE.get(x['kind'], x['kind']))}</td></tr>")
    return ('<table class="clean"><thead><tr><th>Technique</th><th>Gardée la première fois</th><th>Par</th><th class=n>Copiée par</th>'
            '<th class=n>Portée à la fin</th><th>Ce que ça change</th></tr></thead><tbody>' + "".join(rows) + "</tbody></table>")


def techniques_html(d):
    """The techniques found, tribe by tribe: the story, then the small table."""
    out = []
    for t in tribes(d):
        out.append(f'<section class="tribe">{story_html(d, t)}{found_table(d, t)}</section>')
    return "".join(out) or '<p class="muted">aucune tribu lue</p>'


def kept_list(d, tribe, limit=200):
    """`jour 212, personne 14 cherche plus loin, garde` for every kept trial, by day."""
    items = []
    for p in d.get("people", []):
        if p.get("tribe") != tribe:
            continue
        for t in p.get("trials") or []:
            if t.get("kept"):
                items.append((t.get("judged_day") or t.get("day") or 0, p.get("name", p.get("id")), t))
    items.sort(key=lambda x: x[0])
    if not items:
        return '<p class="muted">aucun essai gardé</p>'
    li = "".join(f"<li>jour {day}, {h(person_fr(who))} : {h(kind_fr(t.get('step', '?')))}, garde "
                 f"<span class=muted>({h(t.get('words', ''))}, essayé le jour {t.get('day')})</span></li>"
                 for day, who, t in items[:limit])
    more = f'<li class=muted>… et {len(items) - limit} de plus</li>' if len(items) > limit else ""
    return f"<ol class=kept>{li}{more}</ol>"


# --- the fire and the common memory (D55) -------------------------------------------

WILL_BAND_FR = {"timid": "frileux (0–20)", "middling": "moyens (21–50)", "bold": "aventuriers (51–100)"}
WAY_FR = {"worth_half_tiles": "la distance de recherche", "sleep_out": "la nuit dehors ou au camp",
          "stock_kj": "la réserve avant le repos", "home": "la maison"}


def has_fire_data(d):
    """Whether the run was written with the fire's keys (D55): `records` or a `fire` in some year."""
    return "records" in d or any("fire" in y for y in d.get("years", []))


def knowledge_at_end(d):
    """What the tribes knew at the last year's end: places, by hearsay only, the share of accesses heard."""
    ys = [y for y in years_of(d) if y.get("year") == last_year(d) and y.get("knowledge")]
    if not ys:
        return None
    places = sum(y["knowledge"].get("places") or 0 for y in ys)
    acc = sum(y["knowledge"].get("accesses") or 0 for y in ys)
    heard = sum(y["knowledge"].get("accesses_heard") or 0 for y in ys)
    return {"places": places, "heard_only": sum(y["knowledge"].get("heard_only") or 0 for y in ys),
            "accesses": acc, "heard_share": heard / acc if acc else None,
            "by_tribe": {y["tribe"]: y["knowledge"].get("places") or 0 for y in ys}}


def records_by_id(d):
    return {r.get("id"): r for r in d.get("records") or []}


def place_fr(place):
    return f"({place[0]}, {place[1]})" if place else "?"


def record_fr(d, r):
    """A record in words: `un lieu (1952, 1936), vu le jour 200 par personne 14`, `un essai gardé : la réserve avant le repos`."""
    if r is None:
        return "un souvenir oublié"
    kind = r.get("kind")
    if kind == "place":
        states = r.get("states") or []
        what = "nu" if not states else f"{len(states)} sorte{'s' if len(states) > 1 else ''} de nourriture"
        return f"un lieu {place_fr(r.get('place'))}, {what}, vu le jour {r.get('day')}"
    if kind == "water":
        return f"de l'eau trouvée le jour {r.get('day')}"
    if kind == "trial":
        verdict = "gardé" if r.get("kept") else "abandonné"
        return f"un essai {verdict} le jour {r.get('day')} : {WAY_FR.get(r.get('way'), r.get('way') or '?')} {'en plus' if r.get('up') else 'en moins'}"
    if kind == "night":
        return f"une nuit du jour {r.get('day')}, {r.get('present')} présents"
    return f"un souvenir ({kind})"


def knowledge_chart(d):
    """Places known by each tribe at each year's end, and the stored food at noon, two small charts."""
    yd = year_days(d)
    ts = tribes(d)
    rows = years_of(d)
    n_years = max((y["year"] for y in rows), default=1)
    top = max([(y.get("knowledge") or {}).get("places") or 0 for y in rows] + [1])
    chart = Chart(0, n_years * yd, top, "années", "lieux connus par la tribu", yd)
    stored = Chart(0, n_years * yd, max([((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean") or 0 for y in rows] + [1]),
                   "années", "nourriture portée à midi, moyenne de l'an (kJ)", yd)
    for i, t in enumerate(ts):
        mine = [y for y in rows if y["tribe"] == t]
        xs = [y["year"] * yd for y in mine]
        color = tribe_color(t, i)
        ks = [(y.get("knowledge") or {}).get("places") for y in mine]
        chart.line(xs, ks, color, f"tribu {t}")
        chart.hover(xs, ks, [f"tribu {t}, an {y['year']} : {fmt((y.get('knowledge') or {}).get('places'))} lieux, "
                             f"{fmt((y.get('knowledge') or {}).get('heard_only'))} par ouï-dire seul, "
                             f"{fmt((y.get('knowledge') or {}).get('accesses_heard'))} souvenirs entendus sur {fmt((y.get('knowledge') or {}).get('accesses'))}"
                             for y in mine], color)
        ss = [((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean") for y in mine]
        stored.line(xs, ss, color, f"tribu {t}")
        stored.hover(xs, ss, [f"tribu {t}, an {y['year']} : {fmt(s)} kJ portés à midi en moyenne, "
                              f"minimum {fmt(((y.get('food') or {}).get('stored_kj') or {}).get('noon_min'))}, fin {fmt((y.get('food') or {}).get('end_kj'))}"
                              for y, s in zip(mine, ss)], color)
    return chart.svg("lieux connus par an"), stored.svg("nourriture portée à midi par an")


def knowledge_html(d):
    """The section « Ce que la tribu sait »: the chart of places known, the share heard, the fire's counts."""
    sm_k = knowledge_at_end(d)
    places_svg, stored_svg = knowledge_chart(d)
    if sm_k is None:
        line = "aucune clé <code>years[].knowledge</code> dans ce run"
    else:
        share = "" if sm_k["heard_share"] is None else f" ; {sm_k['heard_share']:.0%} des souvenirs des vivants viennent d'un conteur"
        line = (f"À la fin, {plural(sm_k['places'], 'lieu', 'x')} connu{'s' if sm_k['places'] > 1 else ''} "
                f"({', '.join(f'tribu {h(t)} {fmt(n)}' for t, n in sm_k['by_tribe'].items())}), "
                f"dont {fmt(sm_k['heard_only'])} que personne de vivant n'a vu{'s' if sm_k['heard_only'] > 1 else ''} lui-même{share}.")
    fire_rows = []
    for y in years_of(d):
        fi = y.get("fire") or {}
        k = y.get("knowledge") or {}
        fire_rows.append(f"<tr><td class=n>{y.get('year')}</td><td>{h(y.get('tribe', ''))}</td><td class=n>{fmt(k.get('places'))}</td>"
                         f"<td class=n>{fmt(k.get('heard_only'))}</td><td class=n>{fmt(k.get('accesses_heard'))} / {fmt(k.get('accesses'))}</td>"
                         f"<td class=n>{fmt(fi.get('nights'))}</td><td class=n>{fmt(fi.get('told'))}</td><td class=n>{fmt(fi.get('heard'))}</td>"
                         f"<td class=n>{fmt(fi.get('dispelled'))}</td></tr>")
    table = ('<table class="clean"><thead><tr><th>an</th><th>tribu</th><th class=n>lieux connus</th><th class=n>par ouï-dire seul</th>'
             '<th class=n>souvenirs entendus / tenus</th><th class=n>nuits au feu</th><th class=n>dits</th><th class=n>entendus</th>'
             '<th class=n>rumeurs dissipées</th></tr></thead><tbody>' + "".join(fire_rows) + "</tbody></table>")
    return ("<h2>Ce que la tribu sait</h2><p class=caption>Les lieux dont un vivant de la tribu garde un souvenir à la fin de chaque an, vus ou entendus au feu ; "
            "et la nourriture que la tribu porte à midi, la mesure du lead.</p>"
            f"<p>{line}</p><div class=charts>{places_svg}{stored_svg}</div>{table}")


def dared_html(d):
    """The table « Qui a osé quoi »: each person's will at the start and the end, its trials, kept, heard; then by band."""
    rows = []
    people = [p for p in d.get("people", []) if p.get("trials")]
    people.sort(key=lambda p: (-len(p.get("trials") or []), p.get("id", 0)))
    for p in people:
        trials = p.get("trials") or []
        kept = sum(1 for t in trials if t.get("kept"))
        heard = sum(1 for t in trials if t.get("heard"))
        will = f"{fmt(p.get('will_start'))} → {fmt(p.get('will'))}" if p.get("will_start") is not None else fmt(p.get("will"))
        rows.append(f"<tr><td>{h(person_fr(p.get('name', p.get('id'))))}</td><td>{h(p.get('tribe', ''))}</td><td class=n>{will}</td>"
                    f"<td class=n>{len(trials)}</td><td class=n>{kept}</td><td class=n>{heard}</td>"
                    f"<td class=muted>{h(', '.join(kind_fr(t.get('step', '?')) for t in trials if t.get('kept')))}</td></tr>")
    bands = []
    for f in d.get("found") or []:
        for b in f.get("by_will") or []:
            bands.append(f"<tr><td>{h(f.get('tribe', ''))}</td><td>{h(WILL_BAND_FR.get(b.get('band'), b.get('band') or '?'))}</td>"
                         f"<td class=n>{fmt(b.get('tried'))}</td><td class=n>{fmt(b.get('kept'))}</td></tr>")
    table = ('<table class="clean"><thead><tr><th>qui</th><th>tribu</th><th class=n>volonté début → fin</th><th class=n>essais</th>'
             '<th class=n>gardés</th><th class=n>dont entendus au feu</th><th>ce qu\'il garde</th></tr></thead><tbody>'
             + ("".join(rows) or '<tr><td colspan=7 class=muted>personne n\'a rien essayé</td></tr>') + "</tbody></table>")
    by_band = ('<table class="clean"><thead><tr><th>tribu</th><th>volonté</th><th class=n>essais</th><th class=n>gardés</th></tr></thead><tbody>'
               + "".join(bands) + "</tbody></table>") if bands else ""
    return ("<h2>Qui a osé quoi</h2><p class=caption>La volonté de chacun au départ et à la fin (elle monte d'un cran à un essai gardé, baisse à un abandon), "
            "ses essais, ceux qu'il a gardés, ceux qu'il a tirés d'un ouï-dire ; puis les essais par bande de volonté.</p>" + table + by_band)


def nights_html(d):
    """The table « Qui a dit quoi à qui »: the nights the hearths still hold, what was told and by whom."""
    recs = records_by_id(d)
    out = []
    for hearth in d.get("hearth") or []:
        nights = hearth.get("nights") or []
        if not nights:
            continue
        rows = []
        for n in sorted(nights, key=lambda n: n.get("day") or 0):
            told = " ; ".join(f"{h(person_fr(t.get('by')))} : {h(record_fr(d, recs.get(t.get('record'))))}" for t in n.get("told") or [])
            rows.append(f"<tr><td class=n>{fmt(n.get('day'))}</td><td class=n>{fmt(n.get('present'))}</td><td>{told or '<span class=muted>rien dit</span>'}</td></tr>")
        out.append(f"<h4>Tribu {h(hearth.get('tribe', ''))}, {plural(len(nights), 'nuit')} gardée{'s' if len(nights) > 1 else ''} au foyer</h4>"
                   '<table class="clean"><thead><tr><th class=n>jour</th><th class=n>présents</th><th>qui a dit quoi</th></tr></thead><tbody>'
                   + "".join(rows) + "</tbody></table>")
    return ("<h3>Qui a dit quoi à qui</h3><p class=muted>Les dernières nuits que le foyer garde (les plus anciennes sont oubliées) : "
            "chaque chose dite, par qui ; ce que chacun en a cru est dans sa vie (« l'ont cru »).</p>" + ("".join(out) or '<p class="muted">aucune nuit gardée</p>'))


def rumours_html(d):
    """The list « Rumeurs »: places heard and found bare, by year."""
    rums = d.get("rumours") or []
    yd = year_days(d)
    by_year = {}
    for r in rums:
        by_year.setdefault((r.get("day", 1) - 1) // yd + 1, []).append(r)
    items = []
    for year in sorted(by_year):
        li = "".join(f"<li>jour {r.get('day')}, {h(person_fr(r.get('body')))} va où {h(person_fr(r.get('from')))} avait dit {place_fr(r.get('place'))} et ne trouve rien</li>"
                     for r in by_year[year][:50])
        more = f"<li class=muted>… et {len(by_year[year]) - 50} de plus</li>" if len(by_year[year]) > 50 else ""
        items.append(f"<li>an {year} : {plural(len(by_year[year]), 'rumeur')} dissipée{'s' if len(by_year[year]) > 1 else ''}<ol class=kept>{li}{more}</ol></li>")
    return ("<h3>Rumeurs</h3><p class=muted>Un lieu entendu au feu, trouvé nu par celui qui y est allé : il le dit le soir même, et les présents le croient.</p>"
            + (f"<ul>{''.join(items)}</ul>" if items else '<p class="muted">aucune rumeur dissipée</p>'))


def believes_html(d, p):
    """What a person believes of the land at the end: its places, seen or heard from whom, which day, still true?"""
    bel = p.get("believes") or []
    if not bel:
        return ""
    recs = records_by_id(d)
    later = {}
    for r in d.get("records") or []:
        if r.get("kind") == "place" and r.get("place"):
            key = tuple(r["place"])
            later[key] = max(later.get(key, 0), r.get("day") or 0)
    seen = sum(1 for b in bel if b.get("seen"))
    rows = []
    for b in sorted(bel, key=lambda b: -(b.get("day") or 0)):
        r = recs.get(b.get("record"))
        key = tuple(b.get("place") or ())
        still = "oui" if r is not None and later.get(key, 0) <= (r.get("day") or 0) else "non : un souvenir plus récent le contredit"
        source = "vu lui-même" if b.get("seen") else f"entendu de {h(person_fr(b.get('by')))}"
        rows.append(f"<tr><td>{place_fr(b.get('place'))}</td><td>{source}</td><td class=n>{fmt(b.get('day'))}</td>"
                    f"<td>{h(record_fr(d, r))}</td><td>{still}</td></tr>")
    return (f"<details><summary>Ce qu'il croit : {plural(len(bel), 'lieu', 'x')}, {seen} vu{'s' if seen > 1 else ''}, {len(bel) - seen} entendu{'s' if len(bel) - seen > 1 else ''}</summary>"
            '<table class="clean"><thead><tr><th>lieu</th><th>source</th><th class=n>jour</th><th>souvenir</th><th>encore vrai ?</th></tr></thead><tbody>'
            + "".join(rows) + "</tbody></table></details>")


# --- the tables -------------------------------------------------------------------

def years_table(d):
    cs = causes(d)
    head = ("<tr><th>an</th><th>tribu</th><th>début → fin</th><th>‰</th><th>naiss.</th><th>morts</th>"
            + "".join(f"<th>{h(cause_fr(c))}</th>" for c in cs)
            + "<th>15 ans</th><th>femmes seules</th><th>essais / gardés / copiés</th><th>maisons</th><th>camp</th>"
            "<th>cueilli kJ</th><th>mangé</th><th>gâté</th><th>perdu</th><th>fin</th><th>porté à midi moy. / min</th>"
            "<th>stock q1 / méd. / q3</th><th>j. maigres</th><th>j. faibles</th>"
            "<th>lieux connus (ouï-dire seul)</th><th>feu : nuits / dits / entendus</th><th>rumeurs</th></tr>")
    rows = []
    for y in years_of(d):
        f = y.get("food") or {}
        ft = y.get("fifteen") or {}
        fw = y.get("fertile_women") or {}
        camp = f"({y['camp'][0]}, {y['camp'][1]})" if y.get("camp") else "-"
        g = y.get("growth_permille")
        st = f.get("stored_kj") or {}
        q = (y.get("ways") or {}).get("stock_kj") or {}
        k = y.get("knowledge") or {}
        fi = y.get("fire") or {}
        rows.append(f"<tr><td class=n>{y.get('year')}</td><td>{h(y.get('tribe', ''))}</td>"
                    f"<td class=n>{fmt(y.get('population_before'))} → {fmt(y.get('population'))}</td>"
                    f"<td class=n>{'-' if g is None else ('+' if g > 0 else '') + fmt(g)}</td>"
                    f"<td class=n>{fmt(y.get('births'))}</td><td class=n>{fmt(y.get('deaths_total'))}</td>"
                    + "".join(f"<td class=n>{fmt((y.get('deaths') or {}).get(c, 0))}</td>" for c in cs)
                    + f"<td class=n>{fmt(ft.get('alive'))}/{fmt(ft.get('turned'))}</td>"
                    f"<td class=n>{fmt(fw.get('without_spouse'))}/{fmt(fw.get('total'))}</td>"
                    f"<td class=n>{fmt(y.get('trials'))} / {fmt(y.get('kept'))} / {fmt(y.get('copied'))}</td>"
                    f"<td class=n>{fmt(y.get('homes_moved'))}</td><td>{camp}</td>"
                    f"<td class=n>{fmt(f.get('gathered_kj'))}</td><td class=n>{fmt(f.get('eaten_kj'))}</td>"
                    f"<td class=n>{fmt(f.get('spoiled_kj'))}</td><td class=n>{fmt(f.get('lost_kj'))}</td><td class=n>{fmt(f.get('end_kj'))}</td>"
                    f"<td class=n>{fmt(st.get('noon_mean'))} / {fmt(st.get('noon_min'))}</td>"
                    f"<td class=n>{fmt(q.get('q1'))} / {fmt(q.get('median'))} / {fmt(q.get('q3'))}</td>"
                    f"<td class=n>{fmt(hours_to_days(f.get('thin_hours')))}</td><td class=n>{fmt(hours_to_days(f.get('weak_hours')))}</td>"
                    f"<td class=n>{fmt(k.get('places'))} ({fmt(k.get('heard_only'))})</td>"
                    f"<td class=n>{fmt(fi.get('nights'))} / {fmt(fi.get('told'))} / {fmt(fi.get('heard'))}</td>"
                    f"<td class=n>{fmt(fi.get('dispelled'))}</td></tr>")
    note = ("<p class=muted>Une ligne par an et par tribu. ‰ : croissance pour mille ; 15 ans : vivants / arrivés à quinze ans parmi les nés du run ; "
            "femmes seules : sans époux possible / femmes de 16 à 42 ans ; jours maigres et faibles : heures-personne sous la moitié "
            "et sous le cinquième de la réserve, divisées par 24 ; porté à midi : la nourriture que portent les vivants de la tribu, "
            "moyenne et minimum des midis de l'an ; stock : quartiles de ce que chacun garde avant de se reposer ; lieux connus : "
            "lieux distincts dont un vivant de la tribu a un souvenir à la fin de l'an, entre parenthèses ceux que personne n'a vus lui-même.</p>")
    return f'<div class=scroll><table class=years>{head}{"".join(rows)}</table></div>{note}'


def hours_to_days(hours):
    if hours is None:
        return None
    return round(hours / 24, 1)


def cause_html(c):
    return f'<span class="cause" style="color:{CAUSE_COLOR.get(c, "#333")}">{h(cause_fr(c))}</span>'


def table_html(title, caption, head, rows, empty, cls="clean"):
    """A titled table: title, one-line caption, full-word headers, zebra rows (the CSS), no borders."""
    out = [f"<h2>{h(title)}</h2><p class=caption>{h(caption)}</p>"]
    if not rows:
        out.append(f'<p class="muted">{h(empty)}</p>')
    else:
        ths = "".join(f'<th{" class=n" if right else ""}>{h(t)}</th>' for t, right in head)
        out.append(f'<table class="{cls}"><thead><tr>{ths}</tr></thead><tbody>{"".join(rows)}</tbody></table>')
    return "".join(out)


def deaths_table(d):
    """« Les morts » : who, when, age at death, cause in words, where (from the camp)."""
    many = len(tribes(d)) > 1
    rows = []
    for p in sorted((p for p in d.get("people", []) if p.get("died")), key=lambda p: p["died"].get("day") or 0):
        died = p["died"]
        rows.append(f"<tr><td>{h(person_fr(p.get('name', p.get('id'))))}{' (' + h(p.get('tribe', '')) + ')' if many else ''}</td>"
                    f"<td>{h(day_fr(d, died.get('day')))}</td><td class=n>{h(age_fr(d, age_days(d, p, died.get('day'))))}</td>"
                    f"<td>{cause_html(died.get('cause', '?'))}</td><td>{h(where_fr(d, p.get('tribe'), died.get('at'), died.get('day')))}</td></tr>")
    return table_html("Les morts", "Chaque mort du run, dans l’ordre des jours : qui, quand, à quel âge, de quoi, et où par rapport au camp de sa tribu cette année-là.",
                      [("Qui", False), ("Quand", False), ("Âge à la mort", True), ("De quoi", False), ("Où", False)],
                      rows, "Personne n’est mort.")


def births_table(d):
    """« Les naissances » : who, when, mother, father, alive at the end?"""
    many = len(tribes(d)) > 1
    rows = []
    for p in sorted((p for p in d.get("people", []) if p.get("born") != "start"), key=lambda p: p.get("born_day") or p.get("born") or 0):
        died = p.get("died")
        fate = ('<span class=alive>oui</span>' if not died else
                f'<span class=dead>non</span> : {"morte" if p.get("sex") == "woman" else "mort"} de {h(cause_fr(died.get("cause", "?")))} '
                f'à {h(age_fr(d, age_days(d, p, died.get("day"))))}')
        rows.append(f"<tr><td>{h(person_fr(p.get('name', p.get('id'))))}{' (' + h(p.get('tribe', '')) + ')' if many else ''}</td>"
                    f"<td>{h(day_fr(d, p.get('born_day') or p.get('born')))}</td>"
                    f"<td>{h(person_fr(p['mother'])) if p.get('mother') is not None else '-'}</td>"
                    f"<td>{h(person_fr(p['father'])) if p.get('father') is not None else '-'}</td><td>{fate}</td></tr>")
    return table_html("Les naissances", "Chaque enfant né pendant le run : qui, quand, de qui, et s’il est vivant à la fin.",
                      [("Qui", False), ("Quand", False), ("Mère", False), ("Père", False), ("Vivant à la fin ?", False)],
                      rows, "Aucune naissance.")


def people_end_table(d):
    """« Les gens à la fin » : name, tribe, sex, fate, age at the end or at death, children, techniques carried."""
    rows = []
    people = sorted(d.get("people", []), key=lambda p: (1 if p.get("died") else 0, p.get("tribe", ""), p.get("id", 0)))
    for p in people:
        died = p.get("died")
        woman = p.get("sex") == "woman"
        if died:
            fate = f'<span class=dead>{"morte" if woman else "mort"}</span>, {h(day_fr(d, died.get("day")))} ({h(cause_fr(died.get("cause", "?")))})'
            age = age_fr(d, age_days(d, p, died.get("day")))
        else:
            fate = f'<span class=alive>{"vivante" if woman else "vivant"}</span>'
            age = age_fr(d, age_days(d, p))
        techs = sorted({t.get("step") for t in p.get("trials") or [] if t.get("kept") and t.get("step")}, key=lambda k: (kinds(d) + [k]).index(k))
        tech_txt = ", ".join(h(kind_fr(k)) for k in techs) or '<span class=muted>aucune</span>'
        rows.append(f"<tr><td>{h(person_fr(p.get('name', p.get('id'))))}</td><td>{h(p.get('tribe', ''))}</td>"
                    f"<td>{h(SEX_FR.get(p.get('sex'), p.get('sex') or '-'))}</td><td>{fate}</td><td class=n>{h(age)}</td>"
                    f"<td class=n>{fmt(p.get('children'))}</td><td>{tech_txt}</td></tr>")
    return table_html("Les gens à la fin", "Tout le monde, les vivants d’abord : son sort, son âge à la fin ou à sa mort, ses enfants, les techniques qu’il a gardées en route.",
                      [("Qui", False), ("Tribu", False), ("Sexe", False), ("Sort", False), ("Âge à la fin, ou à la mort", True),
                       ("Enfants", True), ("Techniques portées", False)],
                      rows, "Personne.")


def people_table(d):
    rows = []
    for p in sorted(d.get("people", []), key=lambda p: (p.get("tribe", ""), p.get("id", 0))):
        died = p.get("died")
        death = (f"jour {died.get('day')}, {h(cause_fr(died.get('cause', '?')))}"
                 + (f" ({h(SEASON_FR.get(died.get('season'), died.get('season') or ''))})" if died.get("season") else "")) if died else "vivant"
        born = "au départ" + (f" ({age_fr(d, p['age_days_at_start'])})" if p.get("age_days_at_start") is not None else "") \
            if p.get("born") == "start" else f"jour {p.get('born')}"
        ways = p.get("ways") or {}
        way_txt = (f"{'dort dehors' if ways.get('sleep_out') else 'dort au camp'}, cherche jusqu'à {fmt(ways.get('worth_half_tiles'))} tuiles, "
                   f"garde {fmt(ways.get('stock_kj'))} kJ, maison ({ways['home'][0]}, {ways['home'][1]})") if ways and ways.get("home") else "esprit non lu"
        lines = "".join(f"<li>jour {ln.get('day')} : {h(ln.get('what', ''))}</li>" for ln in p.get("lines") or [])
        trials = len(p.get("trials") or [])
        kept = sum(1 for t in p.get("trials") or [] if t.get("kept"))
        will_txt = fmt(p.get("will"))
        if p.get("will_start") is not None and p.get("will_start") != p.get("will"):
            will_txt = f"{fmt(p.get('will_start'))} → {fmt(p.get('will'))}"
        detail = (f"<details><summary>{h(person_fr(p.get('name', p.get('id'))))}</summary>"
                  f"<p class=muted>{h(way_txt)} ; {trials} essai{'s' if trials > 1 else ''}, {kept} gardé{'s' if kept > 1 else ''}, "
                  f"{fmt(p.get('copied'))} copie{'s' if (p.get('copied') or 0) > 1 else ''} ; "
                  f"{fmt(hours_to_days(p.get('thin_hours')))} j. maigres, {fmt(hours_to_days(p.get('weak_hours')))} j. faibles ; "
                  f"{fmt(p.get('gathered_kj'))} kJ cueillis, {fmt(p.get('meals'))} repas, {fmt(p.get('drinks'))} boissons.</p>"
                  f"<ul class=lines>{lines or '<li class=muted>aucune ligne</li>'}</ul>{believes_html(d, p)}</details>")
        rows.append(f"<tr><td class=n>{p.get('id')}</td><td>{h(p.get('tribe', ''))}</td><td>{h(SEX_FR.get(p.get('sex'), p.get('sex') or '-'))}</td>"
                    f"<td>{born}</td><td>{death}</td><td class=n>{fmt(p.get('children'))}</td><td class=n>{will_txt}</td><td>{detail}</td></tr>")
    return (f"<table class=people><tr><th>n°</th><th>tribu</th><th>sexe</th><th>né</th><th>mort</th><th>enfants</th><th>volonté</th><th>sa vie</th></tr>"
            f"{''.join(rows)}</table>")


def verdict(d, sm=None):
    """The run in three everyday sentences: who is alive, what killed, what was kept."""
    sm = sm or summary(d)
    settings = (d.get("meta") or {}).get("settings") or {}
    alive, start, births = sm["alive"], sm["start"], sm["births"]
    when = f"Après {plural(sm['years'], 'an')}" if sm["years"] else f"Après {plural(run_days(d), 'jour')}"
    alive_start = sm["alive_start"]
    alive_born = alive - alive_start
    if alive == 0:
        first = f"{when}, plus personne : les {fmt(start)} du départ sont morts" + \
            ("" if not births else ", et l’enfant né en route aussi" if births == 1 else f", et les {plural(births, 'enfant')} nés en route aussi") + "."
    else:
        if alive_start == 0:
            first = f"{when}, plus aucun des {fmt(start)} du départ n’est vivant"
        else:
            first = f"{when}, {fmt(alive_start)} des {fmt(start)} du départ {'sont vivants' if alive_start > 1 else 'est vivant'}"
        if births and alive_born == 0:
            first += ", et l’enfant né en route n’a pas survécu" if births == 1 else f", et aucun des {plural(births, 'enfant')} nés en route n’a survécu"
        elif births and alive_born == births:
            first += ", mais l’enfant né en route l’est" if births == 1 and not alive_start else \
                (", et l’enfant né en route aussi" if births == 1 else f", {'et' if alive_start else 'mais'} les {plural(births, 'enfant')} nés en route aussi")
        elif births:
            first += f", {'et' if alive_start else 'mais'} {fmt(alive_born)} des {plural(births, 'enfant')} nés en route"
        first += "."
    deaths = deaths_sentence(d, sm)
    if not settings.get("trials", True):
        third = "Les épreuves étaient coupées (c’est le témoin) : rien à garder."
    else:
        held = sorted(((k, n) for k, n in carried(d).items() if n), key=lambda x: (-x[1], kinds(d).index(x[0]) if x[0] in kinds(d) else 99))
        if not held:
            third = (f"Aucune technique n’est portée à la fin, malgré {plural(sm['kept'], 'essai')} gardé{'s' if sm['kept'] > 1 else ''} en route."
                     if sm["kept"] else "Aucune technique gardée.")
        elif len(held) == 1:
            third = f"Une technique gardée : {kind_fr(held[0][0])}."
        else:
            names = [kind_fr(k) for k, _ in held]
            count = {2: "Deux", 3: "Trois", 4: "Quatre", 5: "Cinq"}.get(len(held), fmt(len(held)))
            third = f"{count} techniques gardées : {', '.join(names[:-1])} et {names[-1]}."
    return f"{deaths} {first} {third}"


def deaths_sentence(d, sm):
    """The deaths by want first, each cause counted, then the background deaths:
    "3 morts de manque : 1 de faim, 2 de froid, le plus souvent en hiver ; 24 morts de fond : maladie 22, vieillesse 2."
    """
    want, background = sm["want"], sm["background"]
    if not want and not background:
        return "Personne n’est mort."
    if want:
        seasons = {}
        for p in d.get("people", []):
            died = p.get("died")
            if died and is_want(died.get("cause")) and died.get("season"):
                seasons[died["season"]] = seasons.get(died["season"], 0) + 1
        season = max(seasons, key=seasons.get) if seasons else None
        when = "" if not season else (f", {SEASON_IN.get(season, season)}" if seasons[season] == want
                                       else f", le plus souvent {SEASON_IN.get(season, season)}")
        first = f"{plural(want, 'mort')} de manque : " + ", ".join(f"{fmt(n)} de {cause_fr(c)}" for c, n in sm["want_by"].items() if n) + when
    else:
        first = "Aucun mort de manque"
    if background:
        second = f"{plural(background, 'mort')} de fond : " + ", ".join(f"{cause_fr(c)} {fmt(n)}" for c, n in sm["background_by"].items() if n)
    else:
        second = "aucun mort de fond"
    return f"{first} ; {second}."


def stock_sub(st):
    """The small line under the stock tile: days of food per living person, the year's start, each tribe."""
    if st is None:
        return "aucune clé <code>years[].food</code> dans ce run"
    parts = []
    if st["days_per_person"] is not None:
        parts.append(f"≈ {plural(round(st['days_per_person']), 'jour')} de vivres par vivant")
    parts.append(f"début de l’an {fmt(st['year'])} : {fmt(st['start_kj'])}")
    if len(st["by_tribe"]) > 1:
        parts.append(", ".join(f"{h(t)} {fmt(kj)}" for t, kj in st["by_tribe"].items()))
    return " · ".join(parts)


def tiles_html(sm, other=None, label=None):
    """Six big tiles: alive, deaths by want, background deaths, births, stored food, techniques kept;
    with `other`, a coloured arrow up or down."""
    def tile(cls, title, value, ref, sub, good_up=True, text=None):
        arrow = ""
        if other is not None and ref is not None and value is not None and value != ref:
            dv = value - ref
            good = (dv > 0) == good_up
            arrow = (f'<span class="arrow {"good" if good else "bad"}" title="{"+" if dv > 0 else ""}{fmt(dv)} par rapport à l’autre côté">'
                     f'{"↑" if dv > 0 else "↓"} {"+" if dv > 0 else ""}{fmt(dv)}</span>')
        shown = f'<div class="v text">{h(text)}</div>' if text else f"<div class=v>{fmt(value)}{arrow}</div>"
        return f'<div class="tile {cls}">{shown}<div class=k>{h(title)}</div><div class=s>{sub}</div></div>'
    o = other or {}
    st = sm.get("stock")
    ost = o.get("stock")
    tiles = [tile("alive", "vivants à la fin", sm["alive"], o.get("alive"), f"{fmt(sm['start'])} au départ, {plural(sm['years'], 'an')}"),
             tile("want", "morts de manque", sm["want"], o.get("want"),
                  " · ".join(f"{cause_html(c)} {fmt(n)}" for c, n in sm["want_by"].items()), good_up=False),
             tile("background", "morts de fond", sm["background"], o.get("background"),
                  " · ".join(f"{cause_html(c)} {fmt(n)}" for c, n in sm["background_by"].items()), good_up=False),
             tile("births", "naissances", sm["births"], o.get("births"),
                  f"{fmt(sm['alive'] - sm['alive_start'])} encore vivant{'s' if sm['alive'] - sm['alive_start'] > 1 else ''}" if sm["births"] else "aucune"),
             tile("stock", "nourriture stockée (kJ)", st["kj"] if st else None, ost["kj"] if ost else None, stock_sub(st),
                  text=None if st else NO_STOCK),
             tile("kept", "techniques gardées", sm["techniques"], o.get("techniques"),
                  f"{plural(sm['kept'], 'essai')} gardé{'s' if sm['kept'] > 1 else ''} sur {fmt(sm['trials'])}, {plural(sm['copied'], 'copie')}")]
    return f'<div class=tiles>{"<div class=tilelabel>" + h(label) + "</div>" if label else ""}{"".join(tiles)}</div>'


# --- the pages' bodies ------------------------------------------------------------

def version_html(v):
    """`commit abc1234 (feat/x) — « ce qui a changé »` for a series' version record."""
    v = v or {}
    commit = v.get("commit") or "inconnu"
    branch = v.get("branch") or ""
    dirty = " +modifs locales" if v.get("dirty") else ""
    change = v.get("change") or ""
    txt = f'<code>{h(commit)}</code>' + (f' ({h(branch)}{dirty})' if branch else "")
    if change:
        txt += f' — <b>{h(change)}</b>'
    return f'<span class=version title="{h(v.get("worktree") or "")}">version {txt}</span>'


def run_url(serie, run):
    return f"/run/{urllib.parse.quote(serie)}/{urllib.parse.quote(run)}"


def serie_url(serie):
    return f"/serie/{urllib.parse.quote(serie)}"


def compare_url(a, b):
    return "/compare?" + urllib.parse.urlencode({"a": a, "b": b})


def trials_tag(run):
    return "avec" if run.get("trials", True) else "sans"


def run_body(s, r, d):
    """The page of one run: `s` the series (from Lab.series()), `r` its run's state, `d` its data or None."""
    serie, name = s["name"], r["name"]
    where = {"fini": "terminé", "en cours": f"en cours, année {fmt(r.get('year'))}", "attend": "pas encore lancé"}.get(r["state"], r["state"])
    head = (f'<h1>Run {h(name)} <span class="etat">{h(where)}</span></h1>'
            f'<p class=muted>série <a href="{serie_url(serie)}">{h(serie)}</a> · {version_html(s.get("version"))} · '
            f'graine {r.get("seed", "?")}, {trials_tag(r)} épreuves · {secs(r.get("elapsed"))}'
            + (f' · <a href="{r["page"]}">page du vivarium</a>' if r.get("page") else "")
            + (f' · <a href="{r["log"]}">log</a>' if r.get("log") else "") + "</p>")
    if s.get("question"):
        head += f'<p class=q>{h(s["question"])}</p>'
    if d is None:
        live = f" Le run est {where} : {fmt(r.get('alive'))} vivants, {fmt(r.get('dead'))} morts d’après le log." if r["state"] == "en cours" else ""
        return head + f'<p class="err">{NO_DATA}.{h(live)}</p>'
    sm = summary(d)
    ts = tribes(d)
    body = [head, f"<p class=verdict>{h(verdict(d, sm))}</p>", tiles_html(sm)]
    body.append("<h2>La carte</h2><p class=caption>Où ils étaient, jour après jour : faites glisser le curseur ou appuyez sur Lire.</p>" + map_svg(d, "m1"))
    body.append("<h2>La population</h2><p class=caption>Les vivants jour par jour, par tribu ; les hivers en bleu pâle, les années en bas.</p>"
                + population_chart(d, wide=True).svg("population par tribu"))
    body.append("<p class=caption>Dessous, les morts de chaque année par tribu : en rouge ceux de manque (faim, soif, froid), en gris ceux "
                "de fond (maladie, vieillesse, couches) ; le contour dit la tribu, A à gauche de l’année"
                + (", B à droite" if len(ts) > 1 else "") + ".</p>" + want_chart(d, wide=True).svg("morts de manque et de fond par an"))
    body.append("<h2>Ce qu’ils ont trouvé</h2><p class=caption>Les techniques essayées, gardées et copiées, tribu par tribu.</p>" + techniques_html(d))
    if has_fire_data(d):
        body.append(knowledge_html(d))
        body.append(dared_html(d))
    body.append(deaths_table(d))
    body.append(births_table(d))
    body.append(people_end_table(d))
    detail = [f"<p class=settings>{h(settings_fr(d))}</p>"]
    for t in ts:
        detail.append(f"<p><b>La culture de la tribu {h(t)} à la fin</b> : {h(culture_line(d, t))}.</p>")
    detail.append("<h3>Les autres courbes</h3><div class=charts>")
    detail.append(births_deaths_chart(d).svg("naissances et morts par an"))
    detail.append(kept_chart(d).svg("essais gardés par an"))
    detail.append(reserve_chart(d).svg("réserve moyenne"))
    detail.append("</div>")
    for t in ts:
        detail.append(f"<h3>Les essais gardés de la tribu {h(t)}, par jour</h3>{kept_list(d, t)}")
    if has_fire_data(d):
        detail.append(nights_html(d))
        detail.append(rumours_html(d))
    detail.append("<h3>Année par année</h3>" + years_table(d))
    detail.append(f"<h3>La vie de chacun ({len(d.get('people', []))})</h3>" + people_table(d))
    body.append(f'<details class=detail><summary>Le détail</summary>{"".join(detail)}</details>')
    body.append(f"<script>{MAP_JS}</script>")
    return "".join(body)


def aggregates(values):
    """Mean, min, max and standard deviation of a list; None when empty."""
    if not values:
        return None
    return {"n": len(values), "mean": statistics.fmean(values), "min": min(values), "max": max(values),
            "sd": statistics.stdev(values) if len(values) > 1 else 0.0}


def aggregates_html(agg, label, what="vivants"):
    if not agg:
        return f"<li>{h(label)} : aucun run lu</li>"
    return (f"<li>{h(label)} : <b>{fmt(agg['mean'])}</b> {h(what)} en moyenne sur {agg['n']} run{'s' if agg['n'] > 1 else ''} "
            f"(de {fmt(agg['min'])} à {fmt(agg['max'])}, écart-type {fmt(agg['sd'])})</li>")


def want_aggregates_html(agg, label):
    """The two figures of a series' summary: deaths by want and background deaths, with/without."""
    if not agg:
        return f"<li>{h(label)} : aucun run lu</li>"
    w, b = agg["want"], agg["background"]
    return (f"<li>{h(label)} : <b>{fmt(w['mean'])}</b> morts de manque en moyenne sur {w['n']} run{'s' if w['n'] > 1 else ''} "
            f"(de {fmt(w['min'])} à {fmt(w['max'])}), et {fmt(b['mean'])} morts de fond (de {fmt(b['min'])} à {fmt(b['max'])})</li>")


def series_numbers(s, store):
    """Per run, the key numbers read from its data (None without); the alive aggregates by with/without."""
    rows = []
    for r in s["runs"]:
        d = store.load(s["name"], r["name"])
        sm = summary(d) if d else None
        rows.append((r, d, sm))
    agg = {}
    for trials in (True, False):
        sms = [sm for r, _d, sm in rows if sm and r.get("trials", True) == trials]
        agg[trials] = aggregates([sm["alive"] for sm in sms])
        if agg[trials]:
            agg[trials]["want"] = aggregates([sm["want"] for sm in sms])
            agg[trials]["background"] = aggregates([sm["background"] for sm in sms])
    return rows, agg


def key_line(s, store):
    """One line of key numbers for a series, for the lists."""
    rows, agg = series_numbers(s, store)
    read = sum(1 for _r, d, _sm in rows if d)
    if not read:
        return f"{len(rows)} run{'s' if len(rows) > 1 else ''}, aucune donnée lue encore"
    parts = [f"{read}/{len(rows)} runs lus"]
    for trials, label in ((True, "avec épreuves"), (False, "sans")):
        a = agg[trials]
        if a:
            parts.append(f"{label} : {fmt(a['mean'])} vivants" + (f" (de {fmt(a['min'])} à {fmt(a['max'])})" if a["n"] > 1 else ""))
    want, background, births = {}, {}, 0
    for _r, _d, sm in rows:
        if sm:
            births += sm["births"]
            for c, n in sm["want_by"].items():
                want[c] = want.get(c, 0) + n
            for c, n in sm["background_by"].items():
                background[c] = background.get(c, 0) + n
    for name, by in (("morts de manque", want), ("morts de fond", background)):
        detail = ", ".join(f"{cause_fr(c)} {n}" for c, n in sorted(by.items(), key=lambda x: -x[1]) if n)
        parts.append(f"{name} : {sum(by.values())}" + (f" ({detail})" if detail else ""))
    parts.append(f"{births} naissance{'s' if births > 1 else ''}")
    return " · ".join(parts)


def series_verdict(agg, read, total):
    """One sentence for a series: the mean alive with and without trials, and what it says."""
    if not read:
        return f"Aucun run lu encore sur {total}."
    a, b = agg.get(True), agg.get(False)
    if a and b:
        gap, want_gap = a["mean"] - b["mean"], a["want"]["mean"] - b["want"]["mean"]
        effects = ([f"gardent {more_or_less(gap)}" if gap > 0 else f"coûtent {mean_fr(-gap)} vivant{'s' if -gap >= 2 else ''}"] if abs(gap) >= 0.5 else []) + \
            ([f"{'épargnent' if want_gap < 0 else 'ajoutent'} {want_fr(abs(want_gap))}"] if abs(want_gap) >= 0.5 else [])
        says = "Les épreuves ne changent rien." if not effects else f"Les épreuves {' et '.join(effects)}."
        return (f"Avec les épreuves, {mean_fr(a['mean'])} vivants à la fin et {want_fr(a['want']['mean'])} en moyenne sur {plural(a['n'], 'run')} ; "
                f"sans, {mean_fr(b['mean'])} vivants et {want_fr(b['want']['mean'])} sur {plural(b['n'], 'run')}. {says}")
    one = a or b
    label = "avec les épreuves" if a else "sans les épreuves"
    return f"{mean_fr(one['mean'])} vivants à la fin et {want_fr(one['want']['mean'])} en moyenne sur {plural(one['n'], 'run')} {label}" + \
        (f" (de {fmt(one['min'])} à {fmt(one['max'])} vivants)." if one["n"] > 1 else ".")


def want_fr(x):
    """`2 morts de manque`, `1,5 mort de manque`, `0 mort de manque` for a mean."""
    return f"{mean_fr(x)} mort{'s' if round(x, 1) >= 2 else ''} de manque"


def mean_fr(x):
    """A mean for a sentence: `3` when whole, else one decimal (`2,5`)."""
    r = round(x, 1)
    return fmt(int(r)) if r == int(r) else fmt(r)


def more_or_less(gap):
    """`pareil`, `2 vivants de plus`, `1,5 vivant de moins` for a difference of means."""
    if abs(gap) < 0.5:
        return "pareil"
    return f"{mean_fr(abs(gap))} vivant{'s' if abs(gap) >= 2 else ''} de {'plus' if gap > 0 else 'moins'}"


def compare_verdict(agg_a, agg_b):
    """One sentence for two versions: B against A, with trials then without."""
    parts = []
    for trials, label in ((True, "avec épreuves"), (False, "sans épreuves")):
        a, b = agg_a.get(trials), agg_b.get(trials)
        if a and b:
            want_gap = b["want"]["mean"] - a["want"]["mean"]
            want = "autant de morts de manque" if abs(want_gap) < 0.5 else f"{want_fr(abs(want_gap))} de {'plus' if want_gap > 0 else 'moins'}"
            parts.append(f"{label}, {more_or_less(b['mean'] - a['mean'])} et {want}")
    if not parts:
        return "Rien à comparer encore : aucune paire de runs lue des deux côtés."
    return "La version B contre la version A : " + " ; ".join(parts) + "."


def same_plan(s, others):
    """The other series made from the same plan (same plan name or identical runs), newest first."""
    out = []
    for o in others:
        if o["name"] == s["name"]:
            continue
        if (s.get("plan_name") and o.get("plan_name") == s.get("plan_name")) or \
                (s.get("signature") and o.get("signature") == s.get("signature")):
            out.append(o)
    return out


def serie_body(s, all_series, store):
    rows, agg = series_numbers(s, store)
    head = (f'<h1>Série {h(s["name"])} <span class="etat">{h(s["state"])}</span></h1>'
            f'<p class=muted>{version_html(s.get("version"))}'
            + (f' · plan <code>{h(s["plan_name"])}</code>' if s.get("plan_name") else "")
            + (f' · créée le {h(s["created"].replace("T", " à "))}' if s.get("created") else "")
            + (f' · <a href="{s["synthese"]}">synthèse</a>' if s.get("synthese") else "") + "</p>")
    if s.get("question"):
        head += f'<p class=q>{h(s["question"])}</p>'
    # compare controls
    kin = same_plan(s, all_series)
    if kin:
        opts = "".join(f'<option value="{h(o["name"])}">{h(o["name"])} — {h((o.get("version") or {}).get("commit") or "inconnu")}'
                       f'{" : " + h(o["version"]["change"]) if (o.get("version") or {}).get("change") else ""}</option>' for o in kin)
        versions = (f'<form class=inline method=get action="/compare"><input type=hidden name=a value="{h(s["name"])}">'
                    f'<label>Comparer avec la version… <select name=b>{opts}</select></label> <button class=main>Comparer</button></form>')
    else:
        versions = '<p class=muted>Comparer avec la version… : aucune autre série n’a été faite du même plan. Relancez-le avec <code>lab.py run</code> et une phrase <code>--change</code>.</p>'
    run_opts = "".join(f'<option value="{h(s["name"])}/{h(r["name"])}">{h(r["name"])}</option>' for r in s["runs"])
    runs_cmp = (f'<form class=inline method=get action="/compare"><label>Deux runs de la série : <select name=a>{run_opts}</select></label> '
                f'<label>et <select name=b>{run_opts}</select></label> <button>Comparer</button></form>')
    body = [head, f"<p class=verdict>{h(series_verdict(agg, sum(1 for _r, d, _sm in rows if d), len(rows)))}</p>",
            f"<div class=controls>{versions}{runs_cmp}</div>"]
    # table
    trs = []
    for r, d, sm in rows:
        link = f'<a href="{run_url(s["name"], r["name"])}">{h(r["name"])}</a>'
        if sm:
            cells = (f"<td class=n>{fmt(sm['alive'])}</td><td class=n>{fmt(sm['want'])}</td><td class=n>{fmt(sm['background'])}</td>"
                     f"<td class=n>{fmt(sm['births'])}</td><td class=n>{fmt(sm['kept'])} / {fmt(sm['trials'])}</td>")
        else:
            cells = f'<td colspan=5 class=muted>{NO_DATA if r["state"] not in ("en cours", "attend") else "en cours, année " + fmt(r.get("year"))}</td>'
        trs.append(f"<tr><td>{link}</td><td class=n>{r.get('seed', '')}</td><td>{trials_tag(r)}</td><td><span class=etat>{h(r['state'])}</span></td>"
                   f"{cells}<td class=n>{secs(r.get('elapsed'))}</td><td>"
                   + (f'<a href="{r["page"]}">page</a> ' if r.get("page") else "") + (f'<a href="{r["log"]}">log</a>' if r.get("log") else "") + "</td></tr>")
    body.append('<h2>Les runs</h2><table class=clean><thead><tr><th>run</th><th>graine</th><th>épreuves</th><th>état</th><th class=n>vivants</th><th class=n>morts de manque</th><th class=n>morts de fond</th>'
                f'<th class=n>naissances</th><th class=n>gardés / essais</th><th class=n>temps</th><th></th></tr></thead><tbody>{"".join(trs)}</tbody></table>')
    body.append("<h2>Vivants à la fin</h2><ul>" + aggregates_html(agg[True], "avec épreuves") + aggregates_html(agg[False], "sans épreuves") + "</ul>")
    body.append("<h2>Morts de manque et de fond</h2><ul>" + want_aggregates_html(agg[True], "avec épreuves") + want_aggregates_html(agg[False], "sans épreuves") + "</ul>")
    # overlaid population curves
    chart = None
    for r, d, _sm in rows:
        if not d:
            continue
        c = d.get("curves") or {}
        days, alive = c.get("days") or [], c.get("alive") or []
        if not days:
            continue
        if chart is None:
            chart = Chart(0, max(days[-1], year_days(d)), max(alive or [1]), "années", "vivants (toutes tribus)", year_days(d))
        else:
            chart.ytop = nice_max(max(chart.ytop, max(alive or [1])))
            chart.y = Scale(0, chart.ytop, chart.H - chart.B, chart.T)
            if days[-1] > chart.xhi:
                chart.xhi = days[-1]
                chart.x = Scale(0, chart.xhi, chart.L, chart.W - chart.R)
        chart.line(days, alive, TRIALS_COLOR[r.get("trials", True)], None, width=1.3)
        xs, ys, titles = year_marks(d, None, "alive")
        chart.hover(xs, ys, [f"{r['name']}, {t}" for t in titles], TRIALS_COLOR[r.get("trials", True)])
    if chart:
        chart.legend_item(TRIALS_COLOR[True], "avec épreuves")
        chart.legend_item(TRIALS_COLOR[False], "sans épreuves")
        body.append("<h2>Population de tous les runs</h2>" + chart.svg("population de tous les runs"))
    else:
        body.append(f'<p class="err">{NO_DATA}.</p>')
    return "".join(body)


def lire_body(all_series, store):
    items = []
    for s in all_series:
        items.append(f'<section class=serie><h2><a href="{serie_url(s["name"])}">{h(s["name"])}</a> <span class=etat>{h(s["state"])}</span></h2>'
                     f'<p class=muted>{version_html(s.get("version"))}'
                     + (f' · plan <code>{h(s["plan_name"])}</code>' if s.get("plan_name") else "")
                     + (f' · créée le {h(s["created"].replace("T", " à "))}' if s.get("created") else "") + "</p>"
                     + (f'<p class=q>{h(s["question"])}</p>' if s.get("question") else "")
                     + f'<p>{h(key_line(s, store))}</p></section>')
    if not items:
        items.append('<p class=muted>Aucune série dans le dossier des runs. <a href="/nouvelle">Créer la première</a>.</p>')
    opts = []
    for s in all_series:
        opts.append(f'<optgroup label="{h(s["name"])}"><option value="{h(s["name"])}">la série {h(s["name"])}</option>'
                    + "".join(f'<option value="{h(s["name"])}/{h(r["name"])}">— run {h(r["name"])}</option>' for r in s["runs"]) + "</optgroup>")
    form = ('<form class="inline compare" method=get action="/compare"><label>Comparer <select name=a>' + "".join(opts) + "</select></label> "
            '<label>avec <select name=b>' + "".join(opts) + "</select></label> <button class=main>Comparer</button></form>"
            "<p class=muted>Deux séries du même plan se comparent run par run (même graine, même avec/sans) ; deux runs se comparent face à face. "
            "La page d'une série propose d'abord « Comparer avec la version… ».</p>") if all_series else ""
    return f"<h1>Lire / Comparer</h1>{form}{''.join(items)}"


# --- compare ------------------------------------------------------------------------

def pair_runs(sa, sb):
    """Runs of two series paired by (seed, with/without), then the unpaired."""
    pairs, left = [], list(sb["runs"])
    for ra in sa["runs"]:
        rb = next((r for r in left if r.get("seed") == ra.get("seed") and r.get("trials", True) == ra.get("trials", True)), None)
        if rb is not None:
            left.remove(rb)
        pairs.append((ra, rb))
    return pairs, left


def side_colors(side):
    base = SIDE_COLOR[side]
    return lambda t, i: base if i == 0 else shade(base, 0.35)


def shade(hex_color, amount):
    """A lighter shade of `#rrggbb`, `amount` towards white."""
    r, g, b = (int(hex_color[i:i + 2], 16) for i in (1, 3, 5))
    r, g, b = (int(c + (255 - c) * amount) for c in (r, g, b))
    return f"#{r:02x}{g:02x}{b:02x}"


def compare_runs_body(sa, ra, da, sb, rb, db, title=True):
    """Two runs face to face: cards with the differences, curves overlaid, maps and discoveries side by side."""
    la, lb = f"{sa['name']} / {ra['name']}", f"{sb['name']} / {rb['name']}"
    body = []
    if title:
        body.append(f'<h1>Comparer deux runs</h1>')
    body.append(f'<div class=cols2><div><h3 style="color:{SIDE_COLOR["a"]}">A — <a href="{run_url(sa["name"], ra["name"])}">{h(la)}</a></h3>'
                f'<p class=muted>{version_html(sa.get("version"))}</p></div>'
                f'<div><h3 style="color:{SIDE_COLOR["b"]}">B — <a href="{run_url(sb["name"], rb["name"])}">{h(lb)}</a></h3>'
                f'<p class=muted>{version_html(sb.get("version"))}</p></div></div>')
    if da is None or db is None:
        missing = [n for n, d in ((la, da), (lb, db)) if d is None]
        body.append(f'<p class="err">{NO_DATA} : {h(", ".join(missing))}.</p>')
        return "".join(body)
    sma, smb = summary(da), summary(db)
    body.append(f'<div class=cols2><p class="verdict small"><b style="color:{SIDE_COLOR["a"]}">A.</b> {h(verdict(da, sma))}</p>'
                f'<p class="verdict small"><b style="color:{SIDE_COLOR["b"]}">B.</b> {h(verdict(db, smb))}</p></div>')
    body.append(f'<div class=cols2>{tiles_html(sma, smb, "A")}{tiles_html(smb, sma, "B")}</div>'
                '<p class=muted>À côté de chaque nombre, la flèche dit l’écart à l’autre côté : vert quand c’est mieux, rouge quand c’est pire.</p>')
    body.append("<h2>Courbes superposées</h2><div class=charts>")
    ch = population_chart(da, "A", side_colors("a"))
    population_chart(db, "B", side_colors("b"), dash=True, chart=ch)
    body.append(ch.svg("population"))
    ch = births_deaths_chart(da, "A", slot=(0, 2))
    births_deaths_chart(db, "B", chart=ch, slot=(1, 2))
    body.append(ch.svg("naissances et morts par an (A à gauche de l’année, B à droite)"))
    ch = kept_chart(da, "A", side_colors("a"), slot=(0, 2))
    kept_chart(db, "B", side_colors("b"), chart=ch, slot=(1, 2))
    body.append(ch.svg("essais gardés par an"))
    ch = reserve_chart(da, "A", side_colors("a"))
    reserve_chart(db, "B", side_colors("b"), dash=True, chart=ch)
    body.append(ch.svg("réserve moyenne"))
    body.append("</div><p class=muted>A en trait plein et B en pointillé ; dans les barres, A à gauche de chaque année et B à droite.</p>")
    body.append(f'<h2>Les cartes</h2><div class=cols2><div>{map_svg(da, "ma", "(A)")}</div><div>{map_svg(db, "mb", "(B)")}</div></div>')
    body.append("<h2>Les découvertes</h2>")
    ta, tb = tribes(da), tribes(db)
    for i in range(max(len(ta), len(tb))):
        left = story_html(da, ta[i]) + found_table(da, ta[i]) if i < len(ta) else '<p class=muted>pas de tribu</p>'
        right = story_html(db, tb[i]) + found_table(db, tb[i]) if i < len(tb) else '<p class=muted>pas de tribu</p>'
        body.append(f'<div class=cols2><section class=tribe>{left}</section><section class=tribe>{right}</section></div>')
    body.append(f"<script>{MAP_JS}</script>")
    return "".join(body)


def compare_series_body(sa, sb, store):
    """Two series run by run: the key numbers with the differences, the overlaid population curves,
    the cultures side by side, and a link to each pair's face-to-face page."""
    pairs, left = pair_runs(sa, sb)
    body = [f'<h1>Comparer deux versions</h1>'
            f'<div class=cols2><div><h3 style="color:{SIDE_COLOR["a"]}">A — <a href="{serie_url(sa["name"])}">{h(sa["name"])}</a></h3>'
            f'<p class=muted>{version_html(sa.get("version"))}</p>'
            + (f'<p class=q>{h(sa["question"])}</p>' if sa.get("question") else "") + "</div>"
            f'<div><h3 style="color:{SIDE_COLOR["b"]}">B — <a href="{serie_url(sb["name"])}">{h(sb["name"])}</a></h3>'
            f'<p class=muted>{version_html(sb.get("version"))}</p>'
            + (f'<p class=q>{h(sb["question"])}</p>' if sb.get("question") else "") + "</div></div>"]
    if sa.get("plan_name") and sa.get("plan_name") == sb.get("plan_name"):
        body.append(f'<p>Même plan <code>{h(sa["plan_name"])}</code> : les runs se comparent graine par graine, avec contre avec, sans contre sans.</p>')
    elif sa.get("signature") and sa.get("signature") == sb.get("signature"):
        body.append("<p>Mêmes runs une fois le plan déplié : comparaison run par run.</p>")
    else:
        body.append('<p class="warn">Les deux séries ne viennent pas du même plan : les runs sont appariés par graine et avec/sans quand c’est possible.</p>')

    def delta(a, b):
        if a is None or b is None:
            return ""
        dv = b - a
        return f'<span class="delta {"plus" if dv > 0 else "minus"}">{dv:+d}</span>' if dv else '<span class="delta zero">=</span>'

    rows, chart = [], None
    cultures = []
    for ra, rb in pairs:
        da = store.load(sa["name"], ra["name"])
        db = store.load(sb["name"], rb["name"]) if rb else None
        sma, smb = (summary(da) if da else None), (summary(db) if db else None)

        def cell(key, deaths=None):
            va = (sma["deaths"].get(deaths, 0) if deaths else sma[key]) if sma else None
            vb = (smb["deaths"].get(deaths, 0) if deaths else smb[key]) if smb else None
            return f"<td class=n>{fmt(va)} → {fmt(vb)} {delta(va, vb)}</td>"

        cs = []
        for d in (da, db):
            if d:
                for c in causes(d):
                    if c not in cs:
                        cs.append(c)
        cause_cells = "".join(cell("deaths", c) for c in cs) if cs else ""
        link = f'<a href="{compare_url(f"{sa["name"]}/{ra["name"]}", f"{sb["name"]}/{rb["name"]}")}">face à face</a>' if rb else '<span class=muted>sans pair en B</span>'
        rows.append((cs, f"<tr><td>{h(ra['name'])}</td><td class=n>{ra.get('seed', '')}</td><td>{trials_tag(ra)}</td>"
                     + (cell("alive") + cell("want") + cell("background") + cell("births") + cell("kept") + cause_cells if sma or smb
                        else f'<td colspan={6 + len(cs)} class=muted>{NO_DATA}</td>')
                     + f"<td>{link}</td></tr>"))
        for d, trials_on, color in ((da, ra.get("trials", True), SIDE_COLOR["a"]), (db, (rb or {}).get("trials", True), SIDE_COLOR["b"])):
            if not d:
                continue
            c = d.get("curves") or {}
            days, alive = c.get("days") or [], c.get("alive") or []
            if not days:
                continue
            if chart is None:
                chart = Chart(0, max(days[-1], year_days(d)), max(alive or [1]), "années", "vivants (toutes tribus)", year_days(d))
            else:
                chart.ytop = nice_max(max(chart.ytop, max(alive or [1])))
                chart.y = Scale(0, chart.ytop, chart.H - chart.B, chart.T)
                if days[-1] > chart.xhi:
                    chart.xhi = days[-1]
                    chart.x = Scale(0, chart.xhi, chart.L, chart.W - chart.R)
            chart.line(days, alive, color, None, dash=not trials_on, width=1.3)
        if da or db:
            ta = tribes(da) if da else []
            tb = tribes(db) if db else []
            for i in range(max(len(ta), len(tb))):
                la = f"tribu {ta[i]} : {culture_line(da, ta[i])}" if i < len(ta) else "-"
                lb = f"tribu {tb[i]} : {culture_line(db, tb[i])}" if i < len(tb) else "-"
                cultures.append(f"<tr><td>{h(ra['name'])}</td><td>{h(la)}</td><td>{h(lb)}</td></tr>")
    all_causes = []
    for cs, _ in rows:
        for c in cs:
            if c not in all_causes:
                all_causes.append(c)
    head = ("<tr><th>run</th><th>graine</th><th>épreuves</th><th>vivants A → B</th><th>morts de manque</th><th>morts de fond</th><th>naissances</th><th>gardés</th>"
            + "".join(f"<th>{h(cause_fr(c))}</th>" for c in all_causes) + "<th></th></tr>")
    body.append(f'<h2>Run par run</h2><div class=scroll><table class=clean><thead>{head}</thead><tbody>{"".join(r for _, r in rows)}</tbody></table></div>')
    if left:
        body.append("<p class=muted>Sans pair en A : " + ", ".join(h(r["name"]) for r in left) + ".</p>")
    agg_a = series_numbers(sa, store)[1]
    agg_b = series_numbers(sb, store)[1]
    body.insert(1, f"<p class=verdict>{h(compare_verdict(agg_a, agg_b))}</p>")
    body.append("<h2>Vivants à la fin</h2><div class=cols2><div><h3>A</h3><ul>" + aggregates_html(agg_a[True], "avec épreuves") + aggregates_html(agg_a[False], "sans épreuves")
                + "</ul></div><div><h3>B</h3><ul>" + aggregates_html(agg_b[True], "avec épreuves") + aggregates_html(agg_b[False], "sans épreuves") + "</ul></div></div>")
    body.append("<h2>Morts de manque et de fond</h2><div class=cols2><div><h3>A</h3><ul>" + want_aggregates_html(agg_a[True], "avec épreuves") + want_aggregates_html(agg_a[False], "sans épreuves")
                + "</ul></div><div><h3>B</h3><ul>" + want_aggregates_html(agg_b[True], "avec épreuves") + want_aggregates_html(agg_b[False], "sans épreuves") + "</ul></div></div>")
    if chart:
        chart.legend_item(SIDE_COLOR["a"], "A, avec épreuves")
        chart.legend_item(SIDE_COLOR["a"], "A, sans", dash=True)
        chart.legend_item(SIDE_COLOR["b"], "B, avec épreuves")
        chart.legend_item(SIDE_COLOR["b"], "B, sans", dash=True)
        body.append("<h2>Population de tous les runs</h2>" + chart.svg("population de tous les runs"))
    if cultures:
        body.append("<h2>Les cultures à la fin</h2><table class=clean><thead><tr><th>run</th><th>A</th><th>B</th></tr></thead><tbody>" + "".join(cultures) + "</tbody></table>")
    return "".join(body)
