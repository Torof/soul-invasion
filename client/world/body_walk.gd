class_name BodyWalk
extends RefCounted
## A body as the client walks it between the server's messages (D32): where
## it is shown, the route ahead and its speed. The server tells a walking
## body again each time it walks into another chunk (protocol v19, A6),
## never at each tile; in between the client walks it along its route at
## the nominal speed.

## How far, in millimetres, the place told may lie from where the body is
## shown and the body still walk on from where it is shown: a walk told
## again as it walks into another chunk never makes it jump back or ahead
## by less than this. Farther (it was slowed by the ground, or seen first),
## it stands where the server says.
const CATCH_UP_MM := 3000.0


## A body walked on `delta` seconds along its route (`entry`: {"at":
## Vector2 in mm, "route": Array of Vector2, "speed": mm per second}),
## changed in place. True if it moved.
static func advance(entry: Dictionary, delta: float) -> bool:
	var route: Array = entry["route"]
	if route.is_empty():
		return false
	var budget: float = entry["speed"] * delta
	var at: Vector2 = entry["at"]
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
	entry["at"] = at
	return true


## Where a body told at `at` along `route` is walked from, given how it
## is shown (`shown`, empty when seen for the first time): from where it
## is shown when that is near and it walks to the same end, so the walk
## goes on without a jump, the waypoints it already passed left out; else
## from `at`.
static func told(shown: Dictionary, at: Vector2, route: Array, speed_mm_per_s: float) -> Dictionary:
	var walking: Array = shown.get("route", [])
	var near := not shown.is_empty() and (shown["at"] as Vector2).distance_to(at) <= CATCH_UP_MM
	if near and not walking.is_empty() and not route.is_empty() and walking.back() == route.back():
		var keep := mini(walking.size(), route.size())
		return {"at": shown["at"], "route": route.slice(route.size() - keep), "speed": speed_mm_per_s}
	return {"at": at, "route": route.duplicate(), "speed": speed_mm_per_s}
