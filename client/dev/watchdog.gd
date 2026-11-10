class_name DevWatchdog
extends Node
## Development aid, only with --dev servers (main.gd): a second thread
## watches the frames go by, and when the main thread has not finished a
## frame for STALL_SECONDS it writes to the error output what the main
## thread was doing (its last phase, the frame and the time), again at
## each REPORT_EVERY, so a freeze leaves a trace in the launcher's log.
## After DEAD_SECONDS with no frame the game is given up for frozen and
## sent SIGSEGV from outside (Linux): Godot's own crash handler then
## prints the main thread's C++ and script backtraces, the only way to
## see where it stands, since a thread can only capture its own stack.
## Costs nothing in play: not started without --dev; the phases are one
## integer written by the main thread.

## Seconds without a frame before the first report, between reports, and
## before the backtrace.
const STALL_SECONDS := 2.0
const REPORT_EVERY := 3.0
const DEAD_SECONDS := 20.0
const PERIOD_MSEC := 250

## What the main thread was last doing (phase), set from the hot spots.
const ELSEWHERE := 0
const NETWORK := 1
const WORLD_STEP := 2
const CHUNK_REDRAW := 3
const CHUNK_BUILD := 4
const TREES := 5
const PHASES := ["elsewhere (the engine: rendering, input, the display server)", "network messages (a burst of chunks, at a login or a teleport, takes a second or two: that is loading, not a freeze)", "world view step", "chunk redraw", "chunk build (ground, bank, brooks)", "planting a chunk's trees"]
static var phase := ELSEWHERE

var _thread: Thread
var _stop := false


func _ready() -> void:
	_thread = Thread.new()
	_thread.start(_watch)
	print("watchdog on: a frame over %.0f s is reported, after %.0f s the backtrace is forced" % [STALL_SECONDS, DEAD_SECONDS])


func _exit_tree() -> void:
	_stop = true
	if _thread != null and _thread.is_started():
		_thread.wait_to_finish()


## Whether a report is due at `stalled` seconds without a frame, the last
## one having been at `reported` (0: none yet).
static func report_due(stalled: float, reported: float) -> bool:
	if stalled < STALL_SECONDS:
		return false
	return reported == 0.0 or stalled - reported >= REPORT_EVERY


static func phase_name(p: int) -> String:
	return PHASES[p] if p >= 0 and p < PHASES.size() else "?"


## The line written for a stall.
static func report(stalled: float, frame: int, p: int) -> String:
	return "watchdog: no frame for %.1f s (frame %d); the main thread was in: %s" % [stalled, frame, phase_name(p)]


func _watch() -> void:
	var seen := Engine.get_process_frames()
	var since := Time.get_ticks_msec()
	var reported := 0.0
	while not _stop:
		OS.delay_msec(PERIOD_MSEC)
		var frame := Engine.get_process_frames()
		var now := Time.get_ticks_msec()
		if frame != seen:
			if reported > 0.0:
				printerr("watchdog: frames again after %.1f s" % ((now - since) / 1000.0))
			seen = frame
			since = now
			reported = 0.0
			continue
		var stalled := (now - since) / 1000.0
		if report_due(stalled, reported):
			reported = stalled
			printerr(report(stalled, frame, phase))
		if stalled >= DEAD_SECONDS and OS.get_name() == "Linux":
			printerr("watchdog: given up after %.1f s: asking the crash handler for the main thread's backtrace" % stalled)
			OS.execute("kill", ["-SEGV", str(OS.get_process_id())])
			return
