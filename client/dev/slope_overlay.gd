class_name SlopeOverlay
extends Node
## Development: the slopes' readout and dials (D38, D44), made by the main
## scene once a --dev server is reached. It shows nothing until a chunk
## carries heights (D43). Then, a line says how the slopes are drawn and
## the keys: 1 the slopes (the ground shader's one rule), 0 the flat
## ground, to compare; G the turf's first tile alone instead of the family
## of five, to see what the family brings; R the rim; P the turf's size. A second line gives the tints as drawn and their dials
## (the project lead's ask of 15 November 2026, to tune the flat grass
## and the slope turf live and read the numbers off the screen): the
## arrows with Shift turn the flat grass's tint, with Ctrl the slope
## turf's; up and down its brightness, right and left its warmth;
## Backspace puts both back. Arrows with a modifier, so the keys are the
## same on an AZERTY keyboard and need no numpad. Below it, the altitude
## of the tile the piloted body stands
## on (the project lead's ask, 13 November 2026), with its trend over the
## last few seconds, the height of the tile under the mouse against the
## body's, and whether the body stands on a summit or a col. A developer's
## readout: the exact altitude is not for the game (the project lead's
## call, 13 November 2026). Never in play: the client decides nothing
## (D05) and a server without --dev never makes it.

const MODES := ["0: flat (the game's ground)", "1: slopes"]
## The span the trend is read over, in seconds of real time.
const TREND_S := 3.0
## The ring read around the body for summits and cols: how many
## directions, and how far, in tiles.
const RING_DIRECTIONS := 16
const RING_TILES := 8.0
## How much higher or lower a height on the ring must be to count, in
## metres.
const RING_MARGIN_M := 1.0
## The turf's sizes key P cycles, in canvas px a repetition.
const TURF_SIZES: Array[float] = [768.0, 512.0, 384.0, 256.0]
## A step of the tint dials: brightness on the three channels, warmth on
## the red one (down: toward green; up: toward yellow).
const BRIGHTER := 1.05
const DIMMER := 0.95
const WARMER := 1.03
const COOLER := 0.97
const TINT_KEYS: Array[Key] = [KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT]
const MINUS := "−"

## The world view (duck-typed: naming WorldView would tie this script to
## the Net autoload, which the headless test runner lacks).
var world: Node
var mode := 1
var _label: Label
var _altitude: Label
var _last_tile := Vector2i(-2, -2)
## The body's altitude over the last TREND_S seconds, oldest first, as
## (seconds, metres); the first one is the newest at or before the span's
## start.
var _trend: Array[Vector2] = []
## "Sommet", "Col" or "" for the body's tile, worked out when it changes.
var _landform := ""


func _ready() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	_label = Label.new()
	# Below the scene's own help line (main.tscn "Help", 8–32 px), never over it.
	_label.position = Vector2(12, 40)
	_label.add_theme_color_override("font_color", Color.WHITE)
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	layer.add_child(_label)
	_altitude = Label.new()
	# Below the two lines of the dials' label.
	_altitude.position = Vector2(12, 92)
	_altitude.add_theme_color_override("font_color", Color.WHITE)
	_altitude.add_theme_color_override("font_outline_color", Color.BLACK)
	_altitude.add_theme_constant_override("outline_size", 6)
	_altitude.add_theme_font_size_override("font_size", 26)
	layer.add_child(_altitude)
	_apply()


func _process(_delta: float) -> void:
	if not HeightIndex.any:
		_label.visible = false
		return
	if not _label.visible:
		_label.visible = true
		mode = ChunkView.slopes_drawn
		_apply()
	var view := world as Node2D
	if view == null or not view.has_method("piloted_tile"):
		return
	var tile: Vector2i = view.call("piloted_tile")
	if tile.x < 0 or not HeightIndex.known(tile):
		_trend.clear()
		_last_tile = tile
		_set_text("")
		return
	var metres := HeightIndex.level(tile)
	if tile != _last_tile:
		_last_tile = tile
		_landform = _landform_at(tile, metres)
	var lines := PackedStringArray([_altitude_line(metres)])
	var hover := Iso.screen_to_tile(view.get_local_mouse_position())
	if HeightIndex.known(hover):
		lines.append("Curseur : %s m" % _signed(roundi(HeightIndex.level(hover) - metres)))
	if _landform != "":
		lines.append(_landform)
	_set_text("\n".join(lines))


## "Altitude N m", with an arrow and the climb in metres a minute when the
## altitude shown changed over the last TREND_S seconds.
func _altitude_line(metres: float) -> String:
	var now := Time.get_ticks_msec() / 1000.0
	_trend.append(Vector2(now, metres))
	while _trend.size() > 1 and _trend[1].x <= now - TREND_S:
		_trend.remove_at(0)
	var line := "Altitude %d m" % roundi(metres)
	var first := _trend[0]
	var change := roundi(metres) - roundi(first.y)
	if change == 0 or now - first.x < 0.5:
		return line
	var per_minute := roundi((metres - first.y) / (now - first.x) * 60.0)
	return "%s %s %s m/min" % [line, "↑" if change > 0 else "↓", _signed(per_minute)]


func _set_text(text: String) -> void:
	if _altitude.text != text:
		_altitude.text = text


## "Sommet", "Col" or "" for a tile at `metres`, from the heights on a
## ring of RING_TILES around it (an unknown height on the ring is given
## below zero, and says nothing).
static func _landform_at(tile: Vector2i, metres: float) -> String:
	var ring := PackedFloat32Array()
	for i in RING_DIRECTIONS:
		var angle := TAU * i / RING_DIRECTIONS
		var at := tile + Vector2i(roundi(RING_TILES * cos(angle)), roundi(RING_TILES * sin(angle)))
		ring.append(HeightIndex.level(at) if HeightIndex.known(at) else -1.0)
	match classify_ring(metres, ring):
		"summit":
			return "Sommet"
		"col":
			return "Col"
	return ""


## What the heights on a ring around a point (in order around it) make of
## the point at `centre` metres: "summit" when every one is lower by at
## least RING_MARGIN_M; "col" when, leaving out those within RING_MARGIN_M
## of the centre, they fall in exactly two higher arcs and two lower arcs;
## "" otherwise, or when a height is unknown (below zero).
static func classify_ring(centre: float, ring: PackedFloat32Array) -> String:
	if centre < 0.0 or ring.is_empty():
		return ""
	var signs: Array[int] = []
	for h in ring:
		if h < 0.0:
			return ""
		if h >= centre + RING_MARGIN_M:
			signs.append(1)
		elif h <= centre - RING_MARGIN_M:
			signs.append(-1)
	if signs.size() == ring.size() and not signs.has(1):
		return "summit"
	var changes := 0
	for i in signs.size():
		if signs[i] != signs[(i + signs.size() - 1) % signs.size()]:
			changes += 1
	return "col" if changes == 4 else ""


## A whole number with its sign, the minus a true minus: "+14", "−6", "0".
static func _signed(n: int) -> String:
	if n > 0:
		return "+%d" % n
	if n < 0:
		return MINUS + str(-n)
	return "0"


func _unhandled_key_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo or not HeightIndex.any:
		return
	if key.keycode == KEY_G:
		ChunkView.set_slope_turf_single(not ChunkView.slope_turf_single)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_P:
		var at := TURF_SIZES.find(ChunkView.slope_turf_px)
		ChunkView.set_slope_turf_px(TURF_SIZES[(at + 1) % TURF_SIZES.size()])
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_R:
		ChunkView.set_slope_rim(not ChunkView.slope_rim)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_BACKSPACE:
		ChunkView.reset_tints()
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode in TINT_KEYS and (key.shift_pressed or key.ctrl_pressed):
		_turn_tint(key)
		_apply()
		get_viewport().set_input_as_handled()
		return
	var chosen := key.keycode - KEY_0
	if chosen < 0 or chosen >= MODES.size():
		return
	mode = chosen
	ChunkView.set_slopes_drawn(mode)
	_apply()
	get_viewport().set_input_as_handled()


## One arrow with Shift (the flat grass) or Ctrl (the slope turf; it wins
## when both are held): up or down a step of brightness, right or left a
## step of warmth.
func _turn_tint(key: InputEventKey) -> void:
	var tint: Vector3 = ChunkView.slope_tint if key.ctrl_pressed else ChunkView.flat_tint
	match key.keycode:
		KEY_UP:
			tint = brighter(tint, true)
		KEY_DOWN:
			tint = brighter(tint, false)
		KEY_RIGHT:
			tint = warmer(tint, true)
		KEY_LEFT:
			tint = warmer(tint, false)
	if key.ctrl_pressed:
		ChunkView.set_slope_tint(tint)
	else:
		ChunkView.set_flat_tint(tint)


## The tint a step brighter (×BRIGHTER on the three channels) or dimmer.
static func brighter(tint: Vector3, up: bool) -> Vector3:
	return tint * (BRIGHTER if up else DIMMER)


## The tint a step warmer (the red ×WARMER, toward yellow) or cooler
## (toward green).
static func warmer(tint: Vector3, up: bool) -> Vector3:
	return Vector3(tint.x * (WARMER if up else COOLER), tint.y, tint.z)


## "(0.94, 1.02, 0.92)": a tint's three channels to two decimals, as the
## project lead reads them out.
static func tint_words(tint: Vector3) -> String:
	return "(%.2f, %.2f, %.2f)" % [tint.x, tint.y, tint.z]


func _apply() -> void:
	var turf := "turf tile 1 alone" if ChunkView.slope_turf_single else "turf family of %d" % ChunkView.SLOPE_LOOK["slope_turf_variants"]
	var rim := "rim" if ChunkView.slope_rim else "no rim"
	var slopes := "Slopes — %s, %s, %s, turf %d px (P)   (keys 1, 0, G, R and P)" % [MODES[mode], turf, rim, roundi(ChunkView.slope_turf_px)]
	var tints := "Tints — flat %s · slope %s   (Shift+arrows flat, Ctrl+arrows slope: ↑↓ brightness, ←→ warmth; Backspace resets)" % [tint_words(ChunkView.flat_tint), tint_words(ChunkView.slope_tint)]
	_label.text = slopes + "\n" + tints
