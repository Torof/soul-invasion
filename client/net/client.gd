extends Node
## The connection to the server (autoload `Net`).
##
## Owns the single WebSocket (D25), speaks the handshake, and turns server
## messages into signals. Decides nothing about the world (D03, D05).

signal connected(tick: int, body: int)
signal world_state(width: int, height: int, chunk_size: int, materials: Array)
signal chunk(x: int, y: int, tiles: Array, bodies: Array, items: Array)
signal chunk_hidden(x: int, y: int)
signal body_walks(body: int, at: Vector2, route: Array, speed_cm_per_s: int)
signal body_stopped(body: int, at: Vector2)
signal body_left(body: int)
signal move_refused(body: int, x: int, y: int)
signal day_started(day: int, tick: int)
signal disconnected(reason: String)

const DEFAULT_URL := "ws://127.0.0.1:7777"

var _peer := WebSocketPeer.new()
var _state := WebSocketPeer.STATE_CLOSED
var _hello_sent := false

## True once the server has welcomed us.
var is_ready := false
## The body this client pilots, or -1 when only watching.
var body := -1


func connect_to(url: String = DEFAULT_URL) -> void:
	_hello_sent = false
	is_ready = false
	body = -1
	var err := _peer.connect_to_url(url)
	if err != OK:
		disconnected.emit("cannot connect to %s (error %d)" % [url, err])


func send_move_to(x: int, y: int) -> void:
	if is_ready and body >= 0:
		_send(Protocol.move_to(x, y))


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
			var text := _peer.get_packet().get_string_from_utf8()
			_handle(Protocol.decode(text))
	elif state == WebSocketPeer.STATE_CLOSED and _state != WebSocketPeer.STATE_CLOSED:
		is_ready = false
		disconnected.emit("connection closed (code %d)" % _peer.get_close_code())
	_state = state


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
			world_state.emit(
				int(message.get("width", 0)),
				int(message.get("height", 0)),
				int(message.get("chunk_size", 0)),
				message.get("materials", [])
			)
		"chunk":
			chunk.emit(
				int(message.get("x", 0)),
				int(message.get("y", 0)),
				message.get("tiles", []),
				message.get("bodies", []),
				message.get("items", [])
			)
		"chunk_hidden":
			chunk_hidden.emit(int(message.get("x", 0)), int(message.get("y", 0)))
		"body_left":
			body_left.emit(int(message.get("body", -1)))
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
