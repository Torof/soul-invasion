extends Control
## Main scene: the world view, the head-up display, a help line and the
## tooltip for whatever is under the mouse.
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing.

@onready var _world: WorldView = $WorldView
@onready var _tooltip: PanelContainer = %Tooltip
@onready var _tooltip_text: Label = %Text


func _ready() -> void:
	_world.hovered.connect(_on_hovered)
	Net.connect_to()
	# Development aid: with SOUL_INVASION_SCREENSHOT set to a path, save what
	# the window shows after three seconds and quit (headless runs cannot
	# render, so this is how a change is looked at without a person).
	var shot := OS.get_environment("SOUL_INVASION_SCREENSHOT")
	if not shot.is_empty():
		_screenshot_then_quit(shot)


func _screenshot_then_quit(path: String) -> void:
	await get_tree().create_timer(3.0).timeout
	get_viewport().get_texture().get_image().save_png(path)
	get_tree().quit()


func _on_hovered(text: String) -> void:
	_tooltip.visible = not text.is_empty()
	_tooltip_text.text = text


func _process(_delta: float) -> void:
	if _tooltip.visible:
		# Beside the cursor, kept inside the window.
		var at := get_viewport().get_mouse_position() + Vector2(16, 16)
		var limit := get_viewport_rect().size - _tooltip.size - Vector2(8, 8)
		_tooltip.position = Vector2(minf(at.x, limit.x), minf(at.y, limit.y))
