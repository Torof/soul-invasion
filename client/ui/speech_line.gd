class_name SpeechLine
extends RefCounted
## What someone says, shown in the speech bubble (SpeechBubble): the
## speaker's name and the line, for SHOW_MS (the last FADE_MS fading) or
## until the next line, which takes its place at once. Pure logic on the
## wall clock: testable headless.

const SHOW_MS := 6000
const FADE_MS := 500

var speaker := ""
var text := ""
## When it was said (Time.get_ticks_msec()); negative: nothing said.
var _since := -1


## `speaker` says `line` at `now_ms`.
func say(who: String, line: String, now_ms: int) -> void:
	speaker = who
	text = line
	_since = now_ms


## Whether a line shows at `now_ms`.
func shown(now_ms: int) -> bool:
	return _since >= 0 and now_ms - _since < SHOW_MS


## Its opacity at `now_ms`.
func alpha(now_ms: int) -> float:
	if not shown(now_ms):
		return 0.0
	return clampf(float(SHOW_MS - (now_ms - _since)) / FADE_MS, 0.0, 1.0)
