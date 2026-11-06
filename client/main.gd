extends Control
## Main scene: the world view, the head-up display, the inventory, a help
## line, the tooltip for whatever is under the mouse, and the context
## menu of what the body could try on a tile.
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing: the server judges.

@onready var _world: WorldView = $WorldView
@onready var _tooltip: PanelContainer = %Tooltip
@onready var _tooltip_text: Label = %Text
@onready var _actions: PopupMenu = %Actions

var _offered: Array = []


func _ready() -> void:
	_world.hovered.connect(_on_hovered)
	_world.acts_requested.connect(_on_acts_requested)
	_actions.id_pressed.connect(_on_act_chosen)
	Net.connect_to()
	# Development aid: with SOUL_INVASION_SCREENSHOT set to a path, save what
	# the window shows after three seconds and quit (headless runs cannot
	# render, so this is how a change is looked at without a person).
	# Development aid: with SOUL_INVASION_WATCH set, the ground shader and
	# textures are reloaded live when their files change (dev/watcher.gd).
	if not OS.get_environment("SOUL_INVASION_WATCH").is_empty():
		add_child(DevWatcher.new())
	var shot := OS.get_environment("SOUL_INVASION_SCREENSHOT")
	if not shot.is_empty():
		var zoom := OS.get_environment("SOUL_INVASION_ZOOM")
		if zoom.is_valid_float():
			_world.get_node("Camera").zoom = Vector2.ONE * float(zoom)
		_screenshot_then_quit(shot)


func _screenshot_then_quit(path: String) -> void:
	await get_tree().create_timer(3.0).timeout
	get_viewport().get_texture().get_image().save_png(path)
	get_tree().quit()


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
