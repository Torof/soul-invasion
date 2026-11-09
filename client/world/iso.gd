class_name Iso
extends RefCounted
## Isometric projection (D02): tile coordinates <-> screen pixels.
##
## A tile is a diamond TILE_W wide and TILE_H high: one metre is 64 px at
## scale 1, horizontally and vertically (the format contract of
## docs/09-visual-production.md). Tile (0, 0) is drawn with its centre at
## the origin; x grows to the south-east, y to the south-west.

const TILE_W := 64.0
const TILE_H := 32.0
## Pixels per metre of height at scale 1, for things standing on a tile.
const PX_PER_M := 64.0


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


## The bounds, in chunks, of `keys` less those in `skip`; empty if none.
static func chunk_bounds(keys: Array, skip: Dictionary) -> Rect2i:
	var lo := Vector2i(1 << 30, 1 << 30)
	var hi := Vector2i(-(1 << 30), -(1 << 30))
	for key: Vector2i in keys:
		if skip.has(key):
			continue
		lo = Vector2i(mini(lo.x, key.x), mini(lo.y, key.y))
		hi = Vector2i(maxi(hi.x, key.x + 1), maxi(hi.y, key.y + 1))
	if lo.x > hi.x:
		return Rect2i()
	return Rect2i(lo, hi - lo)


## Where the camera's centre may stand, in tiles, for a screen `view`
## pixels wide and high at zoom 1, over the ground from `lo` to `hi`
## (tiles): the middle of each edge of the screen stays within `band`
## tiles past it, and `corners` (0 to 1) of the way further, towards the
## screen's corners (1: no corner ever goes past the band). Beyond the
## band the corners show fog. A negative size: the screen is wider than
## all that.
static func camera_area(lo: Vector2, hi: Vector2, view: Vector2, band: float, corners := 0.0) -> Rect2:
	# Along either axis of the tiles, the middle of the farthest edge lies
	# max(w / TILE_W, h / TILE_H) / 2 tiles from the centre, a corner
	# (w / TILE_W + h / TILE_H) / 2.
	var middle := maxf(view.x / TILE_W, view.y / TILE_H) / 2.0
	var corner := (view.x / TILE_W + view.y / TILE_H) / 2.0
	var reach := lerpf(middle, corner, corners)
	var a := lo - Vector2(band, band) + Vector2(reach, reach)
	var b := hi + Vector2(band, band) - Vector2(reach, reach)
	return Rect2(a, b - a)
