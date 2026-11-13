class_name GameClock
extends RefCounted
## Keeps the world's time between server messages (D09): the server says
## which tick it is now and then; in between, ticks advance at the pace
## it announced. Pure arithmetic, no network: testable headless.

## Hours in a game day; the day starts at dawn, 06:00.
const HOURS_PER_DAY := 24.0
const DAWN_HOUR := 6.0

var ticks_per_day := 72000.0
## Game days in a year of four seasons (D09; protocol v24).
var days_per_year := 50
var _ticks_per_real_second := 10.0
var _tick_at_sync := 0.0
var _synced_at_msec := 0


## Takes the server's clock: tick now, ticks per second and per day, pace.
func set_clock(clock: Dictionary, now_msec: int) -> void:
	ticks_per_day = float(clock.get("ticks_per_day", 72000))
	days_per_year = maxi(int(clock.get("days_per_year", 50)), 4)
	var pace := float(clock.get("time_scale_permille", 1000)) / 1000.0
	_ticks_per_real_second = float(clock.get("ticks_per_second", 10)) * pace
	sync(float(clock.get("tick", 0)), now_msec)


## Takes a tick the server just reported.
func sync(tick: float, now_msec: int) -> void:
	_tick_at_sync = tick
	_synced_at_msec = now_msec


## Takes a tick a message was sent at (a walk told): the world was at
## that tick at least, so a clock found behind it is set to it. Never set
## back by it: a message may wait in the queue, its tick past.
func catch_up(tick: float, now_msec: int) -> void:
	if tick > tick_at(now_msec):
		sync(tick, now_msec)


## The tick we estimate the world is at `now_msec`.
func tick_at(now_msec: int) -> float:
	var elapsed := (now_msec - _synced_at_msec) / 1000.0
	return _tick_at_sync + elapsed * _ticks_per_real_second


## The time of day for a tick: (hour, minute), the day starting at dawn.
static func time_of_day(tick: float, ticks_per_day: float) -> Vector2i:
	var fraction := fposmod(tick, ticks_per_day) / ticks_per_day
	var hours := fposmod(DAWN_HOUR + fraction * HOURS_PER_DAY, HOURS_PER_DAY)
	return Vector2i(floori(hours), floori((hours - floorf(hours)) * 60.0))


## The seasons, in the order of the year (the server's `Season`).
const SEASONS: PackedStringArray = ["spring", "summer", "autumn", "winter"]


## The day of a tick, counted from 1.
static func day_of(tick: float, ticks_per_day: float) -> int:
	return floori(maxf(tick, 0.0) / ticks_per_day) + 1


## The year of a day (from 1), counted from 1.
@warning_ignore("integer_division")
static func year_of(day: int, days_per_year: int) -> int:
	return (day - 1) / days_per_year + 1


## The season of a day (from 1), as the server cuts the year: the day of
## the year from 0, times four, over the year's days (50 days: 13, 12,
## 13, 12). Must agree with `Calendar::season_of`.
@warning_ignore("integer_division")
static func season_of(day: int, days_per_year: int) -> String:
	var of_year := (day - 1) % days_per_year
	return SEASONS[of_year * 4 / days_per_year]


## The clock's line: the day, the hour, day or night, and discreetly
## the season and the year (D09), all from the tick.
static func clock_line(tick: float, ticks_per_day: float, days_per_year: int, night: bool) -> String:
	var hm := GameClock.time_of_day(tick, ticks_per_day)
	var day := GameClock.day_of(tick, ticks_per_day)
	return "Day %d · %02d:%02d · %s · %s, year %d" % [
		day, hm.x, hm.y, "night" if night else "day",
		GameClock.season_of(day, days_per_year), GameClock.year_of(day, days_per_year)]


## The state line when a season begins.
static func season_line(year: int, season: String) -> String:
	return "%s begins · year %d" % [season, year]
