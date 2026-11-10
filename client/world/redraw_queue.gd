class_name RedrawQueue
extends RefCounted
## The chunks whose borders must be drawn again because a neighbour came
## or went (WorldView): each held once however many neighbours asked, and
## handed out a few a frame within a time budget, so a teleport's 361
## chunks, each asking its eight neighbours, never redraw all at once nor
## queue one chunk twice. Nothing a chunk does when it redraws puts a
## chunk back in here: the queue only ever drains.

## Microseconds of redraws a frame, at least one chunk however slow.
const BUDGET_USEC := 2000.0

## chunk -> true, in the order they were asked.
var _queued: Dictionary = {}


## Asks for `chunk` to be redrawn; already queued, nothing changes.
func add(chunk: Object) -> void:
	_queued[chunk] = true


## A chunk gone (hidden, freed) is forgotten.
func drop(chunk: Object) -> void:
	_queued.erase(chunk)


func size() -> int:
	return _queued.size()


## The chunks to redraw this frame, oldest asked first: as many as fit in
## `budget_usec` when each costs `cost_usec`, at least one. Freed chunks
## are skipped and forgotten.
func take(budget_usec := BUDGET_USEC, cost_usec := 1000.0) -> Array:
	var out: Array = []
	var spent := 0.0
	for chunk: Variant in _queued.keys():
		if not out.is_empty() and spent + cost_usec > budget_usec:
			break
		_queued.erase(chunk)
		if not is_instance_valid(chunk):
			continue
		out.append(chunk)
		spent += cost_usec
	return out
