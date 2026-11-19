class_name BodyCrowd
extends RefCounted
## Which of the bodies told (about 150 m around, D32) the screen may show:
## only those are walked on every frame and have a node, so a crowd costs
## the client what is on screen, not what is in range.
##
## The area watched is the screen grown by MARGIN_PX. A body is looked at
## when it is told a walk and when the screen leaves the middle of that
## area (`watch`): its walk is followed leg by leg (`first_leg_seen`), each
## leg's screen box grown by its figure and by what is left of the gap
## between where it is shown and its walk. A body whose first leg reaches
## the area is shown; one whose later leg reaches it waits, untouched, for
## the tick it sets off on that leg, far outside the screen still, and is
## shown from that frame; one whose walk never reaches it is left as it is
## until told again. So a body coming onto the screen is walked on before
## it shows, and appears where it should be (BodyWalk walks by the clock).
##
## A walk without arrivals (before protocol v19) moves by the frame's
## time, not by the clock, and is always walked on.

## How far around the screen, in the world's pixels, the bodies are
## watched. The screen may move half of it before they are looked at
## again: more than the mouse's reach around the pointer
## (WorldView._nearest_depth_near) and a body's figure.
const MARGIN_PX := 512.0
## How far a body's figure reaches out of the point it stands on, in the
## world's pixels: a lying body, its "z" and what it holds, all under 2 m.
const FIGURE_PX := 2.0 * Iso.PX_PER_M

## body id -> true: the bodies walked on every frame.
var shown: Dictionary = {}
## The area watched: the screen when it was last looked at, grown by
## MARGIN_PX; empty before the first look.
var _watch := Rect2()
## The body always walked on (the piloted one: the camera follows it).
var _kept := -1
## body id -> the tick it sets off on the first leg that reaches the area.
var _wakes: Dictionary = {}
## Those ticks in order, and whose they are (an entry no longer in _wakes
## is passed over).
var _wake_at := PackedFloat64Array()
var _wake_id := PackedInt32Array()


## The first leg of `walk` (BodyWalk) whose screen box meets `area` (world
## pixels): 0 for the one it is on (or where it stands), k for the leg to
## its k-th waypoint left, -1 when none does.
static func first_leg_seen(walk: Dictionary, area: Rect2) -> int:
	var route: Array = walk["route"]
	if not route.is_empty() and (walk["arrivals"] as Array).is_empty():
		return 0
	var gap := (walk.get("offset", Vector2.ZERO) as Vector2).length()
	var leg := Rect2(walk["at"], Vector2.ZERO).expand(walk["from"])
	if not route.is_empty():
		leg = leg.expand(route[0])
	if area.intersects(_on_screen(leg.grow(gap))):
		return 0
	for k in range(1, route.size()):
		leg = Rect2(route[k - 1], Vector2.ZERO).expand(route[k])
		if area.intersects(_on_screen(leg.grow(gap))):
			return k
	return -1


## The screen box (world pixels) of a box of the ground (mm), grown by a
## body's figure.
static func _on_screen(mm: Rect2) -> Rect2:
	var a := Iso.point_to_screen(mm.position)
	var r := Rect2(a, Vector2.ZERO).expand(Iso.point_to_screen(mm.end))
	r = r.expand(Iso.point_to_screen(Vector2(mm.position.x, mm.end.y)))
	r = r.expand(Iso.point_to_screen(Vector2(mm.end.x, mm.position.y)))
	return r.grow(FIGURE_PX)


## The screen moved to `seen` (world pixels): when it left the middle of
## the area watched, the area is moved and every body of `bodies` (id ->
## walk) looked at again. True when it was.
func watch(seen: Rect2, bodies: Dictionary) -> bool:
	if _watch.has_area() and _watch.grow(-MARGIN_PX / 2.0).encloses(seen):
		return false
	_watch = seen.grow(MARGIN_PX)
	shown.clear()
	_wakes.clear()
	_wake_at.clear()
	_wake_id.clear()
	for id: int in bodies:
		told(id, bodies[id])
	return true


## Body `id` told a new walk (or looked at again).
func told(id: int, walk: Dictionary) -> void:
	shown.erase(id)
	_wakes.erase(id)
	if id == _kept or not _watch.has_area():
		if id == _kept:
			shown[id] = true
		return
	var k := first_leg_seen(walk, _watch)
	if k == 0:
		shown[id] = true
	elif k > 0:
		var tick := float((walk["arrivals"] as Array)[k - 1])
		_wakes[id] = tick
		var i := _wake_at.bsearch(tick, false)
		_wake_at.insert(i, tick)
		_wake_id.insert(i, id)


## Body `id` of `bodies` is always walked on (the piloted one), or none
## for -1.
func keep(id: int, bodies: Dictionary) -> void:
	if id == _kept:
		return
	_kept = id
	if bodies.has(id):
		shown[id] = true


func forget(id: int) -> void:
	shown.erase(id)
	_wakes.erase(id)


func clear() -> void:
	shown.clear()
	_wakes.clear()
	_wake_at.clear()
	_wake_id.clear()


## The bodies whose leg reaching the area starts by tick `now` are shown;
## then those shown, but `skip` (walked already), are walked on to `now`
## (BodyWalk.advance). True when one of them walks.
func step(bodies: Dictionary, delta: float, now: float, skip := -1) -> bool:
	while not _wake_at.is_empty() and _wake_at[0] <= now:
		var id := _wake_id[0]
		if _wakes.get(id, -1.0) == _wake_at[0]:
			_wakes.erase(id)
			shown[id] = true
		_wake_at.remove_at(0)
		_wake_id.remove_at(0)
	var moving := false
	for id: int in shown:
		if id != skip and bodies.has(id) and BodyWalk.advance(bodies[id], delta, now):
			moving = true
	return moving
