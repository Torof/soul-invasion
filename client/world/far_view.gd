class_name FarView
extends Node2D
## The land past the chunks held (D39, D40): the server's far view, one
## letter a cell (Protocol: far_view), drawn as a soft image under the
## chunks, its colours those of the ground's own textures, and a haze over
## it that thickens with the distance from the detailed ground. The haze
## starts at the edge of the detailed chunks, never over them: they keep
## their colours to their last tile (the project lead's call); it rises
## across the ring of outline chunks around them (WorldView).

## Haze colour, and how much of it just past the ground held and far
## away.
const HAZE := Color(0.78, 0.84, 0.86)
const HAZE_EDGE := 0.5
const HAZE_FAR := 0.68
## Tiles past the edge over which the haze rises to HAZE_EDGE, and over
## which it reaches its farthest.
const HAZE_IN := 24.0
const HAZE_OUT := 700.0

var _texture: ImageTexture
var _cell := 8
var _origin := Vector2i.ZERO
var _size := Vector2i.ZERO
var _haze: Node2D
var _haze_material: ShaderMaterial
## The ground held, in tiles: [lo, hi).
var _held := Rect2(0, 0, 0, 0)


func _ready() -> void:
	# Below every layer of the chunks' ground (as low as -5 with theirs).
	z_index = -20
	# A little grain, so the far land is not flat colour.
	var grain := ShaderMaterial.new()
	grain.shader = Shader.new()
	grain.shader.code = """
shader_type canvas_item;
uniform sampler2D noise_texture : repeat_enable, filter_linear;
void fragment() {
	vec4 c = texture(TEXTURE, UV);
	float n = texture(noise_texture, UV * 24.0).r * 0.6 + texture(noise_texture, UV * 71.0).r * 0.4;
	c.rgb *= 0.86 + 0.28 * n;
	COLOR = c;
}
"""
	grain.set_shader_parameter("noise_texture", ChunkView.noise_texture())
	material = grain
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	_haze = Node2D.new()
	_haze.z_index = 40
	_haze_material = ShaderMaterial.new()
	_haze_material.shader = _haze_shader()
	_haze_material.set_shader_parameter("noise_texture", ChunkView.noise_texture())
	_haze.material = _haze_material
	_haze.draw.connect(_paint_haze)
	add_child(_haze)
	_haze.z_as_relative = false


## Takes the server's far view (cells of `cell` tiles from cell (x, y)).
func show_far(cell: int, x: int, y: int, width: int, height: int, cells: String) -> void:
	if width <= 0 or height <= 0 or cells.length() < width * height:
		return
	_cell = cell
	_origin = Vector2i(x, y)
	_size = Vector2i(width, height)
	var colours := palette()
	var image := Image.create(width, height, false, Image.FORMAT_RGBA8)
	for j in height:
		for i in width:
			image.set_pixel(i, j, colours.get(cells[j * width + i], Color(0, 0, 0, 0)))
	_texture = ImageTexture.create_from_image(image)
	queue_redraw()
	_haze.queue_redraw()


## Where the haze starts: the detailed ground, in tiles from `lo` to `hi`
## (excluded), and over how many tiles past it the haze rises to
## HAZE_EDGE: the ring of outline chunks around it, which melts into the
## far land (WorldView), or HAZE_IN without one.
## `edge` and `far` its thickness there and far away (HAZE_EDGE and
## HAZE_FAR; 1 for a fog one cannot see through).
func set_held(lo: Vector2i, hi: Vector2i, rise := HAZE_IN, edge := HAZE_EDGE, far := HAZE_FAR) -> void:
	_held = Rect2(Vector2(lo), Vector2(hi - lo))
	_haze_material.set_shader_parameter("held_lo", Vector2(lo))
	_haze_material.set_shader_parameter("held_hi", Vector2(hi))
	_haze_material.set_shader_parameter("rise_in", rise)
	_haze_material.set_shader_parameter("edge", edge)
	_haze_material.set_shader_parameter("far_haze", far)


## The colour of each letter: the average of the ground's own textures,
## so the far land matches the near; the woods a darker canopy.
static func palette() -> Dictionary:
	var average := func(terrain: String, fallback: Color) -> Color:
		var c := ChunkView.average_colour(terrain)
		return c if c.a > 0.0 else fallback
	var grass: Color = average.call("grass", Color(0.45, 0.62, 0.30))
	var water: Color = average.call("water", Color(0.20, 0.50, 0.62))
	return {
		"g": grass,
		"f": Color(0.20, 0.33, 0.20),
		"w": water,
		"o": water.lerp(grass, 0.35),
		"b": water.lerp(grass, 0.5),
		"r": Color(0.42, 0.40, 0.38),
	}


## From tile space (tile (x, y) centred on (x, y)) to the screen.
static func tile_space() -> Transform2D:
	return Transform2D(Vector2(Iso.TILE_W / 2.0, Iso.TILE_H / 2.0), Vector2(-Iso.TILE_W / 2.0, Iso.TILE_H / 2.0), Vector2.ZERO)


func _draw() -> void:
	if _texture == null:
		return
	var corner := Vector2(_origin * _cell) - Vector2(0.5, 0.5)
	var t := tile_space() * Transform2D(0.0, Vector2(_cell, _cell), 0.0, corner)
	draw_set_transform_matrix(t)
	draw_texture(_texture, Vector2.ZERO)
	draw_set_transform_matrix(Transform2D.IDENTITY)


## The haze: one quad over the far view, its UV the tile coordinates.
func _paint_haze() -> void:
	if _texture == null:
		return
	var lo := Vector2(_origin * _cell) - Vector2(0.5, 0.5)
	var hi := lo + Vector2(_size * _cell)
	var corners := [lo, Vector2(hi.x, lo.y), hi, Vector2(lo.x, hi.y)]
	var points := PackedVector2Array()
	var uvs := PackedVector2Array()
	for c: Vector2 in corners:
		points.append(tile_space() * c)
		uvs.append(c)
	_haze.draw_polygon(points, PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE]), uvs)


static func _haze_shader() -> Shader:
	var shader := Shader.new()
	shader.code = """
shader_type canvas_item;
// The haze over the far land, in tile coordinates (UV): none well inside
// the ground held, rising to its edge, thickening far away (FarView).
uniform vec2 held_lo = vec2(0.0);
uniform vec2 held_hi = vec2(0.0);
uniform vec3 haze = vec3(%f, %f, %f);
uniform float edge = %f;
uniform float far_haze = %f;
uniform float rise_in = %f;
uniform float reach = %f;
uniform sampler2D noise_texture : repeat_enable, filter_linear;
void fragment() {
	vec2 t = UV;
	// A fog, not a wall: its edge wanders outwards by up to two tiles (never
	// over the detailed ground) and drifts
	// slowly, and it is lighter and darker in soft banks.
	vec2 drift = vec2(TIME * 0.25, TIME * 0.1);
	float n = texture(noise_texture, (t + drift) / 14.0).r;
	float m = texture(noise_texture, (t - drift * 0.6) / 37.0 + vec2(0.31, 0.77)).r;
	// Distance outside the ground held, negative inside (the larger of the
	// two axes, as the ground held is a square of chunks).
	vec2 out_lo = held_lo - t;
	vec2 out_hi = t - held_hi;
	float d = max(max(out_lo.x, out_hi.x), max(out_lo.y, out_hi.y)) - n * 2.0;
	// Nothing over the ground held (d <= 0): the haze starts at its edge.
	float a = smoothstep(0.0, rise_in, d) * edge + smoothstep(rise_in, reach, d) * (far_haze - edge);
	COLOR = vec4(haze * (0.94 + 0.1 * m), a);
}
""" % [HAZE.r, HAZE.g, HAZE.b, HAZE_EDGE, HAZE_FAR, HAZE_IN, HAZE_OUT]
	return shader
