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
	# The shore plan: a 2 x 2 pond gets four corners and no edge, a straight
	# river two edges per water tile; the pieces are there.
	var pond: Array = []
	for y in 6:
		for x in 6:
			pond.append("water" if (x == 2 or x == 3) and (y == 2 or y == 3) else "grass")
	view = ChunkView.new()
	view.show_chunk(Vector2i.ZERO, 6, pond, [], {})
	var plan := view.shore_plan()
	var corners := plan.filter(func(p: Dictionary) -> bool: return p["kind"] == "corner")
	_check(plan.size() == 4 and corners.size() == 4, "a 2 x 2 pond gets four corner pieces")
	var river: Array = []
	for y in 6:
		for x in 6:
			river.append("water" if x == 3 else "grass")
	view.show_chunk(Vector2i.ZERO, 6, river, [], {})
	plan = view.shore_plan()
	_check(plan.size() == 12 and plan.all(func(p: Dictionary) -> bool: return p["kind"] == "edge"), "a straight river gets an edge on each side of every water tile")
	_check(ChunkView.shore_pieces("sand", "edge").size() >= 2 and ChunkView.shore_pieces("grass", "corner").size() >= 2, "the shore pieces are there")
	var island: Array = []
	for y in 6:
		for x in 6:
			island.append("grass" if x == 3 and y == 3 else "water")
	view.show_chunk(Vector2i.ZERO, 6, island, [], {})
	plan = view.shore_plan()
	var points := plan.filter(func(p: Dictionary) -> bool: return p["kind"] == "point")
	_check(points.size() == 4 and plan.size() == 4, "a lone land tile gets four point pieces and no edge")
	# A water tile with land north and west but water on the diagonal gets
	# two edges, never a corner (a corner would paint land on that water).
	var notch: Array = []
	for y in 6:
		for x in 6:
			notch.append("water" if (x >= 3 and y >= 3) or (x < 3 and y < 3) else "grass")
	view.show_chunk(Vector2i.ZERO, 6, notch, [], {})
	plan = view.shore_plan()
	var at33 := plan.filter(func(p: Dictionary) -> bool: return p["tile"] == Vector2i(3, 3))
	_check(at33.size() == 2 and at33.all(func(p: Dictionary) -> bool: return p["kind"] == "edge"), "no corner where the diagonal tile is water")
	# A 45-degree staircase: its corner cells draw diagonals, its points nothing.
	var stairs: Array = []
	for y in 8:
		for x in 8:
			stairs.append("water" if x + y >= 8 else "grass")
	view.show_chunk(Vector2i.ZERO, 8, stairs, [], {})
	plan = view.shore_plan()
	var diags := plan.filter(func(p: Dictionary) -> bool: return p["kind"] == "corner" and p.get("diag", false))
	var pts := plan.filter(func(p: Dictionary) -> bool: return p["kind"] == "point")
	_check(diags.size() >= 5 and pts.is_empty(), "a staircase is drawn with diagonal corners and no points")
	_check(ChunkView.shore_pieces("grass", "diag").size() == 3, "the three diagonal pieces are there")
	# The bank as pieces: a straight river gives two lines, each laid as
	# straight pieces one tile long, end to end; a staircase is laid with
	# 45-degree pieces; a pond closes on itself with bends; every piece
	# runs over boundary tiles only.
	var pieces_river: Array = []
	for y in 8:
		for x in 8:
			pieces_river.append("water" if x >= 3 and x <= 5 else "grass")
	view.show_chunk(Vector2i.ZERO, 8, pieces_river, [], {})
	var laid := view.lay_shore_pieces()
	var straight := laid.filter(func(p: Dictionary) -> bool: return p["turn"] == 0)
	_check(laid.size() >= 12 and straight.size() == laid.size(), "a straight river is laid with straight pieces only")
	var one_long := laid.all(func(p: Dictionary) -> bool: return absf(p["from"].distance_to(p["to"]) - 1.0) < 0.01)
	_check(one_long, "a straight piece is one tile long")
	var is_bank := func(t: Vector2i) -> bool:
		var w: bool = view._is_bank_water_tile(t)
		for n in [Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1)]:
			if view._is_known(t.x + n.x, t.y + n.y) and view._is_bank_water_tile(t + n) != w:
				return true
		return false
	var on_boundary := laid.all(func(p: Dictionary) -> bool:
		for q in p["pts"]:
			if not is_bank.call(Vector2i(int(round(q.x)), int(round(q.y)))):
				return false
		return true)
	_check(on_boundary, "no piece passes through a whole tile")
	var pieces_stairs: Array = []
	for y in 10:
		for x in 10:
			pieces_stairs.append("water" if x + y >= 10 else "grass")
	view.show_chunk(Vector2i.ZERO, 10, pieces_stairs, [], {})
	laid = view.lay_shore_pieces()
	var diagonal := laid.filter(func(p: Dictionary) -> bool: return p["turn"] == 0 and ShorePieces.slope_name(p["dir"]) == "45deg")
	_check(laid.size() >= 6 and diagonal.size() >= laid.size() - 2, "a staircase is laid with 45-degree pieces")
	var pieces_pond: Array = []
	for y in 10:
		for x in 10:
			pieces_pond.append("water" if x >= 3 and x <= 6 and y >= 3 and y <= 6 else "grass")
	view.show_chunk(Vector2i.ZERO, 10, pieces_pond, [], {})
	laid = view.lay_shore_pieces()
	var chained := true
	for i in range(1, laid.size()):
		if laid[i]["from"].distance_to(laid[i - 1]["to"]) > 0.001:
			chained = false
	var bends := laid.filter(func(p: Dictionary) -> bool: return p["turn"] != 0)
	var closes: bool = laid.size() > 4 and laid[laid.size() - 1]["to"].distance_to(laid[0]["from"]) <= 0.5
	_check(chained and closes and bends.size() >= 4, "a pond is one chain of pieces that closes on itself, bending at its corners")
	# The isometric shore: a water tile with land to the north takes the far
	# bank sloping down-right; land only on a diagonal takes a corner piece.
	_check(ChunkView.shore_iso_names(true, false, false, false, false, false, false, false) == ["01_far_bank_down_right"], "land north: the far bank piece")
	_check(ChunkView.shore_iso_names(false, false, false, false, false, false, false, true) == ["15_corner_land_up"], "land only on the north-west diagonal: the corner piece name (no image yet: the neighbours' banks already meet there)")
	_check(ChunkView.shore_iso_piece("02_far_bank_up_right") != null, "the isometric pieces load")
	# The shore line: a staircase of two-tile steps simplifies to one run.
	var stairs2: Array = []
	for i in 12:
		stairs2.append(Vector2(3 + i / 2, i * 0.5))
	var runs := ChunkView.simplify(stairs2, 0.6)
	_check(runs.size() == 2, "a two-tile staircase simplifies to one straight run")
	var bent := ChunkView.round_bends([Vector2(0, 0), Vector2(4, 0), Vector2(4, 4)])
	_check(bent.size() > 3 and bent[0] == Vector2(0, 0) and bent[bent.size() - 1] == Vector2(4, 4), "a bend is rounded and the ends stay")
	_check(ChunkView.shore_brush("far") != null and ChunkView.shore_brush("near") != null, "the bank brushes load")
	_check(ChunkView.shore_iso_names(true, false, false, true, true, false, false, true) == ["05_horizontal_land_above"], "land north, west and on an outer diagonal (a step of stairs): the straight piece")
	_check(ChunkView.shore_iso_names(true, false, false, true, false, false, false, true) == ["05i_inner_corner_land_above"], "land north, west and only the diagonal between (a true corner): the rounded L piece")
	view.free()
	var forest := ChunkView.ground_material("forest")
	_check(forest != material and forest.get_shader_parameter("base_tint") != material.get_shader_parameter("base_tint"), "the forest has its own material, in the shade")
