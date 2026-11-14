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
## holds one child canvas per textured terrain, under its own drawing,
## each covering only the rows of tiles it draws (GroundBuild.rows_of);
## the shader keeps the tiles of its terrain from the ground index
## (GroundIndex), which the chunk writes when it builds. Terrains without a texture keep a flat colour.
##
## The tiles are read as bytes (TerrainGrid), the chunk's own and its
## neighbours' around it, once a build (D39).

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
## Width, in tiles, over which water feels its own edge (shore(): the
## decorations that grow in the water by a bank).
const SHORE_WIDTH := 2
## Tiles read around a chunk when it builds: as far as the water meshes
## read (WaterMeshes.READ), which covers the spill and the land under the
## water.
const READ := WaterMeshes.READ
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


func _ready() -> void:
	add_to_group("chunk_views")


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE:
		# A ground under way is left to finish on its own.
		_drop_task()
		# The meshes go with the chunk: off the tally.
		water_bytes -= _water_bytes
		water_bytes_before -= _water_bytes_before
		_water_bytes = 0
		_water_bytes_before = 0
	# The shore brushes repeat along the bank.
	texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
## Where the water runs at any tile the world view knows (a vector in
## metres a second; set by the world view, invalid on the benches).
var current_at: Callable
## The hash of a chunk's tiles as received, 0 when not held (set by the
## world view): the signature of the ground below reads nine of them
## instead of three thousand tiles.
var tiles_hash: Callable
## The currents of a chunk as received (Vector2i -> PackedInt32Array; set by the
## world view): a worker reads a copy of those around the chunk rather
## than the world view's own (`current_at`).
var currents_of: Callable


## The seamless noise every ground material blends with, made here, no
## image needed.
static func noise_texture() -> Texture2D:
	if _noise_texture == null:
		var texture := NoiseTexture2D.new()
		texture.noise = noise_source()
		texture.seamless = true
		texture.width = 256
		texture.height = 256
		_noise_texture = texture
	return _noise_texture


## The noise itself (the ground shader's, and what the water meshes read
## of it on the processor).
static func noise_source() -> FastNoiseLite:
	var noise := FastNoiseLite.new()
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	noise.frequency = 0.02
	noise.seed = 7
	return noise


## The bank's detail for the water meshes' shaders: one seamless image of
## four metres, each channel two octaves of the ground's noise at the
## scales ground.gdshader read them (R the grass edge's rag, G its tufts,
## B the water's lap, A the ripples' grain), so one read gives the four.
## Made once from the noise and kept in the user folder: a million pixels
## take a second to pack in a script.
const DETAIL_SIDE := 1024
const DETAIL_CACHE := "user://water_detail_%d.png" % DETAIL_SIDE
static var _detail_texture: Texture2D


static func detail_texture() -> Texture2D:
	if _detail_texture == null:
		var image: Image = Image.load_from_file(ProjectSettings.globalize_path(DETAIL_CACHE)) if FileAccess.file_exists(DETAIL_CACHE) else null
		if image == null or image.get_width() != DETAIL_SIDE:
			image = make_detail_image()
			image.save_png(DETAIL_CACHE)
		image.generate_mipmaps()
		_detail_texture = ImageTexture.create_from_image(image)
	return _detail_texture


## The detail image: per channel, the two scales (texture units a tile of
## the ground's noise, rounded so each repeats within the four metres)
## and the second's weight against the first's.
const DETAIL_CHANNELS := [[0.5, 1.5, 0.5], [2.0, 4.25, 0.714], [1.25, 3.75, 0.5], [1.0, 2.25, 0.833]]


static func make_detail_image() -> Image:
	var side := DETAIL_SIDE
	var planes: Array[PackedByteArray] = []
	for spec: Array in DETAIL_CHANNELS:
		var noise := noise_source()
		# The image spans four tiles: a feature of the noise at scale s
		# covers 0.2 / s tiles, so s / 20 of the image's side... in pixels of
		# the image, the noise's frequency is 0.02 × s × 256 / (side / 4).
		noise.frequency = 0.02 * float(spec[0]) * 256.0 / (float(side) / 4.0)
		noise.fractal_type = FastNoiseLite.FRACTAL_FBM
		noise.fractal_octaves = 2
		noise.fractal_lacunarity = float(spec[1]) / float(spec[0])
		noise.fractal_gain = float(spec[2])
		var plane := noise.get_seamless_image(side, side)
		plane.convert(Image.FORMAT_L8)
		planes.append(plane.get_data())
	var data := PackedByteArray()
	data.resize(side * side * 4)
	for i in side * side:
		var o := 4 * i
		data[o] = planes[0][i]
		data[o + 1] = planes[1][i]
		data[o + 2] = planes[2][i]
		data[o + 3] = planes[3][i]
	return Image.create_from_data(side, side, false, Image.FORMAT_RGBA8, data)


## The ground material of a terrain: the blending shader with the
## terrain's layers. Shared by every chunk. A layer whose image is not
## there yet is simply off.
static func ground_material(terrain: String = "grass") -> ShaderMaterial:
	if not _ground_materials.has(terrain):
		var material := ShaderMaterial.new()
		material.shader = load("res://world/ground.gdshader")
		material.set_shader_parameter("noise_texture", noise_texture())
		material.set_shader_parameter("ground_index", GroundIndex.texture())
		material.set_shader_parameter("index_bit", int(GroundIndex.BITS.get(terrain, 0)))
		material.set_shader_parameter("index_tiles", float(GroundIndex.TILES))
		material.set_shader_parameter("slope_index", HeightIndex.texture())
		if terrain != "water":
			material.set_shader_parameter("steps_mode", steps_mode)
			material.set_shader_parameter("slope_rim", slope_rim)
			for dial: String in SLOPE_LOOK:
				material.set_shader_parameter(dial, _slope_dial(dial))
		apply_spec(material, terrain, false)
		pin_flow_time(material)
		_ground_materials[terrain] = material
	return _ground_materials[terrain]


## The slope test's mode (StepsTest): 1 the land's grounds draw the
## slopes where the ground slopes, and read the meadow at random
## offsets against repetition (the mountain test's rule), 0 the game's
## ground as it is (any world with no relief).
static var steps_mode := 0
## The slopes' look (the one rule the project lead validated on
## 14 November 2026; ground.gdshader, slope_lit): the two images and the
## dials, each a shader parameter of the same name, so another biome can
## bring its own look without touching the shader. An image is given by
## its path. slope_break_turn: the streaks' turn, in degrees, on a side
## whose crest lies below; slope_streak_mix and slope_streak_pale: how
## much the streaks' light pales the turf, and toward what;
## slope_turf_px: the turf's repetition on the canvas, in pixels.
const SLOPE_LOOK := {
	"slope_streaks": "res://world/test/slope_streaks.png",
	"slope_turf": "res://world/test/slope_turf.png",
	"slope_break_turn": 165.0,
	"slope_streak_mix": 0.2,
	"slope_streak_pale": Vector3(0.92, 0.94, 0.59),
	"slope_turf_px": 768.0,
}
## The turf with tufts, bushes and stones (the lead's grass of
## 13 November 2026) laid instead of the look's (key G).
const SLOPE_TURF_BUSHES := "res://world/test/slope_turf_bushes.png"
## The mountain test: the rim a near face lays over the face behind a crest (key R).
static var slope_rim := true
## The mountain test: the turf with bushes and stones under the streaks (key G).
static var slope_turf_with_bushes := false


## The value of the look's dial `name` for the shader: the image loaded,
## the turf the one with bushes when it is asked for.
static func _slope_dial(name: String) -> Variant:
	if name == "slope_turf" and slope_turf_with_bushes:
		return load(SLOPE_TURF_BUSHES)
	var value: Variant = SLOPE_LOOK[name]
	return load(value) if value is String else value


## Sets the slope shader parameter `name` of every land's ground to
## `value` (the slope test's keys).
static func set_slope_param(name: String, value: Variant) -> void:
	for terrain: String in _ground_materials:
		if terrain != "water":
			(_ground_materials[terrain] as ShaderMaterial).set_shader_parameter(name, value)


## Key G: the turf with bushes (true) or the look's turf.
static func set_slope_turf_with_bushes(on: bool) -> void:
	slope_turf_with_bushes = on
	set_slope_param("slope_turf", _slope_dial("slope_turf"))


static func set_steps_mode(mode: int) -> void:
	steps_mode = mode
	set_slope_param("steps_mode", mode)


## Development: with SOUL_INVASION_WATER_TIME set to a number of seconds,
## the water of `material` (the ground's or the brooks') stands at that
## moment of its drift instead of running, so two captures compare pixel
## by pixel (as SOUL_INVASION_FOG_TIME does for the fog).
static func pin_flow_time(material: ShaderMaterial) -> void:
	var pinned := OS.get_environment("SOUL_INVASION_WATER_TIME")
	if pinned.is_valid_float():
		material.set_shader_parameter("flow_time", float(pinned))


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
	for material: ShaderMaterial in [_water_mesh_material, _water_bank_material]:
		if material != null:
			materials.append(material)
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
		# Seamless textures repeat across the chunk; read with their
		# mipmaps, a level sharper (the shader), so the zoom never shimmers.
		canvas.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		canvas.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		canvas.material = ground_material(terrain)
		add_child(canvas)
		_grounds[terrain] = canvas
	return canvas


## One terrain's ground over a chunk: one array of triangles with that
## terrain's material, covering the rows of tiles it draws (GroundBuild.
## rows_of), whose shader keeps, as a safety net, the tiles the ground
## index gives this terrain (GroundIndex).
class GroundCanvas:
	extends Node2D
	var terrain := ""
	## The chunk covered: its north-west tile and its side.
	var origin := Vector2i.ZERO
	var size := 0
	## The tiles drawn, rectangles [x0, y0, x1, y1) in the chunk.
	var rows := PackedInt32Array()
	## Whether the terrain has any tile to draw in the chunk.
	var shown := false

	func _draw() -> void:
		if not shown or size <= 0 or rows.is_empty():
			return
		var points := PackedVector2Array()
		var uvs := PackedVector2Array()
		var indices := PackedInt32Array()
		for r in range(0, rows.size(), 4):
			var lo := origin + Vector2i(rows[r], rows[r + 1])
			var hi := origin + Vector2i(rows[r + 2], rows[r + 3])
			var first := points.size()
			for c: Vector2i in [lo, Vector2i(hi.x, lo.y), hi, Vector2i(lo.x, hi.y)]:
				# A tile's top corner, and its square of the texture.
				points.append(Iso.tile_to_screen(c.x, c.y) + Vector2(0, -Iso.TILE_H / 2.0))
				uvs.append(Vector2(c) / ChunkView.GROUND_METRES)
			indices.append_array([first, first + 1, first + 2, first, first + 2, first + 3])
		var colours := PackedColorArray()
		colours.resize(points.size())
		colours.fill(Color.WHITE)
		var texture := ChunkView.ground_texture(terrain)
		RenderingServer.canvas_item_add_triangle_array(get_canvas_item(), indices, points, colours, uvs,
				PackedInt32Array(), PackedFloat32Array(), texture.get_rid() if texture != null else RID())


## The river as two meshes a chunk (WaterMeshes, D36): a water mesh and a
## bank mesh, built on a worker thread when the chunk builds, drawn under
## the chunk's own drawing and over the grounds; the brooks a mesh of
## their own over them (BrookLines.mesh).
static var _water_mesh_material: ShaderMaterial
static var _water_bank_material: ShaderMaterial
## The water mesh's time on the worker and the main thread, in microseconds,
## and the builds done (the F3 report).
static var water_build_usec := 0
static var water_builds := 0
static var water_main_usec := 0
## What the water meshes held cost the card, in bytes (the F3 report),
## and what they weighed before they were packed and merged (D39's
## record: the same cells as quarter quads, float colours, 32-bit
## indices).
static var water_bytes := 0
static var water_bytes_before := 0
## Development: SOUL_INVASION_WATER_ARROWS=1 draws, over each brook, the
## current its vertices carry, an arrow a point of the ribbon.
static var show_arrows := OS.get_environment("SOUL_INVASION_WATER_ARROWS") == "1"
## The brooks' arrows of this chunk: [screen point, flow] pairs.
var _arrows: Array = []
var _water_canvas: WaterCanvas = null
var _bank_canvas: WaterCanvas = null
## The water meshes' bytes this chunk holds, and before packing.
var _water_bytes := 0
var _water_bytes_before := 0
## The latest build asked for: an earlier one still on a worker is dropped.


## The water mesh's material (deep water) and the bank mesh's, shared by
## every chunk; the water's dials and layers from ground.json.
static func water_mesh_material() -> ShaderMaterial:
	if _water_mesh_material == null:
		_water_mesh_material = _make_water_material("res://world/water_mesh.gdshader")
	return _water_mesh_material


static func water_bank_material() -> ShaderMaterial:
	if _water_bank_material == null:
		_water_bank_material = _make_water_material("res://world/water_bank.gdshader")
	return _water_bank_material


static func _make_water_material(shader_path: String) -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = load(shader_path)
	apply_water_spec(material, false)
	pin_flow_time(material)
	return material


## Gives a water mesh material its layers and numbers: the bank's earth
## and sand, the first ford bed, the current's streaks, the detail, and
## the water's params.
static func apply_water_spec(material: ShaderMaterial, fresh: bool) -> void:
	var layers := ground_layers("water")
	for pair: Array in [["bank", "earth_texture", ""], ["bank2", "sand_texture", "has_sand"], ["ford1", "bed_texture", "has_bed"], ["streaks", "streak_texture", "has_streaks"]]:
		var path := str(layers.get(pair[0], ""))
		var texture: Texture2D = null
		if path != "":
			texture = fresh_texture(path) if fresh else load_texture(path)
		if texture != null:
			material.set_shader_parameter(pair[1], texture)
		if pair[2] != "":
			material.set_shader_parameter(pair[2], texture != null and (pair[2] != "has_streaks" or water_streaks))
	material.set_shader_parameter("detail_texture", detail_texture())
	material.set_shader_parameter("depth_tiles", WaterMeshes.DEPTH_TILES)
	var params := ground_params("water")
	for name in params:
		material.set_shader_parameter(name, params[name])


## The streaks of the current over the meshes' water (the lead's look,
## ground.json: layers.streaks), off and on to measure them (F11).
static var water_streaks := true


static func set_water_streaks(on: bool) -> void:
	water_streaks = on
	for material: ShaderMaterial in [water_mesh_material(), water_bank_material()]:
		material.set_shader_parameter("has_streaks", on and material.get_shader_parameter("streak_texture") != null)


## Development: the water hidden or not (the F8 measure: a frame
## without the water, against one with it).
func set_water_hidden(hidden: bool) -> void:
	for canvas: Node2D in [_water_canvas, _bank_canvas, _brooks]:
		if canvas != null:
			canvas.visible = not hidden


## The zoom changed: the water meshes' images are read at an integer
## mipmap level a zoom step (one level sharper than the hardware would
## choose for the diamonds, D39) and the bank's detail is off where its
## shapes would be under a few pixels. `stretch` is the screen's pixels
## per logical pixel.
static func set_water_zoom(zoom: float, stretch: float) -> void:
	# Along the screen's rows a tile is 32 × zoom × stretch pixels high and
	# a texture's 256 texels a tile run across it: this many texels a pixel.
	var texels := 8.0 / maxf(zoom * stretch, 0.01)
	var lod := maxf(roundf(log(texels) / log(2.0) - 1.0), 0.0)
	for material: ShaderMaterial in [water_mesh_material(), water_bank_material()]:
		material.set_shader_parameter("water_lod", lod)
		material.set_shader_parameter("ground_lod", lod)
		material.set_shader_parameter("detail_on", 1.0 if zoom >= 0.8 else 0.0)


## A mesh of the water drawn with one material (the water's, the
## bank's or the brooks'), under the chunk's own drawing and over the
## grounds. The arrays (points, uvs when the shader reads them, colours,
## a strip and/or triangles) go to the card as one or two surfaces: the
## colours as bytes, the indices 16-bit.
class WaterCanvas:
	extends Node2D
	var mesh: ArrayMesh = null
	var texture: Texture2D = null

	## Lays `arrays` (empty: nothing) as the mesh drawn.
	func lay(arrays: Dictionary) -> void:
		mesh = null
		if not arrays.is_empty() and not (arrays["points"] as PackedVector2Array).is_empty():
			mesh = ArrayMesh.new()
			for pair: Array in [["strip", Mesh.PRIMITIVE_TRIANGLE_STRIP], ["indices", Mesh.PRIMITIVE_TRIANGLES]]:
				var indices: PackedInt32Array = arrays.get(pair[0], PackedInt32Array())
				if indices.is_empty():
					continue
				var surface: Array = []
				surface.resize(Mesh.ARRAY_MAX)
				surface[Mesh.ARRAY_VERTEX] = arrays["points"]
				if not (arrays["uvs"] as PackedVector2Array).is_empty():
					surface[Mesh.ARRAY_TEX_UV] = arrays["uvs"]
				surface[Mesh.ARRAY_COLOR] = arrays["colours"]
				surface[Mesh.ARRAY_INDEX] = indices
				mesh.add_surface_from_arrays(pair[1], surface)
		queue_redraw()

	func _draw() -> void:
		if mesh != null:
			draw_mesh(mesh, texture)


func _water_canvases() -> Array:
	if _water_canvas == null:
		_water_canvas = WaterCanvas.new()
		_water_canvas.name = "WaterMesh"
		# Over the grounds, under the brooks.
		_water_canvas.z_index = -6 + int(GROUND_ORDER["water"])
		_water_canvas.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		_water_canvas.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		_water_canvas.material = water_mesh_material()
		_water_canvas.texture = ground_texture("water")
		add_child(_water_canvas)
		_bank_canvas = WaterCanvas.new()
		_bank_canvas.name = "BankMesh"
		_bank_canvas.z_index = _water_canvas.z_index
		_bank_canvas.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		_bank_canvas.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		_bank_canvas.material = water_bank_material()
		_bank_canvas.texture = ground_texture("water")
		add_child(_bank_canvas)
	return [_water_canvas, _bank_canvas]


## Lays the water meshes (WaterMeshes.build) on the canvases, and counts
## their bytes.
func _lay_water_meshes(water: Dictionary, bank: Dictionary) -> void:
	water_bytes -= _water_bytes
	water_bytes_before -= _water_bytes_before
	_water_bytes = int(water.get("bytes", 0)) + int(bank.get("bytes", 0))
	_water_bytes_before = int(water.get("bytes_before", 0)) + int(bank.get("bytes_before", 0))
	water_bytes += _water_bytes
	water_bytes_before += _water_bytes_before
	if water.is_empty() and bank.is_empty() and _water_canvas == null:
		return
	var canvases := _water_canvases()
	(canvases[0] as WaterCanvas).lay(water)
	(canvases[1] as WaterCanvas).lay(bank)


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
	# The water meshes' shaders likewise (their include is read by the
	# shader's own loader from the file beside it).
	for pair: Array in [[_water_mesh_material, "res://world/water_mesh.gdshader"], [_water_bank_material, "res://world/water_bank.gdshader"]]:
		var material: ShaderMaterial = pair[0]
		if material == null:
			continue
		var fresh := Shader.new()
		fresh.code = FileAccess.get_file_as_string(pair[1])
		material.shader = fresh
		apply_water_spec(material, true)
		pin_flow_time(material)


## A texture through the import when it has one, else straight from the
## image file (an image added today, not yet imported): never null for a
## file that is there.
## Decorations: small generated images (tufts, flowers and pebbles on
## the bank) sown by a fixed hash of the tile, so
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
			var path := "%s%s_%02d.webp" % [DECOR_DIR, kind, n]
			if ResourceLoader.exists(path) or FileAccess.file_exists(path):
				var texture := load_texture(path)
				if texture != null:
					found.append(texture)
		_decor[kind] = found
	return _decor[kind]


## A fixed number in [0, 1) for a tile and a salt: the same on every
## client, every time ([StableHash]).
static func tile_hash(tx: int, ty: int, salt: int) -> float:
	return StableHash.unit([tx, ty, salt])


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
## The chunk's tiles, as codes (TerrainGrid).
var _codes := PackedByteArray()
## [{"x", "y", "material"}] as received
var _items: Array = []
## material id -> Color
var _looks: Dictionary = {}


## Sets the chunk's tiles (as received, names, or as codes) and items;
## `origin` is its north-west tile. The tiles are laid in TerrainGrid,
## where the neighbours read them.
func show_chunk(origin: Vector2i, size: int, tiles: Variant, items: Array, looks: Dictionary) -> void:
	_origin = origin
	_size = size
	_codes = TerrainGrid.codes(tiles)
	TerrainGrid.write(origin, size, _codes)
	_looks = looks
	# The items' mesh is built when the items change (an item picked up
	# or laid), never on a redraw: the chunk is redrawn whenever a
	# neighbour comes or goes, and 64-segment discs for every branch and
	# pebble cost seconds a teleport (10 November 2026).
	var signature := items.hash()
	if signature != _items_signature:
		_items = items
		_items_signature = signature
		_build_items()
	queue_redraw()


## The codes of the chunk and READ tiles around it (TerrainGrid.window):
## tile (x, y) of the chunk is at (x + READ, y + READ), a row being
## _size + 2 READ wide.
func _window() -> PackedByteArray:
	var side := _size + 2 * READ
	return TerrainGrid.window(_origin - Vector2i(READ, READ), side, side)


## The tiles of this chunk that `terrain` spills over, with the opacity of
## the spill at each corner: 1 at a corner touching a tile of `terrain`,
## fading to 0 FRINGE + 1 corners away.
func fringe(terrain: String) -> Array:
	var spill: Dictionary = FRINGE_OVER.get(terrain, {})
	var over: Array = spill.get("over", [])
	if over.is_empty():
		return []
	var win := _window()
	var nearness := GroundBuild.nearness_of(win, GroundBuild.table([TerrainGrid.code_of(terrain)]), int(spill.get("width", 1)), _size)
	var result: Array = []
	for y in _size:
		for x in _size:
			if not over.has(TerrainGrid.name_of(GroundBuild.drawn_code(win, x + READ, y + READ, _size))):
				continue
			var alphas := GroundBuild.corners_of(nearness, x, y, _size)
			if alphas[0] > 0.0 or alphas[1] > 0.0 or alphas[2] > 0.0 or alphas[3] > 0.0:
				result.append({"tile": _origin + Vector2i(x, y), "alphas": alphas})
	return result


## For the tiles of `terrain` in this chunk, how near each corner is to
## another, known terrain (the bank): tile -> four values, only for the
## tiles where some corner is near.
func shore(terrain: String) -> Dictionary:
	return GroundBuild.shore_of(_window(), _codes, TerrainGrid.code_of(terrain), _origin, _size)


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
	var m := BrookLines.MARGIN
	return maxi(hash(TerrainGrid.window(_origin - Vector2i(m, m), _size + 2 * m, _size + 2 * m)), 1)


var _last_signature := 0
## Building a chunk's ground takes 3 to 150 ms (a river's banks): it is
## worked out on a worker thread (GroundBuild, C3, 11 November 2026) and
## laid on the main thread when done, nearest the camera first, as many a
## frame as fit the world view's budget (`lay_ready`). `waiting`: the
## chunks whose ground is on a worker or done and not yet laid.
static var waiting: Array = []
## The tasks whose chunk went or asked for a newer ground meanwhile:
## waited for once done, their result dropped.
static var _orphans: Array[int] = []
## The ground under way: its task (-1: none), the signature it is for,
## where the worker puts its result.
var _task := -1
var _task_signature := 0
var _task_out: Array = []
## Built the frame it is first drawn, whatever its turn: the world view
## sets it for a chunk the screen may show (C1: every chunk on screen has
## its ground and its water the frame it shows), and it is spent by that
## build. With the margin built ahead (ViewLedger.margin_for) the ground
## is laid before the screen gets there; a chunk the screen catches first
## waits for its worker on the main thread, counted in `in_line` (F3).
var build_at_once := false
## Grounds the screen caught before their worker was done (the safety
## net): the main thread waited for them.
static var in_line := 0
## The main thread's time laying grounds, in microseconds, and how many.
static var lay_usec := 0
static var lays := 0


## The middle of the chunk's ground, in the world's pixels.
func centre_px() -> Vector2:
	return Iso.tile_to_screen(_origin.x + _size / 2, _origin.y + _size / 2)


## Starts working out the chunk's ground on a worker if the tiles around
## changed since it was last laid and no worker is on it already: called
## when the chunk is built (WorldView._build_chunk), so it is under way
## before the chunk is first drawn, and on every redraw.
func start_build() -> void:
	var signature := _terrain_signature()
	if signature == _last_signature or signature == _task_signature:
		return
	_drop_task()
	var input := _gather(true)
	_task_out = []
	_task = WorkerThreadPool.add_task(GroundBuild.run.bind(input, _task_out), true, "chunk ground")
	_task_signature = signature
	if not waiting.has(self):
		waiting.append(self)


## The task under way, if any, is left to finish on its own.
func _drop_task() -> void:
	if _task >= 0:
		_orphans.append(_task)
	_task = -1
	_task_signature = 0
	_task_out = []
	waiting.erase(self)


## Whether the ground worked out on a worker is done and may be laid.
func ground_ready() -> bool:
	return _task >= 0 and WorkerThreadPool.is_task_completed(_task)


## Lays the ground the worker worked out (waiting for it if it is not
## done), for the signature it was started for.
func _lay_task() -> void:
	var signature := _task_signature
	WorkerThreadPool.wait_for_task_completion(_task)
	var result: Dictionary = _task_out[0] if not _task_out.is_empty() else {}
	_task = -1
	_task_signature = 0
	_task_out = []
	waiting.erase(self)
	if not result.is_empty():
		_lay(result, signature)


## Lays the grounds done on the workers, nearest `camera_px` first, as
## many as fit `budget_usec` of the main thread (at least one), and
## forgets the tasks dropped once they are done. How many were laid.
static func lay_ready(budget_usec: int, camera_px: Vector2) -> int:
	for i in range(_orphans.size() - 1, -1, -1):
		if WorkerThreadPool.is_task_completed(_orphans[i]):
			WorkerThreadPool.wait_for_task_completion(_orphans[i])
			_orphans.remove_at(i)
	var done: Array = []
	for chunk: Variant in waiting:
		if is_instance_valid(chunk) and (chunk as ChunkView).ground_ready():
			done.append(chunk)
	if done.is_empty():
		return 0
	done.sort_custom(func(a: ChunkView, b: ChunkView) -> bool:
		return a.centre_px().distance_squared_to(camera_px) < b.centre_px().distance_squared_to(camera_px))
	var started := Time.get_ticks_usec()
	var laid := 0
	for chunk: ChunkView in done:
		if laid > 0 and Time.get_ticks_usec() - started >= budget_usec:
			break
		chunk._lay_task()
		chunk.queue_redraw()
		laid += 1
	return laid


## The chunk's own drawing kept between redraws: flat tiles [points,
## colour] and decorations [texture, rect, tint].
var _flats: Array = []
var _decor_cache: Array = []


## The chunk's plants, from the flora's atlas (art/flora, made by
## content/tools/flora_art.py): one image a plant, its foot on its place,
## sized by its kind's height. One layer a chunk, drawn once, above the
## ground and below the trees. `with_ground`: worked out on a worker with
## the chunk's ground and laid with it (a chunk being built, C3), rather
## than here (plants changed on a chunk built).
func show_plants(plants: PackedInt32Array, flora: Array, with_ground := false) -> void:
	var layer: PlantLayer = get_node_or_null("Plants")
	if layer == null:
		layer = PlantLayer.new()
		layer.name = "Plants"
		# The atlas is read with its mipmaps: small plants far away stay
		# calm (the ground itself is read without).
		layer.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		add_child(layer)
	if with_ground:
		_plants_due = [plants, flora]
		return
	_plants_due = []
	layer.set_plants(plants, flora)


## The plants to work out with the next ground ([plants, flora]; empty:
## none).
var _plants_due: Array = []


const FLORA_ATLAS := "res://art/flora/atlas.webp"
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

	func set_plants(plants: PackedInt32Array, flora: Array) -> void:
		set_draws(draws_of(plants, flora))

	func set_draws(draws: Array) -> void:
		_draws = draws
		queue_redraw()

	## What `plants` (as received: kind, x, y each) draw on the layer,
	## nearest the back first: [texture rect, atlas region, tint]. Pure: a
	## worker may run it once the atlas is read (ChunkView.flora_atlas).
	static func draws_of(plants: PackedInt32Array, flora: Array) -> Array:
		var draws: Array = []
		var sorted: Array = range(0, plants.size(), 3)
		# Drawn from the back to the front (the screen's depth is x + y),
		# ties in the order received.
		sorted.sort_custom(func(a: int, b: int) -> bool:
			var da := plants[a + 1] + plants[a + 2]
			var db := plants[b + 1] + plants[b + 2]
			return da < db or (da == db and a < b))
		for i: int in sorted:
			var k := plants[i]
			if k < 0 or k >= flora.size():
				continue
			var kind: Dictionary = flora[k]
			if ChunkView.plant_stands(kind):
				continue
			var at := Vector2(plants[i + 1], plants[i + 2])
			var region := ChunkView.plant_region(at, kind)
			if region.size == Vector2.ZERO:
				continue
			draws.append([ChunkView.plant_rect(at, kind, region), region, ChunkView.plant_tint(kind)])
		return draws

	func _draw() -> void:
		var atlas := ChunkView.flora_atlas()
		if atlas == null:
			return
		for d: Array in _draws:
			draw_texture_rect_region(atlas, d[0], d[1], d[2])


## Whether the chunk has built its ground at least once.
func is_built() -> bool:
	return _last_signature != 0


## What a redraw without a build costs, in microseconds, averaged over
## the last ones (the world view paces its redraw queue by it).
static var redraw_usec := 1000.0


func _draw() -> void:
	DevTiming.tally("chunk redraws")
	DevWatchdog.phase = DevWatchdog.CHUNK_REDRAW
	var started := Time.get_ticks_usec()
	var signature := _terrain_signature()
	var built := false
	if signature != _last_signature:
		start_build()
		if build_at_once:
			# The screen may show the chunk this frame: its ground now, the
			# worker's if done, else waited for (the safety net, counted).
			build_at_once = false
			DevWatchdog.phase = DevWatchdog.CHUNK_BUILD
			if not ground_ready():
				in_line += 1
				DevTiming.tally("grounds built in line")
			_lay_task()
			built = true
	if not _brook_lines.is_empty():
		var tint: Color = ground_params("water").get("base_tint", Color.WHITE)
		BrookLines.draw(self, _brook_lines, _origin, _origin + Vector2i(_size, _size), ground_texture("water"), tint)
	for flat: Array in _flats:
		draw_colored_polygon(flat[0], flat[1])
	for d: Array in _decor_cache:
		draw_texture_rect(d[0], d[1], false, d[2])
	for arrow: Array in _arrows:
		# The current on the ground plane, a metre a second drawn a tile long.
		var from: Vector2 = arrow[0]
		var flow: Vector2 = arrow[1]
		var to := from + Vector2((flow.x - flow.y) * Iso.TILE_W / 2.0, (flow.x + flow.y) * Iso.TILE_H / 2.0)
		draw_line(from, to, Color(1.0, 0.1, 0.1), 2.0)
		draw_circle(to, 3.0, Color(1.0, 0.1, 0.1))
	_draw_items()
	if not built:
		redraw_usec = lerpf(redraw_usec, float(Time.get_ticks_usec() - started), 0.2)
	DevWatchdog.phase = DevWatchdog.ELSEWHERE


## Builds the chunk's ground in line, on this thread (the tests): the
## same as a worker's.
func _draw_ground_layers() -> void:
	_drop_task()
	_lay(GroundBuild.compute(_gather(false)), _terrain_signature())


## What the ground's build reads (GroundBuild.compute), gathered on the
## main thread: the codes of the chunk and around it, the currents near
## it (a copy of the world view's, for a worker: `for_worker`), which
## terrains have a texture, the decorations' rules and images.
func _gather(for_worker: bool) -> Dictionary:
	WaterMeshes.warm(_size)
	if not _plants_due.is_empty():
		flora_atlas()
	var bit_of := PackedByteArray()
	bit_of.resize(256)
	var colours: Array = []
	colours.resize(256)
	for code in 256:
		var name := TerrainGrid.name_of(code)
		colours[code] = COLORS.get(name, Color.MAGENTA)
		if name != "" and ground_texture(name) != null:
			bit_of[code] = int(GroundIndex.BITS.get(name, 0))
	var fringe_rules: Array = []
	for terrain: String in FRINGE_OVER:
		if ground_texture(terrain) == null:
			continue
		var spill: Dictionary = FRINGE_OVER[terrain]
		var over := GroundBuild.table(spill.get("over", []).map(func(t: String) -> int: return TerrainGrid.code_of(t)))
		fringe_rules.append([TerrainGrid.code_of(terrain), over, int(spill.get("width", 1))])
	var current := current_at
	if for_worker and current_at.is_valid():
		current = Callable()
		if currents_of.is_valid():
			# The currents of the chunk and the eight around it: all the
			# water and the brooks read.
			var key := Vector2i(floori(float(_origin.x) / _size), floori(float(_origin.y) / _size))
			var currents := {}
			for dy in range(-1, 2):
				for dx in range(-1, 2):
					currents[key + Vector2i(dx, dy)] = currents_of.call(key + Vector2i(dx, dy))
			current = GroundBuild.current_in.bind(currents, _size)
	var win := _window()
	var brook_win := PackedByteArray()
	if win.has(TerrainGrid.BROOK):
		var m := GroundBuild.BROOK_READ
		brook_win = TerrainGrid.window(_origin - Vector2i(m, m), _size + 2 * m, _size + 2 * m)
	return {
		"origin": _origin, "size": _size, "win": win, "codes": _codes, "brook_win": brook_win,
		"bit_of": bit_of, "colours": colours, "fringe": fringe_rules, "current": current,
		"decor": _decor_rules(), "brook_bed": str(ground_layers("water").get("brook1", "")) != "",
		"arrows": show_arrows, "plants": _plants_due,
	}


## The decorations' rules (ground.json), each with its images and their
## proportions, in the spec's order: what GroundBuild sows from.
static func _decor_rules() -> Array:
	var decor: Dictionary = ground_spec().get("decor", {})
	var rules: Array = []
	var water_tint: Color = ground_params("water").get("base_tint", Color.WHITE)
	var salt := 0
	for kind: String in decor:
		if not decor[kind] is Dictionary:
			continue  # the entry's note
		salt += 1
		var rule: Dictionary = decor[kind]
		var textures := decor_textures(kind)
		if textures.is_empty():
			continue
		var ratios := PackedFloat32Array()
		for texture: Texture2D in textures:
			ratios.append(float(texture.get_height()) / float(texture.get_width()))
		rules.append({
			"salt": salt, "textures": textures, "ratios": ratios,
			"chance": float(rule.get("chance", 0.1)),
			"size_px": float(rule.get("size_m", 0.5)) * Iso.PX_PER_M,
			"min_near": float(rule.get("min_near", 0.6)),
			"jitter": float(rule.get("size_jitter", 0.0)),
			"tint": water_tint if str(rule.get("tint", "")) == "water" else Color.WHITE,
			"by_water": str(rule.get("where", "")) == "water_near_land",
		})
	return rules


## Lays the ground worked out (GroundBuild.compute) for `signature`: the
## flat tiles and the decorations kept for the chunk's own drawing, the
## ground index written, each terrain's canvas told, the water meshes
## and the brooks' ribbons made from their arrays. The main thread's
## whole share of a build.
func _lay(result: Dictionary, signature: int) -> void:
	var started := Time.get_ticks_usec()
	var t := DevTiming.start()
	_last_signature = signature
	_flats = result["flats"]
	_decor_cache = result["decor"]
	_arrows = result["arrows"]
	_brook_lines = result["brook_lines"]
	var plants: Variant = result["plant_draws"]
	if plants is Array and not _plants_due.is_empty():
		var layer: PlantLayer = get_node_or_null("Plants")
		if layer != null:
			layer.set_draws(plants)
		_plants_due = []
	GroundIndex.write(_origin, _size, result["bits"], result["corners"])
	var rows: Dictionary = result["rows"]
	for terrain: String in GroundIndex.BITS:
		var rects: PackedInt32Array = rows.get(terrain, PackedInt32Array())
		var shown := not rects.is_empty()
		if shown or _grounds.has(terrain):
			var canvas := _ground_of(terrain)
			canvas.origin = _origin
			canvas.size = _size
			canvas.rows = rects
			canvas.shown = shown
			canvas.queue_redraw()
	var water_usec: int = result["water_usec"]
	if water_usec >= 0:
		var laid := Time.get_ticks_usec()
		water_build_usec += water_usec
		water_builds += 1
		_lay_water_meshes(result["water"], result["bank"])
		water_main_usec += Time.get_ticks_usec() - laid
	else:
		_lay_water_meshes({}, {})
	# A brook cuts the bank where it joins a river: drawn over it.
	var brooks: Variant = result["brooks"]
	if brooks is Dictionary:
		_brook_canvas().lay(brooks)
	elif _brooks != null:
		_brooks.lay({})
	lay_usec += Time.get_ticks_usec() - started
	lays += 1
	DevTiming.add("chunk lays", t)


## Items: a small disc of the material's colour, slightly off the tile's
## centre so several on one tile stay visible.
## Segments of an item's disc: 24 are round at a radius of 11 px (zoom
## 2.5); the 64 of Godot's own circles cost three times the vertices.
const ITEM_SEGMENTS := 24
## The mesh of the chunk's items, built once per change of the items
## (_build_items) and kept: drawn again at every redraw of the chunk.
var _items_mesh: ArrayMesh
## Where a corpse's skull is drawn, with the mesh.
var _items_icons: Array[Vector2] = []
## The hash of the items the mesh was built from (0: none yet).
var _items_signature := 0
## How many times the items' mesh was built (the tests).
var items_built := 0


func _draw_items() -> void:
	if _items_mesh != null:
		draw_mesh(_items_mesh, null)
	for at in _items_icons:
		draw_death_icon(self, at)


## Builds the items' mesh: a small disc of the material's colour, slightly
## off the tile's centre so several on one tile stay visible, outlined.
## All of a chunk's discs go into one mesh, drawn at once: drawn one by
## one they were two draw calls each, most of a frame's (from the
## performance audit, 9 November 2026).
func _build_items() -> void:
	items_built += 1
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
	_items_icons = icons
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


## The brook shader's material, shared by every chunk.
static var _brook_material: ShaderMaterial


## The child canvas drawing this chunk's brooks with the brook shader,
## made on first use.
func _brook_canvas() -> WaterCanvas:
	if _brooks == null:
		if _brook_material == null:
			_brook_material = ShaderMaterial.new()
			_brook_material.shader = load("res://world/brook.gdshader")
			_brook_material.set_shader_parameter("noise_texture", noise_texture())
			_brook_material.set_shader_parameter("flow_max", WaterMeshes.FLOW_MAX)
			var layers := ground_layers("water")
			for k in 3:
				_brook_material.set_shader_parameter("bed%d_texture" % (k + 1), load_texture(str(layers.get("brook%d" % (k + 1), ""))))
			_brook_material.set_shader_parameter("earth_texture", load_texture(str(layers.get("bank", ""))))
			_brook_material.set_shader_parameter("water_tint", ground_params("water").get("base_tint", Color.WHITE))
			pin_flow_time(_brook_material)
		_brooks = WaterCanvas.new()
		_brooks.name = "Brooks"
		_brooks.z_index = -3
		_brooks.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
		_brooks.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
		_brooks.material = _brook_material
		_brooks.texture = ground_texture("water")
		add_child(_brooks)
	return _brooks


var _brooks: WaterCanvas = null
## The brooks' lines drawn flat, when the brook's art is missing.
var _brook_lines: Array = []


## A small skull above a corpse, drawn on `canvas` at `at`.
static func draw_death_icon(canvas: CanvasItem, at: Vector2) -> void:
	canvas.draw_circle(at, 4.5, Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at, 4.5, Color.BLACK, false, 1.0)
	canvas.draw_rect(Rect2(at + Vector2(-2.5, 3), Vector2(5, 2.5)), Color(0.92, 0.92, 0.88))
	canvas.draw_circle(at + Vector2(-1.7, -0.5), 1.1, Color.BLACK)
	canvas.draw_circle(at + Vector2(1.7, -0.5), 1.1, Color.BLACK)
