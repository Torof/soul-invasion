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
	_test_the_year_and_its_seasons()
	_test_drink_is_offered_at_running_water_within_reach()
	_test_brooks_are_lines_on_the_ground()
	_test_trees_stand_on_their_foot()
	_test_the_water_meshes_blur_as_three_boxes()
	_test_the_terrain_grid_holds_bytes()
	_test_the_ground_index_says_what_each_canvas_draws()
	_test_each_ground_canvas_covers_only_its_rows()
	_test_currents_decode()
	_test_the_golden_chunk_decodes()
	_test_tree_images_are_clean()
	_test_tree_feet_are_whole()
	_test_loading_waits_for_the_valley()
	_test_dev_map_turns_as_the_game()
	_test_perf_overlay_reports()
	_test_plants_are_named_and_coloured()
	_test_trees_sharpness_eases_with_the_zoom()
	_test_the_zoom_steps_through_round_levels_up_to_2_5()
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
	_test_tall_plants_are_drawn_only_where_the_view_reaches()
	_test_tree_shadows_are_sixteen_sided()
	_test_trees_sort_with_bodies_by_their_foot()
	_test_the_bounds_held_are_kept_as_chunks_come_and_go()
	_test_grounds_built_on_workers_are_laid_nearest_first_within_the_budget()
	_test_the_reach_s_margin_grows_as_the_zoom_pulls_back()
	_test_a_drag_is_smoothed_and_the_view_laid_on_whole_pixels()
	_test_the_fog_uniforms_are_sent_only_when_they_change()
	_test_a_redraw_past_the_cameras_reach_waits_for_the_camera()
	_test_the_chunks_the_camera_reaches_are_found_exactly()
	_test_a_chunk_is_built_when_the_reach_touches_it_and_released_after_the_grace()
	_test_the_depth_behind_the_mouse_is_found_again_only_when_it_may_change()
	_test_the_fog_rings_leave_only_what_is_clear()
	_test_the_fog_rings_are_built_when_the_ground_moves()
	_test_the_far_view_leaves_its_hole_only_under_built_ground()
	_test_the_water_meshes_cover_the_water_once()
	_test_the_water_meshes_join_across_chunks()
	_test_the_water_meshes_measure_the_bank()
	_test_the_water_meshes_merge_the_deep_water()
	_test_a_brooks_current_is_its_tiles_own()
	_test_tree_images_follow_the_stands_that_hold_them()
	_test_tree_images_hold_their_finest_level_only_up_close()
	_test_tree_outlines_hold_every_texel()
	_test_the_frame_cache_keeps_what_left_the_view()
	_test_the_net_keeps_the_chunks_of_the_same_world_only()
	_test_a_body_told_at_each_chunk_keeps_walking_its_route()
	_test_a_new_route_told_while_walking_turns_from_where_the_body_is_shown()
	_test_the_clock_catches_up_with_the_ticks_walks_are_told_at()
	_test_a_blocked_body_is_decoded_and_its_notice_shows_then_fades()
	_test_a_refused_order_says_why_in_a_thought()
	_test_a_thread_keeps_its_last_twenty_and_shows_three_newest_last()
	_test_a_thread_scrolls_back_to_the_oldest_and_down_to_the_newest()
	_test_the_focus_has_older_above_and_newer_below_while_scrolling()
	_test_a_thread_folds_when_left_alone_and_unfolds()
	_test_a_thread_left_alone_longer_leaves_only_the_icon()
	_test_the_icon_reopens_on_the_newest_and_so_does_a_new_bubble()
	_test_a_placed_thought_is_clicked_from_the_history()
	_test_body_thinks_decodes_and_has_its_words()
	_test_the_recentre_button_shows_while_the_camera_does_not_follow()


## Once the main loop runs: the tests that wait for frames, then the end.
func _initialize() -> void:
	await process_frame
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
	_check(Protocol.encode(Protocol.drink(4, 6)) == "{\"type\":\"drink\",\"x\":4,\"y\":6}", "drink wire format matches server")


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
	_check(Tooltip.describe_item(berries, 2400).begins_with("Berries · 2.4 kg · edible"), "an item's own grams")
	_check(Tooltip.describe_item(berries, 40).begins_with("Berries · 40 g"), "a heap lighter than its piece")
	_check(Tooltip.describe_body(2, true, false) == "body 2 · asleep", "a sleeping body")
	_check(Tooltip.describe_body(1, false, true) == "body 1 (you)", "the piloted body")


func _test_inventory_lines_and_refusals() -> void:
	var flint := {"name": "Flint", "mass_g": 400}
	_check(Tooltip.inventory_line(flint, true) == "[in hand] Flint · 400 g", "the item in hand is marked")
	_check(Tooltip.inventory_line({"name": "Log", "mass_g": 25000}, false) == "Log · 25.0 kg", "kilograms")
	_check(Tooltip.inventory_line(flint, false, 50) == "Flint · 50 g", "the item's own grams, not its piece's")
	_check(Tooltip.inventory_line(flint, true, 1300) == "[in hand] Flint · 1.3 kg", "its own kilograms")
	_check(Tooltip.load_line(1900, 20000) == "1.9 / 20.0 kg", "the load over the capacity")
	_check(Tooltip.refusal_words("pick_up", "too_heavy") == "cannot pick up: too heavy", "a refusal in words")
	_check(Tooltip.refusal_words("eat", "not_food") == "cannot eat: not food", "another")


func _test_the_year_and_its_seasons() -> void:
	# Must agree with `a_tick_has_a_year_and_a_season` in the Rust sim: 50
	# days cut 13, 12, 13, 12.
	var cases := [[1, "spring", 1], [13, "spring", 1], [14, "summer", 1], [25, "summer", 1],
		[26, "autumn", 1], [38, "autumn", 1], [39, "winter", 1], [50, "winter", 1], [51, "spring", 2], [114, "summer", 3]]
	for case: Array in cases:
		var day: int = case[0]
		_check(GameClock.season_of(day, 50) == case[1] and GameClock.year_of(day, 50) == case[2], "day %d: %s of year %d" % case)
	_check(GameClock.day_of(0.0, 72000.0) == 1 and GameClock.day_of(71999.0, 72000.0) == 1 and GameClock.day_of(72000.0, 72000.0) == 2, "days count from 1")
	# The HUD's clock line: day, hour, day or night, season and year.
	_check(GameClock.clock_line(72000.0 * 13, 72000.0, 50, false) == "Day 14 · 06:00 · day · summer, year 1", "the season and year beside the day")
	_check(GameClock.clock_line(72000.0 * 50 + 36000.0, 72000.0, 50, true) == "Day 51 · 18:00 · night · spring, year 2", "a new year")
	_check(GameClock.season_line(2, "winter") == "winter begins · year 2", "a season begins")
	# The clock takes the year's length from the server.
	var clock := GameClock.new()
	clock.set_clock({"tick": 0, "ticks_per_second": 10, "ticks_per_day": 72000, "days_per_year": 12, "time_scale_permille": 1000}, 0)
	_check(clock.days_per_year == 12, "the year told by the server")


func _test_drink_is_offered_at_running_water_within_reach() -> void:
	var here := Vector2i(10, 10)
	for terrain: String in ["water", "ford", "brook"]:
		_check(TerrainGrid.can_drink(terrain, Vector2i(11, 9), here), "drink at %s beside the body" % terrain)
	_check(TerrainGrid.can_drink("brook", here, here), "drink where one stands in a brook")
	_check(not TerrainGrid.can_drink("water", Vector2i(12, 10), here), "not beyond reach")
	_check(not TerrainGrid.can_drink("grass", Vector2i(11, 10), here), "not on grass")
	_check(not TerrainGrid.can_drink("", Vector2i(11, 10), here), "not on an unknown tile")
	# The context menu offers it, through the intention the server judges.
	var world := FileAccess.get_file_as_string("res://world/world_view.gd")
	_check(world.contains("TerrainGrid.can_drink(terrain_at(tile), tile, piloted_tile())") and world.contains("\"label\": \"drink\"") and world.contains("Net.send_drink(tile.x, tile.y)"), "the menu's drink entry")
	_check(Tooltip.refusal_words("drink", "not_water") == "cannot drink: no running water there", "a refused drink in words")
	# The thirst gauge sits with the others.
	var scene := FileAccess.get_file_as_string("res://main.tscn")
	_check(scene.contains("[node name=\"Thirst\" type=\"ProgressBar\" parent=\"Ui/Hud/Rows/Gauges\"]"), "the thirst gauge is in the HUD")
	var hud := FileAccess.get_file_as_string("res://ui/hud.gd")
	_check(hud.contains("_thirst.value = float(condition.get(\"thirst\", 0))"), "the thirst gauge follows the condition")


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
	_check(ResourceLoader.exists("res://art/ground/ground_grass_01_summer.webp"), "the grass texture is importable")
	var uvs := ChunkView.ground_uvs(4, 8)
	_check(uvs[0] == Vector2(1.0, 2.0) and uvs[2] == Vector2(1.25, 2.25), "a tile maps to its square of the 4 m texture")
	_check(ResourceLoader.exists("res://art/ground/ground_grass_02_summer.webp"), "the worn grass is importable")
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
	_check(watched.has("res://world/ground.gdshader") and watched.has(ChunkView.GROUND_SPEC) and watched.has("res://art/ground/ground_grass_01_summer.webp"), "the watcher lists the shader, the spec and the ground textures")
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


## The water's line, one convolution of the tiles, gives at the lattice
## points what the three box blurs of the tiles drawn at PX pixels gave
## (the way the distance-field texture was computed, kept here as the
## reference).
func _test_the_water_meshes_blur_as_three_boxes() -> void:
	var size := 32
	var n := size + 2 * WaterMeshes.MARGIN
	var px := WaterMeshes.PX
	var w := n * px
	var noise := FastNoiseLite.new()
	noise.frequency = 0.05
	var terrain := func(t: Vector2i) -> String: return "water" if noise.get_noise_2d(t.x + WaterMeshes.MARGIN, t.y + WaterMeshes.MARGIN) < -0.05 else "grass"
	var lo := Vector2i.ZERO
	var prepared := WaterMeshes.prepare(WaterMeshes.block_of(terrain, lo, size), lo, size, Callable())
	var mask: PackedFloat32Array = prepared["mask"]
	var field := PackedFloat32Array()
	field.resize(w * w)
	for y in w:
		for x in w:
			field[y * w + x] = mask[floori(float(y) / px) * n + floori(float(x) / px)]
	for _pass in WaterMeshes.BLUR_PASSES:
		_box_rows(field, w, WaterMeshes.BLUR)
		_box_columns(field, w, WaterMeshes.BLUR)
	var out: Array = []
	WaterMeshes.build(prepared, out)
	var d: PackedFloat32Array = out[3]
	var l := px * size + 6
	var worst := 0.0
	for b in l:
		for a in l:
			# Lattice point a lies on the boundary between the pixels
			# (MARGIN * PX + a - 2) and the next: the mean of the two.
			var x := WaterMeshes.MARGIN * px + a - 2
			var y := WaterMeshes.MARGIN * px + b - 2
			var v := (field[y * w + x] + field[y * w + x + 1] + field[(y + 1) * w + x] + field[(y + 1) * w + x + 1]) / 4.0
			var expected := -WaterMeshes.sigma() * WaterMeshes.probit(clampf(v, 0.0005, 0.9995))
			worst = maxf(worst, absf(d[b * l + a] - expected))
	_check(worst < 1e-4, "the lattice's distances as the three blurs gave them (worst difference %f)" % worst)
	# A point asked for on its own reads the same field.
	var asked := 0.0
	for b in range(0, l, 7):
		for a in range(0, l, 5):
			var at := Vector2(lo) + Vector2(float(a) / px - 0.25, float(b) / px - 0.25)
			asked = maxf(asked, absf(WaterMeshes.distance_at(prepared, at) - d[b * l + a]))
	_check(asked < 1e-4, "a point asked for alone reads the lattice's value (worst difference %f)" % asked)


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
## bank, nothing under the water away from it (the meshes draw it).
func _test_the_ground_index_says_what_each_canvas_draws() -> void:
	var origin := Vector2i(320, 320)
	var tiles: Array = []
	for y in 32:
		for x in 32:
			tiles.append("forest" if x < 10 else ("water" if x >= 24 else "grass"))
	var map_size := WaterMeshes.map_size
	WaterMeshes.map_size = Vector2i(1024, 1024)
	var view := ChunkView.new()
	view.show_chunk(origin, 32, tiles, [], {})
	view._draw_ground_layers()
	WaterMeshes.map_size = map_size
	var at := func(x: int, y: int) -> PackedByteArray: return GroundIndex.at(origin + Vector2i(x, y))
	var grass := int(GroundIndex.BITS["grass"])
	var forest := int(GroundIndex.BITS["forest"])
	_check(at.call(5, 5)[0] == forest, "the wood is the forest's: %s" % at.call(5, 5))
	_check(at.call(10, 5)[0] == grass | GroundIndex.FRINGE and at.call(10, 5)[1] == 255, "the grass at the wood's edge, under its spill, opaque at the shared corner: %s" % at.call(10, 5))
	_check(at.call(11, 5)[1] == 170 and at.call(12, 5)[1] == 85 and at.call(13, 5)[1] == 0, "the spill fades over two tiles")
	_check(at.call(12, 5)[0] == grass | GroundIndex.FRINGE and at.call(13, 5)[0] == grass, "and stops after them")
	_check(at.call(22, 5)[0] == grass and at.call(23, 5)[0] == grass, "the land up to the water is its own: %s %s" % [at.call(22, 5), at.call(23, 5)])
	_check(at.call(24, 5)[0] == grass and at.call(25, 5)[0] == 0 and at.call(30, 5)[0] == 0, "the grass lies under the water at the bank only, the water meshes draw the rest: %s %s %s" % [at.call(24, 5), at.call(25, 5), at.call(30, 5)])
	var shown: Array = []
	for child in view.get_children():
		if child is ChunkView.GroundCanvas and (child as ChunkView.GroundCanvas).shown:
			shown.append((child as ChunkView.GroundCanvas).terrain)
	shown.sort()
	_check(shown == ["forest", "grass"], "one canvas a terrain drawn, none for the water: %s" % [shown])
	view.free()
	TerrainGrid.clear(origin, 32)


## Each ground canvas draws the rows of tiles it covers, not the whole
## chunk (B9): its rectangles cover exactly the tiles whose index bits
## meet its mask (the forest's own and its spill), each once, and a run
## going on from the row above extends its rectangle.
func _test_each_ground_canvas_covers_only_its_rows() -> void:
	var origin := Vector2i(352, 320)
	var tiles: Array = []
	for y in 32:
		for x in 32:
			var glade := x >= 3 and x < 6 and y >= 10 and y < 13
			tiles.append("water" if x >= 24 else ("forest" if x < 10 and not glade else "grass"))
	var map_size := WaterMeshes.map_size
	WaterMeshes.map_size = Vector2i(1024, 1024)
	var view := ChunkView.new()
	view.show_chunk(origin, 32, tiles, [], {})
	view._draw_ground_layers()
	WaterMeshes.map_size = map_size
	var rects := {}
	for child in view.get_children():
		if child is ChunkView.GroundCanvas and (child as ChunkView.GroundCanvas).shown:
			var canvas := child as ChunkView.GroundCanvas
			rects[canvas.terrain] = canvas.rows
			var mask := GroundBuild.canvas_mask(canvas.terrain)
			var covered := PackedByteArray()
			covered.resize(32 * 32)
			for r in range(0, canvas.rows.size(), 4):
				for y in range(canvas.rows[r + 1], canvas.rows[r + 3]):
					for x in range(canvas.rows[r], canvas.rows[r + 2]):
						covered[y * 32 + x] += 1
			var wrong := 0
			for y in 32:
				for x in 32:
					var own := int(GroundIndex.at(origin + Vector2i(x, y))[0]) & mask != 0
					if covered[y * 32 + x] != (1 if own else 0):
						wrong += 1
			_check(wrong == 0, "the %s canvas covers its own tiles, each once, and no other: %d wrong" % [canvas.terrain, wrong])
	_check(rects.keys().size() == 2 and rects.has("forest") and rects.has("grass"), "the forest and the grass are drawn, the water by its meshes: %s" % [rects.keys()])
	_check(rects.get("forest") == PackedInt32Array([0, 0, 13, 32]), "the wood and its spill over the glade and the meadow, one rectangle: %s" % [rects.get("forest")])
	_check(rects.get("grass") == PackedInt32Array([10, 0, 25, 32, 3, 10, 6, 13]), "the meadow to the bank, and the glade apart: %s" % [rects.get("grass")])
	_check(GroundBuild.rows_of(PackedByteArray([1, 0, 1, 1, 0, 0, 1, 1, 0]), 3, 1) == PackedInt32Array([0, 0, 1, 2, 2, 0, 3, 1, 0, 2, 2, 3]), "runs split where they change")
	view.free()
	TerrainGrid.clear(origin, 32)


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
	_check(text.contains("fps") and text.contains("12.5 ms") and text.contains("frame step") and text.contains("draw calls") and text.contains("wind on") and text.contains("fog rings (F7)") and text.contains("streaks on (F11)"), "the counters report: %s" % text)
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
	# A teleport onto the tile the body stands on: no chunk comes; the
	# screen lifts once the view holds what the screen shows, and quiet.
	var still := LoadingProgress.new()
	still.has_world = true
	still.start(3_000)
	_check(not still.done(9_000), "no chunk, the view not ready: loading")
	still.view_ready = true
	_check(not still.done(3_000 + LoadingProgress.QUIET_MSEC - 1), "the view ready, but chunks may still come: loading")
	_check(still.done(3_000 + LoadingProgress.QUIET_MSEC), "the view ready and quiet since the screen came up: done")
	still.fields = 1
	_check(not still.done(9_000), "a ground still on a worker: loading")
	still.fields = 0
	still.chunk_came(3_600)
	_check(not still.done(3_600 + LoadingProgress.QUIET_MSEC - 1) and still.done(3_600 + LoadingProgress.QUIET_MSEC) == (still.built >= 1), "a chunk came after all: waited for as before")


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
	_check(WaterMeshes.current_vector(0) == Vector2.ZERO, "no current on land")
	var south := WaterMeshes.current_vector(0 * 256 + 50)
	_check(south.distance_to(Vector2(0, 0.5)) < 1e-4, "heading 0 runs +y at 0.5 m/s: %s" % south)
	var east := WaterMeshes.current_vector(4 * 256 + 35)
	_check(east.distance_to(Vector2(0.35, 0)) < 1e-4, "heading 4 runs +x: %s" % east)


## The chunk frame the server's protocol writes (its test
## `the_golden_chunk_is_the_clients` keeps tests/fixtures/chunk_v17.bin):
## every field read back. A change of the layout on one side only breaks
## this test or the server's.
func _test_the_golden_chunk_decodes() -> void:
	var bytes := FileAccess.get_file_as_bytes("res://tests/fixtures/chunk_v17.bin")
	_check(bytes.size() == 87, "the golden chunk is there: %d bytes" % bytes.size())
	var c := Protocol.decode_chunk(bytes, PackedStringArray(["m0", "m1", "m2", "m3", "m4"]))
	_check(not c.is_empty(), "the golden chunk decodes")
	if c.is_empty():
		return
	_check([c["x"], c["y"], c["version"]] == [3, -2, 7], "its place and version: %s" % [[c["x"], c["y"], c["version"]]])
	_check(c["tiles"] == PackedByteArray([1, 2, 3, 4, 5, 6, 1, 3, 2, 2, 1, 1, 6, 6, 6, 5]), "its tiles: %s" % c["tiles"])
	_check(c["currents"] == PackedInt32Array([0, 0, 1059, 50, 3073, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4095]), "its currents: %s" % c["currents"])
	_check(c["trees"] == PackedInt32Array([2, 43_999, -8_000, 0, 13_500, -5_750]), "its trees: %s" % c["trees"])
	_check(c["plants"] == PackedInt32Array([11, 12_700, -4_100]), "its plants: %s" % c["plants"])
	var items: Array = c["items"]
	_check(items == [
		{"id": 281_474_976_710_698, "x": 15, "y": -7, "material": "", "mass_g": 70_123},
		{"id": 4_503_668_346_716_227, "x": 13, "y": -8, "material": "m4"},
		{"id": 4_503_668_346_717_120, "x": 15, "y": -5, "material": "m0"},
	], "its items: %s" % [items])
	_check(Protocol.decode_chunk(bytes.slice(0, 86), PackedStringArray()).is_empty(), "a frame cut short is refused")
	var dry := Protocol.decode_chunk(bytes, PackedStringArray())
	var stand := Trees.Stand.new(dry["trees"], {"oak": 2_200}, PackedStringArray(["oak", "pine", "birch"]))
	_check(Array(stand.kinds) == ["birch", "oak"] and stand.at[1] == Vector2(13_500, -5_750), "a stand reads the trees as they come")
	stand.release()


## Every tree image stands on its foot (no empty rows under it, so it is
## anchored where it stands) and holds no pale opaque speck of the
## background it was cut from (the birches, white of bark, aside).
func _test_tree_images_are_clean() -> void:
	var dir := DirAccess.open(Trees.DIR)
	if dir == null:
		_check(false, "the trees' folder is there")
		return
	for file in dir.get_files():
		if not file.ends_with("_summer.webp"):
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
		if not file.ends_with("_summer.webp"):
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
	var stand := Trees.Stand.new(trees, {"oak": 2_200, "pine": 3_000})
	# Its images read at once, as the game's frame would within a moment.
	Trees.poll(Time.get_ticks_msec(), true)
	return stand


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


func _test_tall_plants_are_drawn_only_where_the_view_reaches() -> void:
	var flora := [
		{"id": "bramble", "name": "Bramble", "family": "shrub", "height_cm": 90},
		{"id": "porcini", "name": "Porcini", "family": "mushroom", "height_cm": 15},
	]
	# A field of brambles and mushrooms, and a kind unknown, as a chunk
	# brings them (kind, x, y).
	var plants := PackedInt32Array()
	var kept: Array = []
	for j in 12:
		for i in 12:
			var k := 0 if (i + j) % 3 else 1
			var at := Vector2(1_100 + 2_300 * i, 900 + 2_100 * j)
			plants.append_array([k, int(at.x), int(at.y)])
			if k == 0:
				kept.append(at)
	plants.append_array([7, 5_000, 5_000])
	var stand := PlantStand.new(plants, flora)
	_check(stand.size() == kept.size() and stand.size() == 96, "only the tall plants of a known kind are kept, in the server's order: %d" % stand.size())
	var placed := true
	for i in stand.size():
		var at: Vector2 = kept[i]
		var region := ChunkView.plant_region(at, flora[0])
		placed = placed and stand.feet[i] == Iso.point_to_screen(at) and stand.region(i) == region and stand.rect(i) == ChunkView.plant_rect(at, flora[0], region)
	_check(placed, "each tall plant keeps its foot, its image and its rectangle, as its node had")
	var parent := RenderingServer.canvas_item_create()
	stand.parent = parent
	for seen: Rect2 in [Rect2(-300, 200, 640, 360), Rect2(-100, 400, 640, 360), Rect2(-900, -200, 2560, 1440), Rect2(5000, 5000, 100, 100), Rect2(-300, 200, 640, 360)]:
		var cells := Trees.Stand.cell_range(seen)
		stand.show_cells(cells)
		var exact := true
		for i in stand.size():
			var r := stand.rect(i)
			if r.intersects(seen) and not stand.items.has(i):
				exact = false
			if stand.items.has(i) != Trees.Stand.cell_range(r).intersects(cells):
				exact = false
		_check(exact, "the tall plants drawn are those whose image meets the view's cells: %s" % seen)
	_check(not stand.items.is_empty() and stand.items.size() < stand.size(), "a view on part of the field draws part of it: %d" % stand.items.size())
	var pooled := Trees._pool.size()
	var held := stand.items.size()
	stand.show_cells(Rect2i())
	_check(stand.items.is_empty() and Trees._pool.size() == pooled + held, "out of view, the tall plants give their canvas items back")
	stand.show_cells(Trees.Stand.cell_range(Rect2(-300, 200, 640, 360)))
	stand.release()
	_check(stand.items.is_empty() and stand.refs.count(0) == stand.size() and stand.shown == Rect2i(), "released, nothing is held")
	_check(PlantStand.new(PackedInt32Array([1, 100, 100]), flora).size() == 0, "a chunk of mushrooms has no tall plant")
	RenderingServer.free_rid(parent)


func _test_tree_shadows_are_sixteen_sided() -> void:
	# A shadow's rim: sixteen vertices round the ellipse, the polygon's area
	# the ellipse's within 2 %.
	for size: Vector2 in [Vector2(200, 97), Vector2(58, 29), Vector2(10, 10)]:
		var centre := Vector2(31, -12)
		var rim := Trees.shadow_polygon(centre, size)
		var area := 0.0
		for j in rim.size():
			area += rim[j].cross(rim[(j + 1) % rim.size()])
		area = absf(area) / 2.0
		var ellipse := PI * size.x * size.y / 4.0
		_check(rim.size() == 16 and absf(area / ellipse - 1.0) < 0.02, "a shadow is 16-sided, its area the ellipse's: %d vertices, %.4f" % [rim.size(), area / ellipse])
	# Two shadows a tree, each its middle, its knee ring and its rim, in one
	# triangle array; opaque in the middle, clear at the rim.
	var rects: Array[Rect2] = [Rect2(-50, -300, 100, 300), Rect2(400, -100, 60, 180)]
	var mesh := Trees.shadow_mesh(rects, PackedVector2Array([Vector2(0, 0), Vector2(430, 80)]))
	var points: PackedVector2Array = mesh[0]
	var colours: PackedColorArray = mesh[1]
	var indices: PackedInt32Array = mesh[2]
	_check(points.size() == 4 * 33 and colours.size() == points.size() and indices.size() == 4 * 48 * 3, "two shadows a tree, 33 vertices and 48 triangles each")
	_check(is_equal_approx(colours[0].a, Trees.CROWN_SHADE) and is_equal_approx(colours[33].a, Trees.FOOT_SHADE) and colours[32].a == 0.0, "dark in the middle, clear at the rim")


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


## A slow drag trembled: the picture is laid on whole screen pixels, so
## every layer rounds the same way as the view moves.
func _test_a_drag_is_smoothed_and_the_view_laid_on_whole_pixels() -> void:
	for at: Vector2 in [Vector2(10.3, -4.71), Vector2(1234.567, 89.01), Vector2(-0.49, 0.51)]:
		for scale: float in [0.4 * 1.92, 1.0, 2.5 * 1.92]:
			var shown := (at + ViewLedger.snapped_offset(at, scale)) * scale
			_check(shown.is_equal_approx(shown.round()), "laid on a whole pixel: %s at %s" % [at, scale])
			_check(ViewLedger.snapped_offset(at, scale).length() * scale <= 0.71, "never more than half a pixel off")


## A chunk's ground is worked out on a worker (GroundBuild) and laid on
## the main thread the same, bit for bit, as built in line; the grounds
## done are laid nearest the camera first, as many a frame as fit the
## budget but at least one; a chunk freed with its ground under way lets
## the task finish on its own (C3).
func _test_grounds_built_on_workers_are_laid_nearest_first_within_the_budget() -> void:
	var was: Array = ChunkView.waiting
	ChunkView.waiting = []
	var map_size := WaterMeshes.map_size
	WaterMeshes.map_size = Vector2i(1024, 1024)
	var tiles: Array = []
	for y in 32:
		for x in 32:
			tiles.append("forest" if x < 10 else ("water" if x >= 24 else "grass"))
	# In line, then on a worker, the same chunk: the same ground.
	var origin := Vector2i(320, 320)
	var inline := ChunkView.new()
	inline.show_chunk(origin, 32, tiles, [], {})
	inline._draw_ground_layers()
	var index_inline: Array = []
	for y in 33:
		for x in 33:
			index_inline.append(GroundIndex.at(origin + Vector2i(x, y)))
	var worked := ChunkView.new()
	worked.show_chunk(origin, 32, tiles, [], {})
	GroundIndex.write(origin, 32, PackedByteArray(range(32 * 32).map(func(_i: int) -> int: return 0)), PackedByteArray(range(33 * 33).map(func(_i: int) -> int: return 0)))
	worked.start_build()
	_check(ChunkView.waiting.has(worked) and not worked.is_built(), "started: waiting, not built yet")
	while not worked.ground_ready():
		OS.delay_msec(1)
	_check(ChunkView.lay_ready(0, Iso.tile_to_screen(336, 336)) == 1 and worked.is_built() and not ChunkView.waiting.has(worked), "done: laid, no longer waiting")
	var index_worked: Array = []
	for y in 33:
		for x in 33:
			index_worked.append(GroundIndex.at(origin + Vector2i(x, y)))
	_check(index_worked == index_inline, "the ground index the same as built in line")
	_check(worked._flats.size() == inline._flats.size() and worked._decor_cache.size() == inline._decor_cache.size(), "the same flat tiles and decorations")
	var meshes := func(view: ChunkView) -> Array:
		var out: Array = []
		for canvas: ChunkView.WaterCanvas in view._water_canvases():
			out.append(canvas.mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX] if canvas.mesh != null else null)
		return out
	_check(meshes.call(worked) == meshes.call(inline) and meshes.call(worked)[0] != null, "the same water meshes")
	worked.start_build()
	_check(not ChunkView.waiting.has(worked), "laid and nothing changed around: nothing more to build")
	inline.free()
	worked.free()
	# Four chunks done at once: the nearest the camera laid first, one
	# when the budget is spent at once, then the rest.
	var views: Array = []
	for at: Vector2i in [Vector2i(160, 96), Vector2i(96, 128), Vector2i(64, 64), Vector2i(128, 96)]:
		var view := ChunkView.new()
		view.show_chunk(at, 32, tiles, [], {})
		view.start_build()
		views.append(view)
	_check(ChunkView.waiting.size() == 4, "four grounds on the workers")
	while not views.all(func(v: ChunkView) -> bool: return v.ground_ready()):
		OS.delay_msec(1)
	var camera := Iso.tile_to_screen(100, 140)
	_check(ChunkView.lay_ready(0, camera) == 1 and (views[1] as ChunkView).is_built() and ChunkView.waiting.size() == 3, "no budget: one laid, the nearest")
	_check(ChunkView.lay_ready(1_000_000, camera) == 3 and views.all(func(v: ChunkView) -> bool: return v.is_built()) and ChunkView.waiting.is_empty(), "budget enough: the rest laid")
	_check(ChunkView.lay_ready(0, camera) == 0, "nothing done: nothing laid")
	# Freed with its ground under way: the task finishes on its own.
	var gone := ChunkView.new()
	gone.show_chunk(Vector2i(192, 192), 32, tiles, [], {})
	gone.start_build()
	gone.free()
	_check(ChunkView.waiting.is_empty() and ChunkView._orphans.size() == 1, "freed: no longer waiting, its task left to finish")
	var waited := 0
	while not ChunkView._orphans.is_empty() and waited < 5000:
		ChunkView.lay_ready(0, camera)
		OS.delay_msec(1)
		waited += 1
	_check(ChunkView._orphans.is_empty(), "and forgotten once done")
	for view: ChunkView in views:
		view.free()
	for at: Vector2i in [origin, Vector2i(160, 96), Vector2i(96, 128), Vector2i(64, 64), Vector2i(128, 96), Vector2i(192, 192)]:
		TerrainGrid.clear(at, 32)
	WaterMeshes.map_size = map_size
	ChunkView.waiting = was


## The camera's reach is the screen and a margin of chunks that grows as
## the zoom pulls back (the grounds are laid before the screen gets
## there); a new margin moves the reach even if the screen stayed (C3).
func _test_the_reach_s_margin_grows_as_the_zoom_pulls_back() -> void:
	_check(ViewLedger.margin_for(2.5) == 1 and ViewLedger.margin_for(1.5) == 1, "close up: a chunk")
	_check(ViewLedger.margin_for(1.0) == 2 and ViewLedger.margin_for(0.7) == 2, "the middle zooms: two")
	_check(ViewLedger.margin_for(0.4) == 3 and ViewLedger.margin_for(0.25) == 3, "pulled back: three")
	var ledger := ViewLedger.new()
	ledger.chunk_size = 32
	var bounds := Rect2i(0, 0, 40, 40)
	var screen := Rect2(Iso.tile_to_screen(640, 640) - Vector2(960, 540), Vector2(1920, 1080))
	_check(ledger.see(screen, 1), "seen")
	var one := ledger.chunks_touching(ledger.reach, bounds).size()
	_check(not ledger.see(screen, 1) and ledger.see(screen, 3), "the same screen with a wider margin: moved")
	var three := ledger.chunks_touching(ledger.reach, bounds).size()
	_check(three > one and ledger.reach.encloses(ledger.shown), "a wider margin reaches more chunks: %d -> %d" % [one, three])
	var keys: Array[Vector2i] = [Vector2i(25, 25), Vector2i(20, 20), Vector2i(22, 20)]
	_check(ViewLedger.nearest_first(keys, ledger, screen.get_center()) == [Vector2i(20, 20), Vector2i(22, 20), Vector2i(25, 25)], "the margin built nearest the screen's middle first")


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


## The chunks the camera's reach touches are found exactly, row by row:
## the same as asking each chunk (ViewLedger.touches), and the same as
## cutting the chunk's diamond of ground with the rectangle.
func _test_the_chunks_the_camera_reaches_are_found_exactly() -> void:
	var ledger := ViewLedger.new()
	ledger.chunk_size = 8
	var bounds := Rect2i(2, 2, 12, 12)
	var rng := RandomNumberGenerator.new()
	rng.seed = 11
	var agreed := 0
	var touched := 0
	for trial in 40:
		var centre := Iso.tile_to_screen(rng.randi_range(16, 112), rng.randi_range(16, 112)) + Vector2(rng.randf_range(-40, 40), rng.randf_range(-40, 40))
		var size := Vector2(rng.randf_range(10, 900), rng.randf_range(10, 500))
		var rect := Rect2(centre - size / 2.0, size)
		var found := ledger.chunks_touching(rect, bounds)
		var each: Array[Vector2i] = []
		for y in range(bounds.position.y, bounds.end.y):
			for x in range(bounds.position.x, bounds.end.x):
				var key := Vector2i(x, y)
				# The diamond of the chunk's ground: its tiles' diamonds start
				# half a tile before their centre (ChunkView.GroundCanvas).
				var lo := key * ledger.chunk_size
				var hi := lo + Vector2i(ledger.chunk_size, ledger.chunk_size)
				var up := Vector2(0, -Iso.TILE_H / 2.0)
				var diamond := PackedVector2Array([Iso.tile_to_screen(lo.x, lo.y) + up, Iso.tile_to_screen(hi.x, lo.y) + up, Iso.tile_to_screen(hi.x, hi.y) + up, Iso.tile_to_screen(lo.x, hi.y) + up])
				var corners := PackedVector2Array([rect.position, Vector2(rect.end.x, rect.position.y), rect.end, Vector2(rect.position.x, rect.end.y)])
				var area := 0.0
				for part: PackedVector2Array in Geometry2D.intersect_polygons(diamond, corners):
					area += _polygon_area(part)
				var cut := area > 1e-3
				if ledger.touches(rect, key) == cut:
					agreed += 1
				if ledger.touches(rect, key):
					each.append(key)
		touched += each.size()
		_check(found == each, "trial %d: the rows give what each chunk says: %s vs %s" % [trial, found, each])
	_check(agreed == 40 * 144, "the chunk touches the rectangle exactly when its diamond cuts it: %d of %d agree" % [agreed, 40 * 144])
	_check(touched > 40, "the trials touched chunks: %d" % touched)
	_check(ledger.chunks_touching(Rect2(), bounds).is_empty() and ledger.chunks_touching(Rect2(0, 0, 10, 10), Rect2i()).is_empty(), "no rectangle or no bounds: nothing")
	_check(ledger.chunks_touching(Rect2(-100000, -100000, 10, 10), bounds).is_empty(), "a rectangle far outside the bounds: nothing")


static func _polygon_area(points: PackedVector2Array) -> float:
	var twice := 0.0
	for i in points.size():
		var a := points[i]
		var b := points[(i + 1) % points.size()]
		twice += a.x * b.y - b.x * a.y
	return absf(twice) / 2.0


## A chunk held is built when the camera's reach touches it, at once
## when the screen may show it (its ground, or what stands on it), paced
## in the margin around; released after the grace once the reach left
## it; and the reach coming back within the grace builds nothing twice,
## so the camera jittering on an edge costs nothing (C1, C5).
func _test_a_chunk_is_built_when_the_reach_touches_it_and_released_after_the_grace() -> void:
	var ledger := ViewLedger.new()
	ledger.chunk_size = 32
	var held := {}
	for y in 10:
		for x in 10:
			held[Vector2i(x, y)] = true
			ledger.hold(Vector2i(x, y))
	var centre := Iso.tile_to_screen(5 * 32 + 16, 5 * 32 + 16)
	var screen := Rect2(centre - Vector2(10, 10), Vector2(20, 20))
	_check(ledger.plan(0, held)["at_once"].is_empty(), "nothing seen yet: nothing to build")
	_check(ledger.see(screen) and not ledger.see(screen), "the screen moved, then stayed")
	var plan: Dictionary = ledger.plan(1000, held)
	var at_once: Array = plan["at_once"]
	var paced: Array = plan["paced"]
	_check(at_once.has(Vector2i(5, 5)) and at_once.has(Vector2i(4, 4)), "the chunk under the screen and the one above, whose trees may reach it: at once")
	_check(not at_once.has(Vector2i(6, 6)) and paced.has(Vector2i(6, 6)), "the chunk below, a chunk away: in the margin, paced")
	_check(not at_once.has(Vector2i(9, 9)) and not paced.has(Vector2i(9, 9)), "a chunk far off: not built")
	for key: Vector2i in at_once:
		_check(ledger.touches(ledger.shown, key) and ledger.within_reach(key), "at once: the screen may show it")
	for key: Vector2i in paced:
		_check(not ledger.touches(ledger.shown, key) and ledger.within_reach(key), "paced: in the reach, past what the screen may show")
	_check(plan["release"].is_empty() and ledger.leaving.is_empty(), "nothing built yet: nothing leaving")
	var built: Array = at_once + paced
	for key: Vector2i in built:
		ledger.mark_built(key)
	plan = ledger.plan(1100, held)
	_check(plan["at_once"].is_empty() and plan["paced"].is_empty() and plan["release"].is_empty() and ledger.leaving.is_empty(), "built: nothing more to do")
	ledger.mark_released(Vector2i(4, 4))
	_check(ledger.plan(1100, held)["at_once"] == [Vector2i(4, 4)], "a chunk released while in reach is asked for again")
	ledger.mark_built(Vector2i(4, 4))
	# The camera goes far away: the built chunks start their grace.
	var away := Rect2(Iso.tile_to_screen(16, 16) - Vector2(10, 10), Vector2(20, 20))
	ledger.see(away)
	plan = ledger.plan(2000, held)
	_check(plan["release"].is_empty() and ledger.leaving.size() == built.size() and int(ledger.leaving[Vector2i(5, 5)]) == 2000, "away: every built chunk leaving, none released yet")
	_check(plan["at_once"].has(Vector2i(0, 0)), "the chunk under the camera there: at once")
	# Back within the grace: nothing to build, the grace cancelled.
	ledger.see(screen)
	plan = ledger.plan(2500, held)
	_check(plan["at_once"].is_empty() and plan["paced"].is_empty() and plan["release"].is_empty(), "back within the grace: nothing built twice")
	_check(not ledger.leaving.has(Vector2i(5, 5)) and not ledger.leaving.has(Vector2i(6, 6)), "the chunks the reach came back to keep their nodes")
	# Away for good: released once the grace is over, not before.
	ledger.see(away)
	ledger.plan(3000, held)
	_check(ledger.plan(3000 + ViewLedger.RELEASE_GRACE_MSEC - 1, held)["release"].is_empty(), "a moment before the grace is over: kept")
	var release: Array = ledger.plan(3000 + ViewLedger.RELEASE_GRACE_MSEC, held)["release"]
	_check(release.size() == built.size() and release.has(Vector2i(5, 5)), "the grace over: every chunk the reach left is released")
	for key: Vector2i in release:
		ledger.mark_released(key)
	_check(ledger.leaving.is_empty() and not ledger.built.has(Vector2i(5, 5)), "released: no longer built nor leaving")
	# Only what is held is built: a chunk the server never sent is skipped.
	held.erase(Vector2i(0, 0))
	_check(not ledger.plan(6000, held)["at_once"].has(Vector2i(0, 0)), "a chunk not held is never built")


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
	var held := {}
	for y in range(3, 8):
		for x in range(3, 8):
			var chunk := ChunkView.new()
			chunk._last_signature = 1
			chunks[Vector2i(x, y)] = chunk
			held[Vector2i(x, y)] = true
	var bounds := Rect2i(3, 3, 5, 5)
	var limit := Rect2(Vector2(140.5, 130.25), Vector2(80, 90))
	# The chunks the screen shows: three under the limit, one outside it.
	var shown: Array[Vector2i] = [Vector2i(4, 4), Vector2i(5, 5), Vector2i(6, 6), Vector2i(3, 3)]
	_check(FarView.limit_built(limit, chunks, held, bounds, 32, shown), "all built under the limit: the hole may be left")
	chunks[Vector2i(6, 6)]._last_signature = 0
	_check(not FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk shown under the limit not built: no hole")
	chunks[Vector2i(6, 6)].build_at_once = true
	_check(FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk shown under the limit that builds this frame: the hole may be left")
	chunks[Vector2i(6, 6)].build_at_once = false
	var unbuilt: ChunkView = chunks[Vector2i(6, 6)]
	chunks.erase(Vector2i(6, 6))
	_check(not FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk shown under the limit with no view (C1, data only): no hole")
	chunks[Vector2i(6, 6)] = unbuilt
	unbuilt._last_signature = 1
	chunks[Vector2i(3, 3)]._last_signature = 0
	_check(FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk not built outside the limit: the hole may be left")
	var unseen: ChunkView = chunks[Vector2i(5, 4)]
	chunks.erase(Vector2i(5, 4))
	_check(FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk under the limit the screen does not show, not built (C1): the hole may be left")
	chunks[Vector2i(5, 4)] = unseen
	_check(not FarView.limit_built(Rect2(Vector2(96, 96), Vector2(100, 100)), chunks, held, bounds, 32, shown), "the limit touching that chunk's last tile: no hole")
	held.erase(Vector2i(7, 3))
	_check(not FarView.limit_built(limit, chunks, held, bounds, 32, shown), "a chunk missing from the bounds held: no hole")
	_check(not FarView.limit_built(limit, chunks, {}, Rect2i(), 32, shown), "nothing held: no hole")
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
	var prepared := WaterMeshes.prepare(WaterMeshes.block_of(river, lo, size), lo, size, runs)
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
		var uvs: PackedVector2Array = mesh["uvs"]
		var ok := (uvs.is_empty() if mesh == water else points.size() == uvs.size()) and points.size() == (mesh["colours"] as PackedColorArray).size()
		var indices: PackedInt32Array = mesh["indices"]
		var strip: PackedInt32Array = mesh["strip"]
		ok = ok and indices.size() % 3 == 0 and strip.size() >= 4
		for i in indices:
			if i < 0 or i >= points.size():
				ok = false
		for i in strip:
			if i < 0 or i >= points.size():
				ok = false
		_check(ok, "the mesh's arrays are whole (%d points, %d strip indices, %d indices)" % [points.size(), strip.size(), indices.size()])
	# The water mesh carries the current: the colour's green is the y
	# component, 0.5 m/s over 2 either way.
	var colours: PackedColorArray = water["colours"]
	var points: PackedVector2Array = water["points"]
	var middle := points.find(WaterMeshes.to_screen(Vector2(43.0, 16.0)))
	_check(middle >= 0 and absf(colours[middle].g - 0.625) < 0.01 and absf(colours[middle].r - 0.5) < 0.01, "the vertices carry the current: %s" % (colours[middle] if middle >= 0 else Color.BLACK))
	_check(middle >= 0 and absf(colours[middle].b - clampf(3.0 / WaterMeshes.DEPTH_TILES, 0.0, 1.0)) < 0.1, "and the depth, three tiles from either bank: %s" % (colours[middle] if middle >= 0 else Color.BLACK))
	# A block without water prepares to nothing.
	var land := func(_t: Vector2i) -> String: return "grass"
	_check(WaterMeshes.prepare(WaterMeshes.block_of(land, lo, size), lo, size, runs).is_empty(), "a dry block has no meshes")


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
		WaterMeshes.build(WaterMeshes.prepare(WaterMeshes.block_of(river, lo, 32), lo, 32, runs), out)
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
			var auv: PackedVector2Array = a["uvs"]
			var buv: PackedVector2Array = b["uvs"]
			if (not auv.is_empty() and auv[i] != buv[j]) or (a["colours"] as PackedColorArray)[i] != (b["colours"] as PackedColorArray)[j]:
				differ += 1
	_check(shared >= 20 and differ == 0, "the chunks share %d vertices on their border, %d differ" % [shared, differ])


## The bank mesh carries the distance to the water's line as the field
## measured it: zero on the line, the tiles inland, a step smoothed.
func _test_the_water_meshes_measure_the_bank() -> void:
	var stepped := func(t: Vector2i) -> String: return "water" if t.x >= (40 if t.y < 16 else 41) else "grass"
	var lo := Vector2i(32, 0)
	var out: Array = []
	WaterMeshes.build(WaterMeshes.prepare(WaterMeshes.block_of(stepped, lo, 32), lo, 32, Callable()), out)
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
	_check(absf(WaterMeshes.probit(0.975) - 1.96) < 0.01, "the inverse normal curve: %f" % WaterMeshes.probit(0.975))
	# A ford is water for the bank, and the water mesh carries it.
	var forded := func(t: Vector2i) -> String: return "ford" if t.x >= 40 and t.x < 60 and t.y >= 8 and t.y < 24 else ("water" if t.x >= 40 and t.x < 60 else "grass")
	var fout: Array = []
	WaterMeshes.build(WaterMeshes.prepare(WaterMeshes.block_of(forded, lo, 32), lo, 32, Callable()), fout)
	var fd: PackedFloat32Array = fout[3]
	_check(fd[int(round((16.0 + 0.25) * WaterMeshes.PX)) * l + int(round((50.0 - lo.x + 0.25) * WaterMeshes.PX))] < -1.0, "a ford is water for the bank")
	var fwater: Dictionary = fout[0]
	var fpoints: PackedVector2Array = fwater["points"]
	var fcolours: PackedColorArray = fwater["colours"]
	var inside := fpoints.find(WaterMeshes.to_screen(Vector2(50.0, 16.0)))
	var away := fpoints.find(WaterMeshes.to_screen(Vector2(50.0, 30.0)))
	_check(inside >= 0 and fcolours[inside].a > 0.7, "inside the ford the vertices carry it: %s" % (fcolours[inside] if inside >= 0 else Color.BLACK))
	_check(away >= 0 and fcolours[away].a < 0.05, "deep water away from the ford carries none: %s" % (fcolours[away] if away >= 0 else Color.BLACK))


## The lattice point a vertex of a water mesh stands at (the inverse of
## WaterMeshes.to_screen, for the chunk at `lo`).
func _lattice_of(point: Vector2, lo: Vector2i) -> Vector2i:
	var wx := point.x / Iso.TILE_W + (point.y + Iso.TILE_H / 2.0) / Iso.TILE_H
	var wy := (point.y + Iso.TILE_H / 2.0) / Iso.TILE_H - point.x / Iso.TILE_W
	return Vector2i(roundi((wx - lo.x + 0.25) * WaterMeshes.PX), roundi((wy - lo.y + 0.25) * WaterMeshes.PX))


## The deep water is merged into half-tile quads where nothing varies,
## without a T-junction: a merged quad's edge beside a finer quad keeps
## the vertex in its middle; the chunk's own edge keeps every lattice
## point, so the neighbouring chunk joins it; the bank stays fine.
func _test_the_water_meshes_merge_the_deep_water() -> void:
	# A river twenty tiles wide, x in [40, 60), crossing the chunk (32, 0),
	# running south.
	var river := func(t: Vector2i) -> String: return "water" if t.x >= 40 and t.x < 60 else "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.0, 0.5)
	var lo := Vector2i(32, 0)
	var out: Array = []
	WaterMeshes.build(WaterMeshes.prepare(WaterMeshes.block_of(river, lo, 32), lo, 32, runs), out)
	var classes: PackedByteArray = out[2]
	var deep := 0
	for k in classes:
		if k == 1:
			deep += 1
	var water: Dictionary = out[0]
	var points: PackedVector2Array = water["points"]
	var indices: PackedInt32Array = water["indices"]
	var strip: PackedInt32Array = water["strip"]
	# A strip draws a triangle an index past its first two (the joins'
	# flat ones counted too): with the fans, well under the unmerged count.
	var triangles := indices.size() / 3 + strip.size() - 2
	_check(deep > 300 and triangles < deep * 32 / 2, "the deep water is merged: %d triangles for %d deep cells (%d unmerged)" % [triangles, deep, deep * 32])
	_check(int(water["bytes"]) == points.size() * WaterMeshes.WATER_VERTEX_BYTES + (indices.size() + strip.size()) * WaterMeshes.INDEX_BYTES, "the bytes counted: %d" % int(water["bytes"]))
	_check(int(water["bytes_before"]) > 3 * int(water["bytes"]), "well under what the same cells weighed before: %d of %d" % [int(water["bytes"]), int(water["bytes_before"])])
	var lattice: Dictionary = {}
	for i in points.size():
		lattice[_lattice_of(points[i], lo)] = i
	var t_junctions := 0
	var long_edges := 0
	for k in range(0, indices.size(), 3):
		for e in 3:
			var a: Vector2i = _lattice_of(points[indices[k + e]], lo)
			var b: Vector2i = _lattice_of(points[indices[k + (e + 1) % 3]], lo)
			var step := b - a
			if (absi(step.x) == 2 and step.y == 0) or (absi(step.y) == 2 and step.x == 0):
				long_edges += 1
				if lattice.has(a + step / 2):
					t_junctions += 1
	_check(long_edges > 0 and t_junctions == 0, "no merged edge passes a finer quad's vertex: %d of %d" % [t_junctions, long_edges])
	# The chunk's edge keeps every lattice point of the deep rows.
	var on_edge: Array = []
	for at: Vector2i in lattice:
		if at.y == 3:
			on_edge.append(at.x)
	on_edge.sort()
	var gaps := 0
	for i in range(1, on_edge.size()):
		if on_edge[i] - on_edge[i - 1] != 1:
			gaps += 1
	_check(on_edge.size() > 60 and gaps == 0, "the chunk's edge keeps its fine vertices: %d points, %d gaps" % [on_edge.size(), gaps])
	# The bank mesh stays fine: all of it one strip of quarter quads, no
	# step of the strip longer than a lattice step but at the joins
	# between runs (a repeated index, then the next run's first).
	var bank: Dictionary = out[1]
	var bpoints: PackedVector2Array = bank["points"]
	var bstrip: PackedInt32Array = bank["strip"]
	var long_bank := 0
	for k in range(1, bstrip.size()):
		if bstrip[k] == bstrip[k - 1] or (k >= 2 and bstrip[k - 1] == bstrip[k - 2]):
			continue
		var step: Vector2i = _lattice_of(bpoints[bstrip[k]], lo) - _lattice_of(bpoints[bstrip[k - 1]], lo)
		if absi(step.x) > 1 or absi(step.y) > 1:
			long_bank += 1
	_check((bank["indices"] as PackedInt32Array).is_empty() and bstrip.size() > 100 and long_bank == 0, "the bank mesh stays at a quarter tile: %d long steps in a strip of %d" % [long_bank, bstrip.size()])
	# Where the current twists within a tile (the four tiles around a
	# point running two ways), the quads stay fine: fewer cells merge. (A
	# current that only shears is linear between tile centres and merges
	# as a still one does.)
	var swings := func(t: Vector2i) -> Vector2: return Vector2(1.0 if (t.x < 50) == (t.y < 16) else -1.0, 0.5)
	var sout: Array = []
	WaterMeshes.build(WaterMeshes.prepare(WaterMeshes.block_of(river, lo, 32), lo, 32, swings), sout)
	var stwisted: int = (sout[0]["strip"] as PackedInt32Array).size()
	_check(stwisted > strip.size(), "a varying current keeps more quads: %d > %d strip indices" % [stwisted, strip.size()])


## A brook's water runs as its own tiles say: each vertex of its ribbon
## carries the current of the tile under it, never an average over the
## tiles around, so the legs of a tight meander, two tiles apart and
## running opposite ways, each keep their own way; at the mouth the
## ribbon fades out where the river's water begins.
func _test_a_brooks_current_is_its_tiles_own() -> void:
	# Grass, a river east of x = 30, and a brook from a spring at (5, 10)
	# east, back west two rows down, then east again to the river: three
	# legs with one tile of grass between them.
	var path: Array[Vector2i] = []
	for x in range(5, 13):
		path.append(Vector2i(x, 10))
	path.append(Vector2i(12, 11))
	for x in range(12, 5, -1):
		path.append(Vector2i(x, 12))
	path.append(Vector2i(6, 13))
	for x in range(6, 30):
		path.append(Vector2i(x, 14))
	var brook: Dictionary = {}
	var current: Dictionary = {}
	for k in path.size():
		brook[path[k]] = true
		var next: Vector2i = path[k + 1] if k + 1 < path.size() else path[k] + Vector2i(1, 0)
		current[path[k]] = Vector2(next - path[k]).normalized() * 0.3
	var terrain := func(t: Vector2i) -> String:
		if brook.has(t):
			return "brook"
		return "water" if t.x >= 30 else "grass"
	var current_at := func(t: Vector2i) -> Vector2: return current.get(t, Vector2.ZERO)
	var lo := Vector2i.ZERO
	var size := 32
	var prepared := WaterMeshes.prepare(WaterMeshes.block_of(terrain, lo, size), lo, size, current_at)
	var to_river := func(p: Vector2) -> float: return WaterMeshes.distance_at(prepared, p)
	var lines := BrookLines.lay(terrain, lo - Vector2i(BrookLines.MARGIN, BrookLines.MARGIN), lo + Vector2i(size + BrookLines.MARGIN, size + BrookLines.MARGIN))
	_check(lines.size() == 1 and (lines[0]["ends"] as Array).has("spring") and (lines[0]["ends"] as Array).has("mouth"), "one brook from its spring to its mouth: %s" % [lines.map(func(l: Dictionary) -> Array: return l["ends"])])
	var line: Dictionary = lines[0]
	var pts: PackedVector2Array = line["pts"]
	var earth: PackedFloat32Array = line["earth"]
	# The line's course from the spring to the mouth, whichever way its
	# points are ordered.
	var sign := 1.0 if (line["ends"] as Array)[0] == "spring" else -1.0
	var mesh := BrookLines.mesh([line], lo, lo + Vector2i(size, size), current_at, to_river)
	var points: PackedVector2Array = mesh["points"]
	var colours: PackedColorArray = mesh["colours"]
	var uvs: PackedVector2Array = mesh["uvs"]
	var indices: PackedInt32Array = mesh["indices"]
	# (A band folded at a tight corner is not drawn, as before: this
	# meander turns on a tile.)
	_check(points.size() >= 4 * (pts.size() - 1) / 2 and points.size() % 4 == 0 and indices.size() == points.size() / 4 * 6, "a band of four vertices a segment: %d vertices, %d indices for %d points" % [points.size(), indices.size(), pts.size()])
	_check(uvs.size() == points.size() and colours.size() == points.size(), "every vertex carries its lateral place and its colour")
	# Across each band, the two edges lie on opposite sides of the axis
	# (the shader draws from the distance to it) and carry the same
	# half-width.
	var one_sided := 0
	for k in range(0, points.size(), 4):
		if uvs[k].x <= 0.0 or uvs[k + 1].x <= 0.0 or uvs[k + 2].x >= 0.0 or uvs[k + 3].x >= 0.0 or uvs[k].y != uvs[k + 3].y or uvs[k].y < 0.0:
			one_sided += 1
	_check(one_sided == 0, "%d bands do not straddle their brook's axis" % one_sided)
	# Every vertex's flow runs along the brook's course, at its tile's own
	# pace: the vertices of segment i belong to points i and i + 1.
	var against := 0
	var slowed := 0
	var at := 0
	var segment := 0
	var flows := BrookLines.flows_of(line, current_at)
	while at + 3 < points.size():
		# Find the segment this band belongs to: its first vertex lies at
		# point i's left edge.
		while segment < pts.size() - 1 and BrookLines.to_screen(pts[segment] + BrookLines._normal(pts, segment) * ((line["widths"] as PackedFloat32Array)[segment] / 2.0 + BrookLines.SHADER_MARGIN)) != points[at]:
			segment += 1
		var along := (pts[segment + 1] - pts[segment]).normalized() * sign
		for k in 4:
			var c := colours[at + k]
			var flow := Vector2(c.r * 2.0 - 1.0, c.g * 2.0 - 1.0) * WaterMeshes.FLOW_MAX
			if flow.dot(along) <= 0.0:
				against += 1
			if absf(flow.length() - 0.3) > 2.0 * WaterMeshes.FLOW_MAX / 255.0:
				slowed += 1
		at += 4
	_check(segment < pts.size() - 1, "every band found its segment")
	_check(against == 0, "%d vertices run against the brook's course" % against)
	_check(slowed == 0, "%d vertices run at another pace than their tile's 0.3 m/s" % slowed)
	# The tiles' own currents, not a mean: at the middle of the west leg
	# the point's flow is west, at the legs beside it east.
	var west := 0
	var east := 0
	for i in pts.size():
		if earth[i] == 0.0:
			continue
		var tile := Vector2i(roundi(pts[i].x), roundi(pts[i].y))
		if tile.y == 12 and tile.x >= 8 and tile.x <= 10:
			west += 1 if flows[i].x < -0.29 else 0
		elif (tile.y == 10 or tile.y == 14) and tile.x >= 8 and tile.x <= 10:
			east += 1 if flows[i].x > 0.29 else 0
	_check(west >= 3 and east >= 6, "the legs keep their own way: %d points west on the middle leg, %d east beside it" % [west, east])
	# The mouth: full on the land, gone in the river's water.
	var mouth_full := 0
	var mouth_gone := 0
	var mouth_half := 0
	for i in points.size():
		var p := pts[0]
		var world := Vector2(points[i].x / Iso.TILE_W + (points[i].y + Iso.TILE_H / 2.0) / Iso.TILE_H, (points[i].y + Iso.TILE_H / 2.0) / Iso.TILE_H - points[i].x / Iso.TILE_W)
		p = world
		var d: float = to_river.call(p)
		if d > 0.3:
			if colours[i].b > 0.99:
				mouth_full += 1
		elif d < -0.6:
			if colours[i].b < 0.01:
				mouth_gone += 1
			else:
				mouth_half += 1
	_check(mouth_full > 100 and mouth_gone > 4 and mouth_half == 0, "the brook is whole on the land (%d vertices) and gone in the river's water (%d gone, %d not): its mouth fades on the river's line" % [mouth_full, mouth_gone, mouth_half])
	# The same brook drawn from its mouth to its spring runs the same way:
	# the flow comes from the tiles, never from the order of the line.
	var reversed := line.duplicate()
	for key: String in ["pts", "widths", "alphas", "earth"]:
		var arr: Variant = line[key].duplicate()
		arr.reverse()
		reversed[key] = arr
	var ends: Array = (line["ends"] as Array).duplicate()
	ends.reverse()
	reversed["ends"] = ends
	var back := BrookLines.mesh([reversed], lo, lo + Vector2i(size, size), current_at, to_river)
	var by_point: Dictionary = {}
	for i in points.size():
		by_point[points[i]] = colours[i]
	var bpoints: PackedVector2Array = back["points"]
	var bcolours: PackedColorArray = back["colours"]
	var differ := 0
	for i in bpoints.size():
		if not by_point.has(bpoints[i]) or by_point[bpoints[i]] != bcolours[i]:
			differ += 1
	_check(bpoints.size() == points.size() and differ == 0, "drawn mouth to spring, the brook's vertices carry the same flow and fade: %d of %d differ" % [differ, bpoints.size()])
## The images of a kind of tree are read when a stand first holds it and
## freed after the grace once no stand holds it (weight rule 4): the
## holds follow the stands' making and release, parked ones included (a
## parked stand is not released); a stand made while its images are read
## shows nothing until they are in, then exactly what it would have.
func _test_tree_images_follow_the_stands_that_hold_them() -> void:
	var now := Time.get_ticks_msec()
	# Every kind out of memory to start from, whatever the tests before held.
	Trees.unload_all()
	_check(not Trees.loaded("oak") and Trees.holds("oak") == 0, "nothing held, nothing in memory")
	var trees: Array = []
	for i in 6:
		trees.append({"at": {"x": 1_700 + 3_100 * i, "y": 2_300}, "species": "oak" if i % 2 else "pine"})
	var first := Trees.Stand.new(trees, {"oak": 2_200, "pine": 3_000})
	_check(Trees.holds("oak") == 1 and Trees.holds("pine") == 1, "a stand holds its kinds")
	_check(not first.built and first.images.count(-1) == first.size(), "made while its images are read, the stand shows no tree")
	first.parent = RenderingServer.canvas_item_create()
	first.show_cells(Trees.Stand.cell_range(Rect2(-4000, -4000, 8000, 8000)))
	_check(first.items.is_empty(), "no tree gets a canvas item before its image is in")
	Trees.poll(now, true)
	_check(Trees.loaded("oak") and Trees.loaded("pine") and first.built, "the images in, the stand is built")
	_check(first.items.size() == first.size(), "and shows at once the trees the view asked for meanwhile")
	var same := Trees.Stand.new(trees, {"oak": 2_200, "pine": 3_000})
	_check(same.built and same.rects == first.rects and same.images == first.images, "a stand made with its kinds in memory is built at once, the same")
	_check(Trees.holds("oak") == 2, "two stands hold the oak")
	same.release()
	first.release()
	var grace := int(Trees.UNLOAD_GRACE_S * 1000.0)
	var released := Time.get_ticks_msec()
	_check(Trees.holds("oak") == 0 and Trees.loaded("oak"), "both released: nothing holds the oak, its images stay for the grace")
	Trees.poll(released + grace - 1)
	_check(Trees.loaded("oak") and Trees.loaded("pine"), "within the grace, both stay")
	var third := Trees.Stand.new([trees[1]], {"oak": 2_200})
	Trees.poll(released + grace + 1)
	_check(Trees.loaded("oak") and not Trees.loaded("pine"), "after the grace, the pine (held by no stand) is freed and the oak (held again) kept")
	var slot := third.images[0]
	third.release()
	released = Time.get_ticks_msec()
	Trees.poll(released + grace - 1)
	_check(Trees.loaded("oak"), "released again: kept until a new grace is over")
	Trees.poll(released + grace + 1)
	_check(not Trees.loaded("oak") and Trees.image(slot) == null, "then the oak's images are freed, their slots emptied")
	var again := Trees.Stand.new([trees[1]], {"oak": 2_200})
	Trees.poll(now, true)
	_check(again.built and again.images[0] == slot and Trees.image(slot) != null, "held again, the images come back in the same slots")
	again.release()
	RenderingServer.free_rid(first.parent)
	Trees.free_pool()
	Trees.unload_all()
	_check(Trees.memory().is_empty(), "the world going, every image is freed at once")


## A tree image is held without its finest mip level (its own BC7 blocks,
## a quarter of its weight) unless a tree of its kind is shown at a zoom
## where the card may read that level; it gets it then, keeping its
## texture (the trees drawn need no new drawing), and gives it up
## FINE_GRACE_S after it is no longer needed, zoomed out or out of view.
func _test_tree_images_hold_their_finest_level_only_up_close() -> void:
	# The levels are the imported texture's own blocks.
	var path := Trees.paths("oak")[0]
	var whole := Trees.read_image(path, 0)
	var coarse := Trees.read_image(path, Trees.COARSE)
	_check(whole.has("image") and coarse.has("image"), "an oak image is read from its imported file, whole and without its finest level")
	if not whole.has("image") or not coarse.has("image"):
		return
	var imported := (load(path) as Texture2D)
	var a: Image = whole["image"]
	var b: Image = coarse["image"]
	_check(a.get_data() == imported.get_image().get_data(), "read whole, it is the imported texture, block for block")
	_check(b.get_width() == a.get_width() / 2 and b.get_height() == a.get_height() / 2 and b.get_data() == a.get_data().slice(a.get_data().size() - b.get_data().size()), "read without its finest level, it is the whole one's chain from its second level")
	_check(whole["size"] == Vector2i(imported.get_width(), imported.get_height()) and coarse["size"] == whole["size"], "both report the imported texture's size")
	_check(int(coarse["bytes"]) * 4 < int(whole["bytes"]) * 1.01 and int(coarse["bytes"]) * 4 > int(whole["bytes"]) * 0.99, "without its finest level it weighs a quarter: %d against %d" % [coarse["bytes"], whole["bytes"]])
	# The rule: on the lead's screen (canvas stretched 2.4 times), every
	# kind is read whole from zoom 0.8 on and never at 0.6 and below, as
	# the card picks its levels (the measured threshold, about 0.73).
	var text := FileAccess.get_file_as_string(ProjectSettings.globalize_path("res://").path_join("../content/species.ron"))
	var re := RegEx.new()
	re.compile("id: \"([a-z_]+)\", name: \"[^\"]*\", height_m: ([0-9.]+)")
	var kinds := 0
	for m in re.search_all(text):
		var texels := 0
		for p: String in Trees.paths(m.get_string(1)):
			var h := int(Trees.read_image(p, Trees.COARSE).get("texels", 0))
			texels = h if texels == 0 else mini(texels, h)
		if texels == 0:
			continue
		kinds += 1
		var cm := int(float(m.get_string(2)) * 100.0)
		_check(not Trees.fine_read(texels, cm, 0.6, 2.4) and Trees.fine_read(texels, cm, 0.8, 2.4), "the %s needs its finest level from zoom 0.8 on a 3072 screen, not at 0.6" % m.get_string(1))
		_check(not Trees.fine_read(texels, cm, 0.4, 2.4) and Trees.fine_read(texels, cm, 2.5, 1.0), "the %s: never at the farthest zoom, always at the nearest" % m.get_string(1))
	_check(kinds >= 6, "every kind with images is checked: %d" % kinds)
	# A level of detail of exactly 1 + FINE_MARGIN at zoom 1 (bias -1).
	var cm := 2_200
	var px := Scale.shown(22.0) * Iso.PX_PER_M * (1.0 + Trees.SIZE_JITTER)
	var edge := int(px * pow(2.0, 2.0 + Trees.FINE_MARGIN))
	_check(Trees.fine_read(edge - 2, cm, 1.0, 1.0) and not Trees.fine_read(edge + 2, cm, 1.0, 1.0), "the threshold is a level of detail of 1 + FINE_MARGIN for the largest tree")
	# In the game: held coarse, sharpened when shown up close, kept for the
	# grace, softened after it; a kind not in view is never sharpened.
	Trees.unload_all()
	var trees: Array = []
	for i in 6:
		trees.append({"at": {"x": 1_700 + 3_100 * i, "y": 2_300}, "species": "oak"})
	var seen := Trees.Stand.new(trees, {"oak": 2_200})
	var unseen := Trees.Stand.new([{"at": {"x": 1_700, "y": 2_300}, "species": "pine"}], {"pine": 2_500})
	var now := Time.get_ticks_msec()
	Trees.view(0.4, 2.4)
	Trees.poll(now, true)
	seen.parent = RenderingServer.canvas_item_create()
	seen.show_cells(Trees.Stand.cell_range(Rect2(-4000, -4000, 8000, 8000)))
	var index := seen.images[0]
	var rid := Trees.image(index).get_rid()
	var coarse_mb: int = Trees.memory()["oak"]
	Trees.poll(now, true)
	_check(seen.built and not seen.items.is_empty() and not Trees.fine("oak") and not Trees.fine("pine"), "far away, the kinds are held without their finest level")
	Trees.view(2.5, 2.4)
	Trees.poll(now, true)
	_check(Trees.fine("oak") and Trees.sharpening().is_empty(), "up close, the oak shown gets its finest level")
	_check(not Trees.fine("pine"), "the pine, held but not shown, does not")
	_check(Trees.image(index).get_rid() == rid and Trees.image(index).get_width() == imported.get_width(), "the oak's texture keeps its RID and its size: the trees drawn with it are not drawn again")
	var fine_mb: int = Trees.memory()["oak"]
	_check(fine_mb > coarse_mb * 3.9 and fine_mb < coarse_mb * 4.1, "and weighs four times more: %d against %d" % [fine_mb, coarse_mb])
	Trees.view(0.4, 2.4)
	var grace := int(Trees.FINE_GRACE_S * 1000.0)
	Trees.poll(now, true)
	Trees.poll(now + grace - 1, true)
	_check(Trees.fine("oak"), "zoomed out, the oak keeps it for the grace")
	Trees.poll(now + grace + 1, true)
	_check(not Trees.fine("oak") and Trees.memory()["oak"] == coarse_mb and Trees.image(index).get_rid() == rid, "then gives it up, the same texture")
	Trees.view(2.5, 2.4)
	Trees.poll(now + grace + 2, true)
	_check(Trees.fine("oak"), "zoomed in again, it gets it back")
	seen.show_cells(Rect2i())
	Trees.poll(now + grace + 3, true)
	Trees.poll(now + 2 * grace + 2, true)
	_check(Trees.fine("oak"), "out of view, it keeps it for the grace")
	Trees.poll(now + 2 * grace + 4, true)
	_check(not Trees.fine("oak"), "then gives it up")
	seen.release()
	unseen.release()
	RenderingServer.free_rid(seen.parent)
	Trees.free_pool()
	Trees.unload_all()
	Trees.view(0.0, 1.0)


## Every tree image's outline (TreeOutline, art/trees/outlines.json) holds
## every texel the sway shader can draw of it: at every mipmap level the
## game reads, as the card holds the texture, with the filter's reach, the
## wind's largest bend at each row and the pixel of safety. A left and a
## right chain of at most MOST_VERTICES vertices, triangulated; and the
## game draws the image with it.
func _test_tree_outlines_hold_every_texel() -> void:
	var dir := DirAccess.open(Trees.DIR)
	if dir == null:
		_check(false, "the trees' folder is there")
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(Trees.OUTLINES))
	_check(parsed is Dictionary, "the outlines are written (tools/tree_outlines.gd)")
	if not parsed is Dictionary:
		return
	var outlines: Dictionary = parsed
	for file in dir.get_files():
		if not file.ends_with(".webp"):
			continue
		var texture: Texture2D = load(Trees.DIR + file)
		var w := texture.get_width()
		var h := texture.get_height()
		var entry: Variant = outlines.get(file)
		_check(entry is Dictionary, "%s has an outline (else run tools/tree_outlines.gd)" % file)
		if not entry is Dictionary:
			continue
		var size: Array = entry.get("texture", [])
		_check(size.size() == 2 and int(size[0]) == w and int(size[1]) == h, "%s's outline is for its size (else run tools/tree_outlines.gd)" % file)
		# The texels are the card's: the image padded to the compression's blocks.
		var image := texture.get_image()
		var padded: Array = entry.get("image", [])
		_check(padded.size() == 2 and int(padded[0]) == image.get_width() and int(padded[1]) == image.get_height(), "%s's outline is in the texels of the image as held" % file)
		h = image.get_height()
		var points := PackedVector2Array()
		for p: Array in entry.get("points", []):
			points.append(Vector2(float(p[0]), float(p[1])))
		_check(points.size() >= 4 and points.size() <= TreeOutline.MOST_VERTICES, "%s's outline has 4 to %d vertices (%d)" % [file, TreeOutline.MOST_VERTICES, points.size()])
		_check(Geometry2D.triangulate_polygon(points).size() == 3 * (points.size() - 2), "%s's outline triangulates" % file)
		# What it must hold, from the texture as the card holds it.
		var req := TreeOutline.required(image)
		var left: PackedFloat32Array = req["left"]
		var right: PackedFloat32Array = req["right"]
		var top: int = req["top"]
		var monotone := true
		var short := 0.0
		for y in range(top, h):
			var mid := TreeOutline.span(points, float(y) + 0.5)
			if int(mid["crossings"]) != 2:
				monotone = false
			# The polygon's span over the whole row: the narrowest it gets
			# at the row's edges and at any vertex within the row.
			var heights: Array[float] = [float(y), float(y + 1)]
			for v: Vector2 in points:
				if v.y > float(y) and v.y < float(y + 1):
					heights.append(v.y)
			for t: float in heights:
				var s := TreeOutline.span(points, t)
				short = maxf(short, maxf(float(s["left"]) - left[y], right[y] - float(s["right"])))
		_check(monotone, "%s's outline is a left and a right chain" % file)
		_check(short < 1e-3, "%s's outline holds every texel the shader may draw, bent and filtered (short by %.2f texels)" % [file, short])
	# The game draws with them.
	var oak: Array = Trees.textures("oak")
	if oak.is_empty():
		return
	var index := Trees.image_index("oak", Vector2(1_500, 2_500))
	var hull := Trees.hull(index)
	_check(hull.size() >= 4 and hull.size() <= TreeOutline.MOST_VERTICES and Trees.hull_indices(index).size() == 3 * (hull.size() - 2), "an oak is drawn as its outline, triangulated")
	var r := Trees.rect(Vector2(1_500, 2_500), 2_200, oak[0])
	var stand := Trees.Stand.new([{"at": {"x": 1_500, "y": 2_500}, "species": "oak"}], {"oak": 2_200})
	stand.parent = RenderingServer.canvas_item_create()
	stand.show_cells(Trees.Stand.cell_range(r))
	_check(stand.items.size() == 1, "the oak shown has its canvas item")
	stand.release()
	RenderingServer.free_rid(stand.parent)
	Trees.free_pool()


## A chunk frame's header alone: all FrameCache reads of a frame.
static func _frame_header(x: int, y: int, version: int) -> PackedByteArray:
	var bytes := PackedByteArray()
	bytes.resize(Protocol.CHUNK_HEADER)
	bytes.encode_s32(0, x)
	bytes.encode_s32(4, y)
	bytes.encode_u32(8, version)
	return bytes


## D4: the frames of the chunks left behind are kept, the least recently
## hidden forgotten first, in the order the server's mirror keeps them;
## `chunk_same` shows one again only at its version.
func _test_the_frame_cache_keeps_what_left_the_view() -> void:
	var cache := FrameCache.new()
	cache.capacity = 3
	for c: Array in [[0, 0, 0], [1, 0, 0], [0, 1, 2], [5, 5, 1]]:
		cache.received(Vector2i(c[0], c[1]), _frame_header(c[0], c[1], c[2]))
	var outside: Array[Vector2i] = [Vector2i(5, 5)]
	_check(cache.view(Rect2i(0, 0, 2, 2)) == outside, "what lies outside the window leaves")
	_check(cache.is_kept(Vector2i(5, 5)) and not cache.live.has(Vector2i(5, 5)), "kept, out of view")
	var left := cache.view(Rect2i(5, 5, 1, 1))
	var in_order: Array[Vector2i] = [Vector2i(0, 0), Vector2i(1, 0), Vector2i(0, 1)]
	_check(left == in_order, "left in rows, then columns: %s" % [left])
	_check(cache.kept_count() == 3 and not cache.is_kept(Vector2i(5, 5)), "the least recently hidden is forgotten first")
	_check(cache.same(Vector2i(0, 1), 2) == FrameCache.RESTORED, "a kept frame at its version comes back")
	_check(FrameCache.version_of(cache.live[Vector2i(0, 1)]) == 2, "in view again, with its frame")
	_check(cache.same(Vector2i(0, 1), 2) == FrameCache.HELD, "in view at that version: nothing to do")
	_check(cache.same(Vector2i(1, 0), 7) == FrameCache.MISSING, "another version: missing")
	_check(not cache.is_kept(Vector2i(1, 0)), "and the stale frame forgotten")
	_check(cache.same(Vector2i(9, 9), 0) == FrameCache.MISSING, "never held: missing")
	_check(cache.held() == [[0, 0, 0], [0, 1, 2]], "held: the kept first, then those in view: %s" % [cache.held()])
	var hello := Protocol.hello("w", cache.held())
	_check(hello["held"]["world"] == "w" and hello["held"]["chunks"].size() == 2, "hello tells them")
	_check(not Protocol.hello().has("held"), "a first hello holds nothing")
	cache.clear()
	_check(cache.live.is_empty() and cache.kept_count() == 0, "cleared")


## D4: the view message lets go of what lies outside, and a reconnection
## keeps the chunks held only when the world is the same.
func _test_the_net_keeps_the_chunks_of_the_same_world_only() -> void:
	var net: Node = load("res://net/client.gd").new()
	var hidden: Array = []
	net.chunk_hidden.connect(func(x: int, y: int) -> void: hidden.append(Vector2i(x, y)))
	net._handle({"type": "world_state", "world": "first", "materials": []})
	_check(not net.same_world, "a first world is new")
	for x in 3:
		net.frames.received(Vector2i(x, 0), _frame_header(x, 0, 0))
	net._handle({"type": "view", "x": 1, "y": 0, "width": 2, "height": 1})
	_check(hidden == [Vector2i(0, 0)], "the chunk outside the window is hidden: %s" % [hidden])
	_check(net.frames.is_kept(Vector2i(0, 0)), "its frame kept")
	net._handle({"type": "world_state", "world": "first", "materials": []})
	_check(net.same_world and net.frames.live.size() == 2 and net.frames.is_kept(Vector2i(0, 0)), "the same world: everything kept")
	net._handle({"type": "world_state", "world": "second", "materials": []})
	_check(not net.same_world and net.frames.live.is_empty() and net.frames.kept_count() == 0, "another world: nothing reused")
	net.free()


func _test_a_body_told_at_each_chunk_keeps_walking_its_route() -> void:
	# Told at tick 100 at (0, 5 m): 20 m of grass reached at tick 240
	# (fast), 10 m of forest reached at tick 380 (slow, the pace changes
	# there), then a turn south, 10 m reached at tick 450.
	var a := Vector2(20000, 5000)
	var b := Vector2(30000, 5000)
	var end := Vector2(30000, 15000)
	var route := [a, b, end]
	var arrivals := [240, 380, 450]
	var body := BodyWalk.told({}, Vector2(0, 5000), route, arrivals, 100.0, 1430.0, 100.0)
	_check(body["at"] == Vector2(0, 5000), "seen first: where the server says")
	var near := func(at: Vector2, want: Vector2) -> bool: return at.distance_to(want) < 1.0
	BodyWalk.advance(body, 0.0, 170.0)
	_check(near.call(body["at"], Vector2(10000, 5000)), "on the fast stretch: half way at half time")
	BodyWalk.advance(body, 0.0, 240.0)
	_check(near.call(body["at"], a), "at the change of pace on time")
	BodyWalk.advance(body, 0.0, 310.0)
	_check(near.call(body["at"], Vector2(25000, 5000)), "on the slow stretch: half its 10 m at half its time")
	# Told again as it walks into the next chunk (x 32 m is not reached:
	# the same walk told at tick 380 at b): it agrees, nothing moves.
	BodyWalk.advance(body, 0.0, 380.0)
	var shown: Vector2 = body["at"]
	var again := BodyWalk.told(body, b, [end], [450], 380.0, 1430.0, 380.0)
	_check(near.call(again["at"], shown), "told again: where it is shown, no jump")
	BodyWalk.advance(again, 0.0, 415.0)
	_check(near.call(again["at"], Vector2(30000, 10000)), "and walks on by time after the turn")
	# Told a little off (within 3 m): walked on from where it is shown, to
	# the next waypoint on time.
	var off := BodyWalk.told(again, Vector2(30000, 11000), [end], [450], 415.0, 1430.0, 415.0)
	_check(near.call(off["at"], Vector2(30000, 10000)), "a little off: no jump")
	BodyWalk.advance(off, 0.0, 450.0)
	_check(near.call(off["at"], end), "and on time at the end")
	_check(not BodyWalk.advance(off, 0.0, 460.0), "then it stands")
	# Far from where it is shown (a new order, a refusal): where told.
	var far := BodyWalk.told(again, Vector2(0, 0), [Vector2(1000, 0)], [500], 415.0, 1430.0, 415.0)
	_check(far["at"] == Vector2(0, 0), "told far from where it is shown: stands where told")
	# A message without arrivals walks at the nominal speed, as before.
	var nominal := BodyWalk.told({}, Vector2(0, 0), [Vector2(2860, 0)], [], 0.0, 1430.0, 0.0)
	BodyWalk.advance(nominal, 1.0, 0.0)
	_check(near.call(nominal["at"], Vector2(1430, 0)), "no arrivals: the nominal speed")


func _test_a_new_route_told_while_walking_turns_from_where_the_body_is_shown() -> void:
	# Found in play (12 November 2026): a new destination clicked while
	# walking sent the body back about a tile in a dash, then on its new
	# way. Walking east at 1.43 m/s (143 mm a tick), it is sent north at
	# tick 150: its new route sets off where the world has it then, a
	# change of pace half a metre on. The client, its clock a few ticks off
	# the world's (ahead or behind), shows it elsewhere when told: it turns
	# from there, never dashing nor standing still, and joins its route.
	var pace := 20000.0 / 140.0
	var turn := Vector2(50.0 * pace, 5000.0)
	var route := [turn + Vector2(0, -500), Vector2(turn.x, -5000)]
	var arrivals := [154, 220]
	for now: float in [153.0, 147.0]:
		var east := BodyWalk.told({}, Vector2(0, 5000), [Vector2(20000, 5000)], [240], 100.0, 1430.0, 100.0)
		BodyWalk.advance(east, 0.0, now)
		var shown: Vector2 = east["at"]
		var north := BodyWalk.told(east, turn, route, arrivals, 150.0, 1430.0, now)
		var why := "clock %s: " % ("ahead" if now > 150.0 else "behind")
		_check(north["at"].distance_to(shown) < 1.0, why + "turns from where it is shown")
		var dash := 0.0
		var still := INF
		var at: Vector2 = north["at"]
		var t := now
		while BodyWalk.advance(north, 0.0, t + 0.25):
			t += 0.25
			var step: float = at.distance_to(north["at"])
			dash = maxf(dash, step)
			still = minf(still, step)
			at = north["at"]
		_check(dash <= 1.6 * pace * 0.25, why + "never a dash (%.0f mm in a quarter tick)" % dash)
		_check(still > 0.25 * pace * 0.25, why + "never standing still on its way (%.0f mm)" % still)
		_check(north["at"] == route[1], why + "where the world has it at the end")


func _test_the_clock_catches_up_with_the_ticks_walks_are_told_at() -> void:
	# Set at the welcome, read two seconds late (a busy first frame): the
	# clock runs 20 ticks behind the world. A walk told at a later tick
	# sets it right; a walk told at a tick past (a message that waited)
	# never sets it back.
	var clock := GameClock.new()
	clock.set_clock({"tick": 1000, "ticks_per_second": 10, "time_scale_permille": 1000}, 0)
	_check(is_equal_approx(clock.tick_at(5000), 1050.0), "set at the welcome")
	clock.catch_up(1070.0, 5000)
	_check(is_equal_approx(clock.tick_at(5000), 1070.0), "caught up by a walk told later")
	_check(is_equal_approx(clock.tick_at(6000), 1080.0), "and runs on from there")
	clock.catch_up(1060.0, 6000)
	_check(is_equal_approx(clock.tick_at(6000), 1080.0), "never set back by a tick past")



## F4, protocol v20: `body_blocked` decodes into the signal, the reason
## has its words, and the cross at the feet shows, whole, then fades.
func _test_a_blocked_body_is_decoded_and_its_notice_shows_then_fades() -> void:
	var net: Node = load("res://net/client.gd").new()
	var got: Array = []
	net.body_blocked.connect(func(body: int, at: Vector2, x: int, y: int, reason: String) -> void:
		got.append([body, at, x, y, reason]))
	var wire := "{\"type\":\"body_blocked\",\"body\":1,\"at\":{\"x\":34700,\"y\":40500},\"x\":40,\"y\":40,\"reason\":\"water\"}"
	net._handle(Protocol.decode(wire))
	_check(got == [[1, Vector2(34700, 40500), 40, 40, "water"]], "body_blocked decodes: %s" % [got])
	net.free()

	_check(UiWords.blocked("water") == "Bloqué : l'eau est trop profonde", "water in words")
	_check(UiWords.blocked("rock") == "Bloqué : la falaise barre le chemin", "the cliff in words")
	_check(UiWords.blocked("no_way") == "Bloqué : aucun passage en vue", "no way in words")
	_check(UiWords.blocked("lava") == UiWords.blocked("no_way"), "an unknown reason reads as no way")
	var notice := BlockedNotice.new()
	_check(not notice.shown(0) and notice.alpha(0) == 0.0, "nothing before it is told")
	notice.show(Vector2(1500, 2500), 10_000)
	_check(notice.at == Vector2(1500, 2500), "the place")
	_check(notice.shown(10_000) and notice.alpha(10_000) == 1.0, "whole at once")
	var fading := 10_000 + BlockedNotice.SHOW_MS - BlockedNotice.FADE_MS / 2
	_check(is_equal_approx(notice.alpha(fading), 0.5), "half faded: %f" % notice.alpha(fading))
	_check(notice.alpha(10_000 + BlockedNotice.SHOW_MS - BlockedNotice.FADE_MS) == 1.0, "whole until the fade")
	_check(not notice.shown(10_000 + BlockedNotice.SHOW_MS) and notice.alpha(10_000 + BlockedNotice.SHOW_MS) == 0.0, "gone after")
	notice.show(Vector2.ZERO, 20_000)
	notice.clear()
	_check(not notice.shown(20_001), "cleared at once")


## Protocol v21: an order refused at once (the body at the bank, sent
## across the water) carries what the body sees; it reaches the thought
## bubble as the same words as a stop on the way, tied to where the body
## stands, so a click takes the camera there.
func _test_a_refused_order_says_why_in_a_thought() -> void:
	var net: Node = load("res://net/client.gd").new()
	var got: Array = []
	net.move_refused.connect(func(body: int, x: int, y: int, reason: String) -> void:
		got.append([body, x, y, reason]))
	# The wire as the server's `wire_format_is_tagged_snake_case` writes it.
	net._handle(Protocol.decode("{\"type\":\"move_refused\",\"body\":1,\"x\":4,\"y\":5,\"reason\":\"water\"}"))
	_check(got == [[1, 4, 5, "water"]], "move_refused decodes with its reason: %s" % [got])
	net.free()
	# As WorldView._on_move_refused does with it, the body standing at the bank.
	var thoughts := BubbleThread.new()
	var stands := Vector2(800_500, 99_700)
	thoughts.push("", UiWords.blocked(str(got[0][3])), stands, true, 1000)
	var said := str(thoughts.shown().back()["text"])
	_check(said == "Bloqué : l'eau est trop profonde", "the bubble says why: %s" % said)
	_check(thoughts.place_at(0) == stands, "and a click takes the camera to the body")


## Each side keeps its last KEPT bubbles in order of arrival (no wait:
## a bubble enters the thread at once) and shows the SHOWN newest, the
## newest last; the same bubble told twice is kept once, at its new place.
func _test_a_thread_keeps_its_last_twenty_and_shows_three_newest_last() -> void:
	var thread := BubbleThread.new()
	_check(thread.size() == 0 and thread.shown().is_empty() and not thread.folded(100_000), "nothing at first")
	thread.push("", "a", Vector2.ZERO, false, 1000)
	thread.push("", "b", Vector2.ZERO, false, 1001)
	_check(_texts(thread.shown()) == ["a", "b"], "two told at once, both shown, the newest last: %s" % [_texts(thread.shown())])
	thread.clear()
	for i in BubbleThread.KEPT + 5:
		thread.push("", str(i), Vector2.ZERO, false, 1000 + i)
	_check(thread.size() == BubbleThread.KEPT, "no more than %d kept: %d" % [BubbleThread.KEPT, thread.size()])
	_check(_texts(thread.shown()) == ["22", "23", "24"], "the three newest, the newest last: %s" % [_texts(thread.shown())])
	thread.push("", "24", Vector2(3, 4), true, 2000)
	_check(thread.size() == BubbleThread.KEPT and thread.place_at(2) == Vector2(3, 4), "the same line told again: kept once, at its new place")
	thread.push("Corps 3", "24", Vector2.ZERO, false, 2001)
	_check(_texts(thread.shown()) == ["23", "24", "24"], "the same line by someone else is a new bubble")


## Hovering, the wheel moves the focus back to the oldest kept and down
## again to the newest; a new bubble brings it back to the newest.
func _test_a_thread_scrolls_back_to_the_oldest_and_down_to_the_newest() -> void:
	var thread := BubbleThread.new()
	for i in BubbleThread.KEPT + 5:
		thread.push("", str(i), Vector2.ZERO, false, 1000)
	_check(thread.max_scroll() == BubbleThread.KEPT - 1, "the range, to the oldest: %d" % thread.max_scroll())
	thread.scroll(1)
	_check(_texts(thread.shown()) == ["22", "23", "24"], "one back: %s" % [_texts(thread.shown())])
	thread.scroll(100)
	_check(thread.scrolled() == thread.max_scroll() and _texts(thread.shown()) == ["5", "6", "7"],
			"no further than the oldest kept: %s" % [_texts(thread.shown())])
	thread.scroll(-1)
	_check(_texts(thread.shown()) == ["5", "6", "7"], "and down again: %s" % [_texts(thread.shown())])
	thread.scroll(-1)
	_check(_texts(thread.shown()) == ["6", "7", "8"], "the window follows the focus: %s" % [_texts(thread.shown())])
	thread.scroll(-100)
	_check(thread.scrolled() == 0 and _texts(thread.shown()) == ["22", "23", "24"], "no further than the newest")
	thread.scroll(4)
	thread.push("", "new", Vector2.ZERO, false, 2000)
	_check(thread.scrolled() == 0 and thread.shown().back()["text"] == "new", "a new bubble shows at the bottom")
	var short := BubbleThread.new()
	short.push("", "a", Vector2.ZERO, false, 0)
	short.push("", "b", Vector2.ZERO, false, 0)
	short.scroll(5)
	_check(short.max_scroll() == 1 and short.scrolled() == 1 and short.focus_slot() == 0,
			"two: the focus goes back to the first, the second below it")


## Scrolling, the bubble in focus has the older ones above it and, until
## it is the newest, a newer one below it: what the stack draws whole and
## what it blurs.
func _test_the_focus_has_older_above_and_newer_below_while_scrolling() -> void:
	var thread := BubbleThread.new()
	_check(thread.focus_slot() == -1, "no focus when nothing is kept")
	for i in 6:
		thread.push("", str(i), Vector2.ZERO, false, 1000)
	var seen: Array[String] = []
	for step in thread.max_scroll() + 1:
		var window := _texts(thread.shown())
		var slot := thread.focus_slot()
		var focus := int(window[slot])
		seen.append("%s@%d" % [window, slot])
		_check(focus == 5 - step, "the focus goes back one bubble a notch: %s" % seen.back())
		for i in window.size():
			_check(int(window[i]) - focus == i - slot, "older above, newer below it, in order: %s" % seen.back())
		var below := window.size() - 1 - slot
		_check((below == 0) == (step == 0), "a newer bubble below the focus unless it is the newest: %s" % seen.back())
		_check(slot > 0 or focus == 0, "an older bubble above it unless it is the oldest: %s" % seen.back())
		thread.scroll(1)
	thread.scroll(-100)
	_check(thread.focus_slot() == 2 and _texts(thread.shown()) == ["3", "4", "5"], "back on the newest, nothing below it")


## Left alone FOLD_MS (nothing new, no hover), the stack folds into one
## small bubble, its newest line shortened; a new bubble or the mouse
## unfolds it, and the delay starts again when the mouse leaves.
func _test_a_thread_folds_when_left_alone_and_unfolds() -> void:
	var thread := BubbleThread.new()
	thread.push("Corps 3", "Ah, t'es là !", Vector2.ZERO, false, 1000)
	_check(not thread.folded(1000 + BubbleThread.FOLD_MS - 1), "open until the delay")
	_check(thread.folded(1000 + BubbleThread.FOLD_MS), "folded after it")
	_check(thread.folded_text() == "Corps 3 : Ah, t'es là !", "the folded bubble says who and what: %s" % thread.folded_text())
	var later := 1000 + BubbleThread.FOLD_MS * 2
	thread.push("Voix d'essai", "Une ligne bien trop longue pour tenir dans la petite bulle.", Vector2.ZERO, false, later)
	_check(not thread.folded(later), "a new bubble unfolds it")
	_check(thread.folded_text().length() <= BubbleThread.FOLDED_CHARS and thread.folded_text().ends_with("…"),
			"a long line shortened: %s" % thread.folded_text())
	var hover_at := later + BubbleThread.FOLD_MS + 100
	_check(thread.folded(hover_at), "folded again")
	for i in 3:
		thread.push("", str(i), Vector2.ZERO, false, hover_at - 50)
	thread.scroll(2)
	thread.hover(true, hover_at)
	_check(not thread.folded(hover_at), "the mouse unfolds it")
	_check(not thread.folded(hover_at + BubbleThread.FOLD_MS * 3), "and it stays open while hovered")
	thread.scroll(2)
	var left := hover_at + BubbleThread.FOLD_MS * 3
	thread.hover(false, left)
	_check(not thread.folded(left + BubbleThread.FOLD_MS - 1) and thread.folded(left + BubbleThread.FOLD_MS),
			"the delay starts again when the mouse leaves")
	thread.hover(true, left + BubbleThread.FOLD_MS)
	_check(thread.scrolled() == 0, "unfolded by the mouse, it opens on the newest")


## Left alone HIDE_MS with nothing new, the folded bubble goes too: only
## the icon stays, and hovering it does not open the stack.
func _test_a_thread_left_alone_longer_leaves_only_the_icon() -> void:
	var thread := BubbleThread.new()
	_check(not thread.hidden(1_000_000), "no icon with nothing kept")
	for i in 4:
		thread.push("", str(i), Vector2.ZERO, false, 1000)
	_check(thread.folded(1000 + BubbleThread.FOLD_MS) and not thread.hidden(1000 + BubbleThread.FOLD_MS),
			"after 8 s, the newest only, folded")
	_check(thread.folded(1000 + BubbleThread.HIDE_MS - 1) and not thread.hidden(1000 + BubbleThread.HIDE_MS - 1),
			"folded until 25 s")
	var gone := 1000 + BubbleThread.HIDE_MS
	_check(thread.hidden(gone) and not thread.folded(gone), "then the icon only")
	thread.hover(true, gone + 10)
	_check(thread.hidden(gone + 10), "the mouse on the icon does not open it")
	thread.hover(false, gone + 20)
	_check(thread.hidden(gone + 20), "nor leaving it")


## Clicked, the icon opens the stack on the newest, whole; a new bubble
## opens it too, on itself.
func _test_the_icon_reopens_on_the_newest_and_so_does_a_new_bubble() -> void:
	var thread := BubbleThread.new()
	for i in 6:
		thread.push("", str(i), Vector2.ZERO, false, 1000)
	thread.hover(true, 1000)
	thread.scroll(3)
	thread.hover(false, 2000)
	var gone := 2000 + BubbleThread.HIDE_MS
	_check(thread.hidden(gone) and thread.scrolled() == 3, "left scrolled back, then iconised")
	thread.reopen(gone + 100)
	_check(not thread.hidden(gone + 100) and not thread.folded(gone + 100), "the click opens it, unfolded")
	_check(thread.scrolled() == 0 and thread.focus_slot() == 2 and thread.shown().back()["text"] == "5",
			"on the newest: %s" % [_texts(thread.shown())])
	_check(thread.folded(gone + 100 + BubbleThread.FOLD_MS), "and folds again when left alone")
	var again := gone + 100 + BubbleThread.HIDE_MS
	_check(thread.hidden(again), "iconised again")
	thread.push("Corps 3", "Te revoilà.", Vector2.ZERO, false, again + 1)
	_check(not thread.hidden(again + 1) and not thread.folded(again + 1), "a new bubble opens it")
	_check(thread.scrolled() == 0 and thread.shown().back()["text"] == "Te revoilà.", "on itself, at the bottom")


## A thought tied to a place stays clickable back in the history: its
## slot in the window gives its place; a thought with none gives null.
func _test_a_placed_thought_is_clicked_from_the_history() -> void:
	var thread := BubbleThread.new()
	thread.push("", "a", Vector2(1, 2), true, 1000)
	for line in ["b", "c", "d"]:
		thread.push("", line, Vector2.ZERO, false, 1000)
	_check(thread.place_at(0) == null and thread.place_at(2) == null, "the window b, c, d: none tied to a place")
	thread.scroll(2)
	_check(_texts(thread.shown()) == ["a", "b", "c"] and thread.place_at(0) == Vector2(1, 2),
			"scrolled back, the first thought still takes the camera to its place")
	_check(thread.place_at(-1) == null and thread.place_at(3) == null, "no slot outside the window")


func _texts(entries: Array[Dictionary]) -> Array[String]:
	var texts: Array[String] = []
	for entry in entries:
		texts.append(str(entry["text"]))
	return texts
## Protocol v22: `body_thinks` decodes into the signal, as the server's
## `wire_format_is_tagged_snake_case` writes it, and each thought has its
## words.
func _test_body_thinks_decodes_and_has_its_words() -> void:
	var net: Node = load("res://net/client.gd").new()
	var got: Array = []
	net.body_thinks.connect(func(body: int, kind: String, side: String, at: Vector2) -> void:
		got.append([body, kind, side, at]))
	net._handle(Protocol.decode("{\"type\":\"body_thinks\",\"body\":1,\"thought\":{\"kind\":\"skirting\",\"side\":\"right\"},\"at\":{\"x\":1500,\"y\":2500}}"))
	net._handle(Protocol.decode("{\"type\":\"body_thinks\",\"body\":1,\"thought\":{\"kind\":\"no_way\"},\"at\":{\"x\":1,\"y\":2}}"))
	_check(got == [[1, "skirting", "right", Vector2(1500, 2500)], [1, "no_way", "", Vector2(1, 2)]], "body_thinks decodes: %s" % [got])
	net.free()
	_check(UiWords.thought("skirting", "left") == "Pas de passage ici… je longe vers la gauche", "skirting left in words")
	_check(UiWords.thought("skirting", "right") == "Pas de passage ici… je longe vers la droite", "skirting right in words")
	_check(UiWords.thought("way_seen", "") == "Je vois un passage, j'y vais", "a way seen in words")
	_check(UiWords.thought("no_way", "") == "Je ne sais pas où aller", "no way in words")
	_check(UiWords.thought("dreaming", "") == UiWords.thought("no_way", ""), "an unknown thought reads as no way")


## The recentre button shows only while the camera does not follow the
## body, as WorldView tells it (following_changed); a click glides the
## camera back over CameraGlide.SECONDS, eased, following the body as it
## walks.
func _test_the_recentre_button_shows_while_the_camera_does_not_follow() -> void:
	var button := RecentreButton.new()
	button.show_for(false)
	_check(button.visible, "shown after a drag")
	button.show_for(true)
	_check(not button.visible, "hidden once the camera follows again")
	button.free()
	var main := FileAccess.get_file_as_string("res://main.gd")
	_check(main.contains("_world.following_changed.connect(recentre.show_for)"), "the button listens to the camera")
	_check(main.contains("recentre.recentre.connect(_world.recentre)"), "and its click recentres it")
	var world := FileAccess.get_file_as_string("res://world/world_view.gd")
	_check(world.contains("following_changed.emit(value)"), "the world tells each change of _following")
	var glide := CameraGlide.new()
	glide.start(Vector2.ZERO)
	var half := glide.step(CameraGlide.SECONDS / 2.0, Vector2(100, 0))
	_check(glide.active and half.x > 50.0 and half.x < 100.0, "quick at first: %s" % half)
	var there := glide.step(CameraGlide.SECONDS / 2.0, Vector2(110, 0))
	_check(not glide.active and there == Vector2(110, 0), "on the body, which walked meanwhile: %s" % there)
