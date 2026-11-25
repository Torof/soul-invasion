//! The meetings between tribes (25 November 2026): measures only, asked by
//! the project lead, who doubts that the tribes never meet (at noon they
//! never came within 256 m on two seeds of three, and their cells visited
//! overlapped in one cell over three seeds). Noon alone may miss them: here
//! the world is read at every game hour.
//!
//! - **In sight**: at each hour, every pair of living bodies of different
//!   tribes, neither carried, within the sight of one of them (the sim's
//!   own, `World::sight_tiles_at`, D32: by the ground each stands on, in
//!   tiles on the longer axis, a tile being a metre). Counted: the hours
//!   with at least one pair, the pair-hours, the distinct pairs, the first
//!   hour, and the first [`SEEN_KEPT`] hours with a pair, each with its
//!   nearest pair and where both stood.
//! - **The nearest**: by season of the cycle, the nearest two bodies of
//!   different tribes over all its hours, with the hour and where.
//!
//! The cost: one sort of the living by x an hour, then a sweep that
//! compares a body only with those within the larger of the widest sight
//! and the nearest distance found so far on x (the tribes far apart, every
//! pair at worst: 1,770 comparisons an hour for sixty bodies), and one
//! look at the ground under each body for its sight: measured at 6 µs an
//! hour for sixty bodies, 0.06 s over 400 journées. It only reads the
//! world: a run gives the same world with it as without it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sim::content::Content;
use sim::map::Pos;
use sim::time::Season;
use sim::{BodyId, World};

use crate::Person;

/// The hours with a pair in sight whose nearest pair is kept, the first
/// ones of the run.
pub const SEEN_KEPT: usize = 12;

/// One hour with bodies of different tribes in sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// The day, from 1.
    pub day: u64,
    /// The game hour of the day, 0 to 23 (0 at dawn).
    pub hour: u64,
    /// The nearest pair in sight that hour: the lower id first.
    pub a: (BodyId, Pos),
    /// See `a`.
    pub b: (BodyId, Pos),
    /// Tiles between them, on the longer axis.
    pub tiles: u32,
    /// Pairs in sight that hour.
    pub pairs: u32,
}

/// One season's nearest bodies of different tribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeasonRow {
    /// Hours read.
    pub hours: u64,
    /// Hours with a pair in sight.
    pub hours_in_sight: u64,
    /// The nearest two, over its hours (`pairs`: the pairs in sight that
    /// hour).
    pub nearest: Option<Seen>,
}

/// The meetings between tribes (its block of the data file).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeetingProbe {
    /// Ticks in a game hour (a day's ticks over 24).
    pub hour_ticks: u64,
    /// Hours read.
    pub hours: u64,
    /// Hours with at least one pair in sight.
    pub hours_in_sight: u64,
    /// Pairs in sight, summed over the hours.
    pub pair_hours: u64,
    /// Every pair ever in sight, the lower id first.
    pub pairs: BTreeSet<(BodyId, BodyId)>,
    /// The first hours with a pair in sight, [`SEEN_KEPT`] at most.
    pub seen: Vec<Seen>,
    /// By season of the cycle: `(cycle, season)`.
    pub seasons: BTreeMap<(u64, Season), SeasonRow>,
}

/// One body read: its tribe, id, tile and sight.
#[derive(Debug, Clone, Copy)]
struct Eye {
    tribe: u8,
    id: BodyId,
    at: Pos,
    sight: u32,
}

/// Tiles between two tiles, on the longer axis.
fn apart(a: Pos, b: Pos) -> u32 {
    a.x.abs_diff(b.x).max(a.y.abs_diff(b.y))
}

impl MeetingProbe {
    /// A probe reading the world every game hour.
    #[must_use]
    pub fn new(ticks_per_day: u64) -> Self {
        Self {
            hour_ticks: (ticks_per_day / 24).max(1),
            ..Self::default()
        }
    }

    /// After each tick: at the first tick of each game hour, reads where
    /// the living of each tribe stand.
    pub fn tick(&mut self, world: &World, content: &Content, people: &BTreeMap<BodyId, Person>) {
        let time = world.time();
        if !time.tick_of_day.is_multiple_of(self.hour_ticks) {
            return;
        }
        let hour = time.tick_of_day / self.hour_ticks;
        let mut eyes: Vec<Eye> = world
            .bodies()
            .iter()
            .filter(|b| b.carried_by().is_none())
            .filter_map(|b| {
                let tribe = people.get(&b.id())?.tribe;
                let at = b.pos();
                let sight = world.sight_tiles_at(at, content);
                Some(Eye {
                    tribe,
                    id: b.id(),
                    at,
                    sight,
                })
            })
            .collect();
        eyes.sort_by_key(|e| (e.at.x, e.at.y, e.id));
        let widest = eyes.iter().map(|e| e.sight).max().unwrap_or(0);
        let mut nearest: Option<(u32, usize, usize)> = None;
        let mut in_sight: Vec<(u32, usize, usize)> = Vec::new();
        for i in 0..eyes.len() {
            for j in i + 1..eyes.len() {
                let dx = eyes[i].at.x.abs_diff(eyes[j].at.x);
                let reach = nearest.map_or(u32::MAX, |n| n.0).max(widest);
                if dx > reach {
                    break;
                }
                if eyes[i].tribe == eyes[j].tribe {
                    continue;
                }
                let d = apart(eyes[i].at, eyes[j].at);
                if nearest.is_none_or(|n| d < n.0) {
                    nearest = Some((d, i, j));
                }
                if d <= eyes[i].sight.max(eyes[j].sight) {
                    in_sight.push((d, i, j));
                }
            }
        }
        self.hours += 1;
        let ends = |i: usize, j: usize| {
            let (a, b) = if eyes[i].id < eyes[j].id {
                (i, j)
            } else {
                (j, i)
            };
            ((eyes[a].id, eyes[a].at), (eyes[b].id, eyes[b].at))
        };
        let row = self.seasons.entry((time.cycle, time.season)).or_default();
        row.hours += 1;
        if let Some((d, i, j)) = nearest
            && row.nearest.is_none_or(|n| d < n.tiles)
        {
            let (a, b) = ends(i, j);
            row.nearest = Some(Seen {
                day: time.day,
                hour,
                a,
                b,
                tiles: d,
                pairs: u32::try_from(in_sight.len()).unwrap_or(u32::MAX),
            });
        }
        if in_sight.is_empty() {
            return;
        }
        row.hours_in_sight += 1;
        self.hours_in_sight += 1;
        self.pair_hours += in_sight.len() as u64;
        for &(_, i, j) in &in_sight {
            let (a, b) = ends(i, j);
            self.pairs.insert((a.0, b.0));
        }
        if self.seen.len() < SEEN_KEPT
            && let Some(&(d, i, j)) = in_sight
                .iter()
                .min_by_key(|&&(d, i, j)| (d, ends(i, j).0.0))
        {
            let (a, b) = ends(i, j);
            self.seen.push(Seen {
                day: time.day,
                hour,
                a,
                b,
                tiles: d,
                pairs: u32::try_from(in_sight.len()).unwrap_or(u32::MAX),
            });
        }
    }

    /// The block of the data file.
    #[must_use]
    pub fn json(&self) -> Value {
        let at = |p: Pos| json!([p.x, p.y]);
        let first = self.seen.first();
        json!({
            "hour_ticks": self.hour_ticks,
            "hours": self.hours,
            "hours_in_sight": self.hours_in_sight,
            "pair_hours": self.pair_hours,
            "pairs": self.pairs.len(),
            "first_day": first.map(|s| s.day),
            "first_hour": first.map(|s| s.hour),
            "seen": self.seen.iter().map(|s| json!({
                "day": s.day,
                "hour": s.hour,
                "a": s.a.0.0,
                "a_at": at(s.a.1),
                "b": s.b.0.0,
                "b_at": at(s.b.1),
                "tiles": s.tiles,
                "pairs": s.pairs,
            })).collect::<Vec<_>>(),
            "seasons": self.seasons.iter().map(|(&(cycle, season), r)| json!({
                "cycle": cycle,
                "season": season,
                "hours": r.hours,
                "hours_in_sight": r.hours_in_sight,
                "nearest_tiles": r.nearest.map(|n| n.tiles),
                "nearest_day": r.nearest.map(|n| n.day),
                "nearest_hour": r.nearest.map(|n| n.hour),
                "nearest_at": r.nearest.map(|n| json!([
                    {"body": n.a.0.0, "at": at(n.a.1)},
                    {"body": n.b.0.0, "at": at(n.b.1)},
                ])),
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sim::map::{Map, Terrain};
    use sim::time::Calendar;

    use super::*;
    use crate::load_content;

    /// A world of grass, four ticks a day (a game hour is a tick), with
    /// bodies of tribes `(tribe, x)` on one row.
    fn world_of(bodies: &[(u8, i32)]) -> (World, Content, BTreeMap<BodyId, Person>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(&dir).expect("the repository's content");
        let calendar = Calendar::new(2, 2).unwrap();
        let mut world = World::empty(calendar, Map::filled(400, 16, Terrain::Grass));
        let mut people = BTreeMap::new();
        for &(tribe, x) in bodies {
            let id = world.spawn_body(Pos::new(x, 5));
            people.insert(
                id,
                Person {
                    tribe,
                    ..Person::default()
                },
            );
        }
        (world, content, people)
    }

    #[test]
    fn bodies_of_two_tribes_within_sight_are_counted_and_the_nearest_kept() {
        // Grass: a body sees 60 tiles. A at 10 and 12, B at 50 and 300.
        let (world, content, people) = world_of(&[(0, 10), (0, 12), (1, 50), (1, 300)]);
        assert_eq!(world.sight_tiles_at(Pos::new(10, 5), &content), 60);
        let mut probe = MeetingProbe::new(world.calendar().ticks_per_day());
        assert_eq!(probe.hour_ticks, 1);
        probe.tick(&world, &content, &people);
        assert_eq!(
            (probe.hours, probe.hours_in_sight, probe.pair_hours),
            (1, 1, 2)
        );
        let ids: Vec<BodyId> = people.keys().copied().collect();
        let expected: BTreeSet<_> = [(ids[0], ids[2]), (ids[1], ids[2])].into();
        assert_eq!(probe.pairs, expected, "the far one is in sight of none");
        let seen = probe.seen[0];
        assert_eq!(
            (seen.a.0, seen.b.0, seen.tiles, seen.pairs),
            (ids[1], ids[2], 38, 2)
        );
        let row = probe.seasons.values().next().unwrap();
        assert_eq!(row.nearest.map(|n| n.tiles), Some(38));
        let block = probe.json();
        assert_eq!(block["first_day"], 1);
        assert_eq!(block["seasons"][0]["nearest_tiles"], 38);

        // The same tribe never meets itself; tribes far apart are only
        // measured.
        let (world, content, people) = world_of(&[(0, 10), (0, 12), (1, 200)]);
        let mut probe = MeetingProbe::new(world.calendar().ticks_per_day());
        probe.tick(&world, &content, &people);
        assert_eq!(
            (probe.hours, probe.hours_in_sight, probe.pair_hours),
            (1, 0, 0)
        );
        assert!(probe.seen.is_empty() && probe.pairs.is_empty());
        let row = probe.seasons.values().next().unwrap();
        assert_eq!(row.nearest.map(|n| n.tiles), Some(188));
        assert!(probe.json()["first_day"].is_null());
    }
}
