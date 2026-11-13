class_name HeightIndex
extends RefCounted
## The ground's slope under each tile, for the ground shader's shading
## (the relief look test, generator version 3): one texture of TILES ×
## TILES laid over the map again and again, as GroundIndex. The heights
## come with the chunks; the slope is worked out here, once a chunk, and
## the shader reads it smoothly between the tiles.
##
## R, G: the slope east and south (rise over run), 128 for flat and
## RANGE either way at 1 and 255; A: 255 where the slope is known, so the
## shader leaves alone a tile whose neighbours have not come yet. A
## world with no relief never writes here, and the shader's relief stays
## off (ChunkView.enable_relief).
##
## Bytes, not floats: the compatibility renderer does not filter a float
## texture (it read as nothing, 13 November 2026).

const TILES := GroundIndex.TILES
## The steepest slope the texture holds, rise over run: 45°.
const RANGE := 1.0
## The slope is read between tiles this far either way (tiles): over 4 m,
## so the heights' decimetre steps do not draw contour lines.
const REACH := 2

## The heights in metres, and whether a chunk wrote each.
static var _heights := PackedFloat32Array()
static var _written := PackedByteArray()
static var _bytes := PackedByteArray()
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false
## Whether any chunk carried heights: the relief is on from the first.
static var any := false


static func texture() -> ImageTexture:
	if _texture == null:
		_heights.resize(TILES * TILES)
		_written.resize(TILES * TILES)
		_bytes.resize(TILES * TILES * 4)
		_image = Image.create_from_data(TILES, TILES, false, Image.FORMAT_RGBA8, _bytes)
		_texture = ImageTexture.create_from_image(_image)
	return _texture


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres) at `lo`, and works
## out the slope of its tiles and of the neighbours' tiles within REACH,
## which now see it. The first heights turn the ground's relief on.
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	texture()
	for y in size:
		for x in size:
			var i := _index(lo.x + x, lo.y + y)
			_heights[i] = heights[y * size + x] * 0.1
			_written[i] = 1
	for y in range(lo.y - REACH, lo.y + size + REACH):
		for x in range(lo.x - REACH, lo.x + size + REACH):
			_slope(x, y)
	if not any:
		any = true
		ChunkView.enable_relief()
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


## The slope of tile (x, y) from the heights REACH tiles either way, or
## from one side and the tile itself where the other is not known yet.
static func _slope(x: int, y: int) -> void:
	var o := 4 * _index(x, y)
	var here := _index(x, y)
	if _written[here] == 0:
		_bytes[o + 3] = 0
		return
	var g := Vector2.ZERO
	for axis in 2:
		var step := Vector2i(REACH, 0) if axis == 0 else Vector2i(0, REACH)
		var a := _index(x + step.x, y + step.y)
		var b := _index(x - step.x, y - step.y)
		var rise := 0.0
		if _written[a] and _written[b]:
			rise = (_heights[a] - _heights[b]) / (2.0 * REACH)
		elif _written[a]:
			rise = (_heights[a] - _heights[here]) / REACH
		elif _written[b]:
			rise = (_heights[here] - _heights[b]) / REACH
		g[axis] = rise
	_bytes[o] = _byte(g.x)
	_bytes[o + 1] = _byte(g.y)
	_bytes[o + 3] = 255


static func _byte(slope: float) -> int:
	return clampi(128 + roundi(slope / RANGE * 127.0), 1, 255)


static func _upload() -> void:
	_upload_due = false
	_image.set_data(TILES, TILES, false, Image.FORMAT_RGBA8, _bytes)
	_texture.update(_image)


## A tile's height in metres, whether a chunk wrote it, and its slope east
## and south as the texture holds it, for the tests.
static func at(tile: Vector2i) -> Dictionary:
	texture()
	var i := _index(tile.x, tile.y)
	return {
		"height": _heights[i],
		"written": _written[i] == 1,
		"slope": Vector2((_bytes[4 * i] - 128) / 127.0 * RANGE, (_bytes[4 * i + 1] - 128) / 127.0 * RANGE),
		"known": _bytes[4 * i + 3] == 255,
	}
