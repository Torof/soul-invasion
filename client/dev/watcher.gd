class_name DevWatcher
extends Node
## Development aid, never active in play: with SOUL_INVASION_WATCH set,
## the ground shader and the ground textures are reloaded into the
## running client the second their files change, so a change is seen
## without a restart (what nodemon does for a Node server). Any other file
## of the client (scripts, scenes, shaders, images, data) changed makes
## the client import the project again and start afresh by itself, a few
## seconds later: a script cannot be swapped into a running game outside
## the editor. A server restarted by `cargo watch` needs nothing: the
## client reconnects by itself.

## How often the files are looked at, in seconds.
const PERIOD := 1.0
## A screenshot request: drop this file (empty, or "x,y" to frame that
## tile, or "x,y,zoom") and the running client saves what it shows to
## SHOT and removes the request. One client open for everyone, no other
## instance to launch.
const REQUEST := "res://.dev/shot.request"
const SHOT := "res://.dev/shot.png"

## Files whose change restarts the client, by extension.
const RESTART_ON := ["gd", "tscn", "tres", "gdshader", "png", "jpg", "webp", "json", "godot", "ttf", "otf"]
## Folders never looked into: Godot's own cache, this watcher's requests,
## the tests.
const SKIP_DIRS := [".godot", ".dev", "tests"]
## Seconds a change must have settled before the restart: a pull or an
## edit across several files is taken in once.
const SETTLE := 1.5

## path -> last modification time seen
var _stamps: Dictionary = {}
## Every client file watched for a restart -> its time, at the last look.
var _files: Dictionary = {}
## Seconds since the last change seen, while a restart waits (-1: none).
var _since_change := -1.0


func _ready() -> void:
	_files = scan()
	_poll()
	var timer := Timer.new()
	timer.wait_time = PERIOD
	timer.timeout.connect(_poll)
	add_child(timer)
	timer.start()
	print("watching the client: the ground reloads live, any other change restarts the client")


## The files watched: the ground shader, the ground spec and every ground
## texture it names.
static func watched() -> Array[String]:
	var paths: Array[String] = ["res://world/ground.gdshader", ChunkView.GROUND_SPEC]
	var spec := ChunkView.read_ground_spec()
	for terrain in spec:
		var path := str(spec[terrain].get("texture", ""))
		if path != "" and not paths.has(path):
			paths.append(path)
		for layer_path in spec[terrain].get("layers", {}).values():
			if not paths.has(layer_path):
				paths.append(str(layer_path))
	return paths


func _poll() -> void:
	var changed := false
	for path in watched():
		var stamp := FileAccess.get_modified_time(path)
		if _stamps.has(path) and _stamps[path] != stamp:
			changed = true
		_stamps[path] = stamp
	# A fresh layer named by the spec is watched from the next poll on.
	if changed:
		ChunkView.reload_art()
		get_tree().call_group("chunk_views", "queue_redraw")
		print("ground reloaded")
	_watch_for_restart()
	if FileAccess.file_exists(REQUEST):
		var framing := FileAccess.get_file_as_string(REQUEST).strip_edges()
		DirAccess.remove_absolute(ProjectSettings.globalize_path(REQUEST))
		_shoot(framing)


## Frames as asked ("x,y" or "x,y,zoom", or nothing), then saves the
## window to SHOT once the frame is drawn.
func _shoot(framing: String) -> void:
	# Untyped on purpose: naming WorldView here would pull the network
	# autoload into the headless tests, which have none.
	var world: Node = get_parent().get_node("WorldView")
	var parts := framing.split(",")
	if parts.size() >= 2 and parts[0].is_valid_int() and parts[1].is_valid_int():
		world.call("look_at_tile", Vector2i(int(parts[0]), int(parts[1])))
	if parts.size() >= 3 and parts[2].is_valid_float():
		world.get_node("Camera").zoom = Vector2.ONE * float(parts[2])
	await get_tree().process_frame
	await get_tree().process_frame
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://.dev"))
	get_viewport().get_texture().get_image().save_png(ProjectSettings.globalize_path(SHOT))
	print("screenshot saved to ", SHOT)


## Every file of the client a change of which restarts it, with its
## modification time: the ground's own files are reloaded live instead.
static func scan() -> Dictionary:
	var found := {}
	var live := watched()
	var folders: Array[String] = ["res://"]
	while not folders.is_empty():
		var folder: String = folders.pop_back()
		var dir := DirAccess.open(folder)
		if dir == null:
			continue
		for sub in dir.get_directories():
			if not SKIP_DIRS.has(sub):
				folders.append(folder.path_join(sub))
		for file in dir.get_files():
			var path := folder.path_join(file)
			if RESTART_ON.has(file.get_extension()) and not live.has(path):
				found[path] = FileAccess.get_modified_time(path)
	return found


## Restarts the client once its files have changed and settled.
func _watch_for_restart() -> void:
	var now := scan()
	if now != _files:
		_files = now
		_since_change = 0.0
		return
	if _since_change < 0.0:
		return
	_since_change += PERIOD
	if _since_change >= SETTLE:
		_since_change = -1.0
		restart()


## Imports the project again (new images, changed shaders) and starts a
## fresh client with the same command line and environment, then quits.
func restart() -> void:
	print("the client changed: restarting")
	set_process(false)
	var layer := CanvasLayer.new()
	layer.layer = 128
	var note := Label.new()
	note.text = "The client changed: reloading…"
	note.position = Vector2(24, 64)
	note.add_theme_font_size_override("font_size", 28)
	note.add_theme_color_override("font_outline_color", Color.BLACK)
	note.add_theme_constant_override("outline_size", 6)
	layer.add_child(note)
	add_child(layer)
	await get_tree().process_frame
	await get_tree().process_frame
	var project := ProjectSettings.globalize_path("res://")
	var output: Array = []
	OS.execute(OS.get_executable_path(), ["--headless", "--path", project, "--import"], output)
	# The project named outright: the engine keeps no `--path` among the
	# arguments it hands back, and the client may run from anywhere.
	var args := OS.get_cmdline_args()
	if not args.has("--path"):
		args = PackedStringArray(["--path", project]) + args
	OS.create_instance(args)
	get_tree().quit()
