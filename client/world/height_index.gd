class_name HeightIndex
extends RefCounted
## The ground's slope under each tile, for the ground shader's slope grass
## (the test worlds, `--world test:...`): one texture of TILES × TILES laid
## over the map again and again, as GroundIndex. R and G (half floats):
## the ground's rise along the map's x and y, in metres a tile, read over
## SPAN tiles on either side (the mountain test's rule, 13 November 2026:
## a slope is read over about two tiles, not tile to tile). Where a
## neighbour is not known yet, the nearest known tile toward it stands in,
## and the tile itself at worst. A world with no heights never writes
## here, and the slopes stay off.

const TILES := GroundIndex.TILES
## How far on either side the slope is read, in tiles.
const SPAN := 2

## The heights, in metres, and whether a chunk said them.
static var _heights := PackedFloat32Array()
static var _known := PackedByteArray()
static var _bytes := PackedByteArray()
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false
## Whether any chunk carried heights.
static var any := false


static func texture() -> ImageTexture:
	if _texture == null:
		_heights.resize(TILES * TILES)
		_known.resize(TILES * TILES)
		_bytes.resize(TILES * TILES * 4)
		_image = Image.create_from_data(TILES, TILES, false, Image.FORMAT_RGH, _bytes)
		_texture = ImageTexture.create_from_image(_image)
	return _texture


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres) at `lo`, and works
## out again the slopes of its tiles and of the tiles within SPAN of it.
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	texture()
	for y in size:
		for x in size:
			var i := _index(lo.x + x, lo.y + y)
			_heights[i] = heights[y * size + x] / 10.0
			_known[i] = 1
	for y in range(-SPAN, size + SPAN):
		for x in range(-SPAN, size + SPAN):
			var tile := Vector2i(lo.x + x, lo.y + y)
			var i := _index(tile.x, tile.y)
			if _known[i] == 0:
				continue
			_bytes.encode_half(4 * i, _rise(tile, Vector2i(1, 0)))
			_bytes.encode_half(4 * i + 2, _rise(tile, Vector2i(0, 1)))
	any = true
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


## The rise at `tile` along `axis`, in metres a tile: the difference of
## the farthest known tiles within SPAN on either side.
static func _rise(tile: Vector2i, axis: Vector2i) -> float:
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
	return (high - low) / (ahead + behind)


static func _upload() -> void:
	_upload_due = false
	_image.set_data(TILES, TILES, false, Image.FORMAT_RGH, _bytes)
	_texture.update(_image)


## A tile's height in metres, or -1 where no chunk said it.
static func level(tile: Vector2i) -> float:
	texture()
	var i := _index(tile.x, tile.y)
	return _heights[i] if _known[i] != 0 else -1.0
