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
	_test_currents_decode()
	_test_tree_images_are_clean()
	_test_loading_waits_for_the_valley()
	_test_dev_map_turns_as_the_game()
	_test_perf_overlay_reports()
	_test_plants_are_named_and_coloured()
	_test_trees_sharpness_eases_with_the_zoom()
	_test_the_shore_field_holds_every_chunk_held()
	_test_the_bounds_of_the_chunks_held()
	_test_the_camera_stops_at_the_band()
	_test_the_watcher_sees_the_client_files()
	_test_the_view_follows_the_body_tile_by_tile()


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
	_check(ChunkView.decor_textures("rock").size() > 5 and ChunkView.decor_textures("tuft").size() > 5, "the rock and tuft decorations are there")
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
	ShoreField.update(terrain, Vector2i(32, 0), 32)
	ShoreField.update(terrain, Vector2i(32, 32), 32)
	for y: float in [10.0, 31.9, 32.1, 50.0]:
		_check(absf(ShoreField.distance_at(Vector2(40.0, y))) < 0.15, "the edge at x = 40 (y %s): %f" % [y, ShoreField.distance_at(Vector2(40.0, y))])
		_check(absf(ShoreField.distance_at(Vector2(38.5, y)) - 1.5) < 0.2, "1.5 tiles inland: %f" % ShoreField.distance_at(Vector2(38.5, y)))
		_check(absf(ShoreField.distance_at(Vector2(41.5, y)) + 1.5) < 0.2, "1.5 tiles out in the water: %f" % ShoreField.distance_at(Vector2(41.5, y)))
	# A step of one tile in the edge comes out smooth: halfway along the
	# step, the edge lies between the two lines of tiles.
	var stepped := func(t: Vector2i) -> String: return "water" if t.x >= (40 if t.y < 80 else 41) else "grass"
	ShoreField.update(stepped, Vector2i(32, 64), 32)
	var at_step := ShoreField.distance_at(Vector2(40.5, 80.0))
	_check(absf(at_step) < 0.25, "the step is smoothed: %f at its middle" % at_step)
	# A pond's corner is rounded: the corner point of the tiles lies on
	# the land side of the smoothed edge.
	var pond := func(t: Vector2i) -> String: return "water" if t.x >= 100 and t.y >= 100 else "grass"
	ShoreField.update(pond, Vector2i(96, 96), 32)
	_check(ShoreField.distance_at(Vector2(100.0, 100.0)) > 0.15, "a corner comes out round: %f" % ShoreField.distance_at(Vector2(100.0, 100.0)))
	# A ford: water for the bank, and its own channel, high inside, nothing
	# far from it.
	var forded := func(t: Vector2i) -> String:
		if t.x < 140:
			return "grass"
		return "ford" if t.y >= 140 and t.y < 146 else "water"
	ShoreField.update(forded, Vector2i(128, 128), 32)
	_check(ShoreField.distance_at(Vector2(150.0, 143.0)) < -1.0, "a ford is water for the bank")
	_check(ShoreField.ford_at(Vector2(150.0, 143.0)) > 0.7, "inside the ford: %f" % ShoreField.ford_at(Vector2(150.0, 143.0)))
	_check(ShoreField.ford_at(Vector2(150.0, 152.0)) < 0.05, "deep water away from the ford: %f" % ShoreField.ford_at(Vector2(150.0, 152.0)))
	# The water runs where the server says, the land between keeps still.
	var river := func(t: Vector2i) -> String: return "water" if t.x >= 200 and t.x < 210 else "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.0, 0.6)
	ShoreField.update(river, Vector2i(192, 192), 32, runs)
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
	ShoreField.update(far_edge, Vector2i(ShoreField.TILES + 32, 0), 32)
	ShoreField.map_size = map_size
	_check(absf(ShoreField.distance_at(Vector2(ShoreField.TILES + 50.0, 10.0))) < 0.15, "the edge past the texture's width: %f" % ShoreField.distance_at(Vector2(ShoreField.TILES + 50.0, 10.0)))
	_check(absf(ShoreField.distance_at(Vector2(ShoreField.TILES + 48.5, 10.0)) - 1.5) < 0.2, "inland past the texture's width")


func _test_the_shore_field_in_slices() -> void:
	# Spread over frames, the field comes out the same as at once; an
	# update overtaken by a newer one for its block gives up.
	var river := func(t: Vector2i) -> String:
		if t.x >= 70 and t.x < 76:
			return "ford" if t.y >= 40 and t.y < 44 else "water"
		return "grass"
	var land := func(_t: Vector2i) -> String: return "grass"
	var runs := func(_t: Vector2i) -> Vector2: return Vector2(0.5, 0.0)
	var block := Rect2i(Vector2i(64, 32) * ShoreField.PX, Vector2i(32, 32) * ShoreField.PX)
	ShoreField.update(river, Vector2i(64, 32), 32, runs)
	var at_once := ShoreField._image.get_region(block).get_data()
	ShoreField.update(land, Vector2i(64, 32), 32)
	ShoreField.update(river, Vector2i(64, 32), 32, runs, 1)
	var landed := ShoreField._image.get_region(block).get_data()
	for i in 60:
		await process_frame
	_check(landed != at_once, "an update in slices writes later")
	_check(ShoreField._image.get_region(block).get_data() == at_once, "in slices, the same field as at once")
	ShoreField.update(river, Vector2i(64, 32), 32, runs, 1)
	ShoreField.update(land, Vector2i(64, 32), 32)
	var bare := ShoreField._image.get_region(block).get_data()
	for i in 60:
		await process_frame
	_check(ShoreField._image.get_region(block).get_data() == bare, "an overtaken update gives up")


func _test_trees_sharpness_eases_with_the_zoom() -> void:
	_check(Trees.sharpness(2.0) == -1.0 and Trees.sharpness(1.0) == -1.0, "a level sharper up close")
	_check(Trees.sharpness(0.5) == 0.0 and Trees.sharpness(0.35) == 0.0, "the hardware's own level far away")
	_check(is_equal_approx(Trees.sharpness(0.75), -0.5), "eased in between")


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
	_check(text.contains("fps") and text.contains("12.5 ms") and text.contains("frame step") and text.contains("draw calls") and text.contains("wind on"), "the counters report: %s" % text)
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
		return tile.x >= lo.x - 20.01 and tile.y >= lo.y - 20.01 and tile.x <= hi.x + 20.01 and tile.y <= hi.y + 20.01
	for view: Vector2 in [Vector2(1600, 900) / 0.4, Vector2(3840, 2160) / 0.4, Vector2(1280, 720)]:
		for corners: float in [0.0, 1.0]:
			var area := Iso.camera_area(lo, hi, view, 20.0, corners)
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
## chunks a client holds (the server's 13 × 13 view, with the margin read
## around a chunk), or
## two chunks held would draw each other's banks.
func _test_the_shore_field_holds_every_chunk_held() -> void:
	var held := (2 * 6 + 1) * 32
	_check(ShoreField.TILES >= held + 2 * ShoreField.MARGIN, "the bank texture is wider than the chunks held: %d" % ShoreField.TILES)


## The limit of the view moves with the body, tile by tile, and stays
## inside the ground held with room for the band and the fog.
func _test_the_view_follows_the_body_tile_by_tile() -> void:
	# 13 chunks held around chunk 50, as the server holds them.
	var held := Rect2(Vector2(44, 44) * 32, Vector2(13, 13) * 32)
	var body := Vector2(50 * 32 + 16.5, 50 * 32 + 16.5)
	var limit := Iso.view_limit(body, 192.0, held, 0.0)
	_check(limit.position == body - Vector2(192, 192) and limit.end == body + Vector2(192, 192), "192 tiles around the body: %s" % limit)
	var step := Iso.view_limit(body + Vector2(1, 0), 192.0, held, 0.0)
	_check(step.position.x == limit.position.x + 1.0, "a tile east, the limit a tile east")
	for x in [0.0, 31.9]:
		var at := Vector2(50 * 32 + x, 50 * 32 + 16.0)
		var l := Iso.view_limit(at, 192.0, held, 0.0)
		_check(is_equal_approx(maxf(l.position.x, held.position.x), l.position.x) and l.end.x <= held.end.x + 0.001,
			"the view never passes the chunks held, wherever the body stands in its chunk: %s" % at)
		_check(l.size.x > 380.0, "and reaches nearly 192 tiles each way: %s" % l.size)


## In development the client restarts when one of its files changes: the
## watcher sees the scripts and the images, not Godot's cache nor the
## ground's files, which reload live.
func _test_the_watcher_sees_the_client_files() -> void:
	var files := DevWatcher.scan()
	_check(files.has("res://main.gd") and files.has("res://world/world_view.gd"), "the scripts are watched")
	_check(files.has("res://world/trees.gdshader"), "the shaders are watched")
	_check(not files.has("res://world/ground.gdshader"), "the ground's shader reloads live, no restart")
	_check(files.keys().all(func(p: String) -> bool: return not p.contains("/.godot/") and not p.ends_with(".import")), "never Godot's cache")


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
		var pale := 0
		for y in range(0, h, 2):
			for x in range(0, w, 2):
				var c := image.get_pixel(x, y)
				if c.a > 0.4 and minf(c.r, minf(c.g, c.b)) > 0.73 and maxf(c.r, maxf(c.g, c.b)) - minf(c.r, minf(c.g, c.b)) < 0.18:
					pale += 1
		_check(pale < 15, "%s has no pale specks: %d" % [file, pale])

