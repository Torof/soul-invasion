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
const ZOOM_MIN := 0.35
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
## A development teleport was asked (the ground there is on its way).
signal teleporting
## body id -> {"id", "material"} of what it holds, or {}
var _holds: Dictionary = {}
## chunk -> [{"at": Vector2 (millimetres), "species"}], the trees standing
## in it, as the server sent them.
var _trees: Dictionary = {}
## species id -> height in centimetres.
var _tree_heights: Dictionary = {}
## chunk -> where its water runs, tile by tile (protocol `currents`).
var _currents: Dictionary = {}
## Seeing through trees: how far the circles around the body and the
## mouse are open (eased), and whether each should be.
var _body_reveal := 0.0
var _mouse_reveal := 0.0
var _body_hidden := false
var _mouse_hidden := false
const BODY_SEE_M := 1.5
const MOUSE_SEE_M := 1.2
## Seconds for a circle to open or close.
const SEE_EASE := 0.2
## Chunks whose borders must be drawn again because a neighbour came or
## went: a few a frame, so a chunk's arrival does not freeze the game.
var _to_redraw: Array = []
const REDRAWS_PER_FRAME := 2
## The layer the trees and bodies stand on, sorted by their feet.
var _standing: Node2D
## chunk -> the node holding its trees.
var _tree_nodes: Dictionary = {}
## Chunk key -> {screen cell: [trees whose image covers that cell]}, cells
## of TREE_CELL_PX: the trees under a point are found without looking at
## every tree (see-through).
var _tree_cells: Dictionary = {}
const TREE_CELL_PX := 128.0
## body id -> the node drawing it.
var _body_nodes: Dictionary = {}
## The trees whose see-through flags are set, to clear them later.
var _flagged: Dictionary = {}
## What the see-through flags were last worked out for (the mouse, the
## bodies and where they stand), and how long ago.
var _flags_looked: Array = []
var _flags_age := 0.0
const FLAGS_REFRESH := 0.25
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
	# Trees and bodies stand on one layer that sorts them by their feet
	# (y-sort): a body walks behind a trunk. The trees sway in the wind and
	# fade where they hide the body or the mouse (trees.gdshader).
	var sway := ShaderMaterial.new()
	sway.shader = load("res://world/trees.gdshader")
	sway.set_shader_parameter("noise_texture", ChunkView.noise_texture())
	_standing = Node2D.new()
	_standing.y_sort_enabled = true
	_standing.material = sway
	add_child(_standing)
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
	_tree_cells.clear()
	for n: Node in _tree_nodes.values():
		n.queue_free()
	_tree_nodes.clear()
	for n: Node in _body_nodes.values():
		n.queue_free()
	_body_nodes.clear()
	_flagged.clear()
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
	_plant_chunk(key, view)
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


## Development only (--dev servers): asks for the piloted body to stand
## near `tile` at once, and follows it there.
func teleport_to(tile: Vector2i) -> void:
	if not Net.dev or Net.body < 0:
		return
	Net.send_dev_teleport(tile.x, tile.y)
	_target = Vector2i(-1, -1)
	_following = true
	teleporting.emit()
	queue_redraw()


## Development (--dev): the wind on or off in the trees, to measure what
## it costs (trees.gdshader's sway and gust; their defaults when on).
func set_wind(on: bool) -> void:
	var sway := _standing.material as ShaderMaterial
	if sway == null:
		return
	sway.set_shader_parameter("sway", 0.005 if on else 0.0)
	sway.set_shader_parameter("gust", 0.012 if on else 0.0)


## What the world holds now, for the development counters.
func counts() -> Dictionary:
	var trees := 0
	for holder: Variant in _tree_nodes.values():
		if is_instance_valid(holder):
			trees += (holder as Node).get_child_count()
	return {"chunks": _chunks.size(), "trees": trees, "bodies": _bodies.size()}


## The tile the piloted body stands on, or (-1, -1) when there is none.
func piloted_tile() -> Vector2i:
	if not _bodies.has(_piloted):
		return Vector2i(-1, -1)
	return _tile_of(_bodies[_piloted]["at"])


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
			if near != null and (dx != 0 or dy != 0) and not _to_redraw.has(near):
				_to_redraw.append(near)


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
		_tree_cells.erase(key)
		var gone: Node = _tree_nodes.get(key)
		if gone != null:
			gone.queue_free()
			_tree_nodes.erase(key)
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


## Lets the waiting chunk nearest the camera build its ground (one a
## frame, ChunkView.waiting).
func _call_waiting_chunk() -> void:
	var best: ChunkView = null
	var best_distance := INF
	for chunk: Variant in ChunkView.waiting.duplicate():
		if not is_instance_valid(chunk) or not (chunk as ChunkView).is_inside_tree():
			ChunkView.waiting.erase(chunk)
			continue
		var view := chunk as ChunkView
		var middle := Iso.tile_to_screen(view._origin.x + view._size / 2, view._origin.y + view._size / 2)
		var distance := middle.distance_squared_to(_camera.position)
		if distance < best_distance:
			best_distance = distance
			best = view
	if best != null:
		ChunkView.waiting.erase(best)
		best.queue_redraw()


func _process(delta: float) -> void:
	var timing := DevTiming.start()
	_step(delta)
	DevTiming.add("world view", timing)


func _step(delta: float) -> void:
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
	_update_see_through(delta)
	_call_waiting_chunk()
	for i in mini(REDRAWS_PER_FRAME, _to_redraw.size()):
		# A chunk hidden since it was queued is freed: check before typing.
		var queued: Variant = _to_redraw.pop_front()
		if is_instance_valid(queued):
			(queued as ChunkView).queue_redraw()
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
					if event.ctrl_pressed and Net.dev:
						# Development (--dev): there at once.
						teleport_to(tile)
					elif _in_map(tile) and Net.body >= 0:
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


## Draws body `id` on `canvas`, whose origin stands at its feet.
func draw_body(canvas: CanvasItem, id: int) -> void:
	if not _bodies.has(id):
		return
	var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
	var feet := Vector2.ZERO
	# Provisional figures: a third of a person's 1.7 m, so they stay
	# small on the diamond until real sprites come (64 px per metre).
	var h := Iso.PX_PER_M * 1.7 * 0.33
	if _bodies[id]["asleep"]:
		# Lying down: a figure along the ground, and a "z" above it.
		canvas.draw_rect(Rect2(feet + Vector2(-h * 0.6, -h * 0.35), Vector2(h, h * 0.3)), color)
		canvas.draw_rect(Rect2(feet + Vector2(-h * 0.6, -h * 0.35), Vector2(h, h * 0.3)), Color.BLACK, false, 1.0)
		canvas.draw_circle(feet + Vector2(h * 0.6, -h * 0.2), h * 0.18, color)
		canvas.draw_circle(feet + Vector2(h * 0.6, -h * 0.2), h * 0.18, Color.BLACK, false, 1.0)
		draw_sleep_icon(canvas, feet + Vector2(0, -h * 0.7))
	else:
		# A standing figure: taller than wide, feet where the body stands.
		canvas.draw_rect(Rect2(feet + Vector2(-h * 0.18, -h), Vector2(h * 0.36, h * 0.9)), color)
		canvas.draw_rect(Rect2(feet + Vector2(-h * 0.18, -h), Vector2(h * 0.36, h * 0.9)), Color.BLACK, false, 1.0)
		canvas.draw_circle(feet + Vector2(0, -h * 1.18), h * 0.2, color)
		canvas.draw_circle(feet + Vector2(0, -h * 1.18), h * 0.2, Color.BLACK, false, 1.0)
		var holds: Dictionary = _holds.get(id, {})
		if not holds.is_empty():
			# What is in hand, in its material's colour, by the right hand.
			var held_colour: Color = _looks.get(str(holds.get("material", "")), Color.MAGENTA)
			canvas.draw_circle(feet + Vector2(h * 0.32, -h * 0.45), h * 0.14, held_colour)
			canvas.draw_circle(feet + Vector2(h * 0.32, -h * 0.45), h * 0.14, Color.BLACK, false, 1.0)


## The trees of chunk `key` as nodes on the standing layer, and their
## shadows on the chunk's ground.
func _plant_chunk(key: Vector2i, view: ChunkView) -> void:
	var old: Node = _tree_nodes.get(key)
	if old != null:
		old.queue_free()
	var holder := Node2D.new()
	holder.y_sort_enabled = true
	holder.use_parent_material = true
	var cells := {}
	for tree: Dictionary in _trees[key]:
		var height := int(_tree_heights.get(tree["species"], 400))
		var node := Trees.TreeNode.new(tree, height)
		tree["node"] = node
		holder.add_child(node)
		var images := Trees.textures(str(tree["species"]))
		if images.is_empty():
			continue
		var at: Vector2 = tree["at"]
		var r := Trees.rect(at, height, images[Trees.variant(at, images.size())])
		tree["rect"] = r
		var lo := (r.position / TREE_CELL_PX).floor()
		var hi := (r.end / TREE_CELL_PX).floor()
		for cy in range(int(lo.y), int(hi.y) + 1):
			for cx in range(int(lo.x), int(hi.x) + 1):
				var cell := Vector2i(cx, cy)
				if not cells.has(cell):
					cells[cell] = []
				cells[cell].append(tree)
	_tree_cells[key] = cells
	_standing.add_child(holder)
	_tree_nodes[key] = holder
	var shadows: Trees.ShadowLayer = view.get_node_or_null("Shadows")
	if shadows == null:
		shadows = Trees.ShadowLayer.new()
		shadows.name = "Shadows"
		view.add_child(shadows)
	shadows.trees = _trees[key]
	shadows.heights = _tree_heights
	shadows.queue_redraw()


## One node per body on the standing layer, at its feet; redrawn every
## frame (a handful of bodies, their pose and what they hold change).
func _sync_bodies() -> void:
	for id: int in _body_nodes.keys():
		if not _bodies.has(id):
			_body_nodes[id].queue_free()
			_body_nodes.erase(id)
	for id: int in _bodies:
		var node: BodyNode = _body_nodes.get(id)
		if node == null:
			node = BodyNode.new()
			node.view = self
			node.id = id
			node.use_parent_material = true
			_standing.add_child(node)
			_body_nodes[id] = node
		node.position = Iso.point_to_screen(_bodies[id]["at"]) + Vector2(0, 2)
		node.queue_redraw()


## A body on the standing layer; the world view draws it.
class BodyNode:
	extends Node2D
	var view: Node
	var id := -1

	func _draw() -> void:
		view.draw_body(self, id)


## The see-through flags of the trees near the body and the mouse; the
## others are cleared. Only the trees in the chunks around them are
## looked at.
func _update_flags(mouse: Vector2) -> void:
	_body_hidden = false
	_mouse_hidden = false
	var keys := {}
	var places: Array = [Iso.screen_to_tile(mouse)]
	if _bodies.has(_piloted):
		places.append(_tile_of(_bodies[_piloted]["at"]))
	for tile: Vector2i in places:
		var c := Iso.chunk_of(tile, _chunk_size)
		for dy in range(-1, 2):
			for dx in range(-1, 2):
				keys[c + Vector2i(dx, dy)] = true
	# What lies near the mouse is looked for once a frame, not once a tree;
	# only the trees whose image covers the mouse or the body's circle are
	# looked at, through the cells of the screen they cover.
	var behind := _nearest_depth_near(mouse)
	var wanted: Array[Vector2i] = [Vector2i((mouse / TREE_CELL_PX).floor())]
	if _bodies.has(_piloted):
		var radius := BODY_SEE_M * Iso.PX_PER_M
		var centre := _body_see_centre()
		var lo := ((centre - Vector2(radius, radius)) / TREE_CELL_PX).floor()
		var hi := ((centre + Vector2(radius, radius)) / TREE_CELL_PX).floor()
		for cy in range(int(lo.y), int(hi.y) + 1):
			for cx in range(int(lo.x), int(hi.x) + 1):
				wanted.append(Vector2i(cx, cy))
	var seen := {}
	var now := {}
	for key in keys:
		var cells: Dictionary = _tree_cells.get(key, {})
		for cell in wanted:
			for tree: Dictionary in cells.get(cell, []):
				var node: Trees.TreeNode = tree.get("node")
				if node == null or seen.has(node):
					continue
				seen[node] = true
				var f := _see_through_flags(tree, node.height_cm, mouse, behind)
				if f != 0:
					now[node] = true
					node.set_flags(f)
	for node: Variant in _flagged:
		if is_instance_valid(node) and not now.has(node):
			(node as Trees.TreeNode).set_flags(0)
	_flagged = now


## Where the circle around the piloted body is centred: its chest.
func _body_see_centre() -> Vector2:
	if not _bodies.has(_piloted):
		return Vector2(-1e6, -1e6)
	return Iso.point_to_screen(_bodies[_piloted]["at"]) - Vector2(0, Iso.PX_PER_M * 0.5)


## Which see-through circles a tree drawn now takes part in (Trees.draw):
## the body's when it stands in front of the piloted body and covers its
## circle; the mouse's when it covers the mouse and something (a body, an
## item) lies behind it near the mouse.
## `behind`: the smallest depth (x + y, millimetres) of a body or an item
## near the mouse (_nearest_depth_near), INF when there is none.
func _see_through_flags(tree: Dictionary, height_cm: int, mouse: Vector2, behind: float) -> int:
	var flags := 0
	var at: Vector2 = tree["at"]
	# The tree's rectangle never changes: worked out once.
	if not tree.has("rect"):
		var images := Trees.textures(str(tree["species"]))
		if images.is_empty():
			return 0
		tree["rect"] = Trees.rect(at, height_cm, images[Trees.variant(at, images.size())])
	var r: Rect2 = tree["rect"]
	if _bodies.has(_piloted):
		var body: Vector2 = _bodies[_piloted]["at"]
		var centre := _body_see_centre()
		if at.x + at.y > body.x + body.y and r.grow(BODY_SEE_M * Iso.PX_PER_M).has_point(centre):
			flags |= 1
			_body_hidden = true
	if r.has_point(mouse) and behind < at.x + at.y:
		flags |= 2
		_mouse_hidden = true
	return flags


## The smallest depth (x + y, in millimetres: the farthest from the
## camera) of a body within half again the mouse's circle around `mouse`,
## or an item within it; INF when there is none. A tree covering the
## mouse hides something when this lies behind its foot. Items are looked
## for only in the chunks around the mouse.
func _nearest_depth_near(mouse: Vector2) -> float:
	var reach := MOUSE_SEE_M * Iso.PX_PER_M
	var nearest := INF
	for id: int in _bodies:
		var b: Vector2 = _bodies[id]["at"]
		if Iso.point_to_screen(b).distance_to(mouse) < reach * 1.5:
			nearest = minf(nearest, b.x + b.y)
	var c := Iso.chunk_of(Iso.screen_to_tile(mouse), _chunk_size)
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			for item: Dictionary in _items.get(c + Vector2i(dx, dy), []):
				var p := Vector2(int(item.get("x", 0)) * 1000 + 500, int(item.get("y", 0)) * 1000 + 500)
				if Iso.point_to_screen(p).distance_to(mouse) < reach:
					nearest = minf(nearest, p.x + p.y)
	return nearest


## Eases the see-through circles open or shut and hands them to the trees'
## shader; redraws when the mouse moves, since which tree it hides changes.
func _update_see_through(delta: float) -> void:
	_sync_bodies()
	var mouse := get_local_mouse_position()
	# Which trees to see through changes only when the mouse or a body
	# moves; looked at again then, or a few times a second at most else
	# (an item laid or taken near the mouse).
	var looked := [mouse, _bodies.keys(), _bodies.values().map(func(b: Dictionary) -> Vector2: return b["at"])]
	_flags_age += delta
	if looked != _flags_looked or _flags_age >= FLAGS_REFRESH:
		_flags_looked = looked
		_flags_age = 0.0
		_update_flags(mouse)
	var step := delta / SEE_EASE
	_body_reveal = move_toward(_body_reveal, 1.0 if _body_hidden else 0.0, step)
	_mouse_reveal = move_toward(_mouse_reveal, 1.0 if _mouse_hidden else 0.0, step)
	var sway := _standing.material as ShaderMaterial
	if sway == null:
		return
	sway.set_shader_parameter("body_at", _body_see_centre())
	sway.set_shader_parameter("body_radius", BODY_SEE_M * Iso.PX_PER_M)
	sway.set_shader_parameter("body_reveal", _body_reveal)
	sway.set_shader_parameter("mouse_at", mouse)
	sway.set_shader_parameter("mouse_radius", MOUSE_SEE_M * Iso.PX_PER_M)
	sway.set_shader_parameter("mouse_reveal", _mouse_reveal)
	if mouse.distance_to(_last_mouse) > 4.0:
		_last_mouse = mouse


var _last_mouse := Vector2(-1e6, -1e6)


## A small "z" above a sleeping body, drawn on `canvas` at `at`.
static func draw_sleep_icon(canvas: CanvasItem, at: Vector2) -> void:
	var font := ThemeDB.fallback_font
	canvas.draw_string_outline(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, 3, Color.BLACK)
	canvas.draw_string(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.75, 0.85, 1.0))


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
