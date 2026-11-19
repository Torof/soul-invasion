class_name LoadingProgress
extends RefCounted
## Whether the world is ready to be shown, from what has come in: the
## server's welcome and world, the chunks sent around the body, and the
## chunks built. The world counts as in once every chunk of the window the
## server said it holds (`view`) came, the screen's are built and every
## one that came is built. Pure arithmetic, no node: testable headless.
## Before (audit F3, 18 November 2026), it waited for no chunk to come for
## QUIET_MSEC as well: 0.5 s more on every login.

## After a teleport onto the tile the body stands on no chunk comes: how
## long since the screen came up before that counts as over.
const QUIET_MSEC := 500

var has_world := false
var received := 0
var built := 0
## Grounds still being built on a worker or waiting to be laid, water
## meshes included (ChunkView.waiting).
var fields := 0
## Whether every chunk of the window held came (Net.window_received).
var window_received := false
## Whether the world view already holds and has built all the screen may
## show (WorldView.reach_ready): after a teleport onto the tile the body
## stands on no chunk comes, and the world counts as in on this instead,
## once nothing came for QUIET_MSEC since the screen came up.
var view_ready := false
var _started_msec := -1


## The screen came up at `now_msec`.
func start(now_msec: int) -> void:
	_started_msec = now_msec


## A chunk came in (at `_now_msec`).
func chunk_came(_now_msec: int) -> void:
	received += 1


## Whether everything sent is in and drawn.
func done(now_msec: int) -> bool:
	if received == 0:
		return has_world and view_ready and _started_msec >= 0 and now_msec - _started_msec >= QUIET_MSEC and fields == 0
	return has_world and window_received and view_ready and built >= received and fields == 0


## How far along, 0 to 1: the world's arrival first, then the building.
func fraction() -> float:
	if not has_world:
		return 0.0
	if received == 0:
		return 0.1
	return 0.1 + 0.9 * clampf(float(built) / float(received), 0.0, 1.0)


## What to say under the logo.
func stage() -> String:
	if not has_world:
		return "Reaching the valley"
	if received == 0 or built < received:
		return "Laying the ground"
	return "Smoothing the riverbanks"
