class_name Trees
extends RefCounted
## The trees standing on the ground (D37): the server says where each
## stands and of what kind; this draws it, an image of its kind standing
## on the point where its trunk meets the ground. The images are 256 px
## per metre (four times the screen, sharp up to zoom 4), their bottom
## edge at the trunk's foot, centred on it.

const DIR := "res://art/trees/"
## How much a tree's size varies from one to the next, either way.
const SIZE_JITTER := 0.12

## species id -> Array[Texture2D], its variants.
static var _textures: Dictionary = {}
## A soft round shade, dark in the middle, for the shadows on the ground.
static var _shade: Texture2D
## Opacity of the shade right under the trunk, and of the crown's.
const FOOT_SHADE := 0.45
const CROWN_SHADE := 0.18


static func shade() -> Texture2D:
	if _shade == null:
		var g := Gradient.new()
		g.set_color(0, Color(0, 0, 0, 1))
		g.set_color(1, Color(0, 0, 0, 0))
		g.add_point(0.55, Color(0, 0, 0, 0.45))
		var t := GradientTexture2D.new()
		t.gradient = g
		t.fill = GradientTexture2D.FILL_RADIAL
		t.fill_from = Vector2(0.5, 0.5)
		t.fill_to = Vector2(1.0, 0.5)
		t.width = 128
		t.height = 128
		_shade = t
	return _shade


## Draws on `canvas` the shadows of a tree whose image covers `r` and
## whose foot stands at `foot` (both on the canvas): a wide faint one
## under its crown and a darker one hugging its foot, so it stands in the
## grass rather than on it. Drawn before trees and bodies.
static func draw_shadow(canvas: CanvasItem, r: Rect2, foot: Vector2) -> void:
	var crown := Vector2(r.size.x * 0.85, r.size.x * 0.42)
	canvas.draw_texture_rect(shade(), Rect2(foot - crown / 2.0 + Vector2(r.size.x * 0.06, 0), crown), false, Color(1, 1, 1, CROWN_SHADE))
	var base := Vector2(Iso.PX_PER_M * 0.9, Iso.PX_PER_M * 0.45) * (r.size.y / (4.0 * Iso.PX_PER_M)) ** 0.5
	canvas.draw_texture_rect(shade(), Rect2(foot - base / 2.0, base), false, Color(1, 1, 1, FOOT_SHADE))


## How much sharper than the hardware's pick the trees' images are read
## (trees.gdshader's mip_bias) at `zoom`: a level sharper up close (zoom 1
## and nearer), easing back to the hardware's own pick at zoom 0.5 and
## farther, where the sharper level costs four times the reading and the
## eye cannot see it.
static func sharpness(zoom: float) -> float:
	return -clampf((zoom - 0.5) / 0.5, 0.0, 1.0)


## The images of a kind of tree: `<id>_adult_01_summer.png`, `_02`, ...
static func textures(species: String) -> Array:
	if not _textures.has(species):
		var found: Array = []
		var n := 1
		while true:
			var path := DIR + "%s_adult_%02d_summer.png" % [species, n]
			if not ResourceLoader.exists(path):
				break
			found.append(load(path))
			n += 1
		_textures[species] = found
	return _textures[species]


## A number in 0..1 from a tree's place and `salt`, the same every time
## and with every engine ([StableHash]).
static func place_hash(at: Vector2, salt: int) -> float:
	return StableHash.unit([int(at.x), int(at.y), salt])


## Which of `count` images a tree at `at` shows.
static func variant(at: Vector2, count: int) -> int:
	return StableHash.below([int(at.x), int(at.y), 7], count)


## The rectangle a tree at `at` (millimetres) of `height_cm` covers on
## screen, its image `texture`: the bottom edge's middle on the foot.
static func rect(at: Vector2, height_cm: int, texture: Texture2D) -> Rect2:
	var scale := 1.0 + (place_hash(at, 3) * 2.0 - 1.0) * SIZE_JITTER
	# The species' true height at the world's scale (D40).
	var h := Scale.shown(float(height_cm) / 100.0) * Iso.PX_PER_M * scale
	var w := h * float(texture.get_width()) / float(texture.get_height())
	var foot := Iso.point_to_screen(at)
	return Rect2(foot - Vector2(w / 2.0, h), Vector2(w, h))


## The modulate that tells the sway shader (trees.gdshader) it draws a
## tree, how tall (`height_px`), its see-through `flags` (1 when it stands
## in front of the piloted body, +2 when it hides something under the
## mouse) and its phase, from its place `at`; the shader draws the image
## untinted.
static func marker(height_px: float, at: Vector2, flags: int) -> Color:
	return Color(clampf(height_px / 1280.0, 0.0, 1.0), 0.25 + 0.25 * float(flags), place_hash(at, 11), 0.94)


## Every image of every kind of tree in one list, so a tree keeps only an
## index; species id -> the indices of its images in that list.
static var _images: Array[Texture2D] = []
static var _image_ids: Dictionary = {}


## The index of the image a tree of `species` at `at` shows, or -1 when its
## kind has none.
static func image_index(species: String, at: Vector2) -> int:
	var ids: Variant = _image_ids.get(species)
	if ids == null:
		var list := PackedInt32Array()
		for t: Texture2D in textures(species):
			list.append(_images.size())
			_images.append(t)
		_image_ids[species] = list
		ids = list
	var n: int = (ids as PackedInt32Array).size()
	return -1 if n == 0 else (ids as PackedInt32Array)[variant(at, n)]


## The image at `index` (image_index).
static func image(index: int) -> Texture2D:
	return _images[index]


## Canvas items no tree uses now, kept to be given to the next tree that
## comes into view; at most POOL_MOST of them.
static var _pool: Array[RID] = []
const POOL_MOST := 4096


## Frees the canvas items kept for trees to come (the world is going).
static func free_pool() -> void:
	for item: RID in _pool:
		RenderingServer.free_rid(item)
	_pool.clear()


## The trees of one chunk, as light as can be (D39): packed arrays, no node
## and no dictionary per tree. Only the trees whose image may show on screen
## have a canvas item, given as the view moves, by cells of the screen
## (`show_cells`); the renderer sorts those with the bodies and the tall
## plants by their foot (y-sort), in the same order as when every tree had
## its node.
class Stand:
	extends RefCounted
	## Side of a cell of the screen, in pixels (zoom 1).
	const CELL_PX := 128.0
	## Each tree's place, in millimetres.
	var at := PackedVector2Array()
	## Each tree's rectangle on screen (Trees.rect): position then size.
	var rects := PackedVector2Array()
	## Each tree's image (Trees.image_index), -1 when its kind has none.
	var images := PackedInt32Array()
	## Each tree's see-through flags (Trees.marker).
	var flags := PackedByteArray()
	## How many of the cells shown each tree's image covers.
	var refs := PackedByteArray()
	## Cell of the screen -> PackedInt32Array, the trees whose image covers it.
	var cells: Dictionary = {}
	## The cells the trees cover, and those of them shown now.
	var bounds := Rect2i()
	var shown := Rect2i()
	## Tree index -> its canvas item, for the trees shown.
	var items: Dictionary = {}
	## The canvas item the trees' items hang from (a y-sorted node's).
	var parent := RID()

	## `trees`: as the server sends them ({"at": {x, y}, "species"});
	## `heights`: species id -> height in centimetres.
	func _init(trees: Array = [], heights: Dictionary = {}) -> void:
		var n := trees.size()
		at.resize(n)
		rects.resize(n * 2)
		images.resize(n)
		flags.resize(n)
		refs.resize(n)
		var lo := Vector2i(1 << 30, 1 << 30)
		var hi := -lo
		for i in n:
			var t: Dictionary = trees[i]
			var species := str(t.get("species", ""))
			# As Net.point reads it.
			var place: Variant = t.get("at", {})
			var p := Vector2(float(place.get("x", 0)), float(place.get("y", 0))) if place is Dictionary else Vector2.ZERO
			at[i] = p
			var index := Trees.image_index(species, p)
			images[i] = index
			if index < 0:
				continue
			var r := Trees.rect(p, int(heights.get(species, 400)), Trees.image(index))
			rects[i * 2] = r.position
			rects[i * 2 + 1] = r.size
			var a := Vector2i((r.position / CELL_PX).floor())
			var b := Vector2i((r.end / CELL_PX).floor())
			lo = Vector2i(mini(lo.x, a.x), mini(lo.y, a.y))
			hi = Vector2i(maxi(hi.x, b.x), maxi(hi.y, b.y))
			for cy in range(a.y, b.y + 1):
				for cx in range(a.x, b.x + 1):
					var cell := Vector2i(cx, cy)
					if not cells.has(cell):
						cells[cell] = PackedInt32Array()
					# Packed arrays are values: append, then store back.
					var list: PackedInt32Array = cells[cell]
					list.append(i)
					cells[cell] = list
		if hi.x >= lo.x:
			bounds = Rect2i(lo, hi - lo + Vector2i.ONE)

	func size() -> int:
		return at.size()

	## Tree `i`'s rectangle on screen.
	func rect(i: int) -> Rect2:
		return Rect2(rects[i * 2], rects[i * 2 + 1])

	## The cells of the screen (cell_range) a view `seen` (in the world's
	## pixels) touches.
	static func cell_range(seen: Rect2) -> Rect2i:
		var a := Vector2i((seen.position / CELL_PX).floor())
		var b := Vector2i((seen.end / CELL_PX).floor())
		return Rect2i(a, b - a + Vector2i.ONE)

	## Shows the trees whose image covers a cell of `range` (cell_range),
	## and only them: a canvas item for each tree coming in, back to the
	## pool for each going out. Only the cells that changed are looked at.
	func show_cells(range: Rect2i) -> void:
		var target := range.intersection(bounds) if range.has_area() and bounds.has_area() else Rect2i()
		if target == shown:
			return
		for part: Rect2i in Stand.minus(shown, target):
			_count(part, -1)
		for part: Rect2i in Stand.minus(target, shown):
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
		var item: RID
		if Trees._pool.is_empty():
			item = RenderingServer.canvas_item_create()
			RenderingServer.canvas_item_set_use_parent_material(item, true)
			# The mipmaps the tree nodes took from the world view before.
			RenderingServer.canvas_item_set_default_texture_filter(item, RenderingServer.CANVAS_ITEM_TEXTURE_FILTER_LINEAR_WITH_MIPMAPS)
		else:
			item = Trees._pool.pop_back()
		RenderingServer.canvas_item_set_parent(item, parent)
		# Sorted by its foot (its transform), ties in the server's order (its
		# index), as when each tree had a node.
		RenderingServer.canvas_item_set_draw_index(item, i)
		var foot := Iso.point_to_screen(at[i])
		RenderingServer.canvas_item_set_transform(item, Transform2D(0.0, foot))
		items[i] = item
		_draw(i, item, foot)

	func _hide(i: int) -> void:
		var item: RID = items[i]
		items.erase(i)
		RenderingServer.canvas_item_clear(item)
		RenderingServer.canvas_item_set_parent(item, RID())
		if Trees._pool.size() < Trees.POOL_MOST:
			Trees._pool.append(item)
		else:
			RenderingServer.free_rid(item)

	func _draw(i: int, item: RID, foot: Vector2) -> void:
		DevTiming.tally("tree redraws")
		var r := rect(i)
		RenderingServer.canvas_item_clear(item)
		RenderingServer.canvas_item_add_texture_rect(item, Rect2(r.position - foot, r.size), Trees.image(images[i]).get_rid(), false, Trees.marker(r.size.y, at[i], flags[i]))

	## Sets tree `i`'s see-through flags (Trees.marker), redrawn if shown.
	func set_flags(i: int, f: int) -> void:
		if flags[i] == f:
			return
		flags[i] = f
		if items.has(i):
			_draw(i, items[i], Iso.point_to_screen(at[i]))

	## Gives every canvas item back (the chunk is going).
	func release() -> void:
		for i: int in items.keys():
			_hide(i)
		refs.fill(0)
		shown = Rect2i()

	## The cells of `a` not in `b`, as up to four rectangles.
	static func minus(a: Rect2i, b: Rect2i) -> Array[Rect2i]:
		var parts: Array[Rect2i] = []
		if not a.has_area():
			return parts
		if not b.has_area() or not a.intersects(b):
			parts.append(a)
			return parts
		var c := a.intersection(b)
		for part: Rect2i in [
			Rect2i(a.position.x, a.position.y, a.size.x, c.position.y - a.position.y),
			Rect2i(a.position.x, c.end.y, a.size.x, a.end.y - c.end.y),
			Rect2i(a.position.x, c.position.y, c.position.x - a.position.x, c.size.y),
			Rect2i(c.end.x, c.position.y, a.end.x - c.end.x, c.size.y),
		]:
			if part.has_area():
				parts.append(part)
		return parts


## The shadows of a chunk's trees, drawn once on the ground.
class ShadowLayer:
	extends Node2D
	var stand: Stand

	func _draw() -> void:
		if stand == null:
			return
		for i in stand.size():
			if stand.images[i] >= 0:
				Trees.draw_shadow(self, stand.rect(i), Iso.point_to_screen(stand.at[i]))
