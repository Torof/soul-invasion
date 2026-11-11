class_name PerfOverlay
extends CanvasLayer
## Development only, when the server runs with --dev: what a frame costs,
## to measure before optimising and to say "it is slow here" with a
## number (D39). F3 shows or hides it; to see what a part costs the
## graphics card: F4 turns the wind in the trees off and on, F5 draws the
## ground a plain colour, F6 hides the trees, F7 draws the fog and the
## far view as rings, as full quads or not at all, F8 measures what the
## water costs the graphics card, F10 and F11 turn its second layer and
## the current's streaks off and on (WaterMeshes).

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
	elif event.keycode == KEY_F9:
		if _recording_left <= 0.0:
			_start_recording()
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F8:
		if not _measuring:
			measure_water()
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F10:
		_second_layer = not _second_layer
		for material: ShaderMaterial in [ChunkView.water_mesh_material(), ChunkView.water_bank_material()]:
			material.set_shader_parameter("flow_mix", 0.45 if _second_layer else 0.0)
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F11:
		ChunkView.set_water_streaks(not ChunkView.water_streaks)
		get_viewport().set_input_as_handled()


## What the water costs the graphics card (F8): the frame's GPU time
## with the water, then without it, MEASURE_FRAMES frames each after a
## few of settling, twice over, alternated; the medians' difference shown
## in the report. Nothing else should move meanwhile.
const MEASURE_FRAMES := 300
const MEASURE_SETTLE := 10
var _measuring := false
## The water with its second layer (F10).
var _second_layer := true
## [the water's GPU ms, the frame's GPU ms with it, clock MHz], empty
## until measured.
var _water_cost: Array = []


func water_cost() -> Array:
	return _water_cost


func measure_water() -> void:
	_measuring = true
	var rid := get_viewport().get_viewport_rid()
	RenderingServer.viewport_set_measure_render_time(rid, true)
	# Without the vertical sync and the frame cap meanwhile: waiting for
	# the screen stalls the graphics card and lowers its clock, and the
	# measure would read the wait.
	var vsync := DisplayServer.window_get_vsync_mode()
	var max_fps := Engine.max_fps
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	var with_water: Array[float] = []
	var without: Array[float] = []
	for pass_n in 2:
		for hidden in [false, true] if pass_n == 0 else [true, false]:
			get_tree().call_group("chunk_views", "set_water_hidden", hidden)
			for i in MEASURE_SETTLE:
				await RenderingServer.frame_post_draw
			for i in MEASURE_FRAMES:
				await RenderingServer.frame_post_draw
				var ms := RenderingServer.viewport_get_measured_render_time_gpu(rid)
				if hidden:
					without.append(ms)
				else:
					with_water.append(ms)
	get_tree().call_group("chunk_views", "set_water_hidden", false)
	DisplayServer.window_set_vsync_mode(vsync)
	Engine.max_fps = max_fps
	var base := _median(with_water)
	_water_cost = [base - _median(without), base, gpu_clock_mhz()]
	_measuring = false


static func _median(values: Array[float]) -> float:
	if values.is_empty():
		return 0.0
	values.sort()
	return values[values.size() / 2]


## The graphics card's clock, in MHz, where Linux tells it; 0 elsewhere.
static func gpu_clock_mhz() -> int:
	for card in ["card0", "card1", "card2"]:
		var path := "/sys/class/drm/%s/gt_act_freq_mhz" % card
		if FileAccess.file_exists(path):
			var text := FileAccess.get_file_as_string(path).strip_edges()
			if text.is_valid_int() and int(text) > 0:
				return int(text)
	return 0


## Recording (F9): RECORD_S seconds of frames, one line each, written to
## user://frames-<time>.csv for the AI to read: what a frame cost on the
## lead's own machine, where his eye sees a jump.
const RECORD_S := 10.0
var _recording_left := 0.0
var _recorded: PackedStringArray = []


func _start_recording() -> void:
	_recording_left = RECORD_S
	DevTiming.enabled = true
	_recorded = PackedStringArray(["t_ms,frame_ms,process_ms,scripts_ms,heaviest,heaviest_ms,gpu_ms,draw_calls,items,zoom"])
	DevTiming.take()
	RenderingServer.viewport_set_measure_render_time(get_viewport().get_viewport_rid(), true)


func _record_frame(now: int, frame_ms: float) -> void:
	var parts := DevTiming.take()
	var total := 0.0
	var heaviest := ""
	var heaviest_ms := 0.0
	for part: String in parts:
		var ms: float = parts[part][0]
		total += ms
		if ms > heaviest_ms:
			heaviest_ms = ms
			heaviest = part
	var zoom := float(world.call("zoom_level")) if world != null else 0.0
	_recorded.append("%d,%.2f,%.2f,%.2f,%s,%.2f,%.2f,%d,%d,%.2f" % [
		now / 1000, frame_ms, Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0, total,
		heaviest.replace(",", " "), heaviest_ms,
		RenderingServer.viewport_get_measured_render_time_gpu(get_viewport().get_viewport_rid()),
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_DRAW_CALLS_IN_FRAME),
		RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_OBJECTS_IN_FRAME), zoom])


func _finish_recording() -> void:
	var path := "user://frames-%s.csv" % Time.get_datetime_string_from_system().replace(":", "-")
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file != null:
		file.store_string("\n".join(_recorded) + "\n")
		file.close()
	print("frames recorded: ", ProjectSettings.globalize_path(path))
	DevTiming.enabled = visible
	_recorded = PackedStringArray()


func _process(delta: float) -> void:
	# From the clock, not `delta`: the engine caps `delta` at a few physics
	# steps (133 ms), which hid every frame over that (10 November 2026).
	var now := Time.get_ticks_usec()
	if _last_frame_usec > 0:
		_worst_ms = maxf(_worst_ms, float(now - _last_frame_usec) / 1000.0)
		if _recording_left > 0.0:
			_record_frame(now, float(now - _last_frame_usec) / 1000.0)
			_recording_left -= delta
			if _recording_left <= 0.0:
				_finish_recording()
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
	lines.append("%d fps   worst frame %.1f ms%s" % [Engine.get_frames_per_second(), worst_ms,
		("   RECORDING %.0f s (F9)" % _recording_left) if _recording_left > 0.0 else "   F9 records 10 s"])
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
		lines.append("chunks %d held, %d built (%d to build, %d leaving, %d stale)   trees %d (%d drawn)   bodies %d" % [
			counts.get("chunks", 0), counts.get("built", 0), ChunkView.waiting.size(), counts.get("leaving", 0), counts.get("stale", 0),
			counts.get("trees", 0), counts.get("trees_drawn", 0), counts.get("bodies", 0),
		])
	# The tree images in memory follow the kinds the chunks held have
	# (Trees.hold): which (* with their finest level, Trees.fine), what they
	# take (the BC7 levels held) against every texture's, how many came
	# and went.
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
		("" if Trees.loading().is_empty() else "   reading %s" % " ".join(Trees.loading()))
		+ ("" if Trees.sharpening().is_empty() else "   sharpening %s" % " ".join(Trees.sharpening())),
	])
	for i in names.size():
		if Trees.fine(names[i]):
			names[i] = names[i] + "*"
	lines.append("  " + " ".join(names))
	lines.append("grounds on the workers %d   built in line: %d   laid %d, %.2f ms each on the main thread   F3 hides" % [
		ChunkView.waiting.size(), ChunkView.in_line, ChunkView.lays, ChunkView.lay_usec / 1000.0 / maxi(ChunkView.lays, 1)])
	lines.append("wind %s (F4)   ground %s (F5)   trees %s (F6)   fog %s (F7)" % [
		"on" if _wind else "off", "plain" if _plain_ground else "shaded", "shown" if _trees_shown else "hidden",
		FarView.drawing_name(),
	])
	lines.append("water: second layer %s (F10)   streaks %s (F11)   %s" % [
		"on" if _second_layer else "off", "on" if ChunkView.water_streaks else "off", "measuring..." if _measuring else "F8 measures its GPU time"])
	if not _water_cost.is_empty():
		lines.append("  water %.2f ms GPU (frame %.2f ms, clock %d MHz)" % [_water_cost[0], _water_cost[1], _water_cost[2]])
	lines.append("  meshes %.2f MB on the card (%.2f before packing), built %d: worker %.1f ms each, main thread %.2f ms each" % [
		ChunkView.water_bytes / 1048576.0, ChunkView.water_bytes_before / 1048576.0, ChunkView.water_builds,
		ChunkView.water_build_usec / 1000.0 / maxi(ChunkView.water_builds, 1), ChunkView.water_main_usec / 1000.0 / maxi(ChunkView.water_builds, 1)])
	return "\n".join(lines)
