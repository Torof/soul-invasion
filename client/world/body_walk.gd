class_name BodyWalk
extends RefCounted
## A body as the client walks it between the server's messages (D32): where
## it is shown, and the rest of its walk by time. The server tells a
## walking body as it sets off and again each time it walks into another
## chunk (protocol v19, A6), never at each tile, with the tick it reaches
## each waypoint (`arrivals`), the route holding the points where its pace
## changes with the ground. In between, the client moves it by time
## between them, so it is where the world has it, in a forest as on grass,
## and each new message agrees with what is shown.
##
## A walk: {"at": Vector2 (mm, as shown), "from": Vector2, "from_tick":
## float, "route": Array of Vector2, "arrivals": Array of int, "speed": mm
## per second, "offset": Vector2, "offset_tick": float, "offset_ticks":
## float}. "from", "from_tick", "route" and "arrivals" are the walk as the
## world has it; "offset" is how far from it the body was shown when told,
## a gap closed evenly from "offset_tick" over "offset_ticks" (the client's
## clock a little off the world's, a new route). With no arrivals (a
## message without them) the body walks at the nominal speed, as before
## v19.

## How far, in millimetres, the place a walk puts a body may lie from where
## it is shown and the body still be walked on from where it is shown, to
## the next waypoint on time: a safety net, never a jump under it. Farther
## (a new order, a refusal, a clock far off), it stands where it is told.
const CATCH_UP_MM := 3000.0
## How fast a body shown off its walk closes the gap, as a share of its
## pace: it never dashes, never stands still, never walks back against its
## route (a dash back to the new route's first waypoint, on time, was the
## jump seen on changing direction, 12 November 2026).
const CLOSING_SHARE := 0.5


## A walk told at `at` at `tick`, reaching `route` at `arrivals`, as the
## body is shown at `now` given how it was shown (`shown`, empty when seen
## for the first time): from where it is shown when that is within
## CATCH_UP_MM of where the walk has it now, closing the gap as it walks
## on, so it never jumps; else from where the walk has it.
static func told(shown: Dictionary, at: Vector2, route: Array, arrivals: Array, tick: float,
		speed_mm_per_s: float, now: float) -> Dictionary:
	var walk := {"at": at, "from": at, "from_tick": tick, "route": route.duplicate(),
		"arrivals": arrivals.duplicate() if arrivals.size() == route.size() else [],
		"speed": speed_mm_per_s, "offset": Vector2.ZERO, "offset_tick": now, "offset_ticks": 0.0}
	var walking := advance(walk, 0.0, now)
	if not walking or shown.is_empty():
		return walk
	var gap: Vector2 = shown["at"] - walk["at"]
	if gap.length() > CATCH_UP_MM:
		return walk
	walk["at"] = shown["at"]
	if (walk["arrivals"] as Array).is_empty():
		walk["from"] = shown["at"]
		return walk
	# Where the world has it, the gap closed at a share of its pace and
	# before the walk ends.
	var arrivals_left: Array = walk["arrivals"]
	var route_left: Array = walk["route"]
	var span := float(arrivals_left[0]) - float(walk["from_tick"])
	var pace := (walk["from"] as Vector2).distance_to(route_left[0]) / span if span > 0.0 else 0.0
	if pace <= 0.0:
		pace = speed_mm_per_s / 10.0
	walk["offset"] = gap
	walk["offset_ticks"] = minf(gap.length() / (CLOSING_SHARE * pace), float(arrivals_left[-1]) - now)
	return walk


## A body standing at `at`.
static func standing(at: Vector2) -> Dictionary:
	return {"at": at, "from": at, "from_tick": 0.0, "route": [], "arrivals": [], "speed": 0.0}


## Moves a walk on: to where it has the body at tick `now` (by time), or,
## without arrivals, `delta` seconds further at its speed. Changed in place;
## true while it walks.
static func advance(walk: Dictionary, delta: float, now: float) -> bool:
	var route: Array = walk["route"]
	if route.is_empty():
		return false
	var arrivals: Array = walk["arrivals"]
	if arrivals.is_empty():
		return _advance_at_speed(walk, delta)
	# The waypoints passed by now become where it walks from.
	while not route.is_empty() and now >= float(arrivals[0]):
		walk["from"] = route.pop_front()
		walk["from_tick"] = float(arrivals.pop_front())
	if route.is_empty():
		walk["at"] = walk["from"]
		return false
	var from: Vector2 = walk["from"]
	var start: float = walk["from_tick"]
	var span := float(arrivals[0]) - start
	var f := clampf((now - start) / span, 0.0, 1.0) if span > 0.0 else 1.0
	walk["at"] = from.lerp(route[0], f) + _gap_left(walk, now)
	return true


## What is left at `now` of the gap between where the body was shown and
## where its walk has it, when told.
static func _gap_left(walk: Dictionary, now: float) -> Vector2:
	var ticks: float = walk.get("offset_ticks", 0.0)
	if ticks <= 0.0:
		return Vector2.ZERO
	var left := 1.0 - (now - float(walk["offset_tick"])) / ticks
	return (walk["offset"] as Vector2) * clampf(left, 0.0, 1.0)


static func _advance_at_speed(walk: Dictionary, delta: float) -> bool:
	var route: Array = walk["route"]
	var budget: float = walk["speed"] * delta
	var at: Vector2 = walk["at"]
	while budget > 0.0 and not route.is_empty():
		var next: Vector2 = route[0]
		var distance := at.distance_to(next)
		if distance <= budget:
			at = next
			route.pop_front()
			budget -= distance
		else:
			at = at.move_toward(next, budget)
			budget = 0.0
	walk["at"] = at
	return true
