class_name DevGreeter
extends RefCounted
## PLACEHOLDER, development only (--dev), until NPCs really talk: body 3
## greets the piloted body when it comes within RADIUS_MM of it, with a
## line picked at random (UiWords.DEV_GREETINGS), once on coming near and
## never twice within COOLDOWN_MS, so standing beside it says nothing more.
## Pure logic: testable headless.

## The body that greets.
const SPEAKER := 3
const RADIUS_MM := 3000.0
const COOLDOWN_MS := 20_000

## Whether the piloted body was near at the last look.
var _near := false
## When it last spoke; far in the past at first.
var _spoke := -COOLDOWN_MS
var _last := -1
var rng := RandomNumberGenerator.new()


## Looks at the distance between the two bodies (mm; INF: body 3 unseen)
## at `now_ms`: the line said, or empty when nothing is said.
func look(distance_mm: float, now_ms: int) -> String:
	var near := distance_mm <= RADIUS_MM
	var arriving := near and not _near
	_near = near
	if not arriving or now_ms - _spoke < COOLDOWN_MS:
		return ""
	_spoke = now_ms
	var count := UiWords.DEV_GREETINGS.size()
	var pick := rng.randi_range(0, count - 1)
	if pick == _last:
		pick = (pick + 1) % count
	_last = pick
	return str(UiWords.DEV_GREETINGS[pick])
