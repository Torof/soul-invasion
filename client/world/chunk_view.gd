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
## terrain -> path of its ground texture under res://art/ground/
## The floor of the woods is the flattest grass in the shade, darker and
## cooler, with the sparse forest grass in a few plaques and bare earth.
## Plain soil does not come out of the generator; leaf litter will come
## later as a sister.
const GROUND_TEXTURES := {
	"grass": "res://art/ground/ground_grass_01_summer.png",
	"forest": "res://art/ground/ground_grass_02_summer.png",
}
## terrain -> compatible textures shown in soft patches over the first:
## "second" over wide areas (a drier grass), "worn" in a few spots.
## Sisters of the same grass (same style and tone, drawn differently) give
## the variety; tone variation is a tint in the shader, never another image.
const LAYER_TEXTURES := {
	"grass": {
		"sibling": "res://art/ground/ground_grass_03_summer.png",
		"second": "res://art/ground/ground_grass_04_summer.png",
		"worn": "res://art/ground/ground_grass_02_summer.png",
	},
	"forest": {
		"sibling": "res://art/ground/ground_grass_06_summer.png",
	},
}
## terrain -> the terrains it spills over at its edge, with a soft eroded
## border, so a wood's edge is not a line of diamonds. The server still
## decides every tile's terrain; this is only how the border is drawn.
const FRINGE_OVER := {"forest": ["grass"]}
## Width of the spill, in tiles.
const FRINGE := 2
## Drawing order of the textured terrains: the one that spills is above.
const GROUND_ORDER := {"grass": 0, "forest": 1}
## terrain -> shader parameters that differ from the shader's defaults
const GROUND_PARAMS := {
	"forest": {
		"base_tint": Color(0.56, 0.70, 0.74),
		"sibling_cover": 0.25,
		"has_variants": true,
		"bare_keep": 0.35,
		"dry_strength": 0.3,
	},
}
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
		var layers: Dictionary = LAYER_TEXTURES.get(terrain, {})
		for layer in layers:
			var path: String = layers[layer]
			if ResourceLoader.exists(path):
				material.set_shader_parameter(layer + "_texture", load(path))
				material.set_shader_parameter("has_" + layer, true)
		var params: Dictionary = GROUND_PARAMS.get(terrain, {})
		for name in params:
			material.set_shader_parameter(name, params[name])
		_ground_materials[terrain] = material
	return _ground_materials[terrain]


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

	func _draw() -> void:
		var white := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
		var texture := ChunkView.ground_texture(terrain)
		for tile in tiles:
			var tx := int(tile.x)
			var ty := int(tile.y)
			draw_polygon(Iso.diamond(tx, ty), white, ChunkView.ground_uvs(tx, ty), texture)
		for spill in fringe:
			var tile: Vector2i = spill["tile"]
			var alphas: PackedFloat32Array = spill["alphas"]
			var colours := PackedColorArray()
			for a in alphas:
				colours.append(Color(1, 1, 1, a))
			draw_polygon(Iso.diamond(tile.x, tile.y), colours, ChunkView.ground_uvs(tile.x, tile.y), texture)


## The ground texture of a terrain, or null when it has none yet.
static func ground_texture(terrain: String) -> Texture2D:
	if not _ground_cache.has(terrain):
		var path: String = GROUND_TEXTURES.get(terrain, "")
		_ground_cache[terrain] = load(path) if path != "" and ResourceLoader.exists(path) else null
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


## The tiles of this chunk that `terrain` spills over, with the opacity of
## the spill at each corner (clockwise from the top, like Iso.diamond): 1
## at a corner touching a tile of `terrain`, fading to 0 FRINGE + 1
## corners away.
func fringe(terrain: String) -> Array:
	var over: Array = FRINGE_OVER.get(terrain, [])
	if over.is_empty():
		return []
	# Corners of the chunk and a margin around it, in tile coordinates
	# relative to the origin: corner (cx, cy) is the top of tile (cx, cy).
	var lo := -FRINGE
	var hi := _size + FRINGE
	var inside: Dictionary = {}
	for cy in range(lo, hi + 1):
		for cx in range(lo, hi + 1):
			for d in [Vector2i(-1, -1), Vector2i(0, -1), Vector2i(-1, 0), Vector2i(0, 0)]:
				if _terrain(_origin.x + cx + d.x, _origin.y + cy + d.y) == terrain:
					inside[Vector2i(cx, cy)] = true
					break
	var alpha_of := func(c: Vector2i) -> float:
		var best := FRINGE + 1
		for dy in range(-FRINGE, FRINGE + 1):
			for dx in range(-FRINGE, FRINGE + 1):
				if inside.has(c + Vector2i(dx, dy)):
					best = mini(best, maxi(absi(dx), absi(dy)))
		return 1.0 - float(best) / float(FRINGE + 1)
	var result: Array = []
	for y in _size:
		for x in _size:
			if not over.has(str(_tiles[y * _size + x])):
				continue
			var alphas := PackedFloat32Array([
				alpha_of.call(Vector2i(x, y)),
				alpha_of.call(Vector2i(x + 1, y)),
				alpha_of.call(Vector2i(x + 1, y + 1)),
				alpha_of.call(Vector2i(x, y + 1)),
			])
			if alphas[0] > 0.0 or alphas[1] > 0.0 or alphas[2] > 0.0 or alphas[3] > 0.0:
				result.append({"tile": _origin + Vector2i(x, y), "alphas": alphas})
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
