class_name PerfOverlay
extends CanvasLayer
## Development only, when the server runs with --dev: what a frame costs,
## to measure before optimising and to say "it is slow here" with a
## number (D39). F3 shows or hides it; F4 turns the wind in the trees
## off and on, to see what it costs.

## How often the numbers are refreshed, in seconds.
const REFRESH := 0.25

## The world view (WorldView), for its counts and the wind; untyped so the
## overlay stands alone in the tests.
var world: Node
var _label: Label
var _enabled := false
var _wind := true
var _since := 0.0
## The longest frame since the last refresh, in milliseconds.
var _worst_ms := 0.0


func _ready() -> void:
	layer = 60
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
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_F4:
		_wind = not _wind
		if world != null:
			world.call("set_wind", _wind)
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	_worst_ms = maxf(_worst_ms, delta * 1000.0)
	_since += delta
	if not visible or _since < REFRESH:
		return
	_since = 0.0
	_label.text = report(_worst_ms)
	_worst_ms = 0.0


## The lines shown, from the engine's monitors and the world's counts.
func report(worst_ms: float) -> String:
	var lines: Array[String] = []
	lines.append("%d fps   worst frame %.1f ms" % [Engine.get_frames_per_second(), worst_ms])
	lines.append("scripts %.1f ms   physics %.1f ms" % [
		Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0,
		Performance.get_monitor(Performance.TIME_PHYSICS_PROCESS) * 1000.0,
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
		lines.append("chunks %d (%d to build)   trees %d   bodies %d" % [
			counts.get("chunks", 0), ChunkView.waiting.size(), counts.get("trees", 0), counts.get("bodies", 0),
		])
	lines.append("bank fields smoothing %d   wind %s (F4)   F3 hides" % [ShoreField.busy, "on" if _wind else "off"])
	return "\n".join(lines)
