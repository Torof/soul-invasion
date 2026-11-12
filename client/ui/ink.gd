class_name Ink
extends RefCounted
## The interface's drawing: shapes filled with cream paper and outlined
## with a dark brown ink line, slightly irregular as if drawn by hand
## (the style bible: thick dark outlines, brown-black, never pure black).
## Vector, drawn in code: no image.

const PAPER := Color(0.96, 0.92, 0.82)
## The paper under the mouse, a shade lighter.
const PAPER_HOVER := Color(1.0, 0.97, 0.89)
const INK := Color(0.18, 0.12, 0.09)
const LINE_WIDTH := 2.5
## Pixels the ink line strays from the shape, at most.
const WOBBLE := 0.7


## Fills `points` (a closed shape) with `fill` and outlines it in ink,
## straying `wobble` pixels at most, the line `width` wide.
static func shape(item: CanvasItem, points: PackedVector2Array, fill: Color, alpha: float = 1.0,
		wobble: float = WOBBLE, width: float = LINE_WIDTH) -> void:
	item.draw_colored_polygon(points, Color(fill, alpha))
	outline(item, points, alpha, wobble, width)


## The ink line round a closed shape, wobbling a little.
static func outline(item: CanvasItem, points: PackedVector2Array, alpha: float = 1.0,
		wobble: float = WOBBLE, width: float = LINE_WIDTH) -> void:
	var line := wobbled(points, wobble)
	line.append(line[0])
	item.draw_polyline(line, Color(INK, alpha), width, true)


## A circle filled and outlined.
static func circle(item: CanvasItem, centre: Vector2, radius: float, fill: Color, alpha: float = 1.0,
		wobble: float = WOBBLE, width: float = LINE_WIDTH) -> void:
	shape(item, oval(Rect2(centre - Vector2(radius, radius), Vector2(radius, radius) * 2.0)),
			fill, alpha, wobble, width)


## An oval filling `rect`, enough points for a smooth line at its size.
static func oval(rect: Rect2) -> PackedVector2Array:
	var points := PackedVector2Array()
	var centre := rect.get_center()
	var r := rect.size / 2.0
	var steps := clampi(int((r.x + r.y) * 1.2), 12, 160)
	for i in steps:
		var a := TAU * float(i) / float(steps)
		points.append(centre + Vector2(cos(a) * r.x, sin(a) * r.y))
	return points


## The points along an arc (angles in radians, screen y down).
static func arc(centre: Vector2, radius: float, from: float, to: float, steps: int) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in steps:
		var a := lerpf(from, to, float(i) / float(steps - 1 if to - from < TAU else steps))
		points.append(centre + Vector2(cos(a), sin(a)) * radius)
	return points


## A rounded rectangle, clockwise on screen from its top-left corner, with
## a tail on its bottom edge when `tail_width` > 0: from `tail_x` (along
## the edge, from the left) to the tip `tail_tip` and back.
static func rounded(rect: Rect2, radius: float, tail_x: float = 0.0, tail_width: float = 0.0,
		tail_tip: Vector2 = Vector2.ZERO) -> PackedVector2Array:
	var p := rect.position
	var e := rect.end
	var steps := 6
	var points := PackedVector2Array()
	points.append_array(arc(Vector2(p.x + radius, p.y + radius), radius, PI, PI * 1.5, steps))
	points.append_array(arc(Vector2(e.x - radius, p.y + radius), radius, PI * 1.5, TAU, steps))
	points.append_array(arc(Vector2(e.x - radius, e.y - radius), radius, 0.0, PI * 0.5, steps))
	if tail_width > 0.0:
		points.append(Vector2(p.x + tail_x + tail_width, e.y))
		points.append(tail_tip)
		points.append(Vector2(p.x + tail_x, e.y))
	points.append_array(arc(Vector2(p.x + radius, e.y - radius), radius, PI * 0.5, PI, steps))
	return points


## `points` moved a little across the line, smoothly and always the same
## way for the same shape (no flicker from frame to frame).
static func wobbled(points: PackedVector2Array, wobble: float = WOBBLE) -> PackedVector2Array:
	var out := PackedVector2Array()
	var n := points.size()
	for i in n:
		var along := points[(i + 1) % n] - points[(i - 1 + n) % n]
		var across := Vector2(-along.y, along.x).normalized()
		var stray := wobble * (0.6 * sin(i * 1.7) + 0.4 * sin(i * 0.53 + 1.0))
		out.append(points[i] + across * stray)
	return out
