class_name ChunkView
extends Node2D
## Draws one chunk of ground. Its draw commands are retained by the
## renderer, so the tiles cost nothing per frame until the chunk changes.

const COLORS := {
	"grass": Color(0.45, 0.62, 0.30),
	"forest": Color(0.22, 0.40, 0.20),
	"water": Color(0.25, 0.45, 0.70),
	"ford": Color(0.55, 0.60, 0.50),
	"rock": Color(0.48, 0.45, 0.42),
}

var _origin := Vector2i.ZERO
var _size := 32
var _tiles: Array = []


## Sets the chunk's tiles; `origin` is its north-west tile.
func show_tiles(origin: Vector2i, size: int, tiles: Array) -> void:
	_origin = origin
	_size = size
	_tiles = tiles
	queue_redraw()


func _draw() -> void:
	for y in _size:
		for x in _size:
			var terrain: String = str(_tiles[y * _size + x])
			draw_colored_polygon(
				Iso.diamond(_origin.x + x, _origin.y + y), COLORS.get(terrain, Color.MAGENTA)
			)
