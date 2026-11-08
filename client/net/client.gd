extends Node
## The connection to the server (autoload `Net`).
##
## Owns the single WebSocket (D25), speaks the handshake, and turns server
## messages into signals. Decides nothing about the world (D03, D05).

signal connected(tick: int, body: int)
signal world_state(width: int, height: int, chunk_size: int, materials: Array, night: bool)
## The clock: {"tick", "ticks_per_second", "ticks_per_day", "time_scale"}, with world_state.
signal clock(clock: Dictionary)
## The kinds of tree, sent with the world's state, before it.
signal species(species: Array)
signal chunk(x: int, y: int, tiles: Array, bodies: Array, items: Array, trees: Array, currents: Array)
signal chunk_hidden(x: int, y: int)
signal body_walks(body: int, at: Vector2, route: Array, speed_cm_per_s: int)
signal body_stopped(body: int, at: Vector2)
signal body_left(body: int)
signal move_refused(body: int, x: int, y: int)
signal body_condition(body: int, condition: Dictionary)
signal body_sleeps(body: int, asleep: bool)
signal body_died(body: int, cause: String)
signal body_assigned(body: int)
signal item_added(item: Dictionary)
signal item_removed(item: int, x: int, y: int)
## act: pick_up | drop | hold | stow | eat; reason: unknown | out_of_reach |
## not_carried | too_heavy | not_food.
signal act_refused(body: int, act: String, item: int, reason: String)
## held: {} or {"id", "material"}; carried: Array of the same.
signal inventory(body: int, held: Dictionary, carried: Array, load_g: int, capacity_g: int)
signal body_holds(body: int, item: Dictionary)
signal night_fell(day: int)
signal day_started(day: int, tick: int)
signal disconnected(reason: String)
## Development tools on (the server runs with --dev): the whole map,
## coarse, one letter a cell of `cell` tiles (see the protocol).
signal dev_overview(cell: int, width: int, height: int, cells: String)

const DEFAULT_URL := "ws://127.0.0.1:7777"
## Seconds between two attempts to come back once the connection is lost
## (the server restarted, say). Zero: never.
const RECONNECT_AFTER := 3.0

var _peer := WebSocketPeer.new()
var _state := WebSocketPeer.STATE_CLOSED
var _hello_sent := false
var _url := DEFAULT_URL
var _reconnect_in := 0.0

## True once the server has welcomed us.
var is_ready := false
## Whether the server runs with --dev (it sent its overview).
var dev := false
## The body this client pilots, or -1 when only watching.
var body := -1


func connect_to(url: String = DEFAULT_URL) -> void:
	_hello_sent = false
	is_ready = false
	dev = false
	body = -1
	_url = url
	_reconnect_in = 0.0
	_peer = WebSocketPeer.new()
	# The development overview is the largest message (up to 512 × 512
	# letters); Godot's default inbound buffer (64 KiB) would drop it.
	_peer.inbound_buffer_size = 1 << 20
	var err := _peer.connect_to_url(url)
	if err != OK:
		disconnected.emit("cannot connect to %s (error %d)" % [url, err])
		_reconnect_in = RECONNECT_AFTER
	else:
		# Connecting: a refusal shows as the peer closed at the next poll,
		# which must count as a loss and be retried (the server not up yet).
		_state = WebSocketPeer.STATE_CONNECTING


## Development only: stand the body near a tile at once (--dev servers).
func send_dev_teleport(x: int, y: int) -> void:
	if is_ready and body >= 0 and dev:
		_send(Protocol.dev_teleport(x, y))


func send_move_to(x: int, y: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.move_to(x, y))


func send_eat(item: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.eat(item))


func send_pick_up(item: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.pick_up(item))


func send_drop(item: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.drop(item))


func send_hold(item: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.hold(item))


func send_stow(item: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.stow(item))


func send_sleep() -> void:
	if is_ready and body >= 0:
		_send(Protocol.sleep())


func send_wake() -> void:
	if is_ready and body >= 0:
		_send(Protocol.wake())


func _send(message: Dictionary) -> void:
	_peer.send_text(Protocol.encode(message))


func _exit_tree() -> void:
	# Leave with a closing handshake rather than a reset.
	if _peer.get_ready_state() == WebSocketPeer.STATE_OPEN:
		_peer.close()
		_peer.poll()


func _process(_delta: float) -> void:
	_peer.poll()
	var state := _peer.get_ready_state()
	if state == WebSocketPeer.STATE_OPEN:
		if not _hello_sent:
			_hello_sent = true
			_send(Protocol.hello())
		while _peer.get_available_packet_count() > 0:
			var t := DevTiming.start()
			var text := _peer.get_packet().get_string_from_utf8()
			_handle(Protocol.decode(text))
			DevTiming.add("network messages", t)
	elif state == WebSocketPeer.STATE_CLOSED and _state != WebSocketPeer.STATE_CLOSED:
		is_ready = false
		disconnected.emit("connection closed (code %d), retrying" % _peer.get_close_code())
		_reconnect_in = RECONNECT_AFTER
	_state = state
	if _reconnect_in > 0.0 and RECONNECT_AFTER > 0.0:
		_reconnect_in -= _delta
		if _reconnect_in <= 0.0:
			connect_to(_url)


## A fine point from the wire, in millimetres.
static func point(p: Variant) -> Vector2:
	if p is Dictionary:
		return Vector2(float(p.get("x", 0)), float(p.get("y", 0)))
	return Vector2.ZERO


func _handle(message: Dictionary) -> void:
	match message.get("type", ""):
		"welcome":
			if int(message.get("protocol_version", 0)) != Protocol.PROTOCOL_VERSION:
				disconnected.emit("server speaks protocol v%s" % message.get("protocol_version"))
				_peer.close()
				return
			is_ready = true
			var assigned: Variant = message.get("body")
			body = int(assigned) if assigned != null else -1
			connected.emit(int(message.get("tick", 0)), body)
		"world_state":
			species.emit(message.get("species", []))
			world_state.emit(
				int(message.get("width", 0)),
				int(message.get("height", 0)),
				int(message.get("chunk_size", 0)),
				message.get("materials", []),
				bool(message.get("night", false))
			)
			clock.emit(message.get("clock", {}))
		"chunk":
			chunk.emit(
				int(message.get("x", 0)),
				int(message.get("y", 0)),
				message.get("tiles", []),
				message.get("bodies", []),
				message.get("items", []),
				message.get("trees", []),
				message.get("currents", [])
			)
		"chunk_hidden":
			chunk_hidden.emit(int(message.get("x", 0)), int(message.get("y", 0)))
		"body_left":
			body_left.emit(int(message.get("body", -1)))
		"dev_overview":
			dev = true
			dev_overview.emit(
				int(message.get("cell", 8)),
				int(message.get("width", 0)),
				int(message.get("height", 0)),
				str(message.get("cells", ""))
			)
		"body_condition":
			body_condition.emit(int(message.get("body", -1)), message.get("condition", {}))
		"body_sleeps":
			body_sleeps.emit(int(message.get("body", -1)), bool(message.get("asleep", false)))
		"body_died":
			body_died.emit(int(message.get("body", -1)), str(message.get("cause", "")))
		"body_assigned":
			var assigned: Variant = message.get("body")
			body = int(assigned) if assigned != null else -1
			body_assigned.emit(body)
		"item_added":
			item_added.emit(message.get("item", {}))
		"item_removed":
			item_removed.emit(
				int(message.get("item", -1)), int(message.get("x", 0)), int(message.get("y", 0))
			)
		"act_refused":
			act_refused.emit(
				int(message.get("body", -1)),
				str(message.get("act", "")),
				int(message.get("item", -1)),
				str(message.get("reason", ""))
			)
		"inventory":
			var held: Variant = message.get("held")
			inventory.emit(
				int(message.get("body", -1)),
				held if held is Dictionary else {},
				message.get("carried", []),
				int(message.get("load_g", 0)),
				int(message.get("capacity_g", 0))
			)
		"body_holds":
			var item: Variant = message.get("item")
			body_holds.emit(int(message.get("body", -1)), item if item is Dictionary else {})
		"night_fell":
			night_fell.emit(int(message.get("day", 0)))
		"body_walks":
			var route: Array = []
			for p in message.get("route", []):
				route.append(point(p))
			body_walks.emit(
				int(message.get("body", -1)),
				point(message.get("at", {})),
				route,
				int(message.get("speed_cm_per_s", 0))
			)
		"body_stopped":
			body_stopped.emit(int(message.get("body", -1)), point(message.get("at", {})))
		"move_refused":
			move_refused.emit(
				int(message.get("body", -1)), int(message.get("x", 0)), int(message.get("y", 0))
			)
		"day_started":
			day_started.emit(int(message.get("day", 0)), int(message.get("tick", 0)))
		"protocol_mismatch":
			disconnected.emit("server expects protocol v%s" % message.get("expected"))
		"pong":
			pass
		_:
			push_warning("unknown server message: %s" % message.get("type", ""))
