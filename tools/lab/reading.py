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
TRIALS_COLOR = {True: "#2b7a3d", False: "#9c6b1f"}  # with trials, without
BIRTH_COLOR = "#3a8f4a"
NO_DATA = "pas de données, relancer avec le vivarium récent"

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
    return {
        "alive": sum(1 for p in people if not p.get("died")),
        "dead": sum(deaths.values()),
        "deaths": {c: deaths.get(c, 0) for c in causes(d)},
        "births": sum(1 for p in people if p.get("born") != "start"),
        "start": sum(1 for p in people if p.get("born") == "start"),
        "trials": totals["tried"], "kept": totals["kept"], "copied": totals["copied"],
        "homes_moved": sum(y.get("homes_moved") or 0 for y in d.get("years", [])),
        "years": last_year(d),
        "days": (d.get("meta") or {}).get("settings", {}).get("days"),
    }


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

    def __init__(self, xlo, xhi, ytop, xlabel, ylabel, year_days=50, pct=False):
        self.xlabel, self.ylabel = xlabel, ylabel
        self.year_days = year_days
        self.x = Scale(xlo, xhi, self.L, self.W - self.R)
        self.ytop = nice_max(ytop) if not pct else 100
        self.y = Scale(0, self.ytop, self.H - self.B, self.T)
        self.parts = []
        self.legend = []
        self.xlo, self.xhi = xlo, xhi

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


def population_chart(d, label=None, color_of=None, dash=False, chart=None, hover=True):
    """Alive per tribe over the days; `color_of(tribe, i)` picks the colour."""
    c = d.get("curves") or {}
    days = c.get("days") or []
    by_tribe = c.get("alive_by_tribe") or []
    ts = tribes(d)
    top = max((max(v) for v in by_tribe if v), default=1)
    if chart is None:
        chart = Chart(0, max(days[-1] if days else 1, year_days(d)), top, "années", "vivants", year_days(d))
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
  const groups = m.querySelectorAll('g[data-year]'), buttons = m.querySelectorAll('button[data-year]');
  const show = y => { groups.forEach(g => g.style.display = g.dataset.year === y ? 'inline' : 'none'); buttons.forEach(b => b.classList.toggle('on', b.dataset.year === y)); };
  buttons.forEach(b => b.onclick = () => show(b.dataset.year));
});
"""


def map_svg(d, uid, side="", show_deaths=True):
    """The map's picture with, per tribe, the camps' trail by year, the presence heatmap
    of a chosen year (buttons; the last year without script) and the deaths' places."""
    m = d.get("map") or {}
    uri = m.get("data_uri")
    tiles = m.get("tiles") or (d.get("meta") or {}).get("map_tiles")
    if not uri or not tiles or not str(uri).startswith("data:image/png;base64,"):
        return f'<p class="muted">{NO_DATA} (pas de carte dans le fichier)</p>'
    W, Hh = tiles
    fs = max(8, W / 60)
    ts = tribes(d)
    parts = [f'<svg viewBox="0 0 {W} {Hh}" class="map" role="img" aria-label="carte du run">',
             f'<image href="{h(uri)}" x="0" y="0" width="{W}" height="{Hh}" style="image-rendering:pixelated"/>']
    # presence heatmap, one group a year, the last one shown
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
    # deaths' places
    if show_deaths:
        for p in d.get("people", []):
            died = p.get("died")
            if died and died.get("at"):
                x, y = died["at"]
                r = fs * 0.5
                parts.append(f'<g stroke="#111" stroke-width="{fs / 8:.2f}"><line x1="{x - r}" y1="{y - r}" x2="{x + r}" y2="{y + r}"/>'
                             f'<line x1="{x - r}" y1="{y + r}" x2="{x + r}" y2="{y - r}"/>'
                             f'<title>{h(person_fr(p.get("name", p.get("id"))))} ({h(p.get("tribe", ""))}) meurt de {h(cause_fr(died.get("cause", "?")))} le jour {died.get("day")}</title></g>')
    # camp trail, one a tribe
    for i, t in enumerate(ts):
        color = tribe_color(t, i)
        pts = [(y["year"], y["camp"]) for y in years_of(d, t) if y.get("camp")]
        camps = (d.get("meta") or {}).get("camps") or []
        if i < len(camps):
            pts.insert(0, (0, camps[i]))
        if len(pts) > 1:
            parts.append(f'<polyline fill="none" stroke="{color}" stroke-width="{fs / 5:.2f}" stroke-opacity="0.9" '
                         f'points="{" ".join(f"{p[0]},{p[1]}" for _, p in pts)}"/>')
        for yr, (x, y) in pts:
            parts.append(f'<circle cx="{x}" cy="{y}" r="{fs / 3:.2f}" fill="{color}" stroke="#fff" stroke-width="{fs / 10:.2f}">'
                         f'<title>tribu {h(t)}, {"départ" if yr == 0 else f"fin de l’an {yr}"} : camp en ({x}, {y})</title></circle>')
            parts.append(f'<text x="{x + fs * 0.45:.1f}" y="{y - fs * 0.3:.1f}" font-size="{fs:.1f}" fill="{color}" '
                         f'stroke="#fff" stroke-width="{fs / 6:.2f}" paint-order="stroke" font-weight="700">{yr}</text>')
    parts.append("</svg>")
    buttons = "".join(f'<button type="button" data-year="{yr}" class="{"on" if yr == years[-1] else ""}">an {yr}</button>'
                      for yr in years)
    legend = " · ".join(f'<span style="color:{tribe_color(t, i)}">■ tribu {h(t)}</span>' for i, t in enumerate(ts))
    return (f'<div class="mapbox" data-map="{h(uid)}">{"".join(parts)}'
            f'<p class="muted">{legend} · chemin des camps année par année (le chiffre est l’année), '
            f'présence à midi de l’année choisie (plus foncé : plus souvent), × les morts{" " + h(side) if side else ""}.</p>'
            f'<p class="years">{buttons}</p></div>')


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
    """The discoveries of a tribe in plain French sentences."""
    found = next((f for f in d.get("found", []) if f.get("tribe") == tribe), {})
    tried = sum((found.get("tried") or {}).values())
    kept = sum((found.get("kept") or {}).values())
    copied = sum((found.get("copied") or {}).values())
    disc = discoveries(d, tribe)
    s = (d.get("meta") or {}).get("settings") or {}
    lines = [f"<p><b>Tribu {h(tribe)}</b> : {tried} essai{'s' if tried > 1 else ''}, {kept} gardé{'s' if kept > 1 else ''}, "
             f"{copied} copié{'s' if copied > 1 else ''}"
             + (" — les épreuves étaient coupées (témoin)." if not s.get("trials", True) else ".") + "</p>"]
    kept_kinds = [x for x in disc if x["kept"]]
    lost_kinds = [x for x in disc if not x["kept"] and x["tried"]]
    if kept_kinds:
        items = []
        for x in kept_kinds:
            first = (f"gardé la première fois le jour {x['first'][0]} par la {h(person_fr(x['first'][1]))}"
                     if x["first"] else "gardé (par qui et quand : non lu)")
            use = (f"{x['holders']} vivant{'s' if x['holders'] > 1 else ''} l’{'ont' if x['holders'] > 1 else 'a'} encore pour dernier essai gardé à la fin"
                   if x["holders"] else "plus personne ne l’a pour dernier essai gardé à la fin")
            items.append(f"<li><b>{h(kind_fr(x['kind']).capitalize())}</b> : {first} ; essayé {x['tried']} fois, gardé {x['kept']}, "
                         f"copié {x['copied']} ; {use}.</li>")
        lines.append("<p>Ce que les recherches ont apporté :</p><ul>" + "".join(items) + "</ul>")
    elif tried:
        lines.append("<p>Les recherches n’ont rien apporté : aucun essai n’a passé le jugement.</p>")
    else:
        lines.append("<p>Aucune recherche.</p>")
    if lost_kinds:
        lines.append("<p>Essayé sans succès : " + ", ".join(
            f"{h(kind_fr(x['kind']))} ({x['tried']} essai{'s' if x['tried'] > 1 else ''}"
            + (f", mais {x['copied']} copie{'s' if x['copied'] > 1 else ''} d’un voisin qui l’avait déjà" if x["copied"] else "") + ")"
            for x in lost_kinds) + ".</p>")
    lines.append(f"<p><b>La culture de la tribu à la fin</b> : {h(culture_line(d, tribe))}.</p>")
    return "".join(lines)


def found_table(d, tribe):
    disc = discoveries(d, tribe)
    if not disc:
        return '<p class="muted">rien d’essayé</p>'
    rows = "".join(f"<tr><td>{h(kind_fr(x['kind']))}</td><td class=n>{x['tried']}</td><td class=n>{x['kept']}</td>"
                   f"<td class=n>{x['copied']}</td><td class=n>{x['holders']}</td></tr>" for x in disc)
    return f"<table><tr><th>façon</th><th>essayé</th><th>gardé</th><th>copié</th><th>portée à la fin</th></tr>{rows}</table>"


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


# --- the tables -------------------------------------------------------------------

def years_table(d):
    cs = causes(d)
    head = ("<tr><th>an</th><th>tribu</th><th>début → fin</th><th>‰</th><th>naiss.</th><th>morts</th>"
            + "".join(f"<th>{h(cause_fr(c))}</th>" for c in cs)
            + "<th>15 ans</th><th>femmes seules</th><th>essais / gardés / copiés</th><th>maisons</th><th>camp</th>"
            "<th>cueilli kJ</th><th>mangé</th><th>gâté</th><th>perdu</th><th>fin</th><th>j. maigres</th><th>j. faibles</th></tr>")
    rows = []
    for y in years_of(d):
        f = y.get("food") or {}
        ft = y.get("fifteen") or {}
        fw = y.get("fertile_women") or {}
        camp = f"({y['camp'][0]}, {y['camp'][1]})" if y.get("camp") else "-"
        g = y.get("growth_permille")
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
                    f"<td class=n>{fmt(hours_to_days(f.get('thin_hours')))}</td><td class=n>{fmt(hours_to_days(f.get('weak_hours')))}</td></tr>")
    note = ("<p class=muted>Une ligne par an et par tribu. ‰ : croissance pour mille ; 15 ans : vivants / arrivés à quinze ans parmi les nés du run ; "
            "femmes seules : sans époux possible / femmes de 16 à 42 ans ; jours maigres et faibles : heures-personne sous la moitié "
            "et sous le cinquième de la réserve, divisées par 24.</p>")
    return f'<div class=scroll><table class=years>{head}{"".join(rows)}</table></div>{note}'


def hours_to_days(hours):
    if hours is None:
        return None
    return round(hours / 24, 1)


def people_table(d):
    rows = []
    for p in sorted(d.get("people", []), key=lambda p: (p.get("tribe", ""), p.get("id", 0))):
        died = p.get("died")
        death = (f"jour {died.get('day')}, {h(cause_fr(died.get('cause', '?')))}"
                 + (f" ({h(SEASON_FR.get(died.get('season'), died.get('season') or ''))})" if died.get("season") else "")) if died else "vivant"
        born = "au départ" + (f" ({p['age_days_at_start'] // 365} ans)" if p.get("age_days_at_start") is not None else "") \
            if p.get("born") == "start" else f"jour {p.get('born')}"
        ways = p.get("ways") or {}
        way_txt = (f"{'dort dehors' if ways.get('sleep_out') else 'dort au camp'}, cherche jusqu'à {fmt(ways.get('worth_half_tiles'))} tuiles, "
                   f"garde {fmt(ways.get('stock_kj'))} kJ, maison ({ways['home'][0]}, {ways['home'][1]})") if ways and ways.get("home") else "esprit non lu"
        lines = "".join(f"<li>jour {ln.get('day')} : {h(ln.get('what', ''))}</li>" for ln in p.get("lines") or [])
        trials = len(p.get("trials") or [])
        kept = sum(1 for t in p.get("trials") or [] if t.get("kept"))
        detail = (f"<details><summary>{h(person_fr(p.get('name', p.get('id'))))}</summary>"
                  f"<p class=muted>{h(way_txt)} ; {trials} essai{'s' if trials > 1 else ''}, {kept} gardé{'s' if kept > 1 else ''}, "
                  f"{fmt(p.get('copied'))} copie{'s' if (p.get('copied') or 0) > 1 else ''} ; "
                  f"{fmt(hours_to_days(p.get('thin_hours')))} j. maigres, {fmt(hours_to_days(p.get('weak_hours')))} j. faibles ; "
                  f"{fmt(p.get('gathered_kj'))} kJ cueillis, {fmt(p.get('meals'))} repas, {fmt(p.get('drinks'))} boissons.</p>"
                  f"<ul class=lines>{lines or '<li class=muted>aucune ligne</li>'}</ul></details>")
        rows.append(f"<tr><td class=n>{p.get('id')}</td><td>{h(p.get('tribe', ''))}</td><td>{h(SEX_FR.get(p.get('sex'), p.get('sex') or '-'))}</td>"
                    f"<td>{born}</td><td>{death}</td><td class=n>{fmt(p.get('children'))}</td><td class=n>{fmt(p.get('will'))}</td><td>{detail}</td></tr>")
    return (f"<table class=people><tr><th>n°</th><th>tribu</th><th>sexe</th><th>né</th><th>mort</th><th>enfants</th><th>volonté</th><th>sa vie</th></tr>"
            f"{''.join(rows)}</table>")


def cards_html(sm, other=None, label=None):
    """The summary cards; with `other`, the difference to it is highlighted."""
    def card(title, value, ref=None, sub=""):
        delta = ""
        if other is not None and ref is not None and value is not None:
            dv = value - ref
            if dv:
                delta = f'<span class="delta {"plus" if dv > 0 else "minus"}">{"+" if dv > 0 else ""}{fmt(dv)}</span>'
        return f'<div class=card><div class=k>{h(title)}</div><div class=v>{fmt(value)}{delta}</div><div class=s>{sub}</div></div>'
    o = other or {}
    deaths_sub = ", ".join(f"{h(cause_fr(c))} {n}" + (f' <span class="delta {"plus" if n - o["deaths"].get(c, 0) > 0 else "minus"}">{n - o["deaths"].get(c, 0):+d}</span>'
                                                      if other is not None and n - o["deaths"].get(c, 0) else "")
                           for c, n in sm["deaths"].items() if n or (other is not None and o.get("deaths", {}).get(c)))
    cards = [card("vivants à la fin", sm["alive"], o.get("alive"), f"{sm['start']} au départ, {sm['years']} an{'s' if sm['years'] > 1 else ''}"),
             card("morts", sm["dead"], o.get("dead"), deaths_sub or "aucun"),
             card("naissances", sm["births"], o.get("births")),
             card("essais", sm["trials"], o.get("trials"), f"gardés {sm['kept']}"
                  + (f' <span class="delta {"plus" if sm["kept"] - o["kept"] > 0 else "minus"}">{sm["kept"] - o["kept"]:+d}</span>' if other is not None and sm["kept"] != o.get("kept") else "")
                  + f", copiés {sm['copied']}"),
             card("maisons déplacées", sm["homes_moved"], o.get("homes_moved"))]
    return f'<div class=cards>{"<div class=cardlabel>" + h(label) + "</div>" if label else ""}{"".join(cards)}</div>'


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
    body = [head, f"<p class=settings>{h(settings_fr(d))}</p>", cards_html(sm)]
    body.append("<h2>Courbes</h2><div class=charts>")
    body.append(population_chart(d).svg("population par tribu"))
    body.append(births_deaths_chart(d).svg("naissances et morts par an"))
    body.append(kept_chart(d).svg("essais gardés par an"))
    body.append(reserve_chart(d).svg("réserve moyenne"))
    body.append("</div>")
    body.append("<h2>La carte</h2>" + map_svg(d, "m1"))
    body.append("<h2>Les découvertes</h2>")
    for t in ts:
        body.append(f'<section class=tribe>{story_html(d, t)}<div class=cols>{found_table(d, t)}<div><p class=muted>Les essais gardés, par jour :</p>{kept_list(d, t)}</div></div></section>')
    body.append("<h2>Année par année</h2>" + years_table(d))
    body.append(f"<h2>Les gens ({len(d.get('people', []))})</h2>" + people_table(d))
    body.append(f"<script>{MAP_JS}</script>")
    return "".join(body)


def aggregates(values):
    """Mean, min, max and standard deviation of a list; None when empty."""
    if not values:
        return None
    return {"n": len(values), "mean": statistics.fmean(values), "min": min(values), "max": max(values),
            "sd": statistics.stdev(values) if len(values) > 1 else 0.0}


def aggregates_html(agg, label):
    if not agg:
        return f"<li>{h(label)} : aucun run lu</li>"
    return (f"<li>{h(label)} : <b>{fmt(agg['mean'])}</b> vivants en moyenne sur {agg['n']} run{'s' if agg['n'] > 1 else ''} "
            f"(de {fmt(agg['min'])} à {fmt(agg['max'])}, écart-type {fmt(agg['sd'])})</li>")


def series_numbers(s, store):
    """Per run, the key numbers read from its data (None without); the alive aggregates by with/without."""
    rows = []
    for r in s["runs"]:
        d = store.load(s["name"], r["name"])
        sm = summary(d) if d else None
        rows.append((r, d, sm))
    agg = {}
    for trials in (True, False):
        vals = [sm["alive"] for r, _d, sm in rows if sm and r.get("trials", True) == trials]
        agg[trials] = aggregates(vals)
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
    deaths = {}
    births = 0
    for _r, _d, sm in rows:
        if sm:
            births += sm["births"]
            for c, n in sm["deaths"].items():
                deaths[c] = deaths.get(c, 0) + n
    if deaths:
        parts.append("morts : " + ", ".join(f"{cause_fr(c)} {n}" for c, n in sorted(deaths.items(), key=lambda x: -x[1]) if n))
    parts.append(f"{births} naissance{'s' if births > 1 else ''}")
    return " · ".join(parts)


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
    body = [head, f"<div class=controls>{versions}{runs_cmp}</div>"]
    # table
    trs = []
    for r, d, sm in rows:
        link = f'<a href="{run_url(s["name"], r["name"])}">{h(r["name"])}</a>'
        if sm:
            cells = (f"<td class=n>{fmt(sm['alive'])}</td><td class=n>{fmt(sm['dead'])}</td><td class=n>{fmt(sm['births'])}</td>"
                     f"<td class=n>{fmt(sm['kept'])} / {fmt(sm['trials'])}</td>")
        else:
            cells = f'<td colspan=4 class=muted>{NO_DATA if r["state"] not in ("en cours", "attend") else "en cours, année " + fmt(r.get("year"))}</td>'
        trs.append(f"<tr><td>{link}</td><td class=n>{r.get('seed', '')}</td><td>{trials_tag(r)}</td><td><span class=etat>{h(r['state'])}</span></td>"
                   f"{cells}<td class=n>{secs(r.get('elapsed'))}</td><td>"
                   + (f'<a href="{r["page"]}">page</a> ' if r.get("page") else "") + (f'<a href="{r["log"]}">log</a>' if r.get("log") else "") + "</td></tr>")
    body.append("<h2>Les runs</h2><table><tr><th>run</th><th>graine</th><th>épreuves</th><th>état</th><th>vivants</th><th>morts</th>"
                f"<th>naissances</th><th>gardés / essais</th><th>temps</th><th></th></tr>{''.join(trs)}</table>")
    body.append("<h2>Vivants à la fin</h2><ul>" + aggregates_html(agg[True], "avec épreuves") + aggregates_html(agg[False], "sans épreuves") + "</ul>")
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
    body.append(f'<div class=cols2>{cards_html(sma, smb, "A")}{cards_html(smb, sma, "B")}</div>'
                '<p class=muted>Sous chaque nombre, en couleur, l’écart à l’autre côté.</p>')
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
                     + (cell("alive") + cell("dead") + cell("births") + cell("kept") + cause_cells if sma or smb
                        else f'<td colspan={5 + len(cs)} class=muted>{NO_DATA}</td>')
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
    head = ("<tr><th>run</th><th>graine</th><th>épreuves</th><th>vivants A → B</th><th>morts</th><th>naissances</th><th>gardés</th>"
            + "".join(f"<th>{h(cause_fr(c))}</th>" for c in all_causes) + "<th></th></tr>")
    body.append(f"<h2>Run par run</h2><div class=scroll><table>{head}{''.join(r for _, r in rows)}</table></div>")
    if left:
        body.append("<p class=muted>Sans pair en A : " + ", ".join(h(r["name"]) for r in left) + ".</p>")
    agg_a = series_numbers(sa, store)[1]
    agg_b = series_numbers(sb, store)[1]
    body.append("<h2>Vivants à la fin</h2><div class=cols2><div><h3>A</h3><ul>" + aggregates_html(agg_a[True], "avec épreuves") + aggregates_html(agg_a[False], "sans épreuves")
                + "</ul></div><div><h3>B</h3><ul>" + aggregates_html(agg_b[True], "avec épreuves") + aggregates_html(agg_b[False], "sans épreuves") + "</ul></div></div>")
    if chart:
        chart.legend_item(SIDE_COLOR["a"], "A, avec épreuves")
        chart.legend_item(SIDE_COLOR["a"], "A, sans", dash=True)
        chart.legend_item(SIDE_COLOR["b"], "B, avec épreuves")
        chart.legend_item(SIDE_COLOR["b"], "B, sans", dash=True)
        body.append("<h2>Population de tous les runs</h2>" + chart.svg("population de tous les runs"))
    if cultures:
        body.append("<h2>Les cultures à la fin</h2><table><tr><th>run</th><th>A</th><th>B</th></tr>" + "".join(cultures) + "</table>")
    return "".join(body)
