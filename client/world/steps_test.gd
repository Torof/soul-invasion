class_name StepsTest
extends Node2D
## The steps test (`--world test:marches`, 13 November 2026), on only in a
## world whose chunks carry heights: keys 1 to 4 and 0 choose what the
## ground shows of the levels, a line on screen says which, and the stones
## of mode C lie at the foot of the 2 m drops. Never in the game.

const MODES := ["0: flat", "1: A (steps)", "2: A + B (light lip)", "3: A + C (stones)", "4: A + B + C"]
## Stones a tile of edge, at most, and their size in metres.
const STONES_PER_TILE := 2
const STONE_M := Vector2(0.3, 0.6)

var mode := 1
var _label: Label
var _stones: Node2D
## The tiles whose heights came, to find the 2 m drops among.
var _tiles: Dictionary = {}


func _ready() -> void:
	z_index = -1
	_stones = Node2D.new()
	add_child(_stones)
	var layer := CanvasLayer.new()
	add_child(layer)
	_label = Label.new()
	_label.position = Vector2(12, 12)
	_label.add_theme_color_override("font_color", Color.WHITE)
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	layer.add_child(_label)
	_apply()


## A chunk's heights came: its tiles join the search for drops.
func chunk(lo: Vector2i, size: int) -> void:
	for y in size:
		for x in size:
			_tiles[lo + Vector2i(x, y)] = true
	_place_stones()


func _unhandled_key_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	var chosen := key.keycode - KEY_0
	if chosen >= 0 and chosen < MODES.size():
		mode = chosen
		_apply()
		get_viewport().set_input_as_handled()


func _apply() -> void:
	ChunkView.set_steps_mode(mode)
	_stones.visible = mode == 3 or mode == 4
	_label.text = "Steps test — %s   (keys 1-4, 0)" % MODES[mode]


## A few existing pebbles at the foot of every drop of 2 m or more turned
## to the camera, by a fixed hash of the tile.
func _place_stones() -> void:
	for child in _stones.get_children():
		child.queue_free()
	var textures := ChunkView.decor_textures("pebble")
	if textures.is_empty():
		return
	for tile: Vector2i in _tiles:
		var upper := HeightIndex.level(tile)
		# East face: the lower tile at x + 1; south face: at y + 1.
		for east in [true, false]:
			var lower_tile: Vector2i = tile + (Vector2i(1, 0) if east else Vector2i(0, 1))
			var lower := HeightIndex.level(lower_tile)
			if lower < 0.0 or upper - lower < 1.95:
				continue
			for k in STONES_PER_TILE:
				var h := ChunkView.tile_hash(tile.x, tile.y, 7 + k + (10 if east else 0))
				if h < 0.25:
					continue
				var t := ChunkView.tile_hash(tile.x, tile.y, 31 + k + (10 if east else 0))
				var edge := Vector2(tile.x + 1, tile.y + t) if east else Vector2(tile.x + t, tile.y + 1)
				var foot := Iso.point_to_screen(edge * 1000.0) + Vector2(0, (upper - lower) * Iso.PX_PER_M + 4.0 + h * 8.0)
				var texture: Texture2D = textures[int(h * 997.0) % textures.size()]
				var sprite := Sprite2D.new()
				sprite.texture = texture
				var width := lerpf(STONE_M.x, STONE_M.y, t) * Iso.PX_PER_M
				sprite.scale = Vector2.ONE * width / texture.get_width()
				sprite.position = foot
				_stones.add_child(sprite)
