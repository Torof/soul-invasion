extends Control
## Main scene: the world view, the head-up display, the inventory, a help
## line, the loading screen, the development map, counters and watchdog
## (--dev), the tooltip for whatever is under the mouse, and the context
## menu of what the body could try on a tile.
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing: the server judges.

@onready var _world: WorldView = $WorldView
@onready var _tooltip: PanelContainer = %Tooltip
@onready var _tooltip_text: Label = %Text
@onready var _actions: PopupMenu = %Actions

var _offered: Array = []
## The development watchdog, once a --dev server is reached.
var _watchdog: Node

## Whether the head-up display (the clock, the body's gauges) and the
## inventory are shown. Off while the world, the creatures and the things
## in it are being designed, before play (the project lead's call, 9
## November 2026): kept working, hidden, for when they are needed.
const STATUS_PANELS_SHOWN := false


func _ready() -> void:
	$Ui/Hud.visible = STATUS_PANELS_SHOWN
	$Ui/Inventory.visible = STATUS_PANELS_SHOWN
	_world.hovered.connect(_on_hovered)
	_world.acts_requested.connect(_on_acts_requested)
	_actions.id_pressed.connect(_on_act_chosen)
	# Development aid: SOUL_INVASION_URL names another server (a test server
	# on another port, beside the one being played).
	var url := OS.get_environment("SOUL_INVASION_URL")
	if url.is_empty():
		Net.connect_to()
	else:
		Net.connect_to(url)
	# Development aid: with SOUL_INVASION_SCREENSHOT set to a path, save what
	# the window shows after three seconds and quit (headless runs cannot
	# render, so this is how a change is looked at without a person).
	# Development aid: with SOUL_INVASION_WATCH set, the ground shader and
	# textures are reloaded live when their files change (dev/watcher.gd).
	if not OS.get_environment("SOUL_INVASION_WATCH").is_empty():
		add_child(DevWatcher.new())
	var shot := OS.get_environment("SOUL_INVASION_SCREENSHOT")
	if not shot.is_empty():
		_screenshot_then_quit(shot)
	else:
		# A loading screen while the valley arrives and is built, and again
		# after a development teleport.
		add_child(LoadingScreen.new())
		_world.teleporting.connect(func() -> void: add_child(LoadingScreen.new(true)))
	# Development (--dev servers): the map of the whole valley, on M.
	var dev_map := DevMap.new()
	dev_map.world = _world
	add_child(dev_map)
	Net.dev_overview.connect(dev_map.show_overview)
	# Development: what a frame costs (F3), the wind on or off (F4).
	var perf := PerfOverlay.new()
	perf.world = _world
	add_child(perf)
	Net.dev_overview.connect(func(_c: int, _w: int, _h: int, _cells: String) -> void: perf.enable())
	# Development: a watchdog that reports a frozen main thread (dev/watchdog.gd).
	Net.dev_overview.connect(func(_c: int, _w: int, _h: int, _cells: String) -> void:
		if _watchdog == null:
			_watchdog = DevWatchdog.new()
			add_child(_watchdog))


func _screenshot_then_quit(path: String) -> void:
	await get_tree().create_timer(2.5).timeout
	# Then until every chunk is built (at most a minute and a half).
	var waited := 0.0
	while waited < 90.0 and (ShoreField.busy > 0 or _unbuilt() > 0):
		await get_tree().create_timer(0.5).timeout
		waited += 0.5
	# Framing for the screenshot, once the world is in: SOUL_INVASION_ZOOM
	# sets the camera zoom, SOUL_INVASION_LOOK_AT="x,y" frames that tile
	# instead of the body.
	var zoom := OS.get_environment("SOUL_INVASION_ZOOM")
	if zoom.is_valid_float():
		_world.get_node("Camera").zoom = Vector2.ONE * float(zoom)
	var look := OS.get_environment("SOUL_INVASION_LOOK_AT").split(",")
	if look.size() == 2 and look[0].is_valid_int() and look[1].is_valid_int():
		_world.look_at_tile(Vector2i(int(look[0]), int(look[1])))
	await get_tree().create_timer(0.5).timeout
	get_viewport().get_texture().get_image().save_png(path)
	get_tree().quit()


func _unbuilt() -> int:
	var count := 0
	for chunk in get_tree().get_nodes_in_group("chunk_views"):
		if not (chunk as ChunkView).is_built():
			count += 1
	return count


func _on_hovered(text: String) -> void:
	_tooltip.visible = not text.is_empty()
	_tooltip_text.text = text


func _on_acts_requested(acts: Array, at_screen: Vector2) -> void:
	_offered = acts
	_actions.clear()
	for i in acts.size():
		_actions.add_item(str(acts[i]["label"]), i)
	_actions.position = Vector2i(at_screen) + Vector2i(get_window().position)
	_actions.popup()


func _on_act_chosen(id: int) -> void:
	if id >= 0 and id < _offered.size():
		var act: Dictionary = _offered[id]
		var send: Callable = act["send"]
		send.call(int(act["item"]))


func _process(_delta: float) -> void:
	if _tooltip.visible:
		# Beside the cursor, kept inside the window.
		var at := get_viewport().get_mouse_position() + Vector2(16, 16)
		var limit := get_viewport_rect().size - _tooltip.size - Vector2(8, 8)
		_tooltip.position = Vector2(minf(at.x, limit.x), minf(at.y, limit.y))

