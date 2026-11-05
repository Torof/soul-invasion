class_name WorldView
extends Node2D
## Shows the ground the server sent (one ChunkView per chunk) and the
## bodies in it, drives the camera, and turns clicks into `move_to`
## intentions. Everything shown comes from the server; nothing is
## decided here (D03, D05).
##
## Bodies walk along the routes the server gives them (D32): the client
## only interpolates between waypoints at the given speed, and snaps to
## wherever the server says they stopped.
##
## Camera: mouse wheel zooms, dragging with the right or middle button
## pans, Space goes back to following the piloted body.

const BODY_COLOR := Color(0.90, 0.80, 0.60)
const PILOTED_COLOR := Color(1.0, 0.55, 0.20)
const TARGET_COLOR := Color(1.0, 0.85, 0.30, 0.9)
const ZOOM_MIN := 0.5
const ZOOM_MAX := 4.0
const ZOOM_STEP := 1.25
const DEFAULT_ZOOM := 2.0
## Millimetres per tile, as the protocol defines fine points.
const MM_PER_TILE := 1000.0

@onready var _camera: Camera2D = $Camera

var _width := 0
var _height := 0
var _chunk_size := 32
## chunk position -> ChunkView (only the chunks the server sent)
var _chunks: Dictionary = {}
## material id -> Color, from the server's catalogue
var _looks: Dictionary = {}
## material id -> name shown to players
var _names: Dictionary = {}
## body id -> {"at": Vector2 (mm), "route": Array[Vector2] (mm), "speed": mm per second}
var _bodies: Dictionary = {}
var _piloted := -1
## Where the piloted body was asked to go, highlighted until it gets there.
var _target := Vector2i(-1, -1)
## The camera follows the piloted body until the player drags it.
var _following := true
var _dragging := false
var _hover := Vector2i(-1, -1)


func _ready() -> void:
	_camera.zoom = Vector2.ONE * DEFAULT_ZOOM
	Net.world_state.connect(_on_world_state)
	Net.chunk.connect(_on_chunk)
	Net.chunk_hidden.connect(_on_chunk_hidden)
	Net.body_walks.connect(_on_body_walks)
	Net.body_stopped.connect(_on_body_stopped)
	Net.body_left.connect(_on_body_left)
	Net.move_refused.connect(func(_body: int, _x: int, _y: int) -> void: _clear_target())
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


func _on_world_state(width: int, height: int, chunk_size: int, materials: Array) -> void:
	_width = width
	_height = height
	_chunk_size = chunk_size
	_looks.clear()
	_names.clear()
	for m in materials:
		var id := str(m.get("id", ""))
		_looks[id] = Color.from_string(str(m.get("colour", "")), Color.MAGENTA)
		_names[id] = str(m.get("name", id))
	for chunk: ChunkView in _chunks.values():
		chunk.queue_free()
	_chunks.clear()
	_bodies.clear()
	_following = true
	queue_redraw()


func _on_chunk(x: int, y: int, tiles: Array, bodies: Array, items: Array) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view == null:
		view = ChunkView.new()
		view.z_index = -1  # ground below bodies and the hover outline
		add_child(view)
		_chunks[key] = view
	view.show_chunk(key * _chunk_size, _chunk_size, tiles, items, _looks)
	for b in bodies:
		var route: Array = []
		for p in b.get("route", []):
			route.append(Net.point(p))
		_place(int(b.get("id", -1)), Net.point(b.get("at", {})), route, int(b.get("speed_cm_per_s", 0)))
	queue_redraw()


func _on_chunk_hidden(x: int, y: int) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view != null:
		view.queue_free()
		_chunks.erase(key)
	for id in _bodies.keys():
		if Iso.chunk_of(_tile_of(_bodies[id]["at"]), _chunk_size) == key:
			_bodies.erase(id)
	queue_redraw()


func _on_body_walks(body: int, at: Vector2, route: Array, speed_cm_per_s: int) -> void:
	_place(body, at, route, speed_cm_per_s)
	queue_redraw()


func _on_body_stopped(body: int, at: Vector2) -> void:
	_place(body, at, [], 0)
	if body == _piloted and _tile_of(at) == _target:
		_clear_target()
	queue_redraw()


func _on_body_left(body: int) -> void:
	_bodies.erase(body)
	queue_redraw()


## Puts a body somewhere with the route it is walking (empty: standing).
func _place(body: int, at: Vector2, route: Array, speed_cm_per_s: int) -> void:
	var first_sight := not _bodies.has(body)
	_bodies[body] = {"at": at, "route": route, "speed": speed_cm_per_s * 10.0}
	if first_sight and body == _piloted:
		_camera.position = Iso.point_to_screen(at)


func _clear_target() -> void:
	_target = Vector2i(-1, -1)
	queue_redraw()


## The tile a fine point (millimetres) is in.
static func _tile_of(mm: Vector2) -> Vector2i:
	return Vector2i(floori(mm.x / MM_PER_TILE), floori(mm.y / MM_PER_TILE))


## Zoom clamped to the allowed range.
static func clamp_zoom(zoom: float) -> float:
	return clampf(zoom, ZOOM_MIN, ZOOM_MAX)


func _process(delta: float) -> void:
	var moving := false
	for id: int in _bodies:
		var entry: Dictionary = _bodies[id]
		var route: Array = entry["route"]
		if route.is_empty():
			continue
		# Walk along the route at the nominal speed; the server's stop
		# message snaps the body wherever it really ended.
		var budget: float = entry["speed"] * delta
		var at: Vector2 = entry["at"]
		while budget > 0.0 and not route.is_empty():
			var next: Vector2 = route[0]
			var distance := at.distance_to(next)
			if distance <= budget:
				at = next
				route.pop_front()
				budget -= distance
			else:
				at = at.move_toward(next, budget)
				budget = 0.0
		entry["at"] = at
		moving = true
	if _following and _bodies.has(_piloted):
		_camera.position = Iso.point_to_screen(_bodies[_piloted]["at"])
	if moving:
		queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		if _dragging:
			_camera.position -= event.relative / _camera.zoom.x
		var tile := Iso.screen_to_tile(get_local_mouse_position())
		if tile != _hover:
			_hover = tile
			queue_redraw()
	elif event is InputEventMouseButton:
		match event.button_index:
			MOUSE_BUTTON_LEFT:
				if event.pressed:
					var tile := Iso.screen_to_tile(get_local_mouse_position())
					if _in_map(tile) and Net.body >= 0:
						Net.send_move_to(tile.x, tile.y)
						_target = tile
						queue_redraw()
			MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE:
				_dragging = event.pressed
				if event.pressed:
					_following = false
			MOUSE_BUTTON_WHEEL_UP:
				if event.pressed:
					_zoom_by(ZOOM_STEP)
			MOUSE_BUTTON_WHEEL_DOWN:
				if event.pressed:
					_zoom_by(1.0 / ZOOM_STEP)
	elif event is InputEventKey and event.pressed and event.keycode == KEY_SPACE:
		_following = true


func _zoom_by(factor: float) -> void:
	var zoom := clamp_zoom(_camera.zoom.x * factor)
	_camera.zoom = Vector2.ONE * zoom


func _in_map(tile: Vector2i) -> bool:
	return tile.x >= 0 and tile.y >= 0 and tile.x < _width and tile.y < _height


func _draw() -> void:
	if _in_map(_target):
		draw_polyline(_closed(Iso.diamond(_target.x, _target.y)), TARGET_COLOR, 2.5)
	if _in_map(_hover):
		draw_polyline(_closed(Iso.diamond(_hover.x, _hover.y)), Color(1, 1, 1, 0.6), 1.5)
	# Bodies, far to near so nearer ones overlap farther ones.
	var ids := _bodies.keys()
	ids.sort_custom(func(a: int, b: int) -> bool:
		var pa: Vector2 = _bodies[a]["at"]
		var pb: Vector2 = _bodies[b]["at"]
		return pa.x + pa.y < pb.x + pb.y)
	for id: int in ids:
		var shown: Vector2 = Iso.point_to_screen(_bodies[id]["at"])
		var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
		# A standing figure: taller than wide, feet where the body stands.
		var feet := shown + Vector2(0, 2)
		draw_rect(Rect2(feet + Vector2(-4, -22), Vector2(8, 20)), color)
		draw_rect(Rect2(feet + Vector2(-4, -22), Vector2(8, 20)), Color.BLACK, false, 1.0)
		draw_circle(feet + Vector2(0, -26), 4.5, color)
		draw_circle(feet + Vector2(0, -26), 4.5, Color.BLACK, false, 1.0)


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
