extends SceneTree
## Ground bench: renders a window of a real map (a PGM written by the
## server's `dump_valley` test) with the ChunkView alone, no interface,
## no bodies, no items, and saves it as a square image. For reference
## images of the grounds as the game shows them.
##   SOUL_INVASION_MAP=valley.pgm SOUL_INVASION_LOOK_AT=x,y \
##   SOUL_INVASION_LAB_ZOOM=2 SOUL_INVASION_SCREENSHOT=out.png \
##   godot --path client -s dev/ground_lab.gd
## SOUL_INVASION_SHORE="" turns the shore drawing off ("pieces", "curve"
## keep it).

const SIZE := 1024
## Tiles rendered around the looked-at tile, each way.
const REACH := 20
const TERRAINS := {200: "grass", 120: "forest", 40: "water", 90: "ford", 60: "brook", 230: "rock"}


func _initialize() -> void:
	var map := _read_pgm(OS.get_environment("SOUL_INVASION_MAP"))
	if map.is_empty():
		push_error("SOUL_INVASION_MAP must name a PGM (P2) file")
		quit(1)
		return
	var look := OS.get_environment("SOUL_INVASION_LOOK_AT").split(",")
	var at := Vector2i(int(look[0]), int(look[1])) if look.size() == 2 else Vector2i(map["width"] / 2, map["height"] / 2)
	var shore := OS.get_environment("SOUL_INVASION_SHORE")
	if shore != "pieces" and shore != "curve" and shore != "field":
		var spec := ChunkView.read_ground_spec()
		spec["shore"] = {}
		ChunkView._spec = spec
	var viewport := SubViewport.new()
	viewport.size = Vector2i(SIZE, SIZE)
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(viewport)
	var world := Node2D.new()
	viewport.add_child(world)
	var origin := at - Vector2i(REACH, REACH)
	var side := 2 * REACH
	var tiles: Array = []
	for y in side:
		for x in side:
			tiles.append(_terrain(map, origin.x + x, origin.y + y))
	var view := ChunkView.new()
	world.add_child(view)
	view.show_chunk(origin, side, tiles, [], {})
	var camera := Camera2D.new()
	world.add_child(camera)
	camera.position = Iso.tile_to_screen(at.x, at.y)
	var zoom := OS.get_environment("SOUL_INVASION_LAB_ZOOM")
	camera.zoom = Vector2.ONE * (float(zoom) if zoom.is_valid_float() else 2.0)
	camera.make_current()
	for i in 8:
		await process_frame
	var out := OS.get_environment("SOUL_INVASION_SCREENSHOT")
	if not out.is_empty():
		viewport.get_texture().get_image().save_png(out)
	quit()


static func _terrain(map: Dictionary, x: int, y: int) -> String:
	if x < 0 or y < 0 or x >= map["width"] or y >= map["height"]:
		return "grass"
	return TERRAINS.get(map["values"][y * map["width"] + x], "grass")


## A plain PGM (P2): width, height and the values.
static func _read_pgm(path: String) -> Dictionary:
	if path.is_empty() or not FileAccess.file_exists(path):
		return {}
	var text := FileAccess.get_file_as_string(path)
	var parts: PackedStringArray = []
	for line in text.split("\n"):
		if line.begins_with("#"):
			continue
		for token in line.split(" ", false):
			parts.append(token)
	if parts.size() < 4 or parts[0] != "P2":
		return {}
	var width := int(parts[1])
	var height := int(parts[2])
	var values := PackedInt32Array()
	values.resize(width * height)
	for i in width * height:
		values[i] = int(parts[4 + i])
	return {"width": width, "height": height, "values": values}
