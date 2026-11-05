class_name WorldView
extends Node2D
## Draws the valley and its bodies in provisional isometric graphics, and
## turns clicks into `move_to` intentions. Everything shown comes from the
## server; nothing is decided here (D03, D05).

const COLORS := {
	"grass": Color(0.45, 0.62, 0.30),
	"forest": Color(0.22, 0.40, 0.20),
	"water": Color(0.25, 0.45, 0.70),
	"ford": Color(0.55, 0.60, 0.50),
	"rock": Color(0.48, 0.45, 0.42),
}
const BODY_COLOR := Color(0.90, 0.80, 0.60)
const PILOTED_COLOR := Color(1.0, 0.55, 0.20)
const WALK_SPEED := 10.0  # tiles per second, for the visual glide only

var _width := 0
var _height := 0
var _tiles: Array = []
## body id -> {"tile": Vector2i, "shown": Vector2 (screen, glides)}
var _bodies: Dictionary = {}
var _piloted := -1
var _hover := Vector2i(-1, -1)


func _ready() -> void:
	Net.world_state.connect(_on_world_state)
	Net.body_moved.connect(_on_body_moved)
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


func _on_world_state(width: int, height: int, tiles: Array, bodies: Array) -> void:
	_width = width
	_height = height
	_tiles = tiles
	_bodies.clear()
	for b in bodies:
		var tile := Vector2i(int(b.get("x", 0)), int(b.get("y", 0)))
		_bodies[int(b.get("id", -1))] = {"tile": tile, "shown": Iso.tile_to_screen(tile.x, tile.y)}
	_center_camera()
	queue_redraw()


func _on_body_moved(body: int, x: int, y: int, _tick: int) -> void:
	if _bodies.has(body):
		_bodies[body]["tile"] = Vector2i(x, y)
	else:
		_bodies[body] = {"tile": Vector2i(x, y), "shown": Iso.tile_to_screen(x, y)}
	queue_redraw()


func _process(delta: float) -> void:
	var moving := false
	for id: int in _bodies:
		var entry: Dictionary = _bodies[id]
		var target: Vector2 = Iso.tile_to_screen(entry["tile"].x, entry["tile"].y)
		var shown: Vector2 = entry["shown"]
		if shown.distance_to(target) > 0.5:
			entry["shown"] = shown.move_toward(target, WALK_SPEED * Iso.TILE_W * delta)
			moving = true
	if moving:
		queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		var tile := Iso.screen_to_tile(get_local_mouse_position())
		if tile != _hover:
			_hover = tile
			queue_redraw()
	elif event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var tile := Iso.screen_to_tile(get_local_mouse_position())
		if _in_map(tile):
			Net.send_move_to(tile.x, tile.y)


func _in_map(tile: Vector2i) -> bool:
	return tile.x >= 0 and tile.y >= 0 and tile.x < _width and tile.y < _height


func _center_camera() -> void:
	var centre := Iso.tile_to_screen(_width / 2, _height / 2)
	position = get_viewport_rect().size / 2.0 - centre


func _draw() -> void:
	for y in _height:
		for x in _width:
			var terrain: String = str(_tiles[y * _width + x])
			draw_colored_polygon(Iso.diamond(x, y), COLORS.get(terrain, Color.MAGENTA))
	if _in_map(_hover):
		draw_polyline(_closed(Iso.diamond(_hover.x, _hover.y)), Color(1, 1, 1, 0.6), 1.5)
	# Bodies, far to near so nearer ones overlap farther ones.
	var ids := _bodies.keys()
	ids.sort_custom(func(a: int, b: int) -> bool:
		var ta: Vector2i = _bodies[a]["tile"]
		var tb: Vector2i = _bodies[b]["tile"]
		return ta.x + ta.y < tb.x + tb.y)
	for id: int in ids:
		var shown: Vector2 = _bodies[id]["shown"]
		var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
		draw_circle(shown + Vector2(0, -6), 7.0, color)
		draw_circle(shown + Vector2(0, -6), 7.0, Color.BLACK, false, 1.5)


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
