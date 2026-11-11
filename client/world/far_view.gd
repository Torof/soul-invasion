class_name FarView
extends Node2D
## The land past the chunks held (D39, D40): the server's far view, one
## letter a cell (Protocol: far_view), drawn as a soft image under the
## chunks, its colours those of the ground's own textures, and a haze over
## it: a veil over the view's last tiles that thickens into a whole fog
## at the view's limit and covers everything past it (WorldView, the
## project lead's call), so the far land itself is hidden for now.
##
## The veils and the far view are drawn as rings (10 November 2026, D39):
## a veil is exactly nothing well inside the ground held, so that part is
## not drawn at all, and the far view lies under opaque ground wherever
## the chunks under the view's limit are built, so that part is left out
## too. The picture is the same to the pixel; the graphics card no longer
## shades two or three screens for nothing.

## Haze colour (a soft grey: a whiter one was too loud, the project
## lead's call), and how much of it just past the ground held and far
## away.
const HAZE := Color(0.62, 0.65, 0.66)
const HAZE_EDGE := 0.5
const HAZE_FAR := 0.68
## Tiles past the edge over which the haze rises to HAZE_EDGE, and over
## which it reaches its farthest.
const HAZE_IN := 24.0
const HAZE_OUT := 700.0
## Where the veil is drawn: over the ground (ChunkView's ground layers,
## banks and brooks, down to -4), under the chunks' items and plants
## (-1) and what stands (0).
const VEIL_Z := -3
## Tiles a veil reaches past the ground held, either way: as far as the
## far view spans around the body (net.rs: FAR_CELLS × FAR_CELL / 2),
## which was the veils' extent before they were rings. The camera stops
## a few tiles past the view's limit, so nothing on screen lies farther.
const VEIL_REACH := 1024.0
## Tiles inside the ground held from which a veil's alpha is exactly 0:
## the noise may bring the fog half a tile sooner (fog.gdshader, n * 0.5),
## and a quarter tile of margin for the floats.
const HOLE_INSET := 0.75
## The haze shader, shared by both veils.
const FOG_SHADER := "res://world/fog.gdshader"

## How the veils and the far view are drawn (development, F7): as rings
## (the game's way), as the full quads they were before 10 November 2026,
## or not at all, to measure what they cost (D39).
## SOUL_INVASION_FOG_DRAWING names the way a client starts with.
enum Drawing { QUADS, RINGS, OFF }
const DRAWING_NAMES := ["quads", "rings", "off"]
static var drawing: int = drawing_named(OS.get_environment("SOUL_INVASION_FOG_DRAWING"))

static var _fog_shader: Shader

var _texture: ImageTexture
var _cell := 8
var _origin := Vector2i.ZERO
var _size := Vector2i.ZERO
var _haze: Veil
var _cover: Veil
## The hole the far view leaves, in tile space: the view's limit, once
## every chunk under it is built (set_complete), and whether it is.
var _hole := Rect2()
var _complete := false
## The frame the hole became complete: drawn from the next one on, so a
## chunk whose canvases draw a frame after its own never shows nothing.
var _complete_since := -1


## A veil: its layer, its material, the ground it leaves clear and the
## ring drawn around it.
class Veil:
	var layer: Node2D
	var material: ShaderMaterial
	## The ground held, in tiles: [lo, hi); and its rounded corners.
	var held := Rect2()
	var corner := 0.0
	## The ring (FarView.ring), on screen: points, uvs, indices.
	var points := PackedVector2Array()
	var uvs := PackedVector2Array()
	var indices := PackedInt32Array()
	## How many times the ring was built (the tests: only when the ground
	## held moves).
	var builds := 0


func _ready() -> void:
	add_to_group("far_views")
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
	# The veil lies on the ground only (VEIL_Z: over the ground, the
	# banks and the brooks, under the items, the plants, the trees and
	# the bodies); the cover, over everything, hides what stands past the
	# view (the project lead's call).
	_haze = _veil(VEIL_Z)
	_cover = _veil(40)


## The haze shader, loaded once and shared by both veils.
static func fog_shader() -> Shader:
	if _fog_shader == null:
		_fog_shader = load(FOG_SHADER)
	return _fog_shader


## The way of drawing a development variable names; rings unless it
## names another.
static func drawing_named(name: String) -> int:
	var at := DRAWING_NAMES.find(name.strip_edges().to_lower())
	return at if at >= 0 else Drawing.RINGS


## The name of the way the fog is drawn now (the development counters).
static func drawing_name() -> String:
	return DRAWING_NAMES[drawing]


func _fog_material() -> ShaderMaterial:
	var fog := ShaderMaterial.new()
	fog.shader = fog_shader()
	fog.set_shader_parameter("noise_texture", ChunkView.noise_texture())
	fog.set_shader_parameter("haze", Vector3(HAZE.r, HAZE.g, HAZE.b))
	fog.set_shader_parameter("edge", HAZE_EDGE)
	fog.set_shader_parameter("far_haze", HAZE_FAR)
	fog.set_shader_parameter("rise_in", HAZE_IN)
	fog.set_shader_parameter("reach", HAZE_OUT)
	# Development: the fog's drift pinned, so two captures compare.
	var pinned := OS.get_environment("SOUL_INVASION_FOG_TIME")
	if pinned.is_valid_float():
		fog.set_shader_parameter("drift_time", float(pinned))
	return fog


func _veil(z: int) -> Veil:
	var veil := Veil.new()
	veil.material = _fog_material()
	veil.layer = Node2D.new()
	veil.layer.z_index = z
	veil.layer.z_as_relative = false
	veil.layer.material = veil.material
	veil.layer.draw.connect(_paint_veil.bind(veil))
	add_child(veil.layer)
	return veil


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


## Where the haze starts: the limit of the view, in tiles from `lo` to
## `hi` (WorldView, around the body), and over how many tiles past it the
## haze rises to `edge`.
## `edge` and `far` its thickness there and far away (HAZE_EDGE and
## HAZE_FAR; 1 for a fog one cannot see through).
func set_held(lo: Vector2, hi: Vector2, rise := HAZE_IN, edge := HAZE_EDGE, far := HAZE_FAR, corner := 0.0) -> void:
	_send(_haze.material, "corner", corner)
	_send(_haze.material, "held_lo", lo)
	_send(_haze.material, "held_hi", hi)
	_send(_haze.material, "rise_in", rise)
	_send(_haze.material, "edge", edge)
	_send(_haze.material, "far_haze", far)
	_place(_haze, Rect2(lo, hi - lo), corner)


## Where the cover starts, over everything, in tiles from `lo` to `hi`:
## whole `rise` tiles past it (WorldView: at the view's limit).
func set_cover(lo: Vector2, hi: Vector2, rise: float, opacity := 1.0, corner := 0.0) -> void:
	_send(_cover.material, "corner", corner)
	_send(_cover.material, "held_lo", lo)
	_send(_cover.material, "held_hi", hi)
	_send(_cover.material, "rise_in", rise)
	_send(_cover.material, "edge", opacity)
	_send(_cover.material, "far_haze", opacity)
	_place(_cover, Rect2(lo, hi - lo), corner)


## The value each uniform of each fog material was last given, and how
## many were sent: the limit moves every frame the body walks, and only
## its two corners change; the rest went to the graphics card twelve
## times a frame for nothing (lightness audit, 10 November 2026).
var _sent: Dictionary = {}
var uniforms_sent := 0


func _send(fog: ShaderMaterial, uniform: String, value: Variant) -> void:
	var sent: Dictionary = _sent.get_or_add(fog, {})
	if sent.has(uniform) and sent[uniform] == value:
		return
	sent[uniform] = value
	uniforms_sent += 1
	fog.set_shader_parameter(uniform, value)


## Gives a veil the ground it leaves clear; its ring is built again only
## when that moves.
func _place(veil: Veil, held: Rect2, corner: float) -> void:
	if veil.builds > 0 and held == veil.held and corner == veil.corner:
		return
	veil.held = held
	veil.corner = corner
	var mesh := ring(held.grow(VEIL_REACH), held, corner, HOLE_INSET)
	veil.uvs = mesh["points"]
	veil.points = PackedVector2Array()
	for p: Vector2 in veil.uvs:
		veil.points.append(tile_space() * p)
	veil.indices = mesh["indices"]
	veil.builds += 1
	veil.layer.queue_redraw()


## How many times each veil's ring was built: [haze, cover] (the tests).
func ring_builds() -> Array[int]:
	return [_haze.builds, _cover.builds]


## The hole the far view may leave: `hole`, in tile space (the view's
## limit), when `complete` says every chunk under it is built (WorldView).
func set_complete(hole: Rect2, complete: bool) -> void:
	if hole == _hole and complete == _complete:
		return
	if complete and not _complete:
		_complete_since = Engine.get_process_frames()
	_hole = hole
	_complete = complete
	queue_redraw()


## Whether every chunk the screen shows (`shown`, chunks) under `limit`
## (tiles) is built, or builds this frame (`chunks`: chunk -> ChunkView,
## the chunks built), with no chunk missing from the bounds of the chunks
## held (`held`: chunk -> anything): then the ground is opaque wherever
## the far view would be seen under the limit and it need not be drawn
## there. The chunks under the limit the screen does not show may stay
## unbuilt, or wait their turn in the margin (C1): nobody sees what lies
## under them.
static func limit_built(limit: Rect2, chunks: Dictionary, held: Dictionary, bounds: Rect2i, chunk_size: int, shown: Array) -> bool:
	if held.is_empty() or limit.size.x <= 1.0 or limit.size.y <= 1.0:
		return false
	if held.size() != bounds.size.x * bounds.size.y:
		return false
	var lo := Vector2i((limit.position / chunk_size).floor())
	var hi := Vector2i((limit.end / chunk_size).ceil())
	for key: Vector2i in shown:
		if key.x < lo.x or key.y < lo.y or key.x >= hi.x or key.y >= hi.y:
			continue
		var chunk: ChunkView = chunks.get(key)
		if chunk == null or not (chunk.is_built() or chunk.build_at_once):
			return false
	return true


## Whether the far view leaves its hole at `frame`: complete, and not on
## the frame it became so.
static func hole_drawn(complete: bool, since: int, frame: int) -> bool:
	return complete and frame > since


func _process(_delta: float) -> void:
	# The frame after the hole became complete: drawn with it from now on.
	if _complete and _complete_since >= 0 and Engine.get_process_frames() > _complete_since:
		_complete_since = -1
		queue_redraw()


## Draws everything again (F7 changed the way).
func redraw_fog() -> void:
	queue_redraw()
	_haze.layer.queue_redraw()
	_cover.layer.queue_redraw()


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


## A ring: `outer` with a hole where a veil is exactly nothing, an
## octagon inscribed in the rounded rectangle `held` (corners of `corner`
## tiles, the shader's own radius) inset by `inset` tiles. Points in tile
## space; 12 vertices and 12 triangles sharing their inner edges by index,
## so each pixel is drawn once, with no seam. A plain quad when the hole
## has no room.
static func ring(outer: Rect2, held: Rect2, corner: float, inset: float) -> Dictionary:
	var hole := held.grow(-inset)
	if hole.size.x <= 0.0 or hole.size.y <= 0.0 or not outer.encloses(held):
		return quad(outer)
	var half := held.size * 0.5
	var r := minf(corner, minf(half.x, half.y))
	# The arcs `inset` tiles inside have this radius; their chords, at
	# the arcs' tangent points, lie inside them.
	var rr := maxf(r - inset, 0.0)
	var lo := hole.position
	var hi := hole.end
	var points := PackedVector2Array([
		outer.position, Vector2(outer.end.x, outer.position.y), outer.end, Vector2(outer.position.x, outer.end.y),
		Vector2(lo.x + rr, lo.y), Vector2(hi.x - rr, lo.y),
		Vector2(hi.x, lo.y + rr), Vector2(hi.x, hi.y - rr),
		Vector2(hi.x - rr, hi.y), Vector2(lo.x + rr, hi.y),
		Vector2(lo.x, hi.y - rr), Vector2(lo.x, lo.y + rr),
	])
	var indices := PackedInt32Array()
	for side in 4:
		var o0 := side
		var o1 := (side + 1) % 4
		var i0 := 4 + 2 * side
		var i1 := i0 + 1
		var i2 := 4 + (2 * side + 2) % 8
		# The band along the side, two triangles; then the corner's one.
		indices.append_array([o0, o1, i1, o0, i1, i0, o1, i2, i1])
	return {"points": points, "indices": indices}


## A plain quad over `outer`, in the same form as a ring.
static func quad(outer: Rect2) -> Dictionary:
	return {
		"points": PackedVector2Array([outer.position, Vector2(outer.end.x, outer.position.y), outer.end, Vector2(outer.position.x, outer.end.y)]),
		"indices": PackedInt32Array([0, 1, 2, 0, 2, 3]),
	}


## The haze shader's distance at a tile-space point from the ground held
## from `lo` to `hi` with `corner` tiles of rounding, without the noise:
## negative inside (the tests check the hole lies where the veil is 0).
static func fog_distance(t: Vector2, lo: Vector2, hi: Vector2, corner: float) -> float:
	var half := (hi - lo) * 0.5
	var r := minf(corner, minf(half.x, half.y))
	var q := (t - (lo + hi) * 0.5).abs() - (half - Vector2(r, r))
	return q.max(Vector2.ZERO).length() + minf(maxf(q.x, q.y), 0.0) - r


func _draw() -> void:
	if _texture == null or drawing == Drawing.OFF:
		return
	var corner := Vector2(_origin * _cell) - Vector2(0.5, 0.5)
	var extent := Vector2(_size * _cell)
	var holed := drawing == Drawing.RINGS and hole_drawn(_complete, _complete_since, Engine.get_process_frames())
	if not holed:
		var t := tile_space() * Transform2D(0.0, Vector2(_cell, _cell), 0.0, corner)
		draw_set_transform_matrix(t)
		draw_texture(_texture, Vector2.ZERO)
		draw_set_transform_matrix(Transform2D.IDENTITY)
		return
	var mesh := ring(Rect2(corner, extent), _hole, 0.0, 0.0)
	var tiles: PackedVector2Array = mesh["points"]
	var points := PackedVector2Array()
	var uvs := PackedVector2Array()
	var colours := PackedColorArray()
	for p: Vector2 in tiles:
		points.append(tile_space() * p)
		uvs.append((p - corner) / extent)
		colours.append(Color.WHITE)
	RenderingServer.canvas_item_add_triangle_array(get_canvas_item(), mesh["indices"], points, colours, uvs,
		PackedInt32Array(), PackedFloat32Array(), _texture.get_rid())


## A veil: its ring, the UVs the tile coordinates; as a full quad (the
## way before the rings) or not at all under F7.
func _paint_veil(veil: Veil) -> void:
	if drawing == Drawing.OFF or veil.builds == 0:
		return
	if drawing == Drawing.QUADS:
		var outer := veil.held.grow(VEIL_REACH)
		var corners := [outer.position, Vector2(outer.end.x, outer.position.y), outer.end, Vector2(outer.position.x, outer.end.y)]
		var points := PackedVector2Array()
		var uvs := PackedVector2Array()
		for c: Vector2 in corners:
			points.append(tile_space() * c)
			uvs.append(c)
		veil.layer.draw_polygon(points, PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE]), uvs)
		return
	var colours := PackedColorArray()
	colours.resize(veil.points.size())
	colours.fill(Color.WHITE)
	RenderingServer.canvas_item_add_triangle_array(veil.layer.get_canvas_item(), veil.indices, veil.points, colours, veil.uvs)
