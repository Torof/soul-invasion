class_name StepsTest
extends Node2D
## The slope test (`--world test:...`, the mountain test's rule, 13
## November 2026), on only in a world whose chunks carry heights above
## zero: key 1 draws the slope's grass (the ground shader), key 0 the flat
## ground, to compare; a line on screen says which. Never in the game.

const MODES := ["0: flat (the game's ground)", "1: slopes"]

var mode := 1
var _label: Label


func _ready() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	_label = Label.new()
	_label.position = Vector2(12, 12)
	_label.add_theme_color_override("font_color", Color.WHITE)
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	layer.add_child(_label)
	_apply()


func _unhandled_key_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	var chosen := key.keycode - KEY_0
	if chosen < 0 or chosen >= MODES.size():
		return
	mode = chosen
	_apply()
	get_viewport().set_input_as_handled()


func _apply() -> void:
	ChunkView.set_steps_mode(mode)
	_label.text = "Slope test — %s   (keys 1 and 0)" % MODES[mode]
