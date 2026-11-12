class_name BlockedView
extends Node2D
## Draws the "blocked" mark (BlockedNotice) over the world: a small red
## cross at the feet of the piloted body where it stopped, fading after a
## few seconds. What it sees barring its way is said in its thought bubble
## (ThoughtBubble); the cross shows the place in the world.

const MARK_COLOR := Color(0.95, 0.35, 0.25)
## Half the cross's arms, in world pixels.
const MARK_HALF := 6.0

var notice := BlockedNotice.new()
## Whether the last frame drew something: one more redraw erases it.
var _drawn := false


## Marks a body stopped at `at` (mm).
func mark(at: Vector2) -> void:
	notice.show(at, Time.get_ticks_msec())
	queue_redraw()


## Hides the notice at once.
func clear() -> void:
	notice.clear()
	queue_redraw()


func _process(_delta: float) -> void:
	if _drawn or notice.shown(Time.get_ticks_msec()):
		queue_redraw()


func _draw() -> void:
	var alpha := notice.alpha(Time.get_ticks_msec())
	_drawn = alpha > 0.0
	if not _drawn:
		return
	var feet := Iso.point_to_screen(notice.at)
	var mark := Color(MARK_COLOR, alpha)
	draw_line(feet + Vector2(-MARK_HALF, -MARK_HALF / 2.0), feet + Vector2(MARK_HALF, MARK_HALF / 2.0), mark, 2.0)
	draw_line(feet + Vector2(-MARK_HALF, MARK_HALF / 2.0), feet + Vector2(MARK_HALF, -MARK_HALF / 2.0), mark, 2.0)
