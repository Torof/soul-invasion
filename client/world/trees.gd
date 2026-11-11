class_name Trees
extends RefCounted
## The trees standing on the ground (D37): the server says where each
## stands and of what kind; this draws it, an image of its kind standing
## on the point where its trunk meets the ground. The images are 256 px
## per metre (four times the screen, sharp up to zoom 4), their bottom
## edge at the trunk's foot, centred on it. Each is drawn as the tight
## polygon around what it shows (TreeOutline, `art/trees/outlines.json`),
## not its whole rectangle, so the clear corners cost the card nothing.

const DIR := "res://art/trees/"
## The outlines of the images, written by tools/tree_outlines.gd.
const OUTLINES := DIR + "outlines.json"
## How much a tree's size varies from one to the next, either way.
const SIZE_JITTER := 0.12
## The wind in trees.gdshader (its defaults, set again by the world view):
## the bend at the top as a share of the tree's height, at rest and in a
## gust, and the share of the height from the foot that never moves. The
## outlines are sized for them: change them, and run tools/tree_outlines.gd.
const SWAY := 0.005
const GUST := 0.012
const STILL := 0.3

## The images of a kind of tree are held in video memory only while a
## chunk held (or parked) has a tree of that kind (weight rule 4,
## `09-visual-production.md`; D39): each stand of trees holds its kinds
## (`hold`) and lets them go when it is released (`let_go`). The images of
## a kind are read on a worker thread the first time a stand holds it
## (`ResourceLoader.load_threaded_request`, one image at a time, so that
## the main thread makes one video texture a frame at most), the stands
## that wait for them built once they are in (`poll`, each frame); nothing
## of the kind is drawn meanwhile. They are freed UNLOAD_GRACE_S after the last stand let
## them go, so that walking along a stand's edge, where the farthest
## chunks of a kind come and go, does not read them again and again.
## Nothing changes on screen once they are in.

## Seconds the images of a kind no stand holds stay in memory. A body
## walks about 42 m in that time (speed 1.4 m/s): chunks come and go 288 m
## from it, so a kind's last chunk must be left well behind before its
## images are read again.
const UNLOAD_GRACE_S := 30.0
## species id -> Array[Texture2D], the images of the kinds in memory.
static var _textures: Dictionary = {}
## species id -> the paths of its images (`<id>_adult_01_summer.png`, ...),
## found once; a kind with none has an empty list.
static var _paths: Dictionary = {}
## species id -> how many stands hold it.
static var _holds: Dictionary = {}
## species id -> when (msec) its last hold went: freed after the grace.
static var _let_go: Dictionary = {}
## species id -> how many of its images are in (`_read`), for the kinds
## being read, in the order they were asked for. One image is read at a
## time, whatever the kind (`_in_flight`): its video texture is made by
## the main thread when it syncs the rendering, so one at a time spreads
## that over the frames instead of piling several into one.
static var _loading: Dictionary = {}
## species id -> Array[Texture2D], the images in already of a kind being read.
static var _read: Dictionary = {}
## The path of the image being read on the worker thread, "" when none.
static var _in_flight := ""
## The kind of the image being read.
static var _in_flight_species := ""
## The index of the image being read in its kind's `paths`.
static var _in_flight_index := 0
## The stands waiting for their kinds' images.
static var _waiting: Array = []
## The kinds in memory and the kinds being read, counted for F3.
static var images_loaded := 0
static var images_freed := 0
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


## The paths of the images of a kind of tree: `<id>_adult_01_summer.png`,
## `_02`, ... (found once, kept).
static func paths(species: String) -> PackedStringArray:
	if not _paths.has(species):
		var found := PackedStringArray()
		var n := 1
		while true:
			var path := DIR + "%s_adult_%02d_summer.png" % [species, n]
			if not ResourceLoader.exists(path):
				break
			found.append(path)
			n += 1
		_paths[species] = found
	return _paths[species]


## The images of a kind of tree, read now if they are not in memory (the
## tests and tools; the game holds its kinds and lets `poll` read them).
## Read this way and held by no stand, they are freed after the grace.
static func textures(species: String) -> Array:
	if not _textures.has(species):
		var found: Array = []
		for path: String in paths(species):
			# A read already started on a worker thread is waited for (load does).
			found.append(load(path))
		if _in_flight_species == species:
			_in_flight = ""
		_loading.erase(species)
		_read.erase(species)
		_advance()
		_place(species, found)
		if int(_holds.get(species, 0)) == 0 and not _let_go.has(species):
			_let_go[species] = Time.get_ticks_msec()
	return _textures[species]


## Whether the images of `species` are in memory.
static func loaded(species: String) -> bool:
	return _textures.has(species)


## How many stands hold `species`.
static func holds(species: String) -> int:
	return int(_holds.get(species, 0))


## A stand holds `kinds`: their images are kept, read on a worker thread
## if they are not in memory nor being read.
static func hold(kinds: PackedStringArray) -> void:
	for species: String in kinds:
		_holds[species] = int(_holds.get(species, 0)) + 1
		_let_go.erase(species)
		if _textures.has(species) or _loading.has(species):
			continue
		var list := paths(species)
		if list.is_empty():
			_place(species, [])
			continue
		_loading[species] = 0
		_read[species] = []
		_advance()


## A stand let `kinds` go: those no stand holds any more are freed after
## the grace, unless a stand holds them again before.
static func let_go(kinds: PackedStringArray) -> void:
	var now := Time.get_ticks_msec()
	for species: String in kinds:
		var left := int(_holds.get(species, 0)) - 1
		if left > 0:
			_holds[species] = left
			continue
		_holds.erase(species)
		_let_go[species] = now


## Whether every kind of `kinds` has its images in memory.
static func ready(kinds: PackedStringArray) -> bool:
	for species: String in kinds:
		if not _textures.has(species):
			return false
	return true


## Each frame (`now` in msec; the tests pass a later time): takes in the
## images whose read is done and builds the stands that waited for them;
## frees the images of the kinds no stand has held for the grace. How many
## stands were built. With `wait`, the reads in progress are waited for.
static func poll(now: int = Time.get_ticks_msec(), wait: bool = false) -> int:
	# The image being read is in: taken, its kind placed if it was its
	# last, the next one asked for (with `wait`, each in turn, waited for).
	while not _in_flight.is_empty():
		if ResourceLoader.load_threaded_get_status(_in_flight) == ResourceLoader.THREAD_LOAD_IN_PROGRESS and not wait:
			break
		var t := ResourceLoader.load_threaded_get(_in_flight)
		var species := _in_flight_species
		_in_flight = ""
		if t is Texture2D:
			(_read[species] as Array).append(t)
		else:
			push_error("tree image %s could not be read" % _paths[species][_in_flight_index])
		_loading[species] = _in_flight_index + 1
		if int(_loading[species]) >= (_paths[species] as PackedStringArray).size():
			var found: Array = _read[species]
			_loading.erase(species)
			_read.erase(species)
			_place(species, found)
		_advance()
	var built := 0
	if not _waiting.is_empty():
		var still: Array = []
		for stand: Stand in _waiting:
			if ready(stand.kinds):
				stand._build()
				built += 1
			else:
				still.append(stand)
		_waiting = still
	for species: String in _let_go.keys():
		if now - int(_let_go[species]) < UNLOAD_GRACE_S * 1000.0 or _loading.has(species):
			continue
		_let_go.erase(species)
		_unload(species)
	return built


## Asks the worker thread for the next image of the kinds being read,
## if none is being read.
static func _advance() -> void:
	if not _in_flight.is_empty():
		return
	for species: String in _loading:
		var i := int(_loading[species])
		var list: PackedStringArray = _paths[species]
		if i >= list.size():
			continue
		_in_flight = list[i]
		_in_flight_species = species
		_in_flight_index = i
		ResourceLoader.load_threaded_request(_in_flight, "Texture2D")
		return


## The images of `species` are in: kept, and given their indices.
static func _place(species: String, found: Array) -> void:
	_textures[species] = found
	var ids: Variant = _image_ids.get(species)
	if ids == null:
		var list := PackedInt32Array()
		var list_paths := paths(species)
		for i in found.size():
			var t: Texture2D = found[i]
			list.append(_images.size())
			_images.append(t)
			var outline := _outline(list_paths[i], t)
			_hulls.append(outline)
			var triangles := Geometry2D.triangulate_polygon(outline)
			if triangles.is_empty():
				push_warning("%s: its outline could not be triangulated; drawn whole" % list_paths[i])
				outline = PackedVector2Array()
				for p: Vector2 in TreeOutline.fallback(t.get_width(), t.get_height()):
					outline.append(Vector2(p.x / float(t.get_width()), p.y / float(t.get_height())))
				_hulls[-1] = outline
				triangles = Geometry2D.triangulate_polygon(outline)
			_hull_indices.append(triangles)
		_image_ids[species] = list
	else:
		# The same indices as the first time: a stand keeps its indices.
		for i in (ids as PackedInt32Array).size():
			_images[(ids as PackedInt32Array)[i]] = found[i] if i < found.size() else null
	if not found.is_empty():
		images_loaded += 1
		DevTiming.tally("tree images loaded")


## The images of `species` leave memory (video memory with them, nothing
## else holds them).
static func _unload(species: String) -> void:
	var found: Variant = _textures.get(species)
	_textures.erase(species)
	if found == null or (found as Array).is_empty():
		return
	for index: int in (_image_ids[species] as PackedInt32Array):
		_images[index] = null
	images_freed += 1
	DevTiming.tally("tree images freed")


## Frees the images of every kind at once (the world is going).
static func unload_all() -> void:
	poll(Time.get_ticks_msec(), true)
	for species: String in _textures.keys():
		_unload(species)
	_holds.clear()
	_let_go.clear()
	_waiting.clear()


## The kinds in memory, with the video memory their images take (BC7 with
## mipmaps: a byte a pixel and a third more), for F3: {species: bytes}.
static func memory() -> Dictionary:
	var out := {}
	for species: String in _textures:
		var bytes := 0
		for t: Texture2D in _textures[species]:
			bytes += t.get_width() * t.get_height() * 4 / 3
		out[species] = bytes
	return out


## The kinds being read now, for F3.
static func loading() -> PackedStringArray:
	return PackedStringArray(_loading.keys())


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
## index; species id -> the indices of its images in that list. A kind's
## indices are given once and kept: its slots are emptied when its images
## leave memory and filled again, the same, when they come back.
static var _images: Array[Texture2D] = []
static var _image_ids: Dictionary = {}
## Each image's outline, by its index: the polygon in the image's own
## coordinates (0..1 across and down it, past them for the wind's room),
## and its triangles (Geometry2D.triangulate_polygon, once). Kept when the
## image leaves memory: a few bytes.
static var _hulls: Array[PackedVector2Array] = []
static var _hull_indices: Array[PackedInt32Array] = []
## image file name -> its outline as written (texture size, points in
## texels), read once from OUTLINES.
static var _outlines: Dictionary = {}
static var _outlines_read := false


## The outline of `index`'s image (image_index), in the image's own
## coordinates; and its triangles.
static func hull(index: int) -> PackedVector2Array:
	return _hulls[index]


static func hull_indices(index: int) -> PackedInt32Array:
	return _hull_indices[index]


## The outline of the image at `path`, as `texture` is held (its size):
## from OUTLINES, or the whole rectangle widened for the wind, as before
## the outlines, when the image has none written (a warning: run
## tools/tree_outlines.gd).
static func _outline(path: String, texture: Texture2D) -> PackedVector2Array:
	if not _outlines_read:
		_outlines_read = true
		if FileAccess.file_exists(OUTLINES):
			var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(OUTLINES))
			if parsed is Dictionary:
				_outlines = parsed
	var w := texture.get_width()
	var h := texture.get_height()
	var entry: Variant = _outlines.get(path.get_file())
	var points := PackedVector2Array()
	if entry is Dictionary:
		var size: Array = entry.get("texture", [])
		# The points are in the texels of the image as the card holds it
		# (padded to the compression's blocks), which the UVs 0..1 span.
		var image: Array = entry.get("image", size)
		if size.size() == 2 and int(size[0]) == w and int(size[1]) == h and image.size() == 2:
			for p: Array in entry.get("points", []):
				points.append(Vector2(float(p[0]) / float(image[0]), float(p[1]) / float(image[1])))
	if points.size() < 3:
		push_warning("%s has no outline for its size (%d x %d): drawn whole; run tools/tree_outlines.gd" % [path, w, h])
		points.clear()
		for p: Vector2 in TreeOutline.fallback(w, h):
			points.append(Vector2(p.x / float(w), p.y / float(h)))
	return points


## The index of the image a tree of `species` at `at` shows, or -1 when its
## kind has none; its images must be in memory (`ready`).
static func image_index(species: String, at: Vector2) -> int:
	var ids: Variant = _image_ids.get(species)
	if ids == null:
		return -1
	var n: int = (ids as PackedInt32Array).size()
	return -1 if n == 0 else (ids as PackedInt32Array)[variant(at, n)]


## The image at `index` (image_index); null while its kind is out of memory.
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
## its node. A stand holds its kinds' images (Trees.hold) from its making
## to its release; made while they are being read, it is built (`built`)
## once they are in, and shows nothing before.
class Stand:
	extends RefCounted
	## Side of a cell of the screen, in pixels (zoom 1).
	const CELL_PX := 128.0
	## Each tree's place, in millimetres.
	var at := PackedVector2Array()
	## Each tree's rectangle on screen (Trees.rect): position then size.
	var rects := PackedVector2Array()
	## Each tree's image (Trees.image_index), -1 when its kind has none or
	## while the stand is not built.
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
	## The kinds of tree in the stand, held while it lives.
	var kinds := PackedStringArray()
	## Each tree's kind, an index in `kinds`.
	var kind := PackedByteArray()
	## Each kind's height in centimetres, in the order of `kinds`.
	var heights := PackedInt32Array()
	## Whether the rectangles and cells are worked out (the images are in).
	var built := false
	## The cells asked for before the stand was built: shown once it is.
	var _wanted := Rect2i()
	## Whether the stand holds its kinds (once, until released).
	var _holding := false
	## The stand is built: its trees may show from now on.
	signal was_built

	## `trees`: as a chunk brings them (Protocol.decode_chunk: three ints
	## a tree, its species' index in `species_ids`, then where it stands, x
	## and y, in millimetres), or as dictionaries ({"at": {x, y},
	## "species"}); `heights`: species id -> height in centimetres.
	func _init(trees: Variant = [], tree_heights: Dictionary = {}, species_ids := PackedStringArray()) -> void:
		var packed: bool = trees is PackedInt32Array
		var triples: PackedInt32Array = trees if packed else PackedInt32Array()
		var listed: Array = [] if packed else trees
		var n := triples.size() / 3 if packed else listed.size()
		at.resize(n)
		rects.resize(n * 2)
		images.resize(n)
		images.fill(-1)
		flags.resize(n)
		refs.resize(n)
		kind.resize(n)
		var which: Dictionary = {}
		for i in n:
			var species: String
			if packed:
				var index := triples[i * 3]
				species = species_ids[index] if index < species_ids.size() else ""
				at[i] = Vector2(triples[i * 3 + 1], triples[i * 3 + 2])
			else:
				var t: Dictionary = listed[i]
				species = str(t.get("species", ""))
				# As Net.point reads it.
				var place: Variant = t.get("at", {})
				at[i] = Vector2(float(place.get("x", 0)), float(place.get("y", 0))) if place is Dictionary else Vector2.ZERO
			var k: Variant = which.get(species)
			if k == null:
				k = kinds.size()
				which[species] = k
				kinds.append(species)
				heights.append(int(tree_heights.get(species, 400)))
			kind[i] = k
		if n == 0:
			built = true
			return
		Trees.hold(kinds)
		_holding = true
		if Trees.ready(kinds):
			_build()
		else:
			Trees._waiting.append(self)

	## The images are in: each tree's rectangle and the cells it covers,
	## then the cells asked for meanwhile are shown.
	func _build() -> void:
		var n := at.size()
		var lo := Vector2i(1 << 30, 1 << 30)
		var hi := -lo
		for i in n:
			var p := at[i]
			var index := Trees.image_index(kinds[kind[i]], p)
			images[i] = index
			if index < 0:
				continue
			var r := Trees.rect(p, heights[kind[i]], Trees.image(index))
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
		built = true
		if _wanted.has_area():
			var wanted := _wanted
			_wanted = Rect2i()
			show_cells(wanted)
		was_built.emit()

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
		if not built:
			_wanted = range
			return
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

	## The tree's image as its outline (Trees.hull) scaled to its rectangle:
	## the polygon's vertices carry the image's own coordinates as UVs (past
	## 0..1 in the room left for the wind, which the shader discards once
	## bent back), and the marker as their colour.
	func _draw(i: int, item: RID, foot: Vector2) -> void:
		DevTiming.tally("tree redraws")
		var r := rect(i)
		var index := images[i]
		var outline := Trees.hull(index)
		var n := outline.size()
		var points := PackedVector2Array()
		points.resize(n)
		var origin := r.position - foot
		for j in n:
			points[j] = origin + outline[j] * r.size
		var colours := PackedColorArray()
		colours.resize(n)
		colours.fill(Trees.marker(r.size.y, at[i], flags[i]))
		RenderingServer.canvas_item_clear(item)
		RenderingServer.canvas_item_add_triangle_array(item, Trees.hull_indices(index), points, colours, outline, PackedInt32Array(), PackedFloat32Array(), Trees.image(index).get_rid())

	## Sets tree `i`'s see-through flags (Trees.marker), redrawn if shown.
	func set_flags(i: int, f: int) -> void:
		if flags[i] == f:
			return
		flags[i] = f
		if items.has(i):
			_draw(i, items[i], Iso.point_to_screen(at[i]))

	## Gives every canvas item back and lets its kinds go (the chunk is
	## going): their images are freed after the grace if no stand holds them.
	func release() -> void:
		for i: int in items.keys():
			_hide(i)
		refs.fill(0)
		shown = Rect2i()
		_wanted = Rect2i()
		if _holding:
			_holding = false
			Trees.let_go(kinds)
			if not built:
				Trees._waiting.erase(self)

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


## The shadows of a chunk's trees, drawn once on the ground, and again
## when the stand is built after its images came in.
class ShadowLayer:
	extends Node2D
	var stand: Stand:
		set(value):
			if stand != null and stand.was_built.is_connected(queue_redraw):
				stand.was_built.disconnect(queue_redraw)
			stand = value
			if stand != null and not stand.built:
				stand.was_built.connect(queue_redraw)
			queue_redraw()

	func _draw() -> void:
		if stand == null:
			return
		for i in stand.size():
			if stand.images[i] >= 0:
				Trees.draw_shadow(self, stand.rect(i), Iso.point_to_screen(stand.at[i]))
