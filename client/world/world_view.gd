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
const DEFAULT_ZOOM := 1.5
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
## body id -> {"at": Vector2 (mm), "route": Array[Vector2] (mm), "speed": mm per second, "asleep": bool}
var _bodies: Dictionary = {}
## chunk position -> Array of items as received, redrawn on change
var _items: Dictionary = {}
## chunk position -> Array of tiles as received
var _tiles: Dictionary = {}
## material id -> edible?
var _edible: Dictionary = {}
## material id -> the whole catalogue entry, for tooltips
var _materials: Dictionary = {}

## Words for what is under the mouse, or "" when nothing is.
signal hovered(text: String)
## A right-click without dragging on a tile: the acts the piloted body
## could try there, as [{"label", "send": Callable, "item": int}].
signal acts_requested(acts: Array, at_screen: Vector2)
## body id -> {"id", "material"} of what it holds, or {}
var _holds: Dictionary = {}
## chunk -> [{"at": Vector2 (millimetres), "species"}], the trees standing
## in it, as the server sent them.
var _trees: Dictionary = {}
## species id -> height in centimetres.
var _tree_heights: Dictionary = {}
## chunk -> where its water runs, tile by tile (protocol `currents`).
var _currents: Dictionary = {}
## The camera's position and zoom at the last drawing.
var _last_view := Vector3.INF
var _drag_start := Vector2.ZERO
var _dragged := false
var _night := false
var _piloted := -1
## Where the piloted body was asked to go, highlighted until it gets there.
var _target := Vector2i(-1, -1)
## The camera follows the piloted body until the player drags it.
var _following := true
## Has the camera been put somewhere sensible since the last world state?
var _camera_placed := false
var _dragging := false
var _hover := Vector2i(-1, -1)


func _ready() -> void:
	add_to_group("world_views")
	_camera.zoom = Vector2.ONE * DEFAULT_ZOOM
	Net.world_state.connect(_on_world_state)
	Net.chunk.connect(_on_chunk)
	Net.species.connect(func(species: Array) -> void:
		_tree_heights.clear()
		for s in species:
			_tree_heights[str(s.get("id", ""))] = int(s.get("height_cm", 400)))
	# Tree images are four times the screen: mipmaps keep them clean when
	# the camera pulls back.
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
	# The trees sway in the wind (trees.gdshader).
	var sway := ShaderMaterial.new()
	sway.shader = load("res://world/trees.gdshader")
	sway.set_shader_parameter("noise_texture", ChunkView.noise_texture())
	material = sway
	Net.chunk_hidden.connect(_on_chunk_hidden)
	Net.body_walks.connect(_on_body_walks)
	Net.body_stopped.connect(_on_body_stopped)
	Net.body_left.connect(_on_body_left)
	Net.body_sleeps.connect(_on_body_sleeps)
	Net.body_holds.connect(func(body: int, item: Dictionary) -> void:
		_holds[body] = item
		queue_redraw())
	Net.body_died.connect(_on_body_died)
	Net.body_assigned.connect(_on_body_assigned)
	Net.item_added.connect(_on_item_added)
	Net.item_removed.connect(_on_item_removed)
	Net.night_fell.connect(func(_day: int) -> void: _set_night(true))
	Net.day_started.connect(func(_day: int, _tick: int) -> void: _set_night(false))
	Net.move_refused.connect(func(_body: int, _x: int, _y: int) -> void: _clear_target())
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


func _on_world_state(width: int, height: int, chunk_size: int, materials: Array, night: bool) -> void:
	_width = width
	_height = height
	ShoreField.map_size = Vector2i(width, height)
	_chunk_size = chunk_size
	_looks.clear()
	_names.clear()
	_edible.clear()
	_materials.clear()
	for m in materials:
		var id := str(m.get("id", ""))
		_looks[id] = Color.from_string(str(m.get("colour", "")), Color.MAGENTA)
		_names[id] = str(m.get("name", id))
		_edible[id] = bool(m.get("edible", false))
		_materials[id] = m
	for chunk: ChunkView in _chunks.values():
		chunk.queue_free()
	_chunks.clear()
	_items.clear()
	_tiles.clear()
	_currents.clear()
	_trees.clear()
	_bodies.clear()
	_following = true
	_camera_placed = false
	_set_night(night)
	queue_redraw()


func _set_night(night: bool) -> void:
	_night = night
	# Provisional graphics: the ground dims at night. SOUL_INVASION_DAYLIGHT
	# keeps it lit, to judge the ground on a development screenshot.
	var lit := night and OS.get_environment("SOUL_INVASION_DAYLIGHT").is_empty()
	modulate = Color(0.45, 0.5, 0.7) if lit else Color.WHITE


func _on_chunk(x: int, y: int, tiles: Array, bodies: Array, items: Array, trees: Array = [], currents: Array = []) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view == null:
		view = ChunkView.new()
		view.z_index = -1  # ground below bodies and the hover outline
		# The ground and the bank pieces keep plain linear filtering: the
		# mipmaps this view uses for the trees would blur them.
		view.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		add_child(view)
		_chunks[key] = view
	_tiles[key] = tiles
	_currents[key] = currents
	_items[key] = items
	var standing: Array = []
	for t in trees:
		standing.append({"at": Net.point(t.get("at", {})), "species": str(t.get("species", ""))})
	_trees[key] = standing
	view.terrain_at = terrain_at
	view.current_at = current_at
	view.show_chunk(key * _chunk_size, _chunk_size, tiles, items, _looks)
	_redraw_neighbours(key)
	_relay_shore()
	if not _camera_placed and _piloted < 0:
		# Watching, no body to follow: look at the ground we were given
		# rather than at the valley's corner.
		var middle := key * _chunk_size + Vector2i(_chunk_size / 2, _chunk_size / 2)
		_camera.position = Iso.tile_to_screen(middle.x, middle.y)
		_camera_placed = true
	for b in bodies:
		var route: Array = []
		for p in b.get("route", []):
			route.append(Net.point(p))
		_place(int(b.get("id", -1)), Net.point(b.get("at", {})), route, int(b.get("speed_cm_per_s", 0)))
		_bodies[int(b.get("id", -1))]["asleep"] = bool(b.get("asleep", false))
		var holds: Variant = b.get("holds")
		_holds[int(b.get("id", -1))] = holds if holds is Dictionary else {}
	queue_redraw()


func _on_item_added(item: Dictionary) -> void:
	var key := Iso.chunk_of(Vector2i(int(item.get("x", 0)), int(item.get("y", 0))), _chunk_size)
	if _chunks.has(key):
		_items[key].append(item)
		_chunks[key].show_chunk(key * _chunk_size, _chunk_size, _tiles[key], _items[key], _looks)


func _on_item_removed(id: int, x: int, y: int) -> void:
	var key := Iso.chunk_of(Vector2i(x, y), _chunk_size)
	if _chunks.has(key):
		var kept: Array = []
		for item in _items[key]:
			if int(item.get("id", -1)) != id:
				kept.append(item)
		_items[key] = kept
		_chunks[key].show_chunk(key * _chunk_size, _chunk_size, _tiles[key], kept, _looks)


func _on_body_assigned(body: int) -> void:
	_piloted = body
	_following = true
	if body >= 0 and _bodies.has(body):
		_camera.position = Iso.point_to_screen(_bodies[body]["at"])
	_camera_placed = body >= 0 and _bodies.has(body)
	queue_redraw()


func _on_body_sleeps(body: int, asleep: bool) -> void:
	if _bodies.has(body):
		_bodies[body]["asleep"] = asleep
		queue_redraw()


func _on_body_died(body: int, _cause: String) -> void:
	_bodies.erase(body)
	queue_redraw()


## Items on the piloted body's tile or next to it, as received.
func items_within_reach() -> Array:
	var found: Array = []
	if not _bodies.has(_piloted):
		return found
	var here := _tile_of(_bodies[_piloted]["at"])
	for key in _items:
		for item in _items[key]:
			var tile := Vector2i(int(item.get("x", 0)), int(item.get("y", 0)))
			if absi(tile.x - here.x) <= 1 and absi(tile.y - here.y) <= 1:
				found.append(item)
	return found


## The id of an edible item within reach, or -1.
func food_within_reach() -> int:
	for item in items_within_reach():
		if _edible.get(str(item.get("material", "")), false):
			return int(item.get("id", -1))
	return -1


## The id of any item within reach, or -1.
func item_within_reach() -> int:
	var items := items_within_reach()
	return int(items[0].get("id", -1)) if not items.is_empty() else -1


## What the piloted body could try on a tile: the client proposes, the
## server judges (D35).
func acts_on(tile: Vector2i) -> Array:
	var acts: Array = []
	for item in items_within_reach():
		if Vector2i(int(item.get("x", 0)), int(item.get("y", 0))) != tile:
			continue
		var id := int(item.get("id", -1))
		var material := str(item.get("material", ""))
		var name := str(_names.get(material, material))
		acts.append({"label": "pick up " + name, "send": Net.send_pick_up, "item": id})
		if _edible.get(material, false):
			acts.append({"label": "eat " + name, "send": Net.send_eat, "item": id})
	return acts


## Frames a tile and stops following the body (development screenshots).
func look_at_tile(tile: Vector2i) -> void:
	_following = false
	_camera_placed = true
	_camera.position = Iso.tile_to_screen(tile.x, tile.y)


## Where the water runs at a tile the server has sent: a unit vector in
## tiles times its speed in metres a second; zero on land or unknown.
func current_at(tile: Vector2i) -> Vector2:
	var key := Iso.chunk_of(tile, _chunk_size)
	var currents: Array = _currents.get(key, [])
	if currents.is_empty():
		return Vector2.ZERO
	var local := tile - key * _chunk_size
	return ShoreField.current_vector(int(currents[local.y * _chunk_size + local.x]))


## The terrain of any tile the server has sent, "" otherwise.
func terrain_at(tile: Vector2i) -> String:
	var key := Iso.chunk_of(tile, _chunk_size)
	var tiles: Array = _tiles.get(key, [])
	if tiles.is_empty():
		return ""
	var local := tile - key * _chunk_size
	return str(tiles[local.y * _chunk_size + local.x])


## Whether a laying of the bank pieces is already due this frame.
var _shore_due := false


## Lays the bank pieces over every chunk known, once per frame at most,
## and hands each chunk its own: laid together, they join at the chunk
## borders without a seam (ShorePieces).
func relay_shore() -> void:
	_relay_shore()


func _relay_shore() -> void:
	if _shore_due or str(ChunkView.ground_spec().get("shore", {}).get("mode", "")) != "pieces":
		return
	_shore_due = true
	_lay_shore.call_deferred()


func _lay_shore() -> void:
	_shore_due = false
	if _chunks.is_empty():
		return
	var lo := Vector2i(1 << 30, 1 << 30)
	var hi := Vector2i(-(1 << 30), -(1 << 30))
	for key in _chunks:
		lo = Vector2i(mini(lo.x, key.x), mini(lo.y, key.y))
		hi = Vector2i(maxi(hi.x, key.x + 1), maxi(hi.y, key.y + 1))
	var is_known := func(t: Vector2i) -> bool: return terrain_at(t) != ""
	var is_water := func(t: Vector2i) -> bool:
		var terrain := terrain_at(t)
		return terrain == "water" or terrain == "ford"
	var pieces := ShorePieces.lay_all(is_known, is_water, lo * _chunk_size, hi * _chunk_size)
	var by_chunk: Dictionary = {}
	for piece in pieces:
		var pts: PackedVector2Array = piece["pts"]
		var mid: Vector2 = pts[pts.size() / 2]
		var key := Iso.chunk_of(Vector2i(int(round(mid.x)), int(round(mid.y))), _chunk_size)
		if not by_chunk.has(key):
			by_chunk[key] = []
		by_chunk[key].append(piece)
	for key in _chunks:
		var view: ChunkView = _chunks[key]
		view.bank_pieces = by_chunk.get(key, [])
		view.queue_redraw()


## The chunks around `key` draw their borders from their neighbours'
## tiles: redraw them when `key` appears or goes.
func _redraw_neighbours(key: Vector2i) -> void:
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			var near: ChunkView = _chunks.get(key + Vector2i(dx, dy))
			if near != null and (dx != 0 or dy != 0):
				near.queue_redraw()


func _on_chunk_hidden(x: int, y: int) -> void:
	var key := Vector2i(x, y)
	var view: ChunkView = _chunks.get(key)
	if view != null:
		view.queue_free()
		_chunks.erase(key)
		_items.erase(key)
		_tiles.erase(key)
		_currents.erase(key)
		_trees.erase(key)
		_redraw_neighbours(key)
		_relay_shore()
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
	_holds.erase(body)
	queue_redraw()


## Puts a body somewhere with the route it is walking (empty: standing).
func _place(body: int, at: Vector2, route: Array, speed_cm_per_s: int) -> void:
	var first_sight := not _bodies.has(body)
	var asleep: bool = _bodies[body]["asleep"] if not first_sight else false
	_bodies[body] = {"at": at, "route": route, "speed": speed_cm_per_s * 10.0, "asleep": asleep}
	if first_sight and body == _piloted:
		_camera.position = Iso.point_to_screen(at)
		_camera_placed = true


func _clear_target() -> void:
	_target = Vector2i(-1, -1)
	queue_redraw()


## The tile a fine point (millimetres) is in.
static func _tile_of(mm: Vector2) -> Vector2i:
	return Vector2i(floori(mm.x / MM_PER_TILE), floori(mm.y / MM_PER_TILE))


## What stands or lies on a tile, as words for a tooltip.
func _describe(tile: Vector2i) -> String:
	var lines: PackedStringArray = []
	for id: int in _bodies:
		if _tile_of(_bodies[id]["at"]) == tile:
			var line := Tooltip.describe_body(id, _bodies[id]["asleep"], id == _piloted)
			var holds: Dictionary = _holds.get(id, {})
			if not holds.is_empty():
				line += " · holds " + str(_materials.get(str(holds.get("material", "")), {}).get("name", "?"))
			lines.append(line)
	var key := Iso.chunk_of(tile, _chunk_size)
	for item in _items.get(key, []):
		if Vector2i(int(item.get("x", 0)), int(item.get("y", 0))) == tile:
			lines.append(Tooltip.describe_item(_materials.get(str(item.get("material", "")), {})))
	return "\n".join(lines)


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
	# Only the trees in view are drawn: redraw when the view moves. The
	# sway itself is the shader's, every frame, with no redraw.
	var view := Vector3(_camera.position.x, _camera.position.y, _camera.zoom.x)
	if moving or view != _last_view:
		_last_view = view
		queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		if _dragging:
			_camera.position -= event.relative / _camera.zoom.x
			if event.position.distance_to(_drag_start) > 4.0:
				_dragged = true
				_following = false
		var tile := Iso.screen_to_tile(get_local_mouse_position())
		if tile != _hover:
			_hover = tile
			hovered.emit(_describe(tile))
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
					_drag_start = event.position
					_dragged = false
				elif event.button_index == MOUSE_BUTTON_RIGHT and not _dragged:
					# A right-click without dragging: what can be done here?
					var tile := Iso.screen_to_tile(get_local_mouse_position())
					var acts := acts_on(tile)
					if not acts.is_empty():
						acts_requested.emit(acts, event.position)
			MOUSE_BUTTON_WHEEL_UP:
				if event.pressed:
					_zoom_by(ZOOM_STEP)
			MOUSE_BUTTON_WHEEL_DOWN:
				if event.pressed:
					_zoom_by(1.0 / ZOOM_STEP)
	elif event is InputEventKey and event.pressed:
		match event.keycode:
			KEY_SPACE:
				_following = true
			KEY_E:
				var food := food_within_reach()
				if food >= 0:
					Net.send_eat(food)
			KEY_G:
				var item := item_within_reach()
				if item >= 0:
					Net.send_pick_up(item)
			KEY_Z:
				if _bodies.has(_piloted) and _bodies[_piloted]["asleep"]:
					Net.send_wake()
				else:
					Net.send_sleep()


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
	# Trees and bodies, far to near, so nearer ones overlap farther ones
	# and a body walks behind a trunk. Trees off screen are left out.
	var seen := get_canvas_transform().affine_inverse() * get_viewport_rect()
	seen = seen.grow(Iso.PX_PER_M * 6.0)
	var standing: Array = []
	for key in _trees:
		for tree: Dictionary in _trees[key]:
			if seen.has_point(Iso.point_to_screen(tree["at"])):
				standing.append([tree["at"].x + tree["at"].y, 0, tree])
	for id: int in _bodies:
		var at: Vector2 = _bodies[id]["at"]
		standing.append([at.x + at.y, 1, id])
	standing.sort_custom(func(a: Array, b: Array) -> bool: return a[0] < b[0])
	for entry: Array in standing:
		if entry[1] == 0:
			var tree: Dictionary = entry[2]
			Trees.draw(self, tree, int(_tree_heights.get(tree["species"], 400)))
		else:
			_draw_body(entry[2])


func _draw_body(id: int) -> void:
	var shown: Vector2 = Iso.point_to_screen(_bodies[id]["at"])
	var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
	var feet := shown + Vector2(0, 2)
	# Provisional figures: a third of a person's 1.7 m, so they stay
	# small on the diamond until real sprites come (64 px per metre).
	var h := Iso.PX_PER_M * 1.7 * 0.33
	if _bodies[id]["asleep"]:
		# Lying down: a figure along the ground, and a "z" above it.
		draw_rect(Rect2(feet + Vector2(-h * 0.6, -h * 0.35), Vector2(h, h * 0.3)), color)
		draw_rect(Rect2(feet + Vector2(-h * 0.6, -h * 0.35), Vector2(h, h * 0.3)), Color.BLACK, false, 1.0)
		draw_circle(feet + Vector2(h * 0.6, -h * 0.2), h * 0.18, color)
		draw_circle(feet + Vector2(h * 0.6, -h * 0.2), h * 0.18, Color.BLACK, false, 1.0)
		draw_sleep_icon(self, feet + Vector2(0, -h * 0.7))
	else:
		# A standing figure: taller than wide, feet where the body stands.
		draw_rect(Rect2(feet + Vector2(-h * 0.18, -h), Vector2(h * 0.36, h * 0.9)), color)
		draw_rect(Rect2(feet + Vector2(-h * 0.18, -h), Vector2(h * 0.36, h * 0.9)), Color.BLACK, false, 1.0)
		draw_circle(feet + Vector2(0, -h * 1.18), h * 0.2, color)
		draw_circle(feet + Vector2(0, -h * 1.18), h * 0.2, Color.BLACK, false, 1.0)
		var holds: Dictionary = _holds.get(id, {})
		if not holds.is_empty():
			# What is in hand, in its material's colour, by the right hand.
			var held_colour: Color = _looks.get(str(holds.get("material", "")), Color.MAGENTA)
			draw_circle(feet + Vector2(h * 0.32, -h * 0.45), h * 0.14, held_colour)
			draw_circle(feet + Vector2(h * 0.32, -h * 0.45), h * 0.14, Color.BLACK, false, 1.0)


## A small "z" above a sleeping body, drawn on `canvas` at `at`.
static func draw_sleep_icon(canvas: CanvasItem, at: Vector2) -> void:
	var font := ThemeDB.fallback_font
	canvas.draw_string_outline(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, 3, Color.BLACK)
	canvas.draw_string(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.75, 0.85, 1.0))


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
