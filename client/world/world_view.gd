class_name WorldView
extends Node2D
## Shows the ground the server sent (one ChunkView per chunk) and the
## bodies in it, drives the camera, and turns clicks into `move_to`
## intentions. Everything shown comes from the server; nothing is
## decided here (D03, D05).
##
## Camera: mouse wheel zooms, dragging with the right or middle button
## pans, Space goes back to following the piloted body.

const BODY_COLOR := Color(0.90, 0.80, 0.60)
const PILOTED_COLOR := Color(1.0, 0.55, 0.20)
const TARGET_COLOR := Color(1.0, 0.85, 0.30, 0.9)
## Seconds a body is assumed to take per step before its pace is measured.
const DEFAULT_STEP_SECONDS := 0.7
const ZOOM_MIN := 0.5
const ZOOM_MAX := 4.0
const ZOOM_STEP := 1.25
const DEFAULT_ZOOM := 2.0

@onready var _camera: Camera2D = $Camera

var _width := 0
var _height := 0
var _chunk_size := 32
## chunk position -> ChunkView (only the chunks the server sent)
var _chunks: Dictionary = {}
## body id -> {"tile": Vector2i, "shown": Vector2 (screen, glides),
##   "step": seconds per step as measured, "last_move": msec of the last move}
## The glide lasts one measured step, so a walking body never stops between
## tiles: it is always one step behind the server, and continuous.
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
	Net.body_moved.connect(_on_body_moved)
	Net.body_left.connect(_on_body_left)
	Net.move_refused.connect(func(_body: int, _x: int, _y: int) -> void: _clear_target())
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


func _on_world_state(width: int, height: int, chunk_size: int) -> void:
	_width = width
	_height = height
	_chunk_size = chunk_size
	for chunk: ChunkView in _chunks.values():
		chunk.queue_free()
	_chunks.clear()
	_bodies.clear()
	_following = true
	queue_redraw()


func _on_chunk(x: int, y: int, tiles: Array, bodies: Array) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view == null:
		view = ChunkView.new()
		view.z_index = -1  # ground below bodies and the hover outline
		add_child(view)
		_chunks[key] = view
	view.show_tiles(key * _chunk_size, _chunk_size, tiles)
	for b in bodies:
		_place(int(b.get("id", -1)), Vector2i(int(b.get("x", 0)), int(b.get("y", 0))))
	queue_redraw()


func _on_chunk_hidden(x: int, y: int) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view != null:
		view.queue_free()
		_chunks.erase(key)
	for id in _bodies.keys():
		if Iso.chunk_of(_bodies[id]["tile"], _chunk_size) == key:
			_bodies.erase(id)
	queue_redraw()


func _on_body_moved(body: int, x: int, y: int, _tick: int) -> void:
	_place(body, Vector2i(x, y))
	if body == _piloted and Vector2i(x, y) == _target:
		_clear_target()
	queue_redraw()


func _on_body_left(body: int) -> void:
	_bodies.erase(body)
	queue_redraw()


## Puts a body on a tile, gliding there if it was already shown, at the
## pace measured between its last two moves.
func _place(body: int, tile: Vector2i) -> void:
	var now := Time.get_ticks_msec()
	if _bodies.has(body):
		var entry: Dictionary = _bodies[body]
		entry["tile"] = tile
		if entry["last_move"] >= 0:
			var measured: float = clampf((now - entry["last_move"]) / 1000.0, 0.05, 3.0)
			entry["step"] = lerpf(entry["step"], measured, 0.5)
		entry["last_move"] = now
	else:
		_bodies[body] = {
			"tile": tile,
			"shown": Iso.tile_to_screen(tile.x, tile.y),
			"step": DEFAULT_STEP_SECONDS,
			"last_move": -1,
		}
		if body == _piloted:
			_camera.position = _bodies[body]["shown"]


func _clear_target() -> void:
	_target = Vector2i(-1, -1)
	queue_redraw()


## Zoom clamped to the allowed range.
static func clamp_zoom(zoom: float) -> float:
	return clampf(zoom, ZOOM_MIN, ZOOM_MAX)


func _process(delta: float) -> void:
	var moving := false
	for id: int in _bodies:
		var entry: Dictionary = _bodies[id]
		var target: Vector2 = Iso.tile_to_screen(entry["tile"].x, entry["tile"].y)
		var shown: Vector2 = entry["shown"]
		if shown.distance_to(target) > 0.5:
			# One tile's width per measured step: a straight step covers
			# TILE_W, so the glide ends when the next move is due.
			var speed: float = Iso.TILE_W / entry["step"]
			entry["shown"] = shown.move_toward(target, speed * delta)
			moving = true
	if _following and _bodies.has(_piloted):
		_camera.position = _bodies[_piloted]["shown"]
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
		var ta: Vector2i = _bodies[a]["tile"]
		var tb: Vector2i = _bodies[b]["tile"]
		return ta.x + ta.y < tb.x + tb.y)
	for id: int in ids:
		var shown: Vector2 = _bodies[id]["shown"]
		var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
		# A standing figure: taller than wide, feet on the tile's centre.
		var feet := shown + Vector2(0, 2)
		draw_rect(Rect2(feet + Vector2(-4, -22), Vector2(8, 20)), color)
		draw_rect(Rect2(feet + Vector2(-4, -22), Vector2(8, 20)), Color.BLACK, false, 1.0)
		draw_circle(feet + Vector2(0, -26), 4.5, color)
		draw_circle(feet + Vector2(0, -26), 4.5, Color.BLACK, false, 1.0)


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
