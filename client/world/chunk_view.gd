class_name ChunkView
extends Node2D
## Draws one chunk of ground and the items lying on it. Its draw commands
## are retained by the renderer, so they cost nothing per frame until the
## chunk changes.

const COLORS := {
	"grass": Color(0.45, 0.62, 0.30),
	"forest": Color(0.22, 0.40, 0.20),
	"water": Color(0.25, 0.45, 0.70),
	"ford": Color(0.55, 0.60, 0.50),
	"rock": Color(0.48, 0.45, 0.42),
}
const ITEM_RADIUS := 3.5

var _origin := Vector2i.ZERO
var _size := 32
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
	for y in _size:
		for x in _size:
			var terrain: String = str(_tiles[y * _size + x])
			draw_colored_polygon(
				Iso.diamond(_origin.x + x, _origin.y + y), COLORS.get(terrain, Color.MAGENTA)
			)
	# Items: a small disc of the material's colour, slightly off the
	# tile's centre so several on one tile stay visible.
	var on_tile: Dictionary = {}
	for item in _items:
		var tile := Vector2i(int(item.get("x", 0)), int(item.get("y", 0)))
		var nth: int = on_tile.get(tile, 0)
		on_tile[tile] = nth + 1
		var centre := Iso.tile_to_screen(tile.x, tile.y) + Vector2(-6 + 6 * (nth % 3), 2 - 4 * (nth / 3))
		var material := str(item.get("material", ""))
		var colour: Color = _looks.get(material, Color.MAGENTA)
		draw_circle(centre, ITEM_RADIUS, colour)
		draw_circle(centre, ITEM_RADIUS, Color(0, 0, 0, 0.6), false, 1.0)
		if material == "corpse":
			draw_death_icon(self, centre + Vector2(0, -12))


## A small skull above a corpse, drawn on `canvas` at `at`.
static func draw_death_icon(canvas: CanvasItem, at: Vector2) -> void:
	canvas.draw_circle(at, 4.5, Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at, 4.5, Color.BLACK, false, 1.0)
	canvas.draw_rect(Rect2(at + Vector2(-2.5, 3), Vector2(5, 2.5)), Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at + Vector2(-1.7, -0.5), 1.1, Color.BLACK)
	canvas.draw_circle(at + Vector2(1.7, -0.5), 1.1, Color.BLACK)
