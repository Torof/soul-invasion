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


func _on_hovered(text: String) -> void:
	_tooltip.visible = not text.is_empty()
	_tooltip_text.text = text


func _process(_delta: float) -> void:
	if _tooltip.visible:
		# Beside the cursor, kept inside the window.
		var at := get_viewport().get_mouse_position() + Vector2(16, 16)
		var limit := get_viewport_rect().size - _tooltip.size - Vector2(8, 8)
		_tooltip.position = Vector2(minf(at.x, limit.x), minf(at.y, limit.y))
