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
const LAYERS := ["sibling", "second", "worn", "bank", "bank2", "bank3", "ford1", "ford2", "ford3", "ford4"]
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
		canvas.z_index = -3
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
## Where the water runs at any tile the world view knows (a vector in
## metres a second; set by the world view, invalid on the benches).
var current_at: Callable
## The hash of a chunk's tiles as received, 0 when not held (set by the
## world view): the signature of the ground below reads nine of them
## instead of three thousand tiles.
var tiles_hash: Callable


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


## The average colour of a terrain's ground texture; clear when it has
## none (drawn a flat colour already).
static func average_colour(terrain: String) -> Color:
	var texture := ground_texture(terrain)
	if texture == null:
		return Color(0, 0, 0, 0)
	var image := texture.get_image()
	if image == null:
		return Color(0, 0, 0, 0)
	if image.is_compressed():
		image.decompress()
	image.resize(1, 1, Image.INTERPOLATE_BILINEAR)
	var c := image.get_pixel(0, 0)
	return Color(c.r, c.g, c.b, 1.0)


## Development (--dev, F5): the ground drawn a plain colour, its shaders
## (the grounds', the banks', the brooks') swapped for one that does
## nothing, to measure what they cost the graphics card; and back.
static func set_plain_ground(on: bool) -> void:
	var materials: Array = _ground_materials.values()
	if _brook_material != null:
		materials.append(_brook_material)
	for material: ShaderMaterial in materials:
		if on and not _real_shaders.has(material):
			_real_shaders[material] = material.shader
			material.shader = _plain_shader()
		elif not on and _real_shaders.has(material):
			material.shader = _real_shaders[material]
			_real_shaders.erase(material)


static var _real_shaders: Dictionary = {}
static var _plain: Shader


static func _plain_shader() -> Shader:
	if _plain == null:
		_plain = Shader.new()
		_plain.code = "shader_type canvas_item;\nvoid fragment() { COLOR = vec4(0.45, 0.62, 0.30, 1.0); }\n"
	return _plain


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
		# Low enough that the fog's veil (FarView.VEIL_Z) lies over the
		# ground and under what stands on it.
		canvas.z_index = -6 + int(GROUND_ORDER.get(terrain, 0))
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
		# Every polygon of this canvas in one mesh, drawn at once: one
		# draw call per terrain and chunk instead of one per tile.
		var verts := PackedVector2Array()
		var cols := PackedColorArray()
		var uvs := PackedVector2Array()
		var add_quad := func(pts: PackedVector2Array, c: PackedColorArray, u: PackedVector2Array) -> void:
			for k in [0, 1, 2, 0, 2, 3]:
				verts.append(pts[k])
				cols.append(c[k])
				uvs.append(u[k])
		var white := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
		for tile in tiles:
			var tx := int(tile.x)
			var ty := int(tile.y)
			var colours := white
			var near: PackedFloat32Array = shore.get(Vector2i(tx, ty), PackedFloat32Array())
			if not near.is_empty():
				colours = PackedColorArray()
				for n in near:
					colours.append(Color(1, 1 - n, 1, 1))
			add_quad.call(Iso.diamond(tx, ty), colours, ChunkView.ground_uvs(tx, ty))
		for fill in fills:
			var pts: PackedVector2Array = fill["points"]
			var fuv: PackedVector2Array = fill["uvs"]
			var order := Geometry2D.triangulate_polygon(pts)
			for k in order:
				verts.append(pts[k])
				cols.append(Color.WHITE)
				uvs.append(fuv[k])
		for spill in fringe:
			var tile: Vector2i = spill["tile"]
			var alphas: PackedFloat32Array = spill["alphas"]
			var colours := PackedColorArray()
			for a in alphas:
				colours.append(Color(1, 1, 1, a))
			add_quad.call(Iso.diamond(tile.x, tile.y), colours, ChunkView.ground_uvs(tile.x, tile.y))
		if verts.is_empty():
			return
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		var v3 := PackedVector3Array()
		v3.resize(verts.size())
		for i in verts.size():
			v3[i] = Vector3(verts[i].x, verts[i].y, 0.0)
		arrays[Mesh.ARRAY_VERTEX] = v3
		arrays[Mesh.ARRAY_COLOR] = cols
		arrays[Mesh.ARRAY_TEX_UV] = uvs
		var mesh := ArrayMesh.new()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		_mesh = mesh
		draw_mesh(mesh, ChunkView.ground_texture(terrain))

	var _mesh: ArrayMesh


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
	# Corners of the chunk and a margin around it: corner c is inside when
	# one of the four tiles meeting there is near. Tiles read once each.
	var lo := -width
	var span := _size + 2 * width + 1
	var tiles := span + 1
	var near_tile := PackedByteArray()
	near_tile.resize(tiles * tiles)
	var any := false
	for ty in tiles:
		for tx in tiles:
			if is_near.call(_terrain(_origin.x + lo - 1 + tx, _origin.y + lo - 1 + ty)):
				near_tile[ty * tiles + tx] = 1
				any = true
	var corners := _size + 1
	var values := PackedFloat32Array()
	values.resize(corners * corners)
	if any:
		# Corner (lo + i, lo + j) touches tiles i - 1 .. i and j - 1 .. j of
		# the margin, that is indices i .. i + 1 and j .. j + 1 here.
		var inside := PackedByteArray()
		inside.resize(span * span)
		for j in span:
			for i in span:
				var a := j * tiles + i
				if near_tile[a] or near_tile[a + 1] or near_tile[a + tiles] or near_tile[a + tiles + 1]:
					inside[j * span + i] = 1
		# The distance to the nearest inside corner, square rings (the
		# larger of the two offsets): first along each row, then across.
		var far := width + 1
		var along := PackedInt32Array()
		along.resize(span * span)
		for j in span:
			for i in span:
				var best := far
				for d in range(-width, width + 1):
					var k := i + d
					if k >= 0 and k < span and inside[j * span + k] and absi(d) < best:
						best = absi(d)
				along[j * span + i] = best
		for cy in corners:
			for cx in corners:
				var best := far
				var i := cx - lo
				for d in range(-width, width + 1):
					var k := cy - lo + d
					if k >= 0 and k < span:
						best = mini(best, maxi(absi(d), along[k * span + i]))
				values[cy * corners + cx] = 1.0 - float(best) / float(far)
	return func(c: Vector2i) -> float:
		if c.x < 0 or c.y < 0 or c.x >= corners or c.y >= corners:
			return 0.0
		return values[c.y * corners + c.x]


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
			if not over.has(_ground_terrain(_origin.x + x, _origin.y + y)):
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


## What this chunk's heavy drawing depends on: its tiles and those around
## it as far as any of its parts reads (the brooks' lines, the farthest).
## The same signature means the ground canvases, the bank's field, the
## brooks and the decorations are as they were: only the items change.
func _terrain_signature() -> int:
	if tiles_hash.is_valid():
		# The chunk and its eight neighbours cover all it reads (the
		# brooks' margin is under a chunk's side): their tiles' hashes.
		var key := Vector2i(floori(float(_origin.x) / _size), floori(float(_origin.y) / _size))
		var parts := PackedInt64Array()
		for dy in range(-1, 2):
			for dx in range(-1, 2):
				parts.append(int(tiles_hash.call(key + Vector2i(dx, dy))))
		return maxi(hash(parts), 1)
	var codes := PackedStringArray()
	var m := BrookLines.MARGIN
	for y in range(-m, _size + m):
		for x in range(-m, _size + m):
			codes.append(_terrain(_origin.x + x, _origin.y + y))
	return hash(codes)


var _last_signature := 0
## Building a chunk's ground takes tens of milliseconds: one chunk builds
## a frame, the others keep what they drew and wait in `waiting` for the
## world view to call them, nearest the camera first.
static var _built_frame := -1
static var waiting: Array = []
## The chunk's own drawing kept between redraws: flat tiles [points,
## colour] and decorations [texture, rect, tint].
var _flats: Array = []
var _decor_cache: Array = []


## The chunk's plants, from the flora's atlas (art/flora, made by
## content/tools/flora_art.py): one image a plant, its foot on its place,
## sized by its kind's height. One layer a chunk, drawn once, above the
## ground and below the trees.
func show_plants(plants: Array, flora: Array) -> void:
	var layer: PlantLayer = get_node_or_null("Plants")
	if layer == null:
		layer = PlantLayer.new()
		layer.name = "Plants"
		# The atlas is read with its mipmaps: small plants far away stay
		# calm (the ground itself is read without).
		layer.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		add_child(layer)
	layer.set_plants(plants, flora)


const FLORA_ATLAS := "res://art/flora/atlas.png"
const FLORA_RECTS := "res://art/flora/atlas.json"
## From this height, in metres, a plant stands among the trees and the
## bodies, sorted by its foot (WorldView: a bramble hides a body behind
## it); shorter ones lie on the ground's own layer.
const PLANT_STANDING_M := 0.9
## The world's scale (D40): under 3 m everything is drawn at its true
## height; above, heights ease towards reality (Scale.shown). Small
## plants, which the light grass swallows, are drawn a little brighter
## (SMALL_LIGHT, below SMALL_M).
const SMALL_M := 0.7
const SMALL_LIGHT := Color(1.18, 1.18, 1.1)
static var _flora_atlas: Texture2D
static var _flora_rects: Dictionary = {}


## The flora's atlas and its rectangles ("<id>_<nn>" -> Rect2).
static func flora_atlas() -> Texture2D:
	if _flora_atlas == null and ResourceLoader.exists(FLORA_ATLAS):
		_flora_atlas = load(FLORA_ATLAS)
		var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(FLORA_RECTS))
		if parsed is Dictionary:
			for key: String in parsed:
				var r: Array = parsed[key]
				_flora_rects[key] = Rect2(r[0], r[1], r[2], r[3])
	return _flora_atlas


## The atlas rectangles of a kind's images, `_01`, `_02`, ...
static func flora_variants(id: String) -> Array:
	flora_atlas()
	var found: Array = []
	var n := 1
	while _flora_rects.has("%s_%02d" % [id, n]):
		found.append(_flora_rects["%s_%02d" % [id, n]])
		n += 1
	return found


## The height a plant of `kind` is drawn at, in metres (SCALE_POWER).
static func shown_height(kind: Dictionary) -> float:
	return Scale.shown(float(kind.get("height_cm", 30)) / 100.0)


## The tint a plant of `kind` is drawn with: brighter when small.
static func plant_tint(kind: Dictionary) -> Color:
	return SMALL_LIGHT if shown_height(kind) < SMALL_M else Color.WHITE


## Whether a plant of `kind` stands among the trees (PLANT_STANDING_M).
static func plant_stands(kind: Dictionary) -> bool:
	return shown_height(kind) >= PLANT_STANDING_M


## The atlas region a plant of `kind` at `at` takes (its variant).
static func plant_region(at: Vector2, kind: Dictionary) -> Rect2:
	var variants := flora_variants(str(kind.get("id", "")))
	if variants.is_empty():
		return Rect2()
	return variants[int(Trees.place_hash(at, 9) * variants.size()) % variants.size()]


## A plant standing among the trees and bodies: placed at its foot, so
## the standing layer sorts it by depth.
class PlantNode:
	extends Node2D
	var region := Rect2()
	var rect := Rect2()

	func _draw() -> void:
		var atlas := ChunkView.flora_atlas()
		if atlas != null:
			draw_texture_rect_region(atlas, Rect2(rect.position - position, rect.size), region)


## Where a plant of `kind` at `at` (millimetres) is drawn on screen, with
## the variant `variants[...]` its place picks: the image's bottom middle
## on its foot.
static func plant_rect(at: Vector2, kind: Dictionary, region: Rect2) -> Rect2:
	var metres := shown_height(kind)
	var scale := 1.0 + (Trees.place_hash(at, 5) * 2.0 - 1.0) * 0.12
	var h := metres * Iso.PX_PER_M * scale
	var w := h * region.size.x / region.size.y
	var foot := Iso.point_to_screen(at)
	return Rect2(foot - Vector2(w / 2.0, h), Vector2(w, h))


class PlantLayer:
	extends Node2D
	## [texture rect, atlas region], nearest the back first.
	var _draws: Array = []

	func set_plants(plants: Array, flora: Array) -> void:
		_draws.clear()
		var sorted := plants.duplicate()
		# Drawn from the back to the front (the screen's depth is x + y).
		sorted.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
			return int(a.get("x", 0)) + int(a.get("y", 0)) < int(b.get("x", 0)) + int(b.get("y", 0)))
		for plant: Dictionary in sorted:
			var k := int(plant.get("kind", -1))
			if k < 0 or k >= flora.size():
				continue
			var kind: Dictionary = flora[k]
			if ChunkView.plant_stands(kind):
				continue
			var at := Vector2(float(plant.get("x", 0)), float(plant.get("y", 0)))
			var region := ChunkView.plant_region(at, kind)
			if region.size == Vector2.ZERO:
				continue
			_draws.append([ChunkView.plant_rect(at, kind, region), region, ChunkView.plant_tint(kind)])
		queue_redraw()

	func _draw() -> void:
		var atlas := ChunkView.flora_atlas()
		if atlas == null:
			return
		for d: Array in _draws:
			draw_texture_rect_region(atlas, d[0], d[1], d[2])


## Whether the chunk has built its ground at least once.
func is_built() -> bool:
	return _last_signature != 0


func _draw() -> void:
	DevTiming.tally("chunk redraws")
	var signature := _terrain_signature()
	if signature != _last_signature or not shore_field_on():
		var frame := Engine.get_process_frames()
		if _built_frame != frame:
			_built_frame = frame
			waiting.erase(self)
			_last_signature = signature
			_flats.clear()
			_decor_cache.clear()
			var t := DevTiming.start()
			_draw_ground_layers()
			DevTiming.add("chunk builds", t)
		elif not waiting.has(self):
			waiting.append(self)
	for flat: Array in _flats:
		draw_colored_polygon(flat[0], flat[1])
	for d: Array in _decor_cache:
		draw_texture_rect(d[0], d[1], false, d[2])
	_draw_items()


## The ground: textured tiles to their terrain's canvas, flat ones and the
## decorations to the caches, the bank and the brooks to their canvases.
func _draw_ground_layers() -> void:
	# Textured tiles go to their terrain's canvas; the rest is flat here.
	var by_terrain: Dictionary = {}
	for y in _size:
		for x in _size:
			var tx := _origin.x + x
			var ty := _origin.y + y
			var terrain := _ground_terrain(tx, ty)
			if ground_texture(terrain) != null:
				if not by_terrain.has(terrain):
					by_terrain[terrain] = PackedVector2Array()
				by_terrain[terrain].append(Vector2(tx, ty))
			else:
				_flats.append([Iso.diamond(tx, ty), COLORS.get(terrain, Color.MAGENTA)])
	# A terrain that spills may need a canvas here even without a tile here.
	var spills: Dictionary = {}
	for terrain in FRINGE_OVER:
		if ground_texture(terrain) != null:
			var f := fringe(terrain)
			if not f.is_empty():
				spills[terrain] = f
	if shore_field_on():
		_add_field_tiles(by_terrain)
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
	# A brook cuts the bank where it joins a river: drawn over it.
	_draw_brooks()
	_draw_decor()


## Items: a small disc of the material's colour, slightly off the tile's
## centre so several on one tile stay visible.
## Segments of an item's disc, as Godot draws a circle.
const ITEM_SEGMENTS := 64
## The mesh of the chunk's items, kept while it is drawn (a mesh freed at
## the end of `_draw` draws nothing).
var _items_mesh: ArrayMesh


func _draw_items() -> void:
	# Items: a small disc of the material's colour, slightly off the
	# tile's centre so several on one tile stay visible, outlined. All of
	# a chunk's discs go into one mesh, drawn at once: drawn one by one
	# they were two draw calls each, most of a frame's (from the
	# performance audit, 9 November 2026).
	var points := PackedVector2Array()
	var colours := PackedColorArray()
	var indices := PackedInt32Array()
	var ring := PackedVector2Array()
	for i in ITEM_SEGMENTS:
		var angle := i * TAU / ITEM_SEGMENTS
		ring.append(Vector2(cos(angle), sin(angle)))
	var outline := Color(0, 0, 0, 0.6)
	var icons: Array[Vector2] = []
	var on_tile: Dictionary = {}
	for item in _items:
		var tile := Vector2i(int(item.get("x", 0)), int(item.get("y", 0)))
		var nth: int = on_tile.get(tile, 0)
		on_tile[tile] = nth + 1
		var centre := Iso.tile_to_screen(tile.x, tile.y) + Vector2(-8 + 8 * (nth % 3), 3 - 5 * (nth / 3))
		var material := str(item.get("material", ""))
		var colour: Color = _looks.get(material, Color.MAGENTA)
		# The disc: a fan from its first point, as draw_circle makes it.
		var base := points.size()
		for d in ring:
			points.append(centre + d * ITEM_RADIUS)
			colours.append(colour)
		for i in ITEM_SEGMENTS - 2:
			indices.append_array([base, base + i + 1, base + i + 2])
		# Its outline: a band a pixel wide across the rim.
		base = points.size()
		for d in ring:
			points.append(centre + d * (ITEM_RADIUS + 0.5))
			points.append(centre + d * (ITEM_RADIUS - 0.5))
			colours.append(outline)
			colours.append(outline)
		for i in ITEM_SEGMENTS:
			var j := (i + 1) % ITEM_SEGMENTS
			indices.append_array([base + 2 * i, base + 2 * i + 1, base + 2 * j, base + 2 * j, base + 2 * i + 1, base + 2 * j + 1])
		if material == "corpse":
			icons.append(centre + Vector2(0, -16))
	if points.is_empty():
		_items_mesh = null
		return
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = points
	arrays[Mesh.ARRAY_COLOR] = colours
	arrays[Mesh.ARRAY_INDEX] = indices
	_items_mesh = ArrayMesh.new()
	_items_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	draw_mesh(_items_mesh, null)
	for at in icons:
		draw_death_icon(self, at)


## Whether the bank is drawn by the ground shader from the distance to the
## water (shore mode "field").
static func shore_field_on() -> bool:
	return str(ground_spec().get("shore", {}).get("mode", "")) == "field"


## For the bank drawn from the distance field: writes this chunk's block
## of the field, gives the water material the field, and lets each side
## reach across the tiles' edge so the smoothed line can fall anywhere:
## the land beneath under the water tiles touching land, the water (which
## draws the bank) over the land within two tiles of the water.
func _add_field_tiles(by_terrain: Dictionary) -> void:
	var terrain := func(t: Vector2i) -> String: return _terrain(t.x, t.y)
	ShoreField.update(terrain, _origin, _size, current_at, true)
	var water_material := ground_material("water")
	water_material.set_shader_parameter("shore_field", ShoreField.texture())
	water_material.set_shader_parameter("has_field", true)
	water_material.set_shader_parameter("field_tiles", float(ShoreField.TILES))
	water_material.set_shader_parameter("field_range", ShoreField.RANGE)
	if ground_texture("water") == null:
		return
	# The terrains over the chunk and two tiles around, read once, and
	# which tiles have water within two tiles, row by row then across.
	var m := 2
	var span := _size + 2 * m
	var grid := PackedStringArray()
	grid.resize(span * span)
	var water := PackedByteArray()
	water.resize(span * span)
	for j in span:
		for i in span:
			var t := _terrain(_origin.x + i - m, _origin.y + j - m)
			grid[j * span + i] = t
			if ShoreField.is_water(t):
				water[j * span + i] = 1
	var along := PackedByteArray()
	along.resize(span * span)
	for j in span:
		for i in range(m, span - m):
			for d in range(-m, m + 1):
				if water[j * span + i + d]:
					along[j * span + i] = 1
					break
	for y in _size:
		for x in _size:
			var tx := _origin.x + x
			var ty := _origin.y + y
			var here := grid[(y + m) * span + x + m]
			if here == "water" or here == "ford":
				# The land under it, where land is near: forest if the land
				# around is mostly forest, else grass.
				var forest := 0
				var grass := 0
				for dy in range(-1, 2):
					for dx in range(-1, 2):
						var near := grid[(y + m + dy) * span + x + m + dx]
						if near == "brook":
							near = _ground_terrain(tx + dx, ty + dy)
						if near == "forest":
							forest += 1
						elif near == "grass":
							grass += 1
				if forest + grass > 0:
					var land := "forest" if forest > grass else "grass"
					if ground_texture(land) != null:
						if not by_terrain.has(land):
							by_terrain[land] = PackedVector2Array()
						by_terrain[land].append(Vector2(tx, ty))
			elif not ShoreField.is_water(here) and here != "":
				var near_water := false
				for dy in range(-m, m + 1):
					if along[(y + m + dy) * span + x + m]:
						near_water = true
						break
				if near_water:
					if not by_terrain.has("water"):
						by_terrain["water"] = PackedVector2Array()
					by_terrain["water"].append(Vector2(tx, ty))


## The ground drawn on tile (tx, ty): its terrain, except under a brook,
## which is a line on the ground (BrookLines): there, the land around it,
## forest where the tiles around are mostly forest, else grass.
func _ground_terrain(tx: int, ty: int) -> String:
	var terrain := _terrain(tx, ty)
	if terrain == "ford" and shore_field_on():
		# A ford is shallow water: the water's shader draws it (ShoreField).
		return "water"
	if terrain != "brook":
		return terrain
	var forest := 0
	var grass := 0
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			var near := _terrain(tx + dx, ty + dy)
			if near == "forest":
				forest += 1
			elif near == "grass":
				grass += 1
	return "forest" if forest > grass else "grass"


## The brooks crossing this chunk, as lines on the ground (BrookLines):
## found over a margin around the chunk, so they join the neighbours'
## without a seam; each segment is drawn by the chunk under its middle.
func _draw_brooks() -> void:
	var near := false
	for y in range(-2, _size + 2):
		for x in range(-2, _size + 2):
			if _terrain(_origin.x + x, _origin.y + y) == "brook":
				near = true
				break
		if near:
			break
	if not near:
		return
	var margin := Vector2i(BrookLines.MARGIN, BrookLines.MARGIN)
	var terrain := func(t: Vector2i) -> String: return _terrain(t.x, t.y)
	var lines := BrookLines.lay(terrain, _origin - margin, _origin + Vector2i(_size, _size) + margin)
	var tint: Color = ground_params("water").get("base_tint", Color.WHITE)
	var bed := str(ground_layers("water").get("brook1", ""))
	if shore_field_on() and bed != "":
		var canvas := _brook_canvas()
		canvas.lines = lines
		canvas.lo = _origin
		canvas.hi = _origin + Vector2i(_size, _size)
		canvas.water = ground_texture("water")
		canvas.queue_redraw()
		return
	BrookLines.draw(self, lines, _origin, _origin + Vector2i(_size, _size), ground_texture("water"), tint)


## The brook shader's material, shared by every chunk.
static var _brook_material: ShaderMaterial


## The child canvas drawing this chunk's brooks with the brook shader,
## above the grounds and the river's bank, below the decorations.
func _brook_canvas() -> BrookCanvas:
	if _brooks == null:
		if _brook_material == null:
			_brook_material = ShaderMaterial.new()
			_brook_material.shader = load("res://world/brook.gdshader")
			_brook_material.set_shader_parameter("noise_texture", noise_texture())
			_brook_material.set_shader_parameter("lateral_range", BrookLines.LATERAL_RANGE)
			var layers := ground_layers("water")
			for k in 3:
				_brook_material.set_shader_parameter("bed%d_texture" % (k + 1), load_texture(str(layers.get("brook%d" % (k + 1), ""))))
			_brook_material.set_shader_parameter("earth_texture", load_texture(str(layers.get("bank", ""))))
			_brook_material.set_shader_parameter("water_tint", ground_params("water").get("base_tint", Color.WHITE))
			# Where the river's water begins, so a brook fades into it.
			_brook_material.set_shader_parameter("shore_field", ShoreField.texture())
			_brook_material.set_shader_parameter("field_tiles", float(ShoreField.TILES))
			_brook_material.set_shader_parameter("field_range", ShoreField.RANGE)
		_brooks = BrookCanvas.new()
		_brooks.z_index = -3
		_brooks.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		_brooks.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		_brooks.material = _brook_material
		add_child(_brooks)
	return _brooks


var _brooks: BrookCanvas = null


## Draws a chunk's brooks with the brook shader (BrookLines.draw_shaded).
class BrookCanvas:
	extends Node2D
	var lines: Array = []
	var lo := Vector2i.ZERO
	var hi := Vector2i.ZERO
	var water: Texture2D

	func _draw() -> void:
		BrookLines.draw_shaded(self, lines, lo, hi, water)


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


func _draw_shore() -> void:
	# The bank is drawn by the ground shader from the distance to the water
	# (ShoreField, shore mode "field"); the bank's former modes (tiles,
	# isometric pieces, brushes, curves, generated pieces) were removed on
	# 9 November 2026, their images kept outside the repository.
	_set_fills({})


## Sows the decorations of this chunk: for each kind, on the tiles its
## "where" names (water within a tile of land; land within two tiles of
## water), with its chance, at a fixed random spot of the tile.
func _draw_decor() -> void:
	var decor: Dictionary = ground_spec().get("decor", {})
	if decor.is_empty():
		return
	var wanted: Dictionary = {}
	for kind in decor:
		if decor[kind] is Dictionary:
			wanted[str(decor[kind].get("where", ""))] = true
	var water_near_land := shore("water") if wanted.has("water_near_land") else {}
	# How near the water each land tile is, once for every kind (0 for
	# the tiles no decoration grows on).
	var land_near := PackedFloat32Array()
	land_near.resize(_size * _size)
	var any_land_near := false
	if wanted.keys().any(func(w: String) -> bool: return w != "water_near_land"):
		var land_near_water := _nearness(func(t: String) -> bool: return t == "water", 2)
		for y in _size:
			for x in _size:
				var terrain := str(_tiles[y * _size + x])
				if terrain == "water" or terrain == "ford" or terrain == "brook" or terrain == "rock":
					continue
				var near := _max4(_corners(land_near_water, x, y))
				land_near[y * _size + x] = near
				any_land_near = any_land_near or near > 0.0
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
		var by_water := str(rule.get("where", "")) == "water_near_land"
		if not by_water and not any_land_near:
			continue
		for y in _size:
			for x in _size:
				var tx := _origin.x + x
				var ty := _origin.y + y
				var terrain := str(_tiles[y * _size + x])
				var near := 0.0
				if by_water:
					if terrain != "water" or not water_near_land.has(Vector2i(tx, ty)):
						continue
					near = _max4(water_near_land[Vector2i(tx, ty)])
					if near < min_near:
						continue
				else:
					near = land_near[y * _size + x]
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
				_decor_cache.append([texture, Rect2(centre - size / 2.0, size), tint])


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
