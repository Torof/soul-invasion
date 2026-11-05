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
var _condition := {}
var _night := false


func _ready() -> void:
	Net.connected.connect(_on_connected)
	Net.day_started.connect(_on_day_started)
	Net.move_refused.connect(_on_move_refused)
	Net.body_condition.connect(_on_body_condition)
	Net.body_died.connect(_on_body_died)
	Net.body_assigned.connect(_on_body_assigned)
	Net.eat_refused.connect(func(_body: int, _item: int) -> void: _set_state("nothing to eat there"))
	Net.night_fell.connect(func(_day: int) -> void: _set_night(true))
	Net.disconnected.connect(_on_disconnected)
	Net.connect_to()
	_refresh()


func _on_body_condition(body: int, condition: Dictionary) -> void:
	if body == _body:
		_condition = condition
		_refresh()


func _on_body_died(body: int, cause: String) -> void:
	if body == _body:
		_state = "your body died (%s)" % cause
		_refresh()


func _on_body_assigned(body: int) -> void:
	_body = body
	_condition = {}
	_state = "reborn in a free body" if body >= 0 else "no free body: watching"
	_refresh()


func _set_state(state: String) -> void:
	_state = state
	_refresh()


func _set_night(night: bool) -> void:
	_night = night
	_refresh()


func _on_connected(_tick: int, body: int) -> void:
	_body = body
	_state = "connected"
	_refresh()


func _on_day_started(day: int, _tick: int) -> void:
	_day = day
	_night = false
	_refresh()


func _on_move_refused(_body: int, x: int, y: int) -> void:
	_state = "cannot walk to (%d, %d)" % [x, y]
	_refresh()


func _on_disconnected(reason: String) -> void:
	_state = reason
	_refresh()


func _refresh() -> void:
	var who := "body %d" % _body if _body >= 0 else "watching"
	var when := "night" if _night else "day"
	var gauges := ""
	if not _condition.is_empty():
		gauges = " — vitality %d · hunger %d · cold %d · fatigue %d%s" % [
			int(_condition.get("vitality", 0)),
			int(_condition.get("hunger", 0)),
			int(_condition.get("cold", 0)),
			int(_condition.get("fatigue", 0)),
			" · asleep" if bool(_condition.get("asleep", false)) else "",
		]
	_status.text = (
		"Soul Invasion — day %d (%s) — %s%s — %s\nclick: walk · E: eat · Z: sleep/wake · wheel: zoom · right-drag: pan · space: follow"
		% [_day, when, who, gauges, _state]
	)
