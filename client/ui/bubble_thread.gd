class_name BubbleThread
extends RefCounted
## One side's bubbles, chat-style (ThoughtBubble on the right, SpeechBubble
## on the left): the last KEPT kept in order of arrival, SHOWN of them on
## screen, the newest at the bottom. Hovering the stack, the wheel moves
## the focus back to the oldest and down again: the bubble in focus whole,
## the one or two around it smaller and blurred, so a blurred bubble under
## the focus says the newest is further down. Left alone FOLD_MS (no new
## bubble, no hover), the stack folds into one small bubble, the newest;
## the mouse unfolds it. Left alone HIDE_MS, only a small icon stays in the
## corner; clicked, it reopens the stack on the newest. A new bubble opens
## it too. A thought may be tied to a place (where the body was stopped):
## clicked, even back in the history, it takes the camera there.
## Pure logic on the wall clock: testable headless.

## Bubbles kept: past that the oldest is dropped.
const KEPT := 20
## Bubbles on screen at once.
const SHOWN := 3
## How long the stack stays open with nothing new and no hover.
const FOLD_MS := 8000
## How long until only the icon stays, with nothing new and no hover.
const HIDE_MS := 25000
## The folded bubble's line, at most this many characters.
const FOLDED_CHARS := 32

## Oldest first: {"speaker", "text", "at" (mm), "placed", "told"}.
var _entries: Array[Dictionary] = []
## How many bubbles back from the newest the focus is.
var _scroll := 0
var _hovered := false
## When the stack was last told a bubble or left by the mouse.
var _stirred := -1


## Tells a bubble, `line` by `speaker` (empty for the body's own thought),
## at `now_ms`, tied to the place `at` (mm) when `placed`. The same as the
## newest is not told twice: that one takes the new place and time. A new
## bubble brings the window back to the newest.
func push(speaker: String, line: String, at: Vector2, placed: bool, now_ms: int) -> void:
	_stirred = now_ms
	_scroll = 0
	if not _entries.is_empty():
		var newest: Dictionary = _entries.back()
		if newest["speaker"] == speaker and newest["text"] == line:
			newest["at"] = at
			newest["placed"] = placed
			newest["told"] = now_ms
			return
	_entries.append({"speaker": speaker, "text": line, "at": at, "placed": placed, "told": now_ms})
	if _entries.size() > KEPT:
		_entries.pop_front()


## Forgets every bubble at once (another body is piloted).
func clear() -> void:
	_entries.clear()
	_scroll = 0


## How many bubbles are kept.
func size() -> int:
	return _entries.size()


## The bubbles on screen, oldest first. On the newest, the newest last;
## scrolled back, the focus with the one before it above and the one after
## it below (at the oldest, the two after it).
func shown() -> Array[Dictionary]:
	var start := _start()
	return _entries.slice(start, mini(_entries.size(), start + SHOWN))


## The focus's slot in the window (0: the top); -1 when nothing is kept.
func focus_slot() -> int:
	if _entries.is_empty():
		return -1
	return _entries.size() - 1 - _scroll - _start()


## Where the window starts in the kept bubbles.
func _start() -> int:
	var focus := _entries.size() - 1 - _scroll
	var start := focus - (SHOWN - 1) if _scroll == 0 else focus - 1
	return clampi(start, 0, maxi(0, _entries.size() - SHOWN))


## Moves the focus `steps` bubbles back into the history (negative: toward
## the newest), no further than the oldest nor past the newest.
func scroll(steps: int) -> void:
	_scroll = clampi(_scroll + steps, 0, max_scroll())


## How far back the focus is.
func scrolled() -> int:
	return _scroll


## The farthest back it scrolls: the focus on the oldest.
func max_scroll() -> int:
	return maxi(0, _entries.size() - 1)


## The mouse comes onto the stack (`on`) or leaves it, at `now_ms`. Onto a
## folded stack, it opens on the newest; leaving, the delay starts again.
## Onto the icon it does nothing: the icon is clicked (reopen).
func hover(on: bool, now_ms: int) -> void:
	if on and hidden(now_ms):
		return
	if on and folded(now_ms):
		_scroll = 0
	if _hovered and not on:
		_stirred = now_ms
	_hovered = on


## Whether the stack is folded into one small bubble at `now_ms`.
func folded(now_ms: int) -> bool:
	return _left_alone(now_ms, FOLD_MS) and not _left_alone(now_ms, HIDE_MS)


## Whether only the icon stays at `now_ms`.
func hidden(now_ms: int) -> bool:
	return _left_alone(now_ms, HIDE_MS)


func _left_alone(now_ms: int, delay_ms: int) -> bool:
	return not _entries.is_empty() and not _hovered and now_ms - _stirred >= delay_ms


## The icon is clicked at `now_ms`: the stack opens on the newest.
func reopen(now_ms: int) -> void:
	_stirred = now_ms
	_scroll = 0


## The folded bubble's line: the newest, with its speaker, shortened.
func folded_text() -> String:
	if _entries.is_empty():
		return ""
	var newest: Dictionary = _entries.back()
	var line := str(newest["text"])
	if not str(newest["speaker"]).is_empty():
		line = "%s : %s" % [newest["speaker"], line]
	if line.length() > FOLDED_CHARS:
		line = line.left(FOLDED_CHARS - 1).strip_edges() + "…"
	return line


## The place (mm) of the bubble in `slot` of the window (0: the top), if
## it is tied to one; null otherwise.
func place_at(slot: int) -> Variant:
	var window := shown()
	if slot < 0 or slot >= window.size() or not bool(window[slot]["placed"]):
		return null
	return window[slot]["at"]
