class_name LoadingScreen
extends CanvasLayer
## Covers the game while the valley arrives and is built (LoadingProgress):
## a little soul bouncing and blinking, the name of the game, a bar and a
## silly line now and then; it fades out once everything is in. Shown
## again if the connection is lost before.

const BACKGROUND := Color(0.07, 0.09, 0.11)
const SOUL := Color(0.80, 0.95, 1.0)
const GLOW := Color(0.55, 0.85, 1.0, 0.05)
const CHEEK := Color(1.0, 0.55, 0.65, 0.55)
const INK := Color(0.10, 0.12, 0.16)
const TEXT := Color(0.86, 0.90, 0.92)
const FAINT := Color(0.55, 0.62, 0.66)
## Lines shown in turn, LINE_SECONDS each.
const LINES := [
	"Counting the blades of grass",
	"Teaching the willows to sway",
	"Filling the rivers, one bucket at a time",
	"Hiding berries at the forest's edge",
	"Convincing the souls to come back",
	"Sorting pebbles by roundness",
	"Waking up the brooks",
	"Untangling the roots",
]
const LINE_SECONDS := 2.4
const FADE_SECONDS := 0.45

var progress := LoadingProgress.new()
## After a development teleport, the world is known already: only the
## ground there is awaited.
var _after_teleport := false
var _canvas: Control
var _time := 0.0
var _fade := 0.0
var _finished := false


func _init(after_teleport := false) -> void:
	_after_teleport = after_teleport
	progress.has_world = after_teleport


func _ready() -> void:
	layer = 100
	_canvas = Control.new()
	_canvas.set_anchors_preset(Control.PRESET_FULL_RECT)
	_canvas.mouse_filter = Control.MOUSE_FILTER_STOP
	_canvas.draw.connect(_paint)
	add_child(_canvas)
	Net.world_state.connect(func(_w: int, _h: int, _c: int, _m: Array, _n: bool) -> void: progress.has_world = true)
	Net.chunk.connect(func(_x: int, _y: int, _t: Array, _b: Array, _i: Array, _tr: Array, _cu: Array, _pl: Array) -> void:
		progress.chunk_came(Time.get_ticks_msec()))
	Net.disconnected.connect(_on_disconnected)


func _on_disconnected(_reason: String) -> void:
	if _finished:
		return
	progress = LoadingProgress.new()
	progress.has_world = _after_teleport


func _process(delta: float) -> void:
	_time += delta
	if not _finished:
		# Built: what came in, less the chunks still waiting to be built
		# (after a teleport, the chunks held before are built already).
		var unbuilt := 0
		for chunk in get_tree().get_nodes_in_group("chunk_views"):
			if not (chunk as ChunkView).is_built():
				unbuilt += 1
		progress.built = maxi(progress.received - unbuilt, 0)
		progress.fields = ShoreField.busy
		# And the tree images being read (Trees.poll), so no tree shows up
		# a moment after the ground.
		if progress.done(Time.get_ticks_msec()) and Trees.loading().is_empty():
			_finished = true
	else:
		_fade += delta / FADE_SECONDS
		if _fade >= 1.0:
			queue_free()
			return
	_canvas.modulate.a = 1.0 - _fade
	_canvas.mouse_filter = Control.MOUSE_FILTER_IGNORE if _finished else Control.MOUSE_FILTER_STOP
	_canvas.queue_redraw()


func _paint() -> void:
	var size := _canvas.size
	_canvas.draw_rect(Rect2(Vector2.ZERO, size), BACKGROUND)
	var centre := Vector2(size.x * 0.5, size.y * 0.36)
	var unit := clampf(minf(size.x, size.y) / 720.0, 0.6, 2.0)
	draw_soul(_canvas, centre, unit * 46.0, _time)
	var font := ThemeDB.fallback_font
	var title := "SOUL INVASION"
	var title_size := int(34 * unit)
	var title_width := font.get_string_size(title, HORIZONTAL_ALIGNMENT_LEFT, -1, title_size).x
	_canvas.draw_string(font, Vector2(centre.x - title_width / 2.0, centre.y + 175.0 * unit), title, HORIZONTAL_ALIGNMENT_LEFT, -1, title_size, TEXT)
	# The bar.
	var bar := Rect2(centre.x - 160.0 * unit, centre.y + 200.0 * unit, 320.0 * unit, 6.0 * unit)
	_canvas.draw_rect(bar, Color(1, 1, 1, 0.08))
	var shown := bar
	shown.size.x *= progress.fraction()
	_canvas.draw_rect(shown, SOUL)
	var dots := ".".repeat(int(_time * 2.5) % 4)
	_write(font, progress.stage() + dots, Vector2(centre.x, bar.end.y + 30.0 * unit), int(16 * unit), TEXT)
	var line: String = LINES[int(_time / LINE_SECONDS) % LINES.size()]
	_write(font, line + "…", Vector2(centre.x, bar.end.y + 56.0 * unit), int(14 * unit), FAINT)


func _write(font: Font, text: String, centre: Vector2, font_size: int, colour: Color) -> void:
	var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	_canvas.draw_string(font, Vector2(centre.x - width / 2.0, centre.y), text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, colour)


## The little soul: a round head and a wavy tail, bouncing with a squash
## at the bottom, blinking now and then, its shadow on the ground below;
## `radius` its head's, `time` in seconds.
static func draw_soul(canvas: CanvasItem, at: Vector2, radius: float, time: float) -> void:
	var cycle := fposmod(time, 0.9) / 0.9
	var height := 4.0 * cycle * (1.0 - cycle)  # 0 at the ground, 1 at the top
	var squash := 1.0 - 0.18 * pow(1.0 - height, 6.0)
	var lift := radius * 1.4 * height
	var centre := at - Vector2(0, lift)
	var sway := sin(time * 3.1) * 0.12
	# Shadow, smaller as it rises.
	var shadow_scale := 1.0 - 0.45 * height
	var ground := at + Vector2(0, radius * 2.5)
	var shadow := PackedVector2Array()
	for i in 24:
		var a := TAU * i / 24.0
		shadow.append(ground + Vector2(cos(a) * radius * 0.9, sin(a) * radius * 0.22) * shadow_scale)
	canvas.draw_colored_polygon(shadow, Color(0, 0, 0, 0.35 * shadow_scale))
	# Glow, soft: many faint discs, the smaller over the larger.
	for k in 10:
		canvas.draw_circle(centre + Vector2(0, radius * 0.4), radius * (2.4 - 0.13 * k), GLOW)
	# Body: the upper half of a circle, then a tail that waves to a point.
	var body := PackedVector2Array()
	var wide := radius / squash
	var tall := radius * squash
	for i in 17:
		var a := PI + PI * i / 16.0
		body.append(centre + Vector2(cos(a) * wide, sin(a) * tall))
	var steps := 14
	for i in steps + 1:
		var t := float(i) / steps
		var y := tall * 2.1 * t
		var w := wide * (1.0 - pow(t, 1.6))
		var wave := sin(time * 6.0 - t * 4.5) * radius * 0.45 * t * t
		body.append(centre + Vector2(w + wave + sway * radius * t, y))
	for i in range(steps, -1, -1):
		var t := float(i) / steps
		var y := tall * 2.1 * t
		var w := wide * (1.0 - pow(t, 1.6))
		var wave := sin(time * 6.0 - t * 4.5) * radius * 0.45 * t * t
		body.append(centre + Vector2(-w + wave + sway * radius * t, y))
	canvas.draw_colored_polygon(body, SOUL)
	body.append(body[0])
	canvas.draw_polyline(body, INK, maxf(radius * 0.06, 1.5), true)
	# Eyes, blinking about every three seconds; a small mouth open at the top.
	var blink := fposmod(time, 3.2) < 0.12
	var eye_y := centre.y - tall * 0.15
	for side: float in [-1.0, 1.0]:
		var eye := Vector2(centre.x + side * wide * 0.36 + sway * radius * 0.3, eye_y)
		if blink:
			canvas.draw_line(eye - Vector2(radius * 0.12, 0), eye + Vector2(radius * 0.12, 0), INK, maxf(radius * 0.06, 1.5))
		else:
			canvas.draw_circle(eye, radius * 0.13, INK)
			canvas.draw_circle(eye + Vector2(radius * 0.04, -radius * 0.05), radius * 0.04, Color.WHITE)
	for side: float in [-1.0, 1.0]:
		canvas.draw_circle(Vector2(centre.x + side * wide * 0.58 + sway * radius * 0.3, eye_y + radius * 0.24), radius * 0.12, CHEEK)
	var mouth := Vector2(centre.x + sway * radius * 0.3, centre.y + tall * 0.2)
	canvas.draw_circle(mouth, radius * (0.05 + 0.08 * height), INK)
