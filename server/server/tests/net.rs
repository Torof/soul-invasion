//! Drives the real server binary over WebSocket with a small fake client:
//! handshake, liveness, published events, and the ways a client gets
//! disconnected.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use protocol::{
    ChunkFrame, ClientMessage, PROTOCOL_VERSION, Point, ServerMessage, decode, decode_chunk, encode,
};
use server::net::VIEW_RADIUS;
use sim::{BodyId, CHUNK_SIZE, Calendar, ChunkPos, FinePos, Pos, World};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The world the server starts with: the standard valley, so tests can
/// know where bodies stand and which way they will walk.
/// The content the tests' servers run with: the repository's, with
/// hunger and the berries at the forest's edge that the game has paused
/// while gathering is to come (10 November 2026): eating, starving and
/// what a body is told of them are still the server's to get right.
fn content_dir() -> &'static std::path::Path {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let dir = std::env::temp_dir().join(format!("soul-invasion-net-content-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a test content directory");
        for entry in std::fs::read_dir(&repo).expect("repository content") {
            let path = entry.expect("an entry").path();
            if path.extension().is_some_and(|e| e == "ron") {
                let mut text = std::fs::read_to_string(&path).expect("a content file");
                match path.file_name().and_then(|n| n.to_str()) {
                    Some("needs.ron") => {
                        text = text.replace("hunger_per_day: 0,", "hunger_per_day: 33,");
                    }
                    Some("spawns.ron") => {
                        let berries = "    (material: \"berries\", terrain: grass, per_thousand: 60, \
                                       near: Some((terrain: forest, within: 2))),\n]";
                        text = text.replacen("\n]", &format!("\n{berries}"), 1);
                    }
                    _ => {}
                }
                std::fs::write(dir.join(path.file_name().expect("a name")), text).expect("written");
            }
        }
        dir
    })
}

/// The items lying in the standard world: the ground's, made per chunk.
fn ground_items(world: &World) -> Vec<sim::Item> {
    let side = i32::try_from(VALLEY.div_ceil(CHUNK_SIZE)).expect("few chunks");
    (0..side)
        .flat_map(|y| (0..side).map(move |x| ChunkPos::new(x, y)))
        .flat_map(|c| world.items_in(c))
        .collect()
}

fn rules() -> sim::Content {
    server::content::load(content_dir()).expect("the tests' content")
}

/// The world a fresh server starts with, generated once for all the
/// tests: the valley is large, and its generation must not eat into a
/// running server's short days.
/// The tests' valley: smaller than the game's (`--valley-size`), so each
/// server they start is up in seconds; the same rules apply.
const VALLEY: u32 = 768;

fn standard_world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| World::with_valley(Calendar::default(), &rules(), VALLEY))
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
/// The stops of a walk from `start` to the nearest ground past `east`,
/// in legs the server itself can plan: each leg ends at the farthest
/// tile of a walkable way that the server's own search (within what the
/// body sees from its terrain, under the content's budget, D32) joins to
/// the leg's start. The last stop is the far ground itself.
fn legs_east(world: &World, rules: &sim::Content, start: Pos, east: i32) -> Vec<Pos> {
    let cost = |t: sim::Terrain| rules.movement.cost(t);
    let sight = |from: Pos| {
        let terrain = world.map().get(from).expect("on the map");
        i32::try_from(rules.movement.sight_tiles(terrain)).expect("a short sight")
    };
    let ground = reachable(world, start);
    let far = {
        let mut candidates: Vec<_> = ground.iter().filter(|p| p.x >= east).copied().collect();
        candidates.sort_by_key(|p| ((p.x - start.x).abs() + (p.y - start.y).abs(), p.x, p.y));
        candidates[0]
    };
    // A walkable way by straight steps: any way does.
    let way = {
        let mut before = HashMap::from([(start, start)]);
        let mut todo = VecDeque::from([start]);
        while let Some(p) = todo.pop_front() {
            if p == far {
                break;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = Pos::new(p.x + dx, p.y + dy);
                if ground.contains(&next) && !before.contains_key(&next) {
                    before.insert(next, p);
                    todo.push_back(next);
                }
            }
        }
        let mut way = vec![far];
        while *way.last().expect("a tile") != start {
            way.push(before[way.last().expect("a tile")]);
        }
        way.reverse();
        way
    };
    let mut stops = Vec::new();
    let (mut from, mut index) = (start, 0);
    while from != far {
        let sight = sight(from);
        let budget = rules.movement.path_budget_tiles;
        let reached = way
            .iter()
            .enumerate()
            .skip(index + 1)
            .rev()
            .filter(|(_, p)| (p.x - from.x).abs() <= sight && (p.y - from.y).abs() <= sight)
            .find(|(_, p)| {
                world
                    .map()
                    .path_within(from, **p, sight, budget, cost)
                    .is_some()
            })
            .map(|(j, _)| j)
            .expect("the next tile of the way is in sight");
        (from, index) = (way[reached], reached);
        stops.push(from);
    }
    stops
}

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
            .args(["--valley-size", &VALLEY.to_string()])
            .args([
                "--ephemeral",
                "--listen",
                "127.0.0.1:0",
                "--ticks-per-second",
                "20",
                "--seconds-per-day",
                "1",
            ])
            .args(["--content", content_dir().to_str().expect("a UTF-8 path")])
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
fn ambient(message: &Got) -> bool {
    let Got::M(message) = message else {
        return false;
    };
    matches!(
        message,
        ServerMessage::BodyCondition { .. }
            | ServerMessage::ChunkBodies { .. }
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
async fn receive(socket: &mut Socket) -> Option<Got> {
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

/// What the server sent: a message, or a chunk's binary frame decoded.
#[derive(Debug, PartialEq, Eq)]
enum Got {
    M(ServerMessage),
    Chunk(ChunkFrame),
}

/// Next server message of any kind, or `None` once the server closed
/// the connection.
async fn receive_any(socket: &mut Socket) -> Option<Got> {
    loop {
        // Generous: twenty servers start a 3 km valley at once.
        let frame = tokio::time::timeout(Duration::from_secs(15), socket.next())
            .await
            .expect("server answers within 15 s");
        match frame {
            None | Some(Ok(Message::Close(_)) | Err(_)) => return None,
            Some(Ok(Message::Text(text))) => {
                return Some(Got::M(
                    decode(text.as_str()).expect("server speaks the protocol"),
                ));
            }
            Some(Ok(Message::Binary(frame))) => {
                return Some(Got::Chunk(decode_chunk(&frame).expect("a chunk frame")));
            }
            Some(Ok(_)) => {}
        }
    }
}

/// Waits for the next published day boundary.
async fn next_day(socket: &mut Socket) -> u64 {
    loop {
        if let Some(Got::M(ServerMessage::DayStarted { day, .. })) = receive(socket).await {
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
            held: None,
        },
    )
    .await;
    let welcome = receive(&mut socket).await;
    assert!(
        matches!(
            welcome,
            Some(Got::M(ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                body: Some(1),
                ..
            }))
        ),
        "got {welcome:?}"
    );

    // The shape of the world follows the welcome, then the chunks around
    // body 1, each with the bodies standing in it.
    match receive(&mut socket).await {
        Some(Got::M(ServerMessage::WorldState {
            width,
            height,
            chunk_size,
            materials,
            species,
            flora,
            clock,
            ..
        })) => {
            assert!(flora.len() >= 29, "the shrubs, plants and mushrooms");
            assert!(
                flora
                    .iter()
                    .all(|k| ["shrub", "plant", "mushroom"].contains(&k.family.as_str()))
            );
            assert_eq!((width, height), (VALLEY, VALLEY));
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
    let mut far = None;
    // The chunks are built beside the network: a day boundary may slip in
    // between them on a loaded machine (a day lasts a second here).
    // The bodies follow the frame of the chunk they stand in.
    while chunks.len() < window_len(world, centre) || seen_bodies.len() < 3 {
        match receive_any(&mut socket).await {
            Some(Got::M(ServerMessage::DayStarted { .. } | ServerMessage::View { .. })) => {}
            Some(message)
                if ambient(&message)
                    && !matches!(
                        message,
                        Got::M(ServerMessage::FarView { .. } | ServerMessage::ChunkBodies { .. })
                    ) => {}
            Some(Got::M(ServerMessage::FarView {
                cell,
                width,
                height,
                cells,
                ..
            })) => far = Some((cell, width, height, cells)),
            Some(Got::M(ServerMessage::ChunkBodies { x, y, bodies })) => {
                assert_eq!(chunks.last(), Some(&(x, y)), "right after their chunk");
                for b in &bodies {
                    assert_eq!(tile(b.at).chunk(), ChunkPos::new(x, y));
                    assert_eq!(b.route, Vec::new(), "nobody walks at the start");
                    // Not whether it sleeps: a game day passes every second
                    // here, and on a loaded machine a body may tire and fall
                    // asleep before the chunks arrive (sleep has its tests).
                }
                seen_bodies.extend(bodies.iter().map(|b| b.id));
            }
            Some(Got::Chunk(frame)) => {
                let items = frame.items(&[]);
                let ChunkFrame {
                    x,
                    y,
                    tiles,
                    trees,
                    plants,
                    currents,
                    ..
                } = frame;
                let side = i32::try_from(CHUNK_SIZE).unwrap() * sim::MM_PER_TILE;
                for p in plants.iter().chain(&trees) {
                    assert!(
                        i32::from(p.dx) < side && i32::from(p.dy) < side,
                        "a tree or a plant stands in its chunk"
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
                seen_trees += trees.len();
                for i in &items {
                    assert_eq!(Pos::new(i.x, i.y).chunk(), ChunkPos::new(x, y));
                }
                seen_items += items.len();
                assert!((x - centre.x).abs() <= VIEW_RADIUS && (y - centre.y).abs() <= VIEW_RADIUS);
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
    // And the far view around the body's chunk, which leaves as soon as
    // it is made: before the chunks, built beside the network.
    let (cell, width, height, cells) = far.expect("the far view with the chunks");
    assert_eq!(cells.len(), (width * height) as usize);
    assert!(cell >= 8 && width >= 128);
    assert!(cells.contains('f') && cells.contains('g'));
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
            Some(Got::M(ServerMessage::DayStarted { .. })) => {}
            other => break other,
        }
    };
    assert_eq!(pong, Some(Got::M(ServerMessage::Pong { nonce: 7 })));

    // One game day lasts one real second: a day boundary must be published.
    let (day, tick) = match receive(&mut socket).await {
        Some(Got::M(ServerMessage::DayStarted { day, tick })) => (day, tick),
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
            held: None,
        },
    )
    .await;
    assert_eq!(
        receive(&mut socket).await,
        Some(Got::M(ServerMessage::ProtocolMismatch {
            expected: PROTOCOL_VERSION
        }))
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
                held: None,
            },
        )
        .await;
        assert!(matches!(
            receive(socket).await,
            Some(Got::M(ServerMessage::Welcome { .. }))
        ));
    }

    let day_a = next_day(&mut a).await;
    let day_b = next_day(&mut b).await;
    assert_eq!(day_a, day_b);
}

/// Opens a piloting connection: hello, welcome, `world_state` and the
/// initial chunks consumed.
/// Connects and says hello: the socket and the body it pilots, if any.
async fn welcome(server: &Server) -> (Socket, Option<u32>) {
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            held: None,
        },
    )
    .await;
    let body = match receive(&mut socket).await {
        Some(Got::M(ServerMessage::Welcome { body, .. })) => body,
        other => panic!("expected welcome, got {other:?}"),
    };
    (socket, body)
}

/// The chunk a fresh connection's view centres on: its body's, or the
/// middle of the map for a watcher.
fn home_of(world: &World, body: Option<u32>) -> ChunkPos {
    if let Some(body) = body {
        start_of(world, body).chunk()
    } else {
        let middle = i32::try_from(VALLEY / 2).expect("small valley");
        Pos::new(middle, middle).chunk()
    }
}

/// Connects, says hello and receives the view around the body: the
/// socket and the body it pilots, if any.
async fn join(server: &Server) -> (Socket, Option<u32>) {
    let (socket, body, _) = join_counting(server).await;
    (socket, body)
}

/// Like [`join`], returning the chunks received. The chunks are built
/// beside the network and leave after any event of the moment (a body
/// walking, the dev overview): what comes between them is let by.
async fn join_counting(server: &Server) -> (Socket, Option<u32>, HashSet<(i32, i32)>) {
    let (mut socket, body) = welcome(server).await;
    assert!(matches!(
        receive(&mut socket).await,
        Some(Got::M(ServerMessage::WorldState { .. }))
    ));
    let world = standard_world();
    let centre = home_of(world, body);
    let mut chunks = HashSet::new();
    while chunks.len() < window_len(world, centre) {
        match receive(&mut socket).await {
            Some(Got::Chunk(ChunkFrame { x, y, .. })) => {
                assert!(chunks.insert((x, y)), "chunk ({x}, {y}) sent twice");
            }
            Some(_) => {}
            None => panic!("disconnected while the view was coming"),
        }
    }
    (socket, body, chunks)
}

// --- The view follows the body (D08, D31) --------------------------------

#[tokio::test]
async fn the_view_follows_the_piloted_body() {
    let server = Server::start_with(&["--seconds-per-day", "100", "--time-scale", "20"]);
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Walk into the chunk east of the starting one: the window moves a
    // column east (one `view`) and a column is shown to the east.
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

    let (mut shown, mut window) = (Vec::new(), None);
    while shown.len() < 5 || window.is_none() {
        match receive(&mut pilot).await {
            Some(Got::Chunk(ChunkFrame { x, y, .. })) => shown.push((x, y)),
            Some(Got::M(ServerMessage::View { x, width, .. })) => window = Some((x, width)),
            Some(Got::M(
                ServerMessage::BodyWalks { .. }
                | ServerMessage::BodyStopped { .. }
                | ServerMessage::DayStarted { .. },
            )) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(
        shown.iter().all(|(x, _)| *x == here.x + VIEW_RADIUS + 1),
        "{shown:?}"
    );
    let (x, width) = window.expect("a view");
    assert_eq!(x, (here.x - VIEW_RADIUS + 1).max(0), "{window:?}");
    assert_eq!(
        x + i32::try_from(width).expect("small"),
        here.x + VIEW_RADIUS + 2,
        "{window:?}"
    );
}

#[tokio::test]
async fn a_body_walking_out_of_the_view_is_reported_as_left() {
    // Long days: the walk, straight to open ground beyond a path's reach,
    // takes minutes of game time and must not end in a collapse.
    let server = Server::start_with(&["--seconds-per-day", "20000", "--time-scale", "50"]);
    let (mut walker, body) = join(&server).await;
    let (mut watcher, other) = join(&server).await;
    let body = body.expect("pilots body 1");
    let other = other.expect("pilots body 2");

    // Body 1 walks far east, out of body 2's window, in legs no longer
    // than what it sees from the terrain it stands on (a path goes no
    // farther, D32); body 2 stays.
    let world = standard_world();
    let start = start_of(world, body);
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk");
    let east = (start_of(world, other).x.div_euclid(side) + VIEW_RADIUS + 1) * side;
    let mut stops = legs_east(world, &rules(), start, east);
    let far = stops.pop().expect("a last leg out of the view");
    for stop in &stops {
        send(
            &mut walker,
            &ClientMessage::MoveTo {
                x: stop.x,
                y: stop.y,
            },
        )
        .await;
        loop {
            match receive(&mut walker).await {
                Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                    assert_eq!(tile(at), *stop);
                    break;
                }
                Some(_) => {}
                None => panic!("walker disconnected"),
            }
        }
    }
    send(&mut walker, &ClientMessage::MoveTo { x: far.x, y: far.y }).await;

    // The watcher sees the walk start (inside its view), then the body
    // leave; never where it stops, which is outside.
    let mut saw_walk = false;
    loop {
        match receive(&mut watcher).await {
            Some(Got::M(ServerMessage::BodyLeft { body: who })) => {
                assert_eq!(who, body);
                break;
            }
            Some(Got::M(ServerMessage::BodyWalks {
                body: who,
                ref route,
                ..
            })) if who == body => {
                eprintln!("watcher sees walk {route:?}");
                saw_walk = true;
            }
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) => {
                assert!(
                    who != body || stops.contains(&tile(at)),
                    "a stop outside the watcher's view leaked"
                );
            }
            Some(Got::M(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. })) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(saw_walk);
    // The walker itself keeps receiving ground and finally its own stop.
    loop {
        match receive(&mut walker).await {
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                assert_eq!(tile(at), far);
                break;
            }
            Some(Got::M(ServerMessage::BodyLeft { body: who })) => {
                assert_ne!(who, body, "never leaves itself");
            }
            Some(_) => {}
            None => panic!("walker disconnected"),
        }
    }
}

// --- Chunks built beside the network (D29, 11 November 2026) ------------

/// The chunks of the window around `centre` in the standard world.
fn window(world: &World, centre: ChunkPos) -> HashSet<(i32, i32)> {
    let mut chunks = HashSet::new();
    for dy in -VIEW_RADIUS..=VIEW_RADIUS {
        for dx in -VIEW_RADIUS..=VIEW_RADIUS {
            let chunk = ChunkPos::new(centre.x + dx, centre.y + dy);
            if world.map().has_chunk(chunk) {
                chunks.insert((chunk.x, chunk.y));
            }
        }
    }
    chunks
}

#[tokio::test]
async fn clients_logging_in_at_once_each_get_a_complete_view() {
    // Three bodies to pilot, a fourth client watches the middle of the
    // map: four cold views built at the same time on two threads, each
    // whole, each chunk once.
    let server = Server::start_with(&["--seconds-per-day", "1000", "--chunk-threads", "2"]);
    let world = standard_world();
    let (a, b, c, d) = tokio::join!(
        join_counting(&server),
        join_counting(&server),
        join_counting(&server),
        join_counting(&server)
    );
    for (_, body, chunks) in [&a, &b, &c, &d] {
        let centre = home_of(world, *body);
        assert_eq!(*chunks, window(world, centre), "{body:?}'s view");
    }
    let bodies: HashSet<Option<u32>> = [a.1, b.1, c.1, d.1].into_iter().collect();
    assert_eq!(bodies.len(), 4, "three different bodies and a watcher");
    assert!(bodies.contains(&None), "the fourth client watches");
}

#[tokio::test]
async fn an_intention_is_read_and_answered_while_the_view_is_being_sent() {
    // Long days: no day boundary interferes. The client asks to walk as
    // soon as it is welcomed, before any chunk arrived: the walk is told
    // before the view is through (an event goes before the ground), where
    // it used to wait for the last of the 361 chunks. One chunk thread:
    // eight built the whole view (about 80 ms in debug) before the first
    // tick came at times, and the walk had nothing to go before.
    let server = Server::start_with(&["--seconds-per-day", "1000", "--chunk-threads", "1"]);
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            held: None,
        },
    )
    .await;
    let body = match receive(&mut socket).await {
        Some(Got::M(ServerMessage::Welcome { body, .. })) => body.expect("pilots a body"),
        other => panic!("expected welcome, got {other:?}"),
    };
    let world = standard_world();
    let start = start_of(world, body);
    let target = Pos::new(start.x + 3, start.y);
    route(world, start, target); // reachable
    send(
        &mut socket,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;
    let total = window_len(world, start.chunk());
    let (mut chunks, mut walked_after) = (0, None);
    while chunks < total {
        match receive(&mut socket).await {
            Some(Got::Chunk(_)) => chunks += 1,
            Some(Got::M(ServerMessage::BodyWalks { body: who, .. })) if who == body => {
                walked_after.get_or_insert(chunks);
            }
            Some(Got::M(
                ServerMessage::WorldState { .. }
                | ServerMessage::View { .. }
                | ServerMessage::BodyWalks { .. }
                | ServerMessage::BodyStopped { .. },
            )) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    let walked_after = walked_after.expect("the walk is told during the view");
    assert!(
        walked_after < total,
        "the walk was told after {walked_after} of {total} chunks"
    );
}

/// Teleports `hopping` between the two `far` tiles, each hop waited for,
/// as long as `hops` says: how many hops were made.
fn hop_between(
    mut hopper: Socket,
    hopping: u32,
    far: [Pos; 2],
    hops: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<usize> {
    tokio::spawn(async move {
        let mut n = 0;
        while hops.load(Ordering::Relaxed) {
            let to = far[n % 2];
            send(
                &mut hopper,
                &ClientMessage::DevTeleport { x: to.x, y: to.y },
            )
            .await;
            n += 1;
            // Reading what comes, or the server's writer would stall.
            let stopped = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match receive(&mut hopper).await {
                        Some(Got::M(ServerMessage::BodyStopped { body: who, .. }))
                            if who == hopping =>
                        {
                            break;
                        }
                        Some(_) => {}
                        None => panic!("hopper disconnected"),
                    }
                }
            })
            .await;
            assert!(stopped.is_ok(), "a teleport is answered within 10 s");
        }
        n
    })
}

#[tokio::test]
async fn a_walk_is_seen_whole_while_another_client_builds_cold_views() {
    // A walks east across a chunk border, watched by C, while B teleports
    // far away again and again: each teleport a cold view of 361 chunks.
    // Nothing of the walk is lost or held back: C sees it start and stop,
    // A gets the ground it walks into and its own stop.
    let server = Server::start_with(&[
        "--dev",
        "--seconds-per-day",
        "1000",
        "--time-scale",
        "20",
        "--chunk-threads",
        "2",
    ]);
    let (mut walker, body) = join(&server).await;
    let (mut watcher, _) = join(&server).await;
    let (hopper, hopping) = join(&server).await;
    let body = body.expect("pilots body 1");
    let hopping = hopping.expect("pilots body 3");

    let world = standard_world();
    let start = start_of(world, body);
    let here = start.chunk();
    let target = Pos::new(
        here.origin().x + i32::try_from(CHUNK_SIZE).expect("small chunk"),
        start.y,
    );
    route(world, start, target); // reachable
    send(
        &mut walker,
        &ClientMessage::MoveTo {
            x: target.x,
            y: target.y,
        },
    )
    .await;
    // B hops between two far corners for the whole walk.
    let hops = Arc::new(AtomicBool::new(true));
    let far = [
        world
            .map()
            .nearest(Pos::new(100, 600), sim::Terrain::Grass)
            .expect("grass"),
        world
            .map()
            .nearest(Pos::new(600, 100), sim::Terrain::Grass)
            .expect("grass"),
    ];
    let hopping_task = hop_between(hopper, hopping, far, Arc::clone(&hops));

    let (mut shown, mut moved, mut stopped) = (0, false, false);
    while !(stopped && shown >= 5 && moved) {
        match receive(&mut walker).await {
            Some(Got::Chunk(ChunkFrame { x, .. })) => {
                assert_eq!(x, here.x + VIEW_RADIUS + 1);
                shown += 1;
            }
            Some(Got::M(ServerMessage::View { x, .. })) => {
                assert_eq!(x, (here.x - VIEW_RADIUS + 1).max(0));
                moved = true;
            }
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                assert_eq!(tile(at), target);
                stopped = true;
            }
            // B leaves the view with its first hop.
            Some(Got::M(
                ServerMessage::BodyWalks { .. }
                | ServerMessage::BodyStopped { .. }
                | ServerMessage::BodyLeft { .. },
            )) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    let (mut saw_walk, mut saw_stop) = (false, false);
    while !(saw_walk && saw_stop) {
        match receive(&mut watcher).await {
            Some(Got::M(ServerMessage::BodyWalks { body: who, .. })) if who == body => {
                saw_walk = true;
            }
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                assert_eq!(tile(at), target);
                saw_stop = true;
            }
            Some(Got::M(
                ServerMessage::BodyWalks { .. }
                | ServerMessage::BodyStopped { .. }
                | ServerMessage::BodyLeft { .. },
            )) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    hops.store(false, Ordering::Relaxed);
    let hops = hopping_task.await.expect("the hopper's task");
    assert!(hops >= 1, "B hopped {hops} time(s) meanwhile");
}

/// The chunks of a whole view, unclipped: a login at the camp or a
/// watcher at the middle of the game's valley gets them all.
fn whole_view() -> usize {
    usize::try_from((2 * VIEW_RADIUS + 1).pow(2)).expect("small")
}

/// A cold login on the game's valley: how long after hello the first
/// chunk came, and the last of the whole view.
async fn cold_login(server: &Server) -> (Duration, Duration) {
    let started = Instant::now();
    let (mut socket, _) = welcome(server).await;
    let (mut chunks, mut first) = (0, None);
    while chunks < whole_view() {
        if let Some(Got::Chunk(_)) = receive(&mut socket).await {
            chunks += 1;
            first.get_or_insert_with(|| started.elapsed());
        }
    }
    (first.expect("a chunk"), started.elapsed())
}

/// What a client waits for its view on the game's valley, built beside
/// the network: a measure, not a rule. Run by hand, in release:
/// `cargo test --release -p server --test net measure_cold_logins -- --ignored --nocapture`.
///
/// One cold login alone (hello to the last of the 361 chunks, and to the
/// first), then four at once on a fresh server: three pilots at the
/// camp and a watcher at the middle of the map, a cold view each.
#[tokio::test]
#[ignore = "a measure on the game's 3072 valley, run by hand in release"]
async fn measure_cold_logins() {
    const GAME_VALLEY: &[&str] = &[
        "--valley-size",
        "3072",
        "--no-warm-up",
        "--seconds-per-day",
        "1000",
    ];
    let server = Server::start_with(GAME_VALLEY);
    let (first, last) = cold_login(&server).await;
    println!("one cold login: first chunk after {first:?}, the view whole after {last:?}");
    drop(server);

    let server = Server::start_with(GAME_VALLEY);
    let started = Instant::now();
    let four = tokio::join!(
        cold_login(&server),
        cold_login(&server),
        cold_login(&server),
        cold_login(&server)
    );
    let all = started.elapsed();
    for (i, (first, last)) in [four.0, four.1, four.2, four.3].into_iter().enumerate() {
        println!(
            "four cold logins at once, client {i}: first chunk after {first:?}, the view whole after {last:?}"
        );
    }
    println!("four cold logins at once: all four views whole after {all:?}");
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
            held: None,
        },
    )
    .await;
    let Some(Got::M(ServerMessage::Welcome { body, .. })) = receive(&mut socket).await else {
        panic!("expected welcome");
    };
    let body = body.expect("pilots a body");
    assert!(matches!(
        receive(&mut socket).await,
        Some(Got::M(ServerMessage::WorldState { .. }))
    ));
    // The overview: the whole map, one letter a cell.
    let Some(Got::M(ServerMessage::DevOverview {
        cell,
        width,
        height,
        cells,
    })) = receive(&mut socket).await
    else {
        panic!("expected the overview");
    };
    assert_eq!(width, VALLEY.div_ceil(cell));
    assert_eq!(height, VALLEY.div_ceil(cell));
    assert_eq!(cells.len(), (width * height) as usize);
    assert!(cells.chars().all(|c| "gfwobr".contains(c)));
    assert!(cells.contains('w') && cells.contains('f') && cells.starts_with('r'));
    let world = standard_world();
    assert!(matches!(
        receive(&mut socket).await,
        Some(Got::M(ServerMessage::View { .. }))
    ));
    for _ in 0..window_len(world, start_of(world, body).chunk()) {
        assert!(matches!(receive(&mut socket).await, Some(Got::Chunk(_))));
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
    // The stop is an event: it leaves before the new ground, which is
    // built beside the network and follows.
    let (mut shown, mut moved, mut stopped) = (0, false, false);
    while !(stopped && shown > 0 && moved) {
        match receive(&mut socket).await {
            Some(Got::Chunk(_)) => shown += 1,
            Some(Got::M(ServerMessage::View { .. })) => moved = true,
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                assert_eq!(tile(at), far);
                stopped = true;
            }
            Some(_) => {}
            None => panic!("disconnected"),
        }
    }
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
            Some(Got::M(ServerMessage::Pong { nonce: 6 })) => break,
            Some(Got::M(ServerMessage::Pong { .. } | ServerMessage::DayStarted { .. })) => {}
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
                Some(Got::M(ServerMessage::BodyWalks {
                    body: who,
                    at,
                    route,
                    speed_cm_per_s,
                    ..
                })) if who == body => {
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
                Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                    stopped = Some(at);
                }
                Some(Got::M(
                    ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. },
                )) => {}
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
async fn walking_toward_the_cliff_goes_straight_and_stops_before_the_first_obstacle() {
    let server = Server::start();
    let (mut pilot, body) = join(&server).await;
    let body = body.expect("pilots a body");

    // Row 0 is the cliff: not walkable, and no path to it. The body sets
    // off straight toward it, to stop before what is first in its way.
    let world = standard_world();
    let rules = rules();
    let from = start_of(world, body).centre();
    let walkable = |p: Pos| {
        world
            .map()
            .get(p)
            .is_some_and(|t| rules.movement.cost(t).is_some())
    };
    let end = sim::map::line_reach(from, Pos::new(100, 0).centre(), walkable);
    send(&mut pilot, &ClientMessage::MoveTo { x: 100, y: 0 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(Got::M(ServerMessage::BodyWalks {
                body: who, route, ..
            })) if who == body => {
                assert_eq!(route, vec![Point { x: end.x, y: end.y }]);
                break;
            }
            Some(Got::M(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. })) => {}
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
        Some(Got::M(
            ServerMessage::DayStarted { .. } | ServerMessage::NightFell { .. }
        ))
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
            Some(Got::M(ServerMessage::DayStarted { .. })) => {}
            other => {
                assert_eq!(other, Some(Got::M(ServerMessage::Pong { nonce: 3 })));
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

    // Eating an item that does not exist: refused, to the pilot alone.
    send(&mut pilot, &ClientMessage::Eat { item: 999_999 }).await;
    loop {
        match receive(&mut pilot).await {
            Some(Got::M(ServerMessage::ActRefused { body: who, .. })) => {
                assert_eq!(who, body);
                break;
            }
            Some(Got::M(ServerMessage::DayStarted { .. })) => {}
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
            Some(Got::M(ServerMessage::BodyWalks { body: who, .. })) if who == other_body => break,
            Some(Got::M(ServerMessage::ActRefused { .. })) => {
                panic!("another pilot's refusal leaked")
            }
            Some(Got::M(ServerMessage::DayStarted { .. } | ServerMessage::BodyWalks { .. })) => {}
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
            Some(Got::M(ServerMessage::BodyStopped { body: who, at })) if who == body => {
                arrived = tile(at) == north;
            }
            Some(Got::M(ServerMessage::Pong { nonce: 9 })) => answered = true,
            Some(Got::M(ServerMessage::MoveRefused { .. })) => refusals += 1,
            // The flood's eastward asks may carry the body across a chunk.
            Some(
                Got::M(
                    ServerMessage::DayStarted { .. }
                    | ServerMessage::BodyWalks { .. }
                    | ServerMessage::View { .. }
                    | ServerMessage::BodyLeft { .. },
                )
                | Got::Chunk(_),
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
            let got = receive_any(socket).await.expect("still connected");
            if let Got::M(message) = got
                && let Some(found) = pick(&message)
            {
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
    // The view is not waited for: its chunks leave after the conditions
    // of the moment, and a loaded machine would let the first ones by.
    let server = Server::start();
    let (mut pilot, body) = welcome(&server).await;
    let (mut other, other_body) = welcome(&server).await;
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
    let world = standard_world();
    let here = start_of(world, body);
    let far = ground_items(world)
        .into_iter()
        .max_by_key(|i| {
            let p = i.pos().expect("on the ground");
            (p.x - here.x).abs() + (p.y - here.y).abs()
        })
        .expect("items in the valley")
        .id()
        .0;
    send(&mut pilot, &ClientMessage::Eat { item: far }).await;
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
    assert_eq!(refused, (far, protocol::Refusal::OutOfReach));

    // Walk to the nearest berries, then eat them: the watcher sees them go.
    let world = standard_world();
    let rules = rules();
    let start = start_of(world, body);
    let around = reachable(world, start);
    let berries = ground_items(world)
        .into_iter()
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
    // The death may come while the view is still arriving (it leaves
    // before the ground): the view is not waited for.
    let (mut pilot, body) = welcome(&server).await;
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
            // The new body's own chunk, the bodies in it after it.
            ServerMessage::ChunkBodies { bodies, .. }
                if bodies.iter().any(|b| b.id == new_body) =>
            {
                Some(true)
            }
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
    let nearest = ground_items(world)
        .into_iter()
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

// --- Never resend what the client holds (D4) -----------------------------

/// What a fake client holds: each chunk in view, with its frame's
/// version, and the chunks it was sent, by kind, since the last count.
#[derive(Default)]
struct Holder {
    in_view: HashMap<(i32, i32), u32>,
    frames: Vec<(i32, i32)>,
    sames: Vec<(i32, i32)>,
    world: String,
}

impl Holder {
    /// Takes in what concerns the chunks it holds; true when `got` was
    /// one of those messages.
    fn take(&mut self, got: &Got) -> bool {
        match got {
            Got::Chunk(frame) => {
                self.in_view.insert((frame.x, frame.y), frame.version);
                self.frames.push((frame.x, frame.y));
            }
            Got::M(ServerMessage::ChunkSame { x, y, version }) => {
                self.in_view.insert((*x, *y), *version);
                self.sames.push((*x, *y));
            }
            Got::M(ServerMessage::View {
                x,
                y,
                width,
                height,
            }) => {
                let (w, h) = (
                    i32::try_from(*width).expect("small"),
                    i32::try_from(*height).expect("small"),
                );
                self.in_view
                    .retain(|(cx, cy), _| (*x..*x + w).contains(cx) && (*y..*y + h).contains(cy));
            }
            Got::M(ServerMessage::WorldState { world, .. }) => self.world.clone_from(world),
            Got::M(_) => return false,
        }
        true
    }

    /// Reads until the window around `centre` is held whole: what it
    /// was sent meanwhile, frames and `chunk_same`, taken out.
    async fn fill(
        &mut self,
        socket: &mut Socket,
        centre: ChunkPos,
    ) -> (Vec<(i32, i32)>, Vec<(i32, i32)>) {
        let world = standard_world();
        let wanted = window(world, centre);
        tokio::time::timeout(Duration::from_secs(40), async {
            while !wanted.iter().all(|c| self.in_view.contains_key(c)) {
                let got = receive_any(socket).await.expect("still connected");
                self.take(&got);
            }
        })
        .await
        .expect("the view comes within 40 s");
        (
            std::mem::take(&mut self.frames),
            std::mem::take(&mut self.sames),
        )
    }
}

/// Teleports `body` to `to` and back to `from`, each view held whole:
/// what the return sent, frames and `chunk_same`.
async fn away_and_back(
    socket: &mut Socket,
    holder: &mut Holder,
    body: u32,
    from: Pos,
    to: Pos,
) -> (Vec<(i32, i32)>, Vec<(i32, i32)>) {
    for (at, last) in [(to, false), (from, true)] {
        send(socket, &ClientMessage::DevTeleport { x: at.x, y: at.y }).await;
        // The new window, then its ground: what came before it belongs
        // to the old view.
        tokio::time::timeout(Duration::from_secs(20), async {
            let (mut moved, mut stopped) = (false, false);
            while !(moved && stopped) {
                let got = receive_any(socket).await.expect("still connected");
                match got {
                    Got::M(ServerMessage::BodyStopped {
                        body: who,
                        at: here,
                    }) if who == body => {
                        assert_eq!(tile(here), at);
                        stopped = true;
                    }
                    Got::M(ServerMessage::View {
                        x,
                        y,
                        width,
                        height,
                    }) if (x..x + i32::try_from(width).unwrap()).contains(&at.chunk().x)
                        && (y..y + i32::try_from(height).unwrap()).contains(&at.chunk().y) =>
                    {
                        holder.take(&got);
                        holder.frames.clear();
                        holder.sames.clear();
                        moved = true;
                    }
                    got => {
                        holder.take(&got);
                    }
                }
            }
        })
        .await
        .expect("a teleport is answered");
        let sent = holder.fill(socket, at.chunk()).await;
        if last {
            return sent;
        }
    }
    unreachable!()
}

/// Walks `body` onto `at`, the view along it held.
async fn walk_onto(socket: &mut Socket, holder: &mut Holder, body: u32, at: Pos) {
    send(socket, &ClientMessage::MoveTo { x: at.x, y: at.y }).await;
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let got = receive_any(socket).await.expect("still connected");
            if let Got::M(ServerMessage::BodyStopped {
                body: who,
                at: here,
            }) = got
                && who == body
                && tile(here) == at
            {
                break;
            }
            holder.take(&got);
        }
    })
    .await
    .expect("the walk ends");
    holder.fill(socket, at.chunk()).await;
}

/// Waits for `item`'s event, the chunks' messages meanwhile taken in.
async fn wait_item(socket: &mut Socket, holder: &mut Holder, item: u64, added: bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let got = receive_any(socket).await.expect("still connected");
            match &got {
                Got::M(ServerMessage::ItemAdded { item: i }) if added && i.id == item => break,
                Got::M(ServerMessage::ItemRemoved { item: i, .. }) if !added && *i == item => break,
                _ => {
                    holder.take(&got);
                }
            }
        }
    })
    .await
    .expect("the item's event");
}

#[tokio::test]
async fn a_return_sends_again_only_the_chunk_where_an_item_was_taken_or_dropped() {
    let server = Server::start_with(&["--dev", "--seconds-per-day", "1000", "--time-scale", "20"]);
    let (mut socket, body) = welcome(&server).await;
    let body = body.expect("pilots a body");
    let mut holder = Holder::default();
    let world = standard_world();
    let start = start_of(world, body);
    holder.fill(&mut socket, start.chunk()).await;

    // Onto the nearest item, the view held whole there.
    let item = ground_items(world)
        .into_iter()
        .min_by_key(|i| {
            let p = i.pos().expect("on the ground");
            (p.x - start.x).abs() + (p.y - start.y).abs()
        })
        .expect("items in the valley");
    let at = item.pos().expect("on the ground");
    walk_onto(&mut socket, &mut holder, body, at).await;
    let half = i32::try_from(VALLEY / 2).expect("small");
    let far = world
        .map()
        .nearest(
            Pos::new(if at.x < half { 2 * half - 20 } else { 20 }, at.y),
            sim::Terrain::Grass,
        )
        .expect("grass far away");
    assert!(
        (far.chunk().x - at.chunk().x).abs() > VIEW_RADIUS,
        "the item's chunk leaves the view"
    );

    // Nothing changed: the return costs no frame.
    let (frames, sames) = away_and_back(&mut socket, &mut holder, body, at, far).await;
    assert!(frames.is_empty(), "{frames:?}");
    assert!(sames.contains(&(at.chunk().x, at.chunk().y)));

    // An item taken: its chunk alone is sent again.
    send(&mut socket, &ClientMessage::PickUp { item: item.id().0 }).await;
    wait_item(&mut socket, &mut holder, item.id().0, false).await;
    let (frames, sames) = away_and_back(&mut socket, &mut holder, body, at, far).await;
    assert_eq!(
        frames,
        [(at.chunk().x, at.chunk().y)],
        "the chunk changed, once"
    );
    assert!(sames.len() > 1, "the others are the same");

    // An item dropped: the same.
    send(&mut socket, &ClientMessage::Drop { item: item.id().0 }).await;
    wait_item(&mut socket, &mut holder, item.id().0, true).await;
    let (frames, _) = away_and_back(&mut socket, &mut holder, body, at, far).await;
    assert_eq!(
        frames,
        [(at.chunk().x, at.chunk().y)],
        "the chunk changed, once"
    );
}

#[tokio::test]
async fn a_chunk_the_client_lacks_after_all_is_sent_when_it_says_so() {
    let server = Server::start_with(&["--seconds-per-day", "1000"]);
    let (mut first, body) = welcome(&server).await;
    let mut holder = Holder::default();
    let home = home_of(standard_world(), body);
    holder.fill(&mut first, home).await;
    // A client that says it holds the chunk of its body, and does not.
    let mut socket = server.connect().await;
    send(
        &mut socket,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            held: Some(protocol::HeldChunks {
                world: holder.world.clone(),
                chunks: vec![(home.x, home.y, holder.in_view[&(home.x, home.y)])],
            }),
        },
    )
    .await;
    let mut lacking = Holder::default();
    let mut answered = false;
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let got = receive_any(&mut socket).await.expect("still connected");
            match got {
                Got::M(ServerMessage::ChunkSame { x, y, .. }) => {
                    assert_eq!(ChunkPos::new(x, y), home, "the one it said it holds");
                    assert!(!answered, "once");
                    send(&mut socket, &ClientMessage::ChunkMissing { x, y }).await;
                    answered = true;
                }
                Got::Chunk(frame) if answered && ChunkPos::new(frame.x, frame.y) == home => break,
                got => {
                    lacking.take(&got);
                }
            }
        }
    })
    .await
    .expect("the frame follows the answer");
}

#[tokio::test]
async fn a_reconnection_to_the_same_world_sends_no_frame_for_the_chunks_held() {
    // A server, then another on the same world (as after a restart).
    let (held, world_id) = {
        let server = Server::start_with(&["--seconds-per-day", "1000"]);
        let (mut socket, body) = welcome(&server).await;
        let mut holder = Holder::default();
        holder
            .fill(&mut socket, home_of(standard_world(), body))
            .await;
        let held: Vec<(i32, i32, u32)> = holder
            .in_view
            .iter()
            .map(|(&(x, y), &v)| (x, y, v))
            .collect();
        (held, holder.world)
    };
    assert_eq!(world_id.len(), 16, "{world_id}");
    let server = Server::start_with(&["--seconds-per-day", "1000"]);
    for (world, sends_frames) in [
        (world_id.clone(), false),
        ("another world".to_owned(), true),
    ] {
        let mut socket = server.connect().await;
        send(
            &mut socket,
            &ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                held: Some(protocol::HeldChunks {
                    world,
                    chunks: held.clone(),
                }),
            },
        )
        .await;
        let body = match receive(&mut socket).await {
            Some(Got::M(ServerMessage::Welcome { body, .. })) => body,
            other => panic!("expected welcome, got {other:?}"),
        };
        let mut holder = Holder::default();
        let (frames, sames) = holder
            .fill(&mut socket, home_of(standard_world(), body))
            .await;
        assert_eq!(holder.world, world_id, "the same world after a restart");
        if sends_frames {
            assert!(sames.is_empty(), "another world's chunks are never reused");
            assert_eq!(
                frames.len(),
                window_len(standard_world(), home_of(standard_world(), body))
            );
        } else {
            assert!(frames.is_empty(), "{} frames sent again", frames.len());
            assert_eq!(sames.len(), held.len());
        }
        drop(socket);
    }
}
