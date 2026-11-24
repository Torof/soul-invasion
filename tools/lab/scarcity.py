"""La chaîne du manque (24 November 2026): eight measures of a run, on the series page and the run page.

The project lead, 24 November 2026, on the list: "c'est les bonnes métriques… Déménager, l'épuisement,
aller loin très important, la vie, les couples, explorer très important". Each measure is read from the
run's data (`docs/10-vivarium-data.md`): most from what the vivarium already wrote, the rest from its
`chain` key (the need, the walk, the work, why a body lies away at dawn, how far a look goes). A run
written before a key shows "—" where it is needed. Words: « journée » a game day, « cycle » the four
seasons, « an » only for ages, « heure » a game hour (1/24 of a journée). Standard library only.
"""

import html
import math
import statistics

import reading

h = html.escape
fmt = reading.fmt
NONE = "—"
SEASONS = reading.SEASON_ALL


# --- the measures ----------------------------------------------------------------------------------

def _num(x):
    return x if isinstance(x, (int, float)) and not isinstance(x, bool) else 0


def one(x):
    """A float to one decimal (shown `2,5`), or None."""
    return None if x is None else round(float(x), 1)


def p90(values):
    """The distance beyond which the farthest 10 % stand (nearest rank), or None."""
    if not values:
        return None
    s = sorted(values)
    return s[max(0, math.ceil(0.9 * len(s)) - 1)]


def chain_years(d):
    c = d.get("chain")
    ys = c.get("years") if isinstance(c, dict) else None
    return ys if isinstance(ys, list) else None


def noon_distances(d):
    """The bodies' distances at noon to their tribe's camp of that year (else of the year before), on the
    longer axis, by season of the year (`reading.season_of`): `{season: [metres, …]}`."""
    dpy = reading.year_days(d)
    tribe = {p.get("id"): p.get("tribe") for p in d.get("people", []) if p.get("id") is not None}
    camps = {(y.get("tribe"), y.get("year")): y.get("camp") for y in d.get("years", []) if y.get("camp")}
    out = {s: [] for s in SEASONS}
    for entry in d.get("positions") or []:
        if not isinstance(entry, list) or len(entry) != 2:
            continue
        day, bodies = entry
        season = reading.season_of(d, day)
        yr = (day - 1) // dpy + 1
        for b in bodies:
            t = tribe.get(b[0])
            c = camps.get((t, yr)) or camps.get((t, yr - 1))
            if c:
                out[season].append(max(abs(b[1] - c[0]), abs(b[2] - c[1])))
    return out


def person_days(d):
    """The living's days: the chain's hours / 24, else the bodies at noon (`positions`); None without either."""
    ys = chain_years(d)
    if ys:
        return sum(_num(y.get("lived_hours")) for y in ys) / 24
    pos = d.get("positions")
    if isinstance(pos, list) and pos:
        return sum(len(e[1]) for e in pos if isinstance(e, list) and len(e) == 2)
    return None


def age_years_at_death(d, p):
    died = p.get("died") or {}
    days = reading.age_days(d, p, died.get("day")) if died.get("day") is not None else None
    return None if days is None else days / reading.age_year_days(d)


def fertile_years(d):
    for f in (d.get("meta") or {}).get("factors") or []:
        if isinstance(f, dict) and f.get("key") == "life.fertile_years" and isinstance(f.get("value"), list) and len(f["value"]) == 2:
            return tuple(f["value"])
    return (16, 42)


def fertile_woman_days(d):
    """The days women of fertile age lived in the run (whole age-years `fertile_years`, both ends in)."""
    ay = reading.age_year_days(d)
    lo, hi = fertile_years(d)
    end = reading.run_days(d)
    total = 0
    for p in d.get("people", []):
        if p.get("sex") != "woman":
            continue
        first = 1 if p.get("born") == "start" else p.get("born_day")
        if not isinstance(first, int):
            continue
        age0 = reading.age_days(d, p, first)
        if age0 is None:
            continue
        last = (p.get("died") or {}).get("day") or end
        # the days t in [first, last] with lo*ay <= age0 + (t - first) < (hi+1)*ay
        a = max(first, first + lo * ay - age0)
        b = min(last, first + (hi + 1) * ay - age0 - 1)
        total += max(0, b - a + 1)
    return total


def measures(d):
    """The eight measures of a run, flat: a key None where the data does not hold it."""
    m = {}
    years = d.get("years") or []
    ys = chain_years(d)
    pdays = person_days(d)
    # 1. Manger
    eaten = sum(_num((y.get("food") or {}).get("eaten_kj")) for y in years)
    if ys:
        need = sum(_num(y.get("need_kj")) + _num(y.get("walk_kj")) for y in ys)
        m["eaten_pct"] = round(100 * eaten / need) if need else None
    else:
        m["eaten_pct"] = None
    thin = [(y.get("food") or {}).get("thin_hours") for y in years]
    m["thin_h"] = one(sum(_num(x) for x in thin) / pdays) if pdays and any(x is not None for x in thin) else None
    # 2. L'effort
    walker_days = sum(_num(y.get("walker_hours")) for y in ys) / 24 if ys else 0
    tph = (d.get("chain") or {}).get("ticks_per_hour") if ys else None
    m["walked_m"] = round(sum(_num(y.get("walked_m")) for y in ys) / walker_days) if walker_days else None
    m["gather_h"] = one(sum(_num(y.get("gathering_ticks")) for y in ys) / tph / walker_days) if walker_days and tph else None
    # 3. Aller loin
    far = noon_distances(d)
    for s in SEASONS:
        v = far[s]
        m["far_" + s] = (round(statistics.median(v)), p90(v)) if v else None
    # 4. L'épuisement
    no = d.get("nights_out")
    dawns = sum(_num(x.get("dawns")) for x in no) if isinstance(no, list) else 0
    m["away_pct"] = one(100 * sum(_num(x.get("away")) for x in no) / dawns) if dawns else None
    aw = (d.get("chain") or {}).get("away") if isinstance(d.get("chain"), dict) else None
    if dawns and isinstance(aw, list):
        tot = lambda *ks: sum(_num(x.get(k)) for x in aw for k in ks)  # noqa: E731
        m["exhausted_pct"] = one(100 * tot("exhausted") / dawns)
        m["stopped_pct"] = one(100 * tot("stopped") / dawns)
        m["choice_pct"] = one(100 * tot("sleep_out", "goal") / dawns)
        m["awake_pct"] = one(100 * tot("awake", "woke") / dawns)
        m["other_pct"] = one(100 * tot("lay_at_home", "unseen") / dawns)
        fat = [(x.get("fatigue_mean"), _num(x.get("exhausted")) + _num(x.get("stopped")) + _num(x.get("sleep_out"))
                + _num(x.get("goal")) + _num(x.get("lay_at_home"))) for x in aw if x.get("fatigue_mean") is not None]
        n = sum(w for _f, w in fat)
        m["away_fatigue"] = round(sum(f * w for f, w in fat) / n) if n else None
        m["away_counts"] = {k: tot(k) for k in ("exhausted", "stopped", "sleep_out", "goal", "lay_at_home", "awake", "woke", "unseen")}
        m["stopped_by"] = {k: sum(_num((x.get("stopped_by") or {}).get(k)) for x in aw) for k in ("water", "rock", "no_way", "none")}
    else:
        for k in ("exhausted_pct", "stopped_pct", "choice_pct", "awake_pct", "other_pct", "away_fatigue", "away_counts", "stopped_by"):
            m[k] = None
    # 5. Déménager
    hc = d.get("home_changes")
    if isinstance(hc, list):
        m["moves"] = len(hc)
        dist = [math.hypot(c["to"][0] - c["from"][0], c["to"][1] - c["from"][1]) for c in hc if c.get("from") and c.get("to")]
        m["move_median_m"] = round(statistics.median(dist)) if dist else None
    else:
        m["moves"] = m["move_median_m"] = None
    camps = (d.get("meta") or {}).get("camps") or []
    st = d.get("settlements")
    if isinstance(st, list) and camps:
        far_s = [min(math.hypot(g["at"][0] - c[0], g["at"][1] - c[1]) for c in camps)
                 for s in st for g in s.get("groups") or [] if g.get("at")]
        m["settle_far_m"] = round(max(far_s)) if far_s else None
    else:
        m["settle_far_m"] = None
    # 6. Explorer
    acts = [(y.get("decisions") or {}).get("days_by_act") for y in years]
    acts = [a for a in acts if isinstance(a, dict)]
    act_days = sum(_num(v) for a in acts for v in a.values())
    looked = sum(_num(a.get("looked")) for a in acts)
    m["explore_days"] = one(looked * reading.year_days(d) / act_days) if act_days else None
    looks = (d.get("chain") or {}).get("looks") if isinstance(d.get("chain"), dict) else None
    tiles = [x[2] for x in looks if isinstance(x, list) and len(x) == 3] if isinstance(looks, list) else None
    m["look_median_m"] = round(statistics.median(tiles)) if tiles else None
    m["look_max_m"] = max(tiles) if tiles else None
    m["looks"] = len(tiles) if tiles is not None else None
    found = [((y.get("decisions") or {}).get("trips") or {}).get("found") for y in years]
    found = [f for f in found if isinstance(f, dict)]
    m["found_looking"] = sum(_num(f.get("looked")) for f in found) if found else None
    # 7. La vie
    people = d.get("people", [])
    births = sum(1 for p in people if p.get("born") != "start")
    wdays = fertile_woman_days(d)
    m["births_per_woman"] = round(births * reading.year_days(d) / wdays, 2) if wdays else None
    ay = reading.age_year_days(d)
    child = [p for p in people if p.get("born") != "start" or (p.get("age_days_at_start") is not None and p["age_days_at_start"] < 16 * ay)]
    dead_ages = [(p, age_years_at_death(d, p)) for p in people if p.get("died")]
    m["child_deaths"] = sum(1 for _p, a in dead_ages if a is not None and a < 16)
    m["child_deaths_pct"] = round(100 * m["child_deaths"] / len(child)) if child else None
    by = {}
    for p, _a in dead_ages:
        c = (p.get("died") or {}).get("cause") or "?"
        by[c] = by.get(c, 0) + 1
    m["deaths_by"] = by
    m["hunger_deaths"] = by.get("hunger", 0)
    ages = [a for _p, a in dead_ages if a is not None]
    m["mean_age_death"] = one(sum(ages) / len(ages)) if ages else None
    # 8. Les couples
    na = d.get("nights_apart")
    nd = sum(_num(x.get("dawns")) for x in na) if isinstance(na, list) else 0
    m["together"] = round(100 * sum(_num(x.get("together")) for x in na) / nd) if nd else None
    has = any(isinstance(y.get("couples"), dict) for y in years)
    m["parted"] = sum(_num((y.get("couples") or {}).get("parted")) for y in years) if has else None
    return m


# --- the table ---------------------------------------------------------------------------------------

def _season_far(season):
    def text(m):
        v = m.get("far_" + season)
        return None if v is None else f"{reading.SEASON_FR[season]} {fmt(v[0])} · {fmt(v[1])} m"
    return text


def _deaths_other(m):
    by = m.get("deaths_by")
    if by is None:
        return None
    rest = [(c, n) for c, n in sorted(by.items(), key=lambda x: -x[1]) if c != "hunger" and n]
    return " · ".join(f"{reading.cause_fr(c)} {fmt(n)}" for c, n in rest) or "pas d’autre mort"


def _child(m):
    n = m.get("child_deaths")
    if n is None:
        return None
    pc = m.get("child_deaths_pct")
    return f"{fmt(n)} enfant{'s' if n > 1 else ''} mort{'s' if n > 1 else ''}" + (f" ({fmt(pc)} %)" if pc is not None else "")


def _simple(key, words):
    def text(m):
        v = m.get(key)
        return None if v is None else words.format(fmt(v))
    return text


def _plural(key, one_word, many_word):
    def text(m):
        v = m.get(key)
        return None if v is None else f"{fmt(v)} {one_word if abs(v) < 2 else many_word}"
    return text


# The second table's columns: key, French header, the one-line explanation, then its values: each a
# measure's key (what is compared), how it is said, the direction of better (+1 more, -1 fewer, 0 never
# coloured) and the dead zone (an absolute floor and a share of the reference), as in the first table.
COLUMNS = [
    ("manger", "Manger",
     "ce qu’ils mangent par personne et par journée, en % de ce qu’ils brûlent (la marche comprise) ; "
     "puis les heures maigres (la réserve de graisse sous la moitié) par personne et par journée, sur 24",
     [("eaten_pct", _simple("eaten_pct", "{} % du besoin"), 1, (3, 0)),
      ("thin_h", _simple("thin_h", "{} h maigres"), -1, (0.3, 0.05))]),
    ("effort", "L’effort",
     "par personne qui marche et par journée : les mètres marchés, puis les heures passées à cueillir",
     [("walked_m", _simple("walked_m", "{} m marchés"), 0, (0, 0)),
      ("gather_h", _simple("gather_h", "{} h à cueillir"), 0, (0, 0))]),
    ("loin", "Aller loin",
     "à midi, la distance de chacun à son camp, saison par saison : la médiane, puis la distance "
     "au-delà de laquelle se tiennent les 10 % les plus loin, en mètres",
     [("far_" + s, _season_far(s), 0, (0, 0)) for s in SEASONS]),
    ("epuisement", "L’épuisement",
     "les aubes d’un adulte loin de chez lui, en % de ses aubes ; puis pourquoi, en % de toutes les aubes : "
     "tombé de fatigue, arrêté (eau, falaise, pas de chemin), dehors par choix (son habitude ou un but lointain), "
     "debout à l’aube (relevé dans la nuit, ou jamais couché), autre (il a déménagé pendant la nuit)",
     [("away_pct", _simple("away_pct", "{} % des aubes loin"), 0, (0, 0)),
      ("exhausted_pct", _simple("exhausted_pct", "épuisés {} %"), -1, (0.2, 0.05)),
      ("stopped_pct", _simple("stopped_pct", "arrêtés {} %"), 0, (0, 0)),
      ("choice_pct", _simple("choice_pct", "par choix {} %"), 0, (0, 0)),
      ("awake_pct", _simple("awake_pct", "debout {} %"), 0, (0, 0)),
      ("other_pct", _simple("other_pct", "autre {} %"), 0, (0, 0))]),
    ("demenager", "Déménager",
     "combien de fois un adulte a déménagé, la distance médiane d’un déménagement, puis l’installation "
     "(des gens qui vivent côte à côte) la plus loin des camps du départ, en mètres",
     [("moves", _plural("moves", "déménagement", "déménagements"), 0, (0, 0)),
      ("move_median_m", _simple("move_median_m", "de {} m"), 0, (0, 0)),
      ("settle_far_m", _simple("settle_far_m", "la plus loin à {} m"), 0, (0, 0))]),
    ("explorer", "Explorer",
     "les journées passées à explorer (partir seul vers l’inconnu) par personne et par cycle ; le point le plus "
     "loin de chez soi d’une journée d’exploration, la médiane et le plus loin de tous, en mètres ; les endroits "
     "trouvés en explorant et racontés au foyer (la veillée du soir au camp)",
     [("explore_days", _plural("explore_days", "journée par cycle", "journées par cycle"), 0, (0, 0)),
      ("look_median_m", _simple("look_median_m", "jusqu’à {} m"), 0, (0, 0)),
      ("look_max_m", _simple("look_max_m", "au plus {} m"), 0, (0, 0)),
      ("found_looking", _plural("found_looking", "endroit trouvé", "endroits trouvés"), 0, (0, 0))]),
    ("vie", "La vie",
     "les naissances par femme en âge d’avoir des enfants et par cycle ; les enfants morts avant 16 ans, et leur part "
     "parmi les enfants qui ont vécu pendant le run ; les morts de faim, puis des autres causes ; l’âge moyen à la mort, en ans",
     [("births_per_woman", _plural("births_per_woman", "naissance par femme", "naissances par femme"), 1, (0.05, 0.05)),
      ("child_deaths", _child, -1, (2, 0.05)),
      ("hunger_deaths", _simple("hunger_deaths", "faim {}"), -1, (2, 0.05)),
      ("deaths_by", _deaths_other, 0, (0, 0)),
      ("mean_age_death", _simple("mean_age_death", "{} ans à la mort"), 0, (0, 0))]),
    ("couples", "Les couples",
     "ensemble à l’aube, en % des matins où une femme pouvait concevoir et avait un mari vivant ; "
     "puis les séparations (mari et femme restés trop longtemps loin l’un de l’autre)",
     [("together", _simple("together", "{} % ensemble"), 1, (3, 0)),
      ("parted", _plural("parted", "séparation", "séparations"), 0, (0, 0))]),
]


def judge(value, ref, better, zone):
    """`good`, `bad` or '' (as `plain.judge`): '' when either is missing, the column has no better side, or
    the gap is within the dead zone."""
    if not isinstance(value, (int, float)) or not isinstance(ref, (int, float)) or not better:
        return ""
    gap = value - ref
    if abs(gap) <= max(zone[0], zone[1] * abs(ref)):
        return ""
    return "good" if gap * better > 0 else "bad"


def ref_words(key, ref, value):
    if not isinstance(ref, (int, float)) or not isinstance(value, (int, float)):
        return ""
    g = value - ref
    if isinstance(g, float):
        g = round(g, 2)
    return f"référence : {fmt(ref)} ({'+' if g > 0 else ''}{fmt(g)})"


def cells_html(m, ref):
    """A row's eight cells: each value on its line, the coloured ones against `ref` (the reference's
    measures, or None); a value the data does not hold is "—"."""
    m, ref = m or {}, ref or {}
    out = []
    for _k, _head, _expl, parts in COLUMNS:
        lines = []
        for key, text, better, zone in parts:
            t = text(m)
            if t is None:
                lines.append(f"<span class=v>{NONE}</span>")
                continue
            cls = judge(m.get(key), ref.get(key), better, zone)
            tip = ref_words(key, ref.get(key), m.get(key)) if better else ""
            tip_attr = f' title="{h(tip)}"' if tip else ""
            lines.append(f'<span class="v {cls}"{tip_attr}>{h(t)}</span>')
        out.append(f'<td class=ch>{"".join(lines)}</td>')
    return "".join(out)


LEGEND_COLOURS = ("<p class=caption>Comme au premier tableau, chaque run est comparé à sa référence : <span class=good>vert</span> mieux, "
                  "<span class=bad>rouge</span> moins bien, un petit écart n’est pas coloré. Seules sont colorées les valeurs où le mieux "
                  "est clair : manger plus, moins d’heures maigres, moins d’aubes loin par épuisement, moins d’enfants morts, moins de "
                  "morts de faim, plus de naissances, plus de couples ensemble. L’effort, les distances, les déménagements et "
                  "l’exploration ne sont jamais colorés : ni plus ni moins n’est meilleur en soi. Une journée : une journée de jeu ; "
                  "une heure : une heure de jeu, le 24e d’une journée ; un cycle : les quatre saisons ; « ans » : pour les âges "
                  "seulement. « — » : ce run est plus ancien que la mesure.</p>")


def chain_table(rows):
    """The second table, « La chaîne du manque »: the first table's rows (`plain.series_rows`), their
    measures' `chain` part."""
    if not rows:
        return ""
    head = "".join(f'<th class=n scope=col title="{h(expl)}">{h(name)}</th>' for _k, name, expl, _p in COLUMNS)
    body = []
    for row in rows:
        url = reading.run_url(row["serie"], row["run"])
        mark = '<span class=refmk>référence</span>' if row["ref_row"] else ""
        sub = "".join(f'<span class=sub>{h(x)}</span>' for x in (row["where"], row["state"]) if x)
        m = (row.get("m") or {}).get("chain")
        ref = (row.get("ref_m") or {}).get("chain")
        body.append(f'<tr data-href="{h(url)}" class="{"refrow" if row["ref_row"] else ""}">'
                    f'<th scope=row><a href="{h(url)}">{h(row["name"])}</a>{mark}{sub}</th>{cells_html(m, ref)}</tr>')
    legend = "".join(f"<li><b>{h(name)}</b> : {h(expl)}.</li>" for _k, name, expl, _p in COLUMNS)
    return (f'<h2 class=chain>La chaîne du manque</h2><div class=tablebox><table class="plain chain"><thead><tr><th scope=col>run</th>{head}</tr></thead>'
            f'<tbody>{"".join(body)}</tbody></table></div>{LEGEND_COLOURS}<ul class=colhelp>{legend}</ul>')


# --- the run page's part -----------------------------------------------------------------------------

DEATH_WORDS = {"hunger": "de faim", "thirst": "de soif", "cold": "de froid", "illness": "de maladie",
               "old_age": "de vieillesse", "childbirth": "en couches"}   # as plain.DEATH_WORDS

LEAD = ("Ce qui leur manque et ce que cela leur coûte, en huit mesures : ce qu’ils mangent, l’effort, jusqu’où ils vont, "
        "les nuits loin de chez eux, les déménagements, l’exploration, la vie et les couples.")


def b(x, unit=""):
    return f"<b>{h(fmt(x))}{h(unit)}</b>" if x is not None else f"<b>{NONE}</b>"


def part_html(d):
    """The run page's part « La chaîne du manque »: a line a measure, the details folded."""
    m = measures(d)
    li = []
    li.append(f"<li><b>Manger.</b> Ils mangent {b(m['eaten_pct'], ' %')} de ce qu’ils brûlent (la marche comprise), "
              f"par personne et par journée ; {b(m['thin_h'], ' h')} maigres par personne et par journée, sur 24.</li>")
    li.append(f"<li><b>L’effort.</b> Chacun marche {b(m['walked_m'], ' m')} par journée et passe {b(m['gather_h'], ' h')} à cueillir.</li>")
    far = [f"{reading.SEASON_FR[s]} {b(m['far_' + s][0], ' m')} (10 % au-delà de {b(m['far_' + s][1], ' m')})"
           for s in SEASONS if m.get("far_" + s)]
    li.append("<li><b>Aller loin.</b> À midi, la distance médiane au camp : " + ("; ".join(far) if far else NONE) + ".</li>")
    ep = f"<li><b>L’épuisement.</b> {b(m['away_pct'], ' %')} des aubes d’un adulte se passent loin de chez lui"
    if m.get("away_counts"):
        c = m["away_counts"]
        ep += (f" : {b(c['exhausted'])} fois tombé de fatigue, {b(c['stopped'])} fois arrêté sans chemin du retour, "
               f"{b(c['sleep_out'] + c['goal'])} fois dehors par choix, {b(c['woke'])} fois relevé dans la nuit, {b(c['awake'])} fois jamais couché, "
               f"{b(c['lay_at_home'] + c['unseen'])} autres (il a déménagé pendant la nuit, ou son coucher n’a pas été vu)")
        if m.get("away_fatigue") is not None:
            ep += f" ; leur fatigue en se couchant : {b(m['away_fatigue'])} sur 100 en moyenne"
    else:
        ep += " (pourquoi : pas mesuré dans ce run, plus ancien que la mesure)"
    li.append(ep + ".</li>")
    li.append(f"<li><b>Déménager.</b> {b(m['moves'])} déménagement{'s' if (m['moves'] or 0) > 1 else ''}, de {b(m['move_median_m'], ' m')} "
              f"en médiane ; l’installation la plus loin des camps du départ est à {b(m['settle_far_m'], ' m')}.</li>")
    li.append(f"<li><b>Explorer.</b> {b(m['explore_days'])} journées d’exploration par personne et par cycle ; une journée d’exploration "
              f"va jusqu’à {b(m['look_median_m'], ' m')} de chez lui en médiane, {b(m['look_max_m'], ' m')} au plus ; "
              f"{b(m['found_looking'])} endroits trouvés en explorant et racontés au foyer.</li>")
    by = m.get("deaths_by") or {}
    causes = ", ".join(f"{fmt(n)} {DEATH_WORDS.get(c, 'de ' + reading.cause_fr(c))}" for c, n in sorted(by.items(), key=lambda x: -x[1]) if n) or "aucune"
    bw = m["births_per_woman"]
    li.append(f"<li><b>La vie.</b> {b(bw)} naissance{'s' if (bw or 0) >= 2 else ''} par femme en âge d’avoir des enfants et par cycle ; "
              f"{b(m['child_deaths'])} enfants morts avant 16 ans ({b(m['child_deaths_pct'], ' %')} des enfants qui ont vécu) ; "
              f"morts par cause : {h(causes)} ; l’âge moyen à la mort : {b(m['mean_age_death'], ' ans')}.</li>")
    li.append(f"<li><b>Les couples.</b> Ensemble à l’aube {b(m['together'], ' %')} des matins où la femme pouvait concevoir ; "
              f"{b(m['parted'])} séparation{'s' if (m['parted'] or 0) > 1 else ''}.</li>")
    sb = m.get("stopped_by")
    extra = ""
    if sb and sum(sb.values()):
        words = {"water": "l’eau", "rock": "la falaise", "no_way": "pas de chemin vu", "none": "rien de dit"}
        extra = ("<p class=caption>Les arrêtés, par ce qui a barré leur dernière marche de la journée : "
                 + ", ".join(f"{words[k]} {fmt(n)}" for k, n in sb.items() if n) + ".</p>")
    return f'<ul class=chainlist>{"".join(li)}</ul>{extra}'


CSS = """
h2.chain{margin-top:2em}
table.chain td.ch{text-align:left;vertical-align:top;font-variant-numeric:tabular-nums;font-size:.92em;line-height:1.45;padding:.45em .7em;min-width:9em}
table.chain td.ch span.v{display:block;white-space:nowrap}table.chain td.ch span.v:first-child{font-weight:600}
table.chain td.ch span.good,table.chain td.ch span.bad{border-radius:3px;padding:0 .25em;margin:0 -.25em}
table.plain td.ch span.good{background:var(--good-bg);color:var(--good-ink)}table.plain td.ch span.bad{background:var(--bad-bg);color:var(--bad-ink)}
ul.chainlist{margin:.2em 0 .8em;padding-left:1.2em;line-height:1.6;max-width:70em}ul.chainlist li{margin:.15em 0}
"""
