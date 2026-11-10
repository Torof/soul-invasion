//! What the server's bookkeeping costs at scale (D39): measures, not
//! rules, ignored and run by hand in release:
//! `cargo test --release -p server --test scale -- --ignored --nocapture`.

use std::time::Instant;

use server::net::Shared;
use sim::{BodyId, Calendar, Content, Map, Pos, Terrain, World};

/// The bookkeeping of one step of the last body among many: what every
/// `body_moved` event costs the simulation thread.
#[test]
#[ignore = "a measure, run by hand in release"]
fn a_step_of_the_last_body_among_many() {
    // Bodies spread over the land, a row of 32 to a chunk: what a chunk
    // holds does not grow with the world's bodies, as a chunk's bodies
    // must not.
    for bodies in [1_000, 20_000] {
        let side = 1_024;
        let map = Map::filled(side, side, Terrain::Grass);
        let mut world = World::empty(Calendar::default(), map);
        let side = i32::try_from(side).expect("small");
        for n in 0..i32::try_from(bodies).expect("few bodies") {
            world.spawn_body(Pos::new(n % side, (n / side) * 32));
        }
        let shared = Shared::new(&world, &Content::sample(), 4, 1.0);
        let last = BodyId(bodies);
        let at = Pos::new(10, 0).centre();
        let steps = 20_000;
        let started = Instant::now();
        for _ in 0..steps {
            assert!(shared.body_moved(last, at, 0).is_some());
        }
        let per_step = started.elapsed() / steps;
        let started = Instant::now();
        for _ in 0..1_000 {
            assert!(
                shared
                    .bodies_in(at.tile().chunk())
                    .iter()
                    .any(|b| b.id == last.0)
            );
        }
        let per_chunk = started.elapsed() / 1_000;
        println!(
            "{bodies} bodies: a step of the last one {per_step:?}, the bodies of a chunk {per_chunk:?}"
        );
    }
}
