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


## The files watched: the ground shader and every ground texture in use.
static func watched() -> Array[String]:
	var paths: Array[String] = ["res://world/ground.gdshader"]
	for path in ChunkView.GROUND_TEXTURES.values():
		if not paths.has(path):
			paths.append(path)
	for layers in ChunkView.LAYER_TEXTURES.values():
		for path in layers.values():
			if not paths.has(path):
				paths.append(path)
	return paths


func _poll() -> void:
	var changed := false
	for path in watched():
		var stamp := FileAccess.get_modified_time(path)
		if _stamps.has(path) and _stamps[path] != stamp:
			changed = true
		_stamps[path] = stamp
	if changed:
		ChunkView.reload_art()
		get_tree().call_group("chunk_views", "queue_redraw")
		print("ground reloaded")
