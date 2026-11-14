class_name StepsTest
extends Node2D
## The slope test (`--world test:...`, the mountain test's rule, 13
## November 2026), on only in a world whose chunks carry heights above
## zero: key 1 draws the slope's grass (the ground shader), key 0 the flat
## ground, to compare; a line on screen says which, and the altitude of
## the tile the piloted body stands on (the project lead's ask, 13
## November 2026), with its trend over the last few seconds, the height of
## the tile under the mouse against the body's, and whether the body
## stands on a summit or a col. A developer's readout: the exact altitude
## is not for the game (the project lead's call, 13 November 2026). Never
## in the game.

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
const MINUS := "\u2212"

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
	_label.position = Vector2(12, 12)
	_label.add_theme_color_override("font_color", Color.WHITE)
	_label.add_theme_color_override("font_outline_color", Color.BLACK)
	_label.add_theme_constant_override("outline_size", 4)
	layer.add_child(_label)
	_altitude = Label.new()
	_altitude.position = Vector2(12, 40)
	_altitude.add_theme_color_override("font_color", Color.WHITE)
	_altitude.add_theme_color_override("font_outline_color", Color.BLACK)
	_altitude.add_theme_constant_override("outline_size", 6)
	_altitude.add_theme_font_size_override("font_size", 26)
	layer.add_child(_altitude)
	_apply()


func _process(_delta: float) -> void:
	# Duck-typed: naming WorldView would tie this script to the Net
	# autoload, which the headless test runner lacks.
	var view := get_parent() as Node2D
	if view == null or not view.has_method("piloted_tile"):
		return
	var tile: Vector2i = view.call("piloted_tile")
	var metres := HeightIndex.level(tile) if tile.x >= 0 else -1.0
	if metres < 0.0:
		_trend.clear()
		_last_tile = tile
		_set_text("")
		return
	if tile != _last_tile:
		_last_tile = tile
		_landform = _landform_at(tile, metres)
	var lines := PackedStringArray([_altitude_line(metres)])
	var hover := Iso.screen_to_tile(view.get_local_mouse_position())
	var known: bool = view.call("terrain_at", hover) != ""
	var there := HeightIndex.level(hover) if known else -1.0
	if there >= 0.0:
		lines.append("Curseur : %s m" % _signed(roundi(there - metres)))
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
## ring of RING_TILES around it.
static func _landform_at(tile: Vector2i, metres: float) -> String:
	var ring := PackedFloat32Array()
	for i in RING_DIRECTIONS:
		var angle := TAU * i / RING_DIRECTIONS
		var at := tile + Vector2i(roundi(RING_TILES * cos(angle)), roundi(RING_TILES * sin(angle)))
		ring.append(HeightIndex.level(at))
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
	if key == null or not key.pressed or key.echo:
		return
	if key.keycode == KEY_K:
		ChunkView.slope_break_turn += 15.0
		if ChunkView.slope_break_turn > 90.0:
			ChunkView.slope_break_turn = -90.0
		ChunkView.set_slope_param("slope_break_turn", ChunkView.slope_break_turn)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_G:
		ChunkView.slope_turf_with_bushes = not ChunkView.slope_turf_with_bushes
		ChunkView.set_slope_param("slope_turf_with_bushes", ChunkView.slope_turf_with_bushes)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_L:
		ChunkView.slope_layers = not ChunkView.slope_layers
		ChunkView.set_slope_param("slope_layers", ChunkView.slope_layers)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_R:
		ChunkView.slope_rim = not ChunkView.slope_rim
		ChunkView.set_slope_param("slope_rim", ChunkView.slope_rim)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_X:
		ChunkView.slope_crest_square = not ChunkView.slope_crest_square
		ChunkView.set_slope_param("slope_crest_square", ChunkView.slope_crest_square)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_N:
		var steps: Array[float] = [100.0, 120.0, 140.0, 160.0]
		var at := steps.find(ChunkView.slope_crest_degrees)
		ChunkView.slope_crest_degrees = steps[(at + 1) % steps.size()]
		ChunkView.set_slope_param("slope_crest_cos", cos(deg_to_rad(ChunkView.slope_crest_degrees)))
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_C:
		ChunkView.slope_chevron_flip = not ChunkView.slope_chevron_flip
		ChunkView.set_slope_param("slope_chevron_flip", ChunkView.slope_chevron_flip)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_B:
		ChunkView.set_slope_seams_soft(not ChunkView.slope_seams_soft)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_V:
		ChunkView.set_slope_zones(not ChunkView.slope_zones)
		_apply()
		get_viewport().set_input_as_handled()
		return
	if key.keycode == KEY_T:
		ChunkView.set_slope_fall_line(not ChunkView.slope_fall_line)
		_apply()
		get_viewport().set_input_as_handled()
		return
	var chosen := key.keycode - KEY_0
	if chosen < 0 or chosen >= MODES.size():
		return
	mode = chosen
	_apply()
	get_viewport().set_input_as_handled()


func _apply() -> void:
	ChunkView.set_steps_mode(mode)
	var look := "fall-line rule" if ChunkView.slope_fall_line else "earlier rule"
	var zones := "five grasses" if ChunkView.slope_zones else "your grass alone"
	var seams := "soft seams" if ChunkView.slope_seams_soft else "sharp seams"
	var crests := ("side rule" + (", chevron flipped" if ChunkView.slope_chevron_flip else "")) if ChunkView.slope_crest_square else "earlier sides"
	crests += ", crest from %d°" % roundi(ChunkView.slope_crest_degrees)
	crests += ", rim" if ChunkView.slope_rim else ", no rim"
	crests += ", two layers" if ChunkView.slope_layers else ", one image"
	crests += ", turf with bushes" if ChunkView.slope_turf_with_bushes else ", plain turf"
	crests += ", break below the crest %+d°" % roundi(ChunkView.slope_break_turn)
	_label.text = "Slope test — %s, %s, %s, %s, %s   (keys 1, 0, T, V, B, X, C, N, R, L, G and K)" % [MODES[mode], look, zones, seams, crests]
