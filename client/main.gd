extends Control
## Placeholder main scene for milestone 1 ("the skeleton").
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing.


func _ready() -> void:
	var label := Label.new()
	label.text = "Soul Invasion — client skeleton (protocol v%d)" % Protocol.PROTOCOL_VERSION
	label.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	add_child(label)
