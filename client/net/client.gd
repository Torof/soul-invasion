extends Node
## The connection to the server (autoload `Net`).
##
## Owns the single WebSocket (D25), speaks the handshake, and turns server
## messages into signals. Decides nothing about the world (D03, D05).

signal connected(tick: int, body: int)
signal world_state(width: int, height: int, chunk_size: int)
signal chunk(x: int, y: int, tiles: Array, bodies: Array)
signal chunk_hidden(x: int, y: int)
signal body_moved(body: int, x: int, y: int, tick: int)
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
				int(message.get("chunk_size", 0))
			)
		"chunk":
			chunk.emit(
				int(message.get("x", 0)),
				int(message.get("y", 0)),
				message.get("tiles", []),
				message.get("bodies", [])
			)
		"chunk_hidden":
			chunk_hidden.emit(int(message.get("x", 0)), int(message.get("y", 0)))
		"body_left":
			body_left.emit(int(message.get("body", -1)))
		"body_moved":
			body_moved.emit(
				int(message.get("body", -1)),
				int(message.get("x", 0)),
				int(message.get("y", 0)),
				int(message.get("tick", 0))
			)
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
