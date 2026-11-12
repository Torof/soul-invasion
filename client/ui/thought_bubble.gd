class_name ThoughtBubble
extends BubbleStack
## The piloted body's thoughts, top right: round comic thought clouds on
## cream paper outlined in ink, stacked as a thread (BubbleStack), the
## newest at the bottom trailing three small circles down toward the body.
## A thought tied to a place is clicked, even back in the history, to take
## the camera there.

const FONT_SIZE := 17
## The widest a line runs before it wraps.
const TEXT_WIDTH := 230.0
## Room between the text and the cloud's lobes.
const PAD := Vector2(30, 26)
## Room under the newest cloud for the trailing circles.
const TRAIL := 34.0
## The cloud's lobes: few and large, a round cloud, and how deep.
const LOBES := 7
const LOBE_DEPTH := 0.12
## The cloud's ink line strays less than the rest of the interface's.
const WOBBLE := 0.25


func _init() -> void:
	_right = true


## Thinks `line`, tied to the place `at` (mm) when `placed`.
func think(line: String, at: Vector2, placed: bool) -> void:
	thread.push("", line, at, placed, Time.get_ticks_msec())


func _measure(entry: Dictionary, factor: float, is_folded: bool, last: bool) -> Vector2:
	return _cloud_size(str(entry["text"]), factor, is_folded) + Vector2(0, TRAIL * factor if last else 0.0)


func _cloud_size(text: String, factor: float, is_folded: bool) -> Vector2:
	var words := _words(text, factor, is_folded)
	return Vector2(maxf(words.x, 90.0 * factor), words.y) + PAD * factor * 2.0


func _words(text: String, factor: float, is_folded: bool) -> Vector2:
	return ThemeDB.fallback_font.get_multiline_string_size(text, HORIZONTAL_ALIGNMENT_CENTER,
			_wrap(factor, is_folded), _font_size(factor))


## Folded, the line does not wrap.
func _wrap(factor: float, is_folded: bool) -> float:
	return -1.0 if is_folded else TEXT_WIDTH * factor


func _font_size(factor: float) -> int:
	return roundi(FONT_SIZE * factor)


## Only the cloud takes the mouse, not the corners around it nor the trail.
func _on_bubble(rect: Rect2, point: Vector2) -> bool:
	var i := _rects.find(rect)
	var trail := TRAIL * _scales[i] if i == _rects.size() - 1 else 0.0
	var cloud := Rect2(rect.position, rect.size - Vector2(0, trail))
	var r := cloud.size / 2.0
	if r.x <= 0.0 or r.y <= 0.0:
		return false
	return ((point - cloud.get_center()) / r).length_squared() <= 1.0


func _paint(entry: Dictionary, rect: Rect2, factor: float, paper: Color, alpha: float,
		is_folded: bool, last: bool) -> void:
	var text := str(entry["text"])
	var cloud := Rect2(rect.position, _cloud_size(text, factor, is_folded))
	if last:
		# Down and to the left: the body is in the middle of the screen.
		var foot := Vector2(cloud.position.x + cloud.size.x * 0.3, cloud.get_center().y + cloud.size.y * 0.45)
		Ink.circle(self, foot + Vector2(-6, 10) * factor, 6.5 * factor, paper, alpha, WOBBLE)
		Ink.circle(self, foot + Vector2(-16, 21) * factor, 4.5 * factor, paper, alpha, WOBBLE)
		Ink.circle(self, foot + Vector2(-25, 30) * factor, 3.0 * factor, paper, alpha, WOBBLE)
	Ink.shape(self, cloud_points(cloud), paper, alpha, WOBBLE)
	var font := ThemeDB.fallback_font
	var font_size := _font_size(factor)
	var words := _words(text, factor, is_folded)
	var width := words.x if is_folded else TEXT_WIDTH * factor
	var at := Vector2(cloud.get_center().x - width / 2.0, cloud.get_center().y - words.y / 2.0 + font.get_ascent(font_size))
	draw_multiline_string(font, at, text, HORIZONTAL_ALIGNMENT_CENTER, width, font_size, -1, Color(Ink.INK, alpha))


## A cloud filling `rect`: an ellipse with a few large round lobes all
## round, meeting in notches.
static func cloud_points(rect: Rect2) -> PackedVector2Array:
	var points := PackedVector2Array()
	var centre := rect.get_center()
	var r := rect.size / 2.0 / (1.0 + LOBE_DEPTH)
	var steps := LOBES * 16
	for i in steps:
		var a := TAU * float(i) / float(steps)
		var lobe := 1.0 + LOBE_DEPTH * absf(sin(a * LOBES / 2.0))
		points.append(centre + Vector2(cos(a) * r.x, sin(a) * r.y) * lobe)
	return points
