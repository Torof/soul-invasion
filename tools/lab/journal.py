#!/usr/bin/env python3
"""Reads a run's journal (`vivarium --journal FILE`).

    journal.py FILE person N [--from DAY] [--to DAY] [--all]
        the story of one person, a line per thing that happened to it
    journal.py FILE trials
        every person that put food in a hollow, and how its trial ended

The story leaves out the news of every evening (told, heard, company)
unless --all is given. Kept small on purpose: one reader, improved as
questions come, instead of a new script per question.
"""
import json
import re
import sys
from collections import Counter, defaultdict

QUIET = ("Told", "Heard", "Company", "SawHearth", "Dispelled", "Learning", "Habit")
BODY = re.compile(r"BodyId\((\d+)\)")


def lines(path, wanted=None):
    """The journal's lines, parsed; `wanted` filters on raw text first."""
    with open(path, encoding="utf-8") as file:
        for raw in file:
            if wanted and not any(w in raw for w in wanted):
                continue
            line = json.loads(raw)
            if line["k"] != "end":
                yield line


def short(line):
    """One line of the journal as a short text."""
    what = line["w"]
    if line["k"] == "news":
        return what
    if line["k"] == "carry":
        things = ", ".join(m for _, m in what["things"]) or "nothing"
        return (
            f"  carries {what['food_g']} g of food, {what['stowed_g']} g of it in a "
            f"hollow, arms hold {what['arms_g']} g; on it: {things}"
        )
    kind = what.pop("type")
    what.pop("body", None)
    return kind + " " + " ".join(f"{k}={v}" for k, v in what.items() if v is not None)


def person(path, who, first, last, everything):
    """The story of `who` between two days."""
    for line in lines(path, (f'"b":{who},',)):
        if line["b"] != who or not first <= line["d"] <= last:
            continue
        text = short(line)
        if not everything and line["k"] == "news" and text.startswith(QUIET):
            continue
        if not everything and "step: Lived" in text:
            continue
        print(f"d{line['d']:>3} t{line['t']:>7}  {text}")


def trials(path):
    """Who put food in a hollow, and what came of its trial."""
    wanted = ("put_in", '"carry"', "Tried", "body_died", "step: Began")
    # Food is what a gathering gives; a stone put in a hollow is no answer.
    food = {l["w"]["gives"] for l in lines(path, ("work_stopped",)) if l["k"] == "event"}
    story = defaultdict(list)
    others = Counter()
    for line in lines(path, wanted):
        what, body = line["w"], line["b"]
        if line["k"] == "carry":
            story[body].append((line["t"], line["d"], "stop", what))
        elif line["k"] == "event":
            if what["type"] == "body_died":
                story[body].append((line["t"], line["d"], "died", None))
            elif what.get("gesture") == "put_in" and what.get("gave") is not None:
                if what["on"] in food:
                    story[body].append((line["t"], line["d"], "put", what["on"]))
                else:
                    others[what["on"]] += 1
        elif what.startswith("Tried") and "way: Hands" in what:
            story[body].append((line["t"], line["d"], "kept" if "kept: true" in what else "not", None))
        elif "step: Began" in what:
            story[body].append((line["t"], line["d"], "began", None))
    ends, why = Counter(), Counter()
    for body, steps in sorted(story.items()):
        puts = [s for s in steps if s[2] == "put"]
        if not puts:
            continue
        start = puts[0][0]
        end = next((s for s in steps if s[0] >= start and s[2] in ("kept", "not", "died")), None)
        name = end[2] if end else "run ended"
        ends[name] += 1
        stops = [s[3] for s in steps if s[2] == "stop" and s[0] > start and (not end or s[0] <= end[0])]
        full = [s for s in stops if s["food_g"] >= s["arms_g"]]
        with_hollow = [s for s in full if len(s["things"]) > 0]
        stowed = [s for s in full if s["stowed_g"] > 0]
        beyond = [s for s in full if s["food_g"] > s["arms_g"]]
        if name == "not":
            if not full:
                why["no full stop before the verdict"] += 1
            elif not with_hollow:
                why["full stops, nothing on it but food"] += 1
            elif not stowed:
                why["full stops with a thing on it, nothing put in"] += 1
            elif not beyond:
                why["food in a hollow, never beyond the arms"] += 1
            else:
                why["beyond the arms, yet not kept"] += 1
        print(
            f"person {body:>3}: put-in day {puts[0][1]:>3}, {name:<9} "
            f"day {end[1] if end else '-':>3} | full stops {len(full)}, with a thing "
            f"{len(with_hollow)}, food in a hollow {len(stowed)}, beyond the arms {len(beyond)}"
        )
    print("\nput in a hollow, not food:", dict(others))
    print("ends:", dict(ends))
    print("not kept, why:", dict(why))


def main(argv):
    if len(argv) >= 4 and argv[2] == "person":
        opts = argv[4:]
        first = int(opts[opts.index("--from") + 1]) if "--from" in opts else 0
        last = int(opts[opts.index("--to") + 1]) if "--to" in opts else 10**9
        person(argv[1], int(argv[3]), first, last, "--all" in opts)
    elif len(argv) == 3 and argv[2] == "trials":
        trials(argv[1])
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
