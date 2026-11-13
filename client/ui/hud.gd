class_name Hud
extends PanelContainer
## The head-up display: the clock (day, hour, night, season and year), the piloted body
## and its gauges, and a line for what just happened. Shows only what the
## server says; decides nothing (D03, D05).

@onready var _clock_label: Label = %Clock
@onready var _body_label: Label = %Body
@onready var _state_label: Label = %State
@onready var _vitality: ProgressBar = %Vitality
@onready var _hunger: ProgressBar = %Hunger
@onready var _thirst: ProgressBar = %Thirst
@onready var _cold: ProgressBar = %Cold
@onready var _fatigue: ProgressBar = %Fatigue

var _day := 1
var _night := false
var _body := -1
var _state := "connecting…"
var _clock := GameClock.new()
var _has_clock := false
## The bubbles, set by the main scene: beside this panel in the
## interface, shown even while it is hidden.
var speech: SpeechBubble
var thoughts: ThoughtBubble


func _ready() -> void:
	Net.connected.connect(_on_connected)
	Net.clock.connect(_on_clock)
	Net.day_started.connect(_on_day_started)
	Net.night_fell.connect(func(_day: int) -> void: _set_night(true))
	Net.body_condition.connect(_on_body_condition)
	Net.body_died.connect(_on_body_died)
	Net.body_assigned.connect(_on_body_assigned)
	Net.move_refused.connect(func(_body: int, x: int, y: int, _reason: String) -> void: set_state("cannot walk to (%d, %d)" % [x, y]))
	Net.act_refused.connect(_on_act_refused)
	Net.drink_refused.connect(func(_body: int, _x: int, _y: int, reason: String) -> void: set_state(Tooltip.refusal_words("drink", reason)))
	Net.season_started.connect(func(year: int, season: String) -> void: set_state(GameClock.season_line(year, season)))
	Net.disconnected.connect(set_state)
	_refresh()


func _process(_delta: float) -> void:
	if _has_clock:
		_refresh_clock()


func _on_connected(tick: int, body: int) -> void:
	_body = body
	_state = "connected"
	_clock.sync(float(tick), Time.get_ticks_msec())
	_refresh()


func _on_clock(clock: Dictionary) -> void:
	_clock.set_clock(clock, Time.get_ticks_msec())
	_has_clock = true
	_refresh()


func _on_day_started(day: int, tick: int) -> void:
	_day = day
	_night = false
	_clock.sync(float(tick), Time.get_ticks_msec())
	_refresh()


func _set_night(night: bool) -> void:
	_night = night
	_refresh()


func _on_body_condition(body: int, condition: Dictionary) -> void:
	if body != _body:
		return
	_vitality.value = float(condition.get("vitality", 0))
	_hunger.value = float(condition.get("hunger", 0))
	_thirst.value = float(condition.get("thirst", 0))
	_cold.value = float(condition.get("cold", 0))
	_fatigue.value = float(condition.get("fatigue", 0))
	var asleep := bool(condition.get("asleep", false))
	_body_label.text = "body %d%s" % [_body, " — asleep" if asleep else ""]


func _on_act_refused(_body: int, act: String, _item: int, reason: String) -> void:
	set_state(Tooltip.refusal_words(act, reason))


func _on_body_died(body: int, cause: String) -> void:
	if body == _body:
		set_state("your body died: %s" % cause)


func _on_body_assigned(body: int) -> void:
	_body = body
	for gauge in [_vitality, _hunger, _thirst, _cold, _fatigue]:
		gauge.value = 0.0
	set_state("reborn in another body" if body >= 0 else "no body: watching")


func set_state(state: String) -> void:
	_state = state
	_refresh()


func _refresh() -> void:
	_body_label.text = "body %d" % _body if _body >= 0 else "watching"
	_state_label.text = _state
	_refresh_clock()


func _refresh_clock() -> void:
	var tick := _clock.tick_at(Time.get_ticks_msec())
	_clock_label.text = GameClock.clock_line(tick, _clock.ticks_per_day, _clock.days_per_year, _night) if _has_clock else "Day %d · %s" % [_day, "night" if _night else "day"]



## `speaker` says `line`, in the speech thread at the top left: the way
## in for players and NPCs when they talk.
func say(speaker: String, line: String) -> void:
	if speech != null:
		speech.say(speaker, line)


## The piloted body thinks `line`, in the thought thread at the top
## right; tied to the place `at` (mm) when `placed`, so a click takes the
## camera there.
func think(line: String, at: Vector2 = Vector2.ZERO, placed: bool = false) -> void:
	if thoughts != null:
		thoughts.think(line, at, placed)
