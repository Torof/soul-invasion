class_name SpeechBubble
extends Control
## What someone says, on the left: a classic comic speech bubble on cream
## paper outlined in ink, the speaker's name on top, its tail pointing
## down. Shows a line ~6 s or until the next (SpeechLine). Nothing in the
## game speaks yet: HUD.say is the way in, for players and NPCs later.

const FONT_SIZE := 17
const NAME_SIZE := 14
const TEXT_WIDTH := 300.0
const PAD := Vector2(16, 12)
const CORNER := 14.0
const MARGIN := 16.0
## Its top, as a share of the screen's height.
const TOP_SHARE := 0.32
const TAIL := 20.0

var line := SpeechLine.new()
var _shown := ""
var _box := Rect2()


func _ready() -> void:
	set_anchors_preset(Control.PRESET_TOP_LEFT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	visible = false


## `speaker` says `words`.
func say(speaker: String, words: String) -> void:
	line.say(speaker, words, Time.get_ticks_msec())
	_shown = ""


func _process(_delta: float) -> void:
	var alpha := line.alpha(Time.get_ticks_msec())
	visible = alpha > 0.0
	if not visible:
		return
	modulate.a = alpha
	var key := line.speaker + "\n" + line.text
	if key != _shown:
		_shown = key
		_lay_out()
		queue_redraw()


func _lay_out() -> void:
	var font := ThemeDB.fallback_font
	var words := font.get_multiline_string_size(line.text, HORIZONTAL_ALIGNMENT_LEFT, TEXT_WIDTH, FONT_SIZE)
	var name_width := font.get_string_size(line.speaker, HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE).x
	var inner := Vector2(maxf(maxf(words.x, name_width), 120.0), font.get_height(NAME_SIZE) + 6.0 + words.y)
	_box = Rect2(Vector2.ZERO, inner + PAD * 2.0)
	position = Vector2(MARGIN, get_viewport_rect().size.y * TOP_SHARE)
	size = _box.size + Vector2(0, TAIL)


func _draw() -> void:
	if _box.size == Vector2.ZERO:
		return
	var tail_x := 26.0
	Ink.shape(self, Ink.rounded(_box, CORNER, tail_x, 16.0, Vector2(tail_x - 6.0, _box.end.y + TAIL)), Ink.PAPER)
	var font := ThemeDB.fallback_font
	var at := PAD + Vector2(0, font.get_ascent(NAME_SIZE))
	draw_string(font, at, line.speaker, HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE, Ink.INK)
	# The name underlined in ink, as a comic's caption.
	var name_width := font.get_string_size(line.speaker, HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE).x
	draw_line(at + Vector2(0, 3), at + Vector2(name_width, 3), Ink.INK, 1.5, true)
	var text_at := Vector2(PAD.x, PAD.y + font.get_height(NAME_SIZE) + 6.0 + font.get_ascent(FONT_SIZE))
	draw_multiline_string(font, text_at, line.text, HORIZONTAL_ALIGNMENT_LEFT, TEXT_WIDTH, FONT_SIZE, -1, Ink.INK)
