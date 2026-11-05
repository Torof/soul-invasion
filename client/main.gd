extends Control
## Main scene: the world view, the head-up display and a help line.
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing.


func _ready() -> void:
	Net.connect_to()
