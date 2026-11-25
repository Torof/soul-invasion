"""Le foyer : la mise en commun (D58, 25 November 2026), on a run's page.

Reads the JSON block `pooling` (docs/10-vivarium-data.md): what the hearths received, gave and
lost, the children's deaths, the parents against the other grown, and day by day the reserves at
dawn against the share of the grown going out (the project lead's worry: do they stop going out
when the reserve is full, then all leave when it is empty?). Its own module, apart from the others.
"""

import html

import reading

h = html.escape
fmt = reading.fmt

LEAD = ("Ce que chaque foyer a reçu, donné et perdu : ce qu’on pose à la réserve du camp en rentrant, ce qu’on y mange, "
        "les jeunes d’abord ; les enfants morts jeunes ; les parents d’un enfant de moins de 8 ans face aux autres adultes ; "
        "et, journée après journée, la réserve à l’aube face à la part des adultes qui partent cueillir.")


def block(d):
    p = (d or {}).get("pooling")
    return p if isinstance(p, dict) else None


def _mj(kj):
    return fmt(round((kj or 0) / 1000))


def _corr(xs, ys):
    """Pearson's r, or None when one side does not vary."""
    n = len(xs)
    if n < 3:
        return None
    mx, my = sum(xs) / n, sum(ys) / n
    sx = sum((x - mx) ** 2 for x in xs)
    sy = sum((y - my) ** 2 for y in ys)
    if sx == 0 or sy == 0:
        return None
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / (sx * sy) ** 0.5


def cycle(p):
    """The boom and bust looked for: (r between the reserve at dawn and the share going out that
    day, the share's low and high, a sentence)."""
    key = "gathered_percent" if any("gathered_percent" in x for x in p.get("days") or []) else "out_percent"
    days = [x for x in p.get("days") or [] if x.get(key) is not None and x.get("grown")]
    if len(days) < 3:
        return None, None, None, "Trop peu de journées pour le dire."
    res = [x.get("reserve_dawn_kj") or 0 for x in days]
    out = [x[key] for x in days]
    r = _corr(res, out)
    lo, hi = min(out), max(out)
    calm = sum(1 for o in out if o < 50)
    if r is not None and r <= -0.3 and hi - lo >= 30:
        words = (f"Le cycle apparaît : plus la réserve est pleine à l’aube, moins ils partent cueillir (r = {r:.2f}), "
                 f"et la part qui y part va de {lo} à {hi} %.")
    elif calm == 0:
        words = (f"Pas de cycle : chaque journée, {lo} % au moins des adultes partent cueillir (jusqu’à {hi} %), "
                 f"quelle que soit la réserve" + (f" (r = {r:.2f})." if r is not None else "."))
    else:
        words = (f"Pas de cycle net : {calm} journée{'s' if calm > 1 else ''} où moins de la moitié part cueillir, "
                 f"la part qui y part va de {lo} à {hi} %" + (f", r = {r:.2f} avec la réserve." if r is not None else "."))
    return r, lo, hi, words


def bullets(p):
    t = p.get("totals") or {}
    c = p.get("children") or {}
    days = [x for x in p.get("days") or [] if x.get("grown")]
    n = max(1, len(days))
    par, oth = p.get("parents") or {}, p.get("others") or {}

    def share(a, b):
        return f"{round(100 * a / b)} %" if b else "—"

    out = []
    if not p.get("on"):
        out.append("<b>Pas de mise en commun</b> dans ce run (le témoin) : chacun mange ce qu’il cueille, "
                   "un enfant sevré cueille pour lui-même.")
    else:
        out.append(f"<b>Posé au foyer : {_mj(t.get('put_kj'))} MJ</b>, mangé : <b>{_mj(t.get('eaten_kj'))} MJ</b> "
                   f"(dont {_mj(t.get('eaten_young_kj'))} MJ par les moins de 16 ans), pourri : {_mj(t.get('spoiled_kj'))} MJ ; "
                   f"soit {fmt(round((t.get('put_kj') or 0) / n))} kJ posés et {fmt(round((t.get('eaten_kj') or 0) / n))} kJ mangés par journée."
                   + ("" if p.get("care") else " Sans le souci des jeunes."))
    out.append(f"<b>Enfants morts avant 8 ans : {fmt(c.get('dead_before_eight'))}</b> sur {fmt(c.get('under_eight'))} ; "
               f"<b>avant 16 ans : {fmt(c.get('dead_before_sixteen'))}</b> sur {fmt(c.get('young'))} ; "
               f"<b>{fmt(c.get('born'))} naissances</b>.")
    out.append(f"<b>Parents d’un enfant de moins de 8 ans</b> : {fmt(par.get('tiles_per_day'))} tuiles marchées par journée, "
               f"fatigue au coucher {fmt(par.get('fatigue_at_nightfall'))}, maigres {share(par.get('thin') or 0, par.get('body_days') or 0)} des journées ; "
               f"<b>autres adultes</b> : {fmt(oth.get('tiles_per_day'))} tuiles, fatigue {fmt(oth.get('fatigue_at_nightfall'))}, "
               f"maigres {share(oth.get('thin') or 0, oth.get('body_days') or 0)}.")
    _, _, _, words = cycle(p)
    out.append(f"<b>Sortir ou rester</b> : {h(words)}")
    return "<ul class=plainlist>" + "".join(f"<li>{x}</li>" for x in out) + "</ul>"


def charts(d, p):
    days = [x for x in p.get("days") or [] if x.get("grown")]
    if not days:
        return ""
    xs = [x["day"] for x in days]
    yd = reading.year_days(d)
    res = [round((x.get("reserve_dawn_kj") or 0) / 1000) for x in days]
    put = [round((x.get("put_kj") or 0) / 1000) for x in days]
    eat = [round((x.get("eaten_kj") or 0) / 1000) for x in days]
    top = max(res + put + eat + [1])
    a = reading.Chart(0, max(xs[-1], yd), top, "cycles des saisons", "MJ", yd, size=(1000, 230), winters=True)
    a.line(xs, res, "#7a4a1a", "réserve à l’aube")
    a.line(xs, put, "#3a8a4a", "posé dans la journée")
    a.line(xs, eat, "#c33", "mangé dans la journée")
    b = reading.Chart(0, max(xs[-1], yd), 100, "cycles des saisons", "% des adultes", yd, pct=True, size=(1000, 200), winters=True)
    b.line(xs, [x.get("out_percent") for x in days], "#9ab", "sortis de leur camp (boire compris)")
    b.line(xs, [x.get("gathered_percent") for x in days], "#36c", "partis cueillir")
    return (a.svg("les réserves des foyers, journée après journée")
            + b.svg("la part des adultes qui sortent, journée après journée"))


def part_html(d):
    p = block(d)
    if p is None:
        return '<p class=muted>Pas de données du foyer : le run est d’avant la mise en commun.</p>'
    return bullets(p) + (charts(d, p) if p.get("on") else "")
