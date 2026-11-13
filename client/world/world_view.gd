class_name WorldView
extends Node2D
## Shows the ground the server sent (one ChunkView per chunk) and the
## bodies in it, drives the camera, and turns clicks into `move_to`
## intentions. Everything shown comes from the server; nothing is
## decided here (D03, D05).
##
## Bodies walk along the routes the server gives them (D32): the client
## only interpolates between waypoints at the given speed (BodyWalk), told
## again as a body walks into another chunk, and snaps to wherever the
## server says they stopped. Bodies are told within about 150 m of the
## piloted one (protocol v19, D5).
##
## Camera: mouse wheel zooms, dragging with the right or middle button
## pans, Space goes back to following the piloted body.

const BODY_COLOR := Color(0.90, 0.80, 0.60)
const PILOTED_COLOR := Color(1.0, 0.55, 0.20)
const TARGET_COLOR := Color(1.0, 0.85, 0.30, 0.9)
## Farthest zoom: the land is discovered by going there, not seen whole
## (the project lead's call, 9 November 2026: a sense of space).
const ZOOM_MIN := Zoom.MIN
const ZOOM_MAX := Zoom.MAX
const DEFAULT_ZOOM := Zoom.DEFAULT
## Tiles the view reaches from the body, either way, to where the fog is
## whole. It follows the body tile by tile (the chunks held move by whole
## chunks, the view does not); 288, as far as the chunks held always reach
## (the body's chunk and nine more each way: 19 × 19, net.rs VIEW_RADIUS),
## so nothing past it is needed.
const VIEW_REACH := 288.0
## The fog at the edge of the view, in three parts named by the project
## lead (9 November 2026, tried live with soul-try), from the outside in:
## - fog A: the whole fog past the view's limit, nothing drawn under it;
##   the camera stops at its end, so no corner of the screen shows past it.
## - fog B: the view's last tiles, under a veil over everything (trees,
##   plants, items too), whole at the limit.
## - fog C: the tiles before B, under a veil on the ground only, what
##   stands showing over it.
## The veil thickens from nothing at C's start to whole at the limit.
const FOG_A_TILES := 5.0
const FOG_B_TILES := 35.0
const FOG_C_TILES := 15.0
## How thick the fog is at its thickest, past the limit (1: nothing seen
## through it; a little less, the project lead's call).
const FOG_OPACITY := 0.7
## The radius of the view's rounded corners, in tiles: the fog turns them
## in an arc rather than a right angle (the project lead's call).
const FOG_CORNER := 64.0
## Millimetres per tile, as the protocol defines fine points.
const MM_PER_TILE := 1000.0

@onready var _camera: Camera2D = $Camera

var _width := 0
var _height := 0
var _chunk_size := 32
## chunk position -> ChunkView, for the chunks built: those the camera's
## reach touches (C1, 11 November 2026: the 361 chunks the server sends
## are held as data, `_tiles` and the dictionaries beside it; a chunk is
## built, its ground, trees, plants, items and shadows made, only when
## the camera's reach touches it, and released after a grace once the
## reach left it, its data kept: ViewLedger).
var _chunks: Dictionary = {}
## The limit of the view, in tiles (_update_limit).
var _limit := Rect2()
## material id -> Color, from the server's catalogue
var _looks: Dictionary = {}
## material id -> name shown to players
var _names: Dictionary = {}
## body id -> a walk (BodyWalk: "at", the place shown, in mm, "route",
## "arrivals"...) and "asleep"
var _bodies: Dictionary = {}
## The world's tick between server messages: bodies walk by it (BodyWalk).
var _clock := GameClock.new()
## chunk position -> Array of items as received, redrawn on change
var _items: Dictionary = {}
## chunk position -> its tiles, as codes (TerrainGrid)
var _tiles: Dictionary = {}
## chunk position -> hash of its tiles (ChunkView.tiles_hash)
var _tile_hashes: Dictionary = {}
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
## The camera starts or stops following the piloted body (the recentre
## button shows while it does not).
signal following_changed(following: bool)
## The piloted body thinks `line`, about the place `at` (mm): its way
## barred (ThoughtBubble).
signal thinks(line: String, at: Vector2)
## body id -> {"id", "material"} of what it holds, or {}
var _holds: Dictionary = {}
## chunk -> the trees standing in it (Trees.Stand).
var _trees: Dictionary = {}
## species id -> height in centimetres.
var _tree_heights: Dictionary = {}
## The content ids of the kinds of tree, in `world_state`'s order: a
## chunk's trees name their kind by its index.
var _species_ids := PackedStringArray()
## chunk -> where its water runs, tile by tile (protocol `currents`;
## empty for a chunk without water).
var _currents: Dictionary = {}
## Seeing through trees: how far the circles around the body and the
## mouse are open (eased), and whether each should be.
var _body_reveal := 0.0
var _mouse_reveal := 0.0
var _body_hidden := false
var _mouse_hidden := false
const BODY_SEE_M := 1.5
## A body's height, at the world's true scale (under 3 m everything is).
const BODY_HEIGHT_M := 1.7
const MOUSE_SEE_M := 1.2
## Seconds for a circle to open or close.
const SEE_EASE := 0.2
## Chunks whose borders must be drawn again because a neighbour came or
## went: each once, a few a frame within a time budget (RedrawQueue), so
## a chunk's arrival does not freeze the game.
var _to_redraw := RedrawQueue.new()
## The layer the trees and bodies stand on, sorted by their feet.
var _standing: Node2D
## The node the chunks built hang from, and each chunk's rank in the
## order the server sent them (chunk -> rank), the order they hang in.
var _ground: Node2D
## The "blocked" notice of the piloted body.
var _blocked: BlockedView
## The steps test, made when a chunk first carries heights.
var _steps: StepsTest
var _arrival: Dictionary = {}
var _arrivals := 0
## chunk -> the node holding its trees and tall plants.
var _tree_nodes: Dictionary = {}
## chunk -> its tall plants (PlantStand), for the chunks built.
var _plant_stands: Dictionary = {}
## The land past the chunks held (FarView). It is drawn with a hole under
## the view's limit once every chunk under it is built: looked at again
## when a chunk comes, goes or builds, or the limit moves.
var _far: FarView
var _far_dirty := true
var _far_complete := false
## The kinds of shrub, plant and mushroom (Net.flora), and the plants of
## each chunk held, as received (Protocol.decode_chunk: three ints a plant,
## its kind, then where it stands, x and y, in millimetres).
var _flora: Array = []
var _plants: Dictionary = {}
## The trees under a point are found without looking at every tree
## (see-through), by the cells of the screen each stand knows its trees by.
const TREE_CELL_PX := Trees.Stand.CELL_PX
## body id -> the node drawing it.
var _body_nodes: Dictionary = {}
## The trees whose see-through flags are set, to clear them later:
## Vector3i(chunk x, chunk y, tree index) -> true.
var _flagged: Dictionary = {}
## What the see-through flags were last worked out for: the mouse, where
## the piloted body stands, what lies behind near the mouse, which trees
## are held; worked out again only when one of them changed, and a few
## times a second regardless (before, every frame any body walked:
## lightness audit, 10 November 2026). The flags depend on nothing else.
var _flags_mouse := Vector2.INF
var _flags_body := Vector2.INF
var _flags_behind := -1.0
var _flags_trees := -1
var _flags_age := 0.0
const FLAGS_REFRESH := 0.25
## Counted up whenever the trees held or the items held change.
var _trees_version := 0
var _items_version := 0
## The value each uniform of the standing layer's material was last given:
## sent again only when it changes.
var _sway_sent: Dictionary = {}
## The bookkeeping of the chunks held (ViewLedger): the bounds of the
## chunks held, kept as chunks come and go; which chunks are built and
## which the camera's reach asks for or has left (C1, C5); the chunks
## whose redraw waits for the camera to come near; what lies behind the
## mouse among the items.
var _ledger := ViewLedger.new()
## chunk -> the hash of its trees and of its plants as the server sent
## them (a chunk sent again with the same keeps its nodes).
var _trees_hash: Dictionary = {}
var _plants_hash: Dictionary = {}
## The camera's position and zoom at the last drawing.
var _last_view := Vector3.INF
var _drag_start := Vector2.ZERO
var _dragged := false
var _night := false
var _piloted := -1
## Where the piloted body was asked to go, highlighted until it gets there.
var _target := Vector2i(-1, -1)
## The camera follows the piloted body until the player drags it.
var _following := true:
	set(value):
		if value != _following:
			_following = value
			following_changed.emit(value)
## The camera's glide back to the body (recentre) or to a place (a
## thought clicked, toward `_glide_to`, in world pixels).
var _glide := CameraGlide.new()
var _glide_to := Vector2.ZERO
## Has the camera been put somewhere sensible since the last world state?
var _camera_placed := false
var _dragging := false
## The drag the mouse asked for since the last frame, in world pixels:
## mouse events come at their own pace (none, one or several a frame);
## the camera makes them all at once, once a frame, so the map stays
## under the hand and every layer moves together.
var _pan_pending := Vector2.ZERO
var _hover := Vector2i(-1, -1)


func _ready() -> void:
	add_to_group("world_views")
	_camera.zoom = Vector2.ONE * DEFAULT_ZOOM
	Net.world_state.connect(_on_world_state)
	Net.chunk.connect(_on_chunk)
	Net.chunk_bodies.connect(_on_chunk_bodies)
	_far = FarView.new()
	add_child(_far)
	Net.far_view.connect(_far.show_far)
	Net.flora.connect(func(flora: Array) -> void: _flora = flora)
	Net.species.connect(func(species: Array) -> void:
		_tree_heights.clear()
		_species_ids.clear()
		for s in species:
			_species_ids.append(str(s.get("id", "")))
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
	# The chunks built hang from one node, below the bodies and the hover
	# outline, kept in the order the server sent them whatever order the
	# camera builds them in: where two chunks' drawings overlap at a
	# border, the later one shows over the earlier, as when every chunk
	# was built on arrival.
	_ground = Node2D.new()
	_ground.z_index = -1
	add_child(_ground)
	# The "blocked" notice, over the bodies and the trees.
	_blocked = BlockedView.new()
	_blocked.z_index = 1
	add_child(_blocked)
	_set_sway("body_radius", BODY_SEE_M * Iso.PX_PER_M)
	_set_sway("mouse_radius", MOUSE_SEE_M * Iso.PX_PER_M)
	# Which trees have a canvas item is settled just before each frame is
	# drawn, once the camera has moved for good (_cull_trees).
	RenderingServer.frame_pre_draw.connect(_cull_trees)
	Net.chunk_hidden.connect(_on_chunk_hidden)
	Net.body_walks.connect(_on_body_walks)
	Net.clock.connect(func(clock: Dictionary) -> void: _clock.set_clock(clock, Time.get_ticks_msec()))
	Net.connected.connect(func(tick: int, _body: int) -> void: _clock.sync(float(tick), Time.get_ticks_msec()))
	Net.day_started.connect(func(_day: int, tick: int) -> void: _clock.sync(float(tick), Time.get_ticks_msec()))
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
	Net.move_refused.connect(_on_move_refused)
	Net.body_blocked.connect(_on_body_blocked)
	Net.body_thinks.connect(_on_body_thinks)
	Net.connected.connect(func(_tick: int, body: int) -> void: _piloted = body)


## The trees' canvas items belong to no node: given back and freed here.
func _exit_tree() -> void:
	for stand: Trees.Stand in _trees.values():
		stand.release()
	for plants: PlantStand in _plant_stands.values():
		plants.release()
	_plant_stands.clear()
	Trees.free_pool()
	Trees.unload_all()


func _on_world_state(width: int, height: int, chunk_size: int, materials: Array, night: bool) -> void:
	_width = width
	_height = height
	WaterMeshes.map_size = Vector2i(width, height)
	_chunk_size = chunk_size
	_ledger.chunk_size = chunk_size
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
	if Net.same_world:
		_forget_bodies()
		_set_night(night)
		queue_redraw()
		return
	for chunk: ChunkView in _chunks.values():
		chunk.queue_free()
	_chunks.clear()
	_ledger = ViewLedger.new()
	_ledger.chunk_size = chunk_size
	_items.clear()
	_tiles.clear()
	TerrainGrid.clear_all()
	_tile_hashes.clear()
	_currents.clear()
	for stand: Trees.Stand in _trees.values():
		stand.release()
	_trees.clear()
	_trees_hash.clear()
	_plants.clear()
	_plants_hash.clear()
	_trees_version += 1
	_items_version += 1
	_arrival.clear()
	_paced.clear()
	_reach_dirty = true
	_standing_changed = true
	_trees_changed = true
	for n: Node in _tree_nodes.values():
		n.queue_free()
	_tree_nodes.clear()
	for plants: PlantStand in _plant_stands.values():
		plants.release()
	_plant_stands.clear()
	_forget_bodies()
	_following = true
	_camera_placed = false
	_far_dirty = true
	_set_night(night)
	queue_redraw()


## Every body forgotten: a reconnection to the same world keeps the
## chunks held (D4), and the bodies in them come again with them.
func _forget_bodies() -> void:
	for n: Node in _body_nodes.values():
		n.queue_free()
	_body_nodes.clear()
	_flagged.clear()
	_bodies.clear()
	_holds.clear()


func _set_night(night: bool) -> void:
	_night = night
	# Provisional graphics: the ground dims at night. SOUL_INVASION_DAYLIGHT
	# keeps it lit, to judge the ground on a development screenshot.
	var lit := night and OS.get_environment("SOUL_INVASION_DAYLIGHT").is_empty()
	modulate = Color(0.45, 0.5, 0.7) if lit else Color.WHITE


## A chunk as Protocol.decode_chunk gives it: its tiles' codes, currents,
## trees, plants (packed arrays) and items.
func _on_chunk(x: int, y: int, chunk: Dictionary) -> void:
	var key := Vector2i(x, y)
	var codes: PackedByteArray = chunk["tiles"]
	var currents: PackedInt32Array = chunk["currents"]
	var trees: PackedInt32Array = chunk["trees"]
	var plants: PackedInt32Array = chunk["plants"]
	var items: Array = chunk["items"]
	var trees_hash := hash(trees)
	var plants_hash := hash(plants)
	# The chunk is held as data, its tiles laid in the terrain grid where
	# the chunks around read their borders; a chunk sent again keeps its
	# stand when its trees are the same (the tiles never change, D31).
	var same_stand: bool = _trees.has(key) and int(_trees_hash.get(key, 0)) == trees_hash
	var same_nodes: bool = same_stand and int(_plants_hash.get(key, 0)) == plants_hash
	if not _arrival.has(key):
		_arrivals += 1
		_arrival[key] = _arrivals
	_ledger.hold(key)
	_tiles[key] = codes
	_tile_hashes[key] = hash(codes)
	TerrainGrid.write(key * _chunk_size, _chunk_size, codes)
	var heights: PackedInt32Array = chunk.get("heights", PackedInt32Array())
	if not heights.is_empty():
		HeightIndex.write(key * _chunk_size, _chunk_size, heights)
		if _steps == null:
			_steps = StepsTest.new()
			add_child(_steps)
		_steps.chunk(key * _chunk_size, _chunk_size)
	_currents[key] = currents
	_items[key] = items
	_items_version += 1
	_trees_hash[key] = trees_hash
	_plants_hash[key] = plants_hash
	_plants[key] = plants
	if not same_stand:
		var old: Trees.Stand = _trees.get(key)
		if old != null:
			old.release()
		_trees[key] = Trees.Stand.new(trees, _tree_heights, _species_ids)
		_trees_version += 1
	var view: ChunkView = _chunks.get(key)
	if view != null:
		# Built already: its items' mesh is made again only if they changed
		# (ChunkView.show_chunk), its trees and plants only if they did.
		if not same_nodes:
			_plant_chunk(key, view)
			view.show_plants(plants, _flora)
			_standing_changed = true
			_trees_changed = true
		view.show_chunk(key * _chunk_size, _chunk_size, codes, items, _looks)
	elif _camera_placed and _ledger.within_reach(key):
		_build_chunk(key, _ledger.touches(_ledger.shown, key))
	else:
		_reach_dirty = true
	_redraw_neighbours(key)
	_far_dirty = true
	if not _camera_placed and _piloted < 0:
		# Watching, no body to follow: look at the ground we were given
		# rather than at the valley's corner.
		var middle := key * _chunk_size + Vector2i(_chunk_size / 2, _chunk_size / 2)
		_camera.position = Iso.tile_to_screen(middle.x, middle.y)
		_camera_placed = true
	queue_redraw()


## The bodies standing in a chunk, told right after its frame.
func _on_chunk_bodies(_x: int, _y: int, bodies: Array) -> void:
	for b in bodies:
		var route: Array = []
		for p in b.get("route", []):
			route.append(Net.point(p))
		# Where it stood when its chunk left the server, a moment ago:
		# walked on from there, to its next waypoint on time.
		var now := _now()
		var id := int(b.get("id", -1))
		_place(id, BodyWalk.told(_bodies.get(id, {}), Net.point(b.get("at", {})), route,
			Net.arrivals(b.get("arrivals", [])), now, int(b.get("speed_cm_per_s", 0)) * 10.0, now))
		_bodies[int(b.get("id", -1))]["asleep"] = bool(b.get("asleep", false))
		var holds: Variant = b.get("holds")
		_holds[int(b.get("id", -1))] = holds if holds is Dictionary else {}
	queue_redraw()


func _on_item_added(item: Dictionary) -> void:
	var key := Iso.chunk_of(Vector2i(int(item.get("x", 0)), int(item.get("y", 0))), _chunk_size)
	if _items.has(key):
		_items[key].append(item)
		_items_version += 1
		_show_items(key)


func _on_item_removed(id: int, x: int, y: int) -> void:
	var key := Iso.chunk_of(Vector2i(x, y), _chunk_size)
	if _items.has(key):
		var kept: Array = []
		for item in _items[key]:
			if int(item.get("id", -1)) != id:
				kept.append(item)
		_items[key] = kept
		_items_version += 1
		_show_items(key)


## The items of chunk `key` changed: its view, if built, draws them again.
func _show_items(key: Vector2i) -> void:
	var view: ChunkView = _chunks.get(key)
	if view != null:
		view.show_chunk(key * _chunk_size, _chunk_size, _tiles[key], _items[key], _looks)


func _on_body_assigned(body: int) -> void:
	_piloted = body
	_blocked.clear()
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
	_blocked.clear()
	_following = true
	teleporting.emit()
	queue_redraw()


## Development (--dev): the wind on or off in the trees, to measure what
## it costs (trees.gdshader's sway and gust; their defaults when on).
func set_wind(on: bool) -> void:
	var sway := _standing.material as ShaderMaterial
	if sway == null:
		return
	sway.set_shader_parameter("sway", Trees.SWAY if on else 0.0)
	sway.set_shader_parameter("gust", Trees.GUST if on else 0.0)


## Development (--dev, F6): the trees shown or hidden, to measure what
## they cost the graphics card.
func set_trees_shown(on: bool) -> void:
	_trees_shown = on
	_standing_changed = true
	_trees_changed = true
	_cull_standing()


## Pixels a chunk's standing things may reach past its ground on screen:
## up for the tallest trees, sideways and down for their crowns.
const STANDING_REACH_UP := ViewLedger.STANDING_REACH_UP
const STANDING_REACH := ViewLedger.STANDING_REACH


## Shows only the trees and tall plants of chunks on screen: the standing
## layer sorts every child it holds every frame, seen or not (from the
## performance audit, 9 November 2026). What is on screen is unchanged.
func _cull_standing() -> void:
	var seen := _seen()
	if seen == _standing_seen and not _standing_changed:
		return
	_standing_seen = seen
	_standing_changed = false
	for key: Vector2i in _tree_nodes:
		var holder: Variant = _tree_nodes[key]
		if not is_instance_valid(holder):
			continue
		# The chunk's diamond on screen, grown by what may stand out of it.
		var reach := _ledger.chunk_rect(key).grow_individual(STANDING_REACH, STANDING_REACH_UP, STANDING_REACH, STANDING_REACH)
		var shown := _trees_shown and seen.intersects(reach)
		if (holder as Node2D).visible != shown:
			(holder as Node2D).visible = shown
			_trees_changed = true


## What the screen shows, in the world's pixels.
func _seen() -> Rect2:
	var half := get_viewport_rect().size / _camera.zoom.x / 2.0
	return Rect2(_camera.position - half, half * 2.0)


## The camera's reach follows the screen, its margin wider as the zoom
## pulls back (ViewLedger.margin_for): the chunks it touches are built,
## those the screen may show this frame, the margin's nearest the camera
## first within BUILD_BUDGET_USEC a frame (their grounds then worked out
## on the workers and laid nearest first, ChunkView.lay_ready), the ones
## it left for the grace released (ViewLedger.plan); the chunks whose
## redraw waited for it are queued when it reaches them. Looked at when
## the screen moved, when a chunk came or went, while a chunk's grace
## runs and while the margin waits to be built.
func _update_reach() -> void:
	var moved := _ledger.see(_seen(), ViewLedger.margin_for(_camera.zoom.x))
	if not moved and not _reach_dirty and _ledger.leaving.is_empty() and _paced.is_empty():
		return
	if not _camera_placed:
		return
	_reach_dirty = false
	var t := DevTiming.start()
	var plan := _ledger.plan(Time.get_ticks_msec(), _tiles)
	DevTiming.add("reach plan", t)
	for key: Vector2i in plan["at_once"]:
		_build_chunk(key, true)
	var paced: Array[Vector2i] = plan["paced"]
	var started := Time.get_ticks_usec()
	for key: Vector2i in ViewLedger.nearest_first(paced, _ledger, _ledger.seen.get_center()):
		if Time.get_ticks_usec() - started >= BUILD_BUDGET_USEC:
			# The rest next frame.
			_reach_dirty = true
			break
		_build_chunk(key, false)
	for key: Vector2i in plan["release"]:
		_release_chunk(key)
	if moved:
		for view: Variant in _ledger.wake():
			_to_redraw.add(view)
	_hurry_paced()


## Whether the chunks held changed since the reach was last looked at.
var _reach_dirty := true
## The main thread's time a frame for building the margin's chunks
## (their nodes: trees, plants, items; their grounds go to the workers)
## and for laying the grounds the workers finished (C3).
const BUILD_BUDGET_USEC := 1000
const LAY_BUDGET_USEC := 1500
## The chunks built for the margin whose ground still waits its turn
## (chunk -> true): one the screen reaches before its turn is built this
## frame after all, so no chunk shows before its ground.
var _paced: Dictionary = {}


func _hurry_paced() -> void:
	for key: Vector2i in _paced.keys():
		var view: ChunkView = _chunks.get(key)
		if view == null or view.is_built():
			_paced.erase(key)
		elif _ledger.touches(_ledger.shown, key):
			view.build_at_once = true
			view.queue_redraw()
			_paced.erase(key)


## Builds chunk `key` from the data held: its view (the ground, the items
## and, within it, the shadows, the plants and the banks) and its trees
## and tall plants on the standing layer. Its ground is worked out on a
## worker from now; `at_once`: the screen may show it, so its ground is
## laid this frame, the worker waited for if need be; otherwise it is
## laid when done, nearest the camera first (ChunkView.lay_ready).
func _build_chunk(key: Vector2i, at_once: bool) -> void:
	var t := DevTiming.start()
	var view := ChunkView.new()
	# The ground and the bank pieces keep plain linear filtering: the
	# mipmaps this view uses for the trees would blur them.
	view.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	view.tiles_hash = func(k: Vector2i) -> int: return int(_tile_hashes.get(k, 0))
	view.current_at = current_at
	view.currents_of = func(k: Vector2i) -> PackedInt32Array: return _currents.get(k, PackedInt32Array())
	view.build_at_once = at_once
	# Among the chunks built, in the order the server sent them.
	var rank := int(_arrival.get(key, 0))
	var before := 0
	for other: Vector2i in _chunks:
		if int(_arrival.get(other, 0)) < rank:
			before += 1
	_ground.add_child(view)
	_ground.move_child(view, before)
	_chunks[key] = view
	if not at_once:
		_paced[key] = true
	_ledger.mark_built(key)
	_plant_chunk(key, view)
	view.show_plants(_plants[key], _flora, true)
	view.show_chunk(key * _chunk_size, _chunk_size, _tiles[key], _items[key], _looks)
	# Its ground and its plants worked out on a worker from now
	# (ChunkView.start_build).
	view.start_build()
	_standing_changed = true
	_trees_changed = true
	_far_dirty = true
	DevTiming.add("chunks made", t)


## Whether every chunk the screen may show is held and built: the
## loading screen lifts on it when no chunk comes (a teleport onto the
## tile the body stands on).
func reach_ready() -> bool:
	if not _camera_placed or not _ledger.shown.has_area():
		return false
	for key: Vector2i in _ledger.chunks_touching(_ledger.shown, _bounds()):
		var view: ChunkView = _chunks.get(key)
		if _tiles.has(key) and (view == null or not view.is_built()):
			return false
	return true


## Frees chunk `key`'s nodes, its data kept: it builds again from them
## when the camera's reach comes back.
func _release_chunk(key: Vector2i) -> void:
	var view: ChunkView = _chunks.get(key)
	if view != null:
		_to_redraw.drop(view)
		_ledger.forget(view)
		view.queue_free()
		_chunks.erase(key)
		_paced.erase(key)
		DevTiming.tally("chunks released")
	var plants: PlantStand = _plant_stands.get(key)
	if plants != null:
		plants.release()
		_plant_stands.erase(key)
	var holder: Variant = _tree_nodes.get(key)
	if holder != null and is_instance_valid(holder):
		(holder as Node).queue_free()
	_tree_nodes.erase(key)
	var stand: Trees.Stand = _trees.get(key)
	if stand != null:
		stand.show_cells(Rect2i())
		stand.parent = RID()
		stand.flags.fill(0)
	_ledger.mark_released(key)
	_standing_changed = true
	_trees_changed = true
	_far_dirty = true


var _trees_shown := true
## The view the standing layer was last culled for, and whether chunks came
## or went since (then it is culled again even if the view stayed).
var _standing_seen := Rect2()
var _standing_changed := true
## The cells of the screen the trees were last shown for, and whether the
## trees or the chunks shown changed since.
var _trees_cells := Rect2i()
var _trees_changed := true


## Gives a canvas item to the trees whose image may show on screen, and
## only to them (Trees.Stand.show_cells): the renderer sorts by depth
## every tree it is given, seen or not. Run just before a frame is drawn,
## when the camera has stopped moving for that frame; only the cells of
## the screen that came or went are looked at. What is on screen is
## unchanged.
func _cull_trees() -> void:
	if _trees.is_empty():
		return
	var t := DevTiming.start()
	var seen := get_viewport().get_canvas_transform().affine_inverse() * get_viewport().get_visible_rect()
	var cells := Trees.Stand.cell_range(seen)
	if cells == _trees_cells and not _trees_changed:
		DevTiming.add("tree culling", t)
		return
	_trees_cells = cells
	_trees_changed = false
	# Only the stands of built chunks have a holder to hang from; a chunk
	# released gave its cells back (_release_chunk).
	for key: Vector2i in _tree_nodes:
		var holder: Variant = _tree_nodes[key]
		var stand: Trees.Stand = _trees.get(key)
		if stand == null:
			continue
		var on := holder != null and is_instance_valid(holder) and (holder as Node2D).visible
		stand.show_cells(cells if on else Rect2i())
		var plants: PlantStand = _plant_stands.get(key)
		if plants != null:
			plants.show_cells(cells if on else Rect2i())
	DevTiming.add("tree culling", t)


## What the world holds now, for the development counters: the chunks
## held and built, the trees held and those given a canvas item (drawn,
## or just off the screen's edge).
func counts() -> Dictionary:
	var trees := 0
	var drawn := 0
	for stand: Trees.Stand in _trees.values():
		trees += stand.size()
		drawn += stand.items.size()
	return {"chunks": _tiles.size(), "built": _chunks.size(), "in_line": ChunkView.in_line, "leaving": _ledger.leaving.size(), "stale": _ledger.stale.size(), "trees": trees, "trees_drawn": drawn, "bodies": _bodies.size()}


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
	if TerrainGrid.can_drink(terrain_at(tile), tile, piloted_tile()):
		acts.append({"label": "drink", "send": func(_item: int) -> void: Net.send_drink(tile.x, tile.y), "item": -1})
	return acts


## Frames a tile and stops following the body (development screenshots).
func look_at_tile(tile: Vector2i) -> void:
	_following = false
	_glide.stop()
	_camera_placed = true
	_camera.position = Iso.tile_to_screen(tile.x, tile.y)
	_clamp_camera()


## Where the water runs at a tile the server has sent: a unit vector in
## tiles times its speed in metres a second; zero on land or unknown.
func current_at(tile: Vector2i) -> Vector2:
	var key := Iso.chunk_of(tile, _chunk_size)
	var currents: PackedInt32Array = _currents.get(key, PackedInt32Array())
	if currents.is_empty():
		return Vector2.ZERO
	var local := tile - key * _chunk_size
	return WaterMeshes.current_vector(int(currents[local.y * _chunk_size + local.x]))


## The terrain of any tile the server has sent, "" otherwise.
func terrain_at(tile: Vector2i) -> String:
	var key := Iso.chunk_of(tile, _chunk_size)
	var tiles: PackedByteArray = _tiles.get(key, PackedByteArray())
	if tiles.is_empty():
		return ""
	var local := tile - key * _chunk_size
	return TerrainGrid.name_of(tiles[local.y * _chunk_size + local.x])


## Puts the limit of the view VIEW_REACH tiles around the piloted body
## (around the middle of the chunks held when watching), tile by tile as
## it walks, never past the chunks held. Its last FOG_B_TILES +
## FOG_C_TILES tiles lie under a veil, the fog's own colour, that thickens
## until it is whole (FarView): on the ground only over fog C, over
## everything over fog B; the land keeps its colours and melts into it.
## Past the limit, fog A, and the camera stops.
func _update_limit() -> void:
	var all := _bounds()
	if all.size == Vector2i.ZERO:
		return
	var held := Rect2(Vector2(all.position * _chunk_size), Vector2(all.size * _chunk_size))
	var centre := held.get_center()
	if _bodies.has(_piloted):
		centre = (_bodies[_piloted]["at"] as Vector2) / MM_PER_TILE
	var limit := Iso.view_limit(centre, VIEW_REACH, held, 0.0)
	if limit == _limit:
		return
	_limit = limit
	_far_dirty = true
	var veil := FOG_B_TILES + FOG_C_TILES
	var ground := limit.grow(-veil)
	var cover := limit.grow(-FOG_B_TILES)
	if _far != null:
		# The fogs' arcs share one centre: each inner one is as much tighter
		# as it lies inside the limit.
		_far.set_held(ground.position, ground.end, veil, FOG_OPACITY, FOG_OPACITY, maxf(FOG_CORNER - veil, 0.0))
		_far.set_cover(cover.position, cover.end, FOG_B_TILES, FOG_OPACITY, maxf(FOG_CORNER - FOG_B_TILES, 0.0))



## The bounds, in chunks, of the chunks held.
func _bounds() -> Rect2i:
	return _ledger.bounds(_tiles)


## Tells the far view whether it may leave its hole under the limit: when
## every chunk the screen shows under it is built, or builds this frame
## (the others are not seen, built or not). Looked at again while
## something changes or the screen moves, never otherwise
## (FarView.set_complete).
func _update_far_hole() -> void:
	if _far == null or (not _far_dirty and _far_complete):
		return
	_far_dirty = false
	var shown := _ledger.chunks_touching(_ledger.seen, _bounds())
	_far_complete = FarView.limit_built(_limit, _chunks, _tiles, _bounds(), _chunk_size, shown)
	# The limit in tile space (tile x spans [x - 0.5, x + 0.5] there), half
	# a tile inside it either way: only tiles of chunks under the limit.
	_far.set_complete(Rect2(_limit.position, _limit.size - Vector2.ONE), _far_complete)


## The chunks around `key` draw their borders from their neighbours'
## tiles: redraw them when `key` appears or goes, at once within the
## camera's reach, when the camera comes otherwise (_stale).
func _redraw_neighbours(key: Vector2i) -> void:
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			var at := key + Vector2i(dx, dy)
			var near: ChunkView = _chunks.get(at)
			# One not built yet will read its neighbours when it builds.
			if near != null and near.is_built() and (dx != 0 or dy != 0):
				if _ledger.within_reach(at):
					_to_redraw.add(near)
				else:
					_ledger.mark_stale(near, at)
					DevTiming.tally("redraws deferred")


## Chunk `key` left the view (the server's `view`; Net keeps its frame):
## its nodes, if built, and its data go.
func _on_chunk_hidden(x: int, y: int) -> void:
	var key := Vector2i(x, y)
	if _tiles.has(key):
		_release_chunk(key)
		var stand: Trees.Stand = _trees.get(key)
		if stand != null:
			stand.release()
		_ledger.drop(key)
		_items.erase(key)
		_items_version += 1
		_tiles.erase(key)
		TerrainGrid.clear(key * _chunk_size, _chunk_size)
		_tile_hashes.erase(key)
		_currents.erase(key)
		_trees.erase(key)
		_trees_hash.erase(key)
		_trees_version += 1
		_plants.erase(key)
		_plants_hash.erase(key)
		_arrival.erase(key)
		_reach_dirty = true
		_redraw_neighbours(key)
	for id in _bodies.keys():
		if Iso.chunk_of(_tile_of(_bodies[id]["at"]), _chunk_size) == key:
			_bodies.erase(id)
	queue_redraw()


func _on_body_walks(body: int, at: Vector2, route: Array, arrivals: Array, speed_cm_per_s: int, tick: int) -> void:
	# Told again at each chunk it walks into, by time: it agrees with what
	# is shown, and a body near is walked on from where it is shown. The
	# clock, set at the welcome and at dawn only, is caught up by the tick
	# the walk was told at: a welcome read late left it behind for good.
	_clock.catch_up(float(tick), Time.get_ticks_msec())
	if body == _piloted:
		_blocked.clear()
	_place(body, BodyWalk.told(_bodies.get(body, {}), at, route, arrivals, float(tick), speed_cm_per_s * 10.0, _now()))
	queue_redraw()


## The world's tick now, as the clock reckons it.
func _now() -> float:
	return _clock.tick_at(Time.get_ticks_msec())


func _on_body_stopped(body: int, at: Vector2) -> void:
	_place(body, BodyWalk.standing(at))
	if body == _piloted and _tile_of(at) == _target:
		_clear_target()
	queue_redraw()


## The piloted body stopped for good short of its target: it thinks why,
## and a cross marks where it stands.
func _on_body_blocked(body: int, at: Vector2, _x: int, _y: int, reason: String) -> void:
	if body != _piloted:
		return
	_clear_target()
	_blocked.mark(at)
	thinks.emit(UiWords.blocked(reason), at)


## The piloted body thinks aloud as it decides how to go on (protocol
## v22): it skirts to a side, it sees a way, it does not know where to go.
func _on_body_thinks(body: int, kind: String, side: String, at: Vector2) -> void:
	if body != _piloted:
		return
	thinks.emit(UiWords.thought(kind, side), at)


## An order refused at once (the body at the bank, sent across the
## water): it thinks why, as when it stops on the way (protocol v21).
func _on_move_refused(body: int, _x: int, _y: int, reason: String) -> void:
	_clear_target()
	if body != _piloted or not _bodies.has(body):
		return
	var at: Vector2 = _bodies[body]["at"]
	_blocked.mark(at)
	thinks.emit(UiWords.blocked(reason), at)


## The camera glides back to the piloted body and follows it again (the
## recentre button, the space key).
func recentre() -> void:
	_following = true
	_glide.start(_camera.position)


## The camera glides to `at` (mm) and stays there (a thought clicked).
func glide_to(at: Vector2) -> void:
	_following = false
	_camera_placed = true
	_glide_to = Iso.point_to_screen(at)
	_glide.start(_camera.position)


func _on_body_left(body: int) -> void:
	_bodies.erase(body)
	_holds.erase(body)
	queue_redraw()


## Puts a body somewhere with the walk it is on (BodyWalk; no route:
## standing).
func _place(body: int, walk: Dictionary) -> void:
	var first_sight := not _bodies.has(body)
	walk["asleep"] = _bodies[body]["asleep"] if not first_sight else false
	_bodies[body] = walk
	var at: Vector2 = walk["at"]
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
			var material: Dictionary = _materials.get(str(item.get("material", "")), {})
			lines.append(Tooltip.describe_item(material, int(item.get("mass_g", -1))))
	lines.append_array(Tooltip.describe_plants(plants_on(tile), _flora))
	return "\n".join(lines)


## The kinds (indices into the flora) of the plants standing on `tile`.
func plants_on(tile: Vector2i) -> Array:
	var kinds: Array = []
	var plants: PackedInt32Array = _plants.get(Iso.chunk_of(tile, _chunk_size), PackedInt32Array())
	for i in range(0, plants.size(), 3):
		if _tile_of(Vector2(plants[i + 1], plants[i + 2])) == tile:
			kinds.append(plants[i])
	return kinds


## Zoom clamped to the allowed range.
static func clamp_zoom(zoom: float) -> float:
	return clampf(zoom, ZOOM_MIN, ZOOM_MAX)


func _process(delta: float) -> void:
	var timing := DevTiming.start()
	DevWatchdog.phase = DevWatchdog.WORLD_STEP
	_step(delta)
	DevWatchdog.phase = DevWatchdog.ELSEWHERE
	DevTiming.add("world view", timing)


## Keeps the screen within the view and its fog: no corner of the screen
## passes fog A, past the limit of the view, so pushing to
## the edge shows the veil and the fog, never what lies beyond (the
## project lead's call); walking that way moves the limit, tile by tile.
## Moves the camera by the drag asked for since the last frame.
func _pan(_delta: float) -> void:
	if _pan_pending == Vector2.ZERO:
		return
	_camera.position -= _pan_pending
	_pan_pending = Vector2.ZERO
	_clamp_camera()


## The picture is laid on whole pixels of the screen: the camera keeps its
## exact place, and its offset takes the view to the nearest screen pixel,
## so the ground, the water and the trees all round the same way and the
## view does not shimmer as it moves by fractions of a pixel.
func _snap_camera() -> void:
	var scale := _camera.zoom.x * get_viewport().get_screen_transform().get_scale().y
	_camera.offset = ViewLedger.snapped_offset(_camera.position, scale)


func _clamp_camera() -> void:
	if _limit.size.x <= 0.0 or _limit.size.y <= 0.0:
		return
	var a := Iso.camera_area(_limit.position, _limit.end,
		get_viewport_rect().size / _camera.zoom.x, FOG_A_TILES, 1.0)
	if a.size.x < 0.0 or a.size.y < 0.0:
		return
	var t := Vector2((_camera.position.x / (Iso.TILE_W / 2.0) + _camera.position.y / (Iso.TILE_H / 2.0)) / 2.0,
		(_camera.position.y / (Iso.TILE_H / 2.0) - _camera.position.x / (Iso.TILE_W / 2.0)) / 2.0)
	var b := a.end
	var c := Vector2(clampf(t.x, a.position.x, b.x), clampf(t.y, a.position.y, b.y))
	if c != t:
		_camera.position = Vector2((c.x - c.y) * Iso.TILE_W / 2.0, (c.x + c.y) * Iso.TILE_H / 2.0)


func _step(delta: float) -> void:
	var moving := false
	var now := _now()
	for id: int in _bodies:
		# Walk along the route by time (BodyWalk); the server's stop
		# message snaps the body wherever it really ended.
		if BodyWalk.advance(_bodies[id], delta, now):
			moving = true
	_pan(delta)
	if _glide.active:
		var to := _glide_to
		if _following and _bodies.has(_piloted):
			to = Iso.point_to_screen(_bodies[_piloted]["at"])
		_camera.position = _glide.step(delta, to)
	elif _following and _bodies.has(_piloted):
		_camera.position = Iso.point_to_screen(_bodies[_piloted]["at"])
	# The tree images read on a worker thread come in, the stands that
	# waited for them are built and the images no chunk held for the grace
	# are freed (Trees.poll): the built stands' trees show from this frame.
	Trees.view(_camera.zoom.x, get_viewport().get_screen_transform().get_scale().y)
	if Trees.poll() > 0:
		_trees_version += 1
		_trees_changed = true
	_update_limit()
	_clamp_camera()
	_snap_camera()
	_update_reach()
	_update_far_hole()
	_cull_standing()
	_update_see_through(delta)
	# The grounds the workers finished, nearest the camera first.
	ChunkView.lay_ready(LAY_BUDGET_USEC, _camera.position)
	for queued: Variant in _to_redraw.take(RedrawQueue.BUDGET_USEC, ChunkView.redraw_usec):
		(queued as ChunkView).queue_redraw()
	# Only the trees in view are drawn: redraw when the view moves. The
	# sway itself is the shader's, every frame, with no redraw.
	var view := Vector3(_camera.position.x, _camera.position.y, _camera.zoom.x)
	if moving or view != _last_view:
		if view.z != _last_view.z:
			# The water meshes read their images at a level a zoom step.
			ChunkView.set_water_zoom(view.z, get_viewport().get_screen_transform().get_scale().y)
		_last_view = view
		queue_redraw()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		if _dragging:
			_pan_pending += event.relative / _camera.zoom.x
			if event.position.distance_to(_drag_start) > 4.0:
				_dragged = true
				_following = false
				_glide.stop()
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
					_step_zoom(1)
			MOUSE_BUTTON_WHEEL_DOWN:
				if event.pressed:
					_step_zoom(-1)
	elif event is InputEventKey and event.pressed:
		match event.keycode:
			KEY_SPACE:
				recentre()
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


func _step_zoom(direction: int) -> void:
	_camera.zoom = Vector2.ONE * Zoom.next(_camera.zoom.x, direction)


## The camera's zoom now.
func zoom_level() -> float:
	return _camera.zoom.x


func _in_map(tile: Vector2i) -> bool:
	return tile.x >= 0 and tile.y >= 0 and tile.x < _width and tile.y < _height


func _draw() -> void:
	if _in_map(_target):
		draw_polyline(_closed(Iso.diamond(_target.x, _target.y)), TARGET_COLOR, 2.5)
	# Not while the map is dragged: the tile under the hand changes as
	# the view moves, and its outline would flicker from tile to tile.
	if _in_map(_hover) and not _dragged:
		draw_polyline(_closed(Iso.diamond(_hover.x, _hover.y)), Color(1, 1, 1, 0.6), 1.5)


## Draws body `id` on `canvas`, whose origin stands at its feet.
func draw_body(canvas: CanvasItem, id: int) -> void:
	if not _bodies.has(id):
		return
	var color := PILOTED_COLOR if id == _piloted else BODY_COLOR
	var feet := Vector2.ZERO
	# Provisional figures at a person's real height, 1.7 m from the feet
	# to the top of the head (64 px per metre): the head's top is 1.38 h.
	var h := Iso.PX_PER_M * BODY_HEIGHT_M / 1.38
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
	DevWatchdog.phase = DevWatchdog.TREES
	var old: Node = _tree_nodes.get(key)
	if old != null:
		old.queue_free()
	var old_plants: PlantStand = _plant_stands.get(key)
	if old_plants != null:
		old_plants.release()
	var holder := Node2D.new()
	holder.y_sort_enabled = true
	holder.use_parent_material = true
	# The trees hang from a y-sorted node of their own, first: sorted with
	# everything else on the standing layer, ties as before (trees, then
	# the tall plants, in the server's order).
	var trees := Node2D.new()
	trees.name = "Trees"
	trees.y_sort_enabled = true
	trees.use_parent_material = true
	holder.add_child(trees)
	var stand: Trees.Stand = _trees[key]
	stand.show_cells(Rect2i())
	stand.parent = trees.get_canvas_item()
	holder.visible = _trees_shown
	# The tall plants stand among the trees, sorted with them by their foot:
	# their canvas items hang from a y-sorted node after the trees', ties
	# as before (the trees, then the plants in the server's order). Given
	# as the view moves, as the trees' are (_cull_trees).
	var tall := Node2D.new()
	tall.name = "Plants"
	tall.y_sort_enabled = true
	tall.use_parent_material = true
	holder.add_child(tall)
	var plants := PlantStand.new(_plants.get(key, PackedInt32Array()), _flora)
	plants.parent = tall.get_canvas_item()
	_plant_stands[key] = plants
	# Among the chunks' holders, in the order the server sent the chunks:
	# two trees whose feet stand level are sorted as when every chunk was
	# planted on arrival.
	var rank := int(_arrival.get(key, 0))
	var before := 0
	for other: Vector2i in _tree_nodes:
		var h: Variant = _tree_nodes[other]
		if other != key and is_instance_valid(h) and (h as Node).get_parent() == _standing and int(_arrival.get(other, 0)) < rank:
			before += 1
	_standing.add_child(holder)
	_standing.move_child(holder, before)
	_tree_nodes[key] = holder
	var shadows: Trees.ShadowLayer = view.get_node_or_null("Shadows")
	if shadows == null:
		shadows = Trees.ShadowLayer.new()
		shadows.name = "Shadows"
		view.add_child(shadows)
	shadows.stand = stand
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
func _update_flags(mouse: Vector2, behind: float) -> void:
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
	# Only the trees whose image covers the mouse or the body's circle are
	# looked at, through the cells of the screen they cover.
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
	for key: Vector2i in keys:
		var stand: Trees.Stand = _trees.get(key)
		if stand == null:
			continue
		for cell in wanted:
			var list: Variant = stand.cells.get(cell)
			if list == null:
				continue
			for i: int in list:
				var id := Vector3i(key.x, key.y, i)
				if seen.has(id):
					continue
				seen[id] = true
				var f := _see_through_flags(stand.at[i], stand.rect(i), mouse, behind)
				if f != 0:
					now[id] = true
					stand.set_flags(i, f)
	for id: Vector3i in _flagged:
		if not now.has(id):
			var stand: Trees.Stand = _trees.get(Vector2i(id.x, id.y))
			if stand != null:
				stand.set_flags(id.z, 0)
	_flagged = now


## Where the circle around the piloted body is centred: its chest.
func _body_see_centre() -> Vector2:
	if not _bodies.has(_piloted):
		return Vector2(-1e6, -1e6)
	return Iso.point_to_screen(_bodies[_piloted]["at"]) - Vector2(0, Iso.PX_PER_M * BODY_HEIGHT_M * 0.6)


## Which see-through circles a tree standing at `at` (millimetres), its
## image covering `r`, takes part in (Trees.marker):
## the body's when it stands in front of the piloted body and covers its
## circle; the mouse's when it covers the mouse and something (a body, an
## item) lies behind it near the mouse.
## `behind`: the smallest depth (x + y, millimetres) of a body or an item
## near the mouse (_nearest_depth_near), INF when there is none.
func _see_through_flags(at: Vector2, r: Rect2, mouse: Vector2, behind: float) -> int:
	var flags := 0
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
## mouse hides something when this lies behind its foot. The items are
## looked for only in the chunks around the mouse, and only again when
## the mouse or the items held changed.
func _nearest_depth_near(mouse: Vector2) -> float:
	var reach := MOUSE_SEE_M * Iso.PX_PER_M
	var nearest := INF
	for id: int in _bodies:
		var b: Vector2 = _bodies[id]["at"]
		if Iso.point_to_screen(b).distance_to(mouse) < reach * 1.5:
			nearest = minf(nearest, b.x + b.y)
	return minf(nearest, _ledger.items_depth_near(_items, _items_version, mouse, reach))


## Eases the see-through circles open or shut and hands them to the trees'
## shader; redraws when the mouse moves, since which tree it hides changes.
func _update_see_through(delta: float) -> void:
	_sync_bodies()
	var mouse := get_local_mouse_position()
	# Which trees to see through depends on the mouse, on where the piloted
	# body stands, on what lies behind near the mouse and on the trees
	# held: looked at again when one of them changed, and a few times a
	# second regardless.
	var behind := _nearest_depth_near(mouse)
	var body_at: Vector2 = _bodies[_piloted]["at"] if _bodies.has(_piloted) else Vector2.INF
	_flags_age += delta
	if mouse != _flags_mouse or body_at != _flags_body or behind != _flags_behind or _trees_version != _flags_trees or _flags_age >= FLAGS_REFRESH:
		_flags_mouse = mouse
		_flags_body = body_at
		_flags_behind = behind
		_flags_trees = _trees_version
		_flags_age = 0.0
		_update_flags(mouse, behind)
	var step := delta / SEE_EASE
	_body_reveal = move_toward(_body_reveal, 1.0 if _body_hidden else 0.0, step)
	_mouse_reveal = move_toward(_mouse_reveal, 1.0 if _mouse_hidden else 0.0, step)
	_set_sway("body_at", _body_see_centre())
	_set_sway("body_reveal", _body_reveal)
	_set_sway("mip_bias", Trees.sharpness(_camera.zoom.x))
	_set_sway("mouse_at", mouse)
	_set_sway("mouse_reveal", _mouse_reveal)


## Gives the standing layer's material a uniform, when its value changed.
func _set_sway(uniform: String, value: Variant) -> void:
	if _sway_sent.has(uniform) and _sway_sent[uniform] == value:
		return
	_sway_sent[uniform] = value
	(_standing.material as ShaderMaterial).set_shader_parameter(uniform, value)


## A small "z" above a sleeping body, drawn on `canvas` at `at`.
static func draw_sleep_icon(canvas: CanvasItem, at: Vector2) -> void:
	var font := ThemeDB.fallback_font
	canvas.draw_string_outline(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, 3, Color.BLACK)
	canvas.draw_string(font, at + Vector2(-4, 4), "z", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(0.75, 0.85, 1.0))


static func _closed(points: PackedVector2Array) -> PackedVector2Array:
	var closed := PackedVector2Array(points)
	closed.append(points[0])
	return closed
