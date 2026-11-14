class_name HeightIndex
extends RefCounted
## The ground's height under each tile, as the chunks say it (protocol
## v25, D43): one height a tile in decimetres above the sea, kept in a
## ring of TILES × TILES laid over the map again and again, as
## GroundIndex and TerrainGrid are. A chunk far enough away lands on the
## same cells as one in view and overwrites it, which is the ring's
## point: only what is around the body is kept.
## From the heights, the slopes (D44): a texture of TILES × TILES for the
## ground shader, R and G (half floats) the ground's rise along the map's
## x and y, in metres a tile, read over SPAN tiles on either side (a slope
## is read over about two tiles, not tile to tile: the project lead's
## eye, 13 November 2026). Where a neighbour is not known yet, the
## nearest known tile toward it stands in, and the tile itself at worst.
## Only the test worlds (`--world test:*`) send heights today; the
## valley and the game's worlds send none and never write here, and
## their slopes stay off.

const TILES := GroundIndex.TILES
## How far on either side the slope is read, in tiles.
const SPAN := 2

## The heights, in decimetres, and whether a chunk said them.
static var _heights := PackedInt32Array()
static var _known := PackedByteArray()
## The slope texture's bytes (two half floats a tile), its image and the
## texture the ground shader reads; the upload once a frame at most.
static var _bytes := PackedByteArray()
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false
## Whether any chunk carried heights.
static var any := false


static func _ready_store() -> void:
	if _heights.is_empty():
		_heights.resize(TILES * TILES)
		_known.resize(TILES * TILES)
		_bytes.resize(TILES * TILES * 4)


## The slope texture (R, G: the rise along x and y in metres a tile), the
## same object for the life of the client, updated in place.
static func texture() -> ImageTexture:
	_ready_store()
	if _texture == null:
		_image = Image.create_from_data(TILES, TILES, false, Image.FORMAT_RGH, _bytes)
		_texture = ImageTexture.create_from_image(_image)
	return _texture


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres, row by row from
## its north-west corner) at `lo`, the chunk's first tile, and works out
## again the slopes of its tiles and of the tiles within SPAN of it.
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	_ready_store()
	for y in size:
		for x in size:
			var i := _index(lo.x + x, lo.y + y)
			_heights[i] = heights[y * size + x]
			_known[i] = 1
	for y in range(-SPAN, size + SPAN):
		for x in range(-SPAN, size + SPAN):
			var tile := Vector2i(lo.x + x, lo.y + y)
			var i := _index(tile.x, tile.y)
			if _known[i] == 0:
				continue
			_bytes.encode_half(4 * i, rise(tile, Vector2i(1, 0)))
			_bytes.encode_half(4 * i + 2, rise(tile, Vector2i(0, 1)))
	any = true
	if _texture != null and not _upload_due:
		_upload_due = true
		_upload.call_deferred()


## The rise at `tile` along `axis` (a unit step along x or y), in metres
## a tile: the difference of the farthest known tiles within SPAN on
## either side, over their distance, the tile itself standing in for a
## side with no known tile; 0 where the tile itself is unknown, as its
## texel is.
static func rise(tile: Vector2i, axis: Vector2i) -> float:
	_ready_store()
	if _known[_index(tile.x, tile.y)] == 0:
		return 0.0
	var ahead := 0
	for k in range(SPAN, 0, -1):
		if _known[_index(tile.x + k * axis.x, tile.y + k * axis.y)] != 0:
			ahead = k
			break
	var behind := 0
	for k in range(SPAN, 0, -1):
		if _known[_index(tile.x - k * axis.x, tile.y - k * axis.y)] != 0:
			behind = k
			break
	if ahead + behind == 0:
		return 0.0
	var high := _heights[_index(tile.x + ahead * axis.x, tile.y + ahead * axis.y)]
	var low := _heights[_index(tile.x - behind * axis.x, tile.y - behind * axis.y)]
	return (high - low) / 10.0 / (ahead + behind)


static func _upload() -> void:
	_upload_due = false
	if _texture == null:
		return
	_image.set_data(TILES, TILES, false, Image.FORMAT_RGH, _bytes)
	_texture.update(_image)


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


## Forgets every height and every slope (the tests); the texture, if
## made, is emptied in place.
static func clear() -> void:
	_heights = PackedInt32Array()
	_known = PackedByteArray()
	_bytes = PackedByteArray()
	any = false
	if _texture != null:
		_ready_store()
		_upload()
