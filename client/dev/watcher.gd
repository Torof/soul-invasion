class_name DevWatcher
extends Node
## Development aid, never active in play: with SOUL_INVASION_WATCH set,
## the ground shader and the ground textures are reloaded into the
## running client the second their files change, so a change is seen
## without a restart (what nodemon does for a Node server). Scripts and
## scenes still need a restart. The server is not concerned: it knows no
## image, it only names terrains.

## How often the files are looked at, in seconds.
const PERIOD := 1.0
## A screenshot request: drop this file (empty, or "x,y" to frame that
## tile, or "x,y,zoom") and the running client saves what it shows to
## SHOT and removes the request. One client open for everyone, no other
## instance to launch.
const REQUEST := "res://.dev/shot.request"
const SHOT := "res://.dev/shot.png"

## path -> last modification time seen
var _stamps: Dictionary = {}


func _ready() -> void:
	_poll()
	var timer := Timer.new()
	timer.wait_time = PERIOD
	timer.timeout.connect(_poll)
	add_child(timer)
	timer.start()
	print("watching the ground shader and textures: a saved change is reloaded live")


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
