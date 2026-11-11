class_name PerfOverlay
extends CanvasLayer
## Development only, when the server runs with --dev: what a frame costs,
## to measure before optimising and to say "it is slow here" with a
## number (D39). F3 shows or hides it; to see what a part costs the
## graphics card: F4 turns the wind in the trees off and on, F5 draws the
## ground a plain colour, F6 hides the trees, F7 draws the fog and the far
## view as rings, as full quads or not at all.

## How often the numbers are refreshed, in seconds.
const REFRESH := 0.25

## The world view (WorldView), for its counts and the wind; untyped so the
## overlay stands alone in the tests.
var world: Node
var _label: Label
var _enabled := false
var _wind := true
var _plain_ground := false
var _trees_shown := true
var _since := 0.0
## The longest frame since the last refresh, in milliseconds, and when
## the last frame ended.
var _worst_ms := 0.0
var _last_frame_usec := 0
## Frames since the last refresh.
var _frames := 0


func _ready() -> void:
	layer = 110  # above the loading screen: loads are measured too
	visible = false
	var panel := PanelContainer.new()
	panel.position = Vector2(12, 40)
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0, 0, 0, 0.6)
	style.set_content_margin_all(8)
	panel.add_theme_stylebox_override("panel", style)
	_label = Label.new()
	_label.add_theme_font_size_override("font_size", 13)
	panel.add_child(_label)
	add_child(panel)


## The development tools are on (the server sent its overview): F3 and F4
## work from now on.
func enable() -> void:
	_enabled = true


func _unhandled_input(event: InputEvent) -> void:
	if not _enabled or not event is InputEventKey or not event.pressed or event.echo:
		return
	if event.keycode == KEY_F3:
		visible = not visible
		DevTiming.enabled = visible
		DevTiming.take()
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F5:
		_plain_ground = not _plain_ground
		ChunkView.set_plain_ground(_plain_ground)
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F6:
		_trees_shown = not _trees_shown
		if world != null:
			world.call("set_trees_shown", _trees_shown)
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F4:
		_wind = not _wind
		if world != null:
			world.call("set_wind", _wind)
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F7:
		FarView.drawing = (FarView.drawing + 1) % FarView.DRAWING_NAMES.size()
		get_tree().call_group("far_views", "redraw_fog")
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	# From the clock, not `delta`: the engine caps `delta` at a few physics
	# steps (133 ms), which hid every frame over that (10 November 2026).
	var now := Time.get_ticks_usec()
	if _last_frame_usec > 0:
		_worst_ms = maxf(_worst_ms, float(now - _last_frame_usec) / 1000.0)
	_last_frame_usec = now
	_since += delta
	_frames += 1
	if not visible or _since < REFRESH:
		return
	_label.text = report(_worst_ms) + "\n" + breakdown(DevTiming.take(), _frames)
	_since = 0.0
	_frames = 0
	_worst_ms = 0.0


## Where the script time went since the last refresh (DevTiming), a
## frame on average, the heaviest first; `parts` is DevTiming.take().
func breakdown(parts: Dictionary, frames: int) -> String:
	var per := maxf(float(frames), 1.0)
	var names := parts.keys()
	names.sort_custom(func(a: String, b: String) -> bool: return parts[a][0] > parts[b][0])
	var measured := 0.0
	for part: String in names:
		measured += parts[part][0]
	var lines: Array[String] = ["-- scripts measured: %.1f ms a frame --" % (measured / per)]
	for part: String in names:
		var ms: float = parts[part][0]
		var times: int = parts[part][1]
		if ms > 0.0:
			lines.append("%s %.1f ms (%d, %.1f ms each)" % [part, ms / per, times, ms / maxf(times, 1)])
		else:
			lines.append("%s: %d" % [part, times])
	return "\n".join(lines)


## The lines shown, from the engine's monitors and the world's counts.
func report(worst_ms: float) -> String:
	var lines: Array[String] = []
	lines.append("%d fps   worst frame %.1f ms" % [Engine.get_frames_per_second(), worst_ms])
	if world != null:
		lines.append("zoom %s" % String.num(float(world.call("zoom_level")), 2))
	# The engine's process step holds the scripts and also the waits (for
	# the graphics card, or the system holding back a hidden window): what
	# the scripts themselves cost is in the breakdown below.
	lines.append("frame step %.1f ms (scripts and waits)" % [
		Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0,
	])
	lines.append("draw calls %d   canvas items %d" % [
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_DRAW_CALLS_IN_FRAME),
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_OBJECTS_IN_FRAME),
	])
	lines.append("nodes %d   video memory %d MB   memory %d MB" % [
		Performance.get_monitor(Performance.OBJECT_NODE_COUNT),
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_VIDEO_MEM_USED) / 1048576,
		Performance.get_monitor(Performance.MEMORY_STATIC) / 1048576,
	])
	if world != null:
		var counts: Dictionary = world.call("counts")
		lines.append("chunks %d (%d to build, %d parked, %d stale)   trees %d (%d drawn)   bodies %d" % [
			counts.get("chunks", 0), ChunkView.waiting.size(), counts.get("parked", 0), counts.get("stale", 0),
			counts.get("trees", 0), counts.get("trees_drawn", 0), counts.get("bodies", 0),
		])
	# The tree images in memory follow the kinds the chunks held have
	# (Trees.hold): which, what they take (BC7 with mipmaps, worked out from
	# their size) against every texture's, how many came and went.
	var kinds := Trees.memory()
	var bytes := 0
	for species: String in kinds:
		bytes += kinds[species]
	var names := kinds.keys()
	names.sort()
	lines.append("tree images %d kinds, %d MB of textures %d MB   loaded %d, freed %d%s" % [
		kinds.size(), bytes / 1048576,
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TEXTURE_MEM_USED) / 1048576,
		Trees.images_loaded, Trees.images_freed,
		"" if Trees.loading().is_empty() else "   reading %s" % " ".join(Trees.loading()),
	])
	lines.append("  " + " ".join(names))
	lines.append("bank fields smoothing %d   F3 hides" % ShoreField.busy)
	lines.append("wind %s (F4)   ground %s (F5)   trees %s (F6)   fog %s (F7)" % [
		"on" if _wind else "off", "plain" if _plain_ground else "shaded", "shown" if _trees_shown else "hidden",
		FarView.drawing_name(),
	])
	return "\n".join(lines)
