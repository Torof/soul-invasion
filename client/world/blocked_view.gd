class_name BlockedView
extends Node2D
## Draws the "blocked" notice (BlockedNotice) over the world: a short line
## above where the piloted body stopped and a small cross at its feet,
## fading after a few seconds. Kept apart on purpose: it takes only a line
## and a place (`tell`), so the body's thoughts can take its place later
## with neither the protocol nor the simulation touched.

const LINE_COLOR := Color(1.0, 0.93, 0.80)
const MARK_COLOR := Color(0.95, 0.35, 0.25)
## Screen pixels of the line's font, whatever the zoom.
const FONT_SIZE := 15
## Metres above the feet the line stands: over the head.
const LINE_HEIGHT_M := 2.3
## Half the cross's arms, in world pixels.
const MARK_HALF := 6.0

var notice := BlockedNotice.new()
## Whether the last frame drew something: one more redraw erases it.
var _drawn := false


## Shows `line` for a body stopped at `at` (mm).
func tell(line: String, at: Vector2) -> void:
	notice.show(line, at, Time.get_ticks_msec())
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
	# The line at a steady size on screen: drawn unscaled by the zoom.
	var zoom := get_viewport().get_canvas_transform().get_scale().x
	var font := ThemeDB.fallback_font
	var width := font.get_string_size(notice.text, HORIZONTAL_ALIGNMENT_LEFT, -1, FONT_SIZE).x
	var anchor := feet - Vector2(0, Iso.PX_PER_M * LINE_HEIGHT_M)
	draw_set_transform(anchor, 0.0, Vector2.ONE / maxf(zoom, 0.01))
	var at := Vector2(-width / 2.0, 0)
	draw_string_outline(font, at, notice.text, HORIZONTAL_ALIGNMENT_LEFT, -1, FONT_SIZE, 4, Color(0, 0, 0, alpha * 0.8))
	draw_string(font, at, notice.text, HORIZONTAL_ALIGNMENT_LEFT, -1, FONT_SIZE, Color(LINE_COLOR, alpha))
	draw_set_transform(Vector2.ZERO)
