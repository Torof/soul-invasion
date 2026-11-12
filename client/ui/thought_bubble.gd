class_name ThoughtBubble
extends Control
## The piloted body's thoughts, top right: a comic thought cloud on cream
## paper outlined in ink, with three small circles trailing down toward
## the body. One thought at a time (ThoughtQueue); a thought tied to a
## place is clicked to take the camera there.

## A thought tied to a place was clicked: the camera goes to `at` (mm).
signal place_clicked(at: Vector2)

const FONT_SIZE := 17
## The widest a line runs before it wraps.
const TEXT_WIDTH := 280.0
## Room between the text and the cloud's bumps.
const PAD := Vector2(30, 24)
const MARGIN := 16.0
## Below the help line along the top.
const TOP := 40.0
## Room under the cloud for the trailing circles.
const TRAIL := 34.0
## The cloud's bumps: how many, and how deep, as a share of its size.
const BUMPS := 11
const BUMP_DEPTH := 0.1

var queue := ThoughtQueue.new()
var _text := ""
var _hover := false
var _cloud := Rect2()


func _ready() -> void:
	set_anchors_preset(Control.PRESET_TOP_RIGHT)
	mouse_entered.connect(func() -> void:
		_hover = true
		queue_redraw())
	mouse_exited.connect(func() -> void:
		_hover = false
		queue_redraw())
	visible = false


## Thinks `line`, tied to the place `at` (mm) when `placed`.
func think(line: String, at: Vector2, placed: bool) -> void:
	queue.push(line, at, placed, Time.get_ticks_msec())


## Forgets every thought.
func clear() -> void:
	queue.clear()


func _process(_delta: float) -> void:
	var now := Time.get_ticks_msec()
	var alpha := queue.alpha(now)
	visible = alpha > 0.0
	if not visible:
		return
	modulate.a = alpha
	var clickable := queue.clickable(now)
	mouse_filter = Control.MOUSE_FILTER_STOP if clickable else Control.MOUSE_FILTER_IGNORE
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND if clickable else Control.CURSOR_ARROW
	var text := queue.text(now)
	if text != _text:
		_text = text
		_lay_out()
		queue_redraw()


## Sizes the cloud to the line and puts it in the top right corner.
func _lay_out() -> void:
	var font := ThemeDB.fallback_font
	var words := font.get_multiline_string_size(_text, HORIZONTAL_ALIGNMENT_CENTER, TEXT_WIDTH, FONT_SIZE)
	var cloud := Vector2(maxf(words.x, 120.0), words.y) + PAD * 2.0
	_cloud = Rect2(Vector2.ZERO, cloud)
	offset_left = -MARGIN - cloud.x
	offset_right = -MARGIN
	offset_top = TOP
	offset_bottom = TOP + cloud.y + TRAIL


## Only the cloud takes the click, not the corners around it.
func _has_point(point: Vector2) -> bool:
	var r := _cloud.size / 2.0
	if r.x <= 0.0 or r.y <= 0.0:
		return false
	var d := (point - _cloud.get_center()) / r
	return d.length_squared() <= 1.0


func _gui_input(event: InputEvent) -> void:
	var click := event as InputEventMouseButton
	if click == null or click.button_index != MOUSE_BUTTON_LEFT or not click.pressed:
		return
	var now := Time.get_ticks_msec()
	if queue.clickable(now):
		place_clicked.emit(queue.place(now))
		accept_event()


func _draw() -> void:
	if _cloud.size == Vector2.ZERO:
		return
	var paper := Ink.PAPER_HOVER if _hover and queue.clickable(Time.get_ticks_msec()) else Ink.PAPER
	# The trailing circles first, smaller as they go down toward the body
	# (down and to the left: the body is in the middle of the screen).
	var foot := Vector2(_cloud.size.x * 0.3, _cloud.get_center().y + _cloud.size.y * 0.45)
	Ink.circle(self, foot + Vector2(-6, 10), 6.5, paper)
	Ink.circle(self, foot + Vector2(-16, 21), 4.5, paper)
	Ink.circle(self, foot + Vector2(-25, 30), 3.0, paper)
	Ink.shape(self, cloud_points(_cloud), paper)
	var font := ThemeDB.fallback_font
	var words := font.get_multiline_string_size(_text, HORIZONTAL_ALIGNMENT_CENTER, TEXT_WIDTH, FONT_SIZE)
	var at := Vector2(_cloud.get_center().x - TEXT_WIDTH / 2.0, _cloud.get_center().y - words.y / 2.0 + font.get_ascent(FONT_SIZE))
	draw_multiline_string(font, at, _text, HORIZONTAL_ALIGNMENT_CENTER, TEXT_WIDTH, FONT_SIZE, -1, Ink.INK)


## A cloud filling `rect`: an ellipse with round bumps all round, meeting
## in sharp notches.
static func cloud_points(rect: Rect2) -> PackedVector2Array:
	var points := PackedVector2Array()
	var centre := rect.get_center()
	var r := rect.size / 2.0 / (1.0 + BUMP_DEPTH)
	var steps := BUMPS * 12
	for i in steps:
		var a := TAU * float(i) / float(steps)
		var bump := 1.0 + BUMP_DEPTH * absf(sin(a * BUMPS / 2.0))
		points.append(centre + Vector2(cos(a) * r.x, sin(a) * r.y) * bump)
	return points
