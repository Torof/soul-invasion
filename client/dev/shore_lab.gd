extends SceneTree
## Shore bench: renders a fixed little map with the real ChunkView and
## saves a screenshot, so a piece and its joins can be checked without a
## server or a person. SOUL_INVASION_LAB names the map (straight, step,
## stairs, notch), SOUL_INVASION_SCREENSHOT the file.
##   godot --path client -s dev/shore_lab.gd

const SIZE := 12


func _initialize() -> void:
	var which := OS.get_environment("SOUL_INVASION_LAB")
	var tiles: Array = []
	for y in SIZE:
		for x in SIZE:
			tiles.append("water" if _is_water(which, x, y) else "grass")
	var world := Node2D.new()
	root.add_child(world)
	var view := ChunkView.new()
	view.z_index = -1
	world.add_child(view)
	view.show_chunk(Vector2i.ZERO, SIZE, tiles, [], {})
	var camera := Camera2D.new()
	world.add_child(camera)
	camera.position = Iso.tile_to_screen(SIZE / 2, SIZE / 2)
	var zoom := OS.get_environment("SOUL_INVASION_LAB_ZOOM")
	camera.zoom = Vector2.ONE * (float(zoom) if zoom.is_valid_float() else 1.6)
	camera.make_current()
	_shoot(OS.get_environment("SOUL_INVASION_SCREENSHOT"))


func _is_water(which: String, x: int, y: int) -> bool:
	match which:
		"step":
			return x >= (8 if y < 6 else 6)
		"stairs":
			return x + y >= SIZE
		"notch":
			return x >= 7 or (x >= 5 and y >= 5 and y <= 6)
		_:
			return x >= 6 and x <= 8


func _shoot(path: String) -> void:
	for i in 12:
		await process_frame
	if not path.is_empty():
		root.get_viewport().get_texture().get_image().save_png(path)
	quit()
