extends Control
## Main scene: the world view plus a small status line.
##
## The client only displays the world and transmits the player's intentions
## (architecture decision D03). It decides nothing.

@onready var _status: Label = $Status
@onready var _world: WorldView = $WorldView

var _day := 1
var _body := -1
var _state := "connecting…"


func _ready() -> void:
	Net.connected.connect(_on_connected)
	Net.day_started.connect(_on_day_started)
	Net.move_refused.connect(_on_move_refused)
	Net.disconnected.connect(_on_disconnected)
	Net.connect_to()
	_refresh()


func _on_connected(_tick: int, body: int) -> void:
	_body = body
	_state = "connected"
	_refresh()


func _on_day_started(day: int, _tick: int) -> void:
	_day = day
	_refresh()


func _on_move_refused(_body: int, x: int, y: int) -> void:
	_state = "cannot walk to (%d, %d)" % [x, y]
	_refresh()


func _on_disconnected(reason: String) -> void:
	_state = reason
	_refresh()


func _refresh() -> void:
	var who := "body %d" % _body if _body >= 0 else "watching"
	_status.text = (
		"Soul Invasion — day %d — %s — %s — click: walk · wheel: zoom · right-drag: pan · space: follow"
		% [_day, who, _state]
	)
