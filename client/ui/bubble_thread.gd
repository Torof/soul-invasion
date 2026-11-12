class_name BubbleThread
extends RefCounted
## One side's bubbles, chat-style (ThoughtBubble on the right, SpeechBubble
## on the left): the last KEPT kept in order of arrival, SHOWN of them on
## screen, the newest at the bottom. Hovering the stack, the wheel scrolls
## back to the oldest and down again. Left alone FOLD_MS (no new bubble,
## no hover), the stack folds into one small bubble; a new bubble or the
## mouse unfolds it. A thought may be tied to a place (where the body was
## stopped): clicked, even back in the history, it takes the camera there.
## Pure logic on the wall clock: testable headless.

## Bubbles kept: past that the oldest is dropped.
const KEPT := 20
## Bubbles on screen at once.
const SHOWN := 3
## How long the stack stays open with nothing new and no hover.
const FOLD_MS := 9000
## The folded bubble's line, at most this many characters.
const FOLDED_CHARS := 32

## Oldest first: {"speaker", "text", "at" (mm), "placed", "told"}.
var _entries: Array[Dictionary] = []
## How many bubbles back from the newest the window is scrolled.
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


## The bubbles on screen, oldest first: the newest last, unless scrolled.
func shown() -> Array[Dictionary]:
	var end := _entries.size() - _scroll
	return _entries.slice(maxi(0, end - SHOWN), end)


## Scrolls `steps` bubbles back into the history (negative: toward the
## newest), no further than the oldest at the top of the window, nor past
## the newest at its bottom.
func scroll(steps: int) -> void:
	_scroll = clampi(_scroll + steps, 0, max_scroll())


## How far back the window is scrolled.
func scrolled() -> int:
	return _scroll


## The farthest back it scrolls: the oldest at the top of the window.
func max_scroll() -> int:
	return maxi(0, _entries.size() - SHOWN)


## The mouse comes onto the stack (`on`) or leaves it, at `now_ms`. Onto a
## folded stack, it opens on the newest; leaving, the delay starts again.
func hover(on: bool, now_ms: int) -> void:
	if on and folded(now_ms):
		_scroll = 0
	if _hovered and not on:
		_stirred = now_ms
	_hovered = on


## Whether the stack is folded into one small bubble at `now_ms`.
func folded(now_ms: int) -> bool:
	return not _entries.is_empty() and not _hovered and now_ms - _stirred >= FOLD_MS


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
