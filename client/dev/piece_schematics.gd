extends SceneTree
## Renders the schematics of the bank pieces for the image generator: one
## image per piece (sixteen straight headings, a bend to each side of
## each), the ground painted with the game's own textures and shader,
## the water on the right of the bank, the piece drawn as a dark line
## with its ends ticked, the bank continuing lighter beyond it. The image
## is in screen space, eight times the game's scale (a tile is 512 px
## wide), 1024 × 1024 px around the piece's middle.
##   godot --path client -s dev/piece_schematics.gd
## SOUL_INVASION_OUT names the folder (default content/ai-reserve/for-scenario/pieces-v3).

const SIZE := 1024
const ZOOM := 8.0
const LINE := Color(0.26, 0.15, 0.08)
const BEYOND := Color(0.26, 0.15, 0.08, 0.35)

var _out := ""
var _jobs: Array = []
var _viewport: SubViewport
var _scene: Node2D
var _camera: Camera2D
var _index: Array = []


func _initialize() -> void:
	_out = OS.get_environment("SOUL_INVASION_OUT")
	if _out.is_empty():
		_out = "res://../content/ai-reserve/for-scenario/pieces-v3"
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(_out))
	for d in 16:
		_jobs.append({"dir": d, "turn": 0})
		_jobs.append({"dir": d, "turn": 1})
		_jobs.append({"dir": d, "turn": -1})
	_viewport = SubViewport.new()
	_viewport.size = Vector2i(SIZE, SIZE)
	_viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	_viewport.transparent_bg = false
	root.add_child(_viewport)
	_scene = Node2D.new()
	_viewport.add_child(_scene)
	_camera = Camera2D.new()
	_camera.zoom = Vector2.ONE * ZOOM
	_scene.add_child(_camera)
	_camera.make_current()
	_run()


func _run() -> void:
	for job in _jobs:
		await _render(job)
	var index := FileAccess.open(_out.path_join("index.txt"), FileAccess.WRITE)
	for line in _index:
		index.store_line(line)
	index.close()
	print("rendered ", _jobs.size(), " schematics to ", ProjectSettings.globalize_path(_out))
	quit()


## The name of a piece (ShorePieces.piece_name, shared with the client).
static func piece_name(d: int, turn: int) -> String:
	return ShorePieces.piece_name(d, turn)


## Which face the bank shows at this heading: the direction towards the
## water on screen tells whether we look at its earthen face (water
## below), at grass overhanging (water above) or at its profile.
static func face_of(d: int) -> String:
	var h: Vector2 = ShorePieces.HEADINGS[d]
	var right := Vector2(-h.y, h.x)
	var n := (ChunkView.tile_point_to_screen(right) - ChunkView.tile_point_to_screen(Vector2.ZERO)).normalized()
	if n.y > 0.4:
		return "earth-face"
	if n.y < -0.4:
		return "grass-overhang"
	return "profile"


func _render(job: Dictionary) -> void:
	for child in _scene.get_children():
		if child != _camera:
			child.queue_free()
	var d: int = job["dir"]
	var turn: int = job["turn"]
	var pts := ShorePieces.path_points(Vector2.ZERO, d, turn)
	var mid: Vector2 = pts[pts.size() / 2]
	# Shift so the piece's middle sits at the origin of the scene.
	for i in pts.size():
		pts[i] -= mid
	var h0: Vector2 = ShorePieces.HEADINGS[d]
	var h1: Vector2 = ShorePieces.HEADINGS[(d + turn + 16) % 16]
	var r0 := Vector2(-h0.y, h0.x)
	var r1 := Vector2(-h1.y, h1.x)
	var p0: Vector2 = pts[0]
	var p1: Vector2 = pts[pts.size() - 1]
	var far := 8.0
	# Land everywhere, then the water: the right side of the bank, which
	# runs straight on beyond the piece at each end.
	var land := _ground("grass")
	land.polygon = PackedVector2Array([Vector2(-far, -far), Vector2(far, -far), Vector2(far, far), Vector2(-far, far)])
	_scene.add_child(land)
	var water := _ground("water")
	var shore := PackedVector2Array([p0 - h0 * far])
	for p in pts:
		shore.append(p)
	shore.append(p1 + h1 * far)
	shore.append(p1 + h1 * far + r1 * far)
	shore.append(p0 - h0 * far + r0 * far)
	water.polygon = shore
	_scene.add_child(water)
	var ink := Ink.new()
	ink.piece = pts
	ink.before = [p0 - h0 * far, p0]
	ink.after = [p1, p1 + h1 * far]
	_scene.add_child(ink)
	_camera.position = ChunkView.tile_point_to_screen(Vector2.ZERO)
	for i in 4:
		await process_frame
	var image := _viewport.get_texture().get_image()
	var name := piece_name(d, turn)
	image.save_png(_out.path_join(name + ".png"))
	_index.append("%s\theading %d\tturn %d\t%s" % [name, d, turn, face_of(d)])


## A polygon of ground in tile units, drawn with the terrain's texture and
## shader like the game draws it.
func _ground(terrain: String) -> Ground:
	var ground := Ground.new()
	ground.terrain = terrain
	ground.material = ChunkView.ground_material(terrain)
	ground.texture_repeat = CanvasItem.TEXTURE_REPEAT_ENABLED
	return ground


class Ground:
	extends Node2D
	var terrain := ""
	var polygon := PackedVector2Array()

	func _draw() -> void:
		var screen := PackedVector2Array()
		var uvs := PackedVector2Array()
		var colours := PackedColorArray()
		for p in polygon:
			screen.append(ChunkView.tile_point_to_screen(p))
			uvs.append((p + Vector2(0.5, 0.5)) / ChunkView.GROUND_METRES)
			colours.append(Color.WHITE)
		draw_polygon(screen, colours, uvs, ChunkView.ground_texture(terrain))


class Ink:
	extends Node2D
	var piece := PackedVector2Array()
	var before: Array = []
	var after: Array = []

	func _draw() -> void:
		var w := 6.0 / 8.0
		draw_line(ChunkView.tile_point_to_screen(before[0]), ChunkView.tile_point_to_screen(before[1]), BEYOND, w * 0.6, true)
		draw_line(ChunkView.tile_point_to_screen(after[0]), ChunkView.tile_point_to_screen(after[1]), BEYOND, w * 0.6, true)
		var screen := PackedVector2Array()
		for p in piece:
			screen.append(ChunkView.tile_point_to_screen(p))
		draw_polyline(screen, LINE, w, true)
		for end in [screen[0], screen[screen.size() - 1]]:
			draw_circle(end, w * 0.9, Color.WHITE)
			draw_arc(end, w * 0.9, 0.0, TAU, 16, LINE, w * 0.25, true)
