//! Pure world simulation for Soul Invasion.
//!
//! This crate has no I/O, no clock and no network (architecture decision D24).
//! The server feeds it intentions and asks it to advance a number of fixed
//! ticks; it returns the events that happened. Same inputs, same outputs.

/// Number of simulation ticks per second of in-game time.
///
/// Time acceleration (D09) only changes how fast the server chains ticks;
/// it never changes the rules.
pub const TICKS_PER_GAME_SECOND: u32 = 10;

/// A simulation tick counter since the world was created.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Tick(pub u64);

/// The world state. Milestone 1 ("the skeleton") only tracks time.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct World {
    tick: Tick,
}

impl World {
    /// Creates an empty world at tick zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current tick.
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// Advances the world by exactly one fixed tick.
    pub fn step(&mut self) {
        self.tick.0 += 1;
    }

    /// Advances the world by `n` fixed ticks.
    pub fn advance(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_world_starts_at_tick_zero() {
        assert_eq!(World::new().tick(), Tick(0));
    }

    #[test]
    fn advancing_is_deterministic() {
        let mut a = World::new();
        let mut b = World::new();
        a.advance(1_000);
        b.advance(1_000);
        assert_eq!(a.tick(), b.tick());
        assert_eq!(a.tick(), Tick(1_000));
    }
}
