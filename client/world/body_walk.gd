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
## per second}. With no arrivals (a message without them) the body walks at
## the nominal speed, as before v19.

## How far, in millimetres, the place a walk puts a body may lie from where
## it is shown and the body still be walked on from where it is shown, to
## the next waypoint on time: a safety net, never a jump under it. Farther
## (a new order, a refusal, a clock far off), it stands where it is told.
const CATCH_UP_MM := 3000.0


## A walk told at `at` at `tick`, reaching `route` at `arrivals`, as the
## body is shown at `now` given how it was shown (`shown`, empty when seen
## for the first time): from where it is shown when that is within
## CATCH_UP_MM of where the walk has it now, so it never jumps; else from
## where the walk has it.
static func told(shown: Dictionary, at: Vector2, route: Array, arrivals: Array, tick: float,
		speed_mm_per_s: float, now: float) -> Dictionary:
	var walk := {"at": at, "from": at, "from_tick": tick, "route": route.duplicate(),
		"arrivals": arrivals.duplicate() if arrivals.size() == route.size() else [],
		"speed": speed_mm_per_s}
	var walking := advance(walk, 0.0, now)
	if walking and not shown.is_empty() and (shown["at"] as Vector2).distance_to(walk["at"]) <= CATCH_UP_MM:
		walk["at"] = shown["at"]
		walk["from"] = shown["at"]
		walk["from_tick"] = maxf(now, walk["from_tick"])
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
	walk["at"] = from.lerp(route[0], f)
	return true


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
