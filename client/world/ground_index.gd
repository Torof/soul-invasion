class_name GroundIndex
extends RefCounted
## What the ground shader needs to know of each tile, two bytes a tile, in
## one texture of TILES × TILES laid over the map again and again (as
## TerrainGrid): each terrain's canvas draws a few rectangles of tiles a
## chunk (its rows, GroundBuild.rows_of) and the shader keeps, pixel by
## pixel, the tiles that terrain covers (D39).
## Before, each canvas was a mesh of six vertices a tile, built in
## GDScript and held by the graphics card.
##
## R: which canvases draw the tile, one bit each (BITS), and FRINGE when
## the forest's canvas spills over it with a soft edge.
## G: how near the forest is at the tile's top corner (its north-west
## corner on the map), 0 to 255: the spill's opacity at that corner,
## which the shader spreads over the diamond's two triangles exactly as
## a mesh's vertex colours were.

const TILES := 640
## The bit of each textured terrain's canvas.
const BITS := {"grass": 1, "forest": 2, "water": 4}
## The forest's spill over another terrain (its opacity in G).
const FRINGE := 128

static var _bytes := PackedByteArray()
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false


static func texture() -> ImageTexture:
	if _texture == null:
		_bytes.resize(TILES * TILES * 2)
		_image = Image.create_from_data(TILES, TILES, false, Image.FORMAT_RG8, _bytes)
		_texture = ImageTexture.create_from_image(_image)
	return _texture


## Lays a chunk's tiles: `bits` (size × size, R) at `lo`, and `corners`
## ((size + 1) × (size + 1), G) at the corners from `lo`, those of the
## far edges landing on the next chunk's first tiles, which compute the
## same values.
static func write(lo: Vector2i, size: int, bits: PackedByteArray, corners: PackedByteArray) -> void:
	texture()
	var c := size + 1
	for y in c:
		var row := posmod(lo.y + y, TILES) * TILES
		for x in c:
			var o := 2 * (row + posmod(lo.x + x, TILES))
			if x < size and y < size:
				_bytes[o] = bits[y * size + x]
			_bytes[o + 1] = corners[y * c + x]
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


static func _upload() -> void:
	_upload_due = false
	_image.set_data(TILES, TILES, false, Image.FORMAT_RG8, _bytes)
	_texture.update(_image)


## The two bytes of a tile, for the tests: [bits, corner].
static func at(tile: Vector2i) -> PackedByteArray:
	texture()
	var o := 2 * (posmod(tile.y, TILES) * TILES + posmod(tile.x, TILES))
	return PackedByteArray([_bytes[o], _bytes[o + 1]])
