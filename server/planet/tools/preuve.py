#!/usr/bin/env python3
"""The identity proof of the planet's model: one fingerprint of what it makes.

    tools/preuve.py            compare with tools/preuve-reference.txt
    tools/preuve.py --save     write the reference (after a wanted change of the model)
    tools/preuve.py --quick    seeds 14 and 9 only, no 30-seed bilan

It runs the `planet` example on seeds 14 and 9 with every layer, and the
`bilan` on seeds 1-30, with the default settings (or `--settings file.ron`),
and hashes what they write: every layer file, the numbers of each seed, its
journal of events and the bilan's table. Clock times are left out (born_ms, total_ms, the bilan's
seconds). A tooling change must leave every line identical; a change of the
model moves them, and the new reference is saved with it.
"""
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent
SERVER = HERE.parent.parent
REFERENCE = HERE / "preuve-reference.txt"
SEEDS = "14,9"


def cargo(example, args, env=None):
    cage = ["soul-cap"] if shutil.which("soul-cap") else []
    command = cage + ["cargo", "run", "--release", "-q", "-p", "planet", "--example", example, "--"]
    clock = time.time()
    subprocess.run(command + args, cwd=SERVER, env={**os.environ, **(env or {})},
                   check=True, stdout=subprocess.DEVNULL)
    return time.time() - clock


def sha(parts):
    h = hashlib.sha256()
    for part in parts:
        h.update(part)
    return h.hexdigest()[:32]


def main():
    args = sys.argv[1:]
    settings = []
    if "--settings" in args:
        settings = ["--settings", os.path.abspath(args[args.index("--settings") + 1])]
    lines = []
    with tempfile.TemporaryDirectory(prefix="preuve-") as tmp:
        tmp = pathlib.Path(tmp)
        seconds = cargo("planet", settings, {"PLANET_OUT": str(tmp), "PLANET_SEEDS": SEEDS, "PLANET_LAYERS": "1"})
        print(f"planet {SEEDS} with layers: {seconds:.0f} s", file=sys.stderr)
        for seed in SEEDS.split(","):
            journal = tmp / "couches" / f"h{seed}-journal.js"
            files = sorted(p for p in (tmp / "couches").iterdir()
                           if re.match(rf"[a-z]+{seed}-", p.name) and p != journal)
            files += sorted(tmp.glob(f"*-graine-{seed}.png"))
            lines.append(f"layers-{seed} {sha(p.name.encode() + p.read_bytes() for p in files)} {len(files)} files")
            numbers = json.loads((tmp / f"graine-{seed}.json").read_text())
            for clock in ("born_ms", "total_ms"):
                numbers.pop(clock)
            lines.append(f"numbers-{seed} {sha([json.dumps(numbers, sort_keys=True).encode()])}")
            lines.append(f"journal-{seed} {sha([journal.read_bytes()])}")
        known = {p.name for p in (tmp / "couches").iterdir()}
        # (The maps of a seed are counted with its layers but lie beside
        # them; a seed with no site for our continent writes one map,
        # not three. The two journals are layer files left out above.)
        maps = len(list(tmp.glob("*-graine-*.png")))
        counted = sum(int(line.split()[2]) for line in lines if line.startswith("layers")) - maps + 2
        assert counted == len(known), f"{len(known) - counted} layer files belong to no seed"
        if "--quick" not in args:
            table = tmp / "bilan.json"
            seconds = cargo("bilan", settings + ["--seeds", "1-30", "--version", "proof", "--out", str(table)])
            print(f"bilan 1-30: {seconds:.0f} s", file=sys.stderr)
            bilan = json.loads(table.read_text())
            bilan.pop("8 seconds per planet")
            lines.append(f"bilan-30 {sha([json.dumps(bilan, sort_keys=True).encode()])}")
    text = "\n".join(lines) + "\n"
    print(text, end="")
    if "--save" in args:
        assert "--quick" not in args, "the reference holds the bilan too"
        old = REFERENCE.read_text().splitlines() if REFERENCE.exists() else []
        for line in lines:
            if line not in old:
                print(f"changed or new: {line}", file=sys.stderr)
        REFERENCE.write_text(text)
        print(f"reference saved: {REFERENCE}", file=sys.stderr)
        return 0
    reference = REFERENCE.read_text().splitlines()
    wrong = [line for line in lines if line not in reference]
    print("IDENTICAL to the reference" if not wrong else "DIFFERENT from the reference:\n  " + "\n  ".join(wrong))
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
