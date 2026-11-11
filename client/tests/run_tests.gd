extends SceneTree
## Minimal headless test runner for the client.
##
## Run with: godot --headless --path client -s tests/run_tests.gd
## Exits with a non-zero code if any assertion fails, so CI can gate on it.

var _failures := 0
var _checks := 0


func _init() -> void:
	_test_protocol_round_trip()
	_test_protocol_rejects_garbage()
	_test_protocol_wire_format_matches_server()
	_test_iso_round_trip()
	_test_iso_neighbours_are_distinct()
	_test_main_scene_lets_clicks_reach_the_world()
	_test_chunk_of_matches_the_server()
	_test_time_of_day()
	_test_tooltips()
	_test_ground_art_is_reachable()
	_test_inventory_lines_and_refusals()
	_test_brooks_are_lines_on_the_ground()
	_test_trees_stand_on_their_foot()
	_test_the_shore_field_measures_the_bank()
	_test_the_shore_field_blurs_as_three_boxes()
	_test_the_terrain_grid_holds_bytes()
	_test_the_ground_index_says_what_each_canvas_draws()
	_test_currents_decode()
	_test_tree_images_are_clean()
	_test_tree_feet_are_whole()
	_test_loading_waits_for_the_valley()
	_test_dev_map_turns_as_the_game()
	_test_perf_overlay_reports()
	_test_plants_are_named_and_coloured()
	_test_trees_sharpness_eases_with_the_zoom()
	_test_the_zoom_steps_through_round_levels_up_to_2_5()
	_test_the_shore_field_holds_every_chunk_held()
	_test_the_bounds_of_the_chunks_held()
	_test_the_camera_stops_at_the_band()
	_test_the_watcher_sees_the_client_files()
	_test_the_first_connection_is_tried_again_soon()
	_test_the_view_follows_the_body_tile_by_tile()
	_test_the_stable_hash_is_the_servers()
	_test_the_items_mesh_is_built_once_per_change()
	_test_the_redraw_queue_holds_each_chunk_once_and_paces_itself()
	_test_a_redraw_rebuilds_only_when_the_ground_around_changed()
	_test_the_watchdog_reports_a_stalled_frame()
	_test_trees_are_drawn_only_where_the_view_reaches()
	_test_trees_sort_with_bodies_by_their_foot()
	_test_the_bounds_held_are_kept_as_chunks_come_and_go()
	_test_waiting_chunks_are_kept_nearest_the_camera_first()
	_test_the_fog_uniforms_are_sent_only_when_they_change()
	_test_a_redraw_past_the_cameras_reach_waits_for_the_camera()
	_test_a_hidden_chunk_is_parked_and_comes_back_as_it_was()
	_test_the_depth_behind_the_mouse_is_found_again_only_when_it_may_change()
	_test_the_fog_rings_leave_only_what_is_clear()
	_test_the_fog_rings_are_built_when_the_ground_moves()
	_test_the_far_view_leaves_its_hole_only_under_built_ground()
	_test_the_water_meshes_cover_the_water_once()
	_test_the_water_meshes_join_across_chunks()
	_test_the_water_meshes_measure_the_bank()


## Once the main loop runs: the tests that wait for frames, then the end.
func _initialize() -> void:
	await _test_the_shore_field_in_slices()
	print("client tests: %d checks, %d failures" % [_checks, _failures])
	quit(1 if _failures > 0 else 0)


func _check(condition: bool, what: String) -> void:
	_checks += 1
	if not condition:
		_failures += 1
		push_error("FAIL: " + what)


func _test_protocol_round_trip() -> void:
	var message := Protocol.ping(42)
	var back := Protocol.decode(Protocol.encode(message))
	_check(back.get("type") == "ping", "ping type survives round trip")
	_check(int(back.get("nonce")) == 42, "ping nonce survives round trip")


func _test_protocol_rejects_garbage() -> void:
	_check(Protocol.decode("not json").is_empty(), "garbage decodes to empty")
	_check(Protocol.decode("[1, 2]").is_empty(), "array decodes to empty")
	_check(Protocol.decode("{\"nonce\": 1}").is_empty(), "missing type decodes to empty")


func _test_protocol_wire_format_matches_server() -> void:
	# Must match the `wire_format_is_tagged_snake_case` test in the Rust protocol crate.
	_check(Protocol.encode(Protocol.ping(1)) == "{\"type\":\"ping\",\"nonce\":1}", "ping wire format matches server")
	_check(Protocol.encode(Protocol.move_to(2, 3)) == "{\"type\":\"move_to\",\"x\":2,\"y\":3}", "move_to wire format matches server")
	_check(Protocol.hello().get("protocol_version") == Protocol.PROTOCOL_VERSION, "hello carries protocol version")
	_check(Protocol.encode(Protocol.dev_teleport(7, 9)) == "{\"type\":\"dev_teleport\",\"x\":7,\"y\":9}", "dev_teleport wire format matches server")


func _test_iso_round_trip() -> void:
	for y in range(-3, 40):
		for x in range(-3, 40):
			var centre := Iso.tile_to_screen(x, y)
			var back := Iso.screen_to_tile(centre)
			if back != Vector2i(x, y):
				_check(false, "tile (%d, %d) round trips through its centre, got %s" % [x, y, back])
				return
	_check(true, "every tile round trips through its centre")
	# A point just inside the diamond edges still maps to the same tile.
	var c := Iso.tile_to_screen(5, 7)
	_check(Iso.screen_to_tile(c + Vector2(Iso.TILE_W * 0.24, 0)) == Vector2i(5, 7), "east inner edge")
	_check(Iso.screen_to_tile(c + Vector2(0, Iso.TILE_H * 0.24)) == Vector2i(5, 7), "south inner edge")


func _test_iso_neighbours_are_distinct() -> void:
	var seen := {}
	for y in 8:
		for x in 8:
			seen[Iso.tile_to_screen(x, y)] = true
	_check(seen.size() == 64, "64 tiles project to 64 distinct centres")
	_check(Iso.tile_to_screen(1, 0).x > Iso.tile_to_screen(0, 0).x, "x grows to the east")
	_check(Iso.tile_to_screen(0, 1).x < Iso.tile_to_screen(0, 0).x, "y grows to the west")
	_check(Iso.tile_to_screen(1, 1).y > Iso.tile_to_screen(0, 0).y, "both grow to the south")


func _test_main_scene_lets_clicks_reach_the_world() -> void:
	# A full-window Control with the default STOP filter swallows every mouse
	# event before the world's `_unhandled_input` sees it: no hover, no walking.
	# Read as text: loading the scene would compile scripts that need the `Net`
	# autoload, which this runner does not have.
	var text := FileAccess.get_file_as_string("res://main.tscn")
	var start := text.find("[node name=\"Main\"")
	_check(start >= 0, "main scene has a Main node")
	var end := text.find("[node ", start + 1)
	var main_block := text.substr(start, end - start if end >= 0 else -1)
	_check(main_block.contains("mouse_filter = 2"), "Main node ignores mouse events (MOUSE_FILTER_IGNORE)")
	# The interface lives in a CanvasLayer: the world camera must not move it.
	_check(text.contains("[node name=\"Ui\" type=\"CanvasLayer\""), "the interface has its own canvas layer")
	_check(text.contains("[node name=\"Hud\" type=\"PanelContainer\" parent=\"Ui\"]"), "the HUD is in it")
	_check(text.contains("[node name=\"Help\" type=\"Label\" parent=\"Ui\"]"), "the help line is in it")


func _test_time_of_day() -> void:
	# The day starts at dawn, 06:00; 58 % of the day is 20:00 (night falls).
	_check(GameClock.time_of_day(0.0, 72000.0) == Vector2i(6, 0), "dawn at the day's first tick")
	_check(GameClock.time_of_day(36000.0, 72000.0) == Vector2i(18, 0), "half a day later it is 18:00")
	_check(GameClock.time_of_day(72000.0 * 0.58, 72000.0) == Vector2i(19, 55), "night falls just before 20:00")
	_check(GameClock.time_of_day(72000.0 * 0.9, 72000.0) == Vector2i(3, 36), "past midnight wraps")
	_check(GameClock.time_of_day(72000.0 * 3.5, 72000.0) == Vector2i(18, 0), "later days wrap the same way")
	# Between two server messages the clock runs at the announced pace.
	var clock := GameClock.new()
	clock.set_clock({"tick": 100, "ticks_per_second": 10, "ticks_per_day": 72000, "time_scale_permille": 2000}, 1000)
	_check(is_equal_approx(clock.tick_at(1000), 100.0), "the tick just told")
	_check(is_equal_approx(clock.tick_at(3000), 140.0), "two real seconds at double pace: 40 ticks")
	clock.sync(500.0, 3000)
	_check(is_equal_approx(clock.tick_at(3500), 510.0), "resynced on a day start")


func _test_tooltips() -> void:
	var flint := {"name": "Flint", "mass_g": 400, "edible": false, "description": "A hard stone."}
	_check(Tooltip.describe_item(flint) == "Flint · 400 g\nA hard stone.", "an item: name, mass, description")
	var log := {"name": "Log", "mass_g": 25000, "edible": false, "description": ""}
	_check(Tooltip.describe_item(log) == "Log · 25.0 kg", "kilograms past a thousand grams, no empty line")
	var berries := {"name": "Berries", "mass_g": 100, "edible": true, "description": "Ripe."}
	_check(Tooltip.describe_item(berries).begins_with("Berries · 100 g · edible"), "edible is said")
	_check(Tooltip.describe_body(2, true, false) == "body 2 · asleep", "a sleeping body")
	_check(Tooltip.describe_body(1, false, true) == "body 1 (you)", "the piloted body")


func _test_inventory_lines_and_refusals() -> void:
	var flint := {"name": "Flint", "mass_g": 400}
	_check(Tooltip.inventory_line(flint, true) == "[in hand] Flint · 400 g", "the item in hand is marked")
	_check(Tooltip.inventory_line({"name": "Log", "mass_g": 25000}, false) == "Log · 25.0 kg", "kilograms")
	_check(Tooltip.load_line(1900, 20000) == "1.9 / 20.0 kg", "the load over the capacity")
	_check(Tooltip.refusal_words("pick_up", "too_heavy") == "cannot pick up: too heavy", "a refusal in words")
	_check(Tooltip.refusal_words("eat", "not_food") == "cannot eat: not food", "another")


func _test_chunk_of_matches_the_server() -> void:
	# Must agree with `Pos::chunk` on the server: floor division, chunk (0, 0)
	# holding tiles 0..32.
	_check(Iso.chunk_of(Vector2i(0, 0), 32) == Vector2i(0, 0), "tile (0, 0) is in chunk (0, 0)")
	_check(Iso.chunk_of(Vector2i(31, 32), 32) == Vector2i(0, 1), "tile (31, 32) is in chunk (0, 1)")
	_check(Iso.chunk_of(Vector2i(-1, -33), 32) == Vector2i(-1, -2), "negative tiles floor")
	# A tile's centre in millimetres lands exactly where the tile is drawn.
	_check(Iso.point_to_screen(Vector2(5500, 7500)) == Iso.tile_to_screen(5, 7), "point and tile agree on the centre")
	_check(Iso.point_to_screen(Vector2(500, 500)) == Vector2.ZERO, "tile (0, 0) centres on the origin")
	# The format contract: one metre is 64 px at scale 1, both ways.
	_check(Iso.TILE_W == 64.0 and Iso.TILE_H == 32.0, "a cell is a 64 × 32 diamond")
	_check(Iso.tile_to_screen(1, 0).x - Iso.tile_to_screen(0, 0).x == 32.0, "one metre east is half a diamond across")
	_check(Iso.PX_PER_M == Iso.TILE_W, "height uses the same scale as width")


func _test_ground_art_is_reachable() -> void:
	# The validated art lives in content/art/, reached as res://art/ through
	# the client/art link; the grass texture is the first of it.
	_check(ResourceLoader.exists("res://art/ground/ground_grass_01_summer.png"), "the grass texture is importable")
	var uvs := ChunkView.ground_uvs(4, 8)
	_check(uvs[0] == Vector2(1.0, 2.0) and uvs[2] == Vector2(1.25, 2.25), "a tile maps to its square of the 4 m texture")
	_check(ResourceLoader.exists("res://art/ground/ground_grass_02_summer.png"), "the worn grass is importable")
	_check(ResourceLoader.exists("res://world/ground.gdshader"), "the ground shader is there")
	var material := ChunkView.ground_material("grass")
	_check(material.shader != null and material.get_shader_parameter("noise_texture") != null, "the ground material has its shader and noise")
	_check(material.get_shader_parameter("has_worn") == true, "the grass material has its worn layer")
	# A wood spills over the grass beside it: the shared corners are opaque,
	# the far ones fade, and a tile far from any wood is not in the fringe.
	var view := ChunkView.new()
	var tiles: Array = []
	for y in 6:
		for x in 6:
			tiles.append("forest" if x < 2 else "grass")
	view.show_chunk(Vector2i.ZERO, 6, tiles, [], {})
	var fringe := view.fringe("forest")
	var by_tile: Dictionary = {}
	for spill in fringe:
		by_tile[spill["tile"]] = spill["alphas"]
	_check(by_tile.has(Vector2i(2, 3)) and by_tile[Vector2i(2, 3)][0] == 1.0 and by_tile[Vector2i(2, 3)][3] == 1.0, "the grass tile touching the wood is opaque on that side")
	_check(by_tile.has(Vector2i(3, 3)) and by_tile[Vector2i(3, 3)][1] < by_tile[Vector2i(3, 3)][0], "one tile further the spill fades")
	_check(not by_tile.has(Vector2i(5, 3)), "three tiles away the grass is itself")
	# Water knows its banks: a water tile touching grass is near on that
	# side, and open water far from any bank is not in the shore.
	var water: Array = []
	for y in 8:
		for x in 8:
			water.append("grass" if x < 2 else "water")
	view.show_chunk(Vector2i.ZERO, 8, water, [], {})
	var banks := view.shore("water")
	_check(banks.has(Vector2i(2, 4)) and banks[Vector2i(2, 4)][0] == 1.0 and banks[Vector2i(2, 4)][1] < 1.0, "the water tile at the bank is nearest on the bank's side")
	_check(not banks.has(Vector2i(7, 4)), "open water far from the bank has no shore")
	view.free()
	# The development watcher knows the shader and every ground texture,
	# and a reload from the files leaves the materials usable.
	var watched := DevWatcher.watched()
	_check(watched.has("res://world/ground.gdshader") and watched.has(ChunkView.GROUND_SPEC) and watched.has("res://art/ground/ground_grass_01_summer.png"), "the watcher lists the shader, the spec and the ground textures")
	var spec := ChunkView.ground_spec()
	_check(spec.has("grass") and spec.has("forest") and spec.has("water"), "the ground spec names the three textured terrains")
	_check(ChunkView.ground_params("forest").get("base_tint") is Color, "a list of three numbers in the spec is a colour")
	ChunkView.reload_art()
	_check(ChunkView.ground_material("grass").shader.code.contains("void fragment()") and ChunkView.ground_texture("grass") != null, "a live reload reads the shader and textures from their files")
	# Decorations: their images load, and a tile's hash is fixed and spread.
	_check(ChunkView.decor_textures("tuft").size() > 5 and ChunkView.decor_textures("pebble").size() > 5 and ChunkView.decor_textures("flower").size() > 5, "the tuft, pebble and flower decorations are there")
	_check(ChunkView.tile_hash(3, 7, 1) == ChunkView.tile_hash(3, 7, 1) and ChunkView.tile_hash(3, 7, 1) != ChunkView.tile_hash(4, 7, 1), "a tile's hash is fixed and differs from its neighbour's")
	var forest := ChunkView.ground_material("forest")
	_check(forest != material and forest.get_shader_parameter("base_tint") != material.get_shader_parameter("base_tint"), "the forest has its own material, in the shade")


## A small map for the brooks: grass, a river along x = 30..34, and a
## brook from a spring at (5, 10) winding east to the river, two wide for
## a stretch. Returns tile -> terrain.
func _brook_map() -> Dictionary:
	var map := {}
	for y in range(0, 40):
		for x in range(0, 40):
			map[Vector2i(x, y)] = "water" if x >= 30 and x <= 34 else "grass"
	var path: Array = []
	for x in range(5, 12):
		path.append(Vector2i(x, 10))
	for k in range(0, 5):
		path.append(Vector2i(12 + k, 11 + k))
	for x in range(17, 30):
		path.append(Vector2i(x, 15))
	for t: Vector2i in path:
		map[t] = "brook"
	# Two wide from x = 20 to 26.
	for x in range(20, 27):
		map[Vector2i(x, 16)] = "brook"
	return map


func _test_brooks_are_lines_on_the_ground() -> void:
	var map := _brook_map()
	var terrain := func(t: Vector2i) -> String: return str(map.get(t, ""))
	# Thinning keeps a line one wide and wears a band two wide to one.
	var line := {Vector2i(0, 0): true, Vector2i(1, 0): true, Vector2i(2, 1): true, Vector2i(3, 1): true}
	_check(BrookLines.thin(line).size() == 4, "a thin brook stays as it is")
	var band := {}
	for x in 8:
		band[Vector2i(x, 0)] = true
		band[Vector2i(x, 1)] = true
	var thinned := BrookLines.thin(band)
	_check(thinned.size() <= 9 and BrookLines.chains(thinned).size() == 1, "a band two wide thins to one line: %s" % [thinned.keys()])
	# The whole brook is one line from its spring to its mouth.
	var lines := BrookLines.lay(terrain, Vector2i(0, 0), Vector2i(40, 40))
	_check(lines.size() == 1, "one brook, one line: %d" % lines.size())
	if lines.size() != 1:
		return
	var brook: Dictionary = lines[0]
	var ends: Array = (brook["ends"] as Array).duplicate()
	ends.sort()
	_check(ends == ["mouth", "spring"], "a spring and a mouth: %s" % [ends])
	# It runs from its spring towards its mouth.
	var sign := BrookLines.runs_towards_end(brook)
	var first_kind: String = (brook["ends"] as Array)[0]
	_check((sign > 0.0) == (first_kind == "spring"), "a brook runs from its spring: %s %s" % [first_kind, sign])
	var pts: PackedVector2Array = brook["pts"]
	var widths: PackedFloat32Array = brook["widths"]
	var alphas: PackedFloat32Array = brook["alphas"]
	var spring := 0 if pts[0].x < pts[pts.size() - 1].x else pts.size() - 1
	var mouth := pts.size() - 1 - spring
	_check(widths[spring] == 0.0, "the spring ends in a point")
	_check(alphas[mouth] == 0.0 and pts[mouth].x > 29.5, "the mouth runs into the river and fades: %s" % [pts[mouth]])
	# The line keeps to the brook's tiles (and the river at its mouth).
	var strays := 0
	for p in pts:
		var t := Vector2i(roundi(p.x), roundi(p.y))
		var near := false
		for dy in range(-1, 2):
			for dx in range(-1, 2):
				var at := str(map.get(t + Vector2i(dx, dy), ""))
				near = near or at == "brook" or at == "water"
		strays += 0 if near else 1
	_check(strays == 0, "%d points of the line stray from the brook" % strays)
	# Wider where the brook is two wide, never wider than its tiles.
	var narrow := 0.0
	var wide := 0.0
	for i in pts.size():
		_check(widths[i] <= 2.0 * BrookLines.WATER_SHARE * (1.0 + BrookLines.WOBBLE) + 1e-4, "width %f within the tiles" % widths[i])
		if absf(pts[i].x - 9.0) < 0.5:
			narrow = widths[i]
		if absf(pts[i].x - 23.0) < 0.5 and pts[i].y > 14.0:
			wide = widths[i]
	_check(wide > narrow * 1.4, "two wide draws wider: %f against %f" % [wide, narrow])
	# The same, every time.
	_check(str(BrookLines.lay(terrain, Vector2i(0, 0), Vector2i(40, 40))) == str(lines), "a brook is laid the same every time")
	# A chunk's window gives the same segments as the whole map, so chunks
	# join without a seam; and a brook cut by the window is not a spring.
	for chunk: Vector2i in [Vector2i(0, 0), Vector2i(16, 0), Vector2i(16, 16)]:
		var lo := chunk
		var hi := chunk + Vector2i(16, 16)
		var margin := Vector2i(BrookLines.MARGIN, BrookLines.MARGIN)
		var local := BrookLines.lay(terrain, lo - margin, hi + margin)
		_check(_owned(local, lo, hi) == _owned(lines, lo, hi), "chunk %s draws the same brook as the map" % [chunk])
	var cut := BrookLines.lay(terrain, Vector2i(0, 0), Vector2i(10, 40))
	_check(cut.size() == 1 and (cut[0]["ends"] as Array).has("cut"), "a brook cut by the window is not a spring")


## The segments of `lines` a chunk of tiles [lo, hi) draws, rounded.
func _owned(lines: Array, lo: Vector2i, hi: Vector2i) -> Array:
	var out: Array = []
	for line: Dictionary in lines:
		var pts: PackedVector2Array = line["pts"]
		var widths: PackedFloat32Array = line["widths"]
		for i in pts.size() - 1:
			if BrookLines.owns(line, i, lo, hi):
				out.append([pts[i].snapped(Vector2(0.001, 0.001)), pts[i + 1].snapped(Vector2(0.001, 0.001)), snappedf(widths[i], 0.001)])
	out.sort()
	return out


func _test_trees_stand_on_their_foot() -> void:
	# Every kind of tree of the content has at least one image, sharp
	# enough for its height on screen (D40) up close.
	var text := FileAccess.get_file_as_string("res://../content/species.ron")
	if text.is_empty():
		text = FileAccess.get_file_as_string(ProjectSettings.globalize_path("res://").path_join("../content/species.ron"))
	var ids: Array = []
	var heights: Dictionary = {}
	var re := RegEx.new()
	re.compile("id: \"([a-z_]+)\", name: \"[^\"]*\", height_m: ([0-9.]+)")
	for m in re.search_all(text):
		ids.append(m.get_string(1))
		heights[m.get_string(1)] = float(m.get_string(2))
	_check(ids.size() >= 6, "the content names its kinds of tree: %s" % [ids])
	for id: String in ids:
		var images := Trees.textures(id)
		_check(not images.is_empty(), "the %s has an image" % id)
		for t: Texture2D in images:
			_check(t.get_height() >= 760, "%s image is tall enough: %d" % [id, t.get_height()])
	# A tree stands on its foot, its size within the jitter, the same every
	# time; neighbours do not all show the same image.
	var oak: Array = Trees.textures("oak")
	var at := Vector2(12_400, 30_700)
	var r := Trees.rect(at, 400, oak[0])
	var foot := Iso.point_to_screen(at)
	_check(r.end.y == foot.y and absf(r.get_center().x - foot.x) < 0.01, "a tree stands on its foot")
	var tall := Scale.shown(4.0) * Iso.PX_PER_M
	_check(absf(r.size.y - tall) <= tall * Trees.SIZE_JITTER + 0.01, "a tree's size stays within its jitter")
	_check(Trees.rect(at, 400, oak[0]) == r and Trees.variant(at, 2) == Trees.variant(at, 2), "the same tree every time")
	var shown := {}
	for i in 20:
		shown[Trees.variant(Vector2(1000 * i + 300, 4_500), oak.size())] = true
	_check(shown.size() == oak.size(), "neighbouring oaks show every image")


func _test_the_shore_field_measures_the_bank() -> void:
	_check(absf(ShoreField.probit(0.975) - 1.96) < 0.01, "the inverse normal curve: %f" % ShoreField.probit(0.975))
	# Water east of x = 40 on a straight edge, over two chunks side by side.
	var terrain := func(t: Vector2i) -> String: return "water" if t.x >= 40 else "grass"
	ShoreField.update(ShoreField.block_of(terrain, Vector2i(32, 0), 32), Vector2i(32, 0), 32)
	ShoreField.update(ShoreField.block_of(terrain, Vector2i(32, 32), 32), Vector2i(32, 32), 32)
	for y: float in [10.0, 31.9, 32.1, 50.0]:
		_check(absf(ShoreField.distance_at(Vector2(40.0, y))) < 0.15, "the edge at x = 40 (y %s): %f" % [y, ShoreField.distance_at(Vector2(40.0, y))])
		_check(absf(ShoreField.distance_at(Vector2(38.5, y)) - 1.5) < 0.2, "1.5 tiles inland: %f" % ShoreField.distance_at(Vector2(38.5, y)))
		_check(absf(ShoreField.distance_at(Vector2(41.5, y)) + 1.5) < 0.2, "1.5 tiles out in the water: %f" % ShoreField.distance_at(Vector2(41.5, y)))
	# A step of one tile in the edge comes out smooth: halfway along the
	# step, the edge lies between the two lines of tiles.
	var stepped := func(t: Vector2i) -> String: return "water" if t.x >= (40 if t.y < 80 else 41) else "grass"
	ShoreField.update(ShoreField.block_of(stepped, Vector2i(32, 64), 32), Vector2i(32, 64), 32)
	var at_step := ShoreField.distance_at(Vector2(40.5, 80.0))
	_check(absf(at_step) < 0.25, "the step is smoothed: %f at its middle" % at_step)
	# A pond's corner is rounded: the corner point of the tiles lies on
	# the land side of the smoothed edge.
	var pond := func(t: Vector2i) -> String: return "water" if t.x >= 100 and t.y >= 100 else "grass"
	ShoreField.update(ShoreField.block_of(pond, Vector2i(96, 96), 32), Vector2i(96, 96), 32)
	_check(ShoreField.distance_at(Vector2(100.0, 100.0)) > 0.15, "a corner comes out round: %f" % ShoreField.distance_at(Vector2(100.0, 100.0)))
	# A ford: water for the bank, and its own channel, high inside, nothing
	# far from it.
	var forded := func(t: Vector2i) -> String:
		if t.x < 140:
			return "grass"
		return "ford" if t.y >= 140 and t.y < 146 else "water"
	ShoreField.update(ShoreField.block_of(forded, Vector2i(128, 128), 32), Vector2i(128, 128), 32)
	_check(ShoreField.distance_at(Vector2(150.0, 143.0)) < -1.0, "a ford is water for the bank")
	_check(ShoreField.ford_at(Vector2(150.0, 143.0)) > 0.7, "inside the ford: %f" % ShoreField.ford_at(Vector2(150.0, 143.0)))
	_check(ShoreField.ford_at(Vector2(150.0, 152.0)) < 0.05, "deep water away from the ford: %f" % ShoreField.ford_at(Vector2(150.0, 152.0)))
	# The water runs where the server says, the land between keeps still.
	var river := func(t: Vector2i) -> String: return "water" if t.x >= 200 and t.x < 210 else "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.0, 0.6)
	# A block without water lays land, no ford and still water, the same
	# bytes a wet block's encoding gives them, from pieces made once.
	ShoreField.update(ShoreField.block_of(river, Vector2i(192, 192), 32), Vector2i(192, 192), 32, runs)
	var meadow := func(_t: Vector2i) -> String: return "grass"
	ShoreField.update(ShoreField.block_of(meadow, Vector2i(192, 192), 32), Vector2i(192, 192), 32)
	var dry_bank := ShoreField._bank_image.get_region(Rect2i(192 * ShoreField.PX, 192 * ShoreField.PX, 32 * ShoreField.PX, 32 * ShoreField.PX)).get_data()
	var dry_water := ShoreField._water_image.get_region(Rect2i(192, 192, 32, 32)).get_data()
	var all_land := PackedByteArray()
	all_land.resize(dry_bank.size())
	all_land.fill(255)
	var still_water := PackedByteArray()
	for i in 32 * 32:
		still_water.append_array(PackedByteArray([0, 128, 128, 255]))
	_check(dry_bank == all_land and dry_water == still_water, "a dry block: all land, no ford, still water")
	ShoreField.update(ShoreField.block_of(river, Vector2i(192, 192), 32), Vector2i(192, 192), 32, runs)
	_check(ShoreField.flow_at(Vector2(205.0, 200.0)).distance_to(Vector2(0.0, 0.6)) < 0.03, "the water runs down the river: %s" % ShoreField.flow_at(Vector2(205.0, 200.0)))
	_check(ShoreField.flow_at(Vector2(194.0, 200.0)).length() < 0.03, "still far from the water: %s" % ShoreField.flow_at(Vector2(194.0, 200.0)))
	_check(ShoreField.flow_at(Vector2(199.5, 200.0)).y > 0.4, "the bank's edge runs with the river: %s" % ShoreField.flow_at(Vector2(199.5, 200.0)))
	var edge := ShoreField.ford_at(Vector2(150.0, 140.0))
	_check(edge > 0.3 and edge < 0.7, "the ford's edge is soft: %f" % edge)
	# The texture repeats over the map: a chunk past its width lands where
	# the first ones were, and reads back at its own place.
	var far_edge := func(t: Vector2i) -> String: return "water" if t.x >= ShoreField.TILES + 50 else "grass"
	var map_size := ShoreField.map_size
	ShoreField.map_size = Vector2i(4 * ShoreField.TILES, 4 * ShoreField.TILES)
	ShoreField.update(ShoreField.block_of(far_edge, Vector2i(ShoreField.TILES + 32, 0), 32), Vector2i(ShoreField.TILES + 32, 0), 32)
	ShoreField.map_size = map_size
	_check(absf(ShoreField.distance_at(Vector2(ShoreField.TILES + 50.0, 10.0))) < 0.15, "the edge past the texture's width: %f" % ShoreField.distance_at(Vector2(ShoreField.TILES + 50.0, 10.0)))
	_check(absf(ShoreField.distance_at(Vector2(ShoreField.TILES + 48.5, 10.0)) - 1.5) < 0.2, "inland past the texture's width")


## The bank's smoothing, one convolution of the tiles, gives the bytes
## the three box blurs of the tiles drawn at PX pixels gave (the way it
## was computed before, kept here as the reference).
func _test_the_shore_field_blurs_as_three_boxes() -> void:
	var size := 32
	var n := size + 2 * ShoreField.MARGIN
	var px := ShoreField.PX
	var w := n * px
	var mask := PackedFloat32Array()
	mask.resize(n * n)
	var noise := FastNoiseLite.new()
	noise.frequency = 0.05
	for ty in n:
		for tx in n:
			mask[ty * n + tx] = 1.0 if noise.get_noise_2d(tx, ty) < -0.05 else 0.0
	var field := PackedFloat32Array()
	field.resize(w * w)
	for y in w:
		for x in w:
			field[y * w + x] = mask[floori(float(y) / px) * n + floori(float(x) / px)]
	for _pass in ShoreField.BLUR_PASSES:
		_box_rows(field, w, ShoreField.BLUR)
		_box_columns(field, w, ShoreField.BLUR)
	var inner := size * px
	var sigma := sqrt(ShoreField.BLUR_PASSES * (pow(2.0 * ShoreField.BLUR + 1.0, 2.0) - 1.0) / 12.0) / px
	var expected := PackedByteArray()
	expected.resize(inner * inner)
	for y in inner:
		for x in inner:
			var v := field[(y + ShoreField.MARGIN * px) * w + x + ShoreField.MARGIN * px]
			var d := -sigma * ShoreField.probit(clampf(v, 0.0005, 0.9995))
			expected[y * inner + x] = int(round(clampf((d + ShoreField.RANGE) / (2.0 * ShoreField.RANGE), 0.0, 1.0) * 255.0))
	var out := [PackedByteArray(), PackedByteArray()]
	var weights := [ShoreField._weights_of("bank", size), ShoreField._weights_of("centre", size), ShoreField._weights_of("ford", size)]
	var none := PackedFloat32Array()
	none.resize(n * n)
	ShoreField._smooth(out, mask, none, none, none, n, size, weights, false, false, true)
	var bank: PackedByteArray = out[0]
	var worst := 0
	for i in inner * inner:
		worst = maxi(worst, absi(int(bank[i]) - int(expected[i])))
	_check(bank.size() == expected.size() and worst == 0, "the bank's distance as the three blurs gave it (worst difference %d)" % worst)


## One box blur along the rows, its edges held (the former smoothing).
func _box_rows(field: PackedFloat32Array, w: int, radius: int) -> void:
	var line := PackedFloat32Array()
	line.resize(w)
	var span := 2 * radius + 1
	for y in w:
		var row := y * w
		var sum := 0.0
		for k in range(-radius, radius + 1):
			sum += field[row + clampi(k, 0, w - 1)]
		for x in w:
			line[x] = sum / span
			sum += field[row + mini(x + radius + 1, w - 1)] - field[row + maxi(x - radius, 0)]
		for x in w:
			field[row + x] = line[x]


func _box_columns(field: PackedFloat32Array, w: int, radius: int) -> void:
	var line := PackedFloat32Array()
	line.resize(w)
	var span := 2 * radius + 1
	for x in w:
		var sum := 0.0
		for k in range(-radius, radius + 1):
			sum += field[clampi(k, 0, w - 1) * w + x]
		for y in w:
			line[y] = sum / span
			sum += field[mini(y + radius + 1, w - 1) * w + x] - field[maxi(y - radius, 0) * w + x]
		for y in w:
			field[y * w + x] = line[y]


func _test_the_terrain_grid_holds_bytes() -> void:
	var codes := TerrainGrid.codes(["grass", "water", "ford", "brook"])
	_check(codes == PackedByteArray([TerrainGrid.GRASS, TerrainGrid.WATER, TerrainGrid.FORD, TerrainGrid.BROOK]), "the names come as codes: %s" % codes)
	_check(TerrainGrid.name_of(TerrainGrid.code_of("sand")) == "sand" and TerrainGrid.code_of("sand") > TerrainGrid.ROCK, "a terrain new to the client gets a code of its own")
	_check(TerrainGrid.name_of(TerrainGrid.UNKNOWN) == "", "unknown has no name")
	# A block across the grid's edge reads back whole, at its place.
	var lo := Vector2i(TerrainGrid.TILES - 2, 5 * TerrainGrid.TILES - 1)
	var block := PackedByteArray([1, 2, 3, 4, 5, 6, 1, 2, 3])
	TerrainGrid.write(lo, 3, block)
	_check(TerrainGrid.window(lo, 3, 3) == block, "a block straddling the grid's edges reads back")
	_check(TerrainGrid.at(lo + Vector2i(2, 1)) == 6 and TerrainGrid.name_at(lo + Vector2i(1, 1)) == "brook", "one tile at a time too")
	# A chunk hidden is forgotten, unless a newer one took its place.
	var chunk := Vector2i(32 * 30, 32 * 30)
	var ones := PackedByteArray()
	ones.resize(32 * 32)
	ones.fill(TerrainGrid.GRASS)
	TerrainGrid.write(chunk, 32, ones)
	var later := chunk + Vector2i(TerrainGrid.TILES, 0)
	var twos := ones.duplicate()
	twos.fill(TerrainGrid.FOREST)
	TerrainGrid.write(later, 32, twos)
	TerrainGrid.clear(chunk, 32)
	_check(TerrainGrid.at(later) == TerrainGrid.FOREST, "hiding a chunk leaves the one laid at its place since")
	TerrainGrid.clear(later, 32)
	_check(TerrainGrid.at(later) == TerrainGrid.UNKNOWN, "hiding the chunk forgets its tiles")


## A chunk's build writes in the ground index which canvases draw each
## tile: the forest on its tiles and, with a fading opacity at the
## corners, over the grass beside it; the grass under the water at the
## bank; the water over the land within two tiles of it.
func _test_the_ground_index_says_what_each_canvas_draws() -> void:
	var origin := Vector2i(320, 320)
	var tiles: Array = []
	for y in 32:
		for x in 32:
			tiles.append("forest" if x < 10 else ("water" if x >= 24 else "grass"))
	var map_size := ShoreField.map_size
	ShoreField.map_size = Vector2i(1024, 1024)
	var view := ChunkView.new()
	view.show_chunk(origin, 32, tiles, [], {})
	view._draw_ground_layers()
	ShoreField.map_size = map_size
	var at := func(x: int, y: int) -> PackedByteArray: return GroundIndex.at(origin + Vector2i(x, y))
	var grass := int(GroundIndex.BITS["grass"])
	var forest := int(GroundIndex.BITS["forest"])
	var water := int(GroundIndex.BITS["water"])
	_check(at.call(5, 5)[0] == forest, "the wood is the forest's: %s" % at.call(5, 5))
	_check(at.call(10, 5)[0] == grass | GroundIndex.FRINGE and at.call(10, 5)[1] == 255, "the grass at the wood's edge, under its spill, opaque at the shared corner: %s" % at.call(10, 5))
	_check(at.call(11, 5)[1] == 170 and at.call(12, 5)[1] == 85 and at.call(13, 5)[1] == 0, "the spill fades over two tiles")
	_check(at.call(12, 5)[0] == grass | GroundIndex.FRINGE and at.call(13, 5)[0] == grass, "and stops after them")
	_check(at.call(22, 5)[0] == grass | water and at.call(21, 5)[0] == grass, "the water reaches over the land within two tiles: %s %s" % [at.call(22, 5), at.call(21, 5)])
	_check(at.call(24, 5)[0] == water | grass and at.call(30, 5)[0] == water, "the grass lies under the water at the bank only: %s %s" % [at.call(24, 5), at.call(30, 5)])
	var shown: Array = []
	for child in view.get_children():
		if child is ChunkView.GroundCanvas and (child as ChunkView.GroundCanvas).shown:
			shown.append((child as ChunkView.GroundCanvas).terrain)
	shown.sort()
	_check(shown == ["forest", "grass", "water"], "one canvas a terrain drawn: %s" % [shown])
	view.free()
	TerrainGrid.clear(origin, 32)


func _test_the_shore_field_in_slices() -> void:
	# On another core, the field comes out the same as at once; an update
	# overtaken by a newer one for its block gives up.
	var river := func(t: Vector2i) -> String:
		if t.x >= 70 and t.x < 76:
			return "ford" if t.y >= 40 and t.y < 44 else "water"
		return "grass"
	var land := func(_t: Vector2i) -> String: return "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.5, 0.0)
	var block := Rect2i(Vector2i(64, 32) * ShoreField.PX, Vector2i(32, 32) * ShoreField.PX)
	var waters := Rect2i(Vector2i(64, 32), Vector2i(32, 32))
	var state := func() -> PackedByteArray:
		var bytes := ShoreField._bank_image.get_region(block).get_data()
		bytes.append_array(ShoreField._water_image.get_region(waters).get_data())
		return bytes
	ShoreField.update(ShoreField.block_of(river, Vector2i(64, 32), 32), Vector2i(64, 32), 32, runs)
	var at_once: PackedByteArray = state.call()
	ShoreField.update(ShoreField.block_of(land, Vector2i(64, 32), 32), Vector2i(64, 32), 32)
	ShoreField.update(ShoreField.block_of(river, Vector2i(64, 32), 32), Vector2i(64, 32), 32, runs, true)
	var landed: PackedByteArray = state.call()
	for i in 60:
		await process_frame
	_check(landed != at_once, "an update on another core writes later")
	_check(state.call() == at_once, "on another core, the same field as at once")
	ShoreField.update(ShoreField.block_of(river, Vector2i(64, 32), 32), Vector2i(64, 32), 32, runs, true)
	ShoreField.update(ShoreField.block_of(land, Vector2i(64, 32), 32), Vector2i(64, 32), 32)
	var bare: PackedByteArray = state.call()
	for i in 60:
		await process_frame
	_check(state.call() == bare, "an overtaken update gives up")


func _test_trees_sharpness_eases_with_the_zoom() -> void:
	_check(Trees.sharpness(2.0) == -1.0 and Trees.sharpness(1.0) == -1.0, "a level sharper up close")
	_check(Trees.sharpness(0.5) == 0.0 and Trees.sharpness(0.35) == 0.0, "the hardware's own level far away")
	_check(is_equal_approx(Trees.sharpness(0.75), -0.5), "eased in between")


func _test_the_zoom_steps_through_round_levels_up_to_2_5() -> void:
	_check(Zoom.MAX == 2.5 and Zoom.clamped(4.0) == 2.5, "the closest zoom is 2.5")
	_check(Zoom.next(1.5, 1) == 2.0 and Zoom.next(2.0, 1) == 2.5, "the wheel steps 1.5, 2, 2.5")
	_check(Zoom.next(2.5, 1) == 2.5, "no closer than 2.5")
	_check(Zoom.next(1.5, -1) == 1.25 and Zoom.next(0.4, -1) == 0.4, "and back down to 0.4")
	_check(Zoom.next(1.7, 1) == 2.0 and Zoom.next(1.7, -1) == 1.5, "from between two levels, the next one")


func _test_plants_are_named_and_coloured() -> void:
	var flora := [
		{"id": "bramble", "name": "Bramble", "family": "shrub", "height_cm": 90},
		{"id": "porcini", "name": "Porcini", "family": "mushroom", "height_cm": 15},
	]
	_check(Array(Tooltip.describe_plants([0, 1, 0, 0], flora)) == ["Bramble ×3", "Porcini"], "a tile's plants are named, counted")
	_check(Tooltip.describe_plants([7, -1], flora).is_empty(), "an unknown kind is left out")
	# Every kind of the flora has its images in the atlas, and a plant
	# stands on its foot, at the game's scale.
	_check(ChunkView.flora_atlas() != null, "the flora's atlas loads")
	for id: String in ["bramble", "dandelion", "porcini", "wild_flax", "yarrow", "woad", "birch_polypore"]:
		_check(ChunkView.flora_variants(id).size() == 2, "%s has two images" % id)
	var region: Rect2 = ChunkView.flora_variants("porcini")[0]
	var r := ChunkView.plant_rect(Vector2(3500, 7500), flora[1], region)
	var foot := Iso.point_to_screen(Vector2(3500, 7500))
	_check(is_equal_approx(r.end.y, foot.y) and is_equal_approx(r.get_center().x, foot.x), "a plant's foot is on its place")
	_check(is_equal_approx(Scale.shown(2.0), 2.0) and is_equal_approx(Scale.shown(0.08), 0.08), "under 3 m, the true height")
	_check(Scale.shown(20.0) > 6.5 and Scale.shown(20.0) < 12.0 and is_equal_approx(Scale.shown(3.0), 3.0), "above, eased towards reality, without a jump")
	_check(r.size.y < 0.2 * Iso.PX_PER_M, "a mushroom stays small")
	_check(ChunkView.plant_tint(flora[1]) != Color.WHITE and ChunkView.plant_tint(flora[0]) == Color.WHITE, "small plants are drawn brighter")
	_check(ChunkView.plant_stands(flora[0]) and not ChunkView.plant_stands(flora[1]), "a bramble stands among the trees, a mushroom lies on the ground")


func _test_perf_overlay_reports() -> void:
	var perf := PerfOverlay.new()
	var text := perf.report(12.5)
	_check(text.contains("fps") and text.contains("12.5 ms") and text.contains("frame step") and text.contains("draw calls") and text.contains("wind on") and text.contains("fog rings (F9)") and text.contains("water meshes (F7)"), "the counters report: %s" % text)
	# F5: the ground plain, then exactly as it was.
	var grass := ChunkView.ground_material("grass")
	var real := grass.shader
	ChunkView.set_plain_ground(true)
	_check(grass.shader != real and grass.shader.code.contains("COLOR = vec4"), "F5 draws the ground plain")
	ChunkView.set_plain_ground(false)
	_check(grass.shader == real, "F5 again gives the ground its own shader back")
	DevTiming.enabled = true
	var t := DevTiming.start()
	DevTiming.add("chunk builds", t)
	DevTiming.tally("tree redraws")
	var lines := perf.breakdown(DevTiming.take(), 1)
	DevTiming.enabled = false
	_check(lines.contains("scripts measured") and lines.contains("chunk builds") and lines.contains("tree redraws: 1") and DevTiming.take().is_empty(), "the breakdown reports and starts again: %s" % lines)
	perf.free()


func _test_dev_map_turns_as_the_game() -> void:
	var map := DevMap.new()
	var top := Vector2(500, 90)
	# Turned as the game: x runs down to the right, y down to the left.
	_check(map._to_screen(Vector2(10, 0), 2.0, top) == top + Vector2(20, 10), "x runs down to the right")
	_check(map._to_screen(Vector2(0, 10), 2.0, top) == top + Vector2(-20, 10), "y runs down to the left")
	var cell := Vector2(123.5, 37.25)
	_check(map._to_cell(map._to_screen(cell, 3.0, top), 3.0, top).distance_to(cell) < 1e-3, "a click lands on the cell drawn there")
	map.show_overview(8, 2, 1, "gw")
	_check(map.available(), "the overview is kept")
	map.free()
	# Painted as bytes, the overview is the one set_pixel painted, every
	# letter and an unknown one included.
	var cells := "gfwobr?x"
	var painted := Image.create(4, 2, false, Image.FORMAT_RGBA8)
	for y in 2:
		for x in 4:
			painted.set_pixel(x, y, DevMap.COLOURS.get(cells[y * 4 + x], Color.MAGENTA))
	var as_bytes := DevMap.image_of(4, 2, cells, DevMap.COLOURS, Color.MAGENTA)
	_check(as_bytes.get_size() == painted.get_size() and as_bytes.get_data() == painted.get_data(), "the overview as bytes is the overview set_pixel painted")
	_check(DevMap.image_of(2, 1, "g\u00e9", DevMap.COLOURS, Color.MAGENTA).get_data() == DevMap.image_of(2, 1, "g?", DevMap.COLOURS, Color.MAGENTA).get_data(), "a letter past ASCII is painted cell by cell, unknown")


func _test_loading_waits_for_the_valley() -> void:
	var p := LoadingProgress.new()
	_check(not p.done(10_000) and p.fraction() == 0.0, "nothing in yet: loading")
	p.has_world = true
	_check(not p.done(10_000), "the world but no chunk yet: loading")
	p.chunk_came(1_000)
	p.chunk_came(1_100)
	p.built = 1
	_check(not p.done(5_000), "a chunk still to build: loading")
	p.built = 2
	_check(not p.done(1_200), "chunks may still be coming: loading")
	p.fields = 1
	_check(not p.done(5_000), "a riverbank still smoothing: loading")
	p.fields = 0
	_check(p.done(1_100 + LoadingProgress.QUIET_MSEC) and p.fraction() == 1.0, "all in, all built, quiet: done")
	p.chunk_came(2_000)
	_check(p.fraction() < 1.0 and p.stage() == "Laying the ground", "a new chunk sets it back")


## The bounds of the chunks held, in chunks.
func _test_the_bounds_of_the_chunks_held() -> void:
	var keys: Array = []
	for y in range(-2, 3):
		for x in range(-2, 3):
			keys.append(Vector2i(x, y))
	_check(Iso.chunk_bounds(keys) == Rect2i(-2, -2, 5, 5), "all held: 5 x 5")
	_check(Iso.chunk_bounds([]) == Rect2i(), "nothing held: empty")


## The camera stops at the band past the detailed ground, whatever the
## zoom: the middle of each edge of the screen at most (the corners show
## fog), or every corner with `corners` 1.
func _test_the_camera_stops_at_the_band() -> void:
	var lo := Vector2(0, 0)
	var hi := Vector2(416, 416)
	var within := func(tile: Vector2) -> bool:
		return tile.x >= lo.x - 10.01 and tile.y >= lo.y - 10.01 and tile.x <= hi.x + 10.01 and tile.y <= hi.y + 10.01
	for view: Vector2 in [Vector2(1600, 900) / 0.4, Vector2(3840, 2160) / 0.4, Vector2(1280, 720)]:
		for corners: float in [0.0, 1.0]:
			var area := Iso.camera_area(lo, hi, view, 10.0, corners)
			_check(area.size.x > 0.0 and area.size.y > 0.0, "room to move at %s" % view)
			var points := [Vector2(-1, 0), Vector2(1, 0), Vector2(0, -1), Vector2(0, 1)]
			if corners == 1.0:
				points = [Vector2(-1, -1), Vector2(1, -1), Vector2(1, 1), Vector2(-1, 1)]
			for centre: Vector2 in [area.position, area.end, Vector2(area.position.x, area.end.y)]:
				for p: Vector2 in points:
					var px := p * view / 2.0
					var tile := centre + Vector2((px.x / 32.0 + px.y / 16.0) / 2.0, (px.y / 16.0 - px.x / 32.0) / 2.0)
					_check(within.call(tile), "the screen stops at the band (%s): %s" % [corners, tile])


## The bank texture repeats every ShoreField.TILES: wider than the
## chunks a client holds (the server's 19 × 19 view, with the margin read
## around a chunk), or
## two chunks held would draw each other's banks.
func _test_the_shore_field_holds_every_chunk_held() -> void:
	var held := (2 * 9 + 1) * 32
	_check(ShoreField.TILES >= held + 2 * ShoreField.MARGIN, "the bank texture is wider than the chunks held: %d" % ShoreField.TILES)


## The limit of the view moves with the body, tile by tile, and stays
## inside the ground held with room for the band and the fog.
func _test_the_view_follows_the_body_tile_by_tile() -> void:
	# 19 chunks held around chunk 50, as the server holds them.
	var held := Rect2(Vector2(41, 41) * 32, Vector2(19, 19) * 32)
	var body := Vector2(50 * 32 + 16.5, 50 * 32 + 16.5)
	var limit := Iso.view_limit(body, 288.0, held, 0.0)
	_check(limit.position == body - Vector2(288, 288) and limit.end == body + Vector2(288, 288), "288 tiles around the body: %s" % limit)
	var step := Iso.view_limit(body + Vector2(1, 0), 288.0, held, 0.0)
	_check(step.position.x == limit.position.x + 1.0, "a tile east, the limit a tile east")
	for x in [0.0, 31.9]:
		var at := Vector2(50 * 32 + x, 50 * 32 + 16.0)
		var l := Iso.view_limit(at, 288.0, held, 0.0)
		_check(is_equal_approx(maxf(l.position.x, held.position.x), l.position.x) and l.end.x <= held.end.x + 0.001,
			"the view never passes the chunks held, wherever the body stands in its chunk: %s" % at)
		_check(l.size.x > 572.0, "and reaches nearly 288 tiles each way: %s" % l.size)


## In development the client restarts when one of its files changes: the
## watcher sees the scripts and the images, not Godot's cache nor the
## ground's files, which reload live.
func _test_the_watcher_sees_the_client_files() -> void:
	var files := DevWatcher.scan()
	_check(files.has("res://main.gd") and files.has("res://world/world_view.gd"), "the scripts are watched")
	_check(files.has("res://world/trees.gdshader"), "the shaders are watched")
	_check(not files.has("res://world/ground.gdshader"), "the ground's shader reloads live, no restart")
	_check(files.keys().all(func(p: String) -> bool: return not p.contains("/.godot/") and not p.ends_with(".import")), "never Godot's cache")


## Until a server has welcomed it, the client tries again soon (the
## server may be starting beside it); afterwards it waits longer.
func _test_the_first_connection_is_tried_again_soon() -> void:
	var net: Node = load("res://net/client.gd").new()
	_check(net._retry_after() == net.FIRST_TRY_AFTER, "never welcomed: it tries again soon")
	net._ever_welcomed = true
	_check(net._retry_after() == net.RECONNECT_AFTER, "once welcomed: it waits longer")
	net.free()


func _test_currents_decode() -> void:
	# Must agree with `current_code` on the server: heading × 256 + speed.
	_check(ShoreField.current_vector(0) == Vector2.ZERO, "no current on land")
	var south := ShoreField.current_vector(0 * 256 + 50)
	_check(south.distance_to(Vector2(0, 0.5)) < 1e-4, "heading 0 runs +y at 0.5 m/s: %s" % south)
	var east := ShoreField.current_vector(4 * 256 + 35)
	_check(east.distance_to(Vector2(0.35, 0)) < 1e-4, "heading 4 runs +x: %s" % east)


## Every tree image stands on its foot (no empty rows under it, so it is
## anchored where it stands) and holds no pale opaque speck of the
## background it was cut from (the birches, white of bark, aside).
func _test_tree_images_are_clean() -> void:
	var dir := DirAccess.open(Trees.DIR)
	if dir == null:
		_check(false, "the trees' folder is there")
		return
	for file in dir.get_files():
		if not file.ends_with("_summer.png"):
			continue
		var image: Image = (load(Trees.DIR + file) as Texture2D).get_image()
		if image.is_compressed():
			image.decompress()
		var w := image.get_width()
		var h := image.get_height()
		var foot := false
		for x in w:
			if image.get_pixel(x, h - 1).a > 0.8:
				foot = true
				break
		_check(foot, "%s stands on its foot" % file)
		# Pale by nature: the birch's and the ash's bark, the linden's
		# flowers.
		if file.begins_with("birch") or file.begins_with("ash") or file.begins_with("linden"):
			continue
		# A speck is the white background left in the image: pale and
		# colourless. The light the trees are drawn with is pale too, but
		# yellow (a leaf's lit edge, about 255, 255, 220).
		var pale := 0
		for y in range(0, h, 2):
			for x in range(0, w, 2):
				var c := image.get_pixel(x, y)
				if c.a > 0.4 and minf(c.r, minf(c.g, c.b)) > 0.73 and maxf(c.r, maxf(c.g, c.b)) - minf(c.r, minf(c.g, c.b)) < 0.08:
					pale += 1
		_check(pale < 15, "%s has no pale specks: %d" % [file, pale])


## Every tree's foot (the lowest 8 % of its image) ends in one whole trunk
## base: no hole where the ground would show through (grass or an outline
## cleared from inside the trunk), and no speck or stroke apart from the
## tree (an outline left behind by grass cleared around it). Read on every
## other pixel, which still sees a hole or a stroke a few pixels wide.
func _test_tree_feet_are_whole() -> void:
	var dir := DirAccess.open(Trees.DIR)
	if dir == null:
		_check(false, "the trees' folder is there")
		return
	for file in dir.get_files():
		if not file.ends_with("_summer.png"):
			continue
		var image: Image = (load(Trees.DIR + file) as Texture2D).get_image()
		if image.is_compressed():
			image.decompress()
		image.convert(Image.FORMAT_RGBA8)
		var data := image.get_data()
		var full_w := image.get_width()
		var full_h := image.get_height()
		var top := full_h - int(full_h * 0.08)
		# The band, every other pixel: true where the tree is.
		var w := floori(full_w / 2.0)
		var h := floori((full_h - top) / 2.0)
		var solid := PackedByteArray()
		solid.resize(w * h)
		for y in h:
			var row := (top + y * 2) * full_w
			for x in w:
				solid[y * w + x] = 1 if data[(row + x * 2) * 4 + 3] >= 128 else 0
		# Clear pixels reached from the band's edges are the background;
		# any other clear pixel is a hole.
		var outside := _reach(solid, w, h, 0, true)
		var holes := 0
		for i in w * h:
			if solid[i] == 0 and outside[i] == 0:
				holes += 1
		# Tree pixels reached from the band's top row are the tree; any
		# other is a speck apart.
		var tree := _reach(solid, w, h, 1, false)
		var specks := 0
		for i in w * h:
			if solid[i] == 1 and tree[i] == 0:
				specks += 1
		_check(holes < 5, "%s has no hole in its foot: %d" % [file, holes])
		_check(specks < 5, "%s has no speck apart from its foot: %d" % [file, specks])


## The cells of `cells` (w x h) equal to `value` reached, through cells of
## that value, from the top row, and with `all_edges` from the other three
## edges too: 1 where reached.
static func _reach(cells: PackedByteArray, w: int, h: int, value: int, all_edges: bool) -> PackedByteArray:
	var seen := PackedByteArray()
	seen.resize(w * h)
	var todo: Array[int] = []
	for x in w:
		todo.append(x)
		if all_edges:
			todo.append((h - 1) * w + x)
	if all_edges:
		for y in h:
			todo.append(y * w)
			todo.append(y * w + w - 1)
	# The background through its sides (4 neighbours), the tree through
	# its corners too (8): a thin outline running diagonally holds.
	var steps: Array[Vector2i] = [Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1)]
	if value == 1:
		steps.append_array([Vector2i(1, 1), Vector2i(1, -1), Vector2i(-1, 1), Vector2i(-1, -1)])
	while not todo.is_empty():
		var i: int = todo.pop_back()
		if seen[i] == 1 or cells[i] != value:
			continue
		seen[i] = 1
		var x := i % w
		var y := floori(float(i) / w)
		for s in steps:
			var nx := x + s.x
			var ny := y + s.y
			if nx >= 0 and nx < w and ny >= 0 and ny < h and seen[ny * w + nx] == 0:
				todo.append(ny * w + nx)
	return seen


## The client's hash is the server's `sim::gen::hash`, bit for bit: the
## same golden values as `hash_is_pinned` in `server/sim/src/gen.rs`
## (written signed: the server's u64 in two's complement).
func _test_the_stable_hash_is_the_servers() -> void:
	_check(StableHash.of([]) == -7046029254386353131, "hash of nothing (0x9e3779b97f4a7c15)")
	_check(StableHash.of([0]) == 4184946803900360070, "hash of 0 (0x3a13eaf202632586)")
	_check(StableHash.of([1]) == -649073454667991946, "hash of 1 (0xf6fe0721c560f476)")
	_check(StableHash.of([1, 2, 3]) == -783912511994487019, "hash of 1, 2, 3 (0xf51efbb943959f15)")
	_check(StableHash.of([-1, 0x9a7c, 7]) == -8449944619587469277, "hash of u64::MAX, 0x9a7c, 7 (0x8abbc42361c40023)")
	_check(StableHash.of([1, 2]) != StableHash.of([2, 1]), "the order matters")
	var seen := {}
	for i in 2000:
		var b := StableHash.below([i, 5], 7)
		_check(b >= 0 and b < 7, "a draw below 7 is in 0..7")
		seen[b] = true
		var u := StableHash.unit([i, 9])
		_check(u >= 0.0 and u < 1.0, "a unit draw is in [0, 1)")
	_check(seen.size() == 7, "every value below 7 comes out")


## The discs of a chunk's items (branches, pebbles) are one mesh, built
## when the items change and kept across the chunk's redraws: every
## neighbour coming or going redraws a chunk, and rebuilding 64-segment
## discs each time cost seconds a teleport (10 November 2026).
func _test_the_items_mesh_is_built_once_per_change() -> void:
	var view := ChunkView.new()
	var tiles: Array = []
	for i in 16:
		tiles.append("grass")
	var looks := {"branch": Color.SADDLE_BROWN}
	var items: Array = [{"id": 1, "x": 1, "y": 2, "material": "branch"}]
	view.show_chunk(Vector2i.ZERO, 4, tiles, items, looks)
	_check(view.items_built == 1 and view._items_mesh != null, "the mesh is built when the items arrive")
	view.show_chunk(Vector2i.ZERO, 4, tiles, items.duplicate(true), looks)
	_check(view.items_built == 1, "the same items again: not built again")
	items.append({"id": 2, "x": 1, "y": 2, "material": "branch"})
	view.show_chunk(Vector2i.ZERO, 4, tiles, items, looks)
	_check(view.items_built == 2 and view._items_mesh.get_surface_count() == 1, "an item laid: built again, one surface")
	view.show_chunk(Vector2i.ZERO, 4, tiles, [], looks)
	_check(view.items_built == 3 and view._items_mesh == null, "no items left: no mesh")
	view.free()


## A chunk shown or hidden asks its eight neighbours to redraw: the queue
## holds each chunk once, hands out a few a frame within its budget (at
## least one), and forgets chunks freed meanwhile.
func _test_the_redraw_queue_holds_each_chunk_once_and_paces_itself() -> void:
	var queue := RedrawQueue.new()
	var chunks: Array = []
	for i in 5:
		chunks.append(Node.new())
	queue.add(chunks[0])
	queue.add(chunks[0])
	queue.add(chunks[1])
	_check(queue.size() == 2, "a chunk asked twice is held once")
	var slow := queue.take(2000.0, 1500.0)
	_check(slow.size() == 1 and slow[0] == chunks[0] and queue.size() == 1, "too costly for two a frame: one, the oldest first")
	for c: Node in chunks:
		queue.add(c)
	var quick := queue.take(2000.0, 100.0)
	_check(quick.size() == 5 and queue.size() == 0, "cheap redraws all fit the budget: %d" % quick.size())
	var gone := Node.new()
	queue.add(gone)
	queue.add(chunks[2])
	gone.free()
	var left := queue.take()
	_check(left == [chunks[2]] and queue.size() == 0, "a chunk freed meanwhile is dropped")
	for c: Node in chunks:
		c.free()


## Redrawn because a neighbour came or went, a chunk rebuilds its ground
## only when the tiles around it changed; redrawn for anything else, it
## keeps what it built. So a redraw never asks for more redraws: the
## queue only drains.
func _test_a_redraw_rebuilds_only_when_the_ground_around_changed() -> void:
	var tiles: Array = []
	for i in 16:
		tiles.append("grass")
	var hashes := {}
	var view := ChunkView.new()
	view.tiles_hash = func(k: Vector2i) -> int: return int(hashes.get(k, 0))
	view.show_chunk(Vector2i(400, 400), 4, tiles, [], {})
	hashes[Vector2i(100, 100)] = 7
	var alone := view._terrain_signature()
	_check(alone == view._terrain_signature(), "nothing changed around: the same signature, nothing to rebuild")
	hashes[Vector2i(101, 100)] = 9
	_check(alone != view._terrain_signature(), "a neighbour came: the signature changes, the ground rebuilds")
	hashes.erase(Vector2i(101, 100))
	_check(alone == view._terrain_signature(), "it went: back to the signature of before")
	view.free()
	TerrainGrid.clear(Vector2i(400, 400), 4)


## The development watchdog (only with --dev) reports a frame that takes
## over STALL_SECONDS, then every REPORT_EVERY, saying what the main
## thread was doing.
func _test_the_watchdog_reports_a_stalled_frame() -> void:
	_check(not DevWatchdog.report_due(1.9, 0.0) and DevWatchdog.report_due(2.0, 0.0), "the first report at 2 s")
	_check(not DevWatchdog.report_due(4.0, 2.0) and DevWatchdog.report_due(5.0, 2.0), "then one every 3 s")
	var line := DevWatchdog.report(2.5, 120, DevWatchdog.CHUNK_BUILD)
	_check(line.contains("2.5 s") and line.contains("frame 120") and line.contains("chunk build"), "the report says how long, which frame and what was running: %s" % line)
	_check(DevWatchdog.phase_name(99) == "?" and DevWatchdog.phase_name(DevWatchdog.NETWORK).begins_with("network messages"), "every phase has a name")
	_check(DevWatchdog.phase == DevWatchdog.ELSEWHERE, "between frames the main thread is in the engine")


## A stand of oaks and pines on a grid, as the server sends them.
func _stand_of_trees(side: int) -> Trees.Stand:
	var trees: Array = []
	for j in side:
		for i in side:
			trees.append({"at": {"x": 1_700 + 3_100 * i, "y": 2_300 + 2_900 * j}, "species": "oak" if (i + j) % 3 else "pine"})
	trees.append({"at": {"x": 9_000, "y": 9_000}, "species": "no_such_tree"})
	return Trees.Stand.new(trees, {"oak": 2_200, "pine": 3_000})


func _test_trees_are_drawn_only_where_the_view_reaches() -> void:
	var stand := _stand_of_trees(10)
	var parent := RenderingServer.canvas_item_create()
	stand.parent = parent
	_check(stand.size() == 101 and stand.images[100] == -1, "a kind with no image is kept, never drawn")
	# Every tree whose image meets the view has a canvas item, and only the
	# trees whose image covers a cell the view touches; the view moves.
	for seen: Rect2 in [Rect2(-300, 200, 640, 360), Rect2(-100, 400, 640, 360), Rect2(-900, -200, 2560, 1440), Rect2(5000, 5000, 100, 100), Rect2(-300, 200, 640, 360)]:
		var cells := Trees.Stand.cell_range(seen)
		stand.show_cells(cells)
		var exact := true
		for i in stand.size():
			if stand.images[i] < 0:
				exact = exact and not stand.items.has(i)
				continue
			var r := stand.rect(i)
			var near := Trees.Stand.cell_range(r).intersects(cells)
			if r.intersects(seen) and not stand.items.has(i):
				exact = false
			if stand.items.has(i) != near:
				exact = false
		_check(exact, "the trees drawn are those whose image meets the view's cells: %s" % seen)
	var big := stand.items.size()
	stand.show_cells(Trees.Stand.cell_range(Rect2(800, 260, 200, 120)))
	_check(not stand.items.is_empty() and stand.items.size() < big / 4, "a small view at the stand's corner draws a few trees: %d" % stand.items.size())
	stand.show_cells(Trees.Stand.cell_range(Rect2(5000, 5000, 100, 100)))
	_check(stand.items.is_empty(), "a view away from the stand draws none of it")
	stand.show_cells(Trees.Stand.cell_range(Rect2(-300, 200, 640, 360)))
	# Going out of view gives the canvas items back, to be used again.
	var pooled := Trees._pool.size()
	var held := stand.items.size()
	stand.show_cells(Rect2i())
	_check(stand.items.is_empty() and Trees._pool.size() == pooled + held, "out of view, the trees give their canvas items back")
	stand.show_cells(Trees.Stand.cell_range(Rect2(-300, 200, 640, 360)))
	_check(stand.items.size() == held and Trees._pool.size() == pooled, "back in view, they take them again")
	# The see-through flags are kept, drawn or not.
	stand.set_flags(3, 2)
	stand.release()
	_check(stand.items.is_empty() and stand.flags[3] == 2 and stand.refs.count(0) == stand.size(), "released, nothing is held but the flags")
	# The cells of one view less another's, without overlap or gap.
	var a := Rect2i(0, 0, 10, 8)
	for b: Rect2i in [Rect2i(3, 2, 4, 3), Rect2i(-5, -5, 8, 30), Rect2i(20, 20, 3, 3), Rect2i(0, 0, 10, 8), Rect2i()]:
		var area := 0
		var apart := true
		var parts := Trees.Stand.minus(a, b)
		for p: Rect2i in parts:
			area += p.get_area()
			apart = apart and not (b.has_area() and p.intersects(b))
			for q: Rect2i in parts:
				apart = apart and (p == q or not p.intersects(q))
		var common := a.intersection(b).get_area() if b.has_area() and a.intersects(b) else 0
		_check(apart and area + common == a.get_area(), "a view less another: %s less %s" % [a, b])
	RenderingServer.free_rid(parent)


func _test_trees_sort_with_bodies_by_their_foot() -> void:
	# The renderer sorts trees, tall plants and bodies by the point their
	# canvas item stands on (y-sort): a tree's is its foot, the bottom
	# middle of its image, so a body behind it (less x + y) is drawn first
	# and hidden, one in front drawn over it.
	var stand := _stand_of_trees(3)
	for i in stand.size() - 1:
		var foot := Iso.point_to_screen(stand.at[i])
		var r := stand.rect(i)
		_check(is_equal_approx(r.end.y, foot.y) and absf(r.get_center().x - foot.x) < 0.01, "tree %d's image stands on its foot" % i)
		var behind := Iso.point_to_screen(stand.at[i] - Vector2(450, 450))
		var front := Iso.point_to_screen(stand.at[i] + Vector2(450, 450))
		_check(behind.y < foot.y and front.y > foot.y and r.has_point(behind - Vector2(0, 30)), "a body behind tree %d sorts before it, one in front after" % i)
	# Each tree's canvas item hangs from the stand's y-sorted node, at its
	# foot, ordered as the server sent the trees when two feet tie.
	var holder := Node2D.new()
	holder.y_sort_enabled = true
	stand.parent = holder.get_canvas_item()
	stand.show_cells(Trees.Stand.cell_range(Rect2(-2000, -2000, 4000, 4000)))
	_check(stand.items.size() == stand.size() - 1, "every tree in view has its canvas item")
	stand.release()
	holder.free()
	Trees.free_pool()
	_check(Trees._pool.is_empty(), "the pool is freed with the world")



## The bounds of the chunks held are kept as chunks come and go, never
## found again over every key unless one at the edge went.
func _test_the_bounds_held_are_kept_as_chunks_come_and_go() -> void:
	var ledger := ViewLedger.new()
	var chunks := {}
	_check(ledger.bounds(chunks) == Rect2i(), "nothing held: empty bounds")
	for key: Vector2i in [Vector2i(3, 3), Vector2i(5, 4), Vector2i(4, 6), Vector2i(7, 7)]:
		chunks[key] = true
		ledger.hold(key)
	_check(ledger.bounds(chunks) == Rect2i(3, 3, 5, 5) and not ledger.held_stale, "four chunks: the bounds grew to them")
	chunks.erase(Vector2i(5, 4))
	ledger.drop(Vector2i(5, 4))
	_check(not ledger.held_stale and ledger.bounds(chunks) == Rect2i(3, 3, 5, 5), "one inside went: nothing to find again")
	chunks.erase(Vector2i(7, 7))
	ledger.drop(Vector2i(7, 7))
	_check(ledger.held_stale and ledger.bounds(chunks) == Rect2i(3, 3, 2, 4) and not ledger.held_stale, "the corner went: found again, narrower")
	_check(ledger.bounds(chunks) == Iso.chunk_bounds(chunks.keys()), "the same bounds as over every key")


## The chunks waiting to build their ground are kept nearest the camera
## first as they join; the world view takes the first, and orders them
## again only when the camera has moved by a chunk.
func _test_waiting_chunks_are_kept_nearest_the_camera_first() -> void:
	var was: Array = ChunkView.waiting
	ChunkView.waiting = []
	ChunkView.camera_px = Iso.tile_to_screen(100, 100)
	var views: Array = []
	for at: Vector2i in [Vector2i(160, 100), Vector2i(100, 104), Vector2i(60, 60), Vector2i(104, 100)]:
		var view := ChunkView.new()
		view._origin = at
		view._size = 4
		views.append(view)
		ChunkView.enqueue(view)
	ChunkView.enqueue(views[0])
	_check(ChunkView.waiting.size() == 4, "a chunk joins once")
	var order: Array = ChunkView.waiting.map(func(v: ChunkView) -> Vector2i: return v._origin)
	_check(order == [Vector2i(100, 104), Vector2i(104, 100), Vector2i(60, 60), Vector2i(160, 100)], "nearest the camera first: %s" % [order])
	ChunkView.reorder(Iso.tile_to_screen(160, 100))
	_check((ChunkView.waiting[0] as ChunkView)._origin == Vector2i(160, 100), "ordered again around the camera's new place")
	for view: ChunkView in views:
		view.free()
	ChunkView.waiting = was


## The fog's uniforms go to the graphics card only when their value
## changes: as the body walks, the limit's two corners, nothing else.
func _test_the_fog_uniforms_are_sent_only_when_they_change() -> void:
	var far := FarView.new()
	far._ready()
	far.set_held(Vector2(10, 10), Vector2(500, 500), 24.0, 0.7, 0.7, 14.0)
	far.set_cover(Vector2(30, 30), Vector2(480, 480), 35.0, 0.7, 29.0)
	_check(far.uniforms_sent == 12, "the first time, every uniform: %d" % far.uniforms_sent)
	far.set_held(Vector2(10, 10), Vector2(500, 500), 24.0, 0.7, 0.7, 14.0)
	far.set_cover(Vector2(30, 30), Vector2(480, 480), 35.0, 0.7, 29.0)
	_check(far.uniforms_sent == 12, "the same again: none")
	far.set_held(Vector2(11, 10), Vector2(501, 500), 24.0, 0.7, 0.7, 14.0)
	far.set_cover(Vector2(31, 30), Vector2(481, 480), 35.0, 0.7, 29.0)
	_check(far.uniforms_sent == 16, "a tile east: the four corners, nothing else: %d" % far.uniforms_sent)
	far.free()


## A chunk whose neighbour came or went redraws at once when the camera
## can reach it, and waits for the camera otherwise.
func _test_a_redraw_past_the_cameras_reach_waits_for_the_camera() -> void:
	var ledger := ViewLedger.new()
	ledger.chunk_size = 4
	var near := Node.new()
	var key := Vector2i(11, 10)
	_check(not ledger.within_reach(key), "nothing seen yet: nothing within reach")
	ledger.mark_stale(near, key)
	_check(ledger.wake().is_empty() and ledger.stale.get(near) == key, "past the reach: marked, not woken")
	var rect := ledger.chunk_rect(key)
	_check(ledger.see(Rect2(rect.end + Vector2(1000, 1000), Vector2(100, 100))) and ledger.wake().is_empty(), "the screen moved, still a few chunks away: not woken")
	_check(not ledger.see(Rect2(rect.end + Vector2(1000, 1000), Vector2(100, 100))), "the same screen: nothing to look at")
	ledger.see(Rect2(rect.end + Vector2(10, 10), Vector2(100, 100)))
	_check(ledger.within_reach(key) and ledger.wake() == [near] and ledger.stale.is_empty(), "the camera within a chunk: woken, once")
	var gone := Node.new()
	ledger.mark_stale(gone, key)
	gone.free()
	_check(ledger.wake().is_empty() and ledger.stale.is_empty(), "a view freed meanwhile is forgotten")
	near.free()


## A chunk hidden is parked, to come back as it was while the body stays
## within a ring of it; walked away from, or past the room, it is freed.
func _test_a_hidden_chunk_is_parked_and_comes_back_as_it_was() -> void:
	var ledger := ViewLedger.new()
	var key := Vector2i(100, 100)
	ledger.park(key, {"view": 1})
	_check(ledger.take(Vector2i(99, 100)) == null and ledger.parked.has(key), "another chunk: nothing")
	var back: Variant = ledger.take(key)
	_check(back != null and back["view"] == 1 and ledger.parked.is_empty(), "the chunk sent again: its record, no longer parked")
	ledger.park(key, {"view": 2})
	_check(ledger.prune(Vector2i(105, 110)).is_empty() and ledger.parked.has(key), "the body ten chunks away: kept")
	var freed := ledger.prune(Vector2i(111, 100))
	_check(freed.size() == 1 and freed[0]["view"] == 2 and ledger.parked.is_empty(), "eleven chunks away: freed")
	for i in ViewLedger.PARK_MOST + 3:
		ledger.park(Vector2i(i, 0), {"view": i})
	freed = ledger.prune(null)
	_check(freed.size() == 3 and freed[0]["view"] == 0 and ledger.parked.size() == ViewLedger.PARK_MOST and ledger.parked.has(Vector2i(3, 0)), "past the room, the oldest go")
	_check(ledger.take_all().size() == ViewLedger.PARK_MOST and ledger.parked.is_empty(), "a new world: every record out")


## What lies behind the mouse among the items is looked for again only
## when the mouse or the items held changed.
func _test_the_depth_behind_the_mouse_is_found_again_only_when_it_may_change() -> void:
	var ledger := ViewLedger.new()
	var items := {Vector2i(6, 6): [{"id": 1, "x": 201, "y": 201, "material": "branch"}]}
	var mouse := Iso.point_to_screen(Vector2(201_500, 201_500))
	_check(ledger.items_depth_near(items, 1, mouse, 76.8) == 403_000.0, "an item under the mouse: its depth")
	items[Vector2i(6, 6)].append({"id": 2, "x": 200, "y": 200, "material": "branch"})
	_check(ledger.items_depth_near(items, 1, mouse, 76.8) == 403_000.0, "the same mouse and version: not looked for again")
	_check(ledger.items_depth_near(items, 2, mouse, 76.8) == 401_000.0, "an item laid (a new version): looked for again, the farther one found")
	_check(ledger.items_depth_near(items, 2, mouse + Vector2(400, 0), 76.8) == INF, "away from them all: nothing")
## A veil's ring leaves a hole only where the haze is exactly nothing
## (fog.gdshader: the noise moves its distance by at most half a tile,
## so the hole's vertices lie at least half a tile inside), and its
## triangles share every inner edge by index, so no pixel is drawn twice
## nor missed along a seam.
func _test_the_fog_rings_leave_only_what_is_clear() -> void:
	var cases := [
		# [held, corner]: the view's own veils, a corner larger than the half
		# size, no corner, a narrow strip, and a limit with no room at all.
		[Rect2(Vector2(1037.3, 500.7), Vector2(476, 476)), 14.0],
		[Rect2(Vector2(1052.3, 515.7), Vector2(506, 506)), 29.0],
		[Rect2(Vector2(10, 10), Vector2(40, 60)), 100.0],
		[Rect2(Vector2(10, 10), Vector2(40, 60)), 0.0],
		[Rect2(Vector2(0, 0), Vector2(3, 200)), 64.0],
		[Rect2(Vector2(0, 0), Vector2(1, 1)), 64.0],
	]
	for c: Array in cases:
		var held: Rect2 = c[0]
		var corner: float = c[1]
		var outer := held.grow(FarView.VEIL_REACH)
		var mesh := FarView.ring(outer, held, corner, FarView.HOLE_INSET)
		var points: PackedVector2Array = mesh["points"]
		var indices: PackedInt32Array = mesh["indices"]
		if held.size.x <= 2.0 * FarView.HOLE_INSET or held.size.y <= 2.0 * FarView.HOLE_INSET:
			_check(points.size() == 4 and indices.size() == 6, "no room for a hole: a plain quad (%s)" % held)
			continue
		_check(points.size() == 12 and indices.size() == 36, "12 vertices, 12 triangles (%s)" % held)
		var worst := -INF
		for i in range(4, 12):
			worst = maxf(worst, FarView.fog_distance(points[i], held.position, held.end, corner))
		_check(worst <= -0.5, "every hole vertex is where the veil is 0: sdf %.3f at %s corner %s" % [worst, held, corner])
		# Along each hole edge too (the chord of an arc lies inside it).
		for i in range(4, 12):
			var a := points[i]
			var b := points[4 + (i - 4 + 1) % 8]
			for k in range(1, 8):
				var t := a.lerp(b, k / 8.0)
				worst = maxf(worst, FarView.fog_distance(t, held.position, held.end, corner))
		_check(worst <= -0.5, "every point of the hole's edges is where the veil is 0: sdf %.3f (%s)" % [worst, held])
		var edges := {}
		for t in range(0, indices.size(), 3):
			for e in 3:
				var u := indices[t + e]
				var v := indices[t + (e + 1) % 3]
				var edge := Vector2i(mini(u, v), maxi(u, v))
				edges[edge] = int(edges.get(edge, 0)) + 1
		var once := 0
		var twice := 0
		for edge: Vector2i in edges:
			var outer_edge := edge.x < 4 and edge.y < 4
			var hole_edge := edge.x >= 4 and edge.y >= 4
			if edges[edge] == 1:
				once += 1
				_check(outer_edge or hole_edge, "an edge drawn once is on the outer contour or the hole: %s" % edge)
			elif edges[edge] == 2:
				twice += 1
				_check(not outer_edge and not hole_edge, "an edge drawn twice is between two triangles: %s" % edge)
			else:
				_check(false, "an edge drawn %d times: %s" % [edges[edge], edge])
		_check(once == 12 and twice == 12, "12 contour edges, 12 shared edges: %d, %d" % [once, twice])
	# The quad: no texture needed, the veils exist without the far view.
	var veils := FarView.new()
	veils._ready()
	_check(veils.ring_builds() == [0, 0], "no ring before the ground held is known")
	veils.free()


## A veil's ring is built when the ground held or its corner moves, and
## never otherwise: the shader's values may be set again as often as
## WorldView likes.
func _test_the_fog_rings_are_built_when_the_ground_moves() -> void:
	var veils := FarView.new()
	veils._ready()
	veils.set_held(Vector2(100, 100), Vector2(600, 600), 50.0, 0.7, 0.7, 14.0)
	veils.set_cover(Vector2(85, 85), Vector2(615, 615), 35.0, 0.7, 29.0)
	_check(veils.ring_builds() == [1, 1], "a ring each once the ground held is known: %s" % str(veils.ring_builds()))
	veils.set_held(Vector2(100, 100), Vector2(600, 600), 50.0, 0.7, 0.7, 14.0)
	veils.set_cover(Vector2(85, 85), Vector2(615, 615), 35.0, 0.7, 29.0)
	_check(veils.ring_builds() == [1, 1], "the same ground again: nothing built")
	veils.set_held(Vector2(101, 100), Vector2(601, 600), 50.0, 0.7, 0.7, 14.0)
	_check(veils.ring_builds() == [2, 1], "the ground a tile east: the haze's ring built again, the cover's not")
	veils.set_cover(Vector2(85, 85), Vector2(615, 615), 35.0, 0.7, 30.0)
	_check(veils.ring_builds() == [2, 2], "another corner: the cover's ring built again")
	veils.free()


## The far view is drawn with a hole under the view's limit only when
## every chunk under it is held and built, none is waiting to build and
## none is missing from the bounds held; and only from the frame after.
func _test_the_far_view_leaves_its_hole_only_under_built_ground() -> void:
	var chunks := {}
	for y in range(3, 8):
		for x in range(3, 8):
			var chunk := ChunkView.new()
			chunk._last_signature = 1
			chunks[Vector2i(x, y)] = chunk
	var bounds := Rect2i(3, 3, 5, 5)
	var limit := Rect2(Vector2(140.5, 130.25), Vector2(80, 90))
	_check(FarView.limit_built(limit, chunks, bounds, 32, 0), "all built under the limit: the hole may be left")
	_check(not FarView.limit_built(limit, chunks, bounds, 32, 1), "a chunk waiting to build: no hole")
	chunks[Vector2i(6, 6)]._last_signature = 0
	_check(not FarView.limit_built(limit, chunks, bounds, 32, 0), "a chunk under the limit not built: no hole")
	chunks[Vector2i(6, 6)]._last_signature = 1
	chunks[Vector2i(3, 3)]._last_signature = 0
	_check(FarView.limit_built(limit, chunks, bounds, 32, 0), "a chunk not built outside the limit: the hole may be left")
	_check(not FarView.limit_built(Rect2(Vector2(96, 96), Vector2(100, 100)), chunks, bounds, 32, 0), "the limit touching that chunk's last tile: no hole")
	var gone: ChunkView = chunks[Vector2i(7, 3)]
	chunks.erase(Vector2i(7, 3))
	_check(not FarView.limit_built(limit, chunks, bounds, 32, 0), "a chunk missing from the bounds held: no hole")
	gone.free()
	_check(not FarView.limit_built(limit, {}, Rect2i(), 32, 0), "nothing held: no hole")
	for chunk: ChunkView in chunks.values():
		chunk.free()
	_check(not FarView.hole_drawn(true, 10, 10) and FarView.hole_drawn(true, 10, 11) and not FarView.hole_drawn(false, 10, 11), "the hole is drawn from the frame after it is complete")
	# The far view's own ring: the hole is the limit, no corner, no inset.
	var mesh := FarView.ring(Rect2(Vector2(-1000, -1000), Vector2(2048, 2048)), limit, 0.0, 0.0)
	_check((mesh["points"] as PackedVector2Array).size() == 12 and (mesh["indices"] as PackedInt32Array).size() == 36, "the far view's ring has the same form")
	_check((FarView.ring(Rect2(Vector2(0, 0), Vector2(10, 10)), limit, 0.0, 0.0)["points"] as PackedVector2Array).size() == 4, "a hole outside the far view: the plain quad")


## The water meshes (WaterMeshes): every water tile of a chunk is covered
## once, by the water mesh or the bank mesh; the bank mesh lies only along
## the line between water and land; the arrays are whole.
func _test_the_water_meshes_cover_the_water_once() -> void:
	# A river six tiles wide, x in [40, 46), crossing the chunk (32, 0).
	var river := func(t: Vector2i) -> String: return "water" if t.x >= 40 and t.x < 46 else "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.0, 0.5)
	var lo := Vector2i(32, 0)
	var size := 32
	var prepared := WaterMeshes.prepare(ShoreField.block_of(river, lo, size), lo, size, runs)
	_check(not prepared.is_empty(), "a block with water prepares")
	var out: Array = []
	WaterMeshes.build(prepared, out)
	var classes: PackedByteArray = out[2]
	# Cell (i, j) spans the tile centres (i, j) to (i + 1, j + 1) of the
	# chunk: the four cells around a water tile's centre cover it, each by
	# one mesh.
	var missed := 0
	var twice := 0
	for y in range(1, size - 1):
		for x in range(1, size - 1):
			if river.call(lo + Vector2i(x, y)) != "water":
				continue
			for c: Vector2i in [Vector2i(x - 1, y - 1), Vector2i(x, y - 1), Vector2i(x - 1, y), Vector2i(x, y)]:
				var k := classes[c.y * size + c.x]
				if k == 0:
					missed += 1
				elif k != 1 and k != 2:
					twice += 1
	_check(missed == 0 and twice == 0, "every water tile is covered once: %d quarters missed, %d odd" % [missed, twice])
	# Bank cells only along the line: the cells between the tile centres
	# 36 and 37 (3 tiles from the water) carry nothing, the cell across
	# the line (39 to 40) the bank, the river's middle (42 to 43) the water.
	_check(classes[10 * size + (36 - 32)] == 0, "far from the water, no cell: %d" % classes[10 * size + 4])
	_check(classes[10 * size + (39 - 32)] == 2, "across the line, a bank cell: %d" % classes[10 * size + 7])
	_check(classes[10 * size + (42 - 32)] == 1, "in the middle of the river, a water cell: %d" % classes[10 * size + 10])
	var water: Dictionary = out[0]
	var bank: Dictionary = out[1]
	for mesh: Dictionary in [water, bank]:
		var points: PackedVector2Array = mesh["points"]
		var ok := points.size() == (mesh["uvs"] as PackedVector2Array).size() and points.size() == (mesh["colours"] as PackedColorArray).size()
		var indices: PackedInt32Array = mesh["indices"]
		ok = ok and indices.size() % 3 == 0
		for i in indices:
			if i < 0 or i >= points.size():
				ok = false
		_check(ok, "the mesh's arrays are whole (%d points, %d indices)" % [points.size(), indices.size()])
	# The water mesh carries the current: the colour's green is the y
	# component, 0.5 m/s over 2 either way.
	var colours: PackedColorArray = water["colours"]
	var points: PackedVector2Array = water["points"]
	var middle := points.find(WaterMeshes.to_screen(Vector2(43.0, 16.0)))
	_check(middle >= 0 and absf(colours[middle].g - 0.625) < 0.01 and absf(colours[middle].r - 0.5) < 0.01, "the vertices carry the current: %s" % (colours[middle] if middle >= 0 else Color.BLACK))
	_check(middle >= 0 and absf(colours[middle].b - 3.0 / WaterMeshes.DEPTH_TILES) < 0.1, "and the depth, three tiles from either bank: %s" % (colours[middle] if middle >= 0 else Color.BLACK))
	# A block without water prepares to nothing.
	var land := func(_t: Vector2i) -> String: return "grass"
	_check(WaterMeshes.prepare(ShoreField.block_of(land, lo, size), lo, size, runs).is_empty(), "a dry block has no meshes")


## Two chunks side by side build the same values for the lattice points
## they share, to the bit: no seam.
func _test_the_water_meshes_join_across_chunks() -> void:
	# A river running east, y in [10, 16), bending a tile at x = 70, with
	# a ford at the border between the chunks (32, 0) and (64, 0).
	var river := func(t: Vector2i) -> String:
		var top := 10 if t.x < 70 else 11
		if t.y >= top and t.y < top + 6:
			return "ford" if t.x >= 60 and t.x < 68 else "water"
		return "grass"
	var runs := func(t: Vector2i) -> Vector2: return Vector2(0.3 + 0.01 * t.x, 0.0)
	var meshes: Array = []
	for lo: Vector2i in [Vector2i(32, 0), Vector2i(64, 0)]:
		var out: Array = []
		WaterMeshes.build(WaterMeshes.prepare(ShoreField.block_of(river, lo, 32), lo, 32, runs), out)
		meshes.append(out)
	var shared := 0
	var differ := 0
	for m in 2:
		var a: Dictionary = meshes[0][m]
		var b: Dictionary = meshes[1][m]
		if a.is_empty() or b.is_empty():
			continue
		var by_point: Dictionary = {}
		var pa: PackedVector2Array = a["points"]
		for i in pa.size():
			by_point[pa[i]] = i
		var pb: PackedVector2Array = b["points"]
		for j in pb.size():
			if not by_point.has(pb[j]):
				continue
			var i: int = by_point[pb[j]]
			shared += 1
			if (a["uvs"] as PackedVector2Array)[i] != (b["uvs"] as PackedVector2Array)[j] or (a["colours"] as PackedColorArray)[i] != (b["colours"] as PackedColorArray)[j]:
				differ += 1
	_check(shared >= 20 and differ == 0, "the chunks share %d vertices on their border, %d differ" % [shared, differ])


## The bank mesh carries the distance to the water's line as the field
## measured it: zero on the line, the tiles inland, a step smoothed.
func _test_the_water_meshes_measure_the_bank() -> void:
	var stepped := func(t: Vector2i) -> String: return "water" if t.x >= (40 if t.y < 16 else 41) else "grass"
	var lo := Vector2i(32, 0)
	var out: Array = []
	WaterMeshes.build(WaterMeshes.prepare(ShoreField.block_of(stepped, lo, 32), lo, 32, Callable()), out)
	var d: PackedFloat32Array = out[3]
	var l := WaterMeshes.PX * 32 + 6
	# Lattice point a lies a / PX - 0.25 tiles from the chunk's first tile.
	var at := func(x: float, y: float) -> float:
		var a := int(round((x - lo.x + 0.25) * WaterMeshes.PX))
		var b := int(round((y - lo.y + 0.25) * WaterMeshes.PX))
		return d[b * l + a]
	_check(absf(at.call(40.0, 8.0)) < 0.15, "the edge at x = 40: %f" % at.call(40.0, 8.0))
	_check(absf(at.call(38.5, 8.0) - 1.5) < 0.2, "1.5 tiles inland: %f" % at.call(38.5, 8.0))
	_check(absf(at.call(41.5, 8.0) + 1.5) < 0.2, "1.5 tiles out in the water: %f" % at.call(41.5, 8.0))
	_check(absf(at.call(40.5, 16.0)) < 0.25, "the step is smoothed: %f at its middle" % at.call(40.5, 16.0))
	# The bank's vertices carry it: a vertex of the bank mesh at (40, 8)
	# has a distance near zero and a width within the bank's dials.
	var bank: Dictionary = out[1]
	var points: PackedVector2Array = bank["points"]
	var uvs: PackedVector2Array = bank["uvs"]
	var found := -1
	for i in points.size():
		if points[i] == WaterMeshes.to_screen(Vector2(40.0, 8.0)):
			found = i
	_check(found >= 0 and absf(uvs[found].x) < 0.15 and uvs[found].y >= 0.0 and uvs[found].y <= WaterMeshes.WIDTH_MAX * WaterMeshes.BEACH_WIDE + 0.001, "a bank vertex on the line: %s" % (uvs[found] if found >= 0 else Vector2.INF))
