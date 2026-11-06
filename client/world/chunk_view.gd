class_name ChunkView
extends Node2D
## Draws one chunk of ground and the items lying on it. Its draw commands
## are retained by the renderer, so they cost nothing per frame until the
## chunk changes.
##
## The ground is drawn from top-down seamless textures projected onto the
## diamonds (docs/09-visual-production.md): a texture covers
## GROUND_METRES × GROUND_METRES metres and repeats; each tile's diamond
## shows the square of the texture it stands on. A shader blends the
## texture with a sister image (or with itself, shifted and turned) by a
## soft noise, so its repetition does not read. Each terrain has its own
## material, so its layers never leak onto another terrain: the chunk
## holds one child canvas per textured terrain, under its own drawing.
## Terrains without a texture keep a flat colour.

## Metres covered by one ground texture.
const GROUND_METRES := 4.0
## The grounds: per terrain, its texture, its layers ("sibling" blended
## over slow noise, "second" in wide patches, "worn" in a few spots) and
## the shader numbers that differ from the defaults. Read from
## world/ground.json, so the development watcher reloads a change live.
## Sisters of the same grass (same style and tone, drawn differently) give
## the variety; tone variation is a tint in the shader, never another image.
const GROUND_SPEC := "res://world/ground.json"
const LAYERS := ["sibling", "second", "worn", "bank", "bank2"]
static var _spec: Dictionary = {}
## terrain -> the parameter names applied to its material, to reset the
## ones a reload no longer sets.
static var _applied: Dictionary = {}


static func ground_spec() -> Dictionary:
	if _spec.is_empty():
		_spec = read_ground_spec()
	return _spec


static func read_ground_spec() -> Dictionary:
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(GROUND_SPEC))
	return parsed if parsed is Dictionary else {}


## The path of a terrain's ground texture, "" when it has none.
static func ground_path(terrain: String) -> String:
	return str(ground_spec().get(terrain, {}).get("texture", ""))


## layer name -> path of the terrain's layer textures.
static func ground_layers(terrain: String) -> Dictionary:
	return ground_spec().get(terrain, {}).get("layers", {})


## The shader parameters of a terrain, as the shader takes them: a list of
## three numbers in the file is a colour.
static func ground_params(terrain: String) -> Dictionary:
	var params: Dictionary = {}
	var raw: Dictionary = ground_spec().get(terrain, {}).get("params", {})
	for name in raw:
		var value: Variant = raw[name]
		if value is Array and value.size() == 3:
			params[name] = Color(value[0], value[1], value[2])
		else:
			params[name] = value
	return params


## terrain -> the terrains it spills over at its edge, with a soft eroded
## border, so a wood's edge is not a line of diamonds. The server still
## decides every tile's terrain; this is only how the border is drawn.
## Each entry: the terrains spilled over and the width of the spill in
## tiles. The water's spill is its bank: the shader paints sand there.
const FRINGE_OVER := {
	"forest": {"over": ["grass"], "width": 2},
}
## Width, in tiles, over which a terrain feels its own edge (water
## lightens near its banks).
const SHORE_WIDTH := 2
## Terrains that change near their own edge (water lightens near its
## banks): their tiles carry, at each corner, how near the bank is.
const SHORE_TERRAINS := ["water"]
## Drawing order of the textured terrains: the one that spills is above.
const GROUND_ORDER := {"grass": 0, "forest": 1, "water": 2}
static var _ground_cache: Dictionary = {}
## terrain -> its ShaderMaterial
static var _ground_materials: Dictionary = {}
static var _noise_texture: Texture2D

const COLORS := {
	"grass": Color(0.45, 0.62, 0.30),
	"forest": Color(0.22, 0.40, 0.20),
	"water": Color(0.25, 0.45, 0.70),
	"ford": Color(0.55, 0.60, 0.50),
	"rock": Color(0.48, 0.45, 0.42),
}
const ITEM_RADIUS := 4.5

var _origin := Vector2i.ZERO
var _size := 32


## terrain -> the child canvas drawing that terrain's textured tiles
var _grounds: Dictionary = {}


func _ready() -> void:
	add_to_group("chunk_views")
## Looks up the terrain of a tile outside this chunk (set by the world
## view); "" when unknown, and the spill stops there.
var terrain_at: Callable


## The seamless noise every ground material blends with, made here, no
## image needed.
static func noise_texture() -> Texture2D:
	if _noise_texture == null:
		var noise := FastNoiseLite.new()
		noise.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
		noise.frequency = 0.02
		noise.seed = 7
		var texture := NoiseTexture2D.new()
		texture.noise = noise
		texture.seamless = true
		texture.width = 256
		texture.height = 256
		_noise_texture = texture
	return _noise_texture


## The ground material of a terrain: the blending shader with the
## terrain's layers. Shared by every chunk. A layer whose image is not
## there yet is simply off.
static func ground_material(terrain: String = "grass") -> ShaderMaterial:
	if not _ground_materials.has(terrain):
		var material := ShaderMaterial.new()
		material.shader = load("res://world/ground.gdshader")
		material.set_shader_parameter("noise_texture", noise_texture())
		apply_spec(material, terrain, false)
		_ground_materials[terrain] = material
	return _ground_materials[terrain]


## Gives a terrain's material its layers and numbers from the spec. With
## `fresh`, the images are read straight from their files (a live
## reload); otherwise through the import.
static func apply_spec(material: ShaderMaterial, terrain: String, fresh: bool) -> void:
	var layers := ground_layers(terrain)
	for layer in LAYERS:
		var path: String = str(layers.get(layer, ""))
		var texture: Texture2D = null
		if path != "":
			texture = fresh_texture(path) if fresh else load_texture(path)
		if texture != null:
			material.set_shader_parameter(layer + "_texture", texture)
		material.set_shader_parameter("has_" + layer, texture != null)
	var params := ground_params(terrain)
	for name in _applied.get(terrain, []):
		if not params.has(name):
			material.set_shader_parameter(name, null)
	for name in params:
		material.set_shader_parameter(name, params[name])
	_applied[terrain] = params.keys()


## The child canvas of a terrain, made on first use: it carries the
## terrain's material and sits under the chunk's own drawing (the items).
func _ground_of(terrain: String) -> GroundCanvas:
	var canvas: GroundCanvas = _grounds.get(terrain)
	if canvas == null:
		canvas = GroundCanvas.new()
		canvas.terrain = terrain
		canvas.z_index = -3 + int(GROUND_ORDER.get(terrain, 0))
		# Seamless textures repeat across the chunk; linear filtering for the zoom.
		canvas.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		canvas.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		canvas.material = ground_material(terrain)
		add_child(canvas)
		_grounds[terrain] = canvas
	return canvas


## One terrain's textured tiles of a chunk, drawn with that terrain's
## material.
class GroundCanvas:
	extends Node2D
	var terrain := ""
	var tiles: PackedVector2Array = []
	## [{"tile": Vector2i, "alphas": PackedFloat32Array}] the tiles of
	## another terrain this one spills over, with the opacity at each corner.
	var fringe: Array = []
	## tile -> PackedFloat32Array: how near the bank is at each corner of
	## this terrain's own tiles, carried to the shader in the vertex
	## colour's green channel (1 - nearness).
	var shore: Dictionary = {}

	func _draw() -> void:
		var white := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
		var texture := ChunkView.ground_texture(terrain)
		for tile in tiles:
			var tx := int(tile.x)
			var ty := int(tile.y)
			var colours := white
			var near: PackedFloat32Array = shore.get(Vector2i(tx, ty), PackedFloat32Array())
			if not near.is_empty():
				colours = PackedColorArray()
				for n in near:
					colours.append(Color(1, 1 - n, 1, 1))
			draw_polygon(Iso.diamond(tx, ty), colours, ChunkView.ground_uvs(tx, ty), texture)
		for spill in fringe:
			var tile: Vector2i = spill["tile"]
			var alphas: PackedFloat32Array = spill["alphas"]
			var colours := PackedColorArray()
			for a in alphas:
				colours.append(Color(1, 1, 1, a))
			draw_polygon(Iso.diamond(tile.x, tile.y), colours, ChunkView.ground_uvs(tile.x, tile.y), texture)


## Reloads the ground shader and textures from their files into the
## materials in use (development aid, see dev/watcher.gd). The textures
## are read straight from the image files, bypassing the import, so a
## change is seen at once.
static func reload_art() -> void:
	# A file caught half-written (the spec, an image) reads as nothing:
	# keep what we had rather than drop the ground until the next change.
	var spec := read_ground_spec()
	if not spec.is_empty():
		_spec = spec
	_decor.clear()
	_shore.clear()
	_shore_iso.clear()
	var shader := Shader.new()
	shader.code = FileAccess.get_file_as_string("res://world/ground.gdshader")
	for terrain in _spec:
		var path := ground_path(terrain)
		var texture := fresh_texture(path) if path != "" else null
		if texture != null:
			_ground_cache[terrain] = texture
		elif path == "":
			_ground_cache.erase(terrain)
	for terrain in _ground_materials:
		var material: ShaderMaterial = _ground_materials[terrain]
		material.shader = shader
		apply_spec(material, terrain, true)


## A texture through the import when it has one, else straight from the
## image file (an image added today, not yet imported): never null for a
## file that is there.
## Decorations: small generated images (rocks at the waterline; tufts,
## flowers and pebbles on the bank) sown by a fixed hash of the tile, so
## they are the same for everyone and never twice in the same place.
## Client-side and without effect on the game, as a trial: the day one of
## them counts, it becomes a server item like a pebble. Their kinds and
## doses are the "decor" entry of the ground spec.
const DECOR_DIR := "res://art/decor/"
## kind -> Array[Texture2D]
static var _decor: Dictionary = {}


static func decor_textures(kind: String) -> Array:
	if not _decor.has(kind):
		var found: Array = []
		for n in range(1, 40):
			var path := "%s%s_%02d.png" % [DECOR_DIR, kind, n]
			if ResourceLoader.exists(path) or FileAccess.file_exists(path):
				var texture := load_texture(path)
				if texture != null:
					found.append(texture)
		_decor[kind] = found
	return _decor[kind]


## A fixed number in [0, 1) for a tile and a salt: the same on every
## client, every time.
static func tile_hash(tx: int, ty: int, salt: int) -> float:
	var h := (tx * 73856093) ^ (ty * 19349663) ^ (salt * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float((h & 0x7fffffff) % 100003) / 100003.0


## The shore: a tileset of generated pieces laid on every land/water
## boundary. An "edge" piece covers a land tile and the water tile beside
## it (2 m across the bank, 1 m along it, the land on its left, the water
## on its right); a "corner" piece covers a 2 × 2 block whose bottom-right
## tile is water and the three others land. Pieces are mirrored and
## swapped to fit every orientation. Four matters (sand, grass, rock,
## step), chosen by stretches along the river, variants by tile hash.
## Transparent at their land and water ends, so they melt into our ground.
const SHORE_DIR := "res://art/shore/"
## The isometric shore: one generated piece per water tile touching land,
## drawn in three-quarter view (a far bank shows its earthen face, a near
## bank only its line), chosen by the land around the tile and laid on
## the tile's diamond as it is, never turned or mirrored. The pieces are
## numbered like the schematics they were generated from.
const SHORE_ISO_DIR := "res://art/shore_iso/"
const SHORE_ISO := {
	"N": "01_far_bank_down_right", "W": "02_far_bank_up_right",
	"S": "03_near_bank_down_right", "E": "04_near_bank_up_right",
	"NW": "05_horizontal_land_above", "ES": "06_horizontal_land_below",
	"NE": "07_vertical_land_right", "SW": "08_vertical_land_left",
	"NEW": "09_bay_open_down_left", "NES": "10_bay_open_up_left",
	"ESW": "11_bay_open_up_right", "NSW": "12_bay_open_down_right",
	"NS": "13_channel_down_right", "EW": "14_channel_up_right",
	"nw": "15_corner_land_up", "ne": "16_corner_land_right",
	"se": "17_corner_land_down", "sw": "18_corner_land_left",
}
static var _shore_iso: Dictionary = {}


static func shore_iso_piece(name: String) -> Texture2D:
	if not _shore_iso.has(name):
		var path := SHORE_ISO_DIR + name + ".png"
		_shore_iso[name] = load_texture(path) if ResourceLoader.exists(path) or FileAccess.file_exists(path) else null
	return _shore_iso[name]


## The piece names a water tile needs, from the land around it: one for
## its land sides (in N, E, S, W order), else one per land diagonal.
static func shore_iso_names(n: bool, e: bool, s: bool, w: bool, ne: bool, se: bool, sw: bool, nw: bool) -> Array:
	var sides := ""
	if n:
		sides += "N"
	if e:
		sides += "E"
	if s:
		sides += "S"
	if w:
		sides += "W"
	if sides != "":
		return [SHORE_ISO[sides]] if SHORE_ISO.has(sides) else []
	var names: Array = []
	if nw:
		names.append(SHORE_ISO["nw"])
	if ne:
		names.append(SHORE_ISO["ne"])
	if se:
		names.append(SHORE_ISO["se"])
	if sw:
		names.append(SHORE_ISO["sw"])
	return names
## "matter_kind" -> Array[Texture2D]
static var _shore: Dictionary = {}


static func shore_pieces(matter: String, kind: String) -> Array:
	var key := matter + "_" + kind
	if not _shore.has(key):
		var found: Array = []
		for n in range(1, 20):
			var path := "%s%s_%02d.png" % [SHORE_DIR, key, n]
			if ResourceLoader.exists(path) or FileAccess.file_exists(path):
				var texture := load_texture(path)
				if texture != null:
					found.append(texture)
		_shore[key] = found
	return _shore[key]


static func load_texture(path: String) -> Texture2D:
	if path == "":
		return null
	if ResourceLoader.exists(path):
		return load(path)
	return fresh_texture(path)


## A texture read straight from its image file, with mipmaps for the
## zoom; null when the file cannot be read.
static func fresh_texture(path: String) -> Texture2D:
	var image := Image.load_from_file(ProjectSettings.globalize_path(path))
	if image == null:
		return null
	image.generate_mipmaps()
	return ImageTexture.create_from_image(image)


## The ground texture of a terrain, or null when it has none yet.
static func ground_texture(terrain: String) -> Texture2D:
	if not _ground_cache.has(terrain):
		var path := ground_path(terrain)
		_ground_cache[terrain] = load_texture(path)
	return _ground_cache[terrain]


## Texture coordinates of a tile's four corners, clockwise from the top of
## the diamond: the square of the top-down texture under that tile.
static func ground_uvs(x: int, y: int) -> PackedVector2Array:
	var s := 1.0 / GROUND_METRES
	return PackedVector2Array([
		Vector2(x, y) * s,
		Vector2(x + 1, y) * s,
		Vector2(x + 1, y + 1) * s,
		Vector2(x, y + 1) * s,
	])
var _tiles: Array = []
## [{"x", "y", "material"}] as received
var _items: Array = []
## material id -> Color
var _looks: Dictionary = {}


## Sets the chunk's tiles and items; `origin` is its north-west tile.
func show_chunk(origin: Vector2i, size: int, tiles: Array, items: Array, looks: Dictionary) -> void:
	_origin = origin
	_size = size
	_tiles = tiles
	_items = items
	_looks = looks
	queue_redraw()


## The terrain of a tile, inside this chunk or, through the world view,
## outside it.
func _terrain(tx: int, ty: int) -> String:
	var x := tx - _origin.x
	var y := ty - _origin.y
	if x >= 0 and y >= 0 and x < _size and y < _size:
		return str(_tiles[y * _size + x])
	return str(terrain_at.call(Vector2i(tx, ty))) if terrain_at.is_valid() else ""


## Nearness of each corner of the chunk to the tiles `is_near` accepts
## (clockwise from the top, like Iso.diamond): 1 at a corner touching
## one, fading to 0 FRINGE + 1 corners away. Returns a function of the
## corner, in tile coordinates relative to the origin, where corner
## (cx, cy) is the top of tile (cx, cy).
func _nearness(is_near: Callable, width: int) -> Callable:
	# Corners of the chunk and a margin around it.
	var lo := -width
	var hi := _size + width
	var inside: Dictionary = {}
	for cy in range(lo, hi + 1):
		for cx in range(lo, hi + 1):
			for d in [Vector2i(-1, -1), Vector2i(0, -1), Vector2i(-1, 0), Vector2i(0, 0)]:
				if is_near.call(_terrain(_origin.x + cx + d.x, _origin.y + cy + d.y)):
					inside[Vector2i(cx, cy)] = true
					break
	return func(c: Vector2i) -> float:
		var best := width + 1
		for dy in range(-width, width + 1):
			for dx in range(-width, width + 1):
				if inside.has(c + Vector2i(dx, dy)):
					best = mini(best, maxi(absi(dx), absi(dy)))
		return 1.0 - float(best) / float(width + 1)


## The four corners' values of tile (x, y) of the chunk, from `of_corner`.
func _corners(of_corner: Callable, x: int, y: int) -> PackedFloat32Array:
	return PackedFloat32Array([
		of_corner.call(Vector2i(x, y)),
		of_corner.call(Vector2i(x + 1, y)),
		of_corner.call(Vector2i(x + 1, y + 1)),
		of_corner.call(Vector2i(x, y + 1)),
	])


## The tiles of this chunk that `terrain` spills over, with the opacity of
## the spill at each corner: 1 at a corner touching a tile of `terrain`,
## fading to 0 FRINGE + 1 corners away.
func fringe(terrain: String) -> Array:
	var spill: Dictionary = FRINGE_OVER.get(terrain, {})
	var over: Array = spill.get("over", [])
	if over.is_empty():
		return []
	var nearness := _nearness(func(t: String) -> bool: return t == terrain, int(spill.get("width", 1)))
	var result: Array = []
	for y in _size:
		for x in _size:
			if not over.has(str(_tiles[y * _size + x])):
				continue
			var alphas := _corners(nearness, x, y)
			if alphas[0] > 0.0 or alphas[1] > 0.0 or alphas[2] > 0.0 or alphas[3] > 0.0:
				result.append({"tile": _origin + Vector2i(x, y), "alphas": alphas})
	return result


## For the tiles of `terrain` in this chunk, how near each corner is to
## another, known terrain (the bank): tile -> four values, only for the
## tiles where some corner is near.
func shore(terrain: String) -> Dictionary:
	var nearness := _nearness(func(t: String) -> bool: return t != "" and t != terrain, SHORE_WIDTH)
	var result: Dictionary = {}
	for y in _size:
		for x in _size:
			if str(_tiles[y * _size + x]) != terrain:
				continue
			var near := _corners(nearness, x, y)
			if near[0] > 0.0 or near[1] > 0.0 or near[2] > 0.0 or near[3] > 0.0:
				result[_origin + Vector2i(x, y)] = near
	return result


func _draw() -> void:
	# Textured tiles go to their terrain's canvas; the rest is flat here.
	var by_terrain: Dictionary = {}
	for y in _size:
		for x in _size:
			var terrain: String = str(_tiles[y * _size + x])
			var tx := _origin.x + x
			var ty := _origin.y + y
			if ground_texture(terrain) != null:
				if not by_terrain.has(terrain):
					by_terrain[terrain] = PackedVector2Array()
				by_terrain[terrain].append(Vector2(tx, ty))
			else:
				draw_colored_polygon(Iso.diamond(tx, ty), COLORS.get(terrain, Color.MAGENTA))
	# A terrain that spills may need a canvas here even without a tile here.
	var spills: Dictionary = {}
	for terrain in FRINGE_OVER:
		if ground_texture(terrain) != null:
			var f := fringe(terrain)
			if not f.is_empty():
				spills[terrain] = f
	for terrain in _grounds:
		if not by_terrain.has(terrain) and not spills.has(terrain):
			_grounds[terrain].tiles = PackedVector2Array()
			_grounds[terrain].fringe = []
			_grounds[terrain].queue_redraw()
	for terrain in by_terrain.keys() + spills.keys():
		var canvas := _ground_of(terrain)
		canvas.tiles = by_terrain.get(terrain, PackedVector2Array())
		canvas.fringe = spills.get(terrain, [])
		canvas.shore = shore(terrain) if SHORE_TERRAINS.has(terrain) else {}
		canvas.queue_redraw()
	_draw_shore()
	_draw_decor()
	# Items: a small disc of the material's colour, slightly off the
	# tile's centre so several on one tile stay visible.
	var on_tile: Dictionary = {}
	for item in _items:
		var tile := Vector2i(int(item.get("x", 0)), int(item.get("y", 0)))
		var nth: int = on_tile.get(tile, 0)
		on_tile[tile] = nth + 1
		var centre := Iso.tile_to_screen(tile.x, tile.y) + Vector2(-8 + 8 * (nth % 3), 3 - 5 * (nth / 3))
		var material := str(item.get("material", ""))
		var colour: Color = _looks.get(material, Color.MAGENTA)
		draw_circle(centre, ITEM_RADIUS, colour)
		draw_circle(centre, ITEM_RADIUS, Color(0, 0, 0, 0.6), false, 1.0)
		if material == "corpse":
			draw_death_icon(self, centre + Vector2(0, -16))


## The shore pieces of this chunk, from its water tiles: for each, a
## corner on every pair of adjacent land sides, an edge on every other
## land side. [{"kind", "at": Vector2i (block origin), "size": Vector2i,
## "flip_u", "flip_v", "swap", "tile": Vector2i (the water tile)}].
func shore_plan() -> Array:
	var plan: Array = []
	var is_land := func(t: String) -> bool: return t != "" and t != "water" and t != "ford"
	# Points of land: a land tile with water on two adjacent sides and the
	# diagonal between them gets a "point" piece (land in the bottom-left
	# quarter of a 2 x 2 block, water elsewhere), and the two edges that
	# would meet there are not drawn. The chunk holding the land tile
	# draws the point; every chunk drops the edges.
	var consumed: Dictionary = {}
	# Corner cells on a 45-degree run of steps: the next corner cell along
	# the diagonal exists. Those draw a diagonal piece instead of an L, and
	# the points between them draw nothing (the diagonal covers them).
	var is_corner := func(wx: int, wy: int, dx: int, dy: int) -> bool:
		return _terrain(wx, wy) == "water" and is_land.call(_terrain(wx, wy + dy)) and is_land.call(_terrain(wx + dx, wy)) and is_land.call(_terrain(wx + dx, wy + dy))
	var diag: Dictionary = {}
	for y in range(-2, _size + 2):
		for x in range(-2, _size + 2):
			var wx := _origin.x + x
			var wy := _origin.y + y
			for d: Vector2i in [Vector2i(-1, -1), Vector2i(1, -1), Vector2i(-1, 1), Vector2i(1, 1)]:
				if is_corner.call(wx, wy, d.x, d.y) and (is_corner.call(wx - d.x, wy + d.y, d.x, d.y) or is_corner.call(wx + d.x, wy - d.y, d.x, d.y)):
					diag[[Vector2i(wx, wy), d]] = true
	for y in range(-1, _size + 1):
		for x in range(-1, _size + 1):
			var lx := _origin.x + x
			var ly := _origin.y + y
			if not is_land.call(_terrain(lx, ly)):
				continue
			var inside := x >= 0 and y >= 0 and x < _size and y < _size
			var wn := _terrain(lx, ly - 1) == "water"
			var ws := _terrain(lx, ly + 1) == "water"
			var ww := _terrain(lx - 1, ly) == "water"
			var we := _terrain(lx + 1, ly) == "water"
			var land := Vector2i(lx, ly)
			var two := Vector2i(2, 2)
			# A point beside a diagonal corner is covered by it.
			var near_diag := func(ax: int, ay: int, bx: int, by: int) -> bool:
				for d: Vector2i in [Vector2i(-1, -1), Vector2i(1, -1), Vector2i(-1, 1), Vector2i(1, 1)]:
					if diag.has([Vector2i(ax, ay), d]) or diag.has([Vector2i(bx, by), d]):
						return true
				return false
			if wn and we and _terrain(lx + 1, ly - 1) == "water":
				if inside and not near_diag.call(lx, ly - 1, lx + 1, ly):
					plan.append({"kind": "point", "at": Vector2i(lx, ly - 1), "size": two, "flip_u": false, "flip_v": false, "swap": false, "tile": land, "side": 0})
				consumed[[Vector2i(lx, ly - 1), "s"]] = true
				consumed[[Vector2i(lx + 1, ly), "w"]] = true
			if wn and ww and _terrain(lx - 1, ly - 1) == "water":
				if inside and not near_diag.call(lx, ly - 1, lx - 1, ly):
					plan.append({"kind": "point", "at": Vector2i(lx - 1, ly - 1), "size": two, "flip_u": true, "flip_v": false, "swap": false, "tile": land, "side": 1})
				consumed[[Vector2i(lx, ly - 1), "s"]] = true
				consumed[[Vector2i(lx - 1, ly), "e"]] = true
			if ws and we and _terrain(lx + 1, ly + 1) == "water":
				if inside and not near_diag.call(lx, ly + 1, lx + 1, ly):
					plan.append({"kind": "point", "at": Vector2i(lx, ly), "size": two, "flip_u": false, "flip_v": true, "swap": false, "tile": land, "side": 0})
				consumed[[Vector2i(lx, ly + 1), "n"]] = true
				consumed[[Vector2i(lx + 1, ly), "w"]] = true
			if ws and ww and _terrain(lx - 1, ly + 1) == "water":
				if inside and not near_diag.call(lx, ly + 1, lx - 1, ly):
					plan.append({"kind": "point", "at": Vector2i(lx - 1, ly), "size": two, "flip_u": true, "flip_v": true, "swap": false, "tile": land, "side": 1})
				consumed[[Vector2i(lx, ly + 1), "n"]] = true
				consumed[[Vector2i(lx - 1, ly), "e"]] = true
	for y in _size:
		for x in _size:
			if str(_tiles[y * _size + x]) != "water":
				continue
			var tx := _origin.x + x
			var ty := _origin.y + y
			var tile := Vector2i(tx, ty)
			var n: bool = is_land.call(_terrain(tx, ty - 1))
			var s: bool = is_land.call(_terrain(tx, ty + 1))
			var w: bool = is_land.call(_terrain(tx - 1, ty))
			var e: bool = is_land.call(_terrain(tx + 1, ty))
			var used := {"n": false, "s": false, "w": false, "e": false}
			var two := Vector2i(2, 2)
			# A corner paints land on three tiles: the diagonal one must be
			# land too, else two edges do.
			if n and w and is_land.call(_terrain(tx - 1, ty - 1)):
				plan.append({"kind": "corner", "at": Vector2i(tx - 1, ty - 1), "size": two, "flip_u": false, "flip_v": false, "swap": false, "tile": tile, "side": 0, "diag": diag.has([tile, Vector2i(-1, -1)])})
				used["n"] = true
				used["w"] = true
			if n and e and not used["n"] and is_land.call(_terrain(tx + 1, ty - 1)):
				plan.append({"kind": "corner", "at": Vector2i(tx, ty - 1), "size": two, "flip_u": true, "flip_v": false, "swap": false, "tile": tile, "side": 1, "diag": diag.has([tile, Vector2i(1, -1)])})
				used["n"] = true
				used["e"] = true
			if s and w and not used["w"] and is_land.call(_terrain(tx - 1, ty + 1)):
				plan.append({"kind": "corner", "at": Vector2i(tx - 1, ty), "size": two, "flip_u": false, "flip_v": true, "swap": false, "tile": tile, "side": 0, "diag": diag.has([tile, Vector2i(-1, 1)])})
				used["s"] = true
				used["w"] = true
			if s and e and not used["s"] and not used["e"] and is_land.call(_terrain(tx + 1, ty + 1)):
				plan.append({"kind": "corner", "at": Vector2i(tx, ty), "size": two, "flip_u": true, "flip_v": true, "swap": false, "tile": tile, "side": 1, "diag": diag.has([tile, Vector2i(1, 1)])})
				used["s"] = true
				used["e"] = true
			# An edge overlaps its neighbour only where the bank goes on
			# straight: the water tile before or after along the bank has
			# land on the same side. Elsewhere it stops at the block.
			var goes_on := func(dx: int, dy: int, land_dx: int, land_dy: int) -> bool:
				return _terrain(tx + dx, ty + dy) == "water" and is_land.call(_terrain(tx + dx + land_dx, ty + dy + land_dy))
			if w and not used["w"] and not consumed.has([tile, "w"]):
				plan.append({"kind": "edge", "at": Vector2i(tx - 1, ty), "size": Vector2i(2, 1), "flip_u": false, "flip_v": false, "swap": false, "tile": tile, "side": 0,
					"ext0": goes_on.call(0, -1, -1, 0), "ext1": goes_on.call(0, 1, -1, 0)})
			if e and not used["e"] and not consumed.has([tile, "e"]):
				plan.append({"kind": "edge", "at": Vector2i(tx, ty), "size": Vector2i(2, 1), "flip_u": true, "flip_v": false, "swap": false, "tile": tile, "side": 1,
					"ext0": goes_on.call(0, -1, 1, 0), "ext1": goes_on.call(0, 1, 1, 0)})
			if n and not used["n"] and not consumed.has([tile, "n"]):
				plan.append({"kind": "edge", "at": Vector2i(tx, ty - 1), "size": Vector2i(1, 2), "flip_u": false, "flip_v": false, "swap": true, "tile": tile, "side": 0,
					"ext0": goes_on.call(-1, 0, 0, -1), "ext1": goes_on.call(1, 0, 0, -1)})
			if s and not used["s"] and not consumed.has([tile, "s"]):
				plan.append({"kind": "edge", "at": Vector2i(tx, ty), "size": Vector2i(1, 2), "flip_u": true, "flip_v": false, "swap": true, "tile": tile, "side": 1,
					"ext0": goes_on.call(-1, 0, 0, 1), "ext1": goes_on.call(1, 0, 0, 1)})
	return plan


## The matter of the shore around a tile: one per stretch of the river,
## drawn by weight from the spec.
static func shore_matter(tile: Vector2i, side: int) -> String:
	var shore: Dictionary = ground_spec().get("shore", {})
	var weights: Dictionary = shore.get("weights", {"sand": 1.0})
	var stretch := int(shore.get("stretch", 8))
	# One matter per stretch along the river (its axis is y) and per bank.
	var roll := tile_hash(side, int(floor(float(tile.y) / stretch)), 7)
	var total := 0.0
	for m in weights:
		total += float(weights[m])
	var acc := 0.0
	for m in weights:
		acc += float(weights[m]) / total
		if roll <= acc:
			return str(m)
	return str(weights.keys()[0])


## Screen position of a tile corner (cx, cy): the top of tile (cx, cy).
static func corner_to_screen(cx: int, cy: int) -> Vector2:
	return Iso.tile_to_screen(cx, cy) + Vector2(0, -Iso.TILE_H / 2.0)


## The same for a fractional corner position.
static func corner_point_to_screen(cx: float, cy: float) -> Vector2:
	return Vector2((cx - cy) * Iso.TILE_W / 2.0, (cx + cy) * Iso.TILE_H / 2.0 - Iso.TILE_H / 2.0)


## How far, in tiles, a piece's quad extends past its block along the
## bank, so that consecutive pieces overlap and their faded ends blend.
const SHORE_OVERLAP := 0.5


func _draw_shore_iso() -> void:
	var is_land := func(t: String) -> bool: return t != "" and t != "water" and t != "ford"
	for y in _size:
		for x in _size:
			if str(_tiles[y * _size + x]) != "water":
				continue
			var tx := _origin.x + x
			var ty := _origin.y + y
			var names := shore_iso_names(
				is_land.call(_terrain(tx, ty - 1)), is_land.call(_terrain(tx + 1, ty)),
				is_land.call(_terrain(tx, ty + 1)), is_land.call(_terrain(tx - 1, ty)),
				is_land.call(_terrain(tx + 1, ty - 1)), is_land.call(_terrain(tx + 1, ty + 1)),
				is_land.call(_terrain(tx - 1, ty + 1)), is_land.call(_terrain(tx - 1, ty - 1)))
			for name in names:
				var texture := shore_iso_piece(name)
				if texture != null:
					var centre := Iso.tile_to_screen(tx, ty)
					draw_texture_rect(texture, Rect2(centre - Vector2(Iso.TILE_W / 2.0, Iso.TILE_H / 2.0), Vector2(Iso.TILE_W, Iso.TILE_H)), false)


func _draw_shore() -> void:
	var shore: Dictionary = ground_spec().get("shore", {})
	if shore.is_empty():
		return
	if bool(shore.get("iso", false)):
		_draw_shore_iso()
		return
	var white := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
	for piece in shore_plan():
		var at: Vector2i = piece["at"]
		var size: Vector2i = piece["size"]
		var matter := shore_matter(piece["tile"], int(piece.get("side", 0)))
		var kind: String = "diag" if bool(piece.get("diag", false)) else piece["kind"]
		var textures := shore_pieces(matter, kind)
		if textures.is_empty() and kind == "diag":
			textures = shore_pieces("grass", "diag")
		if textures.is_empty() and kind == "diag":
			textures = shore_pieces(matter, "corner")
		var flip_v: bool = piece["flip_v"]
		# Where the matter changes along the river, the first metre of the
		# new stretch is a transition piece (top half the old matter,
		# bottom half the new), on the edges that run along the river.
		if piece["kind"] == "edge" and size.x == 2:
			var before := shore_matter(piece["tile"] + Vector2i(0, -1), int(piece.get("side", 0)))
			if before != matter:
				var trans := shore_pieces("trans_" + before, matter)
				if trans.is_empty():
					trans = shore_pieces("trans_" + matter, before)
					if not trans.is_empty():
						flip_v = not flip_v
				if not trans.is_empty():
					textures = trans
		if textures.is_empty():
			textures = shore_pieces("grass", piece["kind"])
		if textures.is_empty():
			textures = shore_pieces("sand", piece["kind"])
		if textures.is_empty():
			continue
		var tile: Vector2i = piece["tile"]
		# Any variant fits any other: every piece's bank line enters and
		# leaves at the centre of its ends (normalised at cut time).
		var texture: Texture2D = textures[int(tile_hash(tile.x, tile.y, 9) * textures.size()) % textures.size()]
		# Along the bank the quad is longer than the block; corners extend
		# on every side (their faded sides are transparent anyway).
		# Extension at the start and end of the block along the bank.
		var along_y: bool = piece["kind"] != "edge" or size.x == 2
		var e0 := SHORE_OVERLAP if bool(piece.get("ext0", true)) else 0.0
		var e1 := SHORE_OVERLAP if bool(piece.get("ext1", true)) else 0.0
		var lo := Vector2(SHORE_OVERLAP, e0) if along_y else Vector2(e0, SHORE_OVERLAP)
		var hi := Vector2(SHORE_OVERLAP, e1) if along_y else Vector2(e1, SHORE_OVERLAP)
		if piece["kind"] == "edge":
			lo = Vector2(0.0, e0) if along_y else Vector2(e0, 0.0)
			hi = Vector2(0.0, e1) if along_y else Vector2(e1, 0.0)
		if kind == "diag":
			# Consecutive diagonals already overlap by half their length and
			# cross-fade in the piece itself: no extension.
			lo = Vector2.ZERO
			hi = Vector2.ZERO
		var points := PackedVector2Array()
		var uvs := PackedVector2Array()
		for corner: Vector2i in [Vector2i(0, 0), Vector2i(1, 0), Vector2i(1, 1), Vector2i(0, 1)]:
			var cx: float = at.x - lo.x + corner.x * (size.x + lo.x + hi.x)
			var cy: float = at.y - lo.y + corner.y * (size.y + lo.y + hi.y)
			points.append(corner_point_to_screen(cx, cy))
			var u := float(corner.x)
			var v := float(corner.y)
			if piece["swap"]:
				var t := u
				u = v
				v = t
			if piece["flip_u"]:
				u = 1.0 - u
			if flip_v:
				v = 1.0 - v
			uvs.append(Vector2(u, v))
		draw_polygon(points, white, uvs, texture)


## Sows the decorations of this chunk: for each kind, on the tiles its
## "where" names (water within a tile of land; land within two tiles of
## water), with its chance, at a fixed random spot of the tile.
func _draw_decor() -> void:
	var decor: Dictionary = ground_spec().get("decor", {})
	if decor.is_empty():
		return
	var water_near_land := shore("water")
	var land_near_water := _nearness(func(t: String) -> bool: return t == "water", 2)
	var water_tint: Color = ground_params("water").get("base_tint", Color.WHITE)
	var salt := 0
	for kind in decor:
		if not decor[kind] is Dictionary:
			continue  # the entry's note
		salt += 1
		var rule: Dictionary = decor[kind]
		var textures := decor_textures(kind)
		if textures.is_empty():
			continue
		var chance := float(rule.get("chance", 0.1))
		var size_px := float(rule.get("size_m", 0.5)) * Iso.PX_PER_M
		# How near the other terrain a tile must be (1 = touching it), and
		# how much the size varies from one element to the next.
		var min_near := float(rule.get("min_near", 0.6))
		var jitter := float(rule.get("size_jitter", 0.0))
		var tint := water_tint if str(rule.get("tint", "")) == "water" else Color.WHITE
		for y in _size:
			for x in _size:
				var tx := _origin.x + x
				var ty := _origin.y + y
				var terrain := str(_tiles[y * _size + x])
				var near := 0.0
				if str(rule.get("where", "")) == "water_near_land":
					if terrain != "water" or not water_near_land.has(Vector2i(tx, ty)):
						continue
					near = _max4(water_near_land[Vector2i(tx, ty)])
					if near < min_near:
						continue
				else:
					if terrain == "water" or terrain == "ford":
						continue
					near = _max4(_corners(land_near_water, x, y))
					if near <= 0.0:
						continue
				if tile_hash(tx, ty, salt) > chance * near:
					continue
				var texture: Texture2D = textures[int(tile_hash(tx, ty, salt + 100) * textures.size()) % textures.size()]
				var offset := Vector2(tile_hash(tx, ty, salt + 200) - 0.5, tile_hash(tx, ty, salt + 300) - 0.5) * 600.0
				var centre := Iso.point_to_screen(Vector2(tx * 1000 + 500, ty * 1000 + 500) + offset)
				var ratio := float(texture.get_height()) / float(texture.get_width())
				var scale := 1.0 + (tile_hash(tx, ty, salt + 400) * 2.0 - 1.0) * jitter
				var size := Vector2(size_px, size_px * ratio * 0.5) * scale
				draw_texture_rect(texture, Rect2(centre - size / 2.0, size), false, tint)


static func _max4(values: PackedFloat32Array) -> float:
	var best := 0.0
	for v in values:
		best = maxf(best, v)
	return best


## A small skull above a corpse, drawn on `canvas` at `at`.
static func draw_death_icon(canvas: CanvasItem, at: Vector2) -> void:
	canvas.draw_circle(at, 4.5, Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at, 4.5, Color.BLACK, false, 1.0)
	canvas.draw_rect(Rect2(at + Vector2(-2.5, 3), Vector2(5, 2.5)), Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at + Vector2(-1.7, -0.5), 1.1, Color.BLACK)
	canvas.draw_circle(at + Vector2(1.7, -0.5), 1.1, Color.BLACK)
