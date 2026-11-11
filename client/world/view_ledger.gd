class_name ViewLedger
extends RefCounted
## The world view's bookkeeping of the chunks held, kept up to date as
## chunks come and go rather than worked out again every frame (lightness
## audit, 10 November 2026): the bounds of the chunks held; which chunks
## are built and which are to be, from the camera's reach (C1, 11
## November 2026); the chunks whose redraw waits for the camera to come
## near; what lies behind the mouse among the items. No node and no
## network in here: tested bare.

var chunk_size := 32

## The bounds, in chunks, of the chunks held: the view's limit reads them
## every frame, and finding them again over every key cost 0.13 ms a
## frame and 93 ms a teleport. Grown as chunks come; found again only
## when a chunk at their edge went.
var held := Rect2i()
var held_stale := false

## Chunks whose borders must be drawn again because a neighbour came or
## went, but which lie past the camera's reach: view -> its chunk.
## Redrawn when the camera comes within a chunk of them, the same build
## then as it would have been at once. In following play the chunks that
## come and go lie 256 tiles away, under the fog, and the camera never
## reaches them: a border crossing redraws nothing.
var stale: Dictionary = {}

## What the camera can reach, in the world's pixels, from the inside out:
## the screen itself (`seen`); the screen grown by what may stand out of a
## chunk's ground, up for the tallest trees, sideways and down for their
## crowns (`shown`: a chunk touching it may show something on screen);
## and a chunk more around (`reach`).
var seen := Rect2()
var shown := Rect2()
var reach := Rect2()
const STANDING_REACH_UP := 1024.0
const STANDING_REACH := 256.0

## A chunk held is data until the camera's reach touches it; then it is
## built, the chunks the screen may show at once, the chunk of margin
## around them paced one a frame, nearest the camera first. A built chunk
## the reach left is released after RELEASE_GRACE_MSEC, its data kept,
## so it builds again from them with no network (C1); the grace and the
## margin are the hysteresis (C5): the camera jittering on an edge, or
## the body walking back and forth over one, builds nothing twice.
## chunk -> true for the chunks built; chunk -> the moment (msec) the
## reach left it for the built chunks out of reach.
var built: Dictionary = {}
var leaving: Dictionary = {}
const RELEASE_GRACE_MSEC := 2000

## The depth of the nearest item near the mouse (items_depth_near), and
## the mouse and the items it was found for.
var item_depth := INF
var _item_depth_mouse := Vector2.INF
var _item_depth_version := -1


## Chunk `key` is held: the bounds grow to it.
func hold(key: Vector2i) -> void:
	var one := Rect2i(key, Vector2i.ONE)
	held = one if not held.has_area() else held.merge(one)


## Chunk `key` went: the bounds are found again if it lay on their edge.
func drop(key: Vector2i) -> void:
	if key.x == held.position.x or key.y == held.position.y or key.x == held.end.x - 1 or key.y == held.end.y - 1:
		held_stale = true


## The bounds of the chunks held, `chunks` (chunk -> anything) being
## looked through only when a chunk at the edge went.
func bounds(chunks: Dictionary) -> Rect2i:
	if held_stale:
		held = Iso.chunk_bounds(chunks.keys())
		held_stale = false
	return held


## The rectangle around chunk `key`'s diamond of ground on screen.
func chunk_rect(key: Vector2i) -> Rect2:
	var lo := key * chunk_size
	var hi := lo + Vector2i(chunk_size, chunk_size)
	return Rect2(
		Vector2((lo.x - hi.y) * Iso.TILE_W / 2.0, (lo.x + lo.y) * Iso.TILE_H / 2.0),
		Vector2((hi.x - lo.x + hi.y - lo.y) * Iso.TILE_W / 2.0, (hi.x + hi.y - lo.x - lo.y) * Iso.TILE_H / 2.0))


## The screen shows `screen` (the world's pixels): the reach follows it, a
## chunk around. Whether it moved.
func see(screen: Rect2) -> bool:
	if screen == seen:
		return false
	seen = screen
	shown = screen.grow_individual(STANDING_REACH, STANDING_REACH_UP, STANDING_REACH, STANDING_REACH)
	reach = screen.grow_individual(chunk_size * Iso.TILE_W, chunk_size * Iso.TILE_H, chunk_size * Iso.TILE_W, chunk_size * Iso.TILE_H)
	return true


## Whether chunk `key`'s diamond of ground touches `rect` (the world's
## pixels), exactly: in tile space the rect is a band in x - y and in
## x + y, the chunk a square (its tiles' diamonds start half a tile before
## their centre), and two convex shapes meet unless an edge's normal
## parts them.
func touches(rect: Rect2, key: Vector2i) -> bool:
	if not rect.has_area():
		return false
	var u0 := rect.position.x / (Iso.TILE_W / 2.0)
	var u1 := rect.end.x / (Iso.TILE_W / 2.0)
	var v0 := rect.position.y / (Iso.TILE_H / 2.0)
	var v1 := rect.end.y / (Iso.TILE_H / 2.0)
	var x0 := key.x * chunk_size - 0.5
	var y0 := key.y * chunk_size - 0.5
	var x1 := x0 + chunk_size
	var y1 := y0 + chunk_size
	return x0 - y1 < u1 and x1 - y0 > u0 and x0 + y0 < v1 and x1 + y1 > v0 \
		and x0 < (u1 + v1) / 2.0 and x1 > (u0 + v0) / 2.0 \
		and y0 < (v1 - u0) / 2.0 and y1 > (v0 - u1) / 2.0


## The chunks of `bounds` (chunks) whose ground touches `rect` (the
## world's pixels): exactly those `touches` says, found row by row
## without looking at each chunk of the bounds.
func chunks_touching(rect: Rect2, bounds: Rect2i) -> Array[Vector2i]:
	var keys: Array[Vector2i] = []
	if not rect.has_area() or not bounds.has_area():
		return keys
	var u0 := rect.position.x / (Iso.TILE_W / 2.0)
	var u1 := rect.end.x / (Iso.TILE_W / 2.0)
	var v0 := rect.position.y / (Iso.TILE_H / 2.0)
	var v1 := rect.end.y / (Iso.TILE_H / 2.0)
	var cs := float(chunk_size)
	# The rows whose square of tiles crosses the band's span in y.
	var cy_lo := maxi(floori(((v0 - u1) / 2.0 + 0.5) / cs), bounds.position.y)
	var cy_hi := mini(ceili(((v1 - u0) / 2.0 + 0.5) / cs) - 1, bounds.end.y - 1)
	for cy in range(cy_lo, cy_hi + 1):
		var y0 := cy * cs - 0.5
		var y1 := y0 + cs
		if not (y0 < (v1 - u0) / 2.0 and y1 > (v0 - u1) / 2.0):
			continue
		# In this row, the squares' x0 must lie in (lo - cs, hi).
		var hi := minf(minf(u1 + y1, v1 - y0), (u1 + v1) / 2.0)
		var lo := maxf(maxf(u0 + y0, v0 - y1), (u0 + v0) / 2.0)
		var cx_lo := maxi(floori((lo - cs + 0.5) / cs) + 1, bounds.position.x)
		var cx_hi := mini(ceili((hi + 0.5) / cs) - 1, bounds.end.x - 1)
		for cx in range(cx_lo, cx_hi + 1):
			keys.append(Vector2i(cx, cy))
	return keys


## Whether the camera's reach touches chunk `key`.
func within_reach(key: Vector2i) -> bool:
	return touches(reach, key)


## What to do with the chunks now the reach or the chunks held moved:
## the chunks held in reach and not built, those the screen may show
## (`at_once`) and the margin's (`paced`); the built chunks the reach
## left start their grace, those it came back to keep theirs cancelled,
## and those whose grace is over are to be released (`release`). `held`:
## chunk -> anything, the chunks held.
func plan(now_msec: int, held_chunks: Dictionary) -> Dictionary:
	var at_once: Array[Vector2i] = []
	var paced: Array[Vector2i] = []
	var in_reach := {}
	for key: Vector2i in chunks_touching(reach, bounds(held_chunks)):
		if not held_chunks.has(key):
			continue
		in_reach[key] = true
		if built.has(key):
			leaving.erase(key)
		elif touches(shown, key):
			at_once.append(key)
		else:
			paced.append(key)
	for key: Vector2i in built:
		if not in_reach.has(key) and not leaving.has(key):
			leaving[key] = now_msec
	var release: Array[Vector2i] = []
	for key: Vector2i in leaving:
		if now_msec - int(leaving[key]) >= RELEASE_GRACE_MSEC:
			release.append(key)
	return {"at_once": at_once, "paced": paced, "release": release}


## Chunk `key` is built now.
func mark_built(key: Vector2i) -> void:
	built[key] = true
	leaving.erase(key)


## Chunk `key`'s nodes are gone.
func mark_released(key: Vector2i) -> void:
	built.erase(key)
	leaving.erase(key)


## Chunk `key`, drawn by `view`, must be redrawn when the camera reaches it.
func mark_stale(view: Object, key: Vector2i) -> void:
	stale[view] = key


## A view gone is forgotten.
func forget(view: Object) -> void:
	stale.erase(view)


## The stale views the camera reaches now, no longer stale.
func wake() -> Array:
	var woken: Array = []
	for view: Variant in stale.keys():
		if not is_instance_valid(view):
			stale.erase(view)
		elif within_reach(stale[view]):
			stale.erase(view)
			woken.append(view)
	return woken


## The smallest depth (x + y, millimetres) of an item within `reach_px`
## of `mouse` on screen, among `items` (chunk -> the items received) in
## the chunks around the mouse; INF when there is none. Looked for again
## only when the mouse or `version` (counted up when the items held
## change) changed.
func items_depth_near(items: Dictionary, version: int, mouse: Vector2, reach_px: float) -> float:
	if mouse == _item_depth_mouse and version == _item_depth_version:
		return item_depth
	_item_depth_mouse = mouse
	_item_depth_version = version
	item_depth = INF
	var c := Iso.chunk_of(Iso.screen_to_tile(mouse), chunk_size)
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			for item: Dictionary in items.get(c + Vector2i(dx, dy), []):
				var p := Vector2(int(item.get("x", 0)) * 1000 + 500, int(item.get("y", 0)) * 1000 + 500)
				if Iso.point_to_screen(p).distance_to(mouse) < reach_px:
					item_depth = minf(item_depth, p.x + p.y)
	return item_depth
