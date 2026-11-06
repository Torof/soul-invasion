class_name ChunkView
extends Node2D
## Draws one chunk of ground and the items lying on it. Its draw commands
## are retained by the renderer, so they cost nothing per frame until the
## chunk changes.
##
## The ground is drawn from top-down seamless textures projected onto the
## diamonds (docs/09-visual-production.md): a texture covers
## GROUND_METRES × GROUND_METRES metres and repeats; each tile's diamond
## shows the square of the texture it stands on. Terrains without a
## texture yet keep a flat colour.

## Metres covered by one ground texture.
const GROUND_METRES := 4.0
## terrain -> paths of its ground textures under res://art/ground/. With
## several, each 4 m square picks one by a fixed hash of its position, so
## the repetition breaks without the server knowing (decoration, D03).
const GROUND_TEXTURES := {
	"grass": [
		"res://art/ground/ground_grass_01_summer.png",
		"res://art/ground/ground_grass_02_summer.png",
	],
}
static var _ground_cache: Dictionary = {}

const COLORS := {
	"grass": Color(0.45, 0.62, 0.30),
	"forest": Color(0.22, 0.40, 0.20),
	"water": Color(0.25, 0.45, 0.70),
	"ford": Color(0.55, 0.60, 0.50),
	"rock": Color(0.48, 0.45, 0.42),
}
const ITEM_RADIUS := 4.5

var _origin := Vector2i.ZERO
var _size := 32


func _ready() -> void:
	# Seamless textures repeat across the chunk; linear filtering for the zoom.
	texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR


## The ground textures of a terrain, empty when it has none yet.
static func ground_textures(terrain: String) -> Array:
	if not _ground_cache.has(terrain):
		var loaded: Array = []
		for path in GROUND_TEXTURES.get(terrain, []):
			if ResourceLoader.exists(path):
				loaded.append(load(path))
		_ground_cache[terrain] = loaded
	return _ground_cache[terrain]


## Which of `count` variants the 4 m square holding tile (x, y) shows: a
## small fixed hash, the same on every client and every run.
static func variant_of(x: int, y: int, count: int) -> int:
	if count <= 1:
		return 0
	var bx := floori(float(x) / GROUND_METRES)
	var by := floori(float(y) / GROUND_METRES)
	var h := (bx * 73856093) ^ (by * 19349663)
	return absi(h) % count


## Texture coordinates of a tile's four corners, clockwise from the top of
## the diamond: the square of the top-down texture under that tile.
static func ground_uvs(x: int, y: int) -> PackedVector2Array:
	var s := 1.0 / GROUND_METRES
	return PackedVector2Array([
		Vector2(x, y) * s,
		Vector2(x + 1, y) * s,
		Vector2(x + 1, y + 1) * s,
		Vector2(x, y + 1) * s,
	])
var _tiles: Array = []
## [{"x", "y", "material"}] as received
var _items: Array = []
## material id -> Color
var _looks: Dictionary = {}


## Sets the chunk's tiles and items; `origin` is its north-west tile.
func show_chunk(origin: Vector2i, size: int, tiles: Array, items: Array, looks: Dictionary) -> void:
	_origin = origin
	_size = size
	_tiles = tiles
	_items = items
	_looks = looks
	queue_redraw()


func _draw() -> void:
	var white := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
	for y in _size:
		for x in _size:
			var terrain: String = str(_tiles[y * _size + x])
			var tx := _origin.x + x
			var ty := _origin.y + y
			var textures := ground_textures(terrain)
			if not textures.is_empty():
				var texture: Texture2D = textures[variant_of(tx, ty, textures.size())]
				draw_polygon(Iso.diamond(tx, ty), white, ground_uvs(tx, ty), texture)
			else:
				draw_colored_polygon(Iso.diamond(tx, ty), COLORS.get(terrain, Color.MAGENTA))
	# Items: a small disc of the material's colour, slightly off the
	# tile's centre so several on one tile stay visible.
	var on_tile: Dictionary = {}
	for item in _items:
		var tile := Vector2i(int(item.get("x", 0)), int(item.get("y", 0)))
		var nth: int = on_tile.get(tile, 0)
		on_tile[tile] = nth + 1
		var centre := Iso.tile_to_screen(tile.x, tile.y) + Vector2(-8 + 8 * (nth % 3), 3 - 5 * (nth / 3))
		var material := str(item.get("material", ""))
		var colour: Color = _looks.get(material, Color.MAGENTA)
		draw_circle(centre, ITEM_RADIUS, colour)
		draw_circle(centre, ITEM_RADIUS, Color(0, 0, 0, 0.6), false, 1.0)
		if material == "corpse":
			draw_death_icon(self, centre + Vector2(0, -16))


## A small skull above a corpse, drawn on `canvas` at `at`.
static func draw_death_icon(canvas: CanvasItem, at: Vector2) -> void:
	canvas.draw_circle(at, 4.5, Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at, 4.5, Color.BLACK, false, 1.0)
	canvas.draw_rect(Rect2(at + Vector2(-2.5, 3), Vector2(5, 2.5)), Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at + Vector2(-1.7, -0.5), 1.1, Color.BLACK)
	canvas.draw_circle(at + Vector2(1.7, -0.5), 1.1, Color.BLACK)
