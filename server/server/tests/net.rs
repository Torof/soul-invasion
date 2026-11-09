//! Drives the real server binary over WebSocket with a small fake client:
//! handshake, liveness, published events, and the ways a client gets
//! disconnected.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PROTOCOL_VERSION, Point, ServerMessage, decode, encode};
use server::net::VIEW_RADIUS;
use sim::{BodyId, CHUNK_SIZE, Calendar, ChunkPos, FinePos, Pos, VALLEY_SIZE, World};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The world the server starts with: the standard valley, so tests can
/// know where bodies stand and which way they will walk.
fn rules() -> sim::Content {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    server::content::load(&dir).expect("repository content")
}

/// The world a fresh server starts with, generated once for all the
/// tests: the valley is large, and its generation must not eat into a
/// running server's short days.
fn standard_world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| World::new(Calendar::default(), &rules()))
}

/// The tiles a body standing at `from` can walk to, through straight
/// steps only (a diagonal never opens a way a path would not take).
fn reachable(world: &World, from: Pos) -> HashSet<Pos> {
    let rules = rules();
    let mut seen = HashSet::from([from]);
    let mut todo = vec![from];
    while let Some(p) = todo.pop() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = Pos::new(p.x + dx, p.y + dy);
            let passable = world
                .map()
                .get(next)
                .is_some_and(|t| rules.movement.cost(t).is_some());
            if passable && seen.insert(next) {
                todo.push(next);
            }
        }
    }
    seen
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

/// The route the standard world will make a body walk: straight
/// segments, as points.
fn route(world: &World, from: Pos, to: Pos) -> Vec<Point> {
    let rules = rules();
    let cost = |t: sim::Terrain| rules.movement.cost(t);
    let path = world
        .map()
        .path(from, to, cost)
        .expect("reachable in the standard valley");
    world
        .map()
        .pull(from.centre(), &path, cost)
        .into_iter()
        .map(|p| Point { x: p.x, y: p.y })
        .collect()
}

fn tile(point: Point) -> Pos {
    FinePos::new(point.x, point.y).tile()
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
        standard_world();
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

/// Messages that flow all the time, whatever the test is about: a body's
/// condition to its pilot, sleep, night, items coming and going.
fn ambient(message: &ServerMessage) -> bool {
    matches!(
        message,
        ServerMessage::BodyCondition { .. }
            | ServerMessage::BodySleeps { .. }
            | ServerMessage::NightFell { .. }
            | ServerMessage::ItemAdded { .. }
            | ServerMessage::ItemRemoved { .. }
            | ServerMessage::BodyHolds { .. }
            | ServerMessage::Inventory { .. }
            | ServerMessage::FarView { .. }
    )
}

/// Next server message that is not ambient, or `None` once the server
/// closed the connection. Ambient messages never stop flowing, so this
/// gives up after 20 s instead of waiting forever.
async fn receive(socket: &mut Socket) -> Option<ServerMessage> {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match receive_any(socket).await {
                Some(message) if ambient(&message) => {}
                other => return other,
            }
        }
    })
    .await
    .expect("a message of interest within 20 s")
}

/// Next server message of any kind, or `None` once the server closed
/// the connection.
async fn receive_any(socket: &mut Socket) -> Option<ServerMessage> {
    loop {
        // Generous: twenty servers start a 3 km valley at once.
        let frame = tokio::time::timeout(Duration::from_secs(15), socket.next())
            .await
            .expect("server answers within 15 s");
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
#[allow(clippy::too_many_lines)]
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
            species,
            flora,
            clock,
            ..
        }) => {
            assert!(flora.len() >= 29, "the shrubs, plants and mushrooms");
            assert!(
                flora
                    .iter()
                    .all(|k| ["shrub", "plant", "mushroom"].contains(&k.family.as_str()))
            );
            assert_eq!((width, height), (VALLEY_SIZE, VALLEY_SIZE));
            assert!(species.len() >= 6, "the kinds of tree");
            assert!(species.iter().all(|s| s.height_cm >= 100));
            assert_eq!(chunk_size, CHUNK_SIZE);
            assert!(materials.len() >= 9, "the catalogue of materials");
            assert!(materials.iter().all(|m| m.colour.starts_with('#')));
            assert_eq!((clock.ticks_per_second, clock.ticks_per_day), (20, 20));
            assert_eq!(clock.time_scale_permille, 1000);
        }
        other => panic!("expected world_state, got {other:?}"),
    }
    let world = standard_world();
    let centre = start_of(world, 1).chunk();
    let mut seen_bodies = Vec::new();
    let mut seen_items = 0;
    let mut seen_trees = 0;
    let mut chunks = Vec::new();
    let mut seen_plants = 0;
    for _ in 0..window_len(world, centre) {
        match receive(&mut socket).await {
            Some(ServerMessage::Chunk {
                x,
                y,
                tiles,
                bodies,
                items,
                trees,
                plants,
                currents,
            }) => {
                for p in &plants {
                    assert_eq!(
                        tile(Point { x: p.x, y: p.y }).chunk(),
                        ChunkPos::new(x, y),
                        "a plant lies in its chunk"
                    );
                }
                seen_plants += plants.len();
                // Water runs, land does not.
                assert_eq!(currents.len(), tiles.len());
                for (t, c) in tiles.iter().zip(&currents) {
                    let wet = matches!(
                        t,
                        protocol::Terrain::Water
                            | protocol::Terrain::Ford
                            | protocol::Terrain::Brook
                    );
                    assert_eq!(wet, *c != 0, "{t:?} {c}");
                }
                assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
                for t in &trees {
                    assert_eq!(tile(t.at).chunk(), ChunkPos::new(x, y));
                }
                seen_trees += trees.len();
                for i in &items {
                    assert_eq!(Pos::new(i.x, i.y).chunk(), ChunkPos::new(x, y));
                }
                seen_items += items.len();
                assert!((x - centre.x).abs() <= VIEW_RADIUS && (y - centre.y).abs() <= VIEW_RADIUS);
                for b in &bodies {
                    assert_eq!(tile(b.at).chunk(), ChunkPos::new(x, y));
                    assert_eq!(b.route, Vec::new(), "nobody walks at the start");
                    assert!(!b.asleep);
                }
                seen_bodies.extend(bodies.iter().map(|b| b.id));
                chunks.push((x, y));
            }
            other => panic!("expected a chunk, got {other:?}"),
        }
    }
    assert_eq!(
        chunks.len(),
        (2 * VIEW_RADIUS + 1).pow(2) as usize,
        "a full window in the middle of the valley"
    );
    assert!(
        seen_plants > 1000,
        "the flora grows around the camp: {seen_plants}"
    );
    // Then the far view, around the body's chunk.
    match receive_any(&mut socket).await {
        Some(ServerMessage::FarView {
            cell,
            width,
            height,
            cells,
            ..
        }) => {
            assert_eq!(cells.len(), (width * height) as usize);
            assert!(cell >= 8 && width >= 128);
            assert!(cells.contains('f') && cells.contains('g'));
        }
        other => panic!("expected the far view, got {other:?}"),
    }
    assert!(
        seen_items > 100,
        "{seen_items} items in view: the ground is scattered"
    );
    assert!(
        seen_trees > 100,
        "{seen_trees} trees in view: the woods stand"
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
        start_of(world, body).chunk()
    } else {
        let middle = i32::try_from(VALLEY_SIZE / 2).expect("small valley");
        Pos::new(middle, middle).chunk()
    };
    for _ in 0..window_len(world, centre) {
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
    let server = Server::start_with(&["--seconds-per-day", "100", "--time-scale", "20"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Walk into the chunk east of the starting one: a column of chunks
    // is hidden to the west and a column shown to the east.
    let world = standard_world();
    let start = start_of(world, body);
    let here = start.chunk();
    let target = Pos::new(
        here.origin().x + i32::try_from(CHUNK_SIZE).expect("small chunk"),
        start.y,
    );
    route(world, start, target); // reachable
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
            Some(
                ServerMessage::BodyWalks { .. }
                | ServerMessage::BodyStopped { .. }
                | ServerMessage::DayStarted { .. },
            ) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(
        shown.iter().all(|(x, _)| *x == here.x + VIEW_RADIUS + 1),
        "{shown:?}"
    );
    assert!(
        hidden.iter().all(|(x, _)| *x == here.x - VIEW_RADIUS),
        "{hidden:?}"
    );
}

#[tokio::test]
async fn a_body_walking_out_of_the_view_is_reported_as_left() {
    // Long days: the walk is 70 game seconds and must not end in a collapse.
    let server = Server::start_with(&["--seconds-per-day", "1000", "--time-scale", "50"]);
    let (mut walker, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("pilots body 1");
    let other = other.expect("pilots body 2");

    // Body 1 walks far east, out of body 2's window; body 2 stays.
    let world = standard_world();
    let start = start_of(world, body);
    let span = i32::try_from(CHUNK_SIZE).expect("small chunk") * (VIEW_RADIUS + 1);
    let east = start_of(world, other).x + span;
    let far = *reachable(world, start)
        .iter()
        .filter(|p| p.x >= east)
        .min_by_key(|p| ((p.x - start.x).abs() + (p.y - start.y).abs(), p.x, p.y))
        .expect("ground far east");
    assert!(
        far.x - start_of(world, other).x >= span,
        "far enough: {far:?}"
    );
    route(world, start, far); // reachable
    send(&mut walker, &ClientMessage::MoveTo { x: far.x, y: far.y }).await;

    // The watcher sees the walk start (inside its view), then the body
    // leave; never where it stops, which is outside.
    let mut saw_walk = false;
    loop {
        match receive(&mut watcher).await {
            Some(ServerMessage::BodyLeft { body: who }) => {
                assert_eq!(who, body);
                break;
            }
            Some(ServerMessage::BodyWalks { body: who, .. }) if who == body => saw_walk = true,
            Some(ServerMessage::BodyStopped { body: who, .. }) => {
                assert_ne!(who, body, "a stop outside the watcher's view leaked");
            }
            Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(saw_walk);
    // The walker itself keeps receiving ground and finally its own stop.
    loop {
        match receive(&mut walker).await {
            Some(ServerMessage::BodyStopped { body: who, at }) if who == body => {
                assert_eq!(tile(at), far);
                break;
            }
            Some(ServerMessage::BodyLeft { body: who }) => {
                assert_ne!(who, body, "never leaves itself");
            }
            Some(_) => {}
            None => panic!("walker disconnected"),
        }
    }
}

// --- Development tools (--dev) -----------------------------------------

#[tokio::test]
async fn with_dev_the_client_gets_the_overview_and_may_teleport() {
    let server = Server::start_with(&["--dev", "--seconds-per-day", "1000"]);
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await;
    let Some(ServerMessage::Welcome { body, .. }) = receive(&mut socket).await else {
        panic!("expected welcome");
    };
    let body = body.expect("pilots a body");
    assert!(matches!(
        receive(&mut socket).await,
        Some(ServerMessage::WorldState { .. })
    ));
    // The overview: the whole map, one letter a cell.
    let Some(ServerMessage::DevOverview {
        cell,
        width,
        height,
        cells,
    }) = receive(&mut socket).await
    else {
        panic!("expected the overview");
    };
    assert_eq!(width, VALLEY_SIZE.div_ceil(cell));
    assert_eq!(height, VALLEY_SIZE.div_ceil(cell));
    assert_eq!(cells.len(), (width * height) as usize);
    assert!(cells.chars().all(|c| "gfwobr".contains(c)));
    assert!(cells.contains('w') && cells.contains('f') && cells.starts_with('r'));
    let world = standard_world();
    for _ in 0..window_len(world, start_of(world, body).chunk()) {
        assert!(matches!(
            receive(&mut socket).await,
            Some(ServerMessage::Chunk { .. })
        ));
    }
    // Far away at once: the view follows, the body stands there.
    let start = start_of(world, body);
    let far = world
        .map()
        .nearest(Pos::new(start.x - 600, start.y + 300), sim::Terrain::Grass)
        .expect("grass far away");
    send(
        &mut socket,
        &ClientMessage::DevTeleport { x: far.x, y: far.y },
    )
    .await;
    let (mut shown, mut hidden) = (0, 0);
    loop {
        match receive(&mut socket).await {
            Some(ServerMessage::Chunk { .. }) => shown += 1,
            Some(ServerMessage::ChunkHidden { .. }) => hidden += 1,
            Some(ServerMessage::BodyStopped { body: who, at }) if who == body => {
                assert_eq!(tile(at), far);
                break;
            }
            Some(_) => {}
            None => panic!("disconnected"),
        }
    }
    assert!(shown > 0 && hidden > 0, "shown {shown}, hidden {hidden}");
}

#[tokio::test]
async fn without_dev_a_teleport_is_ignored() {
    let server = Server::start_with(&["--seconds-per-day", "1000"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");
    let start = start_of(standard_world(), body);
    send(
        &mut pilot,
        &ClientMessage::DevTeleport {
            x: start.x - 600,
            y: start.y,
        },
    )
    .await;
    send(&mut pilot, &ClientMessage::Ping { nonce: 5 }).await;
    // Give the simulation a few ticks, then ask again: no move came.
    tokio::time::sleep(Duration::from_millis(300)).await;
    send(&mut pilot, &ClientMessage::Ping { nonce: 6 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(ServerMessage::Pong { nonce: 6 }) => break,
            Some(ServerMessage::Pong { .. } | ServerMessage::DayStarted { .. }) => {}
            other => panic!("nothing but pongs expected, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_move_intention_makes_the_piloted_body_walk() {
    let server = Server::start_with(&["--seconds-per-day", "100"]);
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("first connection pilots a body");
    assert_ne!(Some(body), other, "two connections never share a body");

    // Walk a few tiles east of where body 1 starts.
    let world = standard_world();
    let start = start_of(world, body);
    let target = Pos::new(start.x + 3, start.y);
    let expected = route(world, start, target);
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;

    // Both see the body set off along the route the simulation computed,
    // then stop exactly on the target's centre.
    for socket in [&mut pilot, &mut watcher] {
        let (mut walked, mut stopped) = (None, None);
        while stopped.is_none() {
            match receive(socket).await {
                Some(ServerMessage::BodyWalks {
                    body: who,
                    at,
                    route,
                    speed_cm_per_s,
                    ..
                }) if who == body => {
                    assert_eq!(
                        at,
                        Point {
                            x: start.centre().x,
                            y: start.centre().y
                        }
                    );
                    assert_eq!(speed_cm_per_s, 143);
                    walked = Some(route);
                }
                Some(ServerMessage::BodyStopped { body: who, at }) if who == body => {
                    stopped = Some(at);
                }
                Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. }) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(
            walked,
            Some(expected.clone()),
            "the route the simulation computes"
        );
        assert_eq!(stopped.map(tile), Some(target));
    }
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
    // The clock may strike a new day meanwhile: that is not an answer.
    let mut last = receive(&mut pilot).await;
    while matches!(
        last,
        Some(ServerMessage::DayStarted { .. } | ServerMessage::NightFell { .. })
    ) {
        last = receive(&mut pilot).await;
    }
    assert_eq!(last, None, "server closes on an oversized frame");
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
    let server = Server::start_with(&["--seconds-per-day", "100"]);
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
    // broadcast refusal would have arrived before this walk.
    let world = standard_world();
    let start = start_of(world, other_body);
    let target = Pos::new(start.x + 1, start.y);
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
            Some(ServerMessage::BodyWalks { body: who, .. }) if who == other_body => break,
            Some(ServerMessage::MoveRefused { .. }) => {
                panic!("another pilot's refusal leaked")
            }
            Some(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. }) => {}
            unexpected => panic!("unexpected {unexpected:?}"),
        }
    }
}

#[tokio::test]
async fn a_flood_of_intentions_leaves_only_the_last_one() {
    let server = Server::start_with(&["--seconds-per-day", "100"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots body 1");

    // Thousands of contradicting asks (the cliff, then far east), the
    // last one being one step north: the body ends there, and the server
    // is still answering.
    let world = standard_world();
    let start = start_of(world, body);
    let north = Pos::new(start.x, start.y - 1);
    assert_eq!(
        route(world, start, north).len(),
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
            Some(ServerMessage::BodyStopped { body: who, at }) if who == body => {
                arrived = tile(at) == north;
            }
            Some(ServerMessage::Pong { nonce: 9 }) => answered = true,
            Some(ServerMessage::MoveRefused { .. }) => refusals += 1,
            // The flood's eastward asks may carry the body across a chunk.
            Some(
                ServerMessage::DayStarted { .. }
                | ServerMessage::BodyWalks { .. }
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

// --- Needs, sleep, food and death (D34) --------------------------------

/// Skips messages until `pick` returns something; fails after 40 s
/// (generous: twenty servers run at once).
async fn wait_for<T>(socket: &mut Socket, mut pick: impl FnMut(&ServerMessage) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let message = receive_any(socket).await.expect("still connected");
            if let Some(found) = pick(&message) {
                return found;
            }
        }
    })
    .await
    .expect("what we wait for arrives within 40 s")
}

#[tokio::test]
async fn the_pilot_is_told_how_its_body_fares_and_nobody_else() {
    // One game day per real second: hunger climbs a point every 30 ms.
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let (mut other, other_body) = join(&server).await;
    let body = body.expect("pilots body 1");
    let other_body = other_body.expect("pilots body 2");

    let condition = wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyCondition {
            body: who,
            condition,
        } if *who == body => Some(*condition),
        _ => None,
    })
    .await;
    assert!(
        condition.hunger >= 1 && condition.vitality == 100,
        "{condition:?}"
    );

    // The other pilot hears of its own body only.
    for _ in 0..5 {
        let who = wait_for(&mut other, |m| match m {
            ServerMessage::BodyCondition { body: who, .. } => Some(*who),
            _ => None,
        })
        .await;
        assert_eq!(who, other_body, "another body's condition leaked");
    }
}

#[tokio::test]
async fn sleeping_is_seen_by_all_and_waking_by_walking() {
    // Ten-second days: tired enough to sleep soon, not so fast as to
    // collapse before walking.
    let server = Server::start_with(&["--seconds-per-day", "10"]);
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, _) = join(&server).await;
    let body = body.expect("pilots body 1");

    // A little tired first, or the body would wake at once (it is day).
    wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyCondition { condition, .. } if condition.fatigue >= 5 => Some(()),
        _ => None,
    })
    .await;
    send(&mut pilot, &ClientMessage::Sleep).await;
    let asleep = wait_for(&mut watcher, |m| match m {
        ServerMessage::BodySleeps { body: who, asleep } if *who == body => Some(*asleep),
        _ => None,
    })
    .await;
    assert!(asleep);
    let condition = wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyCondition { condition, .. } if condition.asleep => Some(*condition),
        _ => None,
    })
    .await;
    assert!(condition.asleep);

    // Walking wakes the body: everyone sees it get up and set off.
    let world = standard_world();
    let start = start_of(world, body);
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: start.x + 2,
            y: start.y,
        },
    )
    .await;
    let awake = wait_for(&mut watcher, |m| match m {
        ServerMessage::BodySleeps { body: who, asleep } if *who == body => Some(!*asleep),
        _ => None,
    })
    .await;
    assert!(awake);
    wait_for(&mut watcher, |m| match m {
        ServerMessage::BodyWalks { body: who, .. } if *who == body => Some(()),
        _ => None,
    })
    .await;
}

#[tokio::test]
async fn eating_takes_an_item_within_reach_and_refuses_the_rest() {
    let server = Server::start_with(&["--seconds-per-day", "1000", "--time-scale", "20"]);
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, _) = join(&server).await;
    let body = body.expect("pilots body 1");

    // An item far away cannot be eaten from here.
    send(&mut pilot, &ClientMessage::Eat { item: 1 }).await;
    let refused = wait_for(&mut pilot, |m| match m {
        ServerMessage::ActRefused {
            body: who,
            act: protocol::ItemAct::Eat,
            item,
            reason,
        } if *who == body => Some((*item, *reason)),
        _ => None,
    })
    .await;
    assert_eq!(refused, (1, protocol::Refusal::OutOfReach));

    // Walk to the nearest berries, then eat them: the watcher sees them go.
    let world = standard_world();
    let rules = rules();
    let start = start_of(world, body);
    let around = reachable(world, start);
    let berries = world
        .items()
        .iter()
        .filter(|i| {
            rules
                .material(i.material())
                .is_some_and(|m| m.food.is_some())
                && around.contains(&i.pos().expect("on the ground"))
        })
        .min_by_key(|i| {
            let p = i.pos().expect("on the ground");
            (p.x - start.x).abs() + (p.y - start.y).abs()
        })
        .expect("food in the valley");
    let berries_at = berries.pos().expect("on the ground");
    send(
        &mut pilot,
        &ClientMessage::MoveTo {
            x: berries_at.x,
            y: berries_at.y,
        },
    )
    .await;
    wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyStopped { body: who, at } if *who == body => {
            (tile(*at) == berries_at).then_some(())
        }
        _ => None,
    })
    .await;
    send(
        &mut pilot,
        &ClientMessage::Eat {
            item: berries.id().0,
        },
    )
    .await;
    let gone = wait_for(&mut watcher, |m| match m {
        ServerMessage::ItemRemoved { item, .. } => Some(*item),
        _ => None,
    })
    .await;
    assert_eq!(gone, berries.id().0);
}

#[tokio::test]
async fn a_starved_body_dies_and_its_pilot_is_given_another_or_watches() {
    // Days of one second, run fifty times faster: a body starves in well
    // under a real second.
    let server = Server::start_with(&["--time-scale", "50"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots body 1");

    let cause = wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyDied { body: who, cause } if *who == body => Some(*cause),
        _ => None,
    })
    .await;
    assert!(matches!(
        cause,
        protocol::DeathCause::Starved | protocol::DeathCause::Froze
    ));
    // The corpse appeared, the pilot got another body or none. The other
    // bodies starve at the same moment, so the body it is given may die
    // under it at once and be replaced in turn; with a living body, the
    // pilot is settled in it: the ground around it comes when the view
    // moves, its condition in any case.
    let mut assigned = wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyAssigned { body } => Some(*body),
        _ => None,
    })
    .await;
    while let Some(new_body) = assigned {
        assert_ne!(new_body, body, "never the dead body");
        let settled = wait_for(&mut pilot, |m| match m {
            ServerMessage::Chunk { .. } => Some(true),
            ServerMessage::BodyCondition { body: who, .. } if *who == new_body => Some(true),
            ServerMessage::BodyDied { body: who, .. } if *who == new_body => Some(false),
            _ => None,
        })
        .await;
        if settled {
            break;
        }
        assigned = wait_for(&mut pilot, |m| match m {
            ServerMessage::BodyAssigned { body } => Some(*body),
            _ => None,
        })
        .await;
    }
}

#[tokio::test]
async fn picking_up_tells_the_pilot_its_inventory_and_everyone_what_is_in_hand() {
    let server = Server::start_with(&["--seconds-per-day", "1000", "--time-scale", "20"]);
    let (mut pilot, body) = join(&server).await;
    let (mut watcher, _) = join(&server).await;
    let body = body.expect("pilots body 1");

    // Walk to the nearest item, pick it up: it leaves the ground, goes in
    // hand, and the watcher sees the hand.
    let world = standard_world();
    let start = start_of(world, body);
    let nearest = world
        .items()
        .iter()
        .min_by_key(|i| {
            let p = i.pos().expect("on the ground");
            (p.x - start.x).abs() + (p.y - start.y).abs()
        })
        .expect("items in the valley");
    let at = nearest.pos().expect("on the ground");
    send(&mut pilot, &ClientMessage::MoveTo { x: at.x, y: at.y }).await;
    wait_for(&mut pilot, |m| match m {
        ServerMessage::BodyStopped {
            body: who,
            at: here,
        } if *who == body => (tile(*here) == at).then_some(()),
        _ => None,
    })
    .await;
    send(
        &mut pilot,
        &ClientMessage::PickUp {
            item: nearest.id().0,
        },
    )
    .await;
    let inventory = wait_for(&mut pilot, |m| match m {
        ServerMessage::Inventory {
            body: who,
            held,
            load_g,
            capacity_g,
            ..
        } if *who == body => Some((held.clone(), *load_g, *capacity_g)),
        _ => None,
    })
    .await;
    assert!(inventory.1 > 0 && inventory.2 == 20_000, "{inventory:?}");
    assert_eq!(inventory.0.map(|i| i.id), Some(nearest.id().0));
    // In the order the simulation says it: off the ground, then in hand.
    wait_for(&mut watcher, |m| match m {
        ServerMessage::ItemRemoved { item, .. } if *item == nearest.id().0 => Some(()),
        _ => None,
    })
    .await;
    let held = wait_for(&mut watcher, |m| match m {
        ServerMessage::BodyHolds { body: who, item } if *who == body => Some(item.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        held.map(|i| i.material),
        Some(nearest.material().to_owned())
    );

    // Drop it: back on the ground for all, hand empty.
    send(
        &mut pilot,
        &ClientMessage::Drop {
            item: nearest.id().0,
        },
    )
    .await;
    wait_for(&mut watcher, |m| match m {
        ServerMessage::ItemAdded { item } if item.id == nearest.id().0 => Some(()),
        _ => None,
    })
    .await;
    let held = wait_for(&mut watcher, |m| match m {
        ServerMessage::BodyHolds { body: who, item } if *who == body => Some(item.clone()),
        _ => None,
    })
    .await;
    assert!(held.is_none());

    // Dropping it again is refused, with a reason.
    send(
        &mut pilot,
        &ClientMessage::Drop {
            item: nearest.id().0,
        },
    )
    .await;
    let reason = wait_for(&mut pilot, |m| match m {
        ServerMessage::ActRefused {
            act: protocol::ItemAct::Drop,
            reason,
            ..
        } => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, protocol::Refusal::NotCarried);
}
