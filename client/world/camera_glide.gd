class_name CameraGlide
extends RefCounted
## The camera's short glide to a place (the recentre button, a thought
## clicked): from where it was to where it goes, eased out over SECONDS.
## The place may move meanwhile (the body walking): it is given each step.

const SECONDS := 0.3

var active := false
var _from := Vector2.ZERO
var _t := 0.0


## Starts a glide from `from`.
func start(from: Vector2) -> void:
	_from = from
	_t = 0.0
	active = true


## Stops it where it is (the player drags the map).
func stop() -> void:
	active = false


## Where the camera is `delta` seconds on, gliding toward `to`; the glide
## ends there.
func step(delta: float, to: Vector2) -> Vector2:
	_t = minf(_t + delta / SECONDS, 1.0)
	if _t >= 1.0:
		active = false
	return _from.lerp(to, ease_out(_t))


## Quick at first, gentle at the end.
static func ease_out(t: float) -> float:
	return 1.0 - pow(1.0 - t, 3.0)
