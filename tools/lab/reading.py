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
import re
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
TRIBE_COLOR = {"A": "#2a78d6", "B": "#eb6834"}   # blue and orange, the lab 5 page's --tA and --tB
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
        "dispelled_years": sum((y.get("fire") or {}).get("dispelled") or 0 for y in d.get("years", [])) if has_fire_data(d) else None,
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


def knowledge_body(d):
    """« Ce que la tribu sait » for the run page: the line, the two charts, then the years as a short table."""
    out = knowledge_html(d)
    out = out[out.index("</p>", out.index("<p class=caption>")) + 4:]
    out = out[:out.index('<table class="clean">')]
    rows = []
    for y in years_of(d):
        fi, k = y.get("fire") or {}, y.get("knowledge") or {}
        heard = f"{fmt_thin(k.get('accesses_heard'))} / {fmt_thin(k.get('accesses'))}" if k.get("accesses") is not None else None
        rows.append([(fmt_thin(y.get("year")), y.get("year")), (tribe_chip(y.get("tribe")), y.get("tribe")),
                     (fmt_thin(k.get("places")), k.get("places")), (fmt_thin(k.get("heard_only")), k.get("heard_only")),
                     (fmt_thin(fi.get("dispelled")), fi.get("dispelled")), (heard, k.get("accesses_heard")),
                     (fmt_thin(fi.get("nights")), fi.get("nights")), (fmt_thin(fi.get("told")), fi.get("told")), (fmt_thin(fi.get("heard")), fi.get("heard"))])
    return out + data_table("t-savoir", [("An", True), ("Tribu", False), ("Lieux connus", True), ("Par ouï-dire seul", True),
                                         ("Rumeurs dissipées", True), ("Souvenirs entendus / tenus", True, True), ("Nuits au feu", True, True),
                                         ("Dits", True, True), ("Entendus", True, True)], rows, "Aucune année.")


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


def dared_body(d):
    """« Qui a osé quoi » for the run page: who tried most first, as a short table; then the trials by band of will."""
    people = sorted((p for p in d.get("people", []) if p.get("trials")), key=lambda p: (-len(p.get("trials") or []), p.get("id", 0)))
    rows = []
    for p in people:
        trials = p.get("trials") or []
        kept = sum(1 for t in trials if t.get("kept"))
        heard = sum(1 for t in trials if t.get("heard"))
        will = f"{fmt(p.get('will_start'))} → {fmt(p.get('will'))}" if p.get("will_start") is not None else fmt(p.get("will"))
        what = ", ".join(kind_fr(t.get("step", "?")) for t in trials if t.get("kept"))
        rows.append([(person_link(p.get("id")), p.get("id")), (tribe_chip(p.get("tribe")), p.get("tribe")), (fmt_thin(len(trials)), len(trials)),
                     (fmt_thin(kept), kept), (h(will), p.get("will")), (fmt_thin(heard), heard), (h(what) or None, what)])
    out = dared_html(d)
    bands = out.index("</table>") + len("</table>")
    return ("<p class=caption>Cliquez un titre de colonne pour trier, un nom pour la fiche ; la volonté monte d'un cran à un essai gardé, baisse à un abandon.</p>"
            + data_table("t-ose", [("Qui", False), ("Tribu", False), ("Essais", True), ("Gardés", True), ("Volonté, début → fin", True),
                                   ("Dont entendus au feu", True, True), ("Ce qu'il garde", False, True)], rows, "Personne n'a rien essayé.")
            + out[bands:])


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


# --- the run page laid out (lab 5): groups, tribes, tables, chronicle, map, person card ----
# One colour, one meaning, on every new part of the page (the CSS tokens of lab.py):
# red the deaths by want, grey the background deaths, green what was kept, amber the
# rumours, magenta the births, violet the fire, teal the homes; blue tribe A, orange
# tribe B. A colour never travels alone: a glyph, a letter or a word goes with it.

RUN_CSS = """
main:has(.runpage){max-width:1320px}
.runpage{display:grid;grid-template-columns:11em minmax(0,1fr);gap:2.2em;align-items:start}
.toc{position:sticky;top:1em;font-size:.9em;border-left:2px solid var(--line);padding:.2em 0 .2em .9em}
.toc b{display:block;color:var(--muted);font-size:.8em;text-transform:uppercase;letter-spacing:.06em;margin-bottom:.4em}
.toc a{display:block;color:var(--ink2);text-decoration:none;padding:.16em 0}.toc a:hover{color:var(--ink)}
.toc a.sub{padding-left:.9em;font-size:.92em;color:var(--muted)}.toc a.on{color:var(--ink);font-weight:650}
.runpage section{scroll-margin-top:1em}
section.group{margin:2.6em 0 0;padding-top:1.2em;border-top:2px solid var(--line)}
section.group>h2{margin:0 0 .3em;font-size:1.45em}
.trend{font-weight:600;margin:.2em 0 1em;max-width:48em;line-height:1.45}
.hero{font-size:1.75em;line-height:1.3;font-weight:650;margin:.6em 0 .35em;max-width:30em;text-wrap:balance}
.trends{max-width:48em;margin:0 0 1.1em;padding:0;list-style:none}
.trends li{margin:.25em 0;padding-left:1.1em;position:relative;line-height:1.45}.trends li::before{content:"→";position:absolute;left:0;color:var(--muted)}
ul.points{list-style:none;margin:.2em 0 1em;padding:0;display:grid;grid-template-columns:repeat(auto-fit,minmax(16em,1fr));gap:.35em 1.2em;max-width:60em}
ul.points li{line-height:1.4;padding:.1em 0 .1em .8em;border-left:3px solid var(--line)}
ul.points li b{font-weight:700}ul.points.top{font-size:1.08em;margin:.4em 0 1.1em}ul.points.top li{border-left-color:var(--ink2)}
.spark{display:inline-flex;align-items:center;gap:.15em;margin-left:.4em;color:var(--ink2);vertical-align:-.15em}
.spark svg{width:3.6em;height:1em}.spark .ar{font-size:.9em}.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}
svg.chart{background:var(--surface)}svg.chart text{fill:var(--ink2)}
svg.chart [stroke="#ddd"],svg.chart [stroke="#eee"]{stroke:var(--line)}svg.chart [stroke="#888"]{stroke:var(--muted)}
svg.chart [fill="#dde6ee"]{fill:var(--winter)}svg.chart [stroke="#fff"]{stroke:var(--surface)}
.mapbox,.map2{background:var(--surface)}
.hero-sub{color:var(--muted);max-width:48em;margin:0 0 1em;font-size:.9em}
.tiles6{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:.8em;margin:.6em 0 .6em}
.t6{background:var(--surface);border-radius:12px;padding:.8em 1em .85em;box-shadow:var(--shadow);border-top:4px solid var(--c,var(--line))}
.t6 .k{color:var(--ink2);font-size:.9em;display:flex;gap:.4em;align-items:center}
.t6 .v{font-size:2.1em;font-weight:700;line-height:1.15;font-variant-numeric:tabular-nums;margin:.1em 0 .1em}
.t6 .ab{display:flex;gap:1em;font-variant-numeric:tabular-nums;font-size:.95em;margin:.1em 0 .3em;color:var(--ink)}
.t6 .s{color:var(--muted);font-size:.83em;line-height:1.4}.t6 .dl{margin:.1em 0 .35em}
.t6 .d{display:inline-block;font-size:.8em;font-weight:600;padding:.05em .55em;border-radius:1em}
.t6 .d.good{background:var(--good-bg);color:var(--good-ink)}.t6 .d.bad{background:var(--bad-bg);color:var(--bad-ink)}
.t6 .d.same{background:var(--chip);color:var(--ink2)}
.g{font-style:normal;display:inline-block;width:1.1em;text-align:center;line-height:1}
.c-want{color:var(--want)}.c-bg{color:var(--bg-death)}.c-kept{color:var(--kept)}.c-rumour{color:var(--rumour)}
.c-birth{color:var(--birth)}.c-fire{color:var(--fire)}.c-camp{color:var(--camp)}.c-try{color:var(--try)}
.tr{display:inline-flex;align-items:center;justify-content:center;min-width:1.35em;height:1.35em;border-radius:.35em;font-size:.78em;
font-weight:700;color:#fff;background:var(--tc,var(--muted));margin-right:.3em;vertical-align:.08em}
.tr.tA{--tc:var(--tA)}.tr.tB{--tc:var(--tB)}
a.who{color:inherit;text-decoration:underline;text-decoration-color:var(--line);text-underline-offset:.2em;cursor:pointer}
a.who:hover{text-decoration-color:var(--ink)}
.tablebox{position:relative}
table.data{width:100%;border-collapse:separate;border-spacing:0}
table.data th{position:sticky;top:0;z-index:1;padding:0;white-space:nowrap}
table.data th button{all:unset;cursor:pointer;display:block;width:100%;box-sizing:border-box;padding:.65em .9em .5em;color:var(--muted)}
table.data th button:hover{color:var(--ink)}table.data th.n button{text-align:right}
table.data th button::after{content:" ↕";opacity:.35}
table.data th[aria-sort=ascending] button::after{content:" ↑";opacity:1}table.data th[aria-sort=descending] button::after{content:" ↓";opacity:1}
table.data td{border-bottom:1px solid var(--line)}
table.data .xc{display:none}table.data.all .xc{display:table-cell}
table.data.short tbody tr:nth-child(n+11){display:none}
.none{color:var(--muted)}
.cbar{display:inline-block;width:3.2em;height:.42em;border-radius:3px;background:var(--zebra);margin-right:.6em;vertical-align:middle;overflow:hidden}
tbody tr:nth-child(even) .cbar{background:var(--surface)}
.cbar span{display:block;height:100%;border-radius:3px;background:var(--c)}
.more{margin:.6em 0 0;display:flex;gap:.6em;flex-wrap:wrap}
.chron-years{display:flex;flex-wrap:wrap;gap:.3em;margin:.2em 0 .8em;font-size:.9em}
.chron-years button{padding:.1em .55em;border-radius:1em;border:0;color:var(--ink2);background:var(--chip);font-size:1em}
.chron-years button[aria-current=true]{background:var(--ink);color:var(--page)}
.filters{display:flex;flex-wrap:wrap;gap:.5em .8em;align-items:center;background:var(--surface);border-radius:12px;padding:.7em 1em;box-shadow:var(--shadow);margin:0 0 1em;position:sticky;top:0;z-index:2}
.chips{display:flex;flex-wrap:wrap;gap:.35em}
.chips button{font-size:.88em;padding:.2em .75em;border-radius:1em;border:1px solid var(--line);background:var(--surface);color:var(--ink2)}
.chips button[aria-pressed=true]{background:var(--ink);color:var(--page);border-color:var(--ink)}
.filters select,.filters input{font-size:.9em;padding:.25em .5em;border:1px solid var(--line);border-radius:8px;background:var(--surface);color:var(--ink)}
.filters input{width:14em}.filters .count{color:var(--muted);font-size:.85em;margin-left:auto}
.season{margin:0 0 1.2em}
.season>h4{margin:0 0 .5em;font-size:1em;display:flex;gap:.6em;align-items:baseline}
.season>h4 small{color:var(--muted);font-weight:400}
ol.days{list-style:none;margin:0;padding:0 0 0 .2em}
li.day{display:grid;grid-template-columns:4.2em minmax(0,1fr);gap:.6em;padding:.15em 0}
li.day>.dd{color:var(--muted);font-size:.85em;text-align:right;padding-top:.15em;font-variant-numeric:tabular-nums}
ul.evs{list-style:none;margin:0;padding:0 0 .35em 0}
li.ev{padding:.12em 0 .12em .7em;line-height:1.45;border-left:3px solid var(--line)}
li.ev.tA{border-left-color:var(--tA)}li.ev.tB{border-left-color:var(--tB)}
li.ev .lab{font-weight:650;margin-right:.35em}
li.ev .x{color:var(--muted);font-size:.9em}
li.grp>details>summary{list-style:none;cursor:pointer;font-weight:400;color:var(--ink)}
li.grp>details>summary::-webkit-details-marker{display:none}
li.grp>details>summary::after{content:" ▸ voir";color:var(--link);font-size:.85em}
li.grp>details[open]>summary::after{content:" ▾ replier"}
li.grp ul{list-style:none;margin:.2em 0 .3em .6em;padding:0;font-size:.92em;max-height:22em;overflow:auto}
.chron-empty{color:var(--muted)}
.map2{display:grid;grid-template-columns:minmax(0,1fr) 17em;gap:1em;align-items:start;background:var(--surface);border-radius:12px;padding:1em;box-shadow:var(--shadow)}
.map2 svg.map{width:100%;height:auto;display:block;border-radius:6px;background:#78a046}
.map2 .bar{display:flex;flex-wrap:wrap;gap:.5em .9em;align-items:center;margin:.7em 0 .2em}
.map2 .bar input[type=range]{flex:1;min-width:10em}.map2 .yr{font-weight:700;min-width:4.5em}
.map2 .seg{display:inline-flex;border:1px solid var(--line);border-radius:8px;overflow:hidden}
.map2 .seg button{border:0;border-radius:0;padding:.2em .7em;font-size:.88em}.map2 .seg button[aria-pressed=true]{background:var(--ink);color:var(--page)}
.map2 .lay{display:flex;flex-direction:column;gap:.25em;font-size:.9em;margin:.2em 0 .8em}
.mlegend{font-size:.88em;line-height:1.35}.mlegend h4{margin:0 0 .4em;font-size:.95em}
.mlegend ul{list-style:none;margin:0 0 .8em;padding:0}.mlegend li{display:grid;grid-template-columns:1.9em 1fr;gap:.4em;align-items:center;margin:.3em 0}
.mlegend svg{width:1.7em;height:1.7em}
.mlegend .stays{color:var(--ink2);font-size:.95em}
.mapnote{color:var(--muted);font-size:.85em;margin:.3em 0 0}
dialog.fiche{border:0;border-radius:14px;padding:0;max-width:min(46em,94vw);width:46em;background:var(--surface);color:var(--ink);box-shadow:0 10px 40px rgba(0,0,0,.35)}
dialog.fiche::backdrop{background:rgba(0,0,0,.35)}
dialog.fiche .in{padding:1.1em 1.3em 1.3em;max-height:84vh;overflow:auto}
dialog.fiche h3{margin:0 0 .2em;font-size:1.3em;display:flex;align-items:center;gap:.4em}
dialog.fiche h4{margin:1em 0 .3em;font-size:.95em}
dialog.fiche .close{position:sticky;top:0;float:right;margin:-.2em -.3em 0 .5em}
dialog.fiche ul{margin:.2em 0;padding-left:1.2em}dialog.fiche li{margin:.15em 0}
svg.frise{width:100%;height:auto;display:block;margin:.6em 0 .2em}
@media (max-width:980px){.map2{grid-template-columns:minmax(0,1fr)}}
@media (max-width:860px){.runpage{grid-template-columns:minmax(0,1fr);gap:.4em}main:has(.runpage){padding-left:1em;padding-right:1em}
.toc{position:static;border-left:0;padding:0;display:flex;flex-wrap:wrap;gap:.3em}
.toc b{width:100%;margin:0}.toc a,.toc a.sub{padding:.1em .6em;background:var(--chip);border-radius:1em;font-size:.9em}
.hero{font-size:1.35em}.filters{position:static}.tablebox{overflow-x:auto}
li.day{grid-template-columns:3.2em minmax(0,1fr)}}
@media (max-width:560px){.tiles6{grid-template-columns:repeat(2,minmax(0,1fr))}}
"""

THIN = " "   # the thin no-break space between thousands, in the new tables
DASH = '<span class=none title="rien">—</span>'
TRIBE_SIDE = ["de l’est", "du sud-est", "du sud", "du sud-ouest", "de l’ouest", "du nord-ouest", "du nord", "du nord-est"]


def fmt_thin(n):
    """`12 345` with a thin space; the grey dash for None."""
    return DASH if n is None else h(fmt(n)).replace(" ", THIN)


def cell(text):
    """A cell's text, never empty: the helpers' `-` and the empty string become the grey dash."""
    return DASH if text in (None, "", "-") else text


def tribe_side(d, t):
    """`du sud-est`: where a tribe's first camp lies on the map, from its centre; '' when unknown."""
    ts = tribes(d)
    camps = (d.get("meta") or {}).get("camps") or []
    size = (d.get("meta") or {}).get("map_tiles") or (d.get("map") or {}).get("tiles")
    i = ts.index(t) if t in ts else -1
    if i < 0 or i >= len(camps) or not size:
        return ""
    dx, dy = camps[i][0] - size[0] / 2, camps[i][1] - size[1] / 2
    if math.hypot(dx, dy) < min(size) / 10:
        return "du centre"
    return TRIBE_SIDE[int(((math.degrees(math.atan2(dy, dx)) + 360 + 22.5) % 360) // 45)]


def tribe_chip(t):
    """The tribe's letter on its colour: the tribe's sign everywhere on the page."""
    return f'<span class="tr t{h(t or "")}" title="tribu {h(t or "?")}">{h(t or "?")}</span>'


def person_link(pid, label=None):
    """A person's name that opens its card."""
    if pid is None:
        return DASH
    return f'<a class=who data-person="{int(pid)}" href="#fiche-{int(pid)}">{h(label or person_fr(pid))}</a>'


def link_people(text_html):
    """Every `personne N` of an escaped text made a link to its card."""
    return re.sub(r"personne (\d+)", lambda m: person_link(int(m.group(1))), text_html)


def control_of(s, r):
    """The control run of `r` in its series: the same base and seed without the fire (with the fire
    on for `r`), else without the trials; None when `r` is itself a control or has none."""
    def same(o):
        return o.get("name") != r.get("name") and o.get("seed") == r.get("seed") \
            and (o.get("base") == r.get("base") or r.get("base") is None)
    runs = s.get("runs") or []
    if r.get("fire", True) and r.get("trials", True):
        c = next((o for o in runs if same(o) and o.get("fire", True) is False and o.get("trials", True)), None)
        if c is not None:
            return c, "sans le feu"
    if r.get("trials", True):
        c = next((o for o in runs if same(o) and o.get("trials", True) is False and o.get("fire", True) == r.get("fire", True)), None)
        if c is not None:
            return c, "sans les épreuves"
    return None, None


def by_tribe(d):
    """The numbers of the top row, tribe by tribe: alive, deaths, births, food carried, trials, rumours."""
    out = {}
    last = last_year(d)
    for t in tribes(d):
        people = [p for p in d.get("people", []) if p.get("tribe") == t]
        ys = years_of(d, t)
        found = next((f for f in d.get("found", []) if f.get("tribe") == t), {})
        end = next((y for y in ys if y.get("year") == last), {})
        out[t] = {"alive": sum(1 for p in people if not p.get("died")), "start": sum(1 for p in people if p.get("born") == "start"),
                  "dead": sum(1 for p in people if p.get("died")),
                  "want": sum(1 for p in people if p.get("died") and is_want(p["died"].get("cause"))),
                  "births": sum(1 for p in people if p.get("born") != "start"),
                  "food": (end.get("food") or {}).get("end_kj"),
                  "trials": sum((found.get("tried") or {}).values()), "kept": sum((found.get("kept") or {}).values()),
                  "copied": sum((found.get("copied") or {}).values()),
                  "rumours": sum((y.get("fire") or {}).get("dispelled") or 0 for y in ys) if has_fire_data(d) else None,
                  "noon": [((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean") for y in ys],
                  "places": ((end.get("knowledge") or {}).get("places"))}
    return out


def headline(d, sm, smc=None, label=None):
    """The verdict in one sentence, large: alive after so long, against the control when there is one."""
    when = f"Après {plural(sm['years'], 'an')}" if sm["years"] else f"Après {plural(run_days(d), 'jour')}"
    alive = sm["alive"]
    who = "plus personne" if alive == 0 else f"{plural(alive, 'vivant')}"
    if smc is None:
        return f"{when}, {who} sur les {fmt(sm['start'])} du départ et les {plural(sm['births'], 'enfant')} nés en route."
    gap = alive - smc["alive"]
    if gap == 0:
        return f"{when}, {who} : autant que {label}."
    return f"{when}, {who} : {fmt(abs(gap))} de {'plus' if gap > 0 else 'moins'} que {label} ({fmt(smc['alive'])})."


def mean_of(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else None


def pct(a, b):
    """`+41 %` from b to a; None when b is nothing."""
    if a is None or not b:
        return None
    return round(100 * (a - b) / b)


def tile6(title, value, sub, color=None, glyph="", ctrl=None, good_up=None, label=None, text=None, split=None):
    """One tile of the top row: the total, its name, A | B, the gap to the control, a line under it."""
    gap = ""
    if ctrl is not None and value is not None:
        dv = value - ctrl
        if dv == 0:
            gap = f'<span class="d same" title="{h(label)} : {fmt(ctrl)}">= témoin</span>'
        else:
            cls = "same" if good_up is None else ("good" if (dv > 0) == good_up else "bad")
            gap = (f'<span class="d {cls}" title="{h(label)} : {fmt(ctrl)}">{"↑" if dv > 0 else "↓"} '
                   f'{"+" if dv > 0 else "−"}{fmt(abs(dv)).replace(" ", THIN)} · témoin {fmt(ctrl).replace(" ", THIN)}</span>')
    g = f'<i class="g {color}" aria-hidden=true>{glyph}</i>' if glyph else ""
    style = f' style="--c:var(--{TILE_VAR[color]})"' if color else ""
    shown = h(text) if text else fmt_thin(value)
    ab = ""
    if split and len(split) > 1:
        ab = '<div class=ab>' + "".join(f"<span>{tribe_chip(t)}{fmt_thin(v)}</span>" for t, v in split.items()) + "</div>"
    return (f'<div class=t6{style}><div class=k>{g}{h(title)}</div><div class=v>{shown}</div>{ab}'
            f'{f"<div class=dl>{gap}</div>" if gap else ""}<div class=s>{sub}</div></div>')


TILE_VAR = {"c-want": "want", "c-bg": "bg-death", "c-kept": "kept", "c-rumour": "rumour", "c-birth": "birth", "c-fire": "fire", "c-camp": "camp"}


def run_tiles(d, sm, bt, smc=None, label=None):
    """Six tiles: alive, deaths, births, food carried, trials, rumours; each total, then A | B, then the
    arrow against the control."""
    c = smc or {}
    st, cst = sm.get("stock"), c.get("stock")
    sp = lambda key: {t: v[key] for t, v in bt.items()}  # noqa: E731
    rum = sum(v["rumours"] or 0 for v in bt.values()) if has_fire_data(d) else None
    tiles = [
        tile6("vivants à la fin", sm["alive"], f"{fmt(sm['start'])} au départ", None, "", c.get("alive"), True, label, split=sp("alive")),
        tile6("morts", sm["dead"], f'<i class="g c-want">■</i>{fmt(sm["want"])} de manque · <i class="g c-bg">■</i>{fmt(sm["background"])} de fond',
              "c-want", "■", c.get("dead"), False, label, split=sp("dead")),
        tile6("naissances", sm["births"], f"{fmt(sm['alive'] - sm['alive_start'])} encore vivants" if sm["births"] else "aucune",
              "c-birth", "●", c.get("births"), True, label, split=sp("births")),
        tile6("nourriture portée à la fin (kJ)", st["kj"] if st else None,
              (f"≈ {plural(round(st['days_per_person']), 'jour')} de vivres par vivant" if st and st["days_per_person"] is not None else "")
              if st else "pas dans les données", None, "", cst["kj"] if cst else None, True, label, None if st else NO_STOCK, split=sp("food")),
        tile6("essais", sm["trials"], f'<i class="g c-kept">◆</i>{fmt(sm["kept"])} gardés · {plural(sm["copied"], "copie")}',
              "c-kept", "◆", c.get("trials"), None, label, split=sp("trials")),
        tile6("rumeurs dissipées", rum, "un lieu entendu au feu, trouvé nu", "c-rumour", "●",
              c.get("rumours_all") if smc else None, None, label, split=sp("rumours") if rum is not None else None),
    ]
    return f"<div class=tiles6>{''.join(tiles)}</div>"


TABLE_JS = """
function initTables(root) {
root.querySelectorAll('table.data:not([data-bound])').forEach(t => {
  t.dataset.bound = 1;
  const body = t.tBodies[0], ths = [...t.tHead.rows[0].cells];
  ths.forEach((th, i) => th.querySelector('button').addEventListener('click', () => {
    const up = th.getAttribute('aria-sort') !== 'ascending';
    ths.forEach(o => o.removeAttribute('aria-sort'));
    th.setAttribute('aria-sort', up ? 'ascending' : 'descending');
    const key = r => r.cells[i].dataset.k ?? r.cells[i].textContent;
    const num = [...body.rows].every(r => key(r) === '' || !isNaN(+key(r)));
    const rows = [...body.rows].sort((a, b) => {
      const x = key(a), y = key(b);
      const c = num ? (x === '' ? 1e18 : +x) - (y === '' ? 1e18 : +y) : x.localeCompare(y, 'fr');
      return up ? c : -c;
    });
    rows.forEach(r => body.appendChild(r));
  }));
});
root.querySelectorAll('[data-more]:not([data-bound])').forEach(b => { b.dataset.bound = 1; b.addEventListener('click', () => {
  const t = document.getElementById(b.dataset.more), short = t.classList.toggle('short');
  b.textContent = short ? b.dataset.all : 'n’en voir que 10';
}); });
root.querySelectorAll('[data-cols]:not([data-bound])').forEach(b => { b.dataset.bound = 1; b.addEventListener('click', () => {
  const all = document.getElementById(b.dataset.cols).classList.toggle('all');
  b.textContent = all ? 'moins de colonnes' : 'plus de colonnes';
}); });
}
initTables(document);
"""

LAZY_JS = """
(() => {
  const page = document.querySelector('.runpage');
  if (!page) return;
  async function load(box) {
    if (box.dataset.loaded) return; box.dataset.loaded = 1;
    try {
      const r = await fetch(page.dataset.morceau + '?quoi=' + box.dataset.lazy);
      if (!r.ok) throw new Error(r.status);
      box.innerHTML = await r.text();
      box.querySelectorAll('[data-map2]').forEach(initMap2);
      initTables(box);
    } catch (e) { box.dataset.loaded = ''; box.innerHTML = '<p class=muted>ce morceau n’a pas pu être lu (' + e.message + ')</p>'; }
  }
  document.querySelectorAll('[data-lazy]').forEach(box => {
    const d = box.closest('details');
    if (d) d.addEventListener('toggle', () => { if (d.open) load(box); });
    else load(box);
  });
})();
"""


def data_table(uid, head, rows, empty, limit=10):
    """A sortable table: `head` of (title, numeric[, extra]); `rows` of cells (html, sort key); the extra
    columns behind « plus de colonnes »; 10 rows then « voir les N »."""
    if not rows:
        return f'<p class="muted">{h(empty)}</p>'
    head = [(x[0], x[1], x[2] if len(x) > 2 else False) for x in head]

    def cls(num, extra):
        c = " ".join(k for k, on in (("n", num), ("xc", extra)) if on)
        return f' class="{c}"' if c else ""
    ths = "".join(f'<th{cls(num, extra)} scope=col><button type=button>{h(t)}</button></th>' for t, num, extra in head)
    trs = []
    for r in rows:
        tds = "".join(f'<td{cls(num, extra)}{"" if k is None else f" data-k={chr(34)}{h(str(k))}{chr(34)}"}>{cell(v)}</td>'
                      for (v, k), (_, num, extra) in zip(r, head))
        trs.append(f"<tr>{tds}</tr>")
    short = len(rows) > limit
    buttons = []
    if short:
        buttons.append(f'<button type=button data-more="{uid}" data-all="voir les {len(rows)}">voir les {len(rows)}</button>')
    if any(extra for _, _, extra in head):
        buttons.append(f'<button type=button data-cols="{uid}">plus de colonnes</button>')
    more = f'<p class=more>{"".join(buttons)}</p>' if buttons else ""
    return (f'<div class=tablebox><table class="data{" short" if short else ""}" id="{uid}"><thead><tr>{ths}</tr></thead>'
            f'<tbody>{"".join(trs)}</tbody></table></div>{more}')


def death_mark(cause):
    """The glyph of a death: red for a death by want, grey for a background death; the word goes beside."""
    return f'<i class="g {"c-want" if is_want(cause) else "c-bg"}" aria-hidden=true>■</i>'


def when_short(d, day):
    """`an 3, été` for a day of the run."""
    if day is None:
        return None
    return f"an {(int(day) - 1) // year_days(d) + 1}, {SEASON_FR[season_of(d, day)]}"


def years_old(d, days):
    return None if days is None else days // year_days(d)


def deaths_table_v2(d):
    """« Les morts »: who, tribe, when, age, of what; the day and the place behind « plus de colonnes »."""
    dead = sorted((p for p in d.get("people", []) if p.get("died")), key=lambda p: p["died"].get("day") or 0)
    ages = [age_days(d, p, p["died"].get("day")) for p in dead]
    top = max([a for a in ages if a is not None] + [1])
    rows = []
    for p, age in zip(dead, ages):
        died = p["died"]
        cause = died.get("cause", "?")
        camp = camp_at(d, p.get("tribe"), died.get("day"))
        at = died.get("at")
        dist = round(math.hypot(at[0] - camp[0], at[1] - camp[1])) if at and camp else None
        bar = ("" if age is None else
               f'<span class=cbar aria-hidden=true style="--c:var({"--want" if is_want(cause) else "--bg-death"})">'
               f'<span style="width:{max(4, round(100 * age / top))}%"></span></span>')
        rows.append([
            (person_link(p.get("id")), p.get("id")),
            (tribe_chip(p.get("tribe")), p.get("tribe")),
            (h(when_short(d, died.get("day")) or ""), died.get("day")),
            (bar + fmt_thin(years_old(d, age)) if age is not None else None, age),
            (f'{death_mark(cause)}{h(cause_fr(cause))} <span class=muted>({"manque" if is_want(cause) else "fond"})</span>',
             f"{0 if is_want(cause) else 1}{CAUSE_ORDER.index(cause) if cause in CAUSE_ORDER else 9}"),
            (fmt_thin(died.get("day")), died.get("day")),
            (fmt_thin(dist), dist),
            (h(where_fr(d, p.get("tribe"), at, died.get("day"))), dist),
        ])
    return data_table("t-morts", [("Qui", False), ("Tribu", False), ("Quand", False), ("Âge à la mort (ans)", True), ("De quoi", False),
                                  ("Jour du run", True, True), ("Distance au camp (m)", True, True), ("Où, depuis le camp", False, True)],
                      rows, "Personne n’est mort.")


def births_table_v2(d):
    """« Les naissances »: who, tribe, when, mother, alive at the end; the father and the day behind."""
    rows = []
    for p in sorted((p for p in d.get("people", []) if p.get("born") != "start"), key=lambda p: p.get("born_day") or p.get("born") or 0):
        died = p.get("died")
        day = p.get("born_day") or p.get("born")
        fate = ('<i class="g c-kept">●</i>oui' if not died else
                f'{death_mark(died.get("cause"))}non, {h(cause_fr(died.get("cause", "?")))} à {fmt_thin(years_old(d, age_days(d, p, died.get("day"))))} ans')
        rows.append([(person_link(p.get("id")), p.get("id")), (tribe_chip(p.get("tribe")), p.get("tribe")),
                     (h(when_short(d, day) or ""), day),
                     (person_link(p.get("mother")) if p.get("mother") is not None else None, p.get("mother")),
                     (fate, 0 if not died else 1),
                     (person_link(p.get("father")) if p.get("father") is not None else None, p.get("father")),
                     (fmt_thin(day), day)])
    return data_table("t-naiss", [("Qui", False), ("Tribu", False), ("Quand", False), ("Mère", False), ("Vivant à la fin", False),
                                  ("Père", False, True), ("Jour du run", True, True)], rows, "Aucune naissance.")


def people_table_v2(d):
    """« Tout le monde »: who, tribe, fate, age, children; sex, will, trials, kept, the way carried behind."""
    rows = []
    for p in sorted(d.get("people", []), key=lambda p: (1 if p.get("died") else 0, p.get("tribe", ""), p.get("id", 0))):
        died = p.get("died")
        age = age_days(d, p, died.get("day") if died else None)
        fate = (f'{death_mark(died.get("cause"))}mort{"e" if p.get("sex") == "woman" else ""} de {h(cause_fr(died.get("cause", "?")))}, '
                f'{h(when_short(d, died.get("day")))}') if died else '<i class="g c-kept">●</i>vivant' + ("e" if p.get("sex") == "woman" else "")
        trials = p.get("trials") or []
        kept = sum(1 for t in trials if t.get("kept"))
        carried_ = last_kept(p)
        will = (f"{fmt(p.get('will_start'))} → {fmt(p.get('will'))}" if p.get("will_start") is not None and p.get("will_start") != p.get("will")
                else fmt(p.get("will")))
        rows.append([(person_link(p.get("id")), p.get("id")), (tribe_chip(p.get("tribe")), p.get("tribe")),
                     (fate, (died or {}).get("day") or 0), (fmt_thin(years_old(d, age)), age), (fmt_thin(p.get("children")), p.get("children")),
                     (h(SEX_FR.get(p.get("sex"), "")), p.get("sex")), (h(will), p.get("will")), (fmt_thin(len(trials)), len(trials)),
                     (fmt_thin(kept), kept), (h(kind_fr(carried_)) if carried_ else None, carried_ or "")])
    return data_table("t-gens", [("Qui", False), ("Tribu", False), ("Sort", False), ("Âge (ans)", True), ("Enfants", True),
                                 ("Sexe", False, True), ("Volonté, début → fin", True, True), ("Essais", True, True), ("Gardés", True, True),
                                 ("Façon portée à la fin", False, True)], rows, "Personne.")


# --- the lines of the people, read once: copies and lessons at the fire --------------------

def life_lines(d):
    """The copies and the lessons told at the fire, from `people[].lines` (each said once)."""
    seen, copies, lessons = set(), [], []
    for p in d.get("people", []):
        for ln in p.get("lines") or []:
            key = (ln.get("tick"), ln.get("what"))
            if key in seen:
                continue
            seen.add(key)
            what = ln.get("what") or ""
            m = re.fullmatch(r"person (\d+) copies person (\d+)(, losing a step)?: (.+)", what)
            if m:
                copies.append({"day": ln.get("day"), "who": int(m.group(1)), "from": int(m.group(2)), "loss": bool(m.group(3)),
                               "way": way_line_fr(m.group(4))})
                continue
            m = re.fullmatch(r"at the fire, person (\d+) says its (.+) did it (good|no good); (.+) believed it", what)
            if m:
                lessons.append({"day": ln.get("day"), "teller": int(m.group(1)), "way": way_line_fr(m.group(2)), "good": m.group(3) == "good",
                                "believers": [int(x) for x in re.findall(r"person (\d+)", m.group(4))]})
    return copies, lessons


# --- the chronicle --------------------------------------------------------------------------

EV = {  # type: (word on the line, glyph, colour class, filter)
    "naissance": ("Naissance", "●", "c-birth", "naissances"),
    "mort-want": ("Mort", "■", "c-want", "morts"),
    "mort-bg": ("Mort", "■", "c-bg", "morts"),
    "garde": ("Gardé", "◆", "c-kept", "essais"),
    "essai": ("Essai", "○", "c-try", "essais"),
    "abandon": ("Abandon", "○", "c-try", "essais"),
    "copie": ("Copie", "▲", "c-fire", "feu"),
    "lecon": ("Au feu", "▲", "c-fire", "feu"),
    "rumeur": ("Rumeur", "●", "c-rumour", "rumeurs"),
    "maison": ("Maison", "⌂", "c-camp", "maisons"),
}
EV_GROUP = {"naissance": ("naissance", "naissances"), "mort-want": ("mort de manque", "morts de manque"),
            "mort-bg": ("mort de fond", "morts de fond"), "garde": ("façon gardée", "façons gardées"),
            "essai": ("essai commencé", "essais commencés"), "abandon": ("abandon", "abandons"),
            "copie": ("copie", "copies"), "lecon": ("leçon dite au feu", "leçons dites au feu"),
            "rumeur": ("rumeur dissipée", "rumeurs dissipées"), "maison": ("maison déplacée", "maisons déplacées")}
FILTERS = [("tout", "Tout"), ("morts", "Morts"), ("naissances", "Naissances"), ("essais", "Essais et trouvailles"),
           ("feu", "Le feu"), ("rumeurs", "Rumeurs"), ("maisons", "Maisons")]
GROUP_FROM = 3   # the same kind of event three times or more the same day: one foldable line

WAY_LINE_FR = [
    (r"searches with worth halved at (\d+) tiles", r"cherche à \1 tuiles"),
    (r"searches (?:twice as wide|half as wide) \(worth halved at (\d+) tiles instead of (\d+)\)", r"cherche à \1 tuiles au lieu de \2"),
    (r"stores (\d+) kJ before resting", r"garde \1 kJ avant le repos"),
    (r"stores (?:twice as much|half as much) \((\d+) kJ instead of (\d+)\)", r"garde \1 kJ au lieu de \2"),
    (r"sleeps out where night finds it", "dort là où la nuit le trouve"),
    (r"comes home to sleep", "rentre dormir au camp"),
    (r"sets its home at \((\d+), (\d+)\)", r"pose sa maison en (\1, \2)"),
]


def way_line_fr(text):
    """A way as the vivarium's lines say it, in French; an unknown one is kept as written."""
    for pat, rep in WAY_LINE_FR:
        if re.fullmatch(pat, text):
            out = re.sub(pat, rep, text)
            return re.sub(r"(\d{4,}) kJ", lambda m: f"{fmt(int(m.group(1)))} kJ", re.sub(r"(\d{4,}) au lieu de (\d{4,})",
                          lambda m: f"{fmt(int(m.group(1)))} au lieu de {fmt(int(m.group(2)))}", out))
    return f"« {text} »"


def chronicle_events(d, year):
    """The events of one year, each `{day, type, tribe, who, text}` (`text` ready HTML, the names links),
    in the order of the days."""
    yd = year_days(d)
    first, last = (year - 1) * yd + 1, year * yd
    tribe_of = {p.get("id"): p.get("tribe") for p in d.get("people", [])}
    fire_on = ((d.get("meta") or {}).get("settings") or {}).get("fire", False)
    out = []

    def add(day, typ, tribe, who, text, order=0, extra=""):
        """`text` is plain words (escaped here, its names made links); `extra` is markup made here from numbers."""
        if day is not None and first <= day <= last:
            out.append({"day": day, "type": typ, "tribe": tribe or "", "who": [w for w in who if w is not None],
                        "text": link_people(h(text)) + extra, "order": order})
    for p in d.get("people", []):
        pid, t = p.get("id"), p.get("tribe")
        if p.get("born") != "start":
            sex = {"woman": "une fille", "man": "un garçon"}.get(p.get("sex"), "un enfant")
            parents = " et ".join(person_fr(x) for x in (p.get("mother"), p.get("father")) if x is not None)
            add(p.get("born_day") or p.get("born"), "naissance", t, [pid, p.get("mother"), p.get("father")],
                f"{person_fr(pid)}, {sex}, naît" + (f" de {parents}" if parents else ""), 1)
        died = p.get("died")
        if died:
            cause = died.get("cause", "?")
            add(died.get("day"), "mort-want" if is_want(cause) else "mort-bg", t, [pid],
                f"{person_fr(pid)} meurt de {cause_fr(cause)} à {age_fr(d, age_days(d, p, died.get('day')))}, "
                f"{where_fr(d, t, died.get('at'), died.get('day'))}", 0)
        for tr in p.get("trials") or []:
            home = tr.get("step") == "home_moved"   # its start and its giving up are the homes' lines below
            if not home:
                add(tr.get("day"), "essai", t, [pid], f"{person_fr(pid)} essaie : {kind_fr(tr.get('step', '?'))}", 5)
            if tr.get("judged_day") is not None and tr.get("kept") is not None and not (home and not tr["kept"]):
                add(tr["judged_day"], "garde" if tr["kept"] else "abandon", t, [pid],
                    f"{person_fr(pid)} {'garde' if tr['kept'] else 'abandonne'} : {kind_fr(tr.get('step', '?'))}"
                    + (" (entendu au feu)" if tr.get("heard") else ""), 2 if tr["kept"] else 6)
    copies, lessons = life_lines(d)
    for c in copies:
        add(c["day"], "copie" if fire_on else "essai", tribe_of.get(c["who"]), [c["who"], c["from"]],
            f"{person_fr(c['who'])} copie {person_fr(c['from'])} : {c['way']}" + (" (en perdant un cran)" if c["loss"] else ""), 4)
    for le in lessons:
        b = le["believers"]
        add(le["day"], "lecon", tribe_of.get(le["teller"]), [le["teller"]] + b,
            f"{person_fr(le['teller'])} dit que sa façon ({le['way']}) {'lui a fait du bien' if le['good'] else 'ne lui a rien valu'} ; "
            + ("personne ne l’a cru" if not b else f"{plural(len(b), 'personne')} l’{'ont' if len(b) > 1 else 'a'} cru"),
            3, f' <span class=x>({", ".join(str(x) for x in b)})</span>' if b else "")
    for r in d.get("rumours") or []:
        add(r.get("day"), "rumeur", r.get("tribe"), [r.get("body"), r.get("from")],
            f"{person_fr(r.get('body'))} ne trouve rien au lieu {place_fr(r.get('place'))}, où {person_fr(r.get('from'))} avait dit", 7)
    for y in d.get("years", []):
        for mv in y.get("home_moves") or []:
            how = HOW_FR.get(mv.get("how"), mv.get("how") or "")
            verb = {"trial": "essaie de déplacer sa maison", "copy": "déplace sa maison, copiée d’un voisin,",
                    "reverted": "renonce et ramène sa maison"}.get(mv.get("how"), f"déplace sa maison ({how})")
            add(mv.get("day"), "maison", y.get("tribe"), [mv.get("body")],
                f"{person_fr(mv.get('body'))} {verb} de {place_fr(mv.get('from'))} à {place_fr(mv.get('to'))}", 3)
    out.sort(key=lambda e: (e["day"], e["order"], e["who"][:1]))
    return out


def chronicle_groups(events):
    """The events by day, each day a list of single events and of groups (the same type ≥ GROUP_FROM
    times that day): `[(day, [("one", ev) | ("group", type, [ev…])])]`."""
    days = []
    for e in events:
        if not days or days[-1][0] != e["day"]:
            days.append((e["day"], []))
        days[-1][1].append(e)
    out = []
    for day, evs in days:
        items, done = [], set()
        for e in evs:
            if e["type"] in done:
                continue
            same = [x for x in evs if x["type"] == e["type"]]
            if len(same) >= GROUP_FROM:
                items.append(("group", e["type"], same))
                done.add(e["type"])
            else:
                items.append(("one", e))
        out.append((day, items))
    return out


def ev_html(e, word=True):
    word_, glyph, cls, filt = EV[e["type"]]
    lab = f'<b class=lab>{h(word_)}</b>' if word else ""
    return (f'<li class="ev t{h(e["tribe"])}" data-f="{filt}" data-tribe="{h(e["tribe"])}" data-who=" {" ".join(str(w) for w in e["who"])} ">'
            f'<i class="g {cls}" aria-hidden=true>{glyph}</i>{lab}{e["text"]}</li>')


def chronicle_year(d, year):
    """One year of the chronicle: its points, its chips, its seasons; what the lab sends when a year is asked."""
    events = chronicle_events(d, year)
    yd = year_days(d)
    counts = {}
    for e in events:
        counts[EV[e["type"]][3]] = counts.get(EV[e["type"]][3], 0) + 1
    seasons = []
    groups = chronicle_groups(events)
    for season in SEASON_ALL:
        days = [(day, items) for day, items in groups if season_of(d, day) == season]
        span = [day for day in range((year - 1) * yd + 1, year * yd + 1) if season_of(d, day) == season]
        lo, hi = (span[0], span[-1]) if span else (0, 0)
        n = sum(len(it[2]) if it[0] == "group" else 1 for _, items in days for it in items)
        lis = []
        for day, items in days:
            evs = []
            for it in items:
                if it[0] == "one":
                    evs.append(ev_html(it[1]))
                    continue
                _, typ, members = it
                word, glyph, cls, filt = EV[typ]
                one, many = EV_GROUP[typ]
                evs.append(f'<li class="ev grp" data-f="{filt}"><details><summary><i class="g {cls}" aria-hidden=true>{glyph}</i>'
                           f'<b class=lab>{h(word)}</b><span class=gn>{len(members)}</span> <span class=gw data-one="{h(one)}" '
                           f'data-many="{h(many)}">{h(many)}</span> ce jour-là</summary>'
                           f'<ul>{"".join(ev_html(m, word=False) for m in members)}</ul></details></li>')
            lis.append(f'<li class=day><span class=dd>jour {day}</span><ul class=evs>{"".join(evs)}</ul></li>')
        body = f"<ol class=days>{''.join(lis)}</ol>" if lis else '<p class="chron-empty">rien cette saison</p>'
        seasons.append(f'<div class=season><h4>{h(SEASON_FR[season].capitalize())} <small>jours {lo} à {hi} · '
                       f'{plural(n, "événement")}</small></h4>{body}</div>')
    chips = "".join(f'<button type=button data-filter="{k}" aria-pressed="{"true" if k == "tout" else "false"}">{h(lab)}'
                    f' <span class=muted>{len(events) if k == "tout" else counts.get(k, 0)}</span></button>' for k, lab in FILTERS)
    return points_html(chronicle_points(d, year, events)), chips, "".join(seasons)


def chronicle_html(d, year, chron_url):
    """« La chronique »: one year at a time (the others asked to the lab when picked), by season then day,
    a glyph per type and the tribe's colour on the edge, filters, a search, repeats folded."""
    ly = last_year(d)
    if not ly:
        return ""
    year = min(max(1, year or 1), ly)
    years_nav = "".join(f'<button type=button data-year="{y}" aria-current="{"true" if y == year else "false"}">'
                        f'{"an " if y == year else ""}{y}</button>' for y in range(1, ly + 1))
    ts = tribes(d)
    tribe_sel = ("<select data-tribe aria-label=Tribu><option value=''>les deux tribus</option>"
                 + "".join(f"<option value='{h(t)}'>tribu {h(t)}, {h(tribe_side(d, t))}</option>" for t in ts) + "</select>") if len(ts) > 1 else ""
    legend = " · ".join(f'<i class="g {EV[k][2]}">{EV[k][1]}</i>{h(w)}' for k, w in
                        (("naissance", "naissance"), ("mort-want", "mort de manque"), ("mort-bg", "mort de fond"),
                         ("garde", "façon gardée"), ("essai", "essai ou abandon"), ("lecon", "le feu : leçon dite, copie"),
                         ("rumeur", "rumeur dissipée"), ("maison", "maison déplacée")))
    rums = d.get("rumours") or []
    capped = ""
    if rums and sum((y.get("fire") or {}).get("dispelled") or 0 for y in d.get("years", [])) > len(rums):
        days = [r.get("day") for r in rums if r.get("day") is not None]
        capped = (f" Le fichier ne garde le détail que des {fmt(len(rums))} premières rumeurs (jours {min(days)} à {max(days)}) : "
                  "les autres années n’en montrent aucune.")
    tribes_txt = " ; le bord de chaque ligne a la couleur de sa tribu (" + ", ".join(f"{tribe_chip(t)}" for t in ts) + ")" if len(ts) > 1 else ""
    return (f'<section id=chronique class=group data-chron="{h(chron_url)}"><h2>La chronique</h2><div data-chron-lead><p class=muted>l’an {year} se charge…</p></div>'
            f'<p class=caption>Une année à la fois, par saison puis par jour. Ce qui se répète le même jour '
            f'({GROUP_FROM} fois ou plus) tient sur une ligne qu’on déplie{tribes_txt}.{h(capped)}</p><p class=caption>{legend}</p>'
            f'<nav class=chron-years aria-label="années">{years_nav}</nav>'
            f'<div class=filters><div class=chips role=group aria-label="ne montrer que" data-chips></div>{tribe_sel}'
            f'<input type=search data-search placeholder="un nom : 14, personne 14" aria-label="chercher un nom">'
            f'<span class=count data-count></span></div>'
            f'<div class=chron data-chron-body></div></section>')


CHRON_JS = """
document.querySelectorAll('#chronique').forEach(c => {
  let filt = 'tout';
  const tribe = c.querySelector('select[data-tribe]'), search = c.querySelector('[data-search]'), count = c.querySelector('[data-count]');
  function apply() {
    const t = tribe ? tribe.value : '', q = search.value.trim().toLowerCase(), num = (q.match(/^\\D*(\\d+)\\D*$/) || [])[1];
    const ok = e => (filt === 'tout' || e.dataset.f === filt) && (!t || e.dataset.tribe === t)
      && (!q || (num ? e.dataset.who.includes(' ' + num + ' ') : e.textContent.toLowerCase().includes(q)));
    let shown = 0;
    c.querySelectorAll('li.ev:not(.grp)').forEach(e => { e.hidden = !ok(e); });
    c.querySelectorAll('li.grp').forEach(g => {
      const n = [...g.querySelectorAll('li.ev')].filter(e => !e.hidden).length;
      g.hidden = n === 0; g.querySelector('.gn').textContent = n;
      const w = g.querySelector('.gw'); w.textContent = n > 1 ? w.dataset.many : w.dataset.one;
    });
    c.querySelectorAll('ul.evs > li.ev:not(.grp)').forEach(e => { if (!e.hidden) shown++; });
    c.querySelectorAll('ul.evs > li.grp').forEach(g => { if (!g.hidden) shown += +g.querySelector('.gn').textContent; });
    c.querySelectorAll('li.day').forEach(d => d.hidden = !d.querySelector('ul.evs > li:not([hidden])'));
    c.querySelectorAll('.season').forEach(s => { const o = s.querySelector('ol.days'); if (o) s.hidden = !o.querySelector('li.day:not([hidden])'); });
    count.textContent = shown + ' événement' + (shown > 1 ? 's' : '') + ' affiché' + (shown > 1 ? 's' : '');
  }
  function bindChips() {
    const chips = c.querySelectorAll('[data-filter]');
    chips.forEach(b => {
      b.setAttribute('aria-pressed', b.dataset.filter === filt ? 'true' : 'false');
      b.addEventListener('click', () => { filt = b.dataset.filter; chips.forEach(o => o.setAttribute('aria-pressed', o === b ? 'true' : 'false')); apply(); });
    });
  }
  c.querySelectorAll('[data-year]').forEach(b => b.addEventListener('click', async () => {
    const y = b.dataset.year;
    count.textContent = 'chargement de l’an ' + y + '…';
    try {
      const r = await fetch(c.dataset.chron + '?an=' + y);
      if (!r.ok) throw new Error(r.status);
      const data = await r.json();
      c.querySelector('[data-chron-lead]').innerHTML = data.lead;
      c.querySelector('[data-chips]').innerHTML = data.chips;
      c.querySelector('[data-chron-body]').innerHTML = data.seasons;
      c.querySelectorAll('[data-year]').forEach(o => { o.setAttribute('aria-current', o === b ? 'true' : 'false'); o.textContent = (o === b ? 'an ' : '') + o.dataset.year; });
      if (![...c.querySelectorAll('[data-filter]')].some(o => o.dataset.filter === filt)) filt = 'tout';
      bindChips(); apply();
    } catch (e) { count.textContent = 'l’an ' + y + ' n’a pas pu être lu (' + e.message + ')'; }
  }));
  if (tribe) tribe.addEventListener('change', apply);
  search.addEventListener('input', apply);
  const first = c.querySelector('[data-year][aria-current=true]');
  if (first) first.click();
});
"""


# --- the map: where they live, and why there -----------------------------------------------

def camp_stays(d, t):
    """A tribe's camps as stays: `[(first_year, last_year, (x, y))]`, a new stay when the camp moved
    more than a place (16 tiles); the start camp is year 0's."""
    ts = tribes(d)
    camps = (d.get("meta") or {}).get("camps") or []
    i = ts.index(t) if t in ts else -1
    pts = [(0, tuple(camps[i]))] if 0 <= i < len(camps) else []
    pts += [(y["year"], tuple(y["camp"])) for y in years_of(d, t) if y.get("camp")]
    stays = []
    for yr, c in pts:
        if stays and math.hypot(c[0] - stays[-1][2][0], c[1] - stays[-1][2][1]) <= 16:
            stays[-1][1] = yr
        else:
            stays.append([yr, yr, c])
    return [(max(1, a), max(1, b), c) for a, b, c in stays]


def known_places(d, t):
    """What the living of a tribe know at the end: `{(px, py): seen}`, a place seen by one of them
    counts as seen, else heard at the fire. Places are 16-tile squares."""
    out = {}
    for p in d.get("people", []):
        if p.get("tribe") != t or p.get("died"):
            continue
        for b in p.get("believes") or []:
            key = tuple(b.get("place") or ())
            if len(key) == 2:
                out[key] = out.get(key, False) or bool(b.get("seen"))
    return out


def known_places_by_year(d, end_places):
    """The places each tribe's living knew at each year's end (`years[].knowledge.where`), as
    `{year: {tribe: [[x, y, seen], …]}}`; a run written before that key gets its end places for every
    year, and `True` beside to say so."""
    out, any_where = {}, False
    for y in d.get("years", []):
        k = y.get("knowledge") or {}
        if isinstance(k.get("where"), list):
            any_where = True
            out.setdefault(str(y.get("year")), {})[y.get("tribe")] = [[w[0], w[1], 1 if w[2] else 0] for w in k["where"] if len(w) == 3]
    if any_where:
        return out, False
    end = {t: [[x, y, 1 if s else 0] for (x, y), s in places.items()] for t, places in end_places.items()}
    return {str(yr): end for yr in range(1, (last_year(d) or 1) + 1)}, True


def tribe_box(d, t, W, Hh, stays, places):
    """The square the tribe lives in: its camps, its places, its noons; a margin around."""
    xs, ys = [c[0] for _, _, c in stays], [c[1] for _, _, c in stays]
    for (px, py) in places:
        xs += [px * 16, px * 16 + 16]
        ys += [py * 16, py * 16 + 16]
    for pr in d.get("presence") or []:
        if pr.get("tribe") == t:
            cell = pr.get("cell_tiles") or 32
            for cx, cy, _n in pr.get("cells") or []:
                xs += [cx * cell, cx * cell + cell]
                ys += [cy * cell, cy * cell + cell]
    if not xs:
        return [0, 0, W, Hh]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    side = max(x1 - x0, y1 - y0, 160) * 1.15
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    x = min(max(0, cx - side / 2), max(0, W - side))
    y = min(max(0, cy - side / 2), max(0, Hh - side))
    return [round(x), round(y), round(min(side, W)), round(min(side, Hh))]


MAP2_JS = """
function initMap2(m) {
  if (m.dataset.bound) return; m.dataset.bound = 1;
  const data = JSON.parse(m.querySelector('script[type="application/json"]').textContent);
  const svg = m.querySelector('svg.map'), slider = m.querySelector('[data-yr]'), yrTxt = m.querySelector('.yr');
  const dots = m.querySelector('g.dots'), pos = data.positions || [], dpy = data.dpy;
  const legendStays = m.querySelector('[data-stays]');
  // the vivarium's picture carries its own camps, living and dead: painted over with the ground beside them
  const img = svg.querySelector('image'), pic = new Image();
  pic.onload = () => {
    const c = document.createElement('canvas'); c.width = pic.width; c.height = pic.height;
    const g = c.getContext('2d'); g.drawImage(pic, 0, 0);
    const px = g.getImageData(0, 0, c.width, c.height), a = px.data, marks = data.marks;
    const isMark = i => marks.some(k => a[i] === k[0] && a[i + 1] === k[1] && a[i + 2] === k[2]);
    for (let y = 0; y < c.height; y++) for (let x = 0; x < c.width; x++) {
      const i = (y * c.width + x) * 4;
      if (!isMark(i)) continue;
      for (let r = 1; r < 6; r++) {
        const n = [[x - r, y], [x + r, y], [x, y - r], [x, y + r]].find(([u, v]) => u >= 0 && v >= 0 && u < c.width && v < c.height && !isMark((v * c.width + u) * 4));
        if (n) { const j = (n[1] * c.width + n[0]) * 4; a[i] = a[j]; a[i + 1] = a[j + 1]; a[i + 2] = a[j + 2]; break; }
      }
    }
    g.putImageData(px, 0, 0); img.setAttribute('href', c.toDataURL());
  };
  pic.src = img.getAttribute('href');
  const lastSample = year => { let k = -1; for (let i = 0; i < pos.length && pos[i][0] <= year * dpy; i++) k = i; return k; };
  function drawDots(k) {
    let out = '';
    if (k >= 0) {
      const trail = {};
      for (let i = Math.max(0, k - 4); i < k; i++) for (const [id, x, y] of pos[i][1]) (trail[id] = trail[id] || []).push(x + ',' + y);
      for (const [id, x, y] of pos[k][1]) {
        const c = data.colors[data.tribe[id]] || '#444';
        if (trail[id]) out += `<polyline fill="none" stroke="${c}" stroke-opacity=".5" stroke-width="${data.r / 3}" points="${trail[id].join(' ')} ${x},${y}"/>`;
        out += `<circle data-person="${id}" cx="${x}" cy="${y}" r="${data.r}" fill="${c}" stroke="#fff" stroke-width="${data.r / 4}" style="cursor:pointer"><title>personne ${id} (${data.tribe[id]}), jour ${pos[k][0]} : cliquez pour sa fiche</title></circle>`;
      }
    }
    dots.innerHTML = out;
  }
  function places(year) {
    const k = data.known[year] || {}, seen = m.querySelector('g.seen'), heard = m.querySelector('g.heard');
    let a = '', b = '';
    for (const [t, list] of Object.entries(k)) {
      const c = data.colors[t] || '#444';
      for (const [x, y, s] of list) {
        if (s) a += `<rect x="${x * 16}" y="${y * 16}" width="16" height="16" fill="${c}" fill-opacity=".55" stroke="#fff" stroke-width="1"/>`;
        else b += `<rect x="${x * 16 + 1}" y="${y * 16 + 1}" width="14" height="14" fill="none" stroke="${c}" stroke-width="2" stroke-dasharray="3 2"/>`;
      }
    }
    seen.innerHTML = a; heard.innerHTML = b;
  }
  function show(year, day) {
    yrTxt.textContent = 'an ' + year + (day ? ', jour ' + day : '');
    if (!day) places(year);
    let hs = '';
    if (m.querySelector('[data-layer=heat]').checked) for (const [t, [cell, cells]] of Object.entries(data.heat[year] || {})) {
      const c = data.colors[t] || '#444';
      for (const [cx, cy, n] of cells) hs += `<rect x="${cx * cell}" y="${cy * cell}" width="${cell}" height="${cell}" fill="${c}" fill-opacity="${(0.15 + 0.6 * Math.min(1, n / data.peak)).toFixed(2)}"><title>tribu ${t}, an ${year} : ${n} midi${n > 1 ? 's' : ''} ici</title></rect>`;
    }
    m.querySelector('g.heat').innerHTML = hs;
    m.querySelectorAll('[data-dy]').forEach(g => { const y = +g.dataset.dy; g.style.display = y > year ? 'none' : 'inline'; g.style.opacity = y === year ? 1 : .3; });
    m.querySelectorAll('[data-by]').forEach(g => g.style.display = +g.dataset.by === year ? 'inline' : 'none');
    m.querySelectorAll('[data-stay]').forEach(g => {
      const [a, b] = g.dataset.stay.split('-').map(Number);
      g.style.display = a > year ? 'none' : 'inline';
      g.classList.toggle('now', a <= year && year <= b);
      g.querySelector('.big').style.display = a <= year && year <= b ? 'inline' : 'none';
    });
    drawDots(day ? pos.findIndex(p => p[0] >= day) : lastSample(year));
  }
  m.querySelectorAll('[data-layer]').forEach(c => c.addEventListener('change', () => {
    const l = c.dataset.layer;
    if (l !== 'heat') m.querySelectorAll('g.' + l).forEach(g => g.style.display = c.checked ? 'inline' : 'none');
    show(+slider.value);
  }));
  slider.addEventListener('input', () => { stop(); show(+slider.value); });
  m.querySelectorAll('[data-zoom]').forEach(b => b.addEventListener('click', () => {
    svg.setAttribute('viewBox', data.boxes[b.dataset.zoom].join(' '));
    m.querySelectorAll('[data-zoom]').forEach(o => o.setAttribute('aria-pressed', o === b ? 'true' : 'false'));
  }));
  let timer = null;
  const play = m.querySelector('[data-play-years]'), days = m.querySelector('[data-play-days]');
  function stop() { clearInterval(timer); timer = null; play.textContent = '▶ Les années'; days.textContent = '▶ Cette année jour par jour'; }
  play.addEventListener('click', () => {
    if (timer) return stop();
    if (+slider.value >= +slider.max) slider.value = 1;
    play.textContent = '❚❚ Pause'; show(+slider.value);
    timer = setInterval(() => { if (+slider.value >= +slider.max) return stop(); slider.value = +slider.value + 1; show(+slider.value); }, 900);
  });
  days.addEventListener('click', () => {
    if (timer) return stop();
    const year = +slider.value; let day = (year - 1) * dpy + 1;
    days.textContent = '❚❚ Pause';
    timer = setInterval(() => { if (day > year * dpy) return stop(); show(year, day++); }, 120);
  });
  show(+slider.value);
}
document.querySelectorAll('[data-map2]').forEach(initMap2);
"""


def map_v2(d, uid):
    """« Où ils vivent »: the map, a year at a time, with a legend beside: the camps as big signs at
    their tribe's colour with their stays dated, the noons of the year, its deaths and births, the
    living at the year's last noon, and, ticked, the places the living know at the end, seen or heard."""
    m = d.get("map") or {}
    uri = m.get("data_uri")
    tiles = m.get("tiles") or (d.get("meta") or {}).get("map_tiles")
    if not uri or not tiles or not str(uri).startswith("data:image/png;base64,"):
        return f'<p class="muted">{NO_DATA} (pas de carte dans le fichier)</p>'
    W, Hh = tiles
    ts = tribes(d)
    ly = last_year(d) or 1
    yd = year_days(d)
    pos = positions_of(d)
    stays = {t: camp_stays(d, t) for t in ts}
    places = {t: known_places(d, t) for t in ts}
    known_by_year, by_end = known_places_by_year(d, places)
    boxes = {"all": [0, 0, W, Hh]}
    boxes.update({t: tribe_box(d, t, W, Hh, stays[t], places[t]) for t in ts})
    first = ts[0] if ts else "all"
    fs = max(6, boxes[first][2] / 45)   # the signs sized for the tribe's view, still visible on the whole map
    parts = [f'<svg viewBox="{" ".join(map(str, boxes[first]))}" class="map" role="img" aria-label="carte : où vivent les tribus">',
             f'<image href="{h(uri)}" x="0" y="0" width="{W}" height="{Hh}" style="image-rendering:pixelated"/>']
    presence = d.get("presence") or []
    peak = max((c[2] for p in presence for c in (p.get("cells") or [])), default=1)
    heat = {}   # {year: {tribe: [cell, [[cx, cy, n], …]]}}, drawn by the page's script for the year shown
    for p in presence:
        heat.setdefault(str(p.get("year")), {})[p.get("tribe")] = [p.get("cell_tiles") or 32, p.get("cells") or []]
    parts.append('<g class="heat"></g>')
    parts.append('<g class="seen" display="none"></g><g class="heard" display="none"></g>')
    parts.append('<g class="dots"></g>')
    for i, t in enumerate(ts):
        color = tribe_color(t, i)
        st = stays[t]
        if len(st) > 1:
            parts.append(f'<polyline fill="none" stroke="{color}" stroke-width="{fs / 4:.1f}" stroke-dasharray="{fs / 2:.1f} {fs / 3:.1f}" '
                         f'points="{" ".join(f"{c[0]},{c[1]}" for _, _, c in st)}"/>')
        for a, b, (x, y) in st:
            span = f"an {a}" if a == b else f"an {a} → an {b}"
            parts.append(f'<g data-stay="{a}-{b}" style="pointer-events:none"><title>camp {h(t)}, {span} : ({x}, {y})</title>'
                         f'<rect x="{x - fs / 2:.1f}" y="{y - fs / 2:.1f}" width="{fs:.1f}" height="{fs:.1f}" transform="rotate(45 {x} {y})" '
                         f'fill="{color}" stroke="#fff" stroke-width="{fs / 8:.1f}"/>'
                         f'<g class=big><circle cx="{x}" cy="{y}" r="{fs * 1.3:.1f}" fill="{color}" stroke="#fff" stroke-width="{fs / 4:.1f}"/>'
                         f'<text x="{x}" y="{y + fs * 0.5:.1f}" text-anchor="middle" font-size="{fs * 1.5:.1f}" font-weight="800" fill="#fff">{h(t)}</text></g>'
                         f'<text x="{x - fs * 1.7 if x > W * 0.6 else x + fs * 1.7:.1f}" y="{y + fs * 3 if y < Hh * 0.4 else y - fs * 1.8:.1f}" '
                         f'text-anchor="{"end" if x > W * 0.6 else "start"}" font-size="{fs * 1.1:.1f}" font-weight="700" fill="#111" stroke="#fff" '
                         f'stroke-width="{fs / 4:.1f}" paint-order="stroke">camp {h(t)}, {span}</text></g>')
    for p in d.get("people", []):
        died = p.get("died")
        if died and died.get("at"):
            x, y = died["at"]
            r = fs * 0.55
            parts.append(f'<g data-dy="{(died.get("day", 1) - 1) // yd + 1}" data-person="{p.get("id")}" style="cursor:pointer" stroke-linecap="round">'
                         f'<line x1="{x - r}" y1="{y - r}" x2="{x + r}" y2="{y + r}" stroke="#fff" stroke-width="{fs / 2.5:.1f}"/>'
                         f'<line x1="{x - r}" y1="{y + r}" x2="{x + r}" y2="{y - r}" stroke="#fff" stroke-width="{fs / 2.5:.1f}"/>'
                         f'<line x1="{x - r}" y1="{y - r}" x2="{x + r}" y2="{y + r}" stroke="#111" stroke-width="{fs / 6:.1f}"/>'
                         f'<line x1="{x - r}" y1="{y + r}" x2="{x + r}" y2="{y - r}" stroke="#111" stroke-width="{fs / 6:.1f}"/>'
                         f'<title>{h(person_fr(p.get("id")))} ({h(p.get("tribe", ""))}) meurt de {h(cause_fr(died.get("cause", "?")))}, '
                         f'{h(day_fr(d, died.get("day")))} : cliquez pour sa fiche</title></g>')
        born = p.get("born_day") or p.get("born")
        if p.get("born") != "start" and isinstance(born, int) and p.get("born_at"):
            x, y = p["born_at"]
            parts.append(f'<circle data-by="{(born - 1) // yd + 1}" data-person="{p.get("id")}" style="cursor:pointer" cx="{x}" cy="{y}" '
                         f'r="{fs * 0.45:.1f}" fill="#d55181" stroke="#fff" stroke-width="{fs / 6:.1f}" display="none">'
                         f'<title>{h(person_fr(p.get("id")))} naît, {h(day_fr(d, born))} : cliquez pour sa fiche</title></circle>')
    parts.append('</svg>')
    data = {"dpy": yd, "r": round(fs * 0.4, 1), "positions": pos, "boxes": boxes,
            "tribe": {str(p.get("id")): p.get("tribe") for p in d.get("people", [])},
            "colors": {t: tribe_color(t, i) for i, t in enumerate(ts)},
            "known": known_by_year, "known_end": by_end, "heat": heat, "peak": peak,
            "marks": [v for v in ((m.get("legend") or {}).get(k) for k in ("camp", "alive", "dead")) if isinstance(v, list)]}
    payload = json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    zoom = "".join(f'<button type=button data-zoom="{t}" aria-pressed="{"true" if t == first else "false"}">tribu {h(t)}</button>' for t in ts) \
        + f'<button type=button data-zoom="all" aria-pressed="{"true" if first == "all" else "false"}">toute la carte</button>'
    at_end = known_by_year.get(str(ly), {})
    nseen = {t: sum(1 for w in at_end.get(t, []) if w[2]) for t in ts}
    nheard = {t: sum(1 for w in at_end.get(t, []) if not w[2]) for t in ts}
    when = "à la fin du run (ce run n’a pas les lieux année par année)" if by_end else "l’année montrée"
    bar = (f'<div class=bar><span class=yr>an {ly}</span><input type=range data-yr min=1 max={ly} value={ly} aria-label="année">'
           f'<button type=button data-play-years>▶ Les années</button><button type=button data-play-days>▶ Cette année jour par jour</button></div>'
           f'<div class=bar><span class=seg role=group aria-label="cadrer sur">{zoom}</span></div>')

    def sw(inner):
        return f'<svg viewBox="-10 -10 20 20" aria-hidden=true>{inner}</svg>'
    items = []
    for i, t in enumerate(ts):
        c = tribe_color(t, i)
        items.append((sw(f'<circle r="8" fill="{c}" stroke="#fff" stroke-width="1.5"/><text y="4" text-anchor="middle" font-size="11" font-weight="800" fill="#fff">{h(t)}</text>'),
                      f"le camp de la tribu {h(t)}, {h(tribe_side(d, t))}, l’année montrée"))
    items.append((sw('<rect x="-4" y="-4" width="8" height="8" transform="rotate(45)" fill="#888" stroke="#fff"/>'),
                  "un camp de l’année montrée ou d’avant, daté (« an 1 → an 20 ») ; un trait pointillé relie les camps successifs"))
    items.append((sw(f'<circle r="4" fill="{tribe_color(ts[0], 0) if ts else "#444"}" stroke="#fff" stroke-width="1.2"/>'),
                  "une personne à midi, le dernier jour de l’année (cliquez : sa fiche)"))
    items.append((sw(f'<rect x="-8" y="-8" width="16" height="16" fill="{tribe_color(ts[0], 0) if ts else "#444"}" fill-opacity=".45"/>'),
                  "là où la tribu a passé ses midis cette année (plus foncé : plus souvent)"))
    items.append((sw('<g stroke-linecap="round"><path d="M-5-5L5 5M-5 5L5-5" stroke="#fff" stroke-width="4"/><path d="M-5-5L5 5M-5 5L5-5" stroke="#111" stroke-width="2"/></g>'),
                  "un mort cette année (pâle : les années d’avant)"))
    items.append((sw('<circle r="4.5" fill="#d55181" stroke="#fff" stroke-width="1.2"/>'), "une naissance cette année"))
    ground = [(sw('<rect x="-8" y="-8" width="16" height="16" fill="rgb(120,160,70)"/>'), "prairie"),
              (sw('<rect x="-8" y="-8" width="16" height="16" fill="rgb(50,100,45)"/>'), "forêt"),
              (sw('<rect x="-8" y="-8" width="16" height="16" fill="rgb(60,110,170)"/>'), "eau (rivière, lac) ; plus clair : gué, ruisseau")]
    lay = ('<div class=lay>'
           '<label><input type=checkbox data-layer=heat checked> où la tribu a passé ses midis cette année</label>'
           f'<label><input type=checkbox data-layer=seen> lieux de nourriture vus par un vivant, {h(when)} ; à la fin '
           f'({", ".join(f"{h(t)} {fmt(nseen[t])}" for t in ts)})</label>'
           f'<label><input type=checkbox data-layer=heard> lieux connus seulement par ouï-dire au feu, {h(when)} ; à la fin '
           f'({", ".join(f"{h(t)} {fmt(nheard[t])}" for t in ts)})</label></div>')
    stays_txt = "".join(f'<li>{tribe_chip(t)}' + " ; ".join(f'({c[0]}, {c[1]}) an {a}' + ("" if a == b else f" → an {b}") for a, b, c in stays[t]) + "</li>"
                        for t in ts)
    legend = (f'<div class=mlegend><h4>Ce qu’on voit</h4><ul>{"".join(f"<li>{s}<span>{w}</span></li>" for s, w in items)}</ul>'
              f'<h4>Pourquoi ils sont là</h4>{lay}'
              f'<h4>Le fond</h4><ul>{"".join(f"<li>{s}<span>{w}</span></li>" for s, w in ground)}</ul>'
              f'<h4>Les camps, datés</h4><ul class=stays data-stays>{stays_txt}</ul></div>')
    note = ('<p class=mapnote>Les lieux sont des carrés de 16 cases, ceux que les vivants de la tribu ont en tête à la fin de l’année montrée'
            + (" (ce run est plus ancien : seulement à la fin du run)" if by_end else "")
            + '. L’eau trouvée n’a pas de position dans le fichier : on la voit sur le fond.</p>')
    return (f'<div class=map2 data-map2="{h(uid)}"><div>{"".join(parts)}{bar}{note}</div>{legend}'
            f'<script type="application/json">{payload}</script></div>')


# --- the person's card ----------------------------------------------------------------------

def frise_svg(d, p, copies, lessons):
    """A person's life on one line: birth to death or the end, its trials, keeps, lessons, copies, children."""
    end = run_days(d) or 1
    yd = year_days(d)
    W, H, x0, x1 = 640, 74, 14, 626
    sx = lambda day: x0 + (x1 - x0) * (max(1, min(end, day)) - 1) / max(1, end - 1)  # noqa: E731
    born = 1 if p.get("born") == "start" else (p.get("born_day") or p.get("born") or 1)
    died = (p.get("died") or {}).get("day")
    color = tribe_color(p.get("tribe"), tribes(d).index(p["tribe"]) if p.get("tribe") in tribes(d) else 0)
    out = [f'<svg class=frise viewBox="0 0 {W} {H}" role="img" aria-label="sa vie sur une ligne">']
    for y in range(0, end // yd + 1, 5):
        x = sx(y * yd + 1)
        out.append(f'<line x1="{x:.1f}" x2="{x:.1f}" y1="10" y2="56" stroke="var(--line)"/><text x="{x:.1f}" y="70" font-size="10" '
                   f'text-anchor="middle" fill="var(--muted)">an {y + 1}</text>')
    out.append(f'<line x1="{sx(born):.1f}" x2="{sx(died or end):.1f}" y1="33" y2="33" stroke="{color}" stroke-width="5" stroke-linecap="round"/>')

    def mark(day, glyph, cls, title, dy):
        out.append(f'<text x="{sx(day):.1f}" y="{33 + dy}" font-size="13" text-anchor="middle" class="{cls}" fill="currentColor">{glyph}'
                   f'<title>{h(title)}</title></text>')
    for t in p.get("trials") or []:
        mark(t.get("day") or 1, "○", "c-try", f"jour {t.get('day')} : essaie {kind_fr(t.get('step', '?'))}", -9)
        if t.get("judged_day") is not None and t.get("kept"):
            mark(t["judged_day"], "◆", "c-kept", f"jour {t['judged_day']} : garde {kind_fr(t.get('step', '?'))}", -9)
    for le in lessons:
        mark(le["day"], "▲", "c-fire", f"jour {le['day']} : raconte au feu ({le['way']})", 19)
    for c in copies:
        mark(c["day"], "▲", "c-fire", f"jour {c['day']} : copie personne {c['from']}", 19)
    for k in d.get("people", []):
        if p.get("id") in (k.get("mother"), k.get("father")) and k.get("born") != "start":
            mark(k.get("born_day") or k.get("born"), "●", "c-birth", f"jour {k.get('born_day') or k.get('born')} : naissance de personne {k.get('id')}", 19)
    if died:
        cause = p["died"].get("cause")
        mark(died, "■", "c-want" if is_want(cause) else "c-bg", f"jour {died} : meurt de {cause_fr(cause)}", 5)
    out.append("</svg>")
    return "".join(out)


def person_card(d, pid):
    """The card of one person, what the lab sends when a name is clicked: tribe, life in a line,
    what it tried, kept, told, copied, believed, its death."""
    p = next((x for x in d.get("people", []) if x.get("id") == pid), None)
    if p is None:
        return f"<p class=muted>personne {pid} introuvable dans ce run</p>"
    copies_all, lessons_all = life_lines(d)
    copies = [c for c in copies_all if c["who"] == pid]
    copied_from_me = [c for c in copies_all if c["from"] == pid]
    lessons = [le for le in lessons_all if le["teller"] == pid]
    heard = [le for le in lessons_all if pid in le["believers"]]
    woman = p.get("sex") == "woman"
    e = "e" if woman else ""
    pron = "elle" if woman else "il"
    died = p.get("died")
    if p.get("born") == "start":
        born = f"là au départ, à {age_fr(d, p.get('age_days_at_start'))}"
    else:
        day = p.get("born_day") or p.get("born")
        parents = " et ".join(person_link(x) for x in (p.get("mother"), p.get("father")) if x is not None)
        born = f"né{e} {h(day_fr(d, day))}" + (f", de {parents}" if parents else "")
    fate = (f'{death_mark(died.get("cause"))}mort{e} de {h(cause_fr(died.get("cause", "?")))}, {h(day_fr(d, died.get("day")))}, '
            f'à {h(age_fr(d, age_days(d, p, died.get("day"))))}, {h(where_fr(d, p.get("tribe"), died.get("at"), died.get("day")))}') if died \
        else f'<i class="g c-kept">●</i>vivant{e} à la fin, {h(age_fr(d, age_days(d, p)))}'
    will = (f"{fmt(p.get('will_start'))} au départ, {fmt(p.get('will'))} à la fin" if p.get("will_start") is not None else fmt(p.get("will")))
    trials = p.get("trials") or []
    kept = [t for t in trials if t.get("kept")]
    tl = "".join(f"<li>jour {t.get('day')} : {h(kind_fr(t.get('step', '?')))}, "
                 + ("<i class='g c-kept'>◆</i>gardé" if t.get("kept") else "abandonné" if t.get("kept") is False else "pas jugé")
                 + (" (entendu au feu)" if t.get("heard") else "") + "</li>" for t in trials)
    ll = "".join(f"<li>jour {le['day']} : sa façon ({h(le['way'])}) {'lui a fait du bien' if le['good'] else 'ne lui a rien valu'} ; "
                 f"{'personne ne l’a cru' if not le['believers'] else plural(len(le['believers']), 'personne') + ' l’ont cru' if len(le['believers']) > 1 else '1 personne l’a cru'}</li>"
                 for le in lessons)
    cl = "".join(f"<li>jour {c['day']} : copie {person_link(c['from'])} : {h(c['way'])}{' (en perdant un cran)' if c['loss'] else ''}</li>" for c in copies)
    kids = [k for k in d.get("people", []) if pid in (k.get("mother"), k.get("father"))]
    ways = p.get("ways") or {}
    way_txt = (f"{'dort dehors' if ways.get('sleep_out') else 'rentre dormir au camp'}, cherche à {fmt(ways.get('worth_half_tiles'))} tuiles, "
               f"garde {fmt(ways.get('stock_kj'))} kJ avant le repos") if ways else "esprit jamais lu"
    bel = p.get("believes") or []
    nseen = sum(1 for b in bel if b.get("seen"))
    sec = lambda title, items, empty: f"<h4>{title}</h4>" + (f"<ul>{items}</ul>" if items else f"<p class=muted>{empty}</p>")  # noqa: E731
    return (f'<h3>{tribe_chip(p.get("tribe"))}{h(person_fr(pid))} <small class=muted>{h(SEX_FR.get(p.get("sex"), ""))}, '
            f'tribu {h(p.get("tribe", "?"))} {h(tribe_side(d, p.get("tribe")))}</small></h3>'
            f'<p>{born}. {fate}.</p>{frise_svg(d, p, copies, lessons)}'
            f'<p class=caption><i class="g c-try">○</i>essai <i class="g c-kept">◆</i>gardé <i class="g c-fire">▲</i>raconté ou copié au feu '
            f'<i class="g c-birth">●</i>un enfant <i class="g c-bg">■</i>sa mort</p>'
            f'<p><b>Volonté</b> {h(will)} · <b>À la fin</b> : {h(way_txt)} · <b>Enfants</b> : '
            f'{", ".join(person_link(k.get("id")) for k in kids) or "aucun"}.</p>'
            f'<p><b>Ce qu’{pron} a fait</b> : {fmt(p.get("gathered_kj"))} kJ cueillis, {fmt(p.get("meals"))} repas ; '
            f'{fmt(hours_to_days(p.get("thin_hours")))} jours maigre ; {plural(len(heard), "leçon")} entendue{"s" if len(heard) > 1 else ""} au feu et crue{"s" if len(heard) > 1 else ""} ; '
            f'copié{e} {fmt(len(copied_from_me))} fois par d’autres ; {fmt(len(bel))} lieux en tête à la fin ({fmt(nseen)} vus, {fmt(len(bel) - nseen)} entendus).</p>'
            + sec(f"Ce qu’{pron} a essayé ({len(trials)}, {plural(len(kept), 'gardé')})", tl, "rien essayé")
            + sec(f"Ce qu’{pron} a raconté au feu ({len(lessons)})", ll, "rien raconté")
            + sec(f"Ce qu’{pron} a copié ({len(copies)})", cl, "rien copié"))


FICHE_JS = """
(() => {
  const page = document.querySelector('.runpage'), dlg = document.getElementById('fiche');
  if (!page || !dlg) return;
  const body = dlg.querySelector('.body');
  async function open(id) {
    body.innerHTML = '<p class=muted>chargement…</p>';
    if (!dlg.open) dlg.showModal();
    try {
      const r = await fetch(page.dataset.fiche + '?id=' + encodeURIComponent(id));
      body.innerHTML = r.ok ? await r.text() : '<p class=muted>fiche introuvable (' + r.status + ')</p>';
    } catch (e) { body.innerHTML = '<p class=muted>la fiche n’a pas pu être lue</p>'; }
  }
  document.addEventListener('click', e => {
    const a = e.target.closest('[data-person]');
    if (!a) return;
    e.preventDefault(); open(a.dataset.person);
  });
  dlg.querySelector('.close').addEventListener('click', () => dlg.close());
  dlg.addEventListener('click', e => { if (e.target === dlg) dlg.close(); });
})();
"""

TOC_JS = """
(() => {
  const links = [...document.querySelectorAll('.toc a')];
  const secs = links.map(a => document.getElementById(a.hash.slice(1))).filter(Boolean);
  if (!('IntersectionObserver' in window)) return;
  const io = new IntersectionObserver(es => es.forEach(e => {
    if (e.isIntersecting) links.forEach(a => a.classList.toggle('on', a.hash === '#' + e.target.id));
  }), {rootMargin: '0px 0px -70% 0px'});
  secs.forEach(s => io.observe(s));
})();
"""


def toc_html(items):
    return ('<nav class=toc aria-label="sommaire"><b>Sur cette page</b>'
            + "".join(f'<a href="#{k}"{" class=sub" if sub else ""}>{h(v)}</a>' for k, v, sub in items) + "</nav>")


# --- the summaries: a few short points, a figure in bold and half a sentence each --------
# The lead, 23 November 2026: "un résumé simple et rapide… les tendances, les grandes données
# importantes… tout en haut de la page, et puis au début de chaque section… pas juste des gros
# paragraphes." Every point is worked out from the data, never written by hand; a point holds at
# most POINT_WORDS words; the top holds TOP_POINTS[0] to TOP_POINTS[1] points, a section
# SECTION_POINTS[0] to SECTION_POINTS[1].

POINT_WORDS = 15
TOP_POINTS = (4, 6)
SECTION_POINTS = (2, 4)


def num(n):
    """A figure in a point: `1 035`, the thousands kept together by a thin no-break space."""
    return fmt(n).replace(" ", THIN)


def point(bold, rest="", series=None):
    """One point: the figure in bold, half a sentence, and a little curve when it is a series in time."""
    return {"b": bold, "t": rest, "series": series}


def point_words(p):
    """The words of a point, as a reader counts them (a figure is one word)."""
    return len([w for w in f"{p['b']} {p['t']}".replace("(", " ").replace(")", " ").split(" ") if w.strip(" :;,.")])


def trend_of(values):
    """`up`, `down` or `flat`: the last value against the first, flat within 5 %."""
    xs = [v for v in values if v is not None]
    if len(xs) < 2:
        return None
    a, b = xs[0], xs[-1]
    if abs(b - a) <= 0.05 * max(abs(a), abs(b), 1):
        return "flat"
    return "up" if b > a else "down"


def spark(values):
    """A little curve of a series in time, its arrow beside; nothing for fewer than two values."""
    xs = [v for v in values if v is not None]
    tr = trend_of(xs)
    if tr is None:
        return ""
    lo, hi = min(xs), max(xs)
    span = (hi - lo) or 1
    W, H = 64, 16
    pts = " ".join(f"{2 + (W - 4) * i / (len(xs) - 1):.1f},{H - 2 - (H - 4) * (v - lo) / span:.1f}" for i, v in enumerate(xs))
    last = pts.split(" ")[-1].split(",")
    arrow = {"up": "↗", "down": "↘", "flat": "→"}[tr]
    words = {"up": "monte", "down": "baisse", "flat": "stable"}[tr]
    return (f'<span class=spark title="{words}, de {h(fmt(xs[0]))} à {h(fmt(xs[-1]))}"><svg viewBox="0 0 {W} {H}" aria-hidden=true>'
            f'<polyline points="{pts}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>'
            f'<circle cx="{last[0]}" cy="{last[1]}" r="2" fill="currentColor"/></svg><span class=ar>{arrow}</span>'
            f'<span class=sr>{words}</span></span>')


def points_html(points, cls="points"):
    """The points as a short list; the names in them made links to their cards."""
    if not points:
        return ""
    return (f'<ul class="{cls}">' + "".join(f'<li><b>{link_people(h(p["b"]))}</b>{"" if p["t"][:1] in ",." else " "}{link_people(h(p["t"]))}'
                                           f'{spark(p["series"]) if p.get("series") else ""}</li>' for p in points) + "</ul>")


def per_year(d, fn, tribe=None):
    """A value per year, summed over the tribes (or one tribe), the years in order; None where no tribe has it."""
    out = []
    for yr in range(1, last_year(d) + 1):
        vals = [fn(y) for y in d.get("years", []) if y.get("year") == yr and (tribe is None or y.get("tribe") == tribe)]
        vals = [v for v in vals if v is not None]
        out.append(sum(vals) if vals else None)
    return out


def population_series(d, sm):
    return [sm["start"]] + per_year(d, lambda y: y.get("population"))


def noon_food_series(d):
    return per_year(d, lambda y: ((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean"))


def places_series(d, t):
    return per_year(d, lambda y: (y.get("knowledge") or {}).get("places"), t)


def kids_dead(d):
    yd = year_days(d)
    return sum(1 for p in d.get("people", []) if p.get("died") and (age_days(d, p, p["died"].get("day")) or 0) < 15 * yd)


def ratio_fr(a, b):
    """`2×`, `1,6×`: how many times a holds b."""
    r = a / b
    return f"{round(r)}×" if r >= 1.95 else f"{r:.1f}×".replace(".", ",")


def camps_point(d, sm):
    ts = tribes(d)
    moved = {t: camp_stays(d, t) for t in ts}
    movers = [t for t in ts if len(moved[t]) > 1]
    if not movers:
        return point("aucun camp n’a bougé", f"en {plural(sm['years'], 'an')}")
    t = max(movers, key=lambda t: len(moved[t]))
    when = ", ".join(f"an {a}" for a, _, _ in moved[t][1:4])
    return point(f"camp de {t} déplacé {len(moved[t]) - 1} fois", f"({when})")


def places_point(d, bt):
    ts = [t for t in tribes(d) if bt[t]["places"] is not None]
    if len(ts) < 2:
        return None
    hi, lo = sorted(ts, key=lambda t: -(bt[t]["places"] or 0))[:2]
    a, b = bt[hi]["places"] or 0, bt[lo]["places"] or 0
    if b and a / b >= 1.3:
        return point(f"{hi} connaît {ratio_fr(a, b)} plus de lieux", f"que {lo} ({num(a)} et {num(b)})", places_series(d, hi))
    return point(f"{hi} et {lo} connaissent autant de lieux", f"({num(a)} et {num(b)})", places_series(d, hi))


def deaths_point(d, sm):
    if not sm["dead"]:
        return point("personne n’est mort", f"en {plural(sm['years'], 'an')}")
    kids = kids_dead(d)
    if not sm["want"]:
        main = max(sm["background_by"], key=sm["background_by"].get)
        rest = f": {num(sm['background_by'][main])} de {cause_fr(main)}" + (f", dont {num(kids)} enfants" if kids else "")
        return point("0 mort de manque", rest)
    by = ", ".join(f"{num(n)} de {cause_fr(c)}" for c, n in sm["want_by"].items() if n)
    return point(f"{plural(sm['want'], 'mort')} de manque", f": {by}")


def top_points(d, sm, bt, dc=None, label=None):
    """The top of the page: four to six points, by what counts most: the living, the deaths, the
    control's gap, what they learnt, what they know, the camps; then the births and the rumours."""
    ts = tribes(d)
    gap = sm["alive"] - sm["start"]
    pop = population_series(d, sm)
    if len(ts) > 1:
        gains = {t: bt[t]["alive"] - bt[t]["start"] for t in ts}
        who = f", {' et '.join(ts)} à égalité" if len(set(gains.values())) == 1 else f", surtout {max(gains, key=gains.get)}"
    else:
        who = ""
    pts = [point(f"{plural(sm['alive'], 'vivant')}", f"({'+' if gap >= 0 else '−'}{num(abs(gap))} en {plural(sm['years'], 'an')}{who})", pop),
           deaths_point(d, sm)]
    if dc is not None:
        btc = by_tribe(dc)
        a = mean_of([x for t in bt for x in bt[t]["noon"]])
        b = mean_of([x for t in btc for x in btc[t]["noon"]])
        p = pct(a, b)
        if p is not None:
            pts.append(point(f"{'+' if p >= 0 else '−'}{abs(p)} % de nourriture portée", f"à midi, {label.replace('sans', 'avec', 1)}",
                             noon_food_series(d)))
    pts.append(point(f"{plural(sm['trials'], 'essai')}", f", {num(sm['kept'])} gardés, {plural(sm['copied'], 'copie')}"
                     + (" entendues au feu" if sm["copied"] and sm["heard_copies"] == sm["copied"] else "")))
    pl = places_point(d, bt)
    if pl:
        pts.append(pl)
    pts.append(camps_point(d, sm))
    if sm["births"]:
        pts.append(point(f"{plural(sm['births'], 'naissance')}", f", {num(sm['alive'] - sm['alive_start'])} encore en vie"))
    return pts[:TOP_POINTS[1]]


def people_points(d, sm, bt):
    ts = tribes(d)
    pts = []
    if len(ts) > 1:
        pts.append(point(", ".join(f"{t} {num(bt[t]['alive'])}" for t in ts) + " vivants",
                         f"sur {num(sm['start'] + sm['births'])} personnes en tout", population_series(d, sm)))
    else:
        pts.append(point(f"{plural(sm['alive'], 'vivant')}", f"sur {num(sm['start'] + sm['births'])} personnes en tout", population_series(d, sm)))
    pts.append(point(f"{plural(sm['births'], 'naissance')} pour {plural(sm['dead'], 'mort')}",
                     f"; {num(sm['alive'] - sm['alive_start'])} enfants nés en route vivent encore"))
    kids = kids_dead(d)
    if kids:
        pts.append(point(f"{plural(kids, 'enfant')} mort{'s' if kids > 1 else ''}", "avant 15 ans"))
    return pts[:SECTION_POINTS[1]]


def deaths_points(d, sm):
    dead = [p for p in d.get("people", []) if p.get("died")]
    if not dead:
        return [point("personne n’est mort", f"en {plural(sm['years'], 'an')}"), point("0 mort de manque", "ni de fond")]
    yd = year_days(d)
    ages = sorted(age_days(d, p, p["died"].get("day")) or 0 for p in dead)
    med = ages[len(ages) // 2] // yd
    by_year = {}
    for p in dead:
        yr = (p["died"].get("day", 1) - 1) // yd + 1
        by_year[yr] = by_year.get(yr, 0) + 1
    worst = max(by_year, key=lambda y: (by_year[y], -y))
    pts = [point(f"{plural(len(dead), 'mort')}", f": {num(sm['want'])} de manque, {num(sm['background'])} de fond"),
           point(f"{num(med)} ans", "d’âge médian à la mort"),
           point(f"an {worst}", f", le plus dur : {plural(by_year[worst], 'mort')}",
                 [by_year.get(y, 0) for y in range(1, last_year(d) + 1)])]
    return pts


def births_points(d, sm, bt):
    born = [p for p in d.get("people", []) if p.get("born") != "start"]
    if not born:
        return [point("aucune naissance", f"en {plural(sm['years'], 'an')}"), point(f"{plural(sm['alive'], 'vivant')}", "à la fin")]
    yd = year_days(d)
    by_year = {}
    for p in born:
        yr = ((p.get("born_day") or p.get("born") or 1) - 1) // yd + 1
        by_year[yr] = by_year.get(yr, 0) + 1
    best = max(by_year, key=lambda y: (by_year[y], -y))
    pts = [point(f"{plural(len(born), 'naissance')}", f", {num(sum(1 for p in born if not p.get('died')))} en vie à la fin"),
           point(f"an {best}", f", le plus fécond : {plural(by_year[best], 'naissance')}",
                 [by_year.get(y, 0) for y in range(1, last_year(d) + 1)])]
    ts = tribes(d)
    if len(ts) > 1:
        pts.append(point(", ".join(f"{t} {num(bt[t]['births'])}" for t in ts), "naissances par tribu"))
    return pts


def everyone_points(d, sm):
    people = d.get("people", [])
    wills = [(p.get("will_start"), p.get("will")) for p in people if p.get("will_start") is not None and p.get("will") is not None]
    pts = [point(f"{num(sm['alive'])} vivants sur {num(len(people))}", "personnes, les vivants d’abord")]
    if wills:
        a = round(sum(w[0] for w in wills) / len(wills))
        b = round(sum(w[1] for w in wills) / len(wills))
        pts.append(point(f"volonté moyenne {num(a)} → {num(b)}", "du début à la fin"))
    top = max(people, key=lambda p: (p.get("children") or 0, -(p.get("id") or 0)), default=None)
    if top and top.get("children"):
        pts.append(point(f"{person_fr(top.get('id'))} : {plural(top['children'], 'enfant')}", ", le plus de descendants"))
    if len(pts) < 2:
        pts.append(point(plural(sm["dead"], "mort"), f"et {plural(sm['births'], 'naissance')} en tout"))
    return pts


def where_points(d, sm, bt):
    ts = tribes(d)
    pts = [camps_point(d, sm)]
    if all(bt[t]["places"] is not None for t in ts):
        pts.append(point(", ".join(f"{t} {num(bt[t]['places'])}" for t in ts) + " lieux connus", "des vivants à la fin"))
    k = knowledge_at_end(d)
    if k and k["heard_share"] is not None:
        pts.append(point(f"{round(100 * k['heard_share'])} % des souvenirs", "viennent d’un conteur"))
    if len(pts) < 2:
        pts.append(point(f"{len(ts)} tribu{'s' if len(ts) > 1 else ''}", ", ".join(f"{t} {tribe_side(d, t)}" for t in ts)))
    return pts


def knowledge_points(d):
    pts = []
    for t in tribes(d):
        s = places_series(d, t)
        xs = [v for v in s if v is not None]
        if xs:
            pts.append(point(f"{t} : {num(xs[0])} → {num(xs[-1])} lieux", f"connus, an 1 à an {last_year(d)}", s))
    food = noon_food_series(d)
    xs = [v for v in food if v is not None]
    if xs:
        pts.append(point(f"{num(round(xs[-1]))} kJ portés à midi", "en moyenne, la dernière année", food))
    return pts[:SECTION_POINTS[1]] or [point("aucun lieu", "lu dans ce run"), point("aucune nourriture", "lue à midi")]


def learnt_points(d, sm):
    pts = [point(f"{plural(sm['trials'], 'essai')}, {num(sm['kept'])} gardés",
                 f"({round(100 * sm['kept'] / sm['trials']) if sm['trials'] else 0} %)",
                 per_year(d, lambda y: y.get("trials")))]
    held = carried(d)
    if held:
        k = max(held, key=held.get)
        pts.append(point(kind_fr(k), f", la façon portée par le plus de vivants ({num(held[k])})"))
    if sm["copied"]:
        pts.append(point(f"{plural(sm['copied'], 'copie')}", "toutes par ouï-dire au feu" if sm["heard_copies"] == sm["copied"]
                         else f"dont {num(sm['heard_copies'])} par ouï-dire au feu"))
    if len(pts) < 2:
        pts.append(point("aucune copie", "entre voisins"))
    return pts


def techniques_points(d):
    pts = []
    for t in tribes(d):
        held = carried(d, t)
        if held:
            k = max(held, key=held.get)
            pts.append(point(f"{t} : {plural(len(held), 'technique')} portée{'s' if len(held) > 1 else ''}",
                             f"surtout {kind_fr(k)} ({num(held[k])})"))
        else:
            pts.append(point(f"{t} : aucune technique", "portée à la fin"))
    return pts[:SECTION_POINTS[1]]


def dared_points(d):
    people = [p for p in d.get("people", []) if p.get("trials")]
    if not people:
        return [point("personne", "n’a rien essayé"), point("0 essai", "gardé")]
    top = max(people, key=lambda p: (len(p["trials"]), -(p.get("id") or 0)))
    pts = [point(f"{person_fr(top.get('id'))} a osé {len(top['trials'])} fois",
                 f"et gardé {num(sum(1 for t in top['trials'] if t.get('kept')))}")]
    bands = {}
    for f in d.get("found") or []:
        for b in f.get("by_will") or []:
            x = bands.setdefault(b.get("band"), [0, 0])
            x[0] += b.get("tried") or 0
            x[1] += b.get("kept") or 0
    if bands.get("bold") and bands["bold"][0] and bands.get("timid") and bands["timid"][0]:
        pts.append(point(f"{round(100 * bands['bold'][1] / bands['bold'][0])} % gardés chez les aventuriers",
                         f"contre {round(100 * bands['timid'][1] / bands['timid'][0])} % chez les frileux"))
    wills = [(p.get("will_start"), p.get("will")) for p in people if p.get("will_start") is not None and p.get("will") is not None]
    if wills:
        pts.append(point(f"volonté {num(round(sum(w[0] for w in wills) / len(wills)))} → {num(round(sum(w[1] for w in wills) / len(wills)))}",
                         "en moyenne chez ceux qui ont essayé"))
    if len(pts) < SECTION_POINTS[0]:
        pts.append(point(f"{num(len(people))} sur {num(len(d.get('people', [])))}", "ont essayé au moins une fois"))
    return pts[:SECTION_POINTS[1]]


def chronicle_points(d, year, events):
    """The year's points: how much happened, then its two biggest kinds, with their busiest day."""
    pts = [point(f"{plural(len(events), 'événement')}", f"en l’an {year}")]
    by = {}
    for e in events:
        by.setdefault(e["type"], []).append(e)
    for typ in sorted(by, key=lambda k: -len(by[k]))[:SECTION_POINTS[1] - 1]:
        one, many = EV_GROUP[typ]
        days = {}
        for e in by[typ]:
            days[e["day"]] = days.get(e["day"], 0) + 1
        top = max(days, key=lambda k: (days[k], -k))
        pts.append(point(f"{num(len(by[typ]))} {one if len(by[typ]) == 1 else many}",
                         f"dont {num(days[top])} le jour {top}" if days[top] > 1 and len(days) > 1 else f"le jour {top}" if len(days) == 1 else ""))
    if len(pts) < 2:
        pts.append(point("rien", "cette année-là"))
    return pts


def chronicle_json(d, year):
    """The lab's answer for one year of the chronicle: its sentence, its chips, its seasons."""
    year = min(max(1, year or 1), last_year(d) or 1)
    lead, chips, seasons = chronicle_year(d, year)
    return json.dumps({"lead": lead, "chips": chips, "seasons": seasons}, ensure_ascii=False)


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


def run_body(s, r, d, store=None, year=None):
    """The page of one run: `s` the series (from Lab.series()), `r` its run's state, `d` its data or None;
    `store` finds the control run's data for the arrows, `year` the chronicle's year."""
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
    bt = by_tribe(d)
    rc, label = control_of(s, r)
    dc = store.load(s["name"], rc["name"]) if (rc is not None and store is not None) else None
    smc = summary(dc) if dc is not None else None
    if smc is None:
        label, dc = None, None
    else:
        smc["rumours_all"] = sum(v["rumours"] or 0 for v in by_tribe(dc).values()) if has_fire_data(dc) else None
    fire = has_fire_data(d)
    toc = [("resume", "Résumé", False), ("gens", "Les gens", False), ("morts", "Les morts", True), ("naissances", "Les naissances", True),
           ("tous", "Tout le monde", True), ("ou", "Où ils vivent", False), ("appris", "Ce qu’ils ont appris", False),
           ("chronique", "La chronique", False), ("annexes", "Annexes", False)]
    ctrl = (f'<p class=caption>Les pastilles comparent au témoin de la même série, <a href="{run_url(serie, rc["name"])}">{h(rc["name"])}</a> '
            f'({h(label)}) : vert mieux, rouge moins bien, gris ni l’un ni l’autre. Sous chaque total, '
            + " | ".join(f"{tribe_chip(t)} tribu {h(t)}, {h(tribe_side(d, t))}" for t in ts) + ".</p>") if smc else ""
    down = lambda html_: html_.replace("<h2>", "<h3>").replace("</h2>", "</h3>")  # noqa: E731
    body = [f'<section id=resume><p class=hero>{h(headline(d, sm, smc, label))}</p>'
            f'{points_html(top_points(d, sm, bt, dc, label), "points top")}{run_tiles(d, sm, bt, smc, label)}{ctrl}</section>']
    body.append('<section id=gens class=group><h2>Les gens</h2>' + points_html(people_points(d, sm, bt))
                + "<h3>La population</h3><p class=caption>Les vivants jour par jour, par tribu ; les hivers en bleu pâle, les années en bas.</p>"
                + population_chart(d, wide=True).svg("population par tribu")
                + "<p class=caption>Dessous, les morts de chaque année par tribu : en rouge ceux de manque (faim, soif, froid), en gris ceux "
                "de fond (maladie, vieillesse, couches) ; le contour dit la tribu, A à gauche de l’année"
                + (", B à droite" if len(ts) > 1 else "") + ".</p>" + want_chart(d, wide=True).svg("morts de manque et de fond par an")
                + '<section id=morts><h3>Les morts</h3>' + points_html(deaths_points(d, sm))
                + '<p class=caption>Cliquez un titre de colonne pour trier, un nom pour la fiche. '
                '<i class="g c-want">■</i> mort de manque (faim, soif, froid) · <i class="g c-bg">■</i> mort de fond (maladie, vieillesse, couches) ; '
                'la barre dit l’âge.</p>' + deaths_table_v2(d) + "</section>"
                + "<section id=naissances><h3>Les naissances</h3>" + points_html(births_points(d, sm, bt)) + births_table_v2(d) + "</section>"
                + "<section id=tous><h3>Tout le monde</h3>" + points_html(everyone_points(d, sm)) + people_table_v2(d) + "</section></section>")
    body.append('<section id=ou class=group><h2>Où ils vivent</h2>' + points_html(where_points(d, sm, bt))
                + '<div data-lazy="carte" class=lazy><p class=muted>la carte se charge…</p></div>'
                + ("<h3>Ce que la tribu sait</h3>" + points_html(knowledge_points(d)) + knowledge_body(d) if fire else "") + "</section>")
    body.append('<section id=appris class=group><h2>Ce qu’ils ont appris</h2>' + points_html(learnt_points(d, sm))
                + "<h3>Les techniques</h3>" + points_html(techniques_points(d)) + techniques_html(d)
                + ("<h3>Qui a osé quoi</h3>" + points_html(dared_points(d)) + dared_body(d) if fire else "") + "</section>")
    body.append(chronicle_html(d, year, f"/chronique/{urllib.parse.quote(serie)}/{urllib.parse.quote(name)}"))
    body.append('<section id=annexes class=group><h2>Annexes</h2><details class=detail><summary>Les tableaux bruts</summary>'
                '<div data-lazy="annexes"><p class=muted>les annexes se chargent…</p></div></details></section>')
    body.append('<dialog id=fiche class=fiche aria-label="fiche"><div class=in><button type=button class=close>fermer</button><div class=body></div></div></dialog>')
    body.append(f"<script>{TABLE_JS}{CHRON_JS}{MAP2_JS}{LAZY_JS}{FICHE_JS}{TOC_JS}</script>")
    base = f"{urllib.parse.quote(serie)}/{urllib.parse.quote(name)}"
    return (head + f'<div class=runpage data-fiche="/fiche/{h(base)}" data-morceau="/morceau/{h(base)}">{toc_html(toc)}'
            f'<div class=runmain>{"".join(body)}</div></div>')


def run_piece(d, what):
    """A piece of the run page the lab sends when it is asked: `carte` the map, `annexes` the raw
    tables; None for anything else. The page carries neither: they weigh most."""
    if what == "carte":
        return map_v2(d, "m2")
    if what != "annexes":
        return None
    ts = tribes(d)
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
    detail.append(f"<h3>La vie de chacun, l’ancien tableau ({len(d.get('people', []))})</h3>" + people_table(d))
    return "".join(detail)


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
