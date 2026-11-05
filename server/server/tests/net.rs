//! Drives the real server binary over WebSocket with a small fake client:
//! handshake, liveness, published events, and the ways a client gets
//! disconnected.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage, decode, encode};
use server::net::VIEW_RADIUS;
use sim::{BodyId, CHUNK_SIZE, Calendar, ChunkPos, Pos, VALLEY_SIZE, World};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The world the server starts with: the standard valley, so tests can
/// know where bodies stand and which way they will walk.
fn standard_world() -> World {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let content = server::content::load(&dir).expect("repository content");
    World::new(Calendar::default(), &content)
}

/// Where a body stands in the standard world.
fn start_of(world: &World, body: u32) -> Pos {
    world.body(BodyId(body)).expect("standard body").pos()
}

/// How many chunks the view around `centre` holds in the standard world.
fn window_len(world: &World, centre: ChunkPos) -> usize {
    let mut count = 0;
    for dy in -VIEW_RADIUS..=VIEW_RADIUS {
        for dx in -VIEW_RADIUS..=VIEW_RADIUS {
            if world
                .map()
                .has_chunk(ChunkPos::new(centre.x + dx, centre.y + dy))
            {
                count += 1;
            }
        }
    }
    count
}

/// The steps the standard world will make a body take, as (x, y).
fn steps(world: &World, from: Pos, to: Pos) -> Vec<(i32, i32)> {
    world
        .map()
        .path(from, to)
        .expect("reachable in the standard valley")
        .into_iter()
        .map(|p| (p.x, p.y))
        .collect()
}

/// A running server, killed when dropped.
struct Server {
    child: Child,
    addr: String,
}

impl Server {
    /// Starts a paced server with one game day per real second.
    fn start() -> Self {
        Self::start_with(&[])
    }

    /// Like [`Server::start`], with extra command-line options.
    fn start_with(extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
            .args([
                "--ephemeral",
                "--listen",
                "127.0.0.1:0",
                "--ticks-per-second",
                "20",
                "--seconds-per-day",
                "1",
            ])
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("server binary starts");

        let stdout = child.stdout.take().expect("stdout is piped");
        let mut lines = BufReader::new(stdout).lines();
        let addr = loop {
            let line = lines
                .next()
                .expect("server prints its address before exiting")
                .expect("stdout is readable");
            if let Some(addr) = line.strip_prefix("listening on ") {
                break addr.to_owned();
            }
        };
        // Keep draining stdout so the server never blocks on a full pipe.
        std::thread::spawn(move || for _ in lines {});

        Self { child, addr }
    }

    async fn connect(&self) -> Socket {
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{}", self.addr))
            .await
            .expect("websocket connects");
        socket
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn send(socket: &mut Socket, message: &ClientMessage) {
    socket
        .send(Message::text(encode(message).unwrap()))
        .await
        .expect("message is sent");
}

/// Next server message, or `None` once the server closed the connection.
async fn receive(socket: &mut Socket) -> Option<ServerMessage> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("server answers within 5 s");
        match frame {
            None | Some(Ok(Message::Close(_)) | Err(_)) => return None,
            Some(Ok(Message::Text(text))) => {
                return Some(decode(text.as_str()).expect("server speaks the protocol"));
            }
            Some(Ok(_)) => {}
        }
    }
}

/// Waits for the next published day boundary.
async fn next_day(socket: &mut Socket) -> u64 {
    loop {
        if let Some(ServerMessage::DayStarted { day, .. }) = receive(socket).await {
            break day;
        }
    }
}

#[tokio::test]
async fn hello_ping_and_a_published_day() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
    let welcome = receive(&mut socket).await;
    assert!(
        matches!(
            welcome,
            Some(ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                body: Some(1),
                ..
            })
        ),
        "got {welcome:?}"
    );

    // The shape of the world follows the welcome, then the chunks around
    // body 1, each with the bodies standing in it.
    match receive(&mut socket).await {
        Some(ServerMessage::WorldState {
            width,
            height,
            chunk_size,
            materials,
        }) => {
            assert_eq!((width, height), (VALLEY_SIZE, VALLEY_SIZE));
            assert_eq!(chunk_size, CHUNK_SIZE);
            assert!(materials.len() >= 9, "the catalogue of materials");
            assert!(materials.iter().all(|m| m.colour.starts_with('#')));
        }
        other => panic!("expected world_state, got {other:?}"),
    }
    let world = standard_world();
    let centre = start_of(&world, 1).chunk();
    let mut seen_bodies = Vec::new();
    let mut seen_items = 0;
    let mut chunks = Vec::new();
    for _ in 0..window_len(&world, centre) {
        match receive(&mut socket).await {
            Some(ServerMessage::Chunk {
                x,
                y,
                tiles,
                bodies,
                items,
            }) => {
                assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
                for i in &items {
                    assert_eq!(Pos::new(i.x, i.y).chunk(), ChunkPos::new(x, y));
                }
                seen_items += items.len();
                assert!((x - centre.x).abs() <= VIEW_RADIUS && (y - centre.y).abs() <= VIEW_RADIUS);
                for b in &bodies {
                    assert_eq!(Pos::new(b.x, b.y).chunk(), ChunkPos::new(x, y));
                }
                seen_bodies.extend(bodies.iter().map(|b| b.id));
                chunks.push((x, y));
            }
            other => panic!("expected a chunk, got {other:?}"),
        }
    }
    assert_eq!(
        chunks.len(),
        25,
        "a full window in the middle of the valley"
    );
    assert!(
        seen_items > 100,
        "{seen_items} items in view: the ground is scattered"
    );
    seen_bodies.sort_unstable();
    assert_eq!(
        seen_bodies,
        vec![1, 2, 3],
        "the three bodies start close together"
    );

    send(&mut socket, &ClientMessage::Ping { nonce: 7 }).await;
    // A day boundary may slip in before the pong: skip those.
    let pong = loop {
        match receive(&mut socket).await {
            Some(ServerMessage::DayStarted { .. }) => {}
            other => break other,
        }
    };
    assert_eq!(pong, Some(ServerMessage::Pong { nonce: 7 }));

    // One game day lasts one real second: a day boundary must be published.
    let (day, tick) = match receive(&mut socket).await {
        Some(ServerMessage::DayStarted { day, tick }) => (day, tick),
        Some(other) => panic!("unexpected {other:?}"),
        None => panic!("connection closed before a day started"),
    };
    assert!(day >= 2);
    assert_eq!(tick % 20, 0, "a day starts on a day boundary");

    socket.close(None).await.expect("clean close");
}

#[tokio::test]
async fn wrong_protocol_version_is_refused() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION + 1,
        },
    )
    .await;
    assert_eq!(
        receive(&mut socket).await,
        Some(ServerMessage::ProtocolMismatch {
            expected: PROTOCOL_VERSION
        })
    );
    assert_eq!(receive(&mut socket).await, None, "server closes afterwards");
}

#[tokio::test]
async fn anything_but_hello_first_is_disconnected() {
    let server = Server::start();
    let mut socket = server.connect().await;

    send(&mut socket, &ClientMessage::Ping { nonce: 1 }).await;
    assert_eq!(receive(&mut socket).await, None);
}

#[tokio::test]
async fn several_clients_each_get_the_same_days() {
    let server = Server::start();
    let mut a = server.connect().await;
    let mut b = server.connect().await;
    for socket in [&mut a, &mut b] {
        send(
            socket,
            &ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await;
        assert!(matches!(
            receive(socket).await,
            Some(ServerMessage::Welcome { .. })
        ));
    }

    let day_a = next_day(&mut a).await;
    let day_b = next_day(&mut b).await;
    assert_eq!(day_a, day_b);
}

/// Opens a piloting connection: hello, welcome, `world_state` and the
/// initial chunks consumed.
async fn join(server: &Server) -> (Socket, Option<u32>) {
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
    let body = match receive(&mut socket).await {
        Some(ServerMessage::Welcome { body, .. }) => body,
        other => panic!("expected welcome, got {other:?}"),
    };
    assert!(matches!(
        receive(&mut socket).await,
        Some(ServerMessage::WorldState { .. })
    ));
    let world = standard_world();
    let centre = if let Some(body) = body {
        start_of(&world, body).chunk()
    } else {
        let middle = i32::try_from(VALLEY_SIZE / 2).expect("small valley");
        Pos::new(middle, middle).chunk()
    };
    for _ in 0..window_len(&world, centre) {
        assert!(matches!(
            receive(&mut socket).await,
            Some(ServerMessage::Chunk { .. })
        ));
    }
    (socket, body)
}

// --- The view follows the body (D08, D31) --------------------------------

#[tokio::test]
async fn the_view_follows_the_piloted_body() {
    let server = Server::start_with(&["--time-scale", "20"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Walk into the chunk west of the starting one: a column of chunks
    // is hidden to the east and a column shown to the west.
    let world = standard_world();
    let start = start_of(&world, body);
    let here = start.chunk();
    let target = Pos::new(here.origin().x - 1, start.y);
    steps(&world, start, target); // reachable
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;

    let (mut shown, mut hidden) = (Vec::new(), Vec::new());
    while shown.len() < 5 || hidden.len() < 5 {
        match receive(&mut pilot).await {
            Some(ServerMessage::Chunk { x, y, .. }) => shown.push((x, y)),
            Some(ServerMessage::ChunkHidden { x, y }) => hidden.push((x, y)),
            Some(ServerMessage::BodyMoved { .. } | ServerMessage::DayStarted { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(
        shown.iter().all(|(x, _)| *x == here.x - VIEW_RADIUS - 1),
        "{shown:?}"
    );
    assert!(
        hidden.iter().all(|(x, _)| *x == here.x + VIEW_RADIUS),
        "{hidden:?}"
    );
}

#[tokio::test]
async fn a_body_walking_out_of_the_view_is_reported_as_left() {
    let server = Server::start_with(&["--time-scale", "50"]);
    let (mut walker, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("pilots body 1");
    let other = other.expect("pilots body 2");

    // Body 1 walks far west across the ford, out of body 2's window;
    // body 2 stays.
    let world = standard_world();
    let start = start_of(&world, body);
    let far = world
        .map()
        .nearest(Pos::new(start.x - 100, start.y), sim::Terrain::Grass)
        .expect("grass far west");
    let span = i32::try_from(CHUNK_SIZE).expect("small chunk") * (VIEW_RADIUS + 1);
    assert!(
        start_of(&world, other).x - far.x >= span,
        "far enough: {far:?}"
    );
    steps(&world, start, far); // reachable
    send(&mut walker, &ClientMessage::MoveTo { x: far.x, y: far.y }).await;

    let watched_chunks = {
        let centre = start_of(&world, other).chunk();
        move |pos: Pos| {
            let c = pos.chunk();
            (c.x - centre.x).abs() <= VIEW_RADIUS && (c.y - centre.y).abs() <= VIEW_RADIUS
        }
    };
    loop {
        match receive(&mut watcher).await {
            Some(ServerMessage::BodyLeft { body: who }) => {
                assert_eq!(who, body);
                break;
            }
            Some(ServerMessage::BodyMoved {
                body: who, x, y, ..
            }) => {
                assert!(
                    who != body || watched_chunks(Pos::new(x, y)),
                    "a move outside the watcher's view leaked: ({x}, {y})"
                );
            }
            Some(ServerMessage::DayStarted { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    // The walker itself keeps seeing its own moves and receiving ground.
    let mut own_moves = 0;
    while own_moves < 3 {
        match receive(&mut walker).await {
            Some(ServerMessage::BodyMoved { body: who, .. }) if who == body => own_moves += 1,
            Some(ServerMessage::BodyLeft { body: who }) => {
                assert_ne!(who, body, "never leaves itself");
            }
            Some(_) => {}
            None => panic!("walker disconnected"),
        }
    }
}

#[tokio::test]
async fn a_move_intention_makes_the_piloted_body_walk() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("first connection pilots a body");
    assert_ne!(Some(body), other, "two connections never share a body");

    // Walk a few tiles east of where body 1 starts.
    let world = standard_world();
    let start = start_of(&world, body);
    let target = Pos::new(start.x + 3, start.y);
    let expected = steps(&world, start, target);
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;

    let mut seen_by_pilot = Vec::new();
    let mut seen_by_watcher = Vec::new();
    for (socket, seen) in [
        (&mut pilot, &mut seen_by_pilot),
        (&mut watcher, &mut seen_by_watcher),
    ] {
        while seen.len() < expected.len() {
            match receive(socket).await {
                Some(ServerMessage::BodyMoved {
                    body: who, x, y, ..
                }) if who == body => seen.push((x, y)),
                Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyMoved { .. }) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    assert_eq!(seen_by_pilot, expected, "the path the simulation computes");
    assert_eq!(
        seen_by_watcher, seen_by_pilot,
        "everyone sees the same walk"
    );
}

#[tokio::test]
async fn walking_into_the_cliff_is_refused() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Row 0 is the cliff: not walkable.
    send(&mut pilot, &ClientMessage::MoveTo { x: 100, y: 0 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(ServerMessage::MoveRefused { body: who, x, y }) => {
                assert_eq!((who, x, y), (body, 100, 0));
                break;
            }
            Some(ServerMessage::DayStarted { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_released_body_can_be_piloted_again() {
    let server = Server::start();
    let (first, body) = join(&server).await;
    assert_eq!(body, Some(1));
    drop(first);
    // The server releases the body when the connection ends.
    for _ in 0..50 {
        let (_, body) = join(&server).await;
        if body == Some(1) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("body 1 was never released");
}

// --- Limits from the milestone 1 audit ---------------------------------

#[test]
fn a_silent_tcp_connection_is_dropped() {
    let server = Server::start();
    // Plain TCP, never sending the WebSocket upgrade.
    let mut socket = std::net::TcpStream::connect(&server.addr).expect("tcp connects");
    socket
        .set_read_timeout(Some(Duration::from_secs(8)))
        .expect("timeout is set");
    let started = std::time::Instant::now();
    let mut buffer = [0u8; 16];
    let read = socket
        .read(&mut buffer)
        .expect("server closes, not a timeout");
    assert_eq!(read, 0, "server sends nothing, just closes");
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "closed by the handshake timeout, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn an_oversized_message_ends_the_connection() {
    let server = Server::start();
    let (mut pilot, _) = join(&server).await;
    let huge = format!(
        "{{\"type\":\"ping\",\"nonce\":1,\"pad\":\"{}\"}}",
        "x".repeat(server::net::MAX_MESSAGE_BYTES)
    );
    pilot
        .send(Message::text(huge))
        .await
        .expect("the frame leaves");
    assert_eq!(
        receive(&mut pilot).await,
        None,
        "server closes on an oversized frame"
    );
}

#[tokio::test]
async fn connections_beyond_the_limit_are_refused() {
    let server = Server::start_with(&["--max-connections", "1"]);
    let (mut first, body) = join(&server).await;
    assert_eq!(body, Some(1));

    // The second one is closed before any WebSocket upgrade.
    let second = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(format!("ws://{}", server.addr)),
    )
    .await
    .expect("refused promptly");
    assert!(second.is_err(), "no upgrade for the second connection");

    // The first one is unaffected.
    send(&mut first, &ClientMessage::Ping { nonce: 3 }).await;
    loop {
        match receive(&mut first).await {
            Some(ServerMessage::DayStarted { .. }) => {}
            other => {
                assert_eq!(other, Some(ServerMessage::Pong { nonce: 3 }));
                break;
            }
        }
    }

    // Its place is given back when it leaves.
    drop(first);
    for _ in 0..50 {
        if tokio_tungstenite::connect_async(format!("ws://{}", server.addr))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the place was never given back");
}

#[tokio::test]
async fn a_refusal_reaches_the_pilot_alone() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let (mut other, other_body) = join(&server).await;
    let body = body.expect("pilots body 1");
    let other_body = other_body.expect("pilots body 2");

    send(&mut pilot, &ClientMessage::MoveTo { x: 100, y: 0 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(ServerMessage::MoveRefused { body: who, .. }) => {
                assert_eq!(who, body);
                break;
            }
            Some(ServerMessage::DayStarted { .. }) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }

    // Body 2 walks one tile east. Messages keep their order, so a
    // broadcast refusal would have arrived before this move.
    let world = standard_world();
    let start = start_of(&world, other_body);
    let target = Pos::new(start.x + 1, start.y);
    assert_eq!(
        steps(&world, start, target).len(),
        1,
        "one step east is open"
    );
    send(
        &mut other,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;
    loop {
        match receive(&mut other).await {
            Some(ServerMessage::BodyMoved { body: who, .. }) if who == other_body => break,
            Some(ServerMessage::MoveRefused { .. }) => {
                panic!("another pilot's refusal leaked")
            }
            Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyMoved { .. }) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }
}

#[tokio::test]
async fn a_flood_of_intentions_leaves_only_the_last_one() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots body 1");

    // Thousands of contradicting asks (the cliff, then far east), the
    // last one being one step north: the body ends there, and the server
    // is still answering.
    let world = standard_world();
    let start = start_of(&world, body);
    let north = Pos::new(start.x, start.y - 1);
    assert_eq!(
        steps(&world, start, north).len(),
        1,
        "one step north is open"
    );
    for i in 0..2000 {
        let target = if i % 2 == 0 {
            (100, 0)
        } else {
            (start.x + 40, start.y)
        };
        send(
            &mut pilot,
            &ClientMessage::MoveTo {
                x: target.0,
                y: target.1,
            },
        )
        .await;
    }
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: north.x,
            y: north.y,
        },
    )
    .await;
    send(&mut pilot, &ClientMessage::Ping { nonce: 9 }).await;

    let mut arrived = false;
    let mut answered = false;
    let mut refusals = 0;
    while !(arrived && answered) {
        match receive(&mut pilot).await {
            Some(ServerMessage::BodyMoved {
                body: who, x, y, ..
            }) if who == body => {
                arrived = (x, y) == (north.x, north.y);
            }
            Some(ServerMessage::Pong { nonce: 9 }) => answered = true,
            Some(ServerMessage::MoveRefused { .. }) => refusals += 1,
            // The flood's eastward asks may carry the body across a chunk.
            Some(
                ServerMessage::DayStarted { .. }
                | ServerMessage::Chunk { .. }
                | ServerMessage::ChunkHidden { .. }
                | ServerMessage::BodyLeft { .. },
            ) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }
    // At most one intention per tick survives: a refusal per tick the flood
    // spanned, never one per message.
    assert!(
        refusals < 100,
        "{refusals} refusals: the flood was not coalesced"
    );
}
