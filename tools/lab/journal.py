#!/usr/bin/env python3
"""Reads a run's journal (`vivarium --journal FILE`).

    journal.py FILE person N [--from DAY] [--to DAY] [--all]
        the story of one person, a line per thing that happened to it
    journal.py FILE trials
        every person that put food in a hollow, and how its trial ended
    journal.py FILE gestures [--top N]
        the gestures of the hands by kind: in a trial of the hands or not,
        with something coming of them, leading to food put in a hollow or
        to nothing; the deeds seen and kept, the bark lifted, the trees
        felled; the people who worked most; what the kept trials did
        afterwards

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
    wanted = ("put_in", '"carry"', "Tried", "body_died", "step: Began", "step: Abandoned")
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
        elif "kind: 0, step: Abandoned" in what:
            story[body].append((line["t"], line["d"], "given up", None))
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
        # A trial heard at the fire is judged by the days, not by its
        # problem: what served stays known whatever that judge says.
        own = end is not None and any(s[2] == "given up" and s[0] == end[0] for s in steps)
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
            elif own:
                why["beyond the arms, yet given up by its problem"] += 1
            else:
                why["beyond the arms, judged by the days (the deed stays known)"] += 1
        print(
            f"person {body:>3}: put-in day {puts[0][1]:>3}, {name:<9} "
            f"day {end[1] if end else '-':>3} | full stops {len(full)}, with a thing "
            f"{len(with_hollow)}, food in a hollow {len(stowed)}, beyond the arms {len(beyond)}"
        )
    print("\nput in a hollow, not food:", dict(others))
    print("ends:", dict(ends))
    print("not kept, why:", dict(why))


def gestures(path, top):
    """The work of the hands: what belonged to a trial, what led to food."""
    wanted = (
        '"gestured"', "tree_fell", "bark_lifted", "Trying {", "way: Hands", "Untried", '"carry"',
        "step: Began", "work_stopped", "DeedSeen", "DeedKept",
    )
    lifted, seen, kept_own, kept_left = [0], Counter(), Counter(), Counter()
    spans = defaultdict(list)  # body -> [start, end, from a problem, kept, on a tale]
    done, days, felled, short_cut = [], set(), Counter(), [0]
    food, stops = set(), defaultdict(list)
    for line in lines(path, wanted):
        what, body, tick = line["w"], line["b"], line["t"]
        if line["k"] == "carry":
            stops[body].append((tick, what))
        elif line["k"] == "event":
            days.add((body, line["d"]))
            if what["type"] == "gestured":
                done.append((tick, line["d"], body, what))
            elif what["type"] == "tree_fell":
                felled[body] += 1
            elif what["type"] == "bark_lifted":
                lifted[0] += 1
            elif what["type"] == "work_stopped":
                food.add(what["gives"])
        elif what.startswith("DeedSeen"):
            seen[body] += 1
        elif what.startswith("DeedKept"):
            (kept_own if "own: true" in what else kept_left)[body] += 1
        elif what.startswith("Trying"):
            # One trial at a time: another begun cuts short the one under
            # way (a move followed), unjudged.
            if spans[body] and spans[body][-1][1] is None:
                spans[body][-1][1] = tick
                short_cut[0] += 1
            if "way: Hands" in what:
                spans[body].append([tick, None, False, False, "heard: true" in what])
        elif "step: Began" in what and spans[body] and spans[body][-1][0] == tick:
            spans[body][-1][2] = True
        elif what.startswith(("Tried", "Untried")) and spans[body] and spans[body][-1][1] is None:
            if what.startswith("Untried") or "way: Hands" in what:
                spans[body][-1][1] = tick
                spans[body][-1][3] = "kept: true" in what

    def trial_of(body, tick):
        for span in spans[body]:
            if span[0] <= tick <= (span[1] if span[1] is not None else 10**12):
                return span
        return None

    # What led to food in a hollow, read backwards: a hollow food was put
    # in, the thing folded into it, the tool that cut it, the blow that
    # gave the tool.
    useful, led = set(), set()
    for i in range(len(done) - 1, -1, -1):
        what = done[i][3]
        if what["gave"] is None:
            continue
        if what["gesture"] == "put_in" and what["on"] in food:
            useful.add(what["into"])
            led.add(i)
        elif what["gave"] in useful:
            led.add(i)
            useful.add(what["item"])
            if what["into"] is not None:
                useful.add(what["into"])
    used = set()  # the things a later gesture was done on, with or in
    reused = set()
    for i in range(len(done) - 1, -1, -1):
        what = done[i][3]
        if what["gave"] is not None and what["gave"] in used and what["gesture"] not in ("put_in", "take_out"):
            reused.add(i)
        used.add(what["item"])
        if what["into"] is not None:
            used.add(what["into"])
    rows = defaultdict(Counter)
    by_trial = defaultdict(Counter)
    people = defaultdict(Counter)
    per_day = Counter()
    cut_in = defaultdict(set)  # body -> its trials in which it cut
    for i, (tick, day, body, what) in enumerate(done):
        name = what["gesture"]
        if name == "cut":
            name += " " + (what.get("part") or "trunk")
        if name == "put_in":
            name += " food" if what["on"] in food else " other"
        span = trial_of(body, tick)
        row = rows[name]
        row["done"] += 1
        row["gave"] += what["gave"] is not None
        row["in a trial"] += span is not None
        row["trial of a problem"] += span is not None and span[2]
        row["led to food"] += i in led
        row["in a trial, led"] += span is not None and i in led
        row["product used"] += i in reused
        if span is not None:
            kind = ("problem" if span[2] else "no problem") + (", on a tale" if span[4] else ", drawn")
            by_trial[kind]["gestures"] += 1
            by_trial[kind]["led to food"] += i in led
            by_trial[kind]["trunk cuts"] += name == "cut trunk"
        mine = people[body]
        mine["gestures"] += 1
        mine["in a trial"] += span is not None
        mine["led to food"] += i in led
        mine[what["gesture"]] += 1
        if what["gesture"] == "cut":
            per_day[(body, day)] += 1
            if span is not None:
                cut_in[body].add(span[0])
    heads = ("done", "gave", "in a trial", "trial of a problem", "led to food", "in a trial, led", "product used")
    print("gesture".ljust(14) + "".join(h.rjust(19) for h in heads))
    total = Counter()
    for name, row in sorted(rows.items()):
        total.update(row)
        print(name.ljust(14) + "".join(str(row[h]).rjust(19) for h in heads))
    print("all".ljust(14) + "".join(str(total[h]).rjust(19) for h in heads))
    inside, outside = total["in a trial"], total["done"] - total["in a trial"]
    led_in, led_out = total["in a trial, led"], total["led to food"] - total["in a trial, led"]
    share = lambda part, whole: f"{100 * part // whole} %" if whole else "-"
    print(
        f"\nin a trial of the hands: {inside} gestures, {led_in} led to food ({share(led_in, inside)}); "
        f"outside: {outside} gestures, {led_out} led to food ({share(led_out, outside)})"
    )
    print(
        f"deeds seen give a thing in a trial (each once a trial): {sum(seen.values())} by {len(seen)} people; "
        f"deeds kept: {sum(kept_own.values()) + sum(kept_left.values())} "
        f"(of a kept trial {sum(kept_own.values())} by {len(kept_own)} people, "
        f"left by a trial not kept {sum(kept_left.values())} by {len(kept_left)} people)"
    )
    print(f"bark pieces lifted: {lifted[0]}; trees felled: {sum(felled.values())} by {len(felled)} people")
    print(f"\npeople with a gesture: {len(people)}; person-journées seen in the events: {len(days)}")
    trials = [s for body in spans for s in spans[body]]
    lengths = sorted((s[1] - s[0]) for s in trials if s[1] is not None)
    print(
        f"trials of the hands: {len(trials)}, of a problem {sum(s[2] for s in trials)}, "
        f"kept {sum(s[3] for s in trials)}, cut short by another trial {short_cut[0]}, "
        f"under way at the end {sum(s[1] is None for s in trials)}; "
        f"length in ticks median {lengths[len(lengths) // 2] if lengths else 0}, "
        f"longest {lengths[-1] if lengths else 0}"
    )
    for kind in sorted(by_trial):
        n = sum(1 for t in trials if ("problem" if t[2] else "no problem") + (", on a tale" if t[4] else ", drawn") == kind)
        kept = sum(1 for t in trials if t[3] and ("problem" if t[2] else "no problem") + (", on a tale" if t[4] else ", drawn") == kind)
        print(f"  trials {kind}: {n} begun, {kept} kept, " + ", ".join(f"{k} {v}" for k, v in sorted(by_trial[kind].items())))
    beyond = sum(max(0, n - 4) for n in per_day.values())
    print(
        f"cuts beyond four in a person's journée (a trial draws four gestures a journée at most; "
        f"the rest is a deed done again): {beyond} of {sum(per_day.values())}; "
        f"people who cut in three trials or more: {sum(len(t) >= 3 for t in cut_in.values())}"
    )
    print(f"\nthe {top} who worked most (trees felled; most cuts in one journée):")
    for body, mine in sorted(people.items(), key=lambda p: -p[1]["gestures"])[:top]:
        most = max((n for (b, _), n in per_day.items() if b == body), default=0)
        print(
            f"person {body:>3}: {mine['gestures']:>5} gestures, {mine['in a trial']:>5} in a trial, "
            f"{mine['led to food']:>4} led to food | cut {mine['cut']:>4} strike {mine['strike']:>4} "
            f"put_in {mine['put_in']:>4} fold {mine['fold']:>3} | felled {felled[body]:>3}, "
            f"most cuts a journée {most}, trials {len(spans[body])}, with cuts {len(cut_in[body])}"
        )
    print("\nafter a kept trial of the hands: the full stops, and those with food in a hollow")
    for body in sorted(spans):
        kept = [s[1] for s in spans[body] if s[3]]
        if not kept:
            continue
        after = [w for t, w in stops[body] if t > kept[0] and w["food_g"] >= w["arms_g"]]
        stowed = sum(w["stowed_g"] > 0 for w in after)
        puts = sum(1 for t, _, b, w in done if b == body and t > kept[0] and w["gesture"] == "put_in" and w["on"] in food and w["gave"] is not None)
        again = sum(1 for s in spans[body] if s[0] > kept[0])
        print(f"person {body:>3}: kept at tick {kept[0]:>8}, full stops after {len(after):>4}, with food in a hollow {stowed:>4}, food put-ins {puts:>4}, trials begun after {again}")


def main(argv):
    if len(argv) >= 4 and argv[2] == "person":
        opts = argv[4:]
        first = int(opts[opts.index("--from") + 1]) if "--from" in opts else 0
        last = int(opts[opts.index("--to") + 1]) if "--to" in opts else 10**9
        person(argv[1], int(argv[3]), first, last, "--all" in opts)
    elif len(argv) == 3 and argv[2] == "trials":
        trials(argv[1])
    elif len(argv) >= 3 and argv[2] == "gestures":
        opts = argv[3:]
        gestures(argv[1], int(opts[opts.index("--top") + 1]) if "--top" in opts else 12)
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
