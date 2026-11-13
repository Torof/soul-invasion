class_name HeightIndex
extends RefCounted
## The ground's level under each tile, for the ground shader's steps (the
## steps test, `--world test:marches`): one texture of TILES × TILES laid
## over the map again and again, as GroundIndex. R: the height in
## decimetres (0 to 255), G: 255 where a chunk said it. A world with no
## heights never writes here, and the steps stay off.

const TILES := GroundIndex.TILES

static var _bytes := PackedByteArray()
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false
## Whether any chunk carried heights.
static var any := false


static func texture() -> ImageTexture:
	if _texture == null:
		_bytes.resize(TILES * TILES * 2)
		_image = Image.create_from_data(TILES, TILES, false, Image.FORMAT_RG8, _bytes)
		_texture = ImageTexture.create_from_image(_image)
	return _texture


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres) at `lo`. The first
## heights turn the steps test on (StepsTest).
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	texture()
	for y in size:
		for x in size:
			var o := 2 * _index(lo.x + x, lo.y + y)
			_bytes[o] = clampi(heights[y * size + x], 0, 255)
			_bytes[o + 1] = 255
	any = true
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


static func _upload() -> void:
	_upload_due = false
	_image.set_data(TILES, TILES, false, Image.FORMAT_RG8, _bytes)
	_texture.update(_image)


## A tile's height in metres, or -1 where no chunk said it.
static func level(tile: Vector2i) -> float:
	texture()
	var o := 2 * _index(tile.x, tile.y)
	return _bytes[o] / 10.0 if _bytes[o + 1] == 255 else -1.0
