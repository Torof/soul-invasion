class_name HeightIndex
extends RefCounted
## The ground's height under each tile, as the chunks say it (protocol
## v25, D43): one height a tile in decimetres above the sea, kept in a
## ring of TILES × TILES laid over the map again and again, as
## GroundIndex and TerrainGrid are. A chunk far enough away lands on the
## same cells as one in view and overwrites it, which is the ring's
## point: only what is around the body is kept.
## Only the test worlds (`--world test:*`) send heights today; the
## valley and the game's worlds send none and never write here. Nothing
## reads the heights yet: the slopes drawn from them come next.

const TILES := GroundIndex.TILES

## The heights, in decimetres, and whether a chunk said them.
static var _heights := PackedInt32Array()
static var _known := PackedByteArray()
## Whether any chunk carried heights.
static var any := false


static func _ready_store() -> void:
	if _heights.is_empty():
		_heights.resize(TILES * TILES)
		_known.resize(TILES * TILES)


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres, row by row from
## its north-west corner) at `lo`, the chunk's first tile.
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	_ready_store()
	for y in size:
		for x in size:
			var i := _index(lo.x + x, lo.y + y)
			_heights[i] = heights[y * size + x]
			_known[i] = 1
	any = true


## Whether a chunk said the height of `tile`.
static func known(tile: Vector2i) -> bool:
	_ready_store()
	return _known[_index(tile.x, tile.y)] != 0


## A tile's height in decimetres above the sea, 0 where no chunk said it
## (`known` tells the two apart).
static func height_dm(tile: Vector2i) -> int:
	_ready_store()
	return _heights[_index(tile.x, tile.y)]


## A tile's height in metres, 0.0 where no chunk said it.
static func level(tile: Vector2i) -> float:
	return height_dm(tile) / 10.0


## Forgets every height (the tests).
static func clear() -> void:
	_heights = PackedInt32Array()
	_known = PackedByteArray()
	any = false
