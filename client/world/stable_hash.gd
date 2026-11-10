class_name StableHash
extends RefCounted
## A hash of a few integers, the same on every machine and with every
## version of the engine: the server's `sim::gen::hash` (a splitmix64
## finaliser, chained), mirrored exactly and pinned by the same golden
## values in the tests. Godot's own `hash()` makes no such promise, so a
## new engine could change which image every tree shows.
##
## The client uses it for what is only drawn (which image of a kind, a
## size within a few percent, where a tuft of grass lies), from places the
## server has already sent: never the world's seed, which the client
## never receives (D03, D05).

## 2^64 divided by the golden ratio, as a signed 64-bit integer
## (0x9e3779b97f4a7c15).
const GOLDEN := -7046029254386353131
## The two multipliers of splitmix64's finaliser (0xbf58476d1ce4e5b9,
## 0x94d049bb133111eb), signed.
const MIX_1 := -4658895280553007687
const MIX_2 := -7723592293110705685


## `x` shifted right by `n` bits as an unsigned number: GDScript's `>>`
## keeps the sign.
static func _shr(x: int, n: int) -> int:
	return (x >> n) & ((1 << (64 - n)) - 1)


## The hash of `values`, as a signed 64-bit integer (the server's u64,
## two's complement). Order matters.
static func of(values: Array) -> int:
	var h := GOLDEN
	for v: int in values:
		h ^= v + GOLDEN + (h << 6) + _shr(h, 2)
		h += GOLDEN
		var z := h
		z = (z ^ _shr(z, 30)) * MIX_1
		z = (z ^ _shr(z, 27)) * MIX_2
		h = z ^ _shr(z, 31)
	return h


## A number in `0..count` from `values`.
static func below(values: Array, count: int) -> int:
	return posmod(_shr(of(values), 1), count)


## A number in [0, 1) from `values`, in steps of a hundred thousandth or
## so.
static func unit(values: Array) -> float:
	return float(below(values, 100003)) / 100003.0
