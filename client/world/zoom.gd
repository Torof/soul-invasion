class_name Zoom
extends RefCounted
## The camera's zoom levels, apart from the world view so tests can use
## them without the network.

const MIN := 0.4
## The closest zoom (the lead's call, 10 November 2026: 3 and 4 added
## nothing; at 2.5 a body and its surroundings are still in view). Every
## image is drawn to be sharp exactly there (D39).
const MAX := 2.5
const DEFAULT := 1.5
## The levels the wheel steps through: round values.
const STEPS: Array[float] = [0.4, 0.5, 0.6, 0.8, 1.0, 1.25, 1.5, 2.0, 2.5]


## `zoom` kept within MIN and MAX.
static func clamped(zoom: float) -> float:
	return clampf(zoom, MIN, MAX)


## The next level up (`direction` 1) or down (-1) from `now`.
static func next(now: float, direction: int) -> float:
	if direction > 0:
		for level in STEPS:
			if level > now + 0.001:
				return level
	else:
		for i in range(STEPS.size() - 1, -1, -1):
			if STEPS[i] < now - 0.001:
				return STEPS[i]
	return clamped(now)
