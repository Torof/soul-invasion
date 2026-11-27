#!/usr/bin/env python3
"""The page of the planet for the project lead, in one command.

    tools/page.py --out DIR --version 12 [--seeds 14,9] [--settings file.ron]
                  [--changes changes.html] [--previous 11] [--no-run]

Runs the `planet` example (release) on the seeds with every layer, puts the
layers in DIR/couches and the numbers in DIR/donnees-vN, and writes
DIR/carte.html from tools/carte.html: the two views, the journal of the
plates' events beside the history's slider, the strip of the plates' lives.

--changes   a file of <li> items: what changed since the version before.
--previous  keep the page that is there as carte-vM.html, with its layers
            (couches-vM); only the version before is kept, older archives
            are listed, never made again.
--no-run    only write the page again from the numbers already there.

The lead's folder is content/ai-reserve/review/monde-nouveau/etape-0 of the
main checkout (not in the repository).
"""
import argparse
import datetime
import json
import os
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SERVER = HERE.parent.parent
MONTHS = "janv. févr. mars avr. mai juin juil. août sept. oct. nov. déc.".split()


def fr(value, digits=1):
    return f"{value:.{digits}f}".replace(".", ",")


def sp(number):
    return f"{number:,}".replace(",", " ")


def texts(numbers, version):
    """The two texts of a seed, from its numbers (graine-N.json)."""
    h = numbers["history"]
    seed = numbers["seed"]
    lands = h["largest_lands"]
    plates = [p[0] for p in h["plates_today"]]
    planete = (
        f"<p><b>La planète aujourd'hui, version {version}.</b> Terres : {fr(numbers['land_total_km2'] / 1e6)} M km², "
        f"dont {numbers['land_count_over_1M']} de plus de 1 M km² ; la plus grande : {fr(lands[0]['area_km2'] / 1e6)} M km², "
        f"≈ {sp(lands[0]['span_km'])} km d'un bout à l'autre. Plaques : {len(plates)}, la plus grande "
        f"{round(max(plates) / 1e6)} M km².</p>"
        "<p>Les plaques sont nées des rifts (vue « histoire »). Les taches couleur sable sont des cuvettes sous le niveau "
        "de la mer sans lien avec l'océan : des terres. Les chiffres de cette graine : "
        f"<code>donnees-v{version}/graine-{seed}.json</code>.</p>"
    )
    # A rift's row: when it began, its plate, the plate born, the continent's
    # area, abandoned?, when it cut, when its ocean opened, when abandoned.
    never = 65535
    rifts = h["rifts"]
    cut = [r for r in rifts if r[5] != never]
    failed = [r for r in rifts if r[4]]
    histoire = (
        f"<p><b>{h['my']} millions d'années : {len(rifts)} fossés de rift ouverts, {len(cut)} devenus des océans, "
        f"{len(failed)} abandonnés, {len(rifts) - len(cut) - len(failed)} encore en cours aujourd'hui.</b> "
        "Glisse le curseur ou lance ▶. Sous le curseur : <b>ce qui arrive aux plaques à chaque étape</b> (clique sur une "
        "ligne pour voir l'endroit). Bouton « Frise des plaques » : une barre par plaque, de sa naissance à sa fin.</p>"
        f"<p>Au début, un supercontinent sur {h['continent_plates_at_start']} plaque(s), {h['plates_at_start']} plaques "
        "en tout. Il fait couvercle : le <b>manteau chauffe dessous</b> (orange)."
    )
    if cut:
        first = min(cut, key=lambda r: -r[0])
        histoire += (
            f" Le premier fossé (trait rouge) apparaît il y a {first[0]} Ma, s'allonge, et la plaque se coupe il y a "
            f"{first[5]} Ma : {first[0] - first[5]} Ma de fossé avant la mer."
        )
    histoire += (
        f" {len(h['collisions'])} collisions, {len(h['welds'])} soudures.</p>"
        "<p>Les taches couleur sable sont des cuvettes sous le niveau de la mer que l'océan n'atteint pas : des terres.</p>"
    )
    return planete, histoire


def before(out, version):
    """The latest archived page, carte-vM.html, at or before this version."""
    kept = [p.stem[len("carte-v"):] for p in out.glob("carte-v*.html")]
    kept = sorted((int(v) for v in kept if v.isdigit() and version.isdigit() and int(v) <= int(version)), reverse=True)
    return str(kept[0]) if kept else None


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", required=True, type=pathlib.Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--seeds", default="14,9")
    parser.add_argument("--settings")
    parser.add_argument("--changes", type=pathlib.Path)
    parser.add_argument("--previous")
    parser.add_argument("--no-run", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    seeds = [int(s) for s in args.seeds.split(",")]
    data = out / f"donnees-v{args.version}"
    layers = out / "couches"
    if args.previous and (out / "carte.html").exists():
        kept = out / f"couches-v{args.previous}"
        assert not kept.exists(), f"{kept} is already there"
        page = (out / "carte.html").read_text().replace("couches/", f"couches-v{args.previous}/")
        (out / f"carte-v{args.previous}.html").write_text(page)
        layers.rename(kept)
        (out / "carte.html").unlink()
        older = sorted(p.name for p in out.glob("couches-v*") if p != kept)
        if older:
            print(f"older archives, not needed by this page nor by the one before it: {', '.join(older)}")
    if not args.no_run:
        cage = ["soul-cap"] if shutil.which("soul-cap") else []
        command = cage + ["cargo", "run", "--release", "-q", "-p", "planet", "--example", "planet", "--"]
        if args.settings:
            command += ["--settings", os.path.abspath(args.settings)]
        env = {**os.environ, "PLANET_OUT": str(data), "PLANET_SEEDS": args.seeds, "PLANET_LAYERS": "1"}
        subprocess.run(command, cwd=SERVER, env=env, check=True)
        layers.mkdir(parents=True, exist_ok=True)
        for file in (data / "couches").iterdir():
            shutil.move(str(file), str(layers / file.name))
        (data / "couches").rmdir()
    numbers = {seed: json.loads((data / f"graine-{seed}.json").read_text()) for seed in seeds}
    both = {seed: texts(numbers[seed], args.version) for seed in seeds}
    settings = numbers[seeds[0]]["settings"]
    today = datetime.date.today()
    page = {
        "version": args.version,
        "date": f"{today.day} {MONTHS[today.month - 1]} {today.year}",
        "graines": seeds,
        "etapes": settings["steps"],
        "pas": settings["step_my"],
        "avant": args.previous or before(out, args.version),
        "titre_changements": "Ce qui change dans cette version",
        "changements": args.changes.read_text() if args.changes else "",
        "textes": {
            "planete": {seed: both[seed][0] for seed in seeds},
            "histoire": {seed: both[seed][1] for seed in seeds},
        },
    }
    template = (HERE / "carte.html").read_text()
    assert "/*PAGE*/null" in template
    (out / "carte.html").write_text(template.replace("/*PAGE*/null", json.dumps(page, ensure_ascii=False)))
    print(out / "carte.html")
    return 0


if __name__ == "__main__":
    sys.exit(main())
