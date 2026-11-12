class_name ThoughtQueue
extends RefCounted
## The piloted body's thoughts, one at a time (ThoughtBubble draws them).
## A thought stays STAY_MS then fades over FADE_MS; one told while
## another shows waits until that one has shown MIN_MS, then takes its
## place. A thought may be tied to a place (where the body was stopped,
## later an animal seen): a click on it takes the camera there. Pure
## logic on the wall clock: testable headless.

## The least a thought shows when another waits behind it.
const MIN_MS := 2000
## How long a thought shows whole when nothing waits.
const STAY_MS := 5000
## Then how long it fades.
const FADE_MS := 600
## Thoughts kept waiting at most: past that the oldest waiting is dropped.
const MAX_WAITING := 4

## The thought shown: {"text", "at" (mm), "placed", "told", "since"};
## empty when none.
var _current: Dictionary = {}
var _waiting: Array[Dictionary] = []


## Tells a thought, `line`, at `now_ms`, tied to the place `at` (mm) when `placed`.
## The same line as the last one told is not told twice: that one takes
## the new place (and shows anew if it is the one shown).
func push(line: String, at: Vector2, placed: bool, now_ms: int) -> void:
	_settle(now_ms)
	var thought := {"text": line, "at": at, "placed": placed, "told": now_ms, "since": now_ms}
	if _current.is_empty():
		_current = thought
		return
	var last: Dictionary = _waiting.back() if not _waiting.is_empty() else _current
	if last["text"] == line:
		last["at"] = at
		last["placed"] = placed
		if _waiting.is_empty():
			last["since"] = now_ms
		return
	_waiting.append(thought)
	if _waiting.size() > MAX_WAITING:
		_waiting.pop_front()


## Forgets every thought at once (another body is piloted).
func clear() -> void:
	_current = {}
	_waiting.clear()


## The line shown at `now_ms`; empty when none.
func text(now_ms: int) -> String:
	_settle(now_ms)
	return str(_current.get("text", ""))


## Its opacity at `now_ms`: whole, then fading to nothing.
func alpha(now_ms: int) -> float:
	_settle(now_ms)
	if _current.is_empty():
		return 0.0
	var age: int = now_ms - int(_current["since"])
	return clampf(1.0 - float(age - STAY_MS) / FADE_MS, 0.0, 1.0)


## Whether the thought shown at `now_ms` is tied to a place.
func clickable(now_ms: int) -> bool:
	_settle(now_ms)
	return bool(_current.get("placed", false))


## The place (mm) of the thought shown at `now_ms`.
func place(now_ms: int) -> Vector2:
	_settle(now_ms)
	return _current.get("at", Vector2.ZERO)


## Moves on to whatever shows at `now_ms`: the next waiting once the one
## shown has shown its least (from then, so the result does not depend
## on how often this is asked), nothing once it has faded.
func _settle(now_ms: int) -> void:
	while not _current.is_empty():
		var since := int(_current["since"])
		if not _waiting.is_empty():
			var start := maxi(since + MIN_MS, int(_waiting[0]["told"]))
			if now_ms < start:
				return
			_current = _waiting.pop_front()
			_current["since"] = start
			continue
		if now_ms >= since + STAY_MS + FADE_MS:
			_current = {}
		return
