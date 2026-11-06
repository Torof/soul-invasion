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
const LAYERS := ["sibling", "second", "worn", "bank"]
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
	"water": {"over": ["grass", "forest"], "width": 1},
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


## A small skull above a corpse, drawn on `canvas` at `at`.
static func draw_death_icon(canvas: CanvasItem, at: Vector2) -> void:
	canvas.draw_circle(at, 4.5, Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at, 4.5, Color.BLACK, false, 1.0)
	canvas.draw_rect(Rect2(at + Vector2(-2.5, 3), Vector2(5, 2.5)), Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at + Vector2(-1.7, -0.5), 1.1, Color.BLACK)
	canvas.draw_circle(at + Vector2(1.7, -0.5), 1.1, Color.BLACK)
