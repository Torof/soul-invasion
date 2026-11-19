class_name PageLedger
extends RefCounted
## Which screen pages of the land's ground are held, which are stale and
## which are rendered this frame (GroundPages, D39). Pure bookkeeping, apart
## from the viewports so the tests can drive it.
##
## A page is a square of SIZE screen pixels on a grid fixed to the land at
## the current zoom: page (i, j) holds the land's pixels from (i, j) × SIZE
## in "page pixels", the screen's pixels with the camera's whole-pixel
## offset taken out (WorldView._snap_camera). The camera moving slides the
## grid by whole pixels and changes no page; a new scale (a zoom step, the
## window resized) changes every page.

## A page's side, in screen pixels.
const SIZE := 512
## The ring of pages held around the screen, in screen pixels: those the
## camera is about to bring in, rendered ahead of it.
const MARGIN := 256
## Pages of the ring rendered a frame at most, nearest the screen first;
## the screen's own are rendered whatever their number.
const AHEAD_PER_FRAME := 4

## The scale the pages hold (screen pixels a world pixel); 0 before any.
var scale := 0.0
## page -> true when it shows the ground as it is, false when it is stale
## (never rendered, or the ground under it changed since).
var pages: Dictionary = {}
## The screen and the ring last seen, in page pixels.
var screen := Rect2()
var held := Rect2()


## The page covering `rect` (page pixels) at its north-west corner, and the
## one at its far corner.
static func first_page(rect: Rect2) -> Vector2i:
	return Vector2i(floori(rect.position.x / SIZE), floori(rect.position.y / SIZE))


static func last_page(rect: Rect2) -> Vector2i:
	return Vector2i(ceili(rect.end.x / SIZE) - 1, ceili(rect.end.y / SIZE) - 1)


## Page `key`'s square, in page pixels.
static func rect_of(key: Vector2i) -> Rect2:
	return Rect2(Vector2(key * SIZE), Vector2(SIZE, SIZE))


## The most pages ever held for a screen of `size` pixels.
static func most_held(size: Vector2) -> int:
	var w := ceili((size.x + 2.0 * MARGIN) / SIZE) + 1
	var h := ceili((size.y + 2.0 * MARGIN) / SIZE) + 1
	return w * h


## The camera's view: `now_screen` the screen in page pixels, `now_scale`
## the scale. Holds the pages over the screen and its ring, new ones stale;
## a new scale drops every page. The pages dropped.
func see(now_screen: Rect2, now_scale: float) -> Array[Vector2i]:
	var dropped: Array[Vector2i] = []
	if now_scale != scale:
		for key: Vector2i in pages:
			dropped.append(key)
		pages.clear()
		scale = now_scale
	screen = now_screen
	held = now_screen.grow(MARGIN)
	var lo := first_page(held)
	var hi := last_page(held)
	for key: Vector2i in pages.keys():
		if key.x < lo.x or key.y < lo.y or key.x > hi.x or key.y > hi.y:
			pages.erase(key)
			dropped.append(key)
	for y in range(lo.y, hi.y + 1):
		for x in range(lo.x, hi.x + 1):
			var key := Vector2i(x, y)
			if not pages.has(key):
				pages[key] = false
	return dropped


## The ground under `rect` (page pixels) changed: the pages over it are
## stale.
func dirty(rect: Rect2) -> void:
	if not rect.has_area() or pages.is_empty():
		return
	var lo := first_page(rect)
	var hi := last_page(rect)
	for key: Vector2i in pages:
		if key.x >= lo.x and key.y >= lo.y and key.x <= hi.x and key.y <= hi.y:
			pages[key] = false


## The whole ground's look changed (a material, the night): every page is
## stale.
func dirty_all() -> void:
	for key: Vector2i in pages:
		pages[key] = false


## The pages to render this frame, marked fresh: every stale one the screen
## shows, then up to AHEAD_PER_FRAME of the ring, nearest the screen first.
func due() -> Array[Vector2i]:
	var now: Array[Vector2i] = []
	var ahead: Array[Vector2i] = []
	for key: Vector2i in pages:
		if pages[key]:
			continue
		if rect_of(key).intersects(screen):
			now.append(key)
		else:
			ahead.append(key)
	var middle := screen.get_center()
	ahead.sort_custom(func(a: Vector2i, b: Vector2i) -> bool:
		return rect_of(a).get_center().distance_squared_to(middle) < rect_of(b).get_center().distance_squared_to(middle))
	now.append_array(ahead.slice(0, AHEAD_PER_FRAME))
	for key in now:
		pages[key] = true
	return now


## How many pages are stale.
func stale() -> int:
	var n := 0
	for key: Vector2i in pages:
		if not pages[key]:
			n += 1
	return n
