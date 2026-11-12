class_name BlockedNotice
extends RefCounted
## The "blocked" notice (D32, protocol v20): the piloted body stopped for
## good short of where it was sent, and says what it sees barring its way.
## A short line and a mark where it stopped, shown for a few seconds then
## fading. Pure logic on the wall clock, no drawing and no network:
## testable headless; BlockedView draws it.

## The words for each reason the server gives (`body_blocked`'s `reason`),
## the one place they are kept.
const WORDS := {
	"water": "Bloqué : l'eau est trop profonde",
	"rock": "Bloqué : la falaise barre le chemin",
	"no_way": "Bloqué : aucun passage en vue",
}
## Milliseconds the notice shows, fade included.
const SHOW_MS := 4000
## Milliseconds of fading at the end.
const FADE_MS := 1000

## Where the body stopped, in millimetres.
var at := Vector2.ZERO
## The line shown.
var text := ""
## When it was shown (Time.get_ticks_msec()); negative: not shown.
var _since := -1


## The words for a reason; an unknown one reads as no way seen.
static func words(reason: String) -> String:
	return str(WORDS.get(reason, WORDS["no_way"]))


## Shows `line` for a body stopped at `stop` (mm).
func show(line: String, stop: Vector2, now_ms: int) -> void:
	at = stop
	text = line
	_since = now_ms


## Hides it at once (the body sets off again, another body is piloted).
func clear() -> void:
	_since = -1


## Whether it shows at `now_ms`.
func shown(now_ms: int) -> bool:
	return _since >= 0 and now_ms - _since < SHOW_MS


## Its opacity at `now_ms`: whole, then fading to nothing over FADE_MS.
func alpha(now_ms: int) -> float:
	if not shown(now_ms):
		return 0.0
	var left := SHOW_MS - (now_ms - _since)
	return clampf(float(left) / FADE_MS, 0.0, 1.0)
