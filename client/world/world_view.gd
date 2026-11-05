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
var _chunk_size := 32
## chunk position -> Array of tiles, row by row (only the chunks the server sent)
var _chunks: Dictionary = {}
## body id -> {"tile": Vector2i, "shown": Vector2 (screen, glides)}
var _bodies: Dictionary = {}
var _piloted := -1
var _centered := false
var _hover := Vector2i(-1, -1)


func _ready() -> void:
	Net.world_state.connect(_on_world_state)
	Net.chunk.connect(_on_chunk)
	Net.chunk_hidden.connect(_on_chunk_hidden)
	Net.body_moved.connect(_on_body_moved)
	Net.body_left.connect(_on_body_left)
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


func _on_world_state(width: int, height: int, chunk_size: int) -> void:
	_width = width
	_height = height
	_chunk_size = chunk_size
	_chunks.clear()
	_bodies.clear()
	_centered = false
	queue_redraw()


func _on_chunk(x: int, y: int, tiles: Array, bodies: Array) -> void:
	_chunks[Vector2i(x, y)] = tiles
	for b in bodies:
		_place(int(b.get("id", -1)), Vector2i(int(b.get("x", 0)), int(b.get("y", 0))))
	queue_redraw()


func _on_chunk_hidden(x: int, y: int) -> void:
	var chunk := Vector2i(x, y)
	_chunks.erase(chunk)
	for id in _bodies.keys():
		if _chunk_of(_bodies[id]["tile"]) == chunk:
			_bodies.erase(id)
	queue_redraw()


func _on_body_moved(body: int, x: int, y: int, _tick: int) -> void:
	_place(body, Vector2i(x, y))
	queue_redraw()


func _on_body_left(body: int) -> void:
	_bodies.erase(body)
	queue_redraw()


## Puts a body on a tile, gliding there if it was already shown. The view
## centres on the piloted body the first time it is seen.
func _place(body: int, tile: Vector2i) -> void:
	if _bodies.has(body):
		_bodies[body]["tile"] = tile
	else:
		_bodies[body] = {"tile": tile, "shown": Iso.tile_to_screen(tile.x, tile.y)}
	if body == _piloted and not _centered:
		_centered = true
		_center_camera(tile)


func _chunk_of(tile: Vector2i) -> Vector2i:
	return Vector2i(floori(float(tile.x) / _chunk_size), floori(float(tile.y) / _chunk_size))


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


func _center_camera(tile: Vector2i) -> void:
	var centre := Iso.tile_to_screen(tile.x, tile.y)
	position = get_viewport_rect().size / 2.0 - centre


func _draw() -> void:
	# Chunks from far to near so nearer rows overlap farther ones.
	var chunks := _chunks.keys()
	chunks.sort_custom(func(a: Vector2i, b: Vector2i) -> bool: return a.x + a.y < b.x + b.y)
	for chunk: Vector2i in chunks:
		var tiles: Array = _chunks[chunk]
		var origin := chunk * _chunk_size
		for y in _chunk_size:
			for x in _chunk_size:
				var terrain: String = str(tiles[y * _chunk_size + x])
				draw_colored_polygon(Iso.diamond(origin.x + x, origin.y + y), COLORS.get(terrain, Color.MAGENTA))
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
