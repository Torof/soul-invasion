"""The run page's panels of lab 6, and the home's series cards (D52, the brief lab 6).

The lead, 23 November 2026: the factors as cards, not lines; several maps, interactive; the
curves as small multiples, never lines over lines; a home page where each series says what it
is for, when it ran and what it found. Everything here is read from the run's data file
(`docs/10-vivarium-data.md`); the heavy parts (the maps, the curves) are sent by the lab when the
page asks (`/morceau/…?quoi=…`), never carried in the page. Standard library only.
"""

import base64
import html
import json
import math
import os
import statistics
import threading

import reading

h = html.escape
fmt = reading.fmt


# --- the factors as cards ------------------------------------------------------------------

# The needs' rates (`meta.rates[].name`, as content/needs.ron names them for the vivarium's page):
# a French name, the theme, the unit and one sentence. A rate the lab does not know shows its own
# name under "Les corps", never dropped.
RATE_FR = {
    "hunger a day": ("La faim", "food", "points / jour", "monte de tant sur 100 chaque jour sans manger"),
    "thirst a day": ("La soif", "bodies", "points / jour", "monte de tant sur 100 chaque jour sans boire"),
    "cold a day of night": ("Le froid", "bodies", "points / jour de nuit", "monte la nuit dehors, loin du camp"),
    "fatigue a day awake": ("La fatigue", "bodies", "points / jour éveillé", "il faut dormir chaque jour"),
    "vitality lost a day starving, the fat reserve spent": ("La famine", "food", "vie / jour",
                                                            "la vie perdue chaque jour quand la réserve de graisse est vide"),
    "fat reserve, kJ": ("La réserve de graisse", "food", "kJ", "ce qu’un corps brûle avant d’affamer"),
    "a day's eating at most, % of the need": ("Manger au plus", "food", "% du besoin / jour",
                                             "la réserve se refait lentement, jamais d’un repas"),
    "vitality lost a day at full thirst": ("La soif tue", "bodies", "vie / jour", "la vie perdue chaque jour à pleine soif"),
    "vitality lost a day at full cold": ("Le froid tue", "bodies", "vie / jour", "la vie perdue chaque jour à plein froid"),
}
# The factors of content/needs.ron the cards show (`meta.factors[].key`, since lab 6): theme, French
# name, unit, one sentence. Every other key is in the folded list, by its own name.
FACTOR_FR = {
    "thin_percent": ("bodies", "Maigre sous", "% de la réserve", "en dessous, on compte la personne maigre"),
    "weak_percent": ("bodies", "Faible sous", "% de la réserve", "en dessous, faible : elle marche moins vite"),
    "weak_pace_percent": ("bodies", "Le pas d’un faible", "% du pas", "ce qu’il reste du pas quand on est faible"),
    "life.pregnancy_quarters": ("bodies", "La grossesse", "quarts d’année d’âge", "de la conception à la naissance"),
    "life.old_age_from_years": ("bodies", "La vieillesse", "ans", "l’âge où la mort de vieillesse commence"),
    "life.childbirth_death_percent": ("bodies", "Mourir en couches", "%", "la part des naissances où la mère meurt"),
    "meal_g": ("food", "Un repas", "g", "ce qu’on mange d’un coup"),
    "kj_per_hunger_point": ("food", "Un point de faim", "kJ", "ce que vaut un point de faim en énergie"),
    "thought.fed_under": ("food", "Rassasié sous", "points de faim", "on ne repense pas au repas à chaque bouchée"),
    "thought.plant_sight_tiles": ("food", "La vue des plantes", "m", "on voit d’aussi loin un arbre ou une plante qui porte"),
    "thought.place_tiles": ("thought", "Un lieu", "m de côté", "on retient la nourriture par coin, pas par buisson"),
    "thought.places_remembered": ("thought", "Lieux retenus", "lieux", "ceux qui valent le plus depuis le dernier regard"),
    "thought.plant_choice_hours": ("thought", "Choisir une plante", "heures", "on s’en tient à la plante choisie tant de temps"),
    "trials.trial_after_days": ("thought", "Avant d’essayer", "jours de détresse", "il faut souffrir tant de jours pour essayer autre chose"),
    "trials.trial_days": ("thought", "Un essai dure", "jours", "puis le monde le juge : gardé ou abandonné"),
    "trials.imitate_percent": ("thought", "Copier", "%", "la chance de copier le camarade qui va le mieux"),
    "trials.lived_keep_permille": ("thought", "Mémoire des nuits", "‰ par jour", "ce qu’un lieu où l’on a dormi garde d’un jour à l’autre"),
    "fire.told_per_night": ("fire", "Récits par soir", "souvenirs", "ce qu’un conteur dit au feu en une soirée"),
    "fire.heard_max": ("fire", "Retenus au plus", "souvenirs", "ce qu’une personne garde de ce qu’elle a entendu"),
    "fire.hearth_nights": ("fire", "Mémoire du foyer", "soirs", "les soirs que le foyer garde en mémoire"),
}

THEMES = [("world", "Le monde"), ("bodies", "Les corps"), ("food", "La nourriture"), ("thought", "La pensée"), ("fire", "Le feu")]


def factor(name, value, unit, sentence, off=False):
    return {"name": name, "value": value, "unit": unit, "sentence": sentence, "off": off}


def factors(d):
    """The run's factors by theme, `{theme: [factor, …]}`, read from `meta` only."""
    m = d.get("meta") or {}
    s = m.get("settings") or {}
    out = {k: [] for k, _ in THEMES}
    tiles = m.get("map_tiles") or [0, 0]
    yd = m.get("days_per_year") or 50
    days = s.get("days") or 0
    world = "le vivarium, une zone de l’atlas" if s.get("world") == "vivarium" else f"un carré de {fmt(s.get('side'))} m"
    out["world"].append(factor("La carte", f"{tiles[0] / 1000:.1f} km".replace(".", ","), f"{fmt(tiles[0])} × {fmt(tiles[1])} m",
                               f"{world} ; petite exprès, pour que les runs soient courts"))
    out["world"].append(factor("L’année", fmt(yd), "jours", f"quatre saisons serrées dans {fmt(yd)} jours au lieu de 365"))
    ad = m.get("days_per_age_year") or yd
    if ad != yd:
        out["world"].append(factor("L’année d’âge", fmt(ad), "jours", "on vieillit d’un an tous les " + fmt(ad) + " jours"))
    out["world"].append(factor("La durée", fmt(days // yd if yd else 0), "ans", f"{fmt(days)} jours de jeu"))
    tribes = s.get("tribes") or len(m.get("camps") or [])
    out["world"].append(factor("Les tribus", fmt(tribes), "camps",
                               "chacune née à son camp, de part et d’autre de la rivière" if tribes > 1 else "une seule bande"))
    out["world"].append(factor("La graine", fmt(s.get("seed")), "", "le hasard du run ; même graine, même run"))
    out["world"].append(factor("La cadence", fmt(s.get("ticks_per_second")), "tick / seconde",
                               "le jeu tourne à 10 ; une pensée prend un tick, le résultat en dépend un peu"))
    ages = s.get("adult_years") or [None, None]
    out["bodies"].append(factor("Les grands", fmt(s.get("people")), "par camp", f"de {fmt(ages[0])} à {fmt(ages[1])} ans au départ, en couples"))
    out["bodies"].append(factor("Les enfants", fmt(s.get("children")), "par camp", "de 3 à 11 ans, une mère et un père chacun"))
    out["bodies"].append(factor("Les nourrissons", fmt(s.get("infants")), "par camp", "portés et allaités par une femme de la tribu"))
    out["bodies"].append(factor("La vie", "oui", "", "on naît, on s’unit, on meurt de maladie et de vieillesse (D49)"))
    for r in m.get("rates") or []:
        name, theme, unit, sentence = RATE_FR.get(r.get("name"), (r.get("name", "?"), "bodies", "", "taux de content/needs.ron"))
        out[theme].append(factor(name, fmt(r.get("rate")), unit, sentence))
    fed = bool(s.get("fed"))
    out["food"].insert(0, factor("Nourriture sans limite", "oui" if fed else "non", "",
                                 "personne n’a faim : les naissances se lisent sans la nourriture" if fed
                                 else "il faut cueillir ce que les plantes portent, en saison"))
    out["food"].append(factor("Un besoin", fmt(reading.KJ_PER_PERSON_DAY), "kJ / jour", "ce qu’une personne mange en un jour"))
    brain = s.get("brain", True)
    out["thought"].append(factor("Le cerveau", "oui" if brain else "non", "",
                                 "il pense quand quelque chose change pour son corps" if brain else "le témoin : ils restent où ils sont nés",
                                 off=not brain))
    trials = s.get("trials", True)
    out["thought"].append(factor("Les essais", "oui" if trials else "non", "",
                                 "en détresse, un corps essaie une autre manière, le monde la juge (D50)" if trials
                                 else "le témoin : personne n’essaie ni ne copie", off=not trials))
    fire = s.get("fire", True)
    out["fire"].append(factor("Le feu du soir", "oui" if fire else "non", "",
                              "le soir au camp on raconte ce qu’on a vu (D55)" if fire else "le témoin : pas de récits au camp", off=not fire))
    have = {f.get("key"): f.get("value") for f in m.get("factors") or [] if isinstance(f, dict)}
    for key, (theme, name, unit, sentence) in FACTOR_FR.items():
        if key in have:
            out[theme].append(factor(name, value_fr(have[key]), unit, sentence))
    mem = m.get("memory") or {}
    if mem.get("written") is not None:
        out["fire"].append(factor("La mémoire commune", fmt(mem.get("held")), "souvenirs à la fin",
                                  f"{fmt(mem.get('written'))} écrits, {fmt(mem.get('freed'))} oubliés en route"))
    return out


def value_fr(v):
    """A factor's value in French: a number, `a–b` for a pair, `a→b · c→d` for a list of pairs."""
    if isinstance(v, bool):
        return "oui" if v else "non"
    if isinstance(v, (int, float)):
        return fmt(v)
    if isinstance(v, list) and len(v) == 2 and all(isinstance(x, (int, float)) for x in v):
        return f"{fmt(v[0])}–{fmt(v[1])}"
    if isinstance(v, list) and all(isinstance(x, list) and len(x) == 2 for x in v):
        return " · ".join(f"{fmt(a)}→{fmt(b)}" for a, b in v)
    return str(v)


def factors_html(d):
    """The factors as cards, a row of cards per theme; a switched-off factor greyed."""
    fs = factors(d)
    rows = []
    for key, title in THEMES:
        cards = "".join(
            f'<div class="fcard{" off" if f["off"] else ""}"><div class=fn>{h(f["name"])}</div>'
            f'<div class=fv>{h(f["value"])}<small>{h(f["unit"])}</small></div><div class=fs>{h(f["sentence"])}</div></div>'
            for f in fs[key])
        if cards:
            rows.append(f'<h3 class=fth>{h(title)}</h3><div class=fcards>{cards}</div>')
    every = [f for f in ((d.get("meta") or {}).get("factors") or []) if isinstance(f, dict)]
    if every:
        items = "".join(f'<li><code>{h(str(f.get("key")))}</code><span>{h(value_fr(f.get("value")))}</span></li>' for f in every)
        rows.append(f'<details class=allf><summary>Tous les réglages du contenu ({len(every)}), tels que le run les a lus '
                    f'(<code>content/needs.ron</code>)</summary><ul class=allf>{items}</ul></details>')
    else:
        rows.append('<p class=muted>Les autres réglages du contenu ne sont pas dans ce run (relancer avec le vivarium récent).</p>')
    return "".join(rows)


# --- the maps' data, sent on demand ------------------------------------------------------------

def map_png(d):
    """The map's picture as PNG bytes, or None."""
    uri = (d.get("map") or {}).get("data_uri") or ""
    prefix = "data:image/png;base64,"
    if not uri.startswith(prefix):
        return None
    try:
        return base64.b64decode(uri[len(prefix):], validate=True)
    except ValueError:
        return None


def map_base(d):
    """What both maps need for the whole run: the size, the tribes, the frames, the camps' stays,
    the presence of every year (small: a few hundred cells a tribe a year), the people's tribe."""
    m = d.get("map") or {}
    tiles = m.get("tiles") or (d.get("meta") or {}).get("map_tiles") or [0, 0]
    W, Hh = tiles
    ts = reading.tribes(d)
    stays = {t: reading.camp_stays(d, t) for t in ts}
    places = {t: reading.known_places(d, t) for t in ts}
    boxes = {"all": [0, 0, W, Hh]}
    boxes.update({t: reading.tribe_box(d, t, W, Hh, stays[t], places[t]) for t in ts})
    presence = {}
    peak = {}
    for p in d.get("presence") or []:
        t, cells = p.get("tribe"), p.get("cells") or []
        presence.setdefault(str(p.get("year")), {})[t] = cells
        peak[t] = max([peak.get(t, 1)] + [c[2] for c in cells])
    cell = next((p.get("cell_tiles") for p in d.get("presence") or [] if p.get("cell_tiles")), 32)
    return {"tiles": [W, Hh], "scale": m.get("tiles_per_pixel") or 1, "pixels": m.get("pixels"),
            "marks": [v for v in ((m.get("legend") or {}).get(k) for k in ("camp", "alive", "dead")) if isinstance(v, list)],
            "years": reading.last_year(d) or 1, "dpy": reading.year_days(d), "tribes": ts,
            "sides": {t: reading.tribe_side(d, t) for t in ts}, "boxes": boxes,
            "stays": {t: [[a, b, list(c)] for a, b, c in stays[t]] for t in ts},
            "cell": cell, "presence": presence, "peak": peak,
            "people": {str(p.get("id")): p.get("tribe") for p in d.get("people", [])}, "life": life_of(d)}


def life_of(d):
    """Every death and birth with a place, for the map of deaths and births: deaths as
    `[id, x, y, day, cause, want, tribe]`, births as `[id, x, y, day, tribe]`."""
    deaths, births = [], []
    for p in d.get("people", []):
        died = p.get("died")
        if died and died.get("at"):
            deaths.append([p.get("id"), died["at"][0], died["at"][1], died.get("day") or 1, died.get("cause"),
                           reading.is_want(died.get("cause")), p.get("tribe")])
        born = p.get("born_day")
        if p.get("born") != "start" and isinstance(born, int) and p.get("born_at"):
            births.append([p.get("id"), p["born_at"][0], p["born_at"][1], born, p.get("tribe")])
    return {"deaths": deaths, "births": births}


def map_year(d, year):
    """What the maps show of one year: the noons day by day, the year's deaths and births, the
    places the tribes know at its end, the homes moved."""
    yd = reading.year_days(d)
    first, last = (year - 1) * yd + 1, year * yd
    days = [p for p in reading.positions_of(d) if first <= p[0] <= last]
    deaths, births = [], []
    for p in d.get("people", []):
        died = p.get("died")
        if died and died.get("at") and first <= (died.get("day") or 0) <= last:
            deaths.append([p.get("id"), died["at"][0], died["at"][1], died.get("day"), died.get("cause"),
                           reading.is_want(died.get("cause"))])
        born = p.get("born_day")
        if p.get("born") != "start" and isinstance(born, int) and first <= born <= last and p.get("born_at"):
            births.append([p.get("id"), p["born_at"][0], p["born_at"][1], born])
    known = {}
    moves = []
    for y in d.get("years", []):
        if y.get("year") != year:
            continue
        k = y.get("knowledge") or {}
        if isinstance(k.get("where"), list):
            known[y.get("tribe")] = [[w[0], w[1], 1 if w[2] else 0] for w in k["where"] if len(w) == 3]
        for mv in y.get("home_moves") or []:
            moves.append([y.get("tribe"), mv.get("body"), mv.get("day"), mv.get("from"), mv.get("to"), mv.get("how")])
    return {"year": year, "first": first, "last": last, "days": days, "deaths": deaths, "births": births,
            "known": known, "moves": moves}


# --- the curves as small multiples ------------------------------------------------------------

def winters(d):
    """The winters as `[first_day, last_day]`, for the charts' pale bands."""
    yd = reading.year_days(d)
    out = []
    for y in range(1, (reading.last_year(d) or 0) + 1):
        a = (y - 1) * yd + 1 + 3 * yd // 4
        out.append([a, y * yd])
    return out


NONE_FR = {"births": "aucune naissance sur tout le run", "want": "aucune mort de manque sur tout le run",
           "background": "aucune mort de fond sur tout le run", "trials": "aucun essai sur tout le run",
           "kept": "aucun essai gardé sur tout le run", "told": "aucun récit au feu sur tout le run (le feu est éteint)",
           "dispelled": "aucune rumeur dissipée sur tout le run", "places": "aucun lieu compté"}


def curves(d):
    """The measures of the small multiples: one row a measure, a column a tribe, the same time
    axis (days) for every chart. `kind` is `line` (sampled through the day) or `bars` (a value a
    year); `split` false for a measure of all the living together."""
    c = d.get("curves") or {}
    ts = reading.tribes(d)
    days = c.get("days") or []
    yd = reading.year_days(d)
    rows = []

    def line(key, title, unit, what, by_tribe=True, digits=0, smooth=0):
        vals = c.get(key)
        if not isinstance(vals, list) or not vals:
            return
        series = vals if by_tribe else [vals]
        if smooth:
            series = [[None if v is None else statistics.fmean(x for x in s[max(0, i - smooth + 1):i + 1] if x is not None)
                       for i, v in enumerate(s)] for s in series]
        rows.append({"key": key, "title": title, "unit": unit, "what": what, "kind": "line", "split": by_tribe, "digits": digits,
                     "values": [[round(v, digits) if isinstance(v, (int, float)) else None for v in s] for s in series]})

    def bars(key, title, unit, what, fn, color=None, digits=0):
        vals = []
        for t in ts:
            ys = {y.get("year"): y for y in reading.years_of(d, t)}
            row = []
            for yr in range(1, (reading.last_year(d) or 0) + 1):
                y = ys.get(yr)
                v = fn(y) if y else None
                row.append(None if v is None else round(v, digits))
            vals.append(row)
        rows.append({"key": key, "title": title, "unit": unit, "what": what, "kind": "bars", "split": True, "digits": digits,
                     "color": color, "values": vals, "empty": not any(v for s in vals for v in s if v),
                     "none": NONE_FR.get(key, "rien sur tout le run")})

    def stored_days(y):
        st = ((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean")
        pop = y.get("population") or y.get("population_before")
        return st / pop / reading.KJ_PER_PERSON_DAY if st is not None and pop else None

    def want(y):
        return sum((y.get("deaths") or {}).get(k) or 0 for k in reading.WANT_CAUSES)

    def background(y):
        return sum(v or 0 for k, v in (y.get("deaths") or {}).items() if k not in reading.WANT_CAUSES)

    line("alive_by_tribe", "Les vivants", "personnes", "vivants, jour par jour")
    bars("births", "Les naissances", "par an", "naissances de l’année", lambda y: y.get("births"), "birth")
    bars("want", "Morts de manque", "par an", "morts de faim, de soif ou de froid dans l’année", want, "want")
    bars("background", "Morts de fond", "par an", "morts de maladie, de vieillesse ou en couches dans l’année", background, "bg-death")
    bars("stored", "Nourriture portée", "jours / personne", "ce que portent les vivants à midi, en jours de nourriture par personne, "
         "moyenne de l’année", stored_days, digits=1)
    bars("gathered", "Cueilli", "kJ / personne / jour", "cueilli dans l’année, par vivant et par jour",
         lambda y: ((y.get("food") or {}).get("gathered_kj") or 0) / max(1, y.get("population_before") or y.get("population") or 1) / yd)
    line("thin_by_tribe", "Les maigres", "personnes", "vivants dont la réserve de graisse est sous la moitié")
    line("weak_by_tribe", "Les faibles", "personnes", "vivants dont la réserve est sous le cinquième")
    line("reserve_by_tribe", "La réserve de graisse", "% du plein", "réserve moyenne des vivants")
    bars("trials", "Essais commencés", "par an", "essais d’une autre manière commencés dans l’année", lambda y: y.get("trials"))
    bars("kept", "Essais gardés", "par an", "essais jugés et gardés dans l’année", lambda y: y.get("kept"), "kept")
    bars("places", "Lieux connus", "lieux", "lieux que les vivants de la tribu ont en tête à la fin de l’année",
         lambda y: (y.get("knowledge") or {}).get("places"))
    bars("told", "Récits au feu", "par an", "souvenirs racontés au feu du soir dans l’année", lambda y: (y.get("fire") or {}).get("told"), "fire")
    bars("dispelled", "Rumeurs dissipées", "par an", "on va où un conteur a dit, on ne trouve rien",
         lambda y: (y.get("fire") or {}).get("dispelled"), "rumour")
    line("hunger", "La faim", "points sur 100", "faim moyenne des vivants, les deux tribus ensemble, moyenne glissante sur une semaine", by_tribe=False, smooth=8)
    line("thirst", "La soif", "points sur 100", "soif moyenne des vivants, les deux tribus ensemble, moyenne glissante sur une semaine", by_tribe=False, smooth=8)
    line("fatigue", "La fatigue", "points sur 100", "fatigue moyenne des vivants, les deux tribus ensemble, moyenne glissante sur une semaine", by_tribe=False, smooth=8)
    return {"days": days, "dpy": yd, "last": reading.run_days(d), "tribes": ts, "sides": {t: reading.tribe_side(d, t) for t in ts},
            "winters": winters(d), "rows": rows}


# --- the pieces the page asks for ---------------------------------------------------------------

def piece(d, what, year=None):
    """A piece of the lab 6 panels: `(bytes or text, content type)`, or None when unknown."""
    js = "application/json; charset=utf-8"
    if what == "fond":
        png = map_png(d)
        return (png, "image/png") if png else None
    if what == "carte-base":
        return json.dumps(map_base(d), ensure_ascii=False, separators=(",", ":")), js
    if what == "carte-an":
        if year is None or not 1 <= year <= (reading.last_year(d) or 0):
            return None
        return json.dumps(map_year(d, year), ensure_ascii=False, separators=(",", ":")), js
    if what == "annees":
        return reading.years_table(d), "text/html; charset=utf-8"
    if what == "relief":
        r = d.get("relief") or {"cell_tiles": 1, "size": [0, 0], "known": False, "dm": []}
        return json.dumps({"cell": r.get("cell_tiles") or 1, "size": r.get("size") or [0, 0], "known": bool(r.get("known")),
                           "dm": r.get("dm") or []}, separators=(",", ":")), js
    if what == "nourriture":
        b = d.get("bearers") or {}
        return json.dumps({"chunk_tiles": b.get("chunk_tiles") or 32, "margin_chunks": b.get("margin_chunks") or 0,
                           "kinds": b.get("kinds") or [], "cells": b.get("cells") or []}, ensure_ascii=False, separators=(",", ":")), js
    if what == "courbes":
        return json.dumps(curves(d), ensure_ascii=False, separators=(",", ":")), js
    return None


def maps_html():
    """The maps' frame: one row of controls above, two maps side by side, each with its own choice
    among six (terrain and relief, food, water, presence, routes and camps, deaths and births);
    the script loads their data when the section comes into view."""
    def pane(kind):
        return (f'<figure class=m6 data-m6={kind}><figcaption><select data-kind6 aria-label="quelle carte"></select><span class="muted note6"></span></figcaption>'
                '<div class=cv6><canvas></canvas><div class=tip6 hidden></div><div class=zoom6><button type=button data-z=1 aria-label="zoomer">+</button>'
                '<button type=button data-z=-1 aria-label="dézoomer">−</button></div></div><div class=lay6 data-lay6></div><div class=leg6 data-leg6></div></figure>')
    return ('<div class=maps6 data-maps6><div class=mbar6><span class=yr6>an —</span><input type=range data-yr6 min=1 max=1 value=1 aria-label="année">'
            '<button type=button data-play6>▶ les années</button><span class=seg role=group aria-label="cadrer sur" data-frames6></span>'
            '<label class=chk><input type=checkbox data-pale6 checked> fond pâle</label></div>'
            f'<div class=mgrid6>{pane("presence")}{pane("routes")}</div>'
            '<p class="caption">Six cartes au choix dans chaque cadre : terrain et relief, nourriture, eau, midis, trajets et camps, morts et naissances. '
            'Glisser pour déplacer, molette ou + − pour zoomer : les deux cartes bougent ensemble. Survoler : ce qu’il y a, qui est passé. '
            'Cliquer une personne : sa fiche.</p></div>')


def curves_html():
    return ('<div class=sm6 data-sm6><p class=muted>les courbes se chargent…</p></div>'
            '<p class=caption>Une petite courbe par mesure, une colonne par tribu, même axe du temps partout ; les hivers en bandes pâles. '
            'Survoler une courbe : la valeur exacte de ce jour sur toutes. Cliquer une mesure : elle s’agrandit.</p>')


# --- the home's series cards: what each run found, read once and kept -----------------------------

_BRIEFS = {}
_BRIEFS_LOCK = threading.Lock()


def run_brief(path):
    """A run's one line of result, read from its data file once and kept by mtime: alive, deaths of
    want and of background, births, food carried at the end (days a person); None without data."""
    try:
        mtime = os.path.getmtime(path)
    except OSError:
        return None
    with _BRIEFS_LOCK:
        hit = _BRIEFS.get(path)
    if hit and hit[0] == mtime:
        return hit[1]
    try:
        with open(path, encoding="utf-8") as f:
            d = json.load(f)
    except (OSError, ValueError):
        return None
    if not isinstance(d, dict) or not isinstance(d.get("meta"), dict):
        return None
    sm = reading.summary(d)
    st = sm.get("stock") or {}
    brief = {"alive": sm["alive"], "want": sm["want"], "background": sm["background"], "births": sm["births"],
             "start": sm["start"], "food_days": st.get("days_per_person"), "years": sm["years"],
             "trials": (d["meta"].get("settings") or {}).get("trials", True), "fire": (d["meta"].get("settings") or {}).get("fire", True)}
    del d
    with _BRIEFS_LOCK:
        _BRIEFS[path] = (mtime, brief)
    return brief


def variant_fr(trials, fire):
    return ("avec essais" if trials else "sans essais") + (", avec le feu" if fire else ", sans le feu")


def mean_fr(xs, digits=1):
    if not xs:
        return "-"
    m = statistics.fmean(xs)
    return fmt(round(m)) if abs(m - round(m)) < 0.05 else fmt(round(m, digits))


def series_results(runs_dir, s):
    """A series' results: one sentence, two or three points and a line per run, from the runs' data."""
    briefs = {}
    for r in s.get("runs", []):
        b = run_brief(os.path.join(runs_dir, s["name"], r["name"] + ".json"))
        if b:
            briefs[r["name"]] = b
    if not briefs:
        return {"verdict": "", "points": [], "runs": {}}
    groups = {}
    for name, b in briefs.items():
        groups.setdefault((b["trials"], b["fire"]), []).append(b)
    keys = sorted(groups, key=lambda k: (not k[0], not k[1]))
    total = len(s.get("runs", []))
    points = []
    if len(keys) == 2:
        (ka, ga), (kb, gb) = (keys[0], groups[keys[0]]), (keys[1], groups[keys[1]])
        diff = [x for x in ("essais" if ka[0] != kb[0] else "", "le feu" if ka[1] != kb[1] else "") if x]
        word = " et ".join(diff) or "la variante"
        a_al, b_al = statistics.fmean(b["alive"] for b in ga), statistics.fmean(b["alive"] for b in gb)
        gap = a_al - b_al
        verdict = (f"Avec {word}, {mean_fr([b['alive'] for b in ga])} vivants à la fin ; sans, {mean_fr([b['alive'] for b in gb])}"
                   + (f" : {'+' if gap > 0 else '−'}{mean_fr([abs(gap)])}." if abs(gap) >= 0.5 else " : pas de différence."))
        for label, fn in (("morts de manque", lambda b: b["want"]), ("naissances", lambda b: b["births"]),
                          ("jours de nourriture portée par personne à la fin", lambda b: b["food_days"])):
            xa = [fn(b) for b in ga if fn(b) is not None]
            xb = [fn(b) for b in gb if fn(b) is not None]
            if xa and xb:
                points.append(f"<b>{mean_fr(xa)}</b> {label} avec {word}, <b>{mean_fr(xb)}</b> sans")
    elif len(keys) > 2:
        short = {(True, True): "essais et feu", (True, False): "essais seuls", (False, True): "feu seul", (False, False): "ni l’un ni l’autre"}
        means = {k: statistics.fmean(b["alive"] for b in groups[k]) for k in keys}
        hi, lo = max(keys, key=means.get), min(keys, key=means.get)
        verdict = (f"Le plus de vivants à la fin : {short[hi]} ({mean_fr([means[hi]])} en moyenne) ; "
                   f"le moins : {short[lo]} ({mean_fr([means[lo]])}).")
        for label, fn in (("vivants", lambda b: b["alive"]), ("morts de manque", lambda b: b["want"]),
                          ("jours de nourriture portée par personne", lambda b: b["food_days"])):
            parts = []
            for k in keys:
                xs = [fn(b) for b in groups[k] if fn(b) is not None]
                if xs:
                    parts.append(f"<b>{mean_fr(xs)}</b> {short[k]}")
            if parts:
                points.append(f"{label} : " + " · ".join(parts))
    else:
        al = [b["alive"] for g in groups.values() for b in g]
        verdict = f"{mean_fr(al)} vivants à la fin en moyenne" + (f" (de {fmt(min(al))} à {fmt(max(al))})" if len(al) > 1 else "") + "."
        bs = [b for g in groups.values() for b in g]
        points.append(f"<b>{mean_fr([b['want'] for b in bs])}</b> morts de manque, <b>{mean_fr([b['background'] for b in bs])}</b> de fond")
        points.append(f"<b>{mean_fr([b['births'] for b in bs])}</b> naissances")
    points = points[:3]
    if len(briefs) < total:   # said even when three points are already there
        points = points[:2] + [f"<b>{len(briefs)}</b> runs lus sur {total}"]
    runs = {}
    for name, b in briefs.items():
        food = "" if b["food_days"] is None else f" · {fmt(round(b['food_days'], 1))} j de nourriture portée"
        runs[name] = f"{fmt(b['alive'])} vivants · {fmt(b['want'])} morts de manque · {fmt(b['births'])} naissances{food}"
    return {"verdict": verdict, "points": points, "runs": runs}


# --- the page's style and scripts -------------------------------------------------------------

PANELS_CSS = """
.fth{margin:1.1em 0 .4em;font-size:.82em;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);font-weight:650}
.fcards{display:grid;grid-template-columns:repeat(auto-fill,minmax(12.5em,1fr));gap:.6em}
.fcard{background:var(--surface);border:1px solid var(--edge);border-radius:10px;padding:.6em .8em .7em;box-shadow:var(--shadow)}
.fcard .fn{font-size:.85em;color:var(--ink2);font-weight:600}
.fcard .fv{font-size:1.6em;font-weight:700;line-height:1.15;font-variant-numeric:tabular-nums;margin:.1em 0}
.fcard .fv small{font-size:.5em;font-weight:500;color:var(--muted);margin-left:.35em}
.fcard .fs{font-size:.82em;color:var(--muted);line-height:1.35}
.fcard.off{background:var(--zebra)}
details.allf{margin:1em 0 0}ul.allf{columns:2 22em;list-style:none;padding:0;margin:.5em 0;font-size:.85em}
ul.allf li{display:flex;justify-content:space-between;gap:1em;padding:.15em 0;border-bottom:1px solid var(--line);break-inside:avoid}
ul.allf code{color:var(--ink2)}ul.allf span{font-variant-numeric:tabular-nums;text-align:right}.fcard.off .fv{color:var(--muted)}
.maps6 .mbar6{display:flex;flex-wrap:wrap;gap:.5em 1em;align-items:center;margin:.4em 0 .8em;position:sticky;top:0;z-index:3;
 background:var(--page);padding:.4em 0}
.maps6 .yr6{font-weight:700;min-width:3.6em;font-variant-numeric:tabular-nums}.maps6 input[type=range]{flex:1;min-width:10em}
.maps6 .seg{display:inline-flex;border:1px solid var(--line);border-radius:8px;overflow:hidden}
.maps6 .seg button{border:0;border-radius:0;padding:.2em .7em;font-size:.88em}.maps6 .seg button[aria-pressed=true]{background:var(--ink);color:var(--page)}
.maps6 .chk{font-size:.9em;color:var(--ink2)}
.mgrid6{display:grid;grid-template-columns:1fr 1fr;gap:1.2em}@media (max-width:900px){.mgrid6{grid-template-columns:1fr}}
figure.m6{margin:0;background:var(--surface);border-radius:12px;padding:.7em;box-shadow:var(--shadow);border:1px solid var(--edge);min-width:0}
figure.m6 figcaption{margin:0 0 .5em;font-size:.95em;display:flex;flex-wrap:wrap;gap:.3em;align-items:baseline}
figure.m6 select{font-weight:650;font-size:1em;padding:.15em .3em;border:1px solid var(--edge);border-radius:6px;background:var(--surface);color:var(--ink)}
.cv6{position:relative;width:100%;aspect-ratio:1;max-height:72vh;border-radius:6px;overflow:hidden;background:var(--zebra)}
.cv6 canvas{display:block;width:100%;height:100%;cursor:grab;touch-action:none}.cv6 canvas.drag{cursor:grabbing}
.cv6 canvas[data-person]{cursor:pointer}
.tip6{position:absolute;pointer-events:none;background:var(--surface);color:var(--ink);border:1px solid var(--line);border-radius:8px;
 padding:.45em .65em;font-size:.82em;line-height:1.4;box-shadow:0 2px 8px rgba(0,0,0,.18);max-width:19em;z-index:2}
.tip6 b{font-weight:700}.tip6 .m{color:var(--muted)}
.zoom6{position:absolute;right:.5em;top:.5em;display:flex;flex-direction:column;gap:.25em}
.zoom6 button{width:2em;height:2em;padding:0;font-size:1.1em;line-height:1;opacity:.92}
.lay6{display:flex;flex-wrap:wrap;gap:.2em 1em;margin:.6em 0 .2em;font-size:.88em}
.lay6 label{display:inline-flex;align-items:center;gap:.35em;cursor:pointer}
.sw6{display:inline-block;width:1.1em;height:1.1em;vertical-align:-.2em}
.leg6{font-size:.82em;color:var(--muted);display:flex;flex-direction:column;gap:.25em;margin-top:.3em}
.leg6 .ramp{display:inline-block;width:7em;height:.7em;border-radius:3px;vertical-align:middle;margin:0 .4em}
.sm6 .now6{position:sticky;top:0;z-index:2;background:var(--page);padding:.35em 0;font-weight:650;font-variant-numeric:tabular-nums;
 border-bottom:1px solid var(--line);margin-bottom:.4em;min-height:1.5em}
.row6{margin:0 0 .5em;padding:.4em 0;border-bottom:1px solid var(--line)}
.row6 .rh{display:flex;gap:.6em;align-items:baseline;cursor:pointer;flex-wrap:wrap}
.row6 .rh b{font-size:.98em}.row6 .rh span{color:var(--muted);font-size:.82em}
.row6 .rh::after{content:"agrandir";margin-left:auto;font-size:.75em;color:var(--muted)}.row6.big .rh::after{content:"réduire"}
.cols6{display:grid;gap:.9em;margin-top:.25em}
.ch6{min-width:0}.ch6 .ct{display:flex;justify-content:space-between;align-items:baseline;font-size:.8em;color:var(--ink2)}
.ch6 .ct .v{font-size:1.25em;font-weight:700;color:var(--ink);font-variant-numeric:tabular-nums}
.ch6 svg{display:block;width:100%;overflow:visible}.ch6 svg text{fill:var(--muted);font-size:10px}
"""

MAPS6_JS = r"""
(() => {
  const box = document.querySelector('[data-maps6]'), page = document.querySelector('.runpage');
  if (!box || !page) return;
  const url = q => page.dataset.morceau + '?quoi=' + q;
  const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const tc = t => css('--t' + t) || '#888';
  const nf = n => String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
  const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
  const CAUSE = {hunger: 'faim', thirst: 'soif', cold: 'froid', illness: 'maladie', old_age: 'vieillesse', childbirth: 'couches'};
  const SEASON = ['printemps', 'été', 'automne', 'hiver'];
  const KINDS = {terrain: 'Terrain et relief', food: 'Nourriture', water: 'Eau', presence: 'Où ils passent leurs midis',
    routes: 'Trajets et camps', life: 'Morts et naissances'};
  const NOTES = {terrain: 'le sol et ses hauteurs', food: 'les plantes et arbres qui portent, par saison', water: 'ce qui se boit',
    presence: 'l’année montrée', routes: 'de midi en midi, l’année montrée', life: 'jusqu’à l’année montrée'};
  // The terrain's colours in the vivarium's picture (server/vivarium `picture`), exact.
  const GROUNDS = [[[120, 160, 70], 'prairie'], [[50, 100, 45], 'forêt'], [[60, 110, 170], 'eau (rivière, lac)'], [[120, 150, 180], 'gué'],
    [[90, 140, 190], 'ruisseau'], [[130, 125, 115], 'roche, falaise']];
  const WATERS = {water: 0, ford: 1, brook: 2};
  let B = null, terrain = null, tpix = null, Y = 1, V = null, years = {}, cum = {}, playing = null;
  let R = null, shade = null, alt = null, F = null, waterCv = null;
  const maps = [...box.querySelectorAll('[data-m6]')].map(fig => ({fig, kind: fig.dataset.m6, cv: fig.querySelector('canvas'),
    tip: fig.querySelector('.tip6'), on: {}, hover: null}));
  const slider = box.querySelector('[data-yr6]'), yrLabel = box.querySelector('.yr6');
  const sw = (inner) => `<svg class=sw6 viewBox="0 0 16 16" aria-hidden=true>${inner}</svg>`;
  const camp = ['camps', true, sw(`<circle cx=8 cy=8 r=6 fill="${tc(B ? B.tribes[0] : 'A')}" stroke="#fff" stroke-width=1.5/>`) + ' camps'];
  const LAYERS = {
    terrain: () => [['shade', true, sw('<path d="M1 14L6 5L9 9L11 6L15 14Z" fill="#777"/>') + ' ombrage du relief'],
      ['alt', false, `<i class=sw6 style="background:linear-gradient(90deg,#efe6cf,#6b4b2a)"></i> altitude en couleur`], camp],
    food: () => [...SEASON.map((s, i) => ['s' + i, i === 1, `${s}`, 'saison']), camp],
    water: () => [['water', true, `<i class=sw6 style="background:rgb(60,110,170)"></i> rivière et lacs`],
      ['ford', true, `<i class=sw6 style="background:rgb(120,150,180)"></i> gués`], ['brook', true, `<i class=sw6 style="background:rgb(90,140,190)"></i> ruisseaux`], camp],
    presence: () => [...B.tribes.map(t => ['t' + t, true, `<i class=sw6 style="background:${tc(t)};opacity:.7"></i> tribu ${esc(t)}`]),
      ['cum', false, 'toutes les années jusqu’à celle-ci']],
    routes: () => [['tracks', true, sw(`<path d="M1 12L6 5L10 9L15 3" fill=none stroke="${tc(B.tribes[0])}" stroke-width=2/>`) + ' trajets'], camp,
      ['deaths', true, sw(`<path d="M3 3L13 13M3 13L13 3" stroke="${css('--want')}" stroke-width=3/>`) + ' morts'],
      ['births', true, sw(`<circle cx=8 cy=8 r=5 fill="${css('--birth')}" stroke="#fff"/>`) + ' naissances'],
      ['moves', true, sw(`<path d="M2 13L13 3M13 3h-6M13 3v6" fill=none stroke="${css('--camp')}" stroke-width=2/>`) + ' maisons déplacées'],
      ['seen', false, `<i class=sw6 style="background:${tc(B.tribes[0])};opacity:.35"></i> lieux vus`],
      ['heard', false, `<i class=sw6 style="border:2px solid ${css('--rumour')};box-sizing:border-box"></i> lieux entendus au feu seulement`]],
    life: () => [['want', true, sw(`<path d="M3 3L13 13M3 13L13 3" stroke="${css('--want')}" stroke-width=3/>`) + ' morts de manque'],
      ['bg', true, sw(`<path d="M3 3L13 13M3 13L13 3" stroke="${css('--bg-death')}" stroke-width=3/>`) + ' morts de fond'],
      ['births', true, sw(`<circle cx=8 cy=8 r=5 fill="${css('--birth')}" stroke="#fff"/>`) + ' naissances'],
      ['cum', true, 'toutes les années jusqu’à celle-ci'], camp],
  };

  // --- the view: a centre in tiles and a scale in pixels a tile, the same for both maps
  function size(m) { const r = m.cv.getBoundingClientRect(); return [r.width || 300, r.height || 300]; }
  function frame(key) {
    const b = B.boxes[key] || B.boxes.all, [w, h] = size(maps[0]);
    V = {x: b[0] + b[2] / 2, y: b[1] + b[3] / 2, k: Math.min(w / b[2], h / b[3])};
    box.querySelectorAll('[data-frame]').forEach(x => x.setAttribute('aria-pressed', x.dataset.frame === key));
    drawAll();
  }
  const minK = () => { const [w, h] = size(maps[0]); return Math.min(w / B.tiles[0], h / B.tiles[1]) * 0.8; };
  function zoomAt(m, f, px, py) {
    const [w, h] = size(m), k2 = Math.max(minK(), Math.min(12, V.k * f));
    const wx = V.x + (px - w / 2) / V.k, wy = V.y + (py - h / 2) / V.k;
    V = {x: wx - (px - w / 2) / k2, y: wy - (py - h / 2) / k2, k: k2};
    box.querySelectorAll('[data-frame]').forEach(x => x.setAttribute('aria-pressed', 'false'));
    drawAll();
  }
  const toWorld = (m, px, py) => { const [w, h] = size(m); return [V.x + (px - w / 2) / V.k, V.y + (py - h / 2) / V.k]; };

  // --- the terrain: the vivarium's picture, its dots (camps, living, dead at the end) erased
  async function loadTerrain() {
    const r = await fetch(url('fond'));
    if (!r.ok) throw new Error('fond ' + r.status);
    const bmp = await createImageBitmap(await r.blob());
    const c = document.createElement('canvas'); c.width = bmp.width; c.height = bmp.height;
    const g = c.getContext('2d'); g.drawImage(bmp, 0, 0);
    const im = g.getImageData(0, 0, c.width, c.height), p = im.data;
    const mark = (i) => B.marks.some(m => p[i] === m[0] && p[i + 1] === m[1] && p[i + 2] === m[2]);
    for (let pass = 0; pass < 2; pass++)
      for (let y = 0; y < c.height; y++) for (let x = 0; x < c.width; x++) {
        const i = (y * c.width + x) * 4;
        if (!mark(i)) continue;
        const j = x > 1 ? i - 8 : y > 1 ? i - c.width * 8 : i + 8;
        p[i] = p[j]; p[i + 1] = p[j + 1]; p[i + 2] = p[j + 2];
      }
    g.putImageData(im, 0, 0);
    terrain = c; tpix = im;
    // the water, one canvas, each class in its own colour, the rest clear
    const wc = document.createElement('canvas'); wc.width = c.width; wc.height = c.height;
    const wg = wc.getContext('2d'), wi = wg.createImageData(c.width, c.height);
    for (let i = 0; i < p.length; i += 4) {
      const cls = classAt(i);
      if (cls >= 2 && cls <= 4) { wi.data[i] = p[i]; wi.data[i + 1] = p[i + 1]; wi.data[i + 2] = p[i + 2]; wi.data[i + 3] = 255; }
    }
    wg.putImageData(wi, 0, 0); waterCv = wc;
  }
  function classAt(i) {
    const d = tpix.data;
    for (let k = 0; k < GROUNDS.length; k++) { const g = GROUNDS[k][0]; if (d[i] === g[0] && d[i + 1] === g[1] && d[i + 2] === g[2]) return k; }
    return -1;
  }
  function pix(x, y) {
    const px = Math.floor(x / B.scale), py = Math.floor(y / B.scale);
    return px < 0 || py < 0 || px >= tpix.width || py >= tpix.height ? -1 : (py * tpix.width + px) * 4;
  }
  function ground(x, y) { const i = pix(x, y); if (i < 0) return 'hors de la carte'; const k = classAt(i); return k < 0 ? 'autre' : GROUNDS[k][1]; }
  function nearestWater(x, y) {
    const px = Math.floor(x / B.scale), py = Math.floor(y / B.scale);
    for (let r = 0; r <= 80; r++) {
      let best = null;
      for (let dy = -r; dy <= r; dy++) for (let dx = -r; dx <= r; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
        const qx = px + dx, qy = py + dy;
        if (qx < 0 || qy < 0 || qx >= tpix.width || qy >= tpix.height) continue;
        const k = classAt((qy * tpix.width + qx) * 4);
        if (k >= 2 && k <= 4) { const d = Math.hypot(dx, dy) * B.scale; if (!best || d < best[0]) best = [d, GROUNDS[k][1]]; }
      }
      if (best) return best;
    }
    return null;
  }

  // --- the relief and the food, asked when a map needs them
  async function loadRelief() {
    if (R) return R;
    const r = await fetch(url('relief'));
    if (!r.ok) throw new Error('relief ' + r.status);
    R = await r.json();
    if (!R.known) return R;
    const [w, h] = R.size, c = R.cell, dm = R.dm;
    let lo = Infinity, hi = -Infinity; for (const v of dm) { if (v < lo) lo = v; if (v > hi) hi = v; }
    R.lo = lo; R.hi = hi;
    const s = document.createElement('canvas'); s.width = w; s.height = h;
    const a = document.createElement('canvas'); a.width = w; a.height = h;
    const si = s.getContext('2d').createImageData(w, h), ai = a.getContext('2d').createImageData(w, h);
    // two 3×3 box passes: the stored heights step from cell to cell, and the light would draw the steps
    let sm = Float32Array.from(dm);
    for (let pass = 0; pass < 2; pass++) {
      const o = new Float32Array(sm.length);
      for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
        let t = 0, n = 0;
        for (let j = -1; j <= 1; j++) for (let i = -1; i <= 1; i++) { const X = x + i, Y = y + j; if (X >= 0 && Y >= 0 && X < w && Y < h) { t += sm[Y * w + X]; n++; } }
        o[y * w + x] = t / n;
      }
      sm = o;
    }
    const at = (x, y) => sm[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))];
    const lx = -0.6, ly = -0.6, lz = 0.53;   // light from the north-west
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const dx = (at(x + 1, y) - at(x - 1, y)) / 10 / (2 * c), dy = (at(x, y + 1) - at(x, y - 1)) / 10 / (2 * c);
      const n = Math.hypot(dx * 3, dy * 3, 1), lit = (-dx * 3 * lx - dy * 3 * ly + lz) / n;   // slopes exaggerated three times
      const i = (y * w + x) * 4;
      const dark = Math.max(0, Math.min(1, (0.75 - lit) * 1.6));
      si.data[i] = 20; si.data[i + 1] = 20; si.data[i + 2] = 30; si.data[i + 3] = Math.round(dark * 170);
      const f = (dm[y * w + x] - lo) / Math.max(1, hi - lo);
      ai.data[i] = Math.round(239 + (107 - 239) * f); ai.data[i + 1] = Math.round(230 + (75 - 230) * f); ai.data[i + 2] = Math.round(207 + (42 - 207) * f); ai.data[i + 3] = 150;
    }
    s.getContext('2d').putImageData(si, 0, 0); a.getContext('2d').putImageData(ai, 0, 0);
    shade = s; alt = a;
    return R;
  }
  async function loadFood() {
    if (F) return F;
    const r = await fetch(url('nourriture'));
    if (!r.ok) throw new Error('nourriture ' + r.status);
    F = await r.json();
    F.index = new Map(F.cells.map(c => [c[0] + ',' + c[1], c]));
    F.peak = SEASON.map((_, s) => Math.max(1, ...F.cells.map(c => c[2][s])));
    return F;
  }

  // --- a year's data, asked once
  async function year(y) {
    if (years[y]) return years[y];
    const r = await fetch(url('carte-an') + '&an=' + y);
    if (!r.ok) throw new Error('an ' + r.status);
    const d = await r.json();
    d.tracks = {}; d.cells = new Map();
    for (const [day, list] of d.days) for (const [id, x, yy] of list) {
      (d.tracks[id] = d.tracks[id] || []).push([day, x, yy]);
      const key = Math.floor(x / B.cell) + ',' + Math.floor(yy / B.cell);
      const c = d.cells.get(key) || new Map(); c.set(id, (c.get(id) || 0) + 1); d.cells.set(key, c);
    }
    return years[y] = d;
  }
  function presenceOf(m, t) {
    const one = (B.presence[Y] || {})[t] || [];
    if (!m.on.cum) return [one, B.peak[t] || 1];
    const key = t + Y;
    if (!cum[key]) {
      const acc = new Map();
      for (let y = 1; y <= Y; y++) for (const [cx, cy, n] of ((B.presence[y] || {})[t] || [])) acc.set(cx + ',' + cy, (acc.get(cx + ',' + cy) || 0) + n);
      const cells = [...acc].map(([k, n]) => { const [a, b] = k.split(',').map(Number); return [a, b, n]; });
      cum[key] = [cells, Math.max(1, ...cells.map(c => c[2]))];
    }
    return cum[key];
  }
  const lifeOf = m => {
    const yd = B.dpy, last = Y * yd, first = m.on.cum ? 1 : (Y - 1) * yd + 1;
    return {deaths: B.life.deaths.filter(e => e[3] >= first && e[3] <= last), births: B.life.births.filter(e => e[3] >= first && e[3] <= last)};
  };
  const season = m => [0, 1, 2, 3].find(s => m.on['s' + s]) ?? 1;

  // --- drawing
  function drawCamps(g, px, w) {
    for (const t of B.tribes) {
      const st = B.stays[t].filter(s => s[0] <= Y);
      if (st.length > 1) { g.setLineDash([6 * px, 4 * px]); g.strokeStyle = tc(t); g.lineWidth = 2 * px; g.beginPath(); st.forEach((s, i) => i ? g.lineTo(s[2][0], s[2][1]) : g.moveTo(s[2][0], s[2][1])); g.stroke(); g.setLineDash([]); }
      st.forEach((s, i) => {
        const [x, y] = s[2], now = i === st.length - 1, r = (now ? 10 : 5) * px;
        g.beginPath(); if (now) g.arc(x, y, r, 0, 7); else { g.moveTo(x, y - r); g.lineTo(x + r, y); g.lineTo(x, y + r); g.lineTo(x - r, y); g.closePath(); }
        g.fillStyle = tc(t); g.fill(); g.lineWidth = 2 * px; g.strokeStyle = '#fff'; g.stroke();
        if (now) {
          g.fillStyle = '#fff'; g.font = `800 ${12 * px}px system-ui,sans-serif`; g.textAlign = 'center'; g.textBaseline = 'middle'; g.fillText(t, x, y + px);
          const span = s[0] === Math.min(s[1], Y) ? `an ${s[0]}` : `an ${s[0]} → an ${Math.min(s[1], Y)}`;
          const sx = (x - V.x) * V.k + w / 2, right = sx > w - 140, lx = x + (right ? -14 : 14) * px;
          g.font = `700 ${11 * px}px system-ui,sans-serif`; g.textAlign = right ? 'right' : 'left'; g.lineWidth = 3 * px; g.strokeStyle = css('--surface');
          g.strokeText(`camp ${t}, ${span}`, lx, y); g.fillStyle = css('--ink'); g.fillText(`camp ${t}, ${span}`, lx, y);
        }
      });
    }
  }
  function cross(g, x, y, px, color) {
    const r = 5 * px; g.lineCap = 'round';
    for (const [lw, c] of [[4.5, '#fff'], [2.2, color]]) { g.lineWidth = lw * px; g.strokeStyle = c; g.beginPath(); g.moveTo(x - r, y - r); g.lineTo(x + r, y + r); g.moveTo(x - r, y + r); g.lineTo(x + r, y - r); g.stroke(); }
  }
  function dot(g, x, y, px, color) { g.beginPath(); g.arc(x, y, 4.5 * px, 0, 7); g.fillStyle = color; g.fill(); g.lineWidth = 1.2 * px; g.strokeStyle = '#fff'; g.stroke(); }
  function draw(m) {
    if (!V || !terrain) return;
    const [w, h] = size(m), dpr = window.devicePixelRatio || 1;
    if (m.cv.width !== Math.round(w * dpr) || m.cv.height !== Math.round(h * dpr)) { m.cv.width = Math.round(w * dpr); m.cv.height = Math.round(h * dpr); }
    const g = m.cv.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = css('--zebra'); g.fillRect(0, 0, w, h);
    g.save(); g.translate(w / 2 - V.x * V.k, h / 2 - V.y * V.k); g.scale(V.k, V.k);
    g.imageSmoothingEnabled = false;
    const W = B.tiles[0], H = B.tiles[1], px = 1 / V.k, yd = years[Y];
    g.drawImage(terrain, 0, 0, W, H);
    const pale = m.kind === 'water' || (m.kind !== 'terrain' && box.querySelector('[data-pale6]').checked);
    if (pale) { g.globalAlpha = m.kind === 'water' ? .7 : .55; g.fillStyle = css('--surface'); g.fillRect(0, 0, W, H); g.globalAlpha = 1; }
    if (m.kind === 'terrain' && R && R.known) {
      g.imageSmoothingEnabled = true;
      if (m.on.alt) g.drawImage(alt, 0, 0, R.size[0] * R.cell, R.size[1] * R.cell);
      if (m.on.shade) g.drawImage(shade, 0, 0, R.size[0] * R.cell, R.size[1] * R.cell);
      g.imageSmoothingEnabled = false;
    } else if (m.kind === 'water' && waterCv) {
      if (m.on.water && m.on.ford && m.on.brook) g.drawImage(waterCv, 0, 0, W, H);
      else {   // a class off: draw the classes on one by one
        const keep = [m.on.water, m.on.ford, m.on.brook];
        if (!m.wcache || m.wkey !== keep.join()) {
          const c = document.createElement('canvas'); c.width = waterCv.width; c.height = waterCv.height;
          const cg = c.getContext('2d'), im = cg.createImageData(c.width, c.height);
          for (let i = 0; i < tpix.data.length; i += 4) { const k = classAt(i) - 2; if (k >= 0 && k <= 2 && keep[k]) { im.data[i] = tpix.data[i]; im.data[i + 1] = tpix.data[i + 1]; im.data[i + 2] = tpix.data[i + 2]; im.data[i + 3] = 255; } }
          cg.putImageData(im, 0, 0); m.wcache = c; m.wkey = keep.join();
        }
        g.drawImage(m.wcache, 0, 0, W, H);
      }
    } else if (m.kind === 'food' && F) {
      const s = season(m), c = F.chunk_tiles, peak = F.peak[s];
      g.fillStyle = css('--kept');
      for (const [cx, cy, by] of F.cells) { if (!by[s]) continue; g.globalAlpha = .1 + .8 * Math.log1p(by[s]) / Math.log1p(peak); g.fillRect(cx * c, cy * c, c, c); }
      g.globalAlpha = 1;
      if (m.hover && m.hover.chunk) { g.strokeStyle = css('--ink'); g.lineWidth = 2 * px; g.strokeRect(m.hover.chunk[0] * c, m.hover.chunk[1] * c, c, c); }
    } else if (m.kind === 'presence') {
      for (const t of B.tribes) {
        if (!m.on['t' + t]) continue;
        const [cells, peak] = presenceOf(m, t);
        g.fillStyle = tc(t);
        for (const [cx, cy, n] of cells) { g.globalAlpha = .12 + .8 * Math.log1p(n) / Math.log1p(peak); g.fillRect(cx * B.cell, cy * B.cell, B.cell, B.cell); }
      }
      g.globalAlpha = 1;
      if (m.hover && m.hover.cell) { g.strokeStyle = css('--ink'); g.lineWidth = 2 * px; g.strokeRect(m.hover.cell[0] * B.cell, m.hover.cell[1] * B.cell, B.cell, B.cell); }
    } else if (m.kind === 'routes' && yd) {
      for (const t of B.tribes) {
        const list = yd.known[t] || [];
        if (m.on.seen) { g.fillStyle = tc(t); g.globalAlpha = .35; for (const [x, y, s] of list) if (s) g.fillRect(x * 16, y * 16, 16, 16); g.globalAlpha = 1; }
        if (m.on.heard) { g.strokeStyle = css('--rumour'); g.lineWidth = 1.5 * px; for (const [x, y, s] of list) if (!s) g.strokeRect(x * 16 + .5, y * 16 + .5, 15, 15); }
      }
      if (m.on.tracks) {
        g.lineWidth = 1 * px; g.lineJoin = 'round'; g.globalAlpha = .3;
        for (const [id, tr] of Object.entries(yd.tracks)) { g.strokeStyle = tc(B.people[id]); g.beginPath(); tr.forEach(([, x, y], i) => i ? g.lineTo(x, y) : g.moveTo(x, y)); g.stroke(); }
        g.globalAlpha = 1;
        const lastDay = yd.days.length ? yd.days[yd.days.length - 1][1] : [];
        for (const [id, x, y] of lastDay) { g.beginPath(); g.arc(x, y, 3 * px, 0, 7); g.fillStyle = tc(B.people[id]); g.fill(); g.lineWidth = 1 * px; g.strokeStyle = '#fff'; g.stroke(); }
        if (m.hover && m.hover.track) {
          const tr = yd.tracks[m.hover.track[0]]; g.strokeStyle = css('--ink'); g.lineWidth = 2.5 * px; g.beginPath();
          tr.forEach(([, x, y], i) => i ? g.lineTo(x, y) : g.moveTo(x, y)); g.stroke();
          const [, hx, hy] = m.hover.track[1]; g.beginPath(); g.arc(hx, hy, 5 * px, 0, 7); g.fillStyle = css('--ink'); g.fill();
        }
      }
      if (m.on.moves) {
        g.strokeStyle = css('--camp'); g.lineWidth = 2 * px;
        for (const [, , , from, to] of yd.moves) if (from && to) { g.beginPath(); g.moveTo(from[0], from[1]); g.lineTo(to[0], to[1]); g.stroke(); g.beginPath(); g.arc(to[0], to[1], 3 * px, 0, 7); g.fillStyle = css('--camp'); g.fill(); }
      }
    }
    if (m.on.camps) drawCamps(g, px, w);
    if (m.kind === 'routes' && yd) {
      if (m.on.births) for (const [, x, y] of yd.births) dot(g, x, y, px, css('--birth'));
      if (m.on.deaths) for (const [, x, y, , , want] of yd.deaths) cross(g, x, y, px, want ? css('--want') : css('--bg-death'));
    }
    if (m.kind === 'life') {
      const L = lifeOf(m);
      if (m.on.births) for (const e of L.births) dot(g, e[1], e[2], px, css('--birth'));
      for (const e of L.deaths) if (e[5] ? m.on.want : m.on.bg) cross(g, e[1], e[2], px, e[5] ? css('--want') : css('--bg-death'));
    }
    g.restore();
  }
  function drawAll() { maps.forEach(draw); }

  // --- hover: what is under the pointer
  function nearest(m, wx, wy) {
    const lim = 9 / V.k; let best = null, bd = lim * lim;
    const test = (d2, v) => { if (d2 < bd) { bd = d2; best = v; } };
    if (m.kind === 'life') {
      const L = lifeOf(m);
      for (const e of L.deaths) if (e[5] ? m.on.want : m.on.bg) test((e[1] - wx) ** 2 + (e[2] - wy) ** 2, {death: e});
      if (m.on.births) for (const e of L.births) test((e[1] - wx) ** 2 + (e[2] - wy) ** 2, {birth: e});
      return best;
    }
    const yd = years[Y];
    if (!yd || m.kind !== 'routes') return null;
    if (m.on.deaths) for (const e of yd.deaths) test((e[1] - wx) ** 2 + (e[2] - wy) ** 2, {death: e});
    if (m.on.births) for (const e of yd.births) test((e[1] - wx) ** 2 + (e[2] - wy) ** 2, {birth: e});
    if (best) return best;
    if (m.on.tracks) for (const [id, tr] of Object.entries(yd.tracks)) for (const p of tr) test((p[1] - wx) ** 2 + (p[2] - wy) ** 2, {track: [id, p]});
    return best;
  }
  const dayFr = d => { const yd = B.dpy, doy = (d - 1) % yd; return `an ${Math.floor((d - 1) / yd) + 1}, ${SEASON[Math.min(3, Math.floor(doy * 4 / yd))]}, jour ${nf(d)}`; };
  const who = id => `personne ${id} <span class=m>(${esc(B.people[id] || '?')})</span>`;
  const kindFr = k => String(k).replace(/_/g, ' ');
  function cellLines(m, wx, wy) {
    const cx = Math.floor(wx / B.cell), cy = Math.floor(wy / B.cell), yd = years[Y];
    const lines = [`<b>case de ${B.cell} m</b> <span class=m>(${nf(cx * B.cell)}, ${nf(cy * B.cell)})</span> · ${esc(ground(wx, wy))}`];
    if (m.kind === 'terrain' && R && R.known) {
      const rx = Math.floor(wx / R.cell), ry = Math.floor(wy / R.cell);
      if (rx >= 0 && ry >= 0 && rx < R.size[0] && ry < R.size[1]) lines.push(`altitude <b>${nf(R.dm[ry * R.size[0] + rx] / 10)} m</b> <span class=m>(de ${nf(R.lo / 10)} à ${nf(R.hi / 10)} m sur la carte)</span>`);
    }
    if (m.kind === 'water') { const nw = nearestWater(wx, wy); lines.push(nw ? (nw[0] < B.scale ? `ici : <b>${esc(nw[1])}</b>` : `l’eau la plus proche : <b>${nf(nw[0])} m</b> (${esc(nw[1])})`) : 'pas d’eau à moins de 400 m'); }
    if (m.kind === 'food' && F) {
      const c = F.chunk_tiles, fx = Math.floor(wx / c), fy = Math.floor(wy / c), cell = F.index.get(fx + ',' + fy);
      lines[0] = `<b>carré de ${c} m</b> <span class=m>(${nf(fx * c)}, ${nf(fy * c)})</span> · ${esc(ground(wx, wy))}`;
      if (!cell) lines.push('<span class=m>rien qui porte ici, ou pas compté (loin des midis des tribus)</span>');
      else {
        lines.push('plantes et arbres qui portent : ' + SEASON.map((s, i) => `${s} <b>${nf(cell[2][i])}</b>`).join(' · '));
        lines.push('surtout : ' + cell[3].map(([k, n]) => `${esc(kindFr(F.kinds[k]))} (${nf(n)})`).join(', '));
      }
      m.hover = {chunk: [fx, fy]};
      return lines;
    }
    if (m.kind === 'presence') {
      for (const t of B.tribes) {
        const c = (presenceOf(m, t)[0]).find(c => c[0] === cx && c[1] === cy);
        if (c) lines.push(`tribu ${esc(t)} : <b>${nf(c[2])}</b> midis de personnes ${m.on.cum ? 'jusqu’à l’an ' + Y : 'en l’an ' + Y}`);
      }
    }
    if (m.kind === 'presence' || m.kind === 'routes') {
      const passed = yd && yd.cells.get(cx + ',' + cy);
      if (passed) {
        const list = [...passed].sort((a, b) => b[1] - a[1]);
        lines.push('passés ici en l’an ' + Y + ' : ' + list.slice(0, 5).map(([id, n]) => `personne ${id} (${esc(B.people[id] || '?')}, ${n} midi${n > 1 ? 's' : ''})`).join(', ')
          + (list.length > 5 ? ` et ${list.length - 5} autres` : ''));
      } else if (yd) lines.push('<span class=m>aucune personne à midi ici en l’an ' + Y + '</span>');
    }
    m.hover = {cell: [cx, cy]};
    return lines;
  }
  function hover(m, ev) {
    const r = m.cv.getBoundingClientRect(), px = ev.clientX - r.left, py = ev.clientY - r.top;
    const [wx, wy] = toWorld(m, px, py), near = nearest(m, wx, wy);
    let html = '';
    delete m.cv.dataset.person;
    if (near && near.death) {
      const [id, , , day, cause, want] = near.death;
      html = `<b>${who(id)}</b> meurt de ${esc(CAUSE[cause] || cause)} <span class=m>(${want ? 'manque' : 'fond'})</span><br>${dayFr(day)}<br><span class=m>cliquer : sa fiche</span>`;
      m.cv.dataset.person = id; m.hover = near;
    } else if (near && near.birth) {
      html = `<b>${who(near.birth[0])}</b> naît<br>${dayFr(near.birth[3])}<br><span class=m>cliquer : sa fiche</span>`;
      m.cv.dataset.person = near.birth[0]; m.hover = near;
    } else if (near && near.track) {
      const [id, [day]] = near.track;
      html = `<b>${who(id)}</b> à midi<br>${dayFr(day)}<br><span class=m>cliquer : sa fiche</span>`;
      m.cv.dataset.person = id; m.hover = near;
    } else if (wx >= 0 && wy >= 0 && wx < B.tiles[0] && wy < B.tiles[1]) html = cellLines(m, wx, wy).join('<br>');
    else m.hover = null;
    if (!html) { m.tip.hidden = true; } else {
      m.tip.innerHTML = html; m.tip.hidden = false;
      const tw = m.tip.offsetWidth, th = m.tip.offsetHeight;
      m.tip.style.left = Math.max(4, Math.min(r.width - tw - 4, px + 14)) + 'px'; m.tip.style.top = (py + th + 18 > r.height ? Math.max(4, py - th - 10) : py + 14) + 'px';
    }
    draw(m);
  }

  // --- controls
  async function setYear(y) {
    Y = y; slider.value = y; yrLabel.textContent = 'an ' + y;
    try { await year(y); } catch (e) { yrLabel.textContent = 'an ' + y + ' (illisible)'; }
    if (Y === y) { maps.forEach(m => { m.hover = null; m.tip.hidden = true; }); legends(); drawAll(); }
  }
  function legend(m) {
    const yd = years[Y];
    if (m.kind === 'presence') return B.tribes.map(t => { const pk = presenceOf(m, t)[1];
      return `<span>tribu ${esc(t)}, ${esc(B.sides[t] || '')} : peu<i class=ramp style="background:linear-gradient(90deg,color-mix(in srgb,${tc(t)} 14%,transparent),${tc(t)})"></i>souvent (1 à ${nf(pk)} midis de personnes par case, échelle logarithmique)</span>`; }).join('')
      + '<span>Un midi de personne : une personne vue à midi dans la case, un jour.</span>';
    if (m.kind === 'routes') return yd ? `<span>an ${Y} : ${yd.deaths.length} mort${yd.deaths.length > 1 ? 's' : ''} (${yd.deaths.filter(d => d[5]).length} de manque, en rouge ; de fond en gris), ${yd.births.length} naissance${yd.births.length > 1 ? 's' : ''} (magenta), ${yd.moves.length} maison${yd.moves.length > 1 ? 's' : ''} déplacée${yd.moves.length > 1 ? 's' : ''} (sarcelle). Points cerclés de blanc : chacun au dernier midi de l’année.</span>` : '';
    if (m.kind === 'life') { const L = lifeOf(m), want = L.deaths.filter(e => e[5]).length;
      return `<span>${m.on.cum ? 'de l’an 1 à l’an ' + Y : 'en l’an ' + Y} : <b>${L.deaths.length}</b> morts (<b>${want}</b> de manque en rouge, ${L.deaths.length - want} de fond en gris), <b>${L.births.length}</b> naissances (magenta). Survoler : qui, quand ; cliquer : sa fiche.</span>`; }
    if (m.kind === 'terrain') return R && R.known ? `<span>Ombrage : la lumière vient du nord-ouest, les pentes trois fois exagérées. Altitude de ${nf(R.lo / 10)} m (clair) à ${nf(R.hi / 10)} m (foncé), une valeur tous les ${R.cell} m.</span><span>Le sol : prairie, forêt, eau, gué, ruisseau, roche.</span>` : '<span>Ce run n’a pas le relief (relancer avec le vivarium récent).</span>';
    if (m.kind === 'water') return '<span>Le reste de la carte pâli. Survoler : l’eau la plus proche et sa distance.</span>';
    if (m.kind === 'food') return F && F.cells.length ? `<span>${esc(SEASON[season(m)])} : peu<i class=ramp style="background:linear-gradient(90deg,color-mix(in srgb,var(--kept) 14%,transparent),var(--kept))"></i>beaucoup (1 à ${nf(F.peak[season(m)])} plantes et arbres qui portent par carré de ${F.chunk_tiles} m, échelle logarithmique).</span><span>Comptés seulement là où les tribus ont passé un midi, et ${F.margin_chunks} carrés autour.</span>` : '<span>Ce run n’a pas la carte de la nourriture (relancer avec le vivarium récent).</span>';
    return '';
  }
  function legends() { maps.forEach(m => { m.fig.querySelector('[data-leg6]').innerHTML = legend(m); }); }
  async function setKind(m, kind) {
    m.kind = kind; m.hover = null; m.tip.hidden = true; m.on = {};
    m.fig.querySelector('[data-kind6]').value = kind;
    m.fig.querySelector('.note6').textContent = ' · ' + NOTES[kind];
    const lay = m.fig.querySelector('[data-lay6]');
    lay.innerHTML = LAYERS[kind]().map(([k, on, label, group]) => `<label><input type=${group ? 'radio' : 'checkbox'}${group ? ` name="${group}-${maps.indexOf(m)}"` : ''} data-l="${k}"${on ? ' checked' : ''}> ${label}</label>`).join('');
    lay.querySelectorAll('[data-l]').forEach(c => {
      m.on[c.dataset.l] = c.checked;
      c.onchange = () => { lay.querySelectorAll('[data-l]').forEach(x => { m.on[x.dataset.l] = x.checked; }); legends(); draw(m); };
    });
    try { if (kind === 'terrain') await loadRelief(); if (kind === 'food') await loadFood(); }
    catch (e) { m.fig.querySelector('[data-leg6]').textContent = 'les données de cette carte n’ont pas pu être lues (' + e.message + ')'; }
    legends(); draw(m);
  }
  function bind() {
    slider.max = B.years; slider.oninput = () => setYear(+slider.value);
    box.querySelector('[data-frames6]').innerHTML = B.tribes.map(t => `<button type=button data-frame="${esc(t)}">tribu ${esc(t)}</button>`).join('')
      + '<button type=button data-frame=all>toute la carte</button>';
    box.querySelectorAll('[data-frame]').forEach(b => b.onclick = () => frame(b.dataset.frame));
    box.querySelector('[data-pale6]').onchange = drawAll;
    const play = box.querySelector('[data-play6]');
    play.onclick = () => {
      if (playing) { clearInterval(playing); playing = null; play.textContent = '▶ les années'; return; }
      play.textContent = '❚❚ arrêter';
      if (Y >= B.years) setYear(1);
      playing = setInterval(() => { if (Y >= B.years) { play.click(); return; } setYear(Y + 1); }, 900);
    };
    for (const m of maps) {
      const sel = m.fig.querySelector('[data-kind6]');
      sel.innerHTML = Object.entries(KINDS).map(([k, v]) => `<option value="${k}">${esc(v)}</option>`).join('');
      sel.onchange = () => setKind(m, sel.value);
      let drag = null, moved = false;
      m.cv.addEventListener('pointerdown', e => { drag = [e.clientX, e.clientY]; moved = false; m.cv.setPointerCapture(e.pointerId); });
      m.cv.addEventListener('pointermove', e => {
        if (drag) {
          const dx = e.clientX - drag[0], dy = e.clientY - drag[1];
          if (Math.abs(dx) + Math.abs(dy) > 3) { moved = true; m.cv.classList.add('drag'); }
          if (moved) { V.x -= dx / V.k; V.y -= dy / V.k; drag = [e.clientX, e.clientY]; m.tip.hidden = true; box.querySelectorAll('[data-frame]').forEach(x => x.setAttribute('aria-pressed', 'false')); drawAll(); return; }
        }
        hover(m, e);
      });
      m.cv.addEventListener('pointerup', () => { drag = null; m.cv.classList.remove('drag'); });
      m.cv.addEventListener('pointerleave', () => { m.tip.hidden = true; m.hover = null; delete m.cv.dataset.person; draw(m); });
      m.cv.addEventListener('click', e => { if (moved) { e.stopPropagation(); e.preventDefault(); moved = false; } }, true);
      m.cv.addEventListener('wheel', e => { e.preventDefault(); const r = m.cv.getBoundingClientRect(); zoomAt(m, e.deltaY < 0 ? 1.25 : 0.8, e.clientX - r.left, e.clientY - r.top); }, {passive: false});
      m.fig.querySelectorAll('[data-z]').forEach(b => b.onclick = () => { const [w, h] = size(m); zoomAt(m, b.dataset.z === '1' ? 1.5 : 1 / 1.5, w / 2, h / 2); });
    }
    new ResizeObserver(() => drawAll()).observe(maps[0].cv.parentElement);
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', drawAll);
  }
  async function start() {
    try {
      const r = await fetch(url('carte-base'));
      if (!r.ok) throw new Error('carte ' + r.status);
      B = await r.json();
      await loadTerrain();
    } catch (e) { box.insertAdjacentHTML('afterbegin', '<p class=muted>les cartes n’ont pas pu être lues (' + esc(e.message) + ')</p>'); return; }
    bind(); frame(B.tribes[0] || 'all');
    await Promise.all(maps.map(m => setKind(m, m.kind)));
    setYear(B.years);
  }
  const io = new IntersectionObserver(es => { if (es.some(e => e.isIntersecting)) { io.disconnect(); start(); } }, {rootMargin: '300px'});
  io.observe(box);
})();
"""

SM6_JS = r"""
(() => {
  const box = document.querySelector('[data-sm6]'), page = document.querySelector('.runpage');
  if (!box || !page) return;
  const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const tc = t => css('--t' + t) || '#888';
  const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
  const SEASON = ['printemps', 'été', 'automne', 'hiver'];
  const nf = (v, dg) => v === null || v === undefined ? '—' : (dg ? v.toFixed(dg).replace('.', ',') : String(Math.round(v))).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
  let C = null, hoverDay = null;
  const NS = 'http://www.w3.org/2000/svg';
  function nice(v) { if (v <= 0) return 1; const e = 10 ** Math.floor(Math.log10(v)), f = v / e; return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * e; }
  function valueAt(row, s, day) {
    const vals = row.values[s];
    if (row.kind === 'bars') { const y = Math.floor((day - 1) / C.dpy); return [vals[y] ?? null, 'an ' + (y + 1)]; }
    let lo = 0, hi = C.days.length - 1;
    while (lo < hi) { const mid = (lo + hi) >> 1; if (C.days[mid] < day) lo = mid + 1; else hi = mid; }
    return [vals[lo] ?? null, 'jour ' + C.days[lo]];
  }
  function chart(row, s, w, hgt, top) {
    const L = 40, R = 6, T = 6, Bm = 16, iw = w - L - R, ih = hgt - T - Bm, last = C.last || 1;
    const X = d => L + (d - 1) / last * iw, Yv = v => T + ih - v / top * ih;
    const color = row.color ? css('--' + row.color) : row.split ? tc(C.tribes[s]) : css('--ink2');
    let g = '';
    for (const [a, b] of C.winters) g += `<rect x="${X(a).toFixed(1)}" y="${T}" width="${(X(b + 1) - X(a)).toFixed(1)}" height="${ih}" fill="var(--winter)"/>`;
    for (const f of (row.digits || top % 2 === 0 ? [0, .5, 1] : [0, 1])) g += `<line x1="${L}" x2="${L + iw}" y1="${Yv(top * f).toFixed(1)}" y2="${Yv(top * f).toFixed(1)}" stroke="var(--line)"/>`
      + `<text x="${L - 4}" y="${(Yv(top * f) + 3).toFixed(1)}" text-anchor="end">${nf(top * f, top * f % 1 ? 1 : 0)}</text>`;
    const years = Math.ceil(last / C.dpy), every = years > 12 ? 5 : years > 6 ? 2 : 1;
    for (let y = 1; y <= years; y++) if (y === 1 || y % every === 0) g += `<text x="${X((y - 1) * C.dpy + 1 + C.dpy / 2).toFixed(1)}" y="${hgt - 3}" text-anchor="middle">an ${y}</text>`;
    const vals = row.values[s];
    if (row.kind === 'bars') {
      const bw = Math.max(1, iw / last * C.dpy - 2);
      vals.forEach((v, i) => { if (!v) return; const x = X(i * C.dpy + 1) + 1, y0 = Yv(v);
        g += `<rect x="${x.toFixed(1)}" y="${y0.toFixed(1)}" width="${bw.toFixed(1)}" height="${(T + ih - y0).toFixed(1)}" rx="${Math.min(2, bw / 3).toFixed(1)}" fill="${color}"/>`; });
    } else {
      let d = '', pen = false;
      vals.forEach((v, i) => { if (v === null) { pen = false; return; } d += (pen ? 'L' : 'M') + X(C.days[i]).toFixed(1) + ' ' + Yv(v).toFixed(1); pen = true; });
      g += `<path d="${d}" fill="none" stroke="${color}" stroke-width="2" stroke-linejoin="round"/>`;
    }
    g += `<line class=hair x1="0" x2="0" y1="${T}" y2="${T + ih}" stroke="var(--ink)" stroke-width="1" visibility="hidden"/>`;
    return `<svg viewBox="0 0 ${w} ${hgt}" width="${w}" height="${hgt}" data-l="${L}" data-iw="${iw}">${g}</svg>`;
  }
  function header(row, s, day) {
    const [v, when] = day === null ? [row.values[s][row.values[s].length - 1] ?? null, 'à la fin'] : valueAt(row, s, day);
    return `<span>${row.split ? `<span class="tr t${esc(C.tribes[s])}">${esc(C.tribes[s])}</span> ${esc(C.sides[C.tribes[s]] || '')}` : 'les deux tribus'} · ${esc(when)}</span><span class=v>${nf(v, row.digits)}</span>`;
  }
  function renderRow(el, row) {
    if (row.empty) { el.querySelector('.cols6').innerHTML = `<p class=muted>${esc(row.none)}, ni dans une tribu ni dans l’autre</p>`; return; }
    const n = row.split ? C.tribes.length : 1, cols = el.querySelector('.cols6');
    cols.style.gridTemplateColumns = `repeat(${n}, minmax(0, 1fr))`;
    const w = Math.max(140, Math.floor((cols.clientWidth - (n - 1) * 14) / n)), hgt = el.classList.contains('big') ? 240 : 96;
    let top = 0; for (const s of row.values) for (const v of s) if (v !== null && v > top) top = v;
    top = row.key === 'reserve_by_tribe' ? 100 : nice(top);
    cols.innerHTML = row.values.map((_, s) => `<div class=ch6 data-s="${s}"><div class=ct>${header(row, s, hoverDay)}</div>${chart(row, s, w, hgt, top)}</div>`).join('');
  }
  function setHover(day) {
    hoverDay = day;
    const now = box.querySelector('.now6');
    if (day === null) now.textContent = 'Survoler une courbe : la valeur de ce jour sur toutes.';
    else { const y = Math.floor((day - 1) / C.dpy) + 1, doy = (day - 1) % C.dpy; now.textContent = `an ${y}, ${SEASON[Math.min(3, Math.floor(doy * 4 / C.dpy))]}, jour ${day}`; }
    box.querySelectorAll('.row6').forEach(el => {
      const row = C.rows[+el.dataset.i];
      el.querySelectorAll('.ch6').forEach(ch => {
        const s = +ch.dataset.s; ch.querySelector('.ct').innerHTML = header(row, s, day);
        const svg = ch.querySelector('svg'), hair = svg.querySelector('.hair');
        if (day === null) { hair.setAttribute('visibility', 'hidden'); return; }
        const x = +svg.dataset.l + (day - 1) / C.last * +svg.dataset.iw;
        hair.setAttribute('x1', x); hair.setAttribute('x2', x); hair.setAttribute('visibility', 'visible');
      });
    });
  }
  function render() {
    box.innerHTML = '<div class=now6></div>' + C.rows.map((row, i) => `<div class=row6 data-i="${i}"><div class=rh role=button tabindex=0 aria-expanded=false><b>${esc(row.title)}</b><span>${esc(row.unit)} · ${esc(row.what)}</span></div><div class=cols6></div></div>`).join('');
    box.querySelectorAll('.row6').forEach(el => renderRow(el, C.rows[+el.dataset.i]));
    setHover(null);
    box.querySelectorAll('.rh').forEach(rh => {
      const toggle = () => { const el = rh.parentElement; el.classList.toggle('big'); rh.setAttribute('aria-expanded', el.classList.contains('big')); renderRow(el, C.rows[+el.dataset.i]); };
      rh.onclick = toggle; rh.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); } };
    });
    box.addEventListener('pointermove', e => {
      const svg = e.target.closest('svg'); if (!svg || !box.contains(svg)) return;
      const r = svg.getBoundingClientRect(), x = (e.clientX - r.left) * (+svg.getAttribute('width') / r.width);
      const f = (x - +svg.dataset.l) / +svg.dataset.iw;
      if (f < 0 || f > 1) return;
      setHover(Math.max(1, Math.min(C.last, Math.round(f * C.last) + 1)));
    });
    box.addEventListener('pointerleave', () => setHover(null));
  }
  let timer = null;
  new ResizeObserver(() => { if (!C) return; clearTimeout(timer); timer = setTimeout(() => box.querySelectorAll('.row6').forEach(el => renderRow(el, C.rows[+el.dataset.i])), 150); }).observe(box);
  const io = new IntersectionObserver(async es => {
    if (!es.some(e => e.isIntersecting)) return; io.disconnect();
    try { const r = await fetch(page.dataset.morceau + '?quoi=courbes'); if (!r.ok) throw new Error(r.status); C = await r.json(); render(); }
    catch (e) { box.innerHTML = '<p class=muted>les courbes n’ont pas pu être lues (' + esc(e.message) + ')</p>'; }
  }, {rootMargin: '300px'});
  io.observe(box);
})();
"""


# --- the home: series as cards ------------------------------------------------------------------

STAMP_RE = __import__("re").compile(r"-(\d{8}-\d{6}|\d{4}-\d{2}-\d{2})$")
WORD_FR = {"etalon": "étalon", "feu": "feu", "sansfeu": "sans feu"}


def purpose_summary(question):
    """The purpose in one clean sentence for the card: the question's first sentence, its
    parentheses left to the full text; cut at a comma past 220 characters. '' without a question."""
    re_ = __import__("re")
    q = " ".join(str(question or "").split())
    if not q:
        return ""
    m = re_.search(r"[.?!](?=\s|$)", q)
    first = q[:m.end()] if m else q
    clean = re_.sub(r"\s*\([^()]*\)", "", first)
    clean = re_.sub(r"\s+([,.])", r"\1", clean).strip()
    if len(clean) > 220:
        cut = max(clean.rfind(", ", 0, 210), clean.rfind(" ; ", 0, 210), clean.rfind(" : ", 0, 210))
        clean = (clean[:cut] if cut > 80 else clean[:210].rsplit(" ", 1)[0]).rstrip(" ,;:") + "…"
    return clean


def series_title(name, plan=None):
    """A series' title for a human eye: the plan's own `title` when it has one, else its name
    without the date stamp, its words spaced (`lab5-feu-20ans` → `Lab 5 feu 20 ans`)."""
    t = (plan or {}).get("title")
    if t:
        return str(t)
    re_ = __import__("re")
    words = []
    for w in STAMP_RE.sub("", name).split("-"):
        w = WORD_FR.get(w, w)
        w = re_.sub(r"^(\d+)ans$", r"\1 ans", w)
        w = re_.sub(r"^(lab|lot)(\d+)$", r"\1 \2", w)
        words.append(w)
    s = " ".join(words).strip() or name
    return s[0].upper() + s[1:]


HOME_CSS = """
.bar7{display:flex;flex-wrap:wrap;gap:.6em 1em;align-items:center;margin:.6em 0 1.2em;padding:.6em .8em;background:var(--surface);
 border:1px solid var(--edge);border-radius:10px}
.bar7 input[type=search]{flex:1;min-width:14em;padding:.35em .6em;border:1px solid var(--line);border-radius:8px;background:var(--page);color:var(--ink)}
.bar7 select{padding:.3em .4em;border:1px solid var(--line);border-radius:8px;background:var(--page);color:var(--ink)}
.bar7 label{font-size:.9em;color:var(--ink2)}
h2.sec7{font-size:1.25em;margin:2.2em 0 .7em;display:flex;gap:.5em;align-items:baseline}h2.sec7 small{color:var(--muted);font-weight:400;font-size:.75em}
h3.day7{font-size:.82em;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);margin:1.6em 0 .6em;font-weight:650}
.cards7{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,27em),1fr));gap:1.1em;align-items:start}
details.scard{background:var(--surface);border:1px solid var(--edge);border-radius:12px;box-shadow:var(--shadow);min-width:0}
details.scard[open]{grid-column:1/-1}
details.scard>summary{list-style:none;cursor:pointer;padding:.85em 1.1em .9em;font-weight:400}details.scard>summary::after{content:none}details.scard>summary::-webkit-details-marker{display:none}
details.scard>summary:hover .t{text-decoration:underline}
.scard .st{display:flex;gap:.7em;align-items:baseline}.scard .t{font-size:1.1em;font-weight:650;color:var(--ink);line-height:1.3}
.scard .st::after{content:"▸";margin-left:auto;color:var(--muted);font-size:.9em}details.scard[open] .st::after{content:"▾"}
.scard .mk{font-size:.82em;font-weight:600;white-space:nowrap}.scard .mk.cours{color:#9a6a00}.scard .mk.echec,.scard .mk.arret{color:var(--want)}
.scard .when{font-size:.86em;color:var(--muted);margin:.15em 0 0;font-variant-numeric:tabular-nums}
.scard .purpose{margin:.5em 0 0;font-size:1.02em;line-height:1.55;color:var(--ink);max-width:62ch}.scard .purpose.none{color:var(--muted);font-style:italic}
.scard .more{display:block;padding:0 1.1em 1em;border-top:1px solid var(--line);margin-top:.1em}
.scard .full{margin:.7em 0;max-width:72ch;line-height:1.5;color:var(--ink)}.scard .full b{display:block;font-size:.78em;text-transform:uppercase;letter-spacing:.05em;color:var(--muted);margin-bottom:.15em}
.scard .ver{font-size:.86em;color:var(--ink2)}.scard .ver code{background:var(--chip);padding:0 .35em;border-radius:4px}.scard .raw{color:var(--muted)}
.scard .res{border-left:3px solid var(--ink2);padding:.15em 0 .15em .8em;margin:.9em 0}.scard .res .vd{font-weight:650;line-height:1.4;max-width:66ch}
.scard .res ul{margin:.3em 0 0;padding-left:1.1em;font-size:.93em;line-height:1.45}.scard .res.wait{border-color:var(--line);color:var(--muted);font-size:.88em}
.scard .links{display:flex;gap:.4em 1.2em;flex-wrap:wrap;align-items:center;margin:.6em 0}.scard .links a:first-child{font-weight:650}
.scard details summary{font-size:.9em;font-weight:600;cursor:pointer}
ul.runs7{list-style:none;margin:.4em 0 0;padding:0;font-size:.88em}
ul.runs7 li{padding:.45em 0;border-top:1px solid var(--line);display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.1em .8em}
ul.runs7 li .w{color:var(--muted)}ul.runs7 li .r{grid-column:1/3;color:var(--ink2)}
.etat .sg{margin-right:.3em}
.pcards7{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,22em),1fr));gap:.8em}
.pcard{background:var(--surface);border:1px dashed var(--edge);border-radius:12px;padding:.7em 1em;font-size:.92em}
.pcard b{display:block;margin-bottom:.2em}.pcard p{margin:.2em 0;color:var(--ink2)}
"""

HOME_JS = r"""
(() => {
  const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
  const secs = s => s >= 3600 ? Math.floor(s / 3600) + ' h ' + Math.floor(s % 3600 / 60) + ' min' : s >= 60 ? Math.floor(s / 60) + ' min' + (s < 600 ? ' ' + (s % 60) + ' s' : '') : s + ' s';
  const when = t => t ? new Date(t * 1000).toLocaleString('fr-FR', {weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit'}) : '—';
  const hour = t => new Date(t * 1000).toLocaleTimeString('fr-FR', {hour: '2-digit', minute: '2-digit'});
  const sameDay = (a, b) => new Date(a * 1000).toDateString() === new Date(b * 1000).toDateString();
  const dayOf = t => t ? new Date(t * 1000).toLocaleDateString('fr-FR', {weekday: 'long', day: 'numeric', month: 'long', year: 'numeric'}) : 'date inconnue';
  const kind = s => s.startsWith('en cours') || s === 'attend' ? 'cours' : s === 'fini' || s.startsWith('terminée') ? (s.includes('échec') ? 'echec' : 'fini')
    : s.startsWith('ÉCHEC') || s.startsWith('perdu') ? 'echec' : s.startsWith('arrêt') ? 'arret' : '';
  const SIGN = {cours: '◐', fini: '✓', echec: '✕', arret: '■', '': '·'};
  const chip = s => `<span class="etat ${kind(s)}"><span class=sg aria-hidden=true>${SIGN[kind(s)]}</span>${esc(s)}</span>`;
  const q = document.getElementById('q7'), fs = document.getElementById('f7'), so = document.getElementById('s7'), gd = document.getElementById('g7');
  const open = new Set(), pending = new Set(), results = {};
  let last = [];
  const variant = r => [r.base, r.trials === false ? 'sans essais' : 'avec essais', r.fire === false ? 'sans le feu' : 'avec le feu', 'graine ' + (r.seed ?? '?')].filter(Boolean).join(' · ');
  function runLine(s, r) {
    const res = (results[s.name] || {}).runs || {};
    const t = r.started ? (r.ended ? `${hour(r.started)} → ${hour(r.ended)} (${secs(r.elapsed)})` : `depuis ${hour(r.started)} (${secs(r.elapsed)})`) : 'pas encore lancé';
    const line = res[r.name] || (r.state === 'en cours' ? `année ${r.year ?? '—'} · ${r.alive ?? '—'} vivants · ${r.dead ?? '—'} morts (d’après le log)` :
      r.data ? 'résultat en lecture…' : r.alive != null ? `${r.alive} vivants · ${r.dead} morts (d’après le log)` : '');
    return `<li><span><a href="${esc(r.read)}">${esc(variant(r))}</a> <span class=w>${esc(t)}</span></span>${kind(r.state) === 'fini' ? '<span></span>' : `<span class="mk ${kind(r.state)}">${SIGN[kind(r.state)]} ${esc(r.state)}</span>`}<span class=r>${esc(line)}</span></li>`;
  }
  function card(s) {
    const running = s.state === 'en cours' && s.owned, k = kind(s.state), v = s.version || {};
    const stop = running ? (pending.has(s.dir)
      ? `<span class="confirm">Arrêter <b>${esc(s.title)}</b> et tuer ses runs ? <form method="post" action="/arreter" style="display:inline"><input type="hidden" name="serie" value="${esc(s.dir)}"><button class="danger">Oui, arrêter</button></form> <button data-cancel="${esc(s.dir)}">Non</button></span>`
      : `<button data-stop="${esc(s.dir)}">Arrêter la série</button>`) : '';
    const end = s.ended ? (sameDay(s.started, s.ended) ? hour(s.ended) : when(s.ended)) : (k === 'cours' ? 'en cours' : '…');
    const mark = k === 'fini' ? '' : `<span class="mk ${k}">${SIGN[k]} ${esc(s.state)}</span>`;
    const purpose = s.purpose ? `<p class=purpose>${esc(s.purpose)}</p>` : '<p class="purpose none">but non écrit</p>';
    const R = results[s.name];
    const res = k === 'cours' ? '' : !R ? '<div class="res wait">les résultats se lisent…</div>' : R.verdict
      ? `<div class=res><div class=vd>${esc(R.verdict)}</div>${R.points.length ? '<ul>' + R.points.map(p => `<li>${p}</li>`).join('') + '</ul>' : ''}</div>`
      : '<div class="res wait">aucune donnée de run à lire</div>';
    const full = s.question && s.question !== s.purpose ? `<p class=full><b>Le but en entier</b>${esc(s.question)}</p>` : '';
    const change = v.change ? `<p class=full><b>Ce qui a changé</b>${esc(v.change)}</p>` : '';
    const ver = `<p class=ver>version <code>${esc(v.commit || 'inconnue')}</code>${v.branch && v.branch !== 'HEAD' ? ' · branche ' + esc(v.branch) : ''}${v.dirty ? ' · avec des modifications' : ''} · durée ${esc(secs(s.elapsed))} · <span class=raw>${esc(s.name)}</span></p>`;
    const note = s.note ? `<p class="warn">${esc(s.note)}</p>` : '';
    const links = `<p class=links><a href="/serie/${encodeURIComponent(s.name)}">Lire la série →</a>${s.synthese ? `<a href="${esc(s.synthese)}">synthèse</a>` : ''}${stop}</p>`;
    const runs = `<details data-runs="${esc(s.name)}"${open.has('r:' + s.name) || s.runs.length <= 6 ? ' open' : ''}><summary>les ${s.runs.length} runs</summary><ul class=runs7>${s.runs.map(r => runLine(s, r)).join('')}</ul></details>`;
    return `<details class=scard data-card="${esc(s.name)}"${open.has(s.name) ? ' open' : ''}><summary><div class=st><span class=t>${esc(s.title)}</span>${mark}</div>`
      + `<div class=when>${esc(when(s.started))} → ${esc(end)} · ${s.runs.length} run${s.runs.length > 1 ? 's' : ''}</div>${purpose}</summary>`
      + `<div class=more>${res}${full}${change}${ver}${note}${links}${runs}</div></details>`;
  }
  function render() {
    const needle = q.value.trim().toLowerCase(), f = fs.value;
    let list = last.filter(s => (!needle || [s.title, s.name, s.question, s.purpose, (s.version || {}).change, (s.version || {}).commit, (s.version || {}).branch].join(' ').toLowerCase().includes(needle))
      && (f === 'toutes' || kind(s.state) === f));
    const by = so.value;
    list.sort(by === 'ancien' ? (a, b) => (a.started || 0) - (b.started || 0) : by === 'but' ? (a, b) => (a.question || '~').localeCompare(b.question || '~', 'fr')
      : by === 'etat' ? (a, b) => kind(a.state).localeCompare(kind(b.state)) || (b.started || 0) - (a.started || 0) : (a, b) => (b.started || 0) - (a.started || 0));
    const groups = [['En cours', list.filter(s => kind(s.state) === 'cours')], ['Terminées', list.filter(s => kind(s.state) !== 'cours')]];
    document.getElementById('series').innerHTML = groups.map(([title, ss]) => {
      if (!ss.length) return title === 'En cours' && !needle && f === 'toutes' ? '<h2 class=sec7>En cours <small>rien ne tourne</small></h2>' : '';
      let body = '';
      if (gd.checked && by !== 'but' && by !== 'etat') {
        let cur = null, buf = [];
        const flush = () => { if (buf.length) body += `<h3 class=day7>${esc(cur)}</h3><div class=cards7>${buf.join('')}</div>`; buf = []; };
        for (const s of ss) { const d = dayOf(s.started); if (d !== cur) { flush(); cur = d; } buf.push(card(s)); }
        flush();
      } else body = `<div class=cards7>${ss.map(card).join('')}</div>`;
      return `<h2 class=sec7>${title} <small>${ss.length} série${ss.length > 1 ? 's' : ''}</small></h2>${body}`;
    }).join('') || '<p class=muted>Aucune série ne répond à la recherche.</p>';
    if (!last.length) document.getElementById('series').innerHTML = '<p class="muted">Aucune série dans le dossier des runs. <a href="/nouvelle">Créer la première</a>.</p>';
  }
  document.addEventListener('click', e => {
    const st = e.target.closest('[data-stop]'), ca = e.target.closest('[data-cancel]');
    if (st) { pending.add(st.dataset.stop); render(); } else if (ca) { pending.delete(ca.dataset.cancel); render(); }
  });
  document.addEventListener('toggle', e => {
    const d = e.target, key = d.dataset && (d.dataset.card || (d.dataset.runs && 'r:' + d.dataset.runs));
    if (key) d.open ? open.add(key) : open.delete(key);
  }, true);
  [q, fs, so, gd].forEach(x => x.addEventListener('input', render));
  const asked = {};
  async function readResults() {
    for (const s of last) {
      if (kind(s.state) === 'cours') continue;
      const key = s.name + '@' + (s.ended || 0);
      if (asked[s.name] === key) continue;
      asked[s.name] = key;
      try { const r = await fetch('/api/resultats?serie=' + encodeURIComponent(s.name)); if (r.ok) { results[s.name] = await r.json(); render(); } }
      catch (e) { asked[s.name] = null; }
    }
  }
  async function tick() {
    try {
      const r = await fetch('/api/series'); last = await r.json();
      document.getElementById('err').textContent = '';
      if (!document.activeElement || !document.activeElement.closest('.scard .more')) render();
      readResults();
    } catch (e) { document.getElementById('err').textContent = 'Le laboratoire ne répond plus (' + e + ').'; }
  }
  tick(); setInterval(tick, 3000);
})();
"""


def home_body(lab, standard_plan):
    """The home: a bar to search, filter, sort and group; the series as cards (filled by the
    script from `/api/series`, results from `/api/resultats`); the plans at the end."""
    plans = "".join(
        f'<div class=pcard><b>{h(series_title(p, standard_plan(p)))}</b><p>{h(standard_plan(p).get("question", "")) or "<span class=none>but non écrit</span>"}</p>'
        f'<p class=muted><code>{h(p)}</code> · plan étalon : une série neuve à chaque lancement, depuis <a href="/nouvelle">Nouvelle série</a></p></div>'
        for p in lab.standard_plans())
    return ('<h1>Les séries</h1>'
            '<div class=bar7><input type=search id=q7 placeholder="chercher : nom, but, version, ce qui a changé" aria-label="chercher">'
            '<label>état <select id=f7><option value=toutes>toutes</option><option value=cours>en cours</option><option value=fini>terminées</option>'
            '<option value=echec>en échec</option><option value=arret>arrêtées</option></select></label>'
            '<label>tri <select id=s7><option value=recent>lancement, récent d’abord</option><option value=ancien>lancement, ancien d’abord</option>'
            '<option value=but>but (A → Z)</option><option value=etat>état</option></select></label>'
            '<label><input type=checkbox id=g7 checked> grouper par jour</label></div>'
            '<p id=err class=warn></p><div id=series><p class=muted>Chargement…</p></div>'
            f'<h2 class=sec7>Plans <small>{len(lab.standard_plans())} plans étalons</small></h2><div class=pcards7>{plans}</div>'
            f'<script>{HOME_JS}</script>')
HOME_CSS += (".tile,.serie,section.tribe,.mapbox,svg.chart,table,.controls,form.f{border:1px solid var(--edge);box-shadow:var(--shadow)}\n"
             "section.group{border-top-color:var(--edge)}\n")
HOME_CSS += ".etat.cours,.etat.fini,.etat.echec,.etat.perdu,.etat.arret{color:#1f2328}\n"


# --- food and tribes, year by year: the big figures, the trend, a strip a tribe, the table folded ---

P = reading.point
SEASONS_FR = ["printemps", "été", "automne", "hiver"]


def ratio_words(a, b):
    """`triple`, `double`, `+40 %`, `baisse de moitié` from a to b; '' when either is missing."""
    if not a or b is None:
        return ""
    r = b / a
    if r >= 2.75:
        return f"× {fmt(round(r, 1))}"
    if r >= 1.85:
        return "double"
    if r <= 0.55:
        return "baisse de moitié"
    pct = round((r - 1) * 100)
    return f"{'+' if pct >= 0 else '−'}{abs(pct)} %"


def stored_days(y):
    st = ((y.get("food") or {}).get("stored_kj") or {}).get("noon_mean")
    pop = y.get("population") or y.get("population_before")
    return st / pop / reading.KJ_PER_PERSON_DAY if st is not None and pop else None


def want_of(y):
    return sum((y.get("deaths") or {}).get(k) or 0 for k in reading.WANT_CAUSES)


def years_points(d):
    ts = reading.tribes(d)
    pts = []
    for t in ts:
        ys = [y for y in reading.years_of(d, t) if y.get("food")]
        if not ys:
            continue
        first, last = ys[0], ys[-1]
        a, b = stored_days(first), stored_days(last)
        if a is not None and b is not None:
            pts.append(P(f"{fmt(round(a, 1))} → {fmt(round(b, 1))} jours", f"de nourriture portée par personne chez {t}, an {first['year']} à an {last['year']}"
                         + (f" ({ratio_words(a, b)})" if ratio_words(a, b) else ""), [stored_days(y) for y in ys]))
        pop = [y.get("population") for y in ys]
        pts.append(P(f"{fmt(first.get('population_before'))} → {fmt(last.get('population'))} vivants", f"chez {t}", pop))
    allys = [y for y in d.get("years", []) if y.get("food")]
    gathered = sum(y["food"].get("gathered_kj") or 0 for y in allys)
    spoiled = sum(y["food"].get("spoiled_kj") or 0 for y in allys)
    if gathered:
        pts.append(P(f"{round(spoiled * 100 / gathered)} %", "de ce qui est cueilli pourrit en route"))
    thin = [(y.get("year"), y.get("tribe"), (y["food"].get("thin_hours") or 0) / 24) for y in allys]
    worst = max(thin, key=lambda x: x[2], default=None)
    if worst and worst[2] >= 1:
        pts.append(P(f"an {worst[0]}", f"le plus maigre : {fmt(round(worst[2]))} jours-personne maigres chez {worst[1]}"))
    return pts[:6]


def years_tiles(d):
    allys = [y for y in d.get("years", []) if y.get("food")]
    kd = reading.KJ_PER_PERSON_DAY
    g = sum(y["food"].get("gathered_kj") or 0 for y in allys)
    e = sum(y["food"].get("eaten_kj") or 0 for y in allys)
    sp = sum(y["food"].get("spoiled_kj") or 0 for y in allys)
    thin = sum(y["food"].get("thin_hours") or 0 for y in allys) / 24
    births = sum(y.get("births") or 0 for y in d.get("years", []))
    deaths = sum(y.get("deaths_total") or 0 for y in d.get("years", []))
    tiles = [("cueilli", f"{fmt(round(g / kd))}", "jours de nourriture d’une personne"),
             ("mangé", f"{fmt(round(e / kd))}", "jours de nourriture"),
             ("pourri", f"{fmt(round(sp / kd))}", f"jours, {round(sp * 100 / g) if g else 0} % du cueilli"),
             ("jours maigres", fmt(round(thin)), "jours-personne sous la moitié de la réserve"),
             ("naissances · morts", f"{fmt(births)} · {fmt(deaths)}", "sur tout le run")]
    return '<div class=ytiles>' + "".join(f'<div class=yt><div class=k>{h(k)}</div><div class=v>{h(v)}</div><div class=s>{h(s_)}</div></div>'
                                          for k, v, s_ in tiles) + '</div>'


def year_strip(d):
    """A strip a tribe, a cell a year, coloured by the food carried at noon (days a person, one hue a
    tribe), the year's number and signs on the remarkable years."""
    ts = reading.tribes(d)
    ly = reading.last_year(d) or 0
    peak = max((stored_days(y) or 0 for y in d.get("years", [])), default=0) or 1
    rows = []
    for i, t in enumerate(ts):
        ys = {y.get("year"): y for y in reading.years_of(d, t)}
        vals = {yr: stored_days(ys[yr]) for yr in ys if ys[yr].get("food")}
        marks = {}

        def mark(yr, sign, word):
            marks.setdefault(yr, []).append((sign, word))
        if vals:
            mark(max(vals, key=lambda k: vals[k] or 0), "★", "le plus de nourriture portée")
            mark(min(vals, key=lambda k: vals[k] if vals[k] is not None else 1e9), "▼", "le moins de nourriture portée")
        births = {yr: y.get("births") or 0 for yr, y in ys.items()}
        if births and max(births.values()):
            mark(max(births, key=births.get), "▲", "le plus de naissances")
        wants = {yr: want_of(y) for yr, y in ys.items()}
        if wants and max(wants.values()):
            mark(max(wants, key=wants.get), "✕", "le plus de morts de manque")
        cells = []
        for yr in range(1, ly + 1):
            y = ys.get(yr)
            v = vals.get(yr)
            a = 0.12 + 0.78 * (v / peak) if v else 0.05
            sign = "".join(s for s, _ in marks.get(yr, []))
            tip = (f"tribu {t}, an {yr} : " + (f"{fmt(round(v, 1))} j de nourriture portée par personne à midi, " if v is not None else "")
                   + (f"{fmt(y.get('population'))} vivants, {fmt(y.get('births'))} naissances, {fmt(y.get('deaths_total'))} morts "
                      f"({fmt(want_of(y))} de manque)" if y else "")
                   + ("" if not marks.get(yr) else " — " + ", ".join(w for _, w in marks[yr])))
            cells.append(f'<div class="yc{" hot" if sign else ""}" style="--a:{a:.2f};--c:var(--t{h(t)})" title="{h(tip)}">'
                         f'<span class=n>{yr}</span><span class=m>{h(sign)}</span></div>')
        rows.append(f'<div class=yrow><span class=yl>{reading.tribe_chip(t)}</span><div class=ycells style="--n:{ly}">{"".join(cells)}</div></div>')
    legend = ('<p class=caption>Une case par an ; plus la couleur est pleine, plus la tribu porte de nourriture à midi (jours par personne, '
              f'au plus {fmt(round(peak, 1))}). ★ le plus de nourriture portée, ▼ le moins, ▲ le plus de naissances, ✕ le plus de morts de manque. '
              'Survoler une case : ses chiffres.</p>')
    return f'<div class=ystrip>{"".join(rows)}</div>{legend}'


def years_section(d):
    return ('<section id=annees class=group><h2>Nourriture et tribus, année par année</h2>'
            + reading.points_html(years_points(d)) + years_tiles(d) + year_strip(d)
            + '<details class=fold><summary>Le tableau complet, une ligne par tribu et par an</summary><div class=scroll data-lazy="annees">'
            '<p class=muted>le tableau se charge…</p></div></details></section>')


# --- the chronicle's notable facts, chosen from the data ------------------------------------------

def notable_facts(d, limit=10):
    """The run's notable facts, `(day, priority, text)`, chosen from the data and never written by hand:
    deaths of want by season, the camps that moved, the first time each way was kept, the years of most
    births and of the highest and lowest population."""
    yd = reading.year_days(d)
    ts = reading.tribes(d)
    facts = []
    groups = {}
    for p in d.get("people", []):
        died = p.get("died")
        if died and reading.is_want(died.get("cause")) and died.get("day"):
            day = died["day"]
            key = ((day - 1) // yd + 1, min(3, ((day - 1) % yd) * 4 // yd), p.get("tribe"), died.get("cause"))
            groups.setdefault(key, []).append(day)
    for (yr, s, t, cause), days in groups.items():
        n = len(days)
        facts.append((min(days), 0 if n > 1 else 2,
                      f"{SEASONS_FR[s]} de l’an {yr} : {n} mort{'s' if n > 1 else ''} de {reading.cause_fr(cause)} chez {t}"))
    for t in ts:
        stays = reading.camp_stays(d, t)
        for (_a0, _b0, c0), (a1, _b1, c1) in zip(stays, stays[1:]):
            ang = math.degrees(math.atan2(c1[1] - c0[1], c1[0] - c0[0]))
            direction = reading.COMPASS[int(((ang + 360 + 22.5) % 360) // 45)]
            dist = math.hypot(c1[0] - c0[0], c1[1] - c0[1])
            facts.append(((a1 - 1) * yd + 1, 0, f"an {a1} : {t} déplace son camp vers {direction}, à {fmt(round(dist))} m"))
        firsts = {}
        for p in d.get("people", []):
            if p.get("tribe") != t:
                continue
            for tr in p.get("trials") or []:
                if tr.get("kept") and tr.get("judged_day"):
                    k = tr.get("step")
                    if k not in firsts or tr["judged_day"] < firsts[k][0]:
                        firsts[k] = (tr["judged_day"], p.get("id"))
        for k, (day, pid) in firsts.items():
            facts.append((day, 1, f"an {(day - 1) // yd + 1} : chez {t}, personne {pid} garde pour la première fois « {reading.kind_fr(k)} »"))
        ys = reading.years_of(d, t)
        if ys:
            top = max(ys, key=lambda y: y.get("births") or 0)
            if (top.get("births") or 0) >= 2:
                facts.append((top.get("first_day") or 1, 2, f"an {top['year']} : {top['births']} naissances chez {t}, le plus du run"))
            hi = max(ys, key=lambda y: y.get("population") or 0)
            lo = min(ys, key=lambda y: y.get("population") or 0)
            if hi.get("population") != lo.get("population"):
                facts.append((hi.get("last_day") or 1, 3, f"an {hi['year']} : {t} compte {reading.plural(hi['population'], 'vivant')}, son plus haut"))
                facts.append((lo.get("last_day") or 1, 3, f"an {lo['year']} : {t} tombe à {reading.plural(lo['population'], 'vivant')}, son plus bas"))
    # One from each kind in turn, the most urgent kind first, so that the many first ways kept
    # do not crowd out the births and population records.
    buckets = {}
    for f in sorted(facts, key=lambda f: (f[1], f[0])):
        buckets.setdefault(f[1], []).append(f)
    picked, queues = [], [buckets[k] for k in sorted(buckets)]
    while len(picked) < limit and any(queues):
        for q in queues:
            if q and len(picked) < limit:
                picked.append(q.pop(0))
    chosen = sorted(picked, key=lambda f: f[0])
    return chosen, len(facts)


def chronicle_lead(d):
    """The chronicle's head: the notable facts."""
    facts, total = notable_facts(d)
    items = "".join(f'<li><span class=fd>{h(text.split(" : ", 1)[0])}</span>{reading.link_people(h(text.split(" : ", 1)[1] if " : " in text else text))}</li>'
                    for _day, _p, text in facts)
    return ('<h3>Les faits notables</h3>'
            + (f'<ul class=facts>{items}</ul><p class=caption>{len(facts)} faits choisis sur {total} trouvés dans les données : '
               'les morts de manque par saison, les camps déplacés, la première fois que chaque façon est gardée, les records de naissances '
               'et de population.</p>' if items else '<p class=muted>rien de notable dans les données</p>'))


# --- ages and families ---------------------------------------------------------------------------

AGE_BANDS = [(0, 10), (10, 20), (20, 30), (30, 40), (40, 50), (50, 60), (60, 200)]


def pyramid(d, at_day, title):
    yd = reading.age_year_days(d)
    counts = {b: {"man": 0, "woman": 0} for b in AGE_BANDS}
    for p in d.get("people", []):
        born = p.get("born_day") or 1
        died = (p.get("died") or {}).get("day")
        if p.get("born") != "start" and born > at_day:
            continue
        if died is not None and died <= at_day:
            continue
        a = reading.age_days(d, p, at_day)
        if a is None or p.get("sex") not in ("man", "woman"):
            continue
        yrs = a / yd
        for b in AGE_BANDS:
            if b[0] <= yrs < b[1]:
                counts[b][p["sex"]] += 1
    peak = max([1] + [c[s] for c in counts.values() for s in c])
    W, row = 300, 18
    Hh = row * len(AGE_BANDS) + 22
    mid = W / 2
    parts = [f'<svg viewBox="0 0 {W} {Hh}" class=pyr role=img aria-label="{h(title)}">',
             f'<text x="{mid - 6}" y="12" text-anchor=end>hommes</text><text x="{mid + 6}" y="12">femmes</text>']
    for i, b in enumerate(reversed(AGE_BANDS)):
        y = 18 + i * row
        label = f"{b[0]}+" if b[1] >= 200 else f"{b[0]}–{b[1] - 1}"
        m, f = counts[b]["man"], counts[b]["woman"]
        lm, lf = (mid - 42) * m / peak, (mid - 42) * f / peak   # room left for the count beside the longest bar
        parts.append(f'<rect x="{mid - 18 - lm:.1f}" y="{y}" width="{lm:.1f}" height="{row - 4}" rx=2 fill="var(--ink2)"><title>{label} ans : {m} hommes</title></rect>'
                     f'<rect x="{mid + 18:.1f}" y="{y}" width="{lf:.1f}" height="{row - 4}" rx=2 fill="var(--muted)"><title>{label} ans : {f} femmes</title></rect>'
                     f'<text x="{mid}" y="{y + row - 7}" text-anchor=middle class=al>{label}</text>'
                     + (f'<text x="{mid - 22 - lm:.1f}" y="{y + row - 7}" text-anchor=end>{m}</text>' if m else "")
                     + (f'<text x="{mid + 22 + lf:.1f}" y="{y + row - 7}">{f}</text>' if f else ""))
    parts.append("</svg>")
    total = sum(c["man"] + c["woman"] for c in counts.values())
    return f'<figure class=pyrf><figcaption><b>{h(title)}</b> · {total} vivants</figcaption>{"".join(parts)}</figure>'


def families(d):
    """The families: children grouped by mother and father, with the couples' days."""
    by = {}
    people = {p.get("id"): p for p in d.get("people", [])}
    for p in d.get("people", []):
        if p.get("mother") is None and p.get("father") is None:
            continue
        by.setdefault((p.get("mother"), p.get("father")), []).append(p)
    out = []
    for (m, f), kids in by.items():
        kids.sort(key=lambda p: p.get("born_day") or 0)
        tribe = (people.get(m) or people.get(f) or kids[0]).get("tribe")
        out.append({"mother": m, "father": f, "tribe": tribe, "kids": kids,
                    "alive": sum(1 for k in kids if not k.get("died")), "in_run": sum(1 for k in kids if k.get("born") != "start")})
    out.sort(key=lambda x: (-x["in_run"], -len(x["kids"])))
    return out


def pyramids_html(d):
    """The ages at the first day and at the last, side by side, in age-years (D09)."""
    ay = reading.age_year_days(d)
    return (f'<div class=pyrs>{pyramid(d, 1, "Au premier jour")}{pyramid(d, reading.run_days(d), "Au dernier jour")}</div>'
            '<p class=caption>Par tranches de dix années d’âge (une année d’âge : ' + fmt(ay) + ' jours de jeu) ; hommes à gauche, femmes à droite.</p>')


def families_fold(d, fams=None):
    """The families, a card each, folded: the children by mother and father."""
    yd = reading.year_days(d)
    fams = families(d) if fams is None else fams
    if not fams:
        return ""
    cards = []
    for x in fams:
        kids = "".join(f'<li>{reading.person_link(k.get("id"))} <span class=muted>'
                       + ("au départ" if k.get("born") == "start" else f"né{'e' if k.get('sex') == 'woman' else ''} l’an {((k.get('born_day') or 1) - 1) // yd + 1}")
                       + (f", mort{'e' if k.get('sex') == 'woman' else ''} l’an {((k['died'].get('day') or 1) - 1) // yd + 1} ({h(reading.cause_fr(k['died'].get('cause')))})" if k.get("died") else "")
                       + "</span></li>" for k in x["kids"])
        cards.append(f'<details class=famc><summary>{reading.tribe_chip(x["tribe"])} {reading.person_link(x["mother"])} et {reading.person_link(x["father"])}'
                     f' <span class=muted>· {len(x["kids"])} enfant{"s" if len(x["kids"]) > 1 else ""}, {x["alive"]} vivant{"s" if x["alive"] > 1 else ""}</span></summary>'
                     f'<ul>{kids}</ul></details>')
    return f'<details class=fold><summary>Les {len(fams)} familles, une carte chacune</summary><div class=fams>{"".join(cards)}</div></details>'


SECTIONS_CSS = """
.ytiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(9.5em,1fr));gap:.7em;margin:.8em 0 1em}
.yt{background:var(--surface);border:1px solid var(--edge);border-radius:10px;padding:.55em .8em;box-shadow:var(--shadow)}
.yt .k{font-size:.82em;color:var(--ink2);font-weight:600}.yt .v{font-size:1.5em;font-weight:700;font-variant-numeric:tabular-nums}.yt .s{font-size:.78em;color:var(--muted)}
.ystrip{display:flex;flex-direction:column;gap:.35em;margin:.6em 0}.yrow{display:flex;gap:.5em;align-items:center}
.ycells{display:grid;grid-template-columns:repeat(var(--n),minmax(1.6em,1fr));gap:2px;flex:1;overflow-x:auto}
.yc{position:relative;height:2.4em;border-radius:4px;background:color-mix(in srgb,var(--c) calc(var(--a)*100%),var(--surface));
 border:1px solid var(--edge);display:flex;flex-direction:column;align-items:center;justify-content:center;font-size:.72em;cursor:default}
.yc .n{color:var(--ink);font-variant-numeric:tabular-nums}.yc .m{font-size:1.05em;line-height:1;color:var(--ink);font-weight:700}
.yc.hot{border-color:var(--ink);border-width:2px}
details.fold{margin:.9em 0}details.fold>summary{font-weight:600}
ul.facts{list-style:none;padding:0;margin:.3em 0 .6em;max-width:52em}
ul.facts li{padding:.35em 0 .35em .9em;border-left:3px solid var(--edge);margin:.15em 0;line-height:1.4}
ul.facts .fd{font-weight:700;margin-right:.5em;font-variant-numeric:tabular-nums}ul.facts .fd::after{content:" :"}
details.chronall>summary{font-weight:600;margin:.8em 0}
.pyrs{display:flex;flex-wrap:wrap;gap:1em}figure.pyrf{margin:0;flex:1;min-width:17em;max-width:26em;background:var(--surface);
 border:1px solid var(--edge);border-radius:10px;padding:.5em .7em;box-shadow:var(--shadow)}figure.pyrf figcaption{font-size:.9em}
svg.pyr{width:100%;height:auto}svg.pyr text{font-size:10px;fill:var(--muted)}svg.pyr text.al{fill:var(--ink2)}
.fams{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,20em),1fr));gap:.5em;margin-top:.5em}
details.famc{background:var(--surface);border:1px solid var(--edge);border-radius:8px;padding:.4em .7em}details.famc ul{margin:.3em 0;padding-left:1.1em;font-size:.9em}
"""



