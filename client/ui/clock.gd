class_name GameClock
extends RefCounted
## Keeps the world's time between server messages (D09): the server says
## which tick it is now and then; in between, ticks advance at the pace
## it announced. Pure arithmetic, no network: testable headless.

## Hours in a game day; the day starts at dawn, 06:00.
const HOURS_PER_DAY := 24.0
const DAWN_HOUR := 6.0

var ticks_per_day := 72000.0
var _ticks_per_real_second := 10.0
var _tick_at_sync := 0.0
var _synced_at_msec := 0


## Takes the server's clock: tick now, ticks per second and per day, pace.
func set_clock(clock: Dictionary, now_msec: int) -> void:
	ticks_per_day = float(clock.get("ticks_per_day", 72000))
	var pace := float(clock.get("time_scale_permille", 1000)) / 1000.0
	_ticks_per_real_second = float(clock.get("ticks_per_second", 10)) * pace
	sync(float(clock.get("tick", 0)), now_msec)


## Takes a tick the server just reported.
func sync(tick: float, now_msec: int) -> void:
	_tick_at_sync = tick
	_synced_at_msec = now_msec


## The tick we estimate the world is at `now_msec`.
func tick_at(now_msec: int) -> float:
	var elapsed := (now_msec - _synced_at_msec) / 1000.0
	return _tick_at_sync + elapsed * _ticks_per_real_second


## The time of day for a tick: (hour, minute), the day starting at dawn.
static func time_of_day(tick: float, ticks_per_day: float) -> Vector2i:
	var fraction := fposmod(tick, ticks_per_day) / ticks_per_day
	var hours := fposmod(DAWN_HOUR + fraction * HOURS_PER_DAY, HOURS_PER_DAY)
	return Vector2i(floori(hours), floori((hours - floorf(hours)) * 60.0))
