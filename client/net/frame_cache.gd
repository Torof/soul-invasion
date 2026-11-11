class_name FrameCache
extends RefCounted
## The chunk frames the client holds, in memory only (D4): those in view,
## and those of the chunks lately left behind, kept so that a return, or a
## reconnection to the same world, costs no frame. Never on disk: it
## would be a map of everything the player saw.
##
## The server mirrors the kept frames, by the same rule (the least
## recently hidden forgotten first, `Protocol.CHUNK_CACHE_CHUNKS`), to
## know when `chunk_same` is enough. Should it guess wrong, the client
## answers `chunk_missing` and gets the frame.

## What `same` found.
enum { HELD, RESTORED, MISSING }

## chunk -> its frame's bytes, for the chunks in view.
var live: Dictionary = {}
## chunk -> its frame's bytes, for the chunks left behind, the least
## recently hidden first (a Dictionary keeps its order of insertion).
var _kept: Dictionary = {}
var capacity := Protocol.CHUNK_CACHE_CHUNKS


## A frame's version, from its header.
static func version_of(bytes: PackedByteArray) -> int:
	return bytes.decode_u32(8) if bytes.size() >= Protocol.CHUNK_HEADER else -1


## A frame received for `key`: in view now.
func received(key: Vector2i, bytes: PackedByteArray) -> void:
	_kept.erase(key)
	live[key] = bytes


## `chunk_same` for `key` at `version`: HELD when in view at that version
## already, RESTORED when it was kept (now in view: `live[key]` holds its
## frame), MISSING when neither (the client must say so).
func same(key: Vector2i, version: int) -> int:
	if live.has(key) and version_of(live[key]) == version:
		return HELD
	var kept: PackedByteArray = _kept.get(key, PackedByteArray())
	_kept.erase(key)
	if not kept.is_empty() and version_of(kept) == version:
		live[key] = kept
		return RESTORED
	return MISSING


## The window held is now `window` (in chunks): the chunks in view outside
## it are left behind, kept in the order of their rows, then columns, as
## the server's mirror keeps them. Returns them, in that order.
func view(window: Rect2i) -> Array[Vector2i]:
	var left: Array[Vector2i] = []
	for key: Vector2i in live:
		if not window.has_point(key):
			left.append(key)
	left.sort_custom(func(a: Vector2i, b: Vector2i) -> bool: return a.y < b.y or (a.y == b.y and a.x < b.x))
	for key in left:
		_keep(key, live[key])
		live.erase(key)
	return left


## Whether a frame of `key` is kept, out of view.
func is_kept(key: Vector2i) -> bool:
	return _kept.has(key)


## How many frames are kept, out of view.
func kept_count() -> int:
	return _kept.size()


## Every chunk held, as `hello`'s `held` lists them: the least recently
## hidden first, those in view last, at most `Protocol.HELD_CHUNKS_MAX`
## (the oldest left out). Each [x, y, version].
func held() -> Array:
	var all: Array = []
	for source: Dictionary in [_kept, live]:
		for key: Vector2i in source:
			all.append([key.x, key.y, version_of(source[key])])
	if all.size() > Protocol.HELD_CHUNKS_MAX:
		all = all.slice(all.size() - Protocol.HELD_CHUNKS_MAX)
	return all


## Forgets everything (another world).
func clear() -> void:
	live.clear()
	_kept.clear()


func _keep(key: Vector2i, bytes: PackedByteArray) -> void:
	_kept.erase(key)
	_kept[key] = bytes
	while _kept.size() > capacity:
		for oldest: Vector2i in _kept:
			_kept.erase(oldest)
			break
