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
	"brook": Color(0.35, 0.52, 0.62),
	"rock": Color(0.48, 0.45, 0.42),
}
const ITEM_RADIUS := 4.5

var _origin := Vector2i.ZERO
var _size := 32


## terrain -> the child canvas drawing that terrain's textured tiles
var _grounds: Dictionary = {}
## terrain -> the child canvas drawing that terrain's shore fills, above
## every ground canvas and below the chunk's own drawing.
var _fills: Dictionary = {}


func _fill_of(terrain: String) -> GroundCanvas:
	var canvas: GroundCanvas = _fills.get(terrain)
	if canvas == null:
		canvas = GroundCanvas.new()
		canvas.terrain = terrain
		canvas.z_index = -1
		canvas.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		canvas.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		canvas.material = ground_material(terrain)
		add_child(canvas)
		_fills[terrain] = canvas
	return canvas


func _ready() -> void:
	add_to_group("chunk_views")
	# The shore brushes repeat along the bank.
	texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
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
		canvas.z_index = -4 + int(GROUND_ORDER.get(terrain, 0))
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
	## [{"points": PackedVector2Array, "uvs": PackedVector2Array}]: extra
	## polygons of this terrain's texture, the bands along the shore line
	## that paint the terrain up to the line regardless of the tiles.
	var fills: Array = []

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
		for fill in fills:
			var pts: PackedVector2Array = fill["points"]
			var colours := PackedColorArray()
			colours.resize(pts.size())
			colours.fill(Color.WHITE)
			draw_polygon(pts, colours, fill["uvs"], texture)
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
	_brushes.clear()
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
## The pieces are cut with this margin around the tile's diamond.
const SHORE_ISO_MARGIN := 1.25
const SHORE_ISO := {
	"N": "01_far_bank_down_right", "W": "02_far_bank_up_right",
	"S": "03_near_bank_down_right", "E": "04_near_bank_up_right",
	"NW": "05_horizontal_land_above", "ES": "06_horizontal_land_below",
	"NE": "07_vertical_land_right", "SW": "08_vertical_land_left",
	# Two adjacent land sides whose bank turns an L (the diagonal beside
	# one of them is land too) instead of running straight through.
	"NWi": "05i_inner_corner_land_above", "ESi": "06i_inner_corner_land_below",
	"NEi": "07i_inner_corner_land_right", "SWi": "08i_inner_corner_land_left",
	"NEW": "09_bay_open_down_left", "NES": "10_bay_open_up_left",
	"ESW": "11_bay_open_up_right", "NSW": "12_bay_open_down_right",
	"NS": "13_channel_down_right", "EW": "14_channel_up_right",
	"nw": "15_corner_land_up", "ne": "16_corner_land_right",
	"se": "17_corner_land_down", "sw": "18_corner_land_left",
}
static var _shore_iso: Dictionary = {}


## `shade` picks the copy whose grass is tinted like the woods, for a
## bank whose land is forest.
static func shore_iso_piece(name: String, shade: bool = false) -> Texture2D:
	var key := name + ("@forest" if shade else "")
	if not _shore_iso.has(key):
		var path := SHORE_ISO_DIR + ("forest/" if shade else "") + name + ".png"
		_shore_iso[key] = load_texture(path) if ResourceLoader.exists(path) or FileAccess.file_exists(path) else null
		if _shore_iso[key] == null and shade:
			_shore_iso[key] = shore_iso_piece(name, false)
	return _shore_iso[key]


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
	# Two adjacent land sides: a true corner of land (no land on the two
	# outer diagonals) takes the rounded L piece; a step of a staircase
	# (land continues on an outer diagonal) takes the straight piece
	# through the tile centre, so the stairs read as one smooth line.
	if sides == "NW" and not (ne or sw):
		sides = "NWi"
	elif sides == "ES" and not (ne or sw):
		sides = "ESi"
	elif sides == "NE" and not (nw or se):
		sides = "NEi"
	elif sides == "SW" and not (nw or se):
		sides = "SWi"
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
			var shade := _terrain(tx, ty - 1) == "forest" or _terrain(tx + 1, ty) == "forest" or _terrain(tx, ty + 1) == "forest" or _terrain(tx - 1, ty) == "forest"
			for name in names:
				var texture := shore_iso_piece(name, shade)
				if texture != null:
					# The piece covers its tile plus a margin (SHORE_ISO_MARGIN),
					# so a bank painted just outside the diamond still shows.
					var centre := Iso.tile_to_screen(tx, ty)
					var size := Vector2(Iso.TILE_W, Iso.TILE_H) * SHORE_ISO_MARGIN
					draw_texture_rect(texture, Rect2(centre - size / 2.0, size), false)


## The shore as a line: the land/water boundary of the tiles (through
## the midpoints between tile centres), simplified into straight runs
## with a tolerance of about half a tile, its bends rounded, and drawn
## as a strip along which a bank brush repeats. Three brushes, cut from
## the generated straight pieces: "far" where the land is above the water
## on screen (the earthen face shows), "near" where it is below (grass
## overhangs the water), "profile" where the bank is vertical on screen.
## So a run keeps one direction until a bend, whatever its slope, and a
## staircase of tiles reads as one straight line. Tile data is untouched:
## the game still walks on the tiles.
const SHORE_BRUSH_DIR := "res://art/shore_brush/"
const SHORE_MARGIN := 3
const SHORE_TOLERANCE := 0.6
const SHORE_BRUSH_WIDTH := 34.0
const SHORE_BRUSH_LENGTH := 96.0
static var _brushes: Dictionary = {}


static func shore_brush(name: String) -> Texture2D:
	if not _brushes.has(name):
		_brushes[name] = load_texture(SHORE_BRUSH_DIR + name + ".png")
	return _brushes[name]


## Is the tile water, for the shore line: unknown tiles beyond the
## server's view count as whatever their known neighbour is, so the line
## does not close on the edge of the known world.
func _is_water_at(tx: int, ty: int) -> bool:
	var t := _terrain(tx, ty)
	return t == "water"


func _is_known(tx: int, ty: int) -> bool:
	return _terrain(tx, ty) != ""


## The land/water boundary of this chunk and a margin around it, as
## polylines in tile units (a tile's centre is at integer coordinates).
func shore_lines() -> Array:
	var lo := Vector2i(_origin.x - SHORE_MARGIN, _origin.y - SHORE_MARGIN)
	var hi := Vector2i(_origin.x + _size + SHORE_MARGIN, _origin.y + _size + SHORE_MARGIN)
	return ShorePieces.lines(_is_known_tile, _is_water_tile, lo, hi)


func _is_known_tile(t: Vector2i) -> bool:
	return _is_known(t.x, t.y)


func _is_water_tile(t: Vector2i) -> bool:
	return _is_water_at(t.x, t.y)


## For the bank pieces, a ford is water too (it has no bank of its own
## yet) and a brook is land (it is drawn as a line on the ground).
func _is_bank_water_tile(t: Vector2i) -> bool:
	var terrain := _terrain(t.x, t.y)
	return terrain == "water" or terrain == "ford"


## The bank pieces laid over this chunk by the world view (see
## ShorePieces), so chunks join without a seam; null when nobody did,
## and the chunk lays its own (the bench, the tests).
var bank_pieces: Variant = null


## The bank pieces of this chunk and a margin around it, laid here.
func lay_shore_pieces() -> Array:
	var lo := Vector2i(_origin.x - SHORE_MARGIN, _origin.y - SHORE_MARGIN)
	var hi := Vector2i(_origin.x + _size + SHORE_MARGIN, _origin.y + _size + SHORE_MARGIN)
	return ShorePieces.lay_all(_is_known_tile, _is_bank_water_tile, lo, hi)


## The bank pieces' images, one per straight heading and per one-step
## bend (ShorePieces.piece_name), 128 px square around the piece's middle.
const SHORE_PIECES_DIR := "res://art/shore_pieces/"
const SHORE_PIECE_SIZE := 128.0  # on screen at scale 1; the images are 512 px
## Width of the fills on each side of a piece, in tiles: the land's and
## the water's own textures repainted up to the piece, so the tiles'
## staircase never shows past the bank.
const SHORE_PIECE_FILL := 0.8
static var _piece_images: Dictionary = {}


static func shore_piece_image(name: String) -> Texture2D:
	if name == "":
		return null
	if not _piece_images.has(name):
		_piece_images[name] = load_texture(SHORE_PIECES_DIR + name + ".png")
	return _piece_images[name]


## Draws the bank pieces: the ground's own textures filled up to each
## piece on both sides, then the piece's image over the line. With
## "lines" in the shore spec, the pieces are drawn as coloured lines
## instead, to judge the laying.
func _draw_shore_pieces() -> void:
	var pieces: Array = bank_pieces if bank_pieces is Array else lay_shore_pieces()
	var lo := Vector2(_origin.x, _origin.y) - Vector2(0.5, 0.5)
	var hi := Vector2(_origin.x + _size, _origin.y + _size) - Vector2(0.5, 0.5)
	var spec: Dictionary = ground_spec().get("shore", {})
	if bool(spec.get("lines", false)):
		_set_fills({})
		ShorePieces.draw(self, pieces, lo, hi)
		return
	var fill := float(spec.get("fill", SHORE_PIECE_FILL))
	var forest_tint: Color = ground_params("forest").get("base_tint", Color.WHITE)
	var fills: Dictionary = {}
	var sprites: Array = []
	for piece in pieces:
		var mid := ShorePieces.middle(piece)
		if mid.x < lo.x or mid.y < lo.y or mid.x >= hi.x or mid.y >= hi.y:
			continue
		var h: Vector2 = ShorePieces.HEADINGS[piece["dir"]]
		var right := Vector2(-h.y, h.x)
		var land := _terrain(int(round(mid.x - right.x * 0.5)), int(round(mid.y - right.y * 0.5)))
		if land == "" or land == "water" or land == "ford" or ground_texture(land) == null:
			land = "grass"
		# A ford has no bank of its own yet: no water band, no image there.
		var wet := _terrain(int(round(mid.x + right.x * 0.5)), int(round(mid.y + right.y * 0.5)))
		var ford := wet == "ford"
		var pts: PackedVector2Array = piece["pts"]
		for i in range(pts.size() - 1):
			var a: Vector2 = pts[i]
			var b: Vector2 = pts[i + 1]
			var d := (b - a).normalized()
			var n := Vector2(-d.y, d.x)
			for pair in [[land, -fill, 0.02], ["water", -0.02, fill]]:
				var terrain: String = pair[0]
				if ground_texture(terrain) == null or (ford and terrain == "water"):
					continue
				if not fills.has(terrain):
					fills[terrain] = []
				fills[terrain].append({"points": _band(a, b, n, pair[1], pair[2], 0.05), "uvs": _band_uvs(a, b, n, pair[1], pair[2], 0.05)})
		var image := shore_piece_image(ShorePieces.piece_name(piece["dir"], piece["turn"]))
		if image != null and not ford:
			sprites.append([image, tile_point_to_screen(mid), forest_tint if land == "forest" else Color.WHITE])
	_set_fills(fills)
	var size := Vector2(SHORE_PIECE_SIZE, SHORE_PIECE_SIZE)
	for sprite in sprites:
		draw_texture_rect(sprite[0], Rect2(sprite[1] - size / 2.0, size), false, sprite[2])


## Hands each terrain's fill canvas its bands, emptying the others.
func _set_fills(fills: Dictionary) -> void:
	for terrain in _fills:
		if not fills.has(terrain):
			_fills[terrain].fills = []
			_fills[terrain].queue_redraw()
	for terrain in fills:
		var canvas := _fill_of(terrain)
		canvas.fills = fills[terrain]
		canvas.queue_redraw()


## Douglas-Peucker: the fewest straight runs within `tolerance` of the
## line, so a staircase of tiles becomes one run.
static func simplify(points: Array, tolerance: float) -> Array:
	if points.size() < 3:
		return points.duplicate()
	var a: Vector2 = points[0]
	var b: Vector2 = points[points.size() - 1]
	var worst := 0.0
	var at := 0
	for i in range(1, points.size() - 1):
		var p: Vector2 = points[i]
		var d: float
		if a.distance_to(b) < 0.001:
			d = p.distance_to(a)
		else:
			var t := clampf((p - a).dot(b - a) / (b - a).length_squared(), 0.0, 1.0)
			d = p.distance_to(a + (b - a) * t)
		if d > worst:
			worst = d
			at = i
	if worst <= tolerance:
		return [a, b]
	var left := simplify(points.slice(0, at + 1), tolerance)
	var right := simplify(points.slice(at), tolerance)
	return left.slice(0, left.size() - 1) + right


## Chaikin's corner cutting, twice: the bends become arcs, the ends stay.
static func round_bends(points: Array, passes: int = 2) -> Array:
	var line := points.duplicate()
	for _pass in passes:
		if line.size() < 3:
			return line
		var out: Array = [line[0]]
		for i in range(line.size() - 1):
			var p: Vector2 = line[i]
			var q: Vector2 = line[i + 1]
			out.append(p.lerp(q, 0.25))
			out.append(p.lerp(q, 0.75))
		out.append(line[line.size() - 1])
		line = out
	return line


static func tile_point_to_screen(p: Vector2) -> Vector2:
	return Vector2((p.x - p.y) * Iso.TILE_W / 2.0, (p.x + p.y) * Iso.TILE_H / 2.0)


## The shore line's segments for this chunk, computed once per redraw:
## [{"a", "b": Vector2 (tile units), "to_water": Vector2 (tile units,
## unit), "land": terrain name}], only the segments inside the chunk.
func shore_segments() -> Array:
	var inside_lo := Vector2(_origin.x, _origin.y) - Vector2(0.5, 0.5)
	var inside_hi := Vector2(_origin.x + _size, _origin.y + _size) - Vector2(0.5, 0.5)
	var segments: Array = []
	for raw in shore_lines():
		var shore: Dictionary = ground_spec().get("shore", {})
		var line := round_bends(simplify(raw, float(shore.get("tolerance", SHORE_TOLERANCE))), int(shore.get("rounding", 2)))
		if line.size() < 2:
			continue
		# Which side is the water: each segment probes both its sides; the
		# line's majority decides for the segments that cannot tell (short
		# chords at bends, deep in one terrain).
		var verdicts: Array = []
		var votes := 0
		for i in range(line.size() - 1):
			var d: Vector2 = (line[i + 1] - line[i]).normalized()
			var n0 := Vector2(-d.y, d.x)
			var c: Vector2 = (line[i] + line[i + 1]) * 0.5
			var plus := c + n0 * 0.7
			var minus := c - n0 * 0.7
			var pw := _is_water_at(int(round(plus.x)), int(round(plus.y)))
			var mw := _is_water_at(int(round(minus.x)), int(round(minus.y)))
			var v := 0
			if pw and not mw:
				v = 1
			elif mw and not pw:
				v = -1
			verdicts.append(v)
			votes += v
		var sign := 1.0 if votes >= 0 else -1.0
		var length := 0.0
		for i in range(line.size() - 1):
			var a: Vector2 = line[i]
			var b: Vector2 = line[i + 1]
			var dir: Vector2 = (b - a).normalized()
			# A boundary keeps its water on one side all along, so the line's
			# majority decides for every segment (a probe at a bend can lie).
			var to_water: Vector2 = Vector2(-dir.y, dir.x) * sign
			var mid: Vector2 = (a + b) * 0.5
			# The land's terrain: the first non-water tile away from the water,
			# probing farther at a concave bend where the nearest tile is water.
			var land := ""
			for reach: float in [0.6, 1.0, 1.5, 2.0]:
				var land_tile: Vector2 = mid - to_water * reach
				var t := _terrain(int(round(land_tile.x)), int(round(land_tile.y)))
				if t != "" and t != "water" and t != "ford":
					land = t
					break
			var u0 := length
			length += tile_point_to_screen(a).distance_to(tile_point_to_screen(b))
			if mid.x < inside_lo.x or mid.y < inside_lo.y or mid.x >= inside_hi.x or mid.y >= inside_hi.y:
				continue
			segments.append({"a": a, "b": b, "to_water": to_water, "land": land if land != "" else "grass", "u0": u0, "u1": length})
	return segments


## A rectangle in screen space along segment a-b (tile units), from
## `from` to `to` across it (tile units along the normal `n`), extended
## by `ext` tile units at both ends so consecutive rectangles overlap.
static func _band(a: Vector2, b: Vector2, n: Vector2, from: float, to: float, ext: float) -> PackedVector2Array:
	var dir: Vector2 = (b - a).normalized()
	var a2 := a - dir * ext
	var b2 := b + dir * ext
	return PackedVector2Array([
		tile_point_to_screen(a2 + n * from), tile_point_to_screen(b2 + n * from),
		tile_point_to_screen(b2 + n * to), tile_point_to_screen(a2 + n * to),
	])


## Ground texture coordinates of a screen polygon given in tile units.
static func _band_uvs(a: Vector2, b: Vector2, n: Vector2, from: float, to: float, ext: float) -> PackedVector2Array:
	var dir: Vector2 = (b - a).normalized()
	var a2 := a - dir * ext
	var b2 := b + dir * ext
	var s := 1.0 / GROUND_METRES
	var half := Vector2(0.5, 0.5)
	return PackedVector2Array([
		(a2 + n * from + half) * s, (b2 + n * from + half) * s,
		(b2 + n * to + half) * s, (a2 + n * to + half) * s,
	])


## Band width of the fills on each side of the line, in tiles, beyond
## the simplification tolerance: the tiles stray from the line by at most
## the tolerance plus half a tile. The bands repaint the tiles' own
## textures at their own coordinates, so a wide band costs nothing.
const SHORE_FILL_BEYOND := 0.6


func _draw_shore_curve() -> void:
	var brushes := {"far": shore_brush("far"), "near": shore_brush("near"), "profile": shore_brush("profile")}
	if brushes["far"] == null:
		return
	var forest_tint: Color = ground_params("forest").get("base_tint", Color.WHITE)
	var fills: Dictionary = {}
	var segs := shore_segments()
	var fill_width: float = float(ground_spec().get("shore", {}).get("tolerance", SHORE_TOLERANCE)) + SHORE_FILL_BEYOND
	var half := Vector2(0.5, 0.5)
	var s := 1.0 / GROUND_METRES
	for i in segs.size():
		var seg: Dictionary = segs[i]
		var a: Vector2 = seg["a"]
		var b: Vector2 = seg["b"]
		var n: Vector2 = seg["to_water"]
		# Fills: the land's texture up to the line on its side, the water's
		# on the other, so the tiles' own corners never show past the line.
		# Bands are not extended; the gap at a bend is closed by a wedge
		# between this band's end and the next band's start, on each side.
		var land: String = seg["land"]
		# The next segment along the line, or the first one when the line
		# closes on itself (a pond, an island).
		var next: Dictionary = segs[i + 1] if i + 1 < segs.size() else segs[0]
		var joined: bool = next["a"].distance_to(b) < 0.001
		for pair in [[land, -fill_width, 0.02], ["water", -0.02, fill_width]]:
			var terrain: String = pair[0]
			if ground_texture(terrain) == null:
				continue
			if not fills.has(terrain):
				fills[terrain] = []
			fills[terrain].append({"points": _band(a, b, n, pair[1], pair[2], 0.0), "uvs": _band_uvs(a, b, n, pair[1], pair[2], 0.0)})
			if joined:
				var n2: Vector2 = next["to_water"]
				var far: float = pair[1] if absf(pair[1]) > absf(pair[2]) else pair[2]
				var p1: Vector2 = b + n * far
				var p2: Vector2 = b + n2 * far
				if p1.distance_to(p2) > 0.01:
					var pts := PackedVector2Array([tile_point_to_screen(b), tile_point_to_screen(p1), tile_point_to_screen(p2)])
					var uvs := PackedVector2Array([(b + half) * s, (p1 + half) * s, (p2 + half) * s])
					fills[terrain].append({"points": pts, "uvs": uvs})
		# The brush.
		var n_screen: Vector2 = (tile_point_to_screen(b + n) - tile_point_to_screen(b)).normalized()
		var brush: String = "far" if n_screen.y > 0.4 else ("near" if n_screen.y < -0.4 else "profile")
		var v_land := 1.0 if brush == "near" else 0.0
		var v_water := 1.0 - v_land
		var sa := tile_point_to_screen(a)
		var sb := tile_point_to_screen(b)
		var dir: Vector2 = (sb - sa).normalized()
		var nn := Vector2(-dir.y, dir.x)
		if nn.dot(n_screen) < 0.0:
			nn = -nn
		var w := SHORE_BRUSH_WIDTH / 2.0
		var ext := w * 0.5
		var pa := sa - dir * ext
		var pb := sb + dir * ext
		var u0: float = (seg["u0"] - ext) / SHORE_BRUSH_LENGTH
		var u1: float = (seg["u1"] + ext) / SHORE_BRUSH_LENGTH
		var points := PackedVector2Array([pa - nn * w, pb - nn * w, pb + nn * w, pa + nn * w])
		var uvs := PackedVector2Array([Vector2(u0, v_land), Vector2(u1, v_land), Vector2(u1, v_water), Vector2(u0, v_water)])
		var tint := forest_tint if land == "forest" else Color.WHITE
		draw_polygon(points, PackedColorArray([tint, tint, tint, tint]), uvs, brushes[brush])
	for terrain in _fills:
		if not fills.has(terrain):
			_fills[terrain].fills = []
			_fills[terrain].queue_redraw()
	for terrain in fills:
		var canvas := _fill_of(terrain)
		canvas.fills = fills[terrain]
		canvas.queue_redraw()


func _draw_shore() -> void:
	var shore: Dictionary = ground_spec().get("shore", {})
	if shore.is_empty():
		return
	if str(shore.get("mode", "")) == "pieces":
		_draw_shore_pieces()
		return
	if str(shore.get("mode", "")) == "curve":
		_draw_shore_curve()
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
					if terrain == "water" or terrain == "ford" or terrain == "brook":
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
