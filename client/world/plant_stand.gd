class_name PlantStand
extends RefCounted
## The tall plants of one chunk (from ChunkView.PLANT_STANDING_M: brambles,
## elders...), as light as its trees (Trees.Stand, D39): packed arrays, no
## node per plant. Only the plants whose image covers a cell of the screen
## the view touches (`show_cells`) have a canvas item, taken from the trees'
## pool; the renderer sorts those with the trees and the bodies by their
## foot (y-sort), ties in the server's order after the chunk's trees, as
## when each plant had its node.

## Each plant's foot on screen.
var feet := PackedVector2Array()
## Each plant's rectangle on screen (ChunkView.plant_rect): position then
## size.
var rects := PackedVector2Array()
## Each plant's region of the flora's atlas: position then size.
var regions := PackedVector2Array()
## How many of the cells shown each plant's image covers.
var refs := PackedByteArray()
## Cell of the screen (Trees.Stand.CELL_PX) -> PackedInt32Array, the plants
## whose image covers it.
var cells: Dictionary = {}
## The cells the plants cover, and those of them shown now.
var bounds := Rect2i()
var shown := Rect2i()
## Plant index -> its canvas item, for the plants shown.
var items: Dictionary = {}
## The canvas item the plants' items hang from (a y-sorted node's).
var parent := RID()


## `plants`: as a chunk brings them (kind, x, y each, in millimetres), in
## the server's order; only those that stand (ChunkView.plant_stands) and
## have an image are kept. `flora`: the kinds, by index.
func _init(plants := PackedInt32Array(), flora: Array = []) -> void:
	var lo := Vector2i(1 << 30, 1 << 30)
	var hi := -lo
	for p in range(0, plants.size(), 3):
		var k := plants[p]
		if k < 0 or k >= flora.size() or not ChunkView.plant_stands(flora[k]):
			continue
		var at := Vector2(plants[p + 1], plants[p + 2])
		var cut := ChunkView.plant_region(at, flora[k])
		if cut.size == Vector2.ZERO:
			continue
		var r := ChunkView.plant_rect(at, flora[k], cut)
		var i := feet.size()
		feet.append(Iso.point_to_screen(at))
		rects.append(r.position)
		rects.append(r.size)
		regions.append(cut.position)
		regions.append(cut.size)
		var a := Vector2i((r.position / Trees.Stand.CELL_PX).floor())
		var b := Vector2i((r.end / Trees.Stand.CELL_PX).floor())
		lo = Vector2i(mini(lo.x, a.x), mini(lo.y, a.y))
		hi = Vector2i(maxi(hi.x, b.x), maxi(hi.y, b.y))
		for cy in range(a.y, b.y + 1):
			for cx in range(a.x, b.x + 1):
				var cell := Vector2i(cx, cy)
				# Packed arrays are values: append, then store back.
				var list: PackedInt32Array = cells.get(cell, PackedInt32Array())
				list.append(i)
				cells[cell] = list
	refs.resize(feet.size())
	if hi.x >= lo.x:
		bounds = Rect2i(lo, hi - lo + Vector2i.ONE)


func size() -> int:
	return feet.size()


## Plant `i`'s rectangle on screen.
func rect(i: int) -> Rect2:
	return Rect2(rects[i * 2], rects[i * 2 + 1])


## Plant `i`'s region of the flora's atlas.
func region(i: int) -> Rect2:
	return Rect2(regions[i * 2], regions[i * 2 + 1])


## Shows the plants whose image covers a cell of `range`
## (Trees.Stand.cell_range), and only them: a canvas item for each plant
## coming in, back to the pool for each going out. Only the cells that
## changed are looked at.
func show_cells(range: Rect2i) -> void:
	var target := range.intersection(bounds) if range.has_area() and bounds.has_area() else Rect2i()
	if target == shown:
		return
	for part: Rect2i in Trees.Stand.minus(shown, target):
		_count(part, -1)
	for part: Rect2i in Trees.Stand.minus(target, shown):
		_count(part, 1)
	shown = target


func _count(part: Rect2i, by: int) -> void:
	for cy in range(part.position.y, part.end.y):
		for cx in range(part.position.x, part.end.x):
			var list: Variant = cells.get(Vector2i(cx, cy))
			if list == null:
				continue
			for i: int in list:
				var was := refs[i]
				refs[i] = was + by
				if was == 0:
					_show(i)
				elif was + by == 0:
					_hide(i)


func _show(i: int) -> void:
	var item := Trees.take_item()
	RenderingServer.canvas_item_set_parent(item, parent)
	# Sorted by its foot (its transform), ties in the server's order (its
	# index), as when each plant had a node.
	RenderingServer.canvas_item_set_draw_index(item, i)
	RenderingServer.canvas_item_set_transform(item, Transform2D(0.0, feet[i]))
	items[i] = item
	var atlas := ChunkView.flora_atlas()
	if atlas != null:
		var r := rect(i)
		RenderingServer.canvas_item_add_texture_rect_region(item, Rect2(r.position - feet[i], r.size), atlas.get_rid(), region(i))


func _hide(i: int) -> void:
	var item: RID = items[i]
	items.erase(i)
	Trees.give_back(item)


## Gives every canvas item back (the chunk is going, or its plants change).
func release() -> void:
	for i: int in items.keys():
		_hide(i)
	refs.fill(0)
	shown = Rect2i()
