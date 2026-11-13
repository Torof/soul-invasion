class_name StepsTest
extends Node2D
## The slope test (`--world test:...`, the mountain test's rule, 13
## November 2026), on only in a world whose chunks carry heights above
## zero: key 1 draws the slope's grass (the ground shader), key 0 the flat
## ground, to compare; a line on screen says which, and the altitude of
## the tile the piloted body stands on (the project lead's ask, 13
## November 2026). Never in the game.

const MODES := ["0: flat (the game's ground)", "1: slopes"]

var mode := 1
var _label: Label
var _altitude: Label
var _last_tile := Vector2i(-2, -2)


func _ready() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	_label = Label.new()
	_label.position = Vector2(12, 12)
	_label.add_theme_color_override("font_color", Color.WHITE)
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	layer.add_child(_label)
	_altitude = Label.new()
	_altitude.position = Vector2(12, 40)
	_altitude.add_theme_color_override("font_color", Color.WHITE)
	_altitude.add_theme_color_override("font_outline_color", Color.BLACK)
	_altitude.add_theme_constant_override("outline_size", 6)
	_altitude.add_theme_font_size_override("font_size", 26)
	layer.add_child(_altitude)
	_apply()


func _process(_delta: float) -> void:
	var view := get_parent()
	if view == null or not view.has_method("piloted_tile"):
		return
	var tile: Vector2i = view.piloted_tile()
	if tile == _last_tile:
		return
	_last_tile = tile
	var metres := HeightIndex.level(tile) if tile.x >= 0 else -1.0
	_altitude.text = "Altitude %d m" % roundi(metres) if metres >= 0.0 else ""


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
