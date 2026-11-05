class_name Iso
extends RefCounted
## Isometric projection (D02): tile coordinates <-> screen pixels.
##
## A tile is a diamond TILE_W wide and TILE_H high. Tile (0, 0) is drawn
## with its centre at the origin; x grows to the south-east, y to the
## south-west.

const TILE_W := 48.0
const TILE_H := 24.0


## Screen position of the centre of tile (x, y).
static func tile_to_screen(x: int, y: int) -> Vector2:
	return Vector2((x - y) * TILE_W / 2.0, (x + y) * TILE_H / 2.0)


## Screen position of a fine point given in millimetres (1000 per tile);
## the centre of tile (x, y) is at ((x + 0.5) * 1000, (y + 0.5) * 1000).
static func point_to_screen(mm: Vector2) -> Vector2:
	var x := mm.x / 1000.0 - 0.5
	var y := mm.y / 1000.0 - 0.5
	return Vector2((x - y) * TILE_W / 2.0, (x + y) * TILE_H / 2.0)


## Tile under a screen position (inverse of tile_to_screen).
static func screen_to_tile(p: Vector2) -> Vector2i:
	var fx := p.x / (TILE_W / 2.0)
	var fy := p.y / (TILE_H / 2.0)
	return Vector2i(floori((fx + fy) / 2.0 + 0.5), floori((fy - fx) / 2.0 + 0.5))


## The chunk holding a tile, for any sign of coordinates. Must agree with
## `Pos::chunk` on the server: floor division, chunk (0, 0) holding tiles
## 0..size.
static func chunk_of(tile: Vector2i, size: int) -> Vector2i:
	return Vector2i(floori(float(tile.x) / size), floori(float(tile.y) / size))


## The four corners of the diamond for tile (x, y), clockwise from the top.
static func diamond(x: int, y: int) -> PackedVector2Array:
	var c := tile_to_screen(x, y)
	return PackedVector2Array([
		c + Vector2(0, -TILE_H / 2.0),
		c + Vector2(TILE_W / 2.0, 0),
		c + Vector2(0, TILE_H / 2.0),
		c + Vector2(-TILE_W / 2.0, 0),
	])
