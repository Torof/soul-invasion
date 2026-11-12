class_name BubbleStack
extends Control
## A side's bubbles at the top of the screen, as a chat thread
## (BubbleThread): the newest at the bottom. The bubble in focus is whole
## and sharp; the ones around it, older above and newer below, smaller,
## a little transparent and blurred. The wheel over the stack moves the
## focus through the history; a thought tied to a place is clicked to take
## the camera there; left alone the stack folds into one small bubble, then
## into a small icon in the corner, clicked to open it again. ThoughtBubble
## and SpeechBubble draw the bubbles and the icon (_measure, _paint,
## _paint_icon).
##
## The blur: the bubbles out of focus are drawn into a small transparent
## viewport, only when what they show changes, and that texture is shown
## through a nine-tap blur shader (bubble_blur.gdshader). Each frame costs
## one quad of the stack's size, nine texture reads a pixel.

## A bubble tied to a place was clicked: the camera goes to `at` (mm).
signal place_clicked(at: Vector2)

const MARGIN := 16.0
## Below the help line along the top.
const TOP := 40.0
## Room between two bubbles of the stack.
const GAP := 4.0
## A bubble's size and opacity by how far it is from the focus: the focus
## whole, then one away, then two.
const SCALES: Array[float] = [1.0, 0.84, 0.72]
const ALPHAS: Array[float] = [1.0, 0.75, 0.55]
## The folded bubble's size and opacity.
const FOLDED_SCALE := 0.72
const FOLDED_ALPHA := 0.75
## The icon left in the corner, its size and opacity.
const ICON_SIZE := Vector2(28, 22)
const ICON_ALPHA := 0.7
## Room round the blurred bubbles in their texture, for the blur to spread.
const BLUR_PAD := 6.0

enum Look { OPEN, FOLDED, ICON }

var thread := BubbleThread.new()
## On the right of the screen (thoughts) or the left (speech).
var _right := false
## Where each bubble of the window is drawn, the top first, and what.
var _rects: Array[Rect2] = []
var _drawn: Array[Dictionary] = []
var _scales: Array[float] = []
var _alphas: Array[float] = []
## The window's slot drawn sharp: the focus, or the folded bubble.
var _sharp := -1
var _look := Look.OPEN
var _shown_key := ""
var _hover_slot := -1
## The bubbles out of focus: drawn on `_sheet` in `_blur_view`, shown
## blurred by `_blurred`.
var _blur_view: SubViewport
var _sheet: Control
var _blurred: TextureRect


func _ready() -> void:
	set_anchors_preset(Control.PRESET_TOP_RIGHT if _right else Control.PRESET_TOP_LEFT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	mouse_entered.connect(func() -> void: thread.hover(true, Time.get_ticks_msec()))
	mouse_exited.connect(func() -> void:
		thread.hover(false, Time.get_ticks_msec())
		_set_hover_slot(-1))
	_blur_view = SubViewport.new()
	_blur_view.transparent_bg = true
	_blur_view.disable_3d = true
	_blur_view.gui_disable_input = true
	_blur_view.render_target_update_mode = SubViewport.UPDATE_DISABLED
	_sheet = Control.new()
	_sheet.draw.connect(_paint_sheet)
	_blur_view.add_child(_sheet)
	add_child(_blur_view)
	_blurred = TextureRect.new()
	_blurred.texture = _blur_view.get_texture()
	_blurred.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_blurred.show_behind_parent = true
	var blur := ShaderMaterial.new()
	blur.shader = preload("res://ui/bubble_blur.gdshader")
	_blurred.material = blur
	add_child(_blurred)
	visible = false


## Forgets every bubble.
func clear() -> void:
	thread.clear()


func _process(_delta: float) -> void:
	visible = thread.size() > 0
	if not visible:
		return
	var now := Time.get_ticks_msec()
	var look := Look.ICON if thread.hidden(now) else (Look.FOLDED if thread.folded(now) else Look.OPEN)
	var key := "%d|%d|%d" % [look, thread.scrolled(), thread.size()]
	for entry in thread.shown():
		key += "|%s|%s|%s" % [entry["speaker"], entry["text"], entry["placed"]]
	if key != _shown_key:
		_shown_key = key
		_look = look
		_lay_out()
		_set_hover_slot(-1)
		_redraw()


## Stacks the window's bubbles down from the top, or the folded one, or
## leaves only the icon.
func _lay_out() -> void:
	_drawn.clear()
	_scales.clear()
	_alphas.clear()
	_rects.clear()
	_sharp = 0
	if _look == Look.ICON:
		_rects.append(Rect2(Vector2.ZERO, ICON_SIZE))
		_place(ICON_SIZE)
		return
	if _look == Look.FOLDED:
		_drawn.append({"speaker": "", "text": thread.folded_text(), "placed": false, "at": Vector2.ZERO})
		_scales.append(FOLDED_SCALE)
		_alphas.append(FOLDED_ALPHA)
	else:
		_drawn = thread.shown()
		_sharp = thread.focus_slot()
		for i in _drawn.size():
			var away := mini(absi(i - _sharp), SCALES.size() - 1)
			_scales.append(SCALES[away])
			_alphas.append(ALPHAS[away])
	var sizes: Array[Vector2] = []
	var width := 0.0
	for i in _drawn.size():
		var room := _measure(_drawn[i], _scales[i], _look == Look.FOLDED, i == _drawn.size() - 1)
		sizes.append(room)
		width = maxf(width, room.x)
	var y := 0.0
	for room in sizes:
		_rects.append(Rect2(Vector2(width - room.x if _right else 0.0, y), room))
		y += room.y + GAP
	_place(Vector2(width, y - GAP))


## Sets the stack's own rectangle, `size_wanted` in its corner, and the
## blurred texture's round it.
func _place(size_wanted: Vector2) -> void:
	if _right:
		offset_left = -MARGIN - size_wanted.x
		offset_right = -MARGIN
	else:
		offset_left = MARGIN
		offset_right = MARGIN + size_wanted.x
	offset_top = TOP
	offset_bottom = TOP + size_wanted.y
	var pad := Vector2(BLUR_PAD, BLUR_PAD)
	_blur_view.size = Vector2i((size_wanted + pad * 2.0).ceil())
	_blurred.position = -pad
	_blurred.size = Vector2(_blur_view.size)


## Draws the stack again, and the bubbles out of focus once into their
## texture.
func _redraw() -> void:
	queue_redraw()
	var any_blurred := _look == Look.OPEN and _drawn.size() > 1
	_blurred.visible = any_blurred
	if any_blurred:
		_sheet.queue_redraw()
		_blur_view.render_target_update_mode = SubViewport.UPDATE_ONCE


## The room a bubble takes at `scale`, with its tail or trail (`last`:
## the bottom one).
func _measure(_entry: Dictionary, _scale: float, _is_folded: bool, _last: bool) -> Vector2:
	return Vector2.ZERO


## Draws a bubble in `rect` on `canvas`.
func _paint(_canvas: CanvasItem, _entry: Dictionary, _rect: Rect2, _scale: float, _paper: Color,
		_alpha: float, _is_folded: bool, _last: bool) -> void:
	pass


## Draws the icon left in the corner, in `rect`.
func _paint_icon(_rect: Rect2, _paper: Color, _alpha: float) -> void:
	pass


## Whether `point` is on the bubble drawn in `rect` (its corners are not).
func _on_bubble(rect: Rect2, point: Vector2) -> bool:
	return rect.has_point(point)


## The bubble of the window under `point`, from the top; -1: none. The
## icon is slot 0.
func _slot_at(point: Vector2) -> int:
	if _look == Look.ICON:
		return 0 if _rects.size() == 1 and _rects[0].has_point(point) else -1
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
			if _look == Look.ICON:
				if _slot_at(button.position) == 0:
					var now := Time.get_ticks_msec()
					thread.reopen(now)
					# The mouse is on the stack it opens.
					thread.hover(true, now)
					accept_event()
				return
			if _look == Look.FOLDED:
				return
			var place: Variant = thread.place_at(_slot_at(button.position))
			if place != null:
				place_clicked.emit(place)
				accept_event()


func _set_hover_slot(slot: int) -> void:
	if slot == _hover_slot:
		return
	_hover_slot = slot
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND if _clickable(slot) else Control.CURSOR_ARROW
	_redraw()


## Whether the bubble in `slot` does something clicked: a place, or the icon.
func _clickable(slot: int) -> bool:
	match _look:
		Look.ICON:
			return slot == 0
		Look.OPEN:
			return thread.place_at(slot) != null
	return false


func _paper(slot: int) -> Color:
	return Ink.PAPER_HOVER if slot == _hover_slot and _clickable(slot) else Ink.PAPER


## The bubble in focus, or the folded one, or the icon: sharp.
func _draw() -> void:
	if _look == Look.ICON:
		if _rects.size() == 1:
			_paint_icon(_rects[0], _paper(0), ICON_ALPHA)
		return
	if _sharp >= 0 and _sharp < _rects.size():
		_paint(self, _drawn[_sharp], _rects[_sharp], _scales[_sharp], _paper(_sharp), _alphas[_sharp],
				_look == Look.FOLDED, _sharp == _rects.size() - 1)


## The bubbles out of focus, into the texture shown blurred.
func _paint_sheet() -> void:
	if _look != Look.OPEN:
		return
	var pad := Vector2(BLUR_PAD, BLUR_PAD)
	for i in _rects.size():
		if i != _sharp:
			_paint(_sheet, _drawn[i], Rect2(_rects[i].position + pad, _rects[i].size), _scales[i],
					_paper(i), _alphas[i], false, i == _rects.size() - 1)
