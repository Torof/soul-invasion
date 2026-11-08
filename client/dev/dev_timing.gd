class_name DevTiming
extends RefCounted
## Development aid (the F3 counters, D39): where a frame's script time
## goes, on the machine that plays. Parts of the client add the time they
## took and how many times they ran; the counters read and reset the
## totals. Costs two clock reads a part when on, nothing when off.

## Whether the totals are kept (the counters are shown).
static var enabled := false
static var _usec: Dictionary = {}
static var _count: Dictionary = {}


## A clock reading to pass to `add` later, or 0 when off.
static func start() -> int:
	return Time.get_ticks_usec() if enabled else 0


## Adds the time since `started` (from `start`) to `part`, once.
static func add(part: String, started: int) -> void:
	if not enabled or started == 0:
		return
	_usec[part] = int(_usec.get(part, 0)) + Time.get_ticks_usec() - started
	_count[part] = int(_count.get(part, 0)) + 1


## Counts one more of `part`, with no time.
static func tally(part: String) -> void:
	if enabled:
		_count[part] = int(_count.get(part, 0)) + 1


## The totals since the last call, {part: [milliseconds, times]}, and
## starts again from zero.
static func take() -> Dictionary:
	var out := {}
	for part: String in _count:
		out[part] = [float(_usec.get(part, 0)) / 1000.0, int(_count[part])]
	_usec.clear()
	_count.clear()
	return out
