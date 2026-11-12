class_name ThoughtBubble
extends BubbleStack
## The piloted body's thoughts, top right: round bubbles on cream paper
## outlined in a calm ink line, stacked as a thread (BubbleStack), the
## newest at the bottom trailing three small circles down toward the body.
## A thought tied to a place is clicked, even back in the history, to take
## the camera there. Left alone, a small round thought stays in the corner.

const FONT_SIZE := 17
## The widest a line runs before it wraps.
const TEXT_WIDTH := 230.0
## The oval round the words: the words' box grown by STRETCH (its corners
## then inside the oval), plus PAD.
const STRETCH := 1.3
const PAD := Vector2(28, 30)
## Room under the newest bubble for the trailing circles.
const TRAIL := 34.0
## The bubble's ink line strays less than the rest of the interface's.
const WOBBLE := 0.25


func _init() -> void:
	_right = true


## Thinks `line`, tied to the place `at` (mm) when `placed`.
func think(line: String, at: Vector2, placed: bool) -> void:
	thread.push("", line, at, placed, Time.get_ticks_msec())


func _measure(entry: Dictionary, factor: float, is_folded: bool, last: bool) -> Vector2:
	return _oval_size(str(entry["text"]), factor, is_folded) + Vector2(0, TRAIL * factor if last else 0.0)


func _oval_size(text: String, factor: float, is_folded: bool) -> Vector2:
	var words := _words(text, factor, is_folded)
	return Vector2(maxf(words.x, 70.0 * factor), words.y) * STRETCH + PAD * factor


func _words(text: String, factor: float, is_folded: bool) -> Vector2:
	return ThemeDB.fallback_font.get_multiline_string_size(text, HORIZONTAL_ALIGNMENT_CENTER,
			_wrap(factor, is_folded), _font_size(factor))


## Folded, the line does not wrap.
func _wrap(factor: float, is_folded: bool) -> float:
	return -1.0 if is_folded else TEXT_WIDTH * factor


func _font_size(factor: float) -> int:
	return roundi(FONT_SIZE * factor)


## Only the oval takes the mouse, not the corners around it nor the trail.
func _on_bubble(rect: Rect2, point: Vector2) -> bool:
	var i := _rects.find(rect)
	var trail := TRAIL * _scales[i] if i == _rects.size() - 1 else 0.0
	var oval := Rect2(rect.position, rect.size - Vector2(0, trail))
	var r := oval.size / 2.0
	if r.x <= 0.0 or r.y <= 0.0:
		return false
	return ((point - oval.get_center()) / r).length_squared() <= 1.0


func _paint(canvas: CanvasItem, entry: Dictionary, rect: Rect2, factor: float, paper: Color,
		alpha: float, is_folded: bool, last: bool) -> void:
	var text := str(entry["text"])
	var oval := Rect2(rect.position, _oval_size(text, factor, is_folded))
	if last:
		# Down and to the left: the body is in the middle of the screen.
		var foot := Vector2(oval.position.x + oval.size.x * 0.3, oval.get_center().y + oval.size.y * 0.45)
		Ink.circle(canvas, foot + Vector2(-6, 10) * factor, 6.5 * factor, paper, alpha, WOBBLE)
		Ink.circle(canvas, foot + Vector2(-16, 21) * factor, 4.5 * factor, paper, alpha, WOBBLE)
		Ink.circle(canvas, foot + Vector2(-25, 30) * factor, 3.0 * factor, paper, alpha, WOBBLE)
	Ink.shape(canvas, Ink.oval(oval), paper, alpha, WOBBLE)
	var font := ThemeDB.fallback_font
	var font_size := _font_size(factor)
	var words := _words(text, factor, is_folded)
	var width := words.x if is_folded else TEXT_WIDTH * factor
	var at := Vector2(oval.get_center().x - width / 2.0, oval.get_center().y - words.y / 2.0 + font.get_ascent(font_size))
	canvas.draw_multiline_string(font, at, text, HORIZONTAL_ALIGNMENT_CENTER, width, font_size, -1,
			Color(Ink.INK, alpha))


## A small round thought, two dots trailing down to the left.
func _paint_icon(rect: Rect2, paper: Color, alpha: float) -> void:
	var line := 1.6
	Ink.shape(self, Ink.oval(Rect2(rect.position + Vector2(8, 1), Vector2(19, 14))), paper, alpha, WOBBLE, line)
	Ink.circle(self, rect.position + Vector2(7, 17), 2.2, paper, alpha, WOBBLE, line)
	Ink.circle(self, rect.position + Vector2(3, 20.5), 1.3, paper, alpha, WOBBLE, line)
