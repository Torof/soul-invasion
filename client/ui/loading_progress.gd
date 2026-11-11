class_name LoadingProgress
extends RefCounted
## Whether the world is ready to be shown, from what has come in: the
## server's welcome and world, the chunks sent around the body, and the
## chunks built. The server does not say how many chunks it will send, so
## the world counts as in once none has come for QUIET_MSEC and every one
## that came is built. Pure arithmetic, no node: testable headless.

## How long without a new chunk before the sending counts as over.
const QUIET_MSEC := 500

var has_world := false
var received := 0
var built := 0
## Grounds still being built on a worker or waiting to be laid, water
## meshes included (ChunkView.waiting).
var fields := 0
var _last_chunk_msec := -1


## A chunk came in at `now_msec`.
func chunk_came(now_msec: int) -> void:
	received += 1
	_last_chunk_msec = now_msec


## Whether everything sent is in and drawn.
func done(now_msec: int) -> bool:
	return has_world and received > 0 and now_msec - _last_chunk_msec >= QUIET_MSEC \
		and built >= received and fields == 0


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
