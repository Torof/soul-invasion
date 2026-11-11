class_name ViewLedger
extends RefCounted
## The world view's bookkeeping of the chunks held, kept up to date as
## chunks come and go rather than worked out again every frame (lightness
## audit, 10 November 2026): the bounds of the chunks held; the chunks
## whose redraw waits for the camera to come near; the chunks parked
## when hidden, to come back as they were; what lies behind the mouse
## among the items. No node and no network in here: tested bare.

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
## What the camera can reach, in the world's pixels: the screen and a
## chunk around it; and the screen it was last worked out for.
var reach := Rect2()
var reach_seen := Rect2()

## A chunk hidden is parked rather than freed while it stays within
## PARK_RING chunks of the body's chunk (one past the 19 × 19 held) and
## while at most PARK_MOST are parked: walking back over a border brings
## it back as it was, nothing built. chunk -> what the world view keeps
## of it, oldest first.
const PARK_RING := 10
const PARK_MOST := 96
var parked: Dictionary = {}

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


## The screen shows `seen` (the world's pixels): the reach follows it, a
## chunk around. Whether it moved.
func see(seen: Rect2) -> bool:
	if seen == reach_seen:
		return false
	reach_seen = seen
	reach = seen.grow_individual(chunk_size * Iso.TILE_W, chunk_size * Iso.TILE_H, chunk_size * Iso.TILE_W, chunk_size * Iso.TILE_H)
	return true


func within_reach(key: Vector2i) -> bool:
	return reach.intersects(chunk_rect(key))


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


## Keeps `record` for chunk `key`, hidden.
func park(key: Vector2i, record: Dictionary) -> void:
	parked[key] = record


## The record parked for `key`, no longer parked; null when none.
func take(key: Vector2i) -> Variant:
	if not parked.has(key):
		return null
	var record: Dictionary = parked[key]
	parked.erase(key)
	return record


## The records to free: the chunks past PARK_RING from `centre` (the
## body's chunk; null when unknown), then the oldest past PARK_MOST.
func prune(centre: Variant) -> Array:
	var freed: Array = []
	if centre != null:
		var c: Vector2i = centre
		for key: Vector2i in parked.keys():
			if maxi(absi(key.x - c.x), absi(key.y - c.y)) > PARK_RING:
				freed.append(parked[key])
				parked.erase(key)
	while parked.size() > PARK_MOST:
		var oldest: Vector2i = parked.keys()[0]
		freed.append(parked[oldest])
		parked.erase(oldest)
	return freed


## Every record parked, none left.
func take_all() -> Array:
	var all := parked.values()
	parked.clear()
	return all


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
