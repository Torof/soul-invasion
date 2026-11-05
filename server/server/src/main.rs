//! Soul Invasion game server.
//!
//! Milestone 1 ("the skeleton") will add the WebSocket listener, the
//! single-threaded simulation loop (D29) and the storage layer (D28).
//! For now it only proves that the workspace builds and links.

fn main() {
    let mut world = sim::World::new();
    world.advance(u64::from(sim::TICKS_PER_GAME_SECOND));
    println!(
        "soul-invasion server (protocol v{}) — world at tick {}",
        protocol::PROTOCOL_VERSION,
        world.tick().0
    );
}
