class_name RecentreButton
extends Control
## The recentre button, bottom right: a small standing figure with a
## curved arrow coming back to it, on cream paper outlined in ink. Shown
## only while the camera does not follow the piloted body (after a drag);
## a click glides the camera back to the body, which it follows again
## (WorldView.recentre, as the space key).

signal recentre

## Its side, in the interface's units (those of the HUD).
const SIDE := 56.0
const MARGIN := 16.0
const CORNER := 10.0

var _hover := false


func _ready() -> void:
	set_anchors_preset(Control.PRESET_BOTTOM_RIGHT)
	offset_left = -MARGIN - SIDE
	offset_top = -MARGIN - SIDE
	offset_right = -MARGIN
	offset_bottom = -MARGIN
	tooltip_text = UiWords.RECENTRE
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	mouse_entered.connect(func() -> void:
		_hover = true
		queue_redraw())
	mouse_exited.connect(func() -> void:
		_hover = false
		queue_redraw())
	show_for(true)


## Shown only while the camera does not follow the body.
func show_for(following: bool) -> void:
	visible = not following


func _gui_input(event: InputEvent) -> void:
	var click := event as InputEventMouseButton
	if click != null and click.button_index == MOUSE_BUTTON_LEFT and click.pressed:
		recentre.emit()
		accept_event()


func _draw() -> void:
	Ink.shape(self, Ink.rounded(Rect2(Vector2.ZERO, Vector2(SIDE, SIDE)), CORNER), Ink.PAPER_HOVER if _hover else Ink.PAPER)
	var c := Vector2(SIDE, SIDE) / 2.0
	# The figure, dark as a shadow puppet: head, body, legs.
	var ink := Ink.INK
	draw_circle(c + Vector2(0, -10), 4.0, ink)
	draw_colored_polygon(PackedVector2Array([
		c + Vector2(-4.5, -5), c + Vector2(4.5, -5), c + Vector2(3.5, 5), c + Vector2(-3.5, 5)]), ink)
	draw_line(c + Vector2(-2, 4), c + Vector2(-3, 13), ink, 2.5, true)
	draw_line(c + Vector2(2, 4), c + Vector2(3, 13), ink, 2.5, true)
	# The arrow: a curve round the figure, tightening as it comes back to
	# its side, and its head.
	var curve := PackedVector2Array()
	var steps := 24
	var from := deg_to_rad(200.0)
	var to := deg_to_rad(395.0)
	for i in steps:
		var t := float(i) / float(steps - 1)
		var a := lerpf(from, to, t)
		curve.append(c + Vector2(cos(a), sin(a)) * lerpf(20.0, 13.0, t))
	draw_polyline(curve, ink, 2.5, true)
	var tip := curve[steps - 1]
	var dir := (tip - curve[steps - 3]).normalized()
	var side := Vector2(-dir.y, dir.x)
	draw_colored_polygon(PackedVector2Array([
		tip + dir * 4.5, tip - dir * 3.0 + side * 4.0, tip - dir * 3.0 - side * 4.0]), ink)
