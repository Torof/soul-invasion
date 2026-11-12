class_name SpeechBubble
extends BubbleStack
## What is said, top left: classic comic speech bubbles on cream paper
## outlined in ink, the speaker's name on top, the tail pointing down,
## stacked as a thread (BubbleStack). Nothing in the game speaks yet:
## HUD.say is the way in, for players and NPCs later.

const FONT_SIZE := 17
const NAME_SIZE := 14
const TEXT_WIDTH := 300.0
const PAD := Vector2(16, 12)
const CORNER := 14.0
const TAIL := 14.0
## Room between the name and the line.
const NAME_GAP := 6.0


func _init() -> void:
	_right = false


## `speaker` says `words`.
func say(speaker: String, words: String) -> void:
	thread.push(speaker, words, Vector2.ZERO, false, Time.get_ticks_msec())


func _measure(entry: Dictionary, factor: float, is_folded: bool, _last: bool) -> Vector2:
	return _box_size(entry, factor, is_folded) + Vector2(0, TAIL * factor)


## The bubble without its tail: the name above the line, or the line
## alone when folded (it carries the name).
func _box_size(entry: Dictionary, factor: float, is_folded: bool) -> Vector2:
	var font := ThemeDB.fallback_font
	var words := font.get_multiline_string_size(str(entry["text"]), HORIZONTAL_ALIGNMENT_LEFT,
			-1.0 if is_folded else TEXT_WIDTH * factor, roundi(FONT_SIZE * factor))
	var inner := Vector2(maxf(words.x, (40.0 if is_folded else 120.0) * factor), words.y)
	if not is_folded:
		var name_size := roundi(NAME_SIZE * factor)
		inner.x = maxf(inner.x, font.get_string_size(str(entry["speaker"]), HORIZONTAL_ALIGNMENT_LEFT, -1, name_size).x)
		inner.y += font.get_height(name_size) + NAME_GAP * factor
	return inner + PAD * factor * 2.0


func _paint(entry: Dictionary, rect: Rect2, factor: float, paper: Color, alpha: float,
		is_folded: bool, _last: bool) -> void:
	var box := Rect2(rect.position, _box_size(entry, factor, is_folded))
	var tail_x := 26.0 * factor
	Ink.shape(self, Ink.rounded(box, CORNER * factor, tail_x, 16.0 * factor,
			Vector2(box.position.x + tail_x - 6.0 * factor, box.end.y + TAIL * factor)), paper, alpha)
	var font := ThemeDB.fallback_font
	var ink := Color(Ink.INK, alpha)
	var font_size := roundi(FONT_SIZE * factor)
	var text_y := box.position.y + PAD.y * factor
	if not is_folded:
		var name_size := roundi(NAME_SIZE * factor)
		var speaker := str(entry["speaker"])
		var at := box.position + PAD * factor + Vector2(0, font.get_ascent(name_size))
		draw_string(font, at, speaker, HORIZONTAL_ALIGNMENT_LEFT, -1, name_size, ink)
		# The name underlined in ink, as a comic's caption.
		var name_width := font.get_string_size(speaker, HORIZONTAL_ALIGNMENT_LEFT, -1, name_size).x
		draw_line(at + Vector2(0, 3), at + Vector2(name_width, 3), ink, 1.5, true)
		text_y += font.get_height(name_size) + NAME_GAP * factor
	var text_at := Vector2(box.position.x + PAD.x * factor, text_y + font.get_ascent(font_size))
	draw_multiline_string(font, text_at, str(entry["text"]), HORIZONTAL_ALIGNMENT_LEFT,
			-1.0 if is_folded else TEXT_WIDTH * factor, font_size, -1, ink)
