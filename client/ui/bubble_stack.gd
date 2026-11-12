class_name BubbleStack
extends Control
## A side's bubbles at the top of the screen, as a chat thread
## (BubbleThread): the newest at the bottom, the two before it above,
## smaller and a little transparent. The wheel over the stack scrolls the
## history; a thought tied to a place is clicked to take the camera there;
## left alone the stack folds into one small bubble. ThoughtBubble and
## SpeechBubble draw the bubbles themselves (_measure, _paint).

## A bubble tied to a place was clicked: the camera goes to `at` (mm).
signal place_clicked(at: Vector2)

const MARGIN := 16.0
## Below the help line along the top.
const TOP := 40.0
## Room between two bubbles of the stack.
const GAP := 4.0
## Each place of the window, its size and opacity, the top first: the
## newest, at the bottom, whole.
const SCALES: Array[float] = [0.72, 0.84, 1.0]
const ALPHAS: Array[float] = [0.55, 0.75, 1.0]
## The folded bubble's size and opacity.
const FOLDED_SCALE := 0.72
const FOLDED_ALPHA := 0.75

var thread := BubbleThread.new()
## On the right of the screen (thoughts) or the left (speech).
var _right := false
## Where each bubble of the window is drawn, the top first, and what.
var _rects: Array[Rect2] = []
var _drawn: Array[Dictionary] = []
var _scales: Array[float] = []
var _alphas: Array[float] = []
var _folded := false
var _shown_key := ""
var _hover_slot := -1


func _ready() -> void:
	set_anchors_preset(Control.PRESET_TOP_RIGHT if _right else Control.PRESET_TOP_LEFT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	mouse_entered.connect(func() -> void: thread.hover(true, Time.get_ticks_msec()))
	mouse_exited.connect(func() -> void:
		thread.hover(false, Time.get_ticks_msec())
		_set_hover_slot(-1))
	visible = false


## Forgets every bubble.
func clear() -> void:
	thread.clear()


func _process(_delta: float) -> void:
	visible = thread.size() > 0
	if not visible:
		return
	var folded := thread.folded(Time.get_ticks_msec())
	var key := "%s|%d|%d" % [folded, thread.scrolled(), thread.size()]
	for entry in thread.shown():
		key += "|%s|%s|%s" % [entry["speaker"], entry["text"], entry["placed"]]
	if key != _shown_key:
		_shown_key = key
		_folded = folded
		_lay_out()
		queue_redraw()


## Stacks the window's bubbles down from the top, or the folded one.
func _lay_out() -> void:
	_drawn.clear()
	_scales.clear()
	_alphas.clear()
	if _folded:
		var newest: Dictionary = thread.shown().back()
		_drawn.append({"speaker": "", "text": thread.folded_text(), "placed": false, "at": newest["at"]})
		_scales.append(FOLDED_SCALE)
		_alphas.append(FOLDED_ALPHA)
	else:
		_drawn = thread.shown()
		var skip := SCALES.size() - _drawn.size()
		for i in _drawn.size():
			_scales.append(SCALES[skip + i])
			_alphas.append(ALPHAS[skip + i])
	var sizes: Array[Vector2] = []
	var width := 0.0
	for i in _drawn.size():
		var room := _measure(_drawn[i], _scales[i], _folded, i == _drawn.size() - 1)
		sizes.append(room)
		width = maxf(width, room.x)
	_rects.clear()
	var y := 0.0
	for room in sizes:
		_rects.append(Rect2(Vector2(width - room.x if _right else 0.0, y), room))
		y += room.y + GAP
	var height := y - GAP
	if _right:
		offset_left = -MARGIN - width
		offset_right = -MARGIN
	else:
		offset_left = MARGIN
		offset_right = MARGIN + width
	offset_top = TOP
	offset_bottom = TOP + height


## The room a bubble takes at `scale`, with its tail or trail (`last`:
## the bottom one).
func _measure(_entry: Dictionary, _scale: float, _is_folded: bool, _last: bool) -> Vector2:
	return Vector2.ZERO


## Draws a bubble in `rect`.
func _paint(_entry: Dictionary, _rect: Rect2, _scale: float, _paper: Color, _alpha: float,
		_is_folded: bool, _last: bool) -> void:
	pass


## Whether `point` is on the bubble drawn in `rect` (its corners are not).
func _on_bubble(rect: Rect2, point: Vector2) -> bool:
	return rect.has_point(point)


## The bubble of the window under `point`, from the top; -1: none.
func _slot_at(point: Vector2) -> int:
	for i in _rects.size():
		if _on_bubble(_rects[i], point):
			return i
	return -1


func _has_point(point: Vector2) -> bool:
	return _slot_at(point) >= 0


func _gui_input(event: InputEvent) -> void:
	var motion := event as InputEventMouseMotion
	if motion != null:
		_set_hover_slot(_slot_at(motion.position))
		return
	var button := event as InputEventMouseButton
	if button == null or not button.pressed:
		return
	match button.button_index:
		MOUSE_BUTTON_WHEEL_UP:
			thread.scroll(1)
			accept_event()
		MOUSE_BUTTON_WHEEL_DOWN:
			thread.scroll(-1)
			accept_event()
		MOUSE_BUTTON_LEFT:
			if _folded:
				return
			var place: Variant = thread.place_at(_slot_at(button.position))
			if place != null:
				place_clicked.emit(place)
				accept_event()


func _set_hover_slot(slot: int) -> void:
	if slot == _hover_slot:
		return
	_hover_slot = slot
	var clickable := not _folded and thread.place_at(slot) != null
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND if clickable else Control.CURSOR_ARROW
	queue_redraw()


func _draw() -> void:
	for i in _rects.size():
		var lit := i == _hover_slot and not _folded and thread.place_at(i) != null
		_paint(_drawn[i], _rects[i], _scales[i], Ink.PAPER_HOVER if lit else Ink.PAPER, _alphas[i],
				_folded, i == _rects.size() - 1)
