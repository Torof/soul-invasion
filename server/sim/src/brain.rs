//! The minimal brain of the NPCs (D41, the vivarium's lot 1): a pilot
//! like a player's (D20), that asks its body for what the world already
//! knows how to do, and nothing more.
//!
//! A body with a mind **thinks only when something changes** for it: it
//! is born, a gauge crosses one of the thresholds below, it stops (it
//! arrived, or was stopped), something it asked is refused, it drank or
//! ate, night falls or the day starts. Between two thoughts it costs
//! nothing. When it thinks it first **looks**: from where it stands, as
//! far as the ground lets it see ([`crate::content::Movement`]'s sight,
//! the first version's placeholder until perception comes), it remembers
//! the places it may drink at and the food lying there (its **memory of
//! places**, kept small: the nearest drinking place a chunk, the food it
//! saw). Then it decides, in this order:
//!
//! 1. **Thirsty** (thirst at [`THIRSTY`] or more, or [`PARCHED`] while
//!    asleep, which wakes it): to the nearest drinking place it knows,
//!    and drinks there; knowing none, it goes looking (`explore`).
//! 2. **Hungry** (hunger at [`HUNGRY`] or more, and until it falls under
//!    the content's `thought.fed_under`, D53): it eats what it carries
//!    (what spoils first, first); else, by day or while its reserve is
//!    weak, the nearest food it saw lying; else it goes to the **place**
//!    it knows bearing food now (D45, D53) worth most: what an hour of
//!    its best kind gives, less the farther; standing there, it picks
//!    the tree or plant worth most by what is left on it, none holding
//!    less than [`GATHER_MIN_MINUTES`] of work; knowing none, it goes
//!    looking. A hungry body whose reserve is not weak sleeps at
//!    night like the others (the D47 trial of 21 November 2026: hungry
//!    all day, a body never slept and walked twenty-two hours a day).
//! 3. **Night**: home to the camp, and sleeps within [`HOME_TILES`] of it.
//! 4. **By day, a store**: while it carries less than [`STOCK_KJ`] and
//!    its load is not full, it gathers from the nearest tree or plant it
//!    knows bearing food now.
//!    It works at it until the plant is bare or its load full (or night,
//!    thirst or hunger calls it away). The plant is chosen once in the
//!    content's `thought.plant_choice_hours` (D53): on its way to one it
//!    sticks to it unless it is spent or out of reach, or a need calls.
//! 5. Otherwise, by day: home to the camp if it is away, and waits there.
//!
//! It remembers the **places** where it saw trees and plants bearing food
//! (D45, D53: squares of the content's `thought.place_tiles`, with what
//! and how many; nobody remembers every bush), as it remembers water,
//! the content's `thought.places_remembered` worth most from where it
//! last looked; a place it found bare it leaves alone until that crop's
//! next year, one it could not reach until the next day; a plant whose
//! gather the world refused (nothing ripe on it yet, its load full), for
//! [`RETRY_HOURS`]. Hungry, it gathers only while its load is not full
//! either.
//!
//! Every choice comes with its reason ([`Why`]), so the vivarium's report
//! can say why a body did what it did. Everything is deterministic: the
//! same world and the same seed give the same thoughts.
//!
//! **Trials, selection and imitation (D50).** The constants that shape a
//! day are **ways** on each mind ([`Ways`]): how far a plant is worth
//! going, whether it sleeps out, how much it stores, where home is. At
//! nightfall each mind closes its day: what it gathered, whether it is
//! thin, thirsty or hungry, whether the night before froze it, whether
//! the day was bare, and whether it is in **distress** (hungry or
//! thirsty at the day's end, or a day with no gather at all). Distressed for
//! `trial_after_days` running, it
//! may start a **trial** (a daily chance by its **will**, a placeholder
//! drawn from the world's seed and its id until the souls' Willpower
//! comes, D30): one way moved by one step, drawn from the seed, its id
//! and the day. After `trial_days` the world has **judged** on every
//! distress: the way is kept if nothing worsened and one thing improved
//! (gathered a day, thin days, thirsty days, cold days), else reverted. A
//! new home goes to the place or grove of its notebook where its own
//! body would fare best (what it gathered and drank there, less what its
//! nights left lacking and its walking cost, every need alike), lived
//! for the days its will asks. Lying down
//! at its camp at night, a body may **copy** one way from the camp-mate
//! who fared best lately (the fewest days of distress, gathered kJ
//! breaking ties), the more readily the greater that mate's will,
//! sometimes losing a step to the copy. Children follow their mother's
//! home every night, a carried
//! infant its carrier's. The brain tells these in its [`News`], drained
//! by the vivarium after each turn ([`Brains::news`]). The content's
//! section `trials` holds every number ([`crate::content::Trials`]);
//! absent, nobody tries anything.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::content::{Content, Decisions, Fire, Reputation, Temperance, Trials};
use crate::crops::{self, Bearer, Source};
use crate::r#gen::scatter::{self, purpose};
use crate::lru::Lru;
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, Map, Pos, Terrain};
use crate::memory::{self, Access, Fared, Record, RecordId, Store};
use crate::{Blockage, Body, BodyId, Event, GATHER_REACH_MM, Intention, ItemId, World};

/// Thirst from which a body goes to drink: half a game day without
/// water (thirst rises 100 points a day), so it drinks before the night
/// and twice a day.
pub const THIRSTY: u32 = 50;
/// Thirst from which a sleeping body wakes to drink: it would not
/// otherwise last the night.
pub const PARCHED: u32 = 80;
/// Fatigue at or above which a sleeping body stays down even parched: it
/// collapsed of fatigue (the world lays a body down at 100 wherever it
/// stands) and does not rise before it has slept a tenth of its gauge off
/// (48 game minutes at the content's rest); then thirst drives it on.
/// Without it, a parched body collapsed on its way rose at each order
/// of its pilot and fell again a tick later: eight thousand decisions a
/// day and no progress (22 November 2026).
pub const COLLAPSED: u32 = 90;
/// Hunger from which a body goes to eat; it counts as fed again only
/// under the content's `thought.fed_under` (D53: a hysteresis, so that it
/// finishes its meal and does not think of hunger every quarter of an
/// hour while it hovers at the threshold).
pub const HUNGRY: u32 = 50;
/// How near the camp a body counts as home, in tiles (a little more than
/// the ring the bodies are born in).
pub const HOME_TILES: i32 = 6;
/// How many drinking places and how many foods a body remembers: the
/// nearest to the camp are kept.
pub const MEMORY_PLACES: usize = 64;
/// How far, in tiles, a plant's worth is halved: a body weighs what an
/// hour of gathering gives against the walk there (to the place it
/// remembers, D53; the places worth most from where it last looked are
/// kept, the content's `thought.places_remembered` of them).
pub const WORTH_HALF_TILES: u64 = 32;
/// How far, in tiles, a body walks before it looks again for drinking
/// places and for trees and plants bearing food: a lake shore holds
/// thousands of tiles of water, a meadow thousands of plants, and its
/// sight is far wider than that.
pub const LOOK_STEP: i32 = 8;
/// The food a body gathers by day before it rests, in kilojoules: three
/// days of a person's needs.
pub const STOCK_KJ: u64 = 30_000;
/// How long, in game hours, a body leaves alone a plant whose gather was
/// refused: asked again at once, the same refusal came back every tick
/// (a plant picked bare while it ripens, a load already full), a
/// livelock that cost 80 to 95 % of the tick (audit of 18 Nov 2026).
pub const RETRY_HOURS: u64 = 1;
/// How many refused trees and plants a body holds in mind at most.
const LEFT_ALONE: usize = 16;
/// A plant holding less than this many minutes of gathering is not
/// worth the walk: with the fruit ripening along its season (D48), a
/// meadow holds a trickle on each of thousands of plants, and a body
/// that ranked them by what an hour of a full one gives spent its days
/// walking from crumb to crumb (the D47 trial of 21 November 2026: one
/// to thirty grams a gather, 400 to 2,000 kJ an hour of gathering).
pub const GATHER_MIN_MINUTES: u32 = 10;
/// The dawn and the nightfall make every mind think, not all in one
/// tick: each within this many game seconds after, by its body's id (a
/// dawn cost up to 116 ms with every mind at once). So does a gauge
/// past a threshold: a tribe born at once grows hungry in one tick.
pub const WAKE_SPREAD_SECONDS: u64 = 60;
/// How many chunks' trees and plants the brains place a game second at
/// most, spread evenly over its ticks (one a tick at the game's ten):
/// placing one costs 15 to 30 ms on the vivarium's land, and a body
/// seeing new land sees some twenty-five at once. The rest are placed at
/// the next turns, and the body learns their plants then. A second, not
/// a tick, so that what a body knows does not hang on the cadence.
pub const COLD_CHUNKS_PER_SECOND: u64 = 10;
/// How much the brains think a game second at most, spread evenly over
/// its ticks, in work: a thought counts one, and each chunk its look
/// reads for trees and plants one more (a look over new land reads some
/// twenty-five, and costs ten times a thought that does not move). The
/// minds due past a turn's share wait for the next turn, still due, in
/// turn by their bodies' ids from the first one that waited; the work a
/// turn spent past its share (its last thought ran whole) is repaid by
/// the next. A count, not a clock (D24). Only crowds larger than today's
/// ask so much at once: 800 people of two tribes asked at most 6,377 in
/// a tick of the game's ten in their first day (some 30 ms), 7,978 in a
/// tick of a second; a work costs some 5 µs, so a full share of the
/// game's ten some 50 ms.
pub const THOUGHT_WORK_PER_SECOND: u64 = 100_000;
/// How many chunks' trees and plants bearing food the brains hold at
/// least, the least lately used forgotten first (placed again if seen
/// again): as many as the world's own trees
/// ([`crate::ground::TREE_CACHE_CHUNKS`], four views), widened to the
/// chunks of the world they think in ([`Brains::think`]): two tribes
/// wander over more than four views, and a chunk forgotten was placed
/// again at some milliseconds (the vivarium's world has 4,624 chunks; the
/// valley 9,216, a chunk's bearers some 34 bytes each, held once with the
/// world's, S4). The same for the shores to drink from.
pub const BEARER_CHUNKS: usize = crate::ground::TREE_CACHE_CHUNKS;

/// Why a body was asked to do something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Thirsty: it walks to a drinking place it knows, or drinks there.
    Thirst,
    /// Hungry: it walks to food it knows, or eats it.
    Hunger,
    /// Thirsty or hungry and knowing no place: it goes looking.
    Explore,
    /// Night: it walks home, or sleeps there.
    Night,
    /// Day, nothing pressing: it walks home.
    Home,
    /// Day, carrying less than its store: it gathers (D45).
    Gather,
}

/// A thought: what a body was asked, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// What it was asked.
    pub intention: Intention,
    /// Why.
    pub why: Why,
}

/// A place a body may drink at: the tile it stands on, walkable, and the
/// water within reach of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrinkingPlace {
    /// Where to stand.
    pub stand: Pos,
    /// The water to drink, on `stand` or next to it.
    pub water: Pos,
}

/// What a body is about, between two thoughts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doing {
    Nothing,
    ToDrink(DrinkingPlace),
    ToEat(ItemId, Pos),
    Exploring(Why),
    Home,
    /// On its way to a place it remembers bearing food (D53).
    ToPlace(PlaceKey),
    ToGather(Aim),
    Gathering(Aim),
}

/// A tree or plant a body is going to gather from, or gathers from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Aim {
    /// Who it is.
    key: (ChunkPos, Source),
    /// Where it stands.
    at: FinePos,
    /// Its kind, by its number among the content's kinds bearing food
    /// ([`Brains::kind_name`]).
    kind: u16,
}

/// Where a place is: its square's coordinates, a tile's divided by the
/// place's side (the content's `thought.place_tiles`).
pub type PlaceKey = memory::PlaceKey;

/// A place a body remembers (D53): a square of the content's
/// `thought.place_tiles` tiles on a side where it saw trees and plants
/// bearing food, with what and how many. Nobody remembers every bush: a
/// body remembers the corners, and looks again on arrival.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Place {
    /// What it saw there, the kinds rising.
    kinds: Vec<Seen>,
    /// The crop years it found the place bare for, by kind: left alone
    /// until the next.
    spent: Vec<(u16, u64)>,
    /// The tick before which it is left alone: nothing could be gathered
    /// there for now (its plants still ripening, a gather refused). It
    /// tries again [`RETRY_HOURS`] later, or another place meanwhile.
    not_before: Option<u64>,
    /// The record it holds of this place (D55): its own, told at the
    /// fire; or one heard, by whom and when. `None` for a draft never
    /// told: what it saw is in `kinds`.
    access: Option<Access>,
    /// The day it last looked at the place itself.
    looked_day: Option<u32>,
    /// What an hour of the rumour it dispelled there was told to give
    /// (D55): the bare place is told tonight, worth telling as much as
    /// the rumour was; 0 when none.
    rumour_kj: u64,
    /// Seen, not reached (D53, amended 24 November 2026): its own mark,
    /// a walk to it barred by water or rock, or one heard at the fire.
    /// The place is no longer chosen nor called while it holds; it clears
    /// only when the body itself stands on it.
    barred: Option<Mark>,
    /// The last day it stood on the place.
    stood: Option<u32>,
}

/// A mark of seen, not reached a body holds (D53, amended 24 November
/// 2026), as it holds a place (D55).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Its own, not yet told: the day its walk was barred.
    Draft(u32),
    /// The record it holds: its own, told at the fire; or one heard, by
    /// whom and when.
    Held(Access),
}

/// The day of the world as a `u32`.
fn today_u32(world: &World) -> u32 {
    u32::try_from(today(world)).unwrap_or(u32::MAX)
}

/// A place found bare where the record it held said food (D55, amended by
/// lot 5a): "I found it bare", told at the next fire apart from the
/// `told_per_night` slots, to those present who hold a record of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Denial {
    /// The place.
    place: PlaceKey,
    /// The day of the look that found it bare.
    day: u32,
    /// What that look saw: each kind there, bare.
    states: Vec<(u16, u8)>,
    /// What an hour of what the record said was worth: the rumours worth
    /// most are denied first.
    worth_kj: u64,
}

/// What a body saw of one kind at a place: how many trees and plants of
/// it, and how many held food worth a step when it last looked (D48: what
/// is there ripens and is picked; one sees the fruit on a bush, not a
/// bush).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Seen {
    /// Its kind's number ([`Brains::kind_name`]).
    kind: u16,
    /// How many of it stand there, at most (a place seen from its edge
    /// shows part of itself).
    plants: u16,
    /// How many held [`GATHER_MIN_MINUTES`] of food or more when it last
    /// looked.
    holding: u16,
    /// What an hour of the fullest of them gave when it last looked, in
    /// kilojoules ([`kj_of_an_hour`]); 0 when none held food.
    best_kj: u64,
}

impl Place {
    /// It saw `plants` of `kind` there, `holding` of them with food worth
    /// a step, the fullest giving `best_kj` an hour: the most plants it
    /// ever saw, what they held at this look.
    fn saw(&mut self, kind: u16, plants: u16, holding: u16, best_kj: u64) {
        match self.kinds.binary_search_by_key(&kind, |k| k.kind) {
            Ok(i) => {
                self.kinds[i].plants = self.kinds[i].plants.max(plants);
                self.kinds[i].holding = holding;
                self.kinds[i].best_kj = best_kj;
            }
            Err(i) => self.kinds.insert(
                i,
                Seen {
                    kind,
                    plants,
                    holding,
                    best_kj,
                },
            ),
        }
    }

    /// Marked seen, not reached.
    fn is_barred(&self) -> bool {
        self.barred.is_some()
    }

    /// Found bare of `kind` for the crop's `year`.
    fn is_spent(&self, kind: u16, year: u64) -> bool {
        self.spent.contains(&(kind, year))
    }

    /// It found the place bare of `kind` for the crop's `year`.
    fn spend(&mut self, kind: u16, year: u64) {
        self.spent.retain(|s| s.0 != kind);
        self.spent.push((kind, year));
    }

    /// What bears there now, seen holding food and not spent, by what an
    /// hour of its fullest plant gave when last seen, the more holding
    /// breaking ties; `None` if nothing.
    fn bearing(&self, food: &Food) -> Option<Seen> {
        self.kinds
            .iter()
            .filter(|seen| {
                seen.holding > 0
                    && food
                        .now(seen.kind)
                        .is_some_and(|bears| !self.is_spent(seen.kind, bears.year))
            })
            .max_by_key(|seen| (seen.best_kj, seen.holding, std::cmp::Reverse(seen.kind)))
            .copied()
    }

    /// How many trees and plants it knows there bearing now, seen holding
    /// food, not spent.
    fn bearing_count(&self, food: &Food) -> u32 {
        self.kinds
            .iter()
            .filter(|seen| {
                food.now(seen.kind)
                    .is_some_and(|bears| !self.is_spent(seen.kind, bears.year))
            })
            .map(|seen| u32::from(seen.holding))
            .sum()
    }

    /// For the memory's ranking ([`rank_place`]): whether anything bears
    /// there now, seen holding food, and what an hour gives there (of the
    /// fullest plant seen, else of its best kind's best crop).
    fn rank_kj(&self, food: &Food) -> (bool, u64) {
        let now = self
            .kinds
            .iter()
            .filter(|seen| seen.holding > 0 && food.now(seen.kind).is_some())
            .map(|seen| seen.best_kj)
            .max();
        match now {
            Some(kj) => (true, kj),
            None => (
                false,
                self.kinds
                    .iter()
                    .map(|seen| food.best_kj[usize::from(seen.kind)])
                    .max()
                    .unwrap_or(0),
            ),
        }
    }
}

/// The side of a place, in tiles (the content's `thought.place_tiles`).
fn place_side(rules: &Content) -> i32 {
    i32::try_from(rules.needs.thought.place_tiles)
        .unwrap_or(1)
        .max(1)
}

/// How many places a body remembers (`thought.places_remembered`).
fn places_remembered(rules: &Content) -> usize {
    usize::try_from(rules.needs.thought.places_remembered)
        .unwrap_or(1)
        .max(1)
}

/// The place `tile` lies in, for places `side` tiles wide.
fn place_of(tile: Pos, side: i32) -> PlaceKey {
    (tile.x.div_euclid(side), tile.y.div_euclid(side))
}

/// The middle tile of the place `key`.
fn place_centre(key: PlaceKey, side: i32) -> Pos {
    Pos::new(key.0 * side + side / 2, key.1 * side + side / 2)
}

/// Does a body on `tile` stand at the place `key`: in its square, or
/// within half a side of it, where it sees the whole place and picks its
/// plant.
fn at_place(tile: Pos, key: PlaceKey, side: i32) -> bool {
    let (x0, y0) = (key.0 * side, key.1 * side);
    let margin = side / 2;
    (x0 - margin..x0 + side + margin).contains(&tile.x)
        && (y0 - margin..y0 + side + margin).contains(&tile.y)
}

/// The ways of a body (D50): the brain's constants that shape its day,
/// one set a body, defaulting to the constants. A way is a value the body
/// carries in its mind, not a rule; it moves by trials and by imitation,
/// never by a rule named after its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ways {
    /// How far, in tiles, a plant's worth is halved ([`WORTH_HALF_TILES`]).
    pub worth_half_tiles: u64,
    /// Whether it sleeps where it stands at night (true), or goes home
    /// first (false, the constants' way).
    pub sleep_out: bool,
    /// The food it gathers by day before it rests, in kilojoules
    /// ([`STOCK_KJ`]).
    pub stock_kj: u64,
    /// Its home (D50, amended 24 November 2026; the lead: "maintenant
    /// c'est ma maison, ce n'est plus celle-là. C'est là où il va
    /// revenir"): **one at a time**, the place it returns to at dusk,
    /// sleeps at (unless it sleeps out), whose fire it sits at, and goes
    /// looking from. A home trial sets it at once and it is provisional
    /// until judged: kept, it is the home and the old one survives only
    /// as a memory in the notebook ([`Mind`]'s `lived`); not kept, the
    /// trial's ways before set it back and the body returns. There is no
    /// other home anywhere in a mind.
    pub home: Pos,
}

impl Ways {
    /// The constants' ways, the camp at `home`.
    #[must_use]
    pub fn at(home: Pos) -> Self {
        Self {
            worth_half_tiles: WORTH_HALF_TILES,
            sleep_out: false,
            stock_kj: STOCK_KJ,
            home,
        }
    }
}

/// A way a trial or a copy may move (D50): one of [`Ways`]' fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Way {
    /// [`Ways::worth_half_tiles`], twice or half.
    WorthHalfTiles,
    /// [`Ways::sleep_out`], flipped.
    SleepOut,
    /// [`Ways::stock_kj`], twice or half.
    StockKj,
    /// [`Ways::home`], moved.
    Home,
}

impl Way {
    /// Every way, in a fixed order.
    pub const ALL: [Way; 4] = [Way::WorthHalfTiles, Way::SleepOut, Way::StockKj, Way::Home];

    /// Its value in `ways` set to its value in `from`.
    fn copy(self, ways: &mut Ways, from: &Ways) {
        match self {
            Way::WorthHalfTiles => ways.worth_half_tiles = from.worth_half_tiles,
            Way::SleepOut => ways.sleep_out = from.sleep_out,
            Way::StockKj => ways.stock_kj = from.stock_kj,
            Way::Home => ways.home = from.home,
        }
    }

    /// Its name in the content ([`crate::content::TRIAL_WAYS`]).
    #[must_use]
    pub fn key(self) -> &'static str {
        let [worth, sleep, stock, home] = crate::content::TRIAL_WAYS;
        match self {
            Way::WorthHalfTiles => worth,
            Way::SleepOut => sleep,
            Way::StockKj => stock,
            Way::Home => home,
        }
    }

    /// Whether it is the same in `a` and `b`.
    fn same(self, a: &Ways, b: &Ways) -> bool {
        match self {
            Way::WorthHalfTiles => a.worth_half_tiles == b.worth_half_tiles,
            Way::SleepOut => a.sleep_out == b.sleep_out,
            Way::StockKj => a.stock_kj == b.stock_kj,
            Way::Home => a.home == b.home,
        }
    }
}

/// What a present says at the fire it will do tomorrow (D56).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Act {
    /// To a place it knows, alone (heard, or seen and nobody joined).
    Alone,
    /// To a place it saw, calling the others: it leads a group.
    Called,
    /// With a caller, to the place it called.
    Joined,
    /// Looking for land it does not know, alone, one way.
    Looked,
    /// Nothing said: no place worth a day, or it was not at the fire.
    Nothing,
}

impl Act {
    /// Every act, in a fixed order.
    pub const ALL: [Act; 5] = [
        Act::Alone,
        Act::Called,
        Act::Joined,
        Act::Looked,
        Act::Nothing,
    ];

    /// Its code in a habit's byte.
    fn code(self) -> u8 {
        match self {
            Act::Alone => 0,
            Act::Called => 1,
            Act::Joined => 2,
            Act::Looked => 3,
            Act::Nothing => 4,
        }
    }
}

/// How a group of the day ended (D56).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupEnd {
    /// Its caller took it home, enough of it carrying what it wanted.
    Home,
    /// The place was bare and the caller knew nothing worth going on to.
    Bare,
    /// The caller could not walk to the place: seen, not reached (24
    /// November 2026; told as bare before).
    Blocked,
    /// The caller left it (thirst, hunger, its death) or the night came.
    Night,
}

/// What a body said it will do on a day, and how far it is with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Plan {
    /// The day it is for.
    day: u32,
    act: Act,
    /// The place it goes to (alone, called, joined).
    place: Option<PlaceKey>,
    /// Its caller, when it joined one; itself when it called.
    leader: Option<BodyId>,
    /// The way it looks, one of eight ([`DIRECTIONS`]), when it looks.
    octant: u8,
    /// Legs walked looking.
    legs: u32,
    /// Done with: the place found bare, the group home, the legs walked.
    done: bool,
    /// Whom it asked "where is food?" at the fire, and the activity it
    /// believed that one best at (D56, PR 1 ter): it goes where the
    /// answer said.
    asked: Option<(BodyId, u8)>,
}

/// A group of the day (D56): a caller and those who joined it, walking to
/// one place, gathering there, going home together.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    /// The day it is for.
    day: u32,
    /// The place called for.
    called: PlaceKey,
    /// The place it goes to now: the caller moves it on when it finds the
    /// one called bare.
    place: PlaceKey,
    /// Its members, the caller first.
    members: Vec<BodyId>,
    /// The plant each member is at or going to: one does not pick the
    /// bush a mate is on.
    taken: BTreeMap<BodyId, (ChunkPos, Source)>,
    /// Who has what it wanted.
    full: BTreeSet<BodyId>,
    /// Who walked behind the caller's way at least once.
    followed: BTreeSet<BodyId>,
    /// Who left it on the way.
    left: BTreeSet<BodyId>,
    /// When each member first stood at the place.
    arrived: BTreeMap<BodyId, u64>,
    /// The tick of the fire that formed it: its members wait for the
    /// caller to set off an hour at most after it.
    dawn: u64,
    /// The caller called it home.
    home: bool,
    /// It ended (bare and nothing next, the caller gone).
    ended: Option<GroupEnd>,
    /// The caller stands waiting on the way for one left behind (D56,
    /// "look behind"), and how long it has waited so far, in ticks.
    held: bool,
    waited: u64,
    /// The caller has set off.
    started: bool,
    /// The members it waited for on the way (D56, PR 1 ter: a fact of
    /// its review).
    waited_for: BTreeSet<BodyId>,
}

/// A trial under way (D50): the way moved, the ways before the step, and
/// what the days before it gave, to be judged against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trial {
    /// The way moved.
    pub way: Way,
    /// The ways before the step: the old value is read from them.
    pub before: Ways,
    /// The day it started (the day closing at that nightfall).
    pub started_day: u64,
    /// What the `trial_days` before gave: the judge compares against it
    /// when it heard nobody.
    pub before_days: Lately,
    /// The days it heard at the fire of camp-mates who changed nothing
    /// (D50, amended): the judge compares against their mean.
    pub heard: HeardDays,
}

/// The days a body in trial heard at the fire from camp-mates who changed
/// nothing, summed night after night: the relative judge's other side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeardDays {
    /// Their gathered kJ, each in per cent of its own need, summed.
    pub kj_percent: u64,
    /// Their days ended thin.
    pub thin: u32,
    /// Their days ended thirsty.
    pub thirsty: u32,
    /// Their days whose night before was cold.
    pub cold: u32,
    /// Days heard.
    pub days: u32,
    /// Nights at which some were heard.
    pub nights: u32,
}

impl HeardDays {
    /// One more day heard.
    fn add(&mut self, fared: Fared) {
        self.kj_percent += u64::from(fared.kj_percent);
        self.thin += u32::from(fared.thin);
        self.thirsty += u32::from(fared.thirsty);
        self.cold += u32::from(fared.cold);
        self.days += 1;
    }

    /// Their mean, as a [`Lately`] over `days` days (integers, the
    /// division whole): what a body in trial compares its own days with.
    /// `None` when none was heard.
    fn mean(&self, days: u32) -> Option<Lately> {
        if self.days == 0 {
            return None;
        }
        let per = |n: u32| n.saturating_mul(days) / self.days;
        Some(Lately {
            kj_per_day: self.kj_percent / u64::from(self.days),
            thin_days: per(self.thin),
            thirsty_days: per(self.thirsty),
            cold_days: per(self.cold),
            distress_days: 0,
        })
    }
}

/// A way judged today (D55): told at the fire tonight, if its body is
/// there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Judged {
    way: Way,
    after: Ways,
    /// The direction of the step ([`step_up`]).
    up: bool,
    kept: bool,
    before: Lately,
    during: Lately,
}

/// The direction of a step of `way` from `before` to `after`: wider or
/// narrower, more store or less, out or home, a home moved (always up).
fn step_up(way: Way, before: &Ways, after: &Ways) -> bool {
    match way {
        Way::WorthHalfTiles => after.worth_half_tiles > before.worth_half_tiles,
        Way::StockKj => after.stock_kj > before.stock_kj,
        Way::SleepOut => after.sleep_out,
        Way::Home => true,
    }
}

/// One day as a mind closed it at nightfall (D50): what it gathered, and
/// every distress of the day.
#[allow(
    clippy::struct_excessive_bools,
    reason = "five marks of one day, each read by its name"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Day {
    /// Gathered over the day, in kilojoules.
    gathered_kj: u64,
    /// Thin at nightfall.
    thin: bool,
    /// Thirsty at nightfall ([`THIRSTY`] or more).
    thirsty: bool,
    /// Cold the night before: its cold was full, its vitality drained.
    cold: bool,
    /// Hungry at nightfall ([`HUNGRY`] or more).
    hungry: bool,
    /// Nothing gathered at all.
    bare: bool,
}

/// The most places a body keeps in its notes of the nights (D53, amended
/// 23 November 2026): the least lived forgotten first.
const LIVED_MAX: usize = 8;

/// What a body perceives of how it fares, each in its own gauge's unit
/// and all counted alike, per mille of a day of that need (D50, D53,
/// amended 24 November 2026; the lead: the home must end at the optimal
/// place, for good, whatever comes to count later). **A new perception
/// plugs in here**: a variant (fear, sight, warmth kept), what a place
/// gives of it noted in [`DayAct::gains`] where it is given, what the
/// body suffered of it at night in [`Night::losses`]; [`fare`] sums every
/// sense and the home's choice reads nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sense {
    /// Food: what was gathered there, in per mille of a day's need; at
    /// night the hunger left (a hunger point is a hundredth of a day).
    Food,
    /// Water: what was drunk there, the thirst since the last drink in per
    /// mille of a day; at night the thirst left.
    Water,
    /// Warmth: nothing gives it yet; at dawn the cold the night left.
    Warmth,
    /// Company (D53, amended 24 November 2026): a need of the body felt
    /// only at night at the fire, where anyone present counts, family no
    /// more than others ([`crate::content::Company`]). Nothing gives it by
    /// day; at dawn what too few at its fire left it lacking, its whole
    /// need at no fire. It grades places, and orders nothing.
    Company,
}

/// How many [`Sense`]s.
const SENSES: usize = 4;

/// A place in a body's notebook (D50 and D53, amended 23 and 24 November
/// 2026), as its own body lived it, summed and faded day by day, a night
/// or a day counting a thousand: a place it slept or drank at, of
/// `place_tiles`, or a grove it gathered in, of `trials.grove_tiles`. Each
/// holds only what was done there: the night's losses where it slept, what
/// it drank where it drank, what it gathered where it gathered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lived {
    /// The place, on its own grid.
    place: PlaceKey,
    /// A grove (`trials.grove_tiles`) rather than a place (`place_tiles`).
    grove: bool,
    /// The nights slept there, faded.
    nights: u64,
    /// The days it drank or gathered there without sleeping there, faded.
    days: u64,
    /// What the place gave, by [`Sense`].
    gains: [u64; SENSES],
    /// What its nights there left it lacking, by [`Sense`].
    losses: [u64; SENSES],
    /// What its days ending there cost in walking, in per mille of a day's
    /// food.
    walk: u64,
    /// What walking there from home and back cost, in per mille of a day's
    /// food: what living there would have saved.
    saved: u64,
}

impl Lived {
    /// The nights and days lived there, faded.
    fn lived(&self) -> u64 {
        self.nights + self.days
    }

    /// A day's share of `sum`.
    fn per_day(&self, sum: u64) -> u64 {
        sum / self.lived().max(1)
    }

    /// A night's share of `sum`.
    fn per_night(&self, sum: u64) -> u64 {
        sum / self.nights.max(1)
    }
}

/// What a body did at a place in its day, noted at the next dawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DayAct {
    /// The place, on its own grid.
    place: PlaceKey,
    /// A grove rather than a place.
    grove: bool,
    /// The least that walking there from home and back cost that day, in
    /// per mille of a day's food.
    saved: u64,
    /// What it got there, by [`Sense`].
    gains: [u64; SENSES],
}

/// A night to note at the next dawn, where the body lies.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Night {
    /// What it lacked, by [`Sense`]: hunger and thirst at nightfall, the
    /// cold at dawn.
    losses: [u64; SENSES],
    /// What the day's walking cost, in per mille of a day's food.
    walk: u64,
    /// What it did that day, place by place.
    day: Vec<DayAct>,
}

impl Day {
    /// A day of distress: any of its marks.
    fn distress(self) -> bool {
        self.thin || self.thirsty || self.cold || self.hungry || self.bare
    }
}

/// What a mind's last days gave (D50): what the judge compares and what
/// the camp-mates look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lately {
    /// Gathered a day, in kilojoules.
    pub kj_per_day: u64,
    /// Days ended thin.
    pub thin_days: u32,
    /// Days ended thirsty.
    pub thirsty_days: u32,
    /// Days whose night before was cold.
    pub cold_days: u32,
    /// Days of distress of any kind (thin, thirsty, cold, hungry, bare).
    pub distress_days: u32,
}

impl Lately {
    /// The judge's verdict (D50 b): nothing worsened and one thing
    /// improved, over gathered a day, thin days, thirsty days and cold
    /// days.
    #[must_use]
    pub fn better_than(&self, before: &Self) -> bool {
        let worse = self.kj_per_day < before.kj_per_day
            || self.thin_days > before.thin_days
            || self.thirsty_days > before.thirsty_days
            || self.cold_days > before.cold_days;
        let better = self.kj_per_day > before.kj_per_day
            || self.thin_days < before.thin_days
            || self.thirsty_days < before.thirsty_days
            || self.cold_days < before.cold_days;
        !worse && better
    }

    /// How well it fared, the greater the better: the fewest days of
    /// distress, gathered a day breaking ties.
    #[must_use]
    pub fn fared(&self) -> (std::cmp::Reverse<u32>, u64) {
        (std::cmp::Reverse(self.distress_days), self.kj_per_day)
    }
}

/// One evening's review and questions at a fire (D56, PR 1 ter).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EveningReview {
    /// Companions a caller called first, by name.
    pub named: u32,
    /// Companions set aside ("not with us tomorrow").
    pub aside: u32,
    /// Calls one set aside would have joined, refused.
    pub refused: u32,
    /// Joins weighed by a reputation: the call of whom one believes best
    /// at gathering taken first.
    pub weighed: u32,
    /// Questions asked ("where is food?").
    pub asked: u32,
    /// Of them, answered with a place.
    pub answered: u32,
    /// Those who said nothing because they carry their stock and a day's
    /// need (a measure of the « nothing » days).
    pub stocked: u32,
    /// Those who said nothing knowing no place bearing now.
    pub no_place: u32,
    /// Those who said nothing, every place they weighed taken whole.
    pub all_taken: u32,
}

/// A follower joined its caller's group (D56, PR 1 ter).
pub const FOLLOW_JOINED: u8 = 0;
/// A follower went where the one it asked answered.
pub const FOLLOW_ASKED: u8 = 1;
/// A follower went to a place another found and told.
pub const FOLLOW_TOLD: u8 = 2;

/// What the brain tells of its trials (D50), drained with
/// [`Brains::news`] after each turn: the vivarium's chronicle reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum News {
    /// `body`'s night (D53, amended 24 November 2026): `others` at its
    /// fire the night of `day`, 0 at no fire; its sociability.
    Company {
        /// Who.
        body: BodyId,
        /// The day the night closed.
        day: u32,
        /// Others at its fire.
        others: u32,
        /// Its sociability, 0 to 100.
        sociable: u8,
    },
    /// `body` marked `place` seen, not reached (D53, amended 24 November
    /// 2026): its own walk there was barred, or `heard` at the fire.
    Barred {
        /// Who.
        body: BodyId,
        /// The place.
        place: PlaceKey,
        /// What barred its own walk; `None` when heard at the fire.
        cause: Option<Blockage>,
        /// When.
        at: u64,
    },
    /// `body`'s mark on `place` cleared: it stood on it itself (the way
    /// is not told: a mate who stood there does not clear it).
    Reached {
        /// Who.
        body: BodyId,
        /// The place.
        place: PlaceKey,
        /// When.
        at: u64,
    },
    /// `body` started a trial: `way` moved from `before` to `after`.
    Trying {
        /// Who.
        body: BodyId,
        /// The way moved.
        way: Way,
        /// Its ways before the step.
        before: Ways,
        /// Its ways after it.
        after: Ways,
        /// Whether the step was a kept way heard at the fire (D55), or
        /// its own draw.
        heard: bool,
        /// The tick.
        at: u64,
    },
    /// The world has judged `body`'s trial of `way`: kept, or reverted.
    Tried {
        /// Who.
        body: BodyId,
        /// The way tried.
        way: Way,
        /// Kept (it gathered more a day, thin no more often) or reverted.
        kept: bool,
        /// Judged against this many days heard at the fire of camp-mates
        /// who changed nothing (D50, amended); 0: against its own days
        /// before.
        heard_days: u32,
        /// Over this many nights.
        heard_nights: u32,
        /// The tick.
        at: u64,
    },
    /// At the fire of `camp`, `body` told `record` (D55).
    Told {
        /// Who.
        body: BodyId,
        /// The record.
        record: RecordId,
        /// The camp.
        camp: Pos,
        /// The day that closed.
        day: u32,
        /// The tick.
        at: u64,
    },
    /// At the fire, `body` heard `record` from `from` and kept an access
    /// to it, with this credit (D55).
    Heard {
        /// Who.
        body: BodyId,
        /// The record.
        record: RecordId,
        /// Who told it.
        from: BodyId,
        /// Its credit in it, 1 to 100.
        credit: u8,
        /// The tick.
        at: u64,
    },
    /// `body` went to a place it had heard of from `from` and found none
    /// of the food told: the rumour is dispelled, and told tonight (D55).
    Dispelled {
        /// Who.
        body: BodyId,
        /// The place.
        place: PlaceKey,
        /// The record it believed.
        record: RecordId,
        /// Who had told it.
        from: BodyId,
        /// The tick.
        at: u64,
    },
    /// At the fire of `camp` (D56), `body` said what it will do on `day`.
    Decided {
        /// Who.
        body: BodyId,
        /// What it said.
        act: Act,
        /// The place it goes to, if any.
        place: Option<PlaceKey>,
        /// The one it goes with, its caller, if it joined one.
        leader: Option<BodyId>,
        /// The camp.
        camp: Pos,
        /// The day decided for.
        day: u32,
        /// The tick.
        at: u64,
    },
    /// The decisions of one night at `camp` (D56), counted.
    Evening {
        /// The camp.
        camp: Pos,
        /// The first who spoke: whose tribe the night is.
        by: BodyId,
        /// The day decided for.
        day: u32,
        /// Choices passed over because the place's room was taken: the
        /// rush the calls stop.
        skipped: u32,
        /// Calls whose room was filled.
        calls_full: u32,
        /// Bodies of the camp who were not at the fire (slept out, did not
        /// come home): no decision for them.
        absent: u32,
        /// Whether a full place was told tonight.
        full_told: bool,
        /// The evening's review and questions (PR 1 ter): companions
        /// called first by name, set aside ("not with us tomorrow"), calls
        /// refused to one set aside, joins weighed by a reputation,
        /// questions asked and answered with a place.
        review: EveningReview,
    },
    /// A caller told a companion "not with us tomorrow" at the fire (D56,
    /// PR 1 ter): judged over its last outings with it.
    Aside {
        /// The caller.
        leader: BodyId,
        /// The one set aside.
        body: BodyId,
        /// The day it is set aside for.
        day: u32,
        /// When.
        at: u64,
    },
    /// A caller called a good companion first, by name, at the fire
    /// (D56, PR 1 ter), and it joins: a measure of the review.
    Named {
        /// The caller.
        leader: BodyId,
        /// The one named.
        body: BodyId,
        /// When.
        at: u64,
    },
    /// Why it stopped gathering today and what it carried (lot 5b, a
    /// measure of the review): told at nightfall, before its day's habit.
    Stopped {
        /// The body.
        body: BodyId,
        /// Why it went home first today: one of the `STOP_` reasons.
        why: u8,
        /// What it carried then, in kilojoules (0 if it never went).
        stop_kj: u64,
        /// What it carries at nightfall, in kilojoules.
        evening_kj: u64,
        /// What it burns a day, in kilojoules.
        need_kj: u64,
        /// When.
        at: u64,
    },
    /// One with no place to go asked whom it believes best "where is
    /// food?" at the fire (D56, PR 1 ter).
    Asked {
        /// Who asked.
        body: BodyId,
        /// Whom.
        of: BodyId,
        /// The activity it believes that one best at, an index in
        /// `content::ACTIVITIES`.
        activity: u8,
        /// The place answered, if any.
        place: Option<PlaceKey>,
        /// When.
        at: u64,
    },
    /// At nightfall, one who followed another today (D56, PR 1 ter):
    /// joined its call, went where its answer said, or went to a place it
    /// told; how it came back.
    Followed {
        /// The follower.
        body: BodyId,
        /// The one followed.
        whom: BodyId,
        /// How: [`FOLLOW_JOINED`], [`FOLLOW_ASKED`], [`FOLLOW_TOLD`].
        how: u8,
        /// Back with a day of its need.
        fed: bool,
        /// Back with nothing.
        empty: bool,
        /// What it gathered, in per cent of a day of its need.
        kj_percent: u32,
        /// When.
        at: u64,
    },
    /// A group of the day ended (D56).
    GroupEnded {
        /// Its caller.
        leader: BodyId,
        /// The place it was called for.
        place: PlaceKey,
        /// The place it ended at, when it moved on.
        last_place: PlaceKey,
        /// How many it held, its caller with it.
        members: u32,
        /// How many walked behind the caller's way.
        followed: u32,
        /// How many had what they wanted.
        full: u32,
        /// How many left it on the way (thirst, hunger, out of sight).
        left: u32,
        /// How it ended.
        end: GroupEnd,
        /// Ticks between the caller's arrival at the place and the last
        /// member's: how the group stretched (0 when one arrived alone).
        stretch_ticks: u64,
        /// Members who stood at the place within [`TOGETHER_SECONDS`] of
        /// the caller, the caller not counted: those who walked with it.
        together: u32,
        /// Ticks the caller waited on the way for one left behind.
        waited_ticks: u64,
        /// The tick.
        at: u64,
    },
    /// At the fire, `body` told `places` it saw itself the day before that
    /// a listener did not know (D56): what it found.
    Found {
        /// Who.
        body: BodyId,
        /// How many places.
        places: u32,
        /// The tick.
        at: u64,
    },
    /// `body`'s day as its habit keeps it (D56): what it said at the fire,
    /// what it gathered, the places it found bare.
    Habit {
        /// Who.
        body: BodyId,
        /// What it did: what it said, or `Act::Nothing` with no decision.
        act: Act,
        /// Whether it was at the fire that decided the day.
        decided: bool,
        /// What it gathered that day, in per cent of its own need.
        kj_percent: u32,
        /// Places it went to and found bare.
        bare: u32,
        /// The tick.
        at: u64,
    },
    /// A day of an activity judged (D56): `body`'s confidence in it
    /// after.
    Judged {
        /// Who.
        body: BodyId,
        /// The activity, its index in [`crate::content::ACTIVITIES`].
        activity: u8,
        /// Whether the day succeeded.
        success: bool,
        /// Its confidence in it after, 0 to 100.
        confidence: u8,
        /// The tick.
        at: u64,
    },
    /// `body` copied `way` from its camp-mate `from`.
    Copied {
        /// Who.
        body: BodyId,
        /// From whom.
        from: BodyId,
        /// The way copied.
        way: Way,
        /// The copy lost a step: it landed halfway.
        lost: bool,
        /// Its ways before the copy.
        before: Ways,
        /// Its ways after it.
        after: Ways,
        /// The tick.
        at: u64,
    },
}

/// One body's mind: its ways, its memory of places and what it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mind {
    /// Its ways (D50), its home among them.
    ways: Ways,
    /// Its will, 0 to 100 (D50): how often it tries and how far a step
    /// may go. A placeholder drawn from the world's seed and its id at
    /// adoption, a newborn's the same way (nothing is inherited for now),
    /// until the souls' Willpower comes (D30).
    will: u8,
    /// Its innate will (D50, amended): what its will and its confidences
    /// come back to, season after season (D56).
    innate: u8,
    /// Its innate sociability, 0 to 100 (D53, amended 24 November 2026):
    /// drawn from the seed and its id at adoption, never inherited, fixed
    /// for life; how much company it needs at night.
    sociable: u8,
    /// Thousandths of a point of will beyond `will`, and its failures in
    /// a row (D50, amended: the will tempered).
    will_rest: u16,
    will_failures: u16,
    /// Its confidence in each activity of the content's `temperance`
    /// (D56), in that order: explore, gather, stay.
    confidence: Vec<Confidence>,
    /// Gathered since the last nightfall, in kilojoules.
    gathered_today_kj: u64,
    /// Tiles stepped since the last nightfall.
    walked_today: u64,
    /// Tiles stepped since it last stood in its home's place.
    from_home: u64,
    /// The tick it last drank at.
    drank_at: u64,
    /// What it did since the last nightfall, place by place, at most
    /// [`LIVED_MAX`] places.
    day_places: Vec<DayAct>,
    /// The day closed at nightfall, noted where it lies at the next dawn.
    night: Option<Night>,
    /// Its notes of the places it slept at (D50, D53 amended), at most
    /// [`LIVED_MAX`].
    lived: Vec<Lived>,
    /// Days of distress in a row: hungry at nightfall, or no gather at all.
    distress_days: u32,
    /// Its last days, the oldest first: at most twice the longest
    /// trial's days ([`Trials::longest_days`]).
    days: Vec<Day>,
    /// Its cold was full since the last nightfall: the night froze it.
    cold_night: bool,
    /// The trial under way, if any.
    trial: Option<Trial>,
    /// Its home as it was at the last nightfall, not a second home: the
    /// fire it lies down for tonight, kept while the vigil at dawn may
    /// move the home ([`Ways::home`]). The fire's circle is the place of
    /// it (`place_tiles`, D55 amended 24 November 2026).
    home_at_dusk: Pos,
    /// The kept ways it heard at the fire (D55): accesses to trial
    /// records, the oldest first; at most the content's `fire.heard_max`.
    heard_ways: Vec<Access>,
    /// Its way judged today, to tell at the fire tonight.
    judged: Option<Judged>,
    /// The chunks whose drinking place it found today, to tell tonight.
    water_found: Vec<ChunkPos>,
    /// The places it found bare where a record said food since its last
    /// fire, to deny there (lot 5a): kept apart from its places, which
    /// forget a bare place first.
    denials: Vec<Denial>,
    /// The day it last changed its ways (a trial started, a way copied or
    /// kept): the relative judge leaves its days out for `trial_days`.
    changed_day: Option<u64>,
    /// The day it last weighed copying a camp-mate at lay-down: the
    /// control without the fire ([`Brains::imitate_at_camp`]), once a
    /// night.
    copied_night: Option<u64>,
    /// Where it last looked for water from: it looks again only elsewhere.
    looked_from: Option<Pos>,
    /// Where it last looked for trees and plants bearing food from.
    plants_from: Option<Pos>,
    /// Where its last leg of looking around set off from.
    set_off_from: Option<Pos>,
    /// The drinking places it knows, one a chunk.
    water: BTreeMap<ChunkPos, DrinkingPlace>,
    /// The food it saw, and where.
    food: BTreeMap<ItemId, Pos>,
    /// The places where it saw trees and plants bearing food (D53).
    places: BTreeMap<PlaceKey, Place>,
    /// The trees and plants whose gather the world refused, and the tick
    /// before which each is left alone ([`RETRY_HOURS`]); a few at most.
    left_alone: BTreeMap<(ChunkPos, Source), u64>,
    doing: Doing,
    /// Which thresholds its gauges were past when it last thought:
    /// thirsty, parched, hungry, asleep, collapsed.
    past: (bool, bool, bool, bool, bool),
    /// Something changed: it thinks at the next turn.
    due: bool,
    /// Its walk ended since it last thought: it arrived, or was stopped
    /// for good (blocked, refused). A pause on a straight walk, the goal
    /// kept while the body looks again for a way, is not one
    /// ([`Body::is_pausing`]).
    stopped: bool,
    /// What barred its walk since it last thought: blocked short of its
    /// target with no way seen, or refused at the start (D32).
    blocked: Option<Blockage>,
    /// The places it marked seen, not reached, since its last fire, with
    /// the day: told at the next one like a denial.
    barred_new: Vec<(PlaceKey, u32)>,
    /// The places it could not walk to, and the day it tried: left alone
    /// until the next day, when it may stand elsewhere.
    out_of_reach: BTreeMap<PlaceKey, u64>,
    /// The chunks in sight from `plants_from` whose plants were not
    /// placed yet ([`COLD_CHUNKS_PER_SECOND`]): learnt at the next turns.
    unseen: Vec<ChunkPos>,
    /// The tick it thinks at for the dawn or the nightfall
    /// ([`WAKE_SPREAD_SECONDS`]), unless it thought since.
    wake_at: Option<u64>,
    /// The tick it chose the plant it is going to gather from at: kept
    /// for the content's `thought.plant_choice_hours` (D53).
    chose_at: Option<u64>,
    /// What it said at the fire it will do today (D56), and how far it is.
    plan: Option<Plan>,
    /// What it did its last days (D56), the oldest first, one byte a day,
    /// tagged by activity: the low four bits the act's code ([`Act`]:
    /// alone, called and joined gather at a known place, looked explores,
    /// nothing stays or keeps its own day), bit 4 `HABIT_FOUND` (it found
    /// a place new), bit 5 `HABIT_BARE` (it found one bare), bit 6
    /// `HABIT_FED` (it gathered a day of its need); bit 7 free. At most the
    /// content's `decisions.habit_days` bytes.
    habit: Vec<u8>,
    /// Places it went to and found bare today.
    bare_today: u32,
    /// Whom it believes best at each activity of the content's
    /// `reputation` (D56, PR 1 ter): one at most an activity.
    reputed: Vec<Belief>,
    /// Why it first went home from gathering today, and what it carried
    /// then (a measure, told at nightfall).
    stop: Option<(u8, u64)>,
    /// The dated facts it keeps on its companions as a caller (D56, PR 1
    /// ter), the oldest first: at most the content's
    /// `decisions.facts_kept`.
    companions: Vec<Fact>,
    /// How many times it went looking: the next direction.
    walks: u64,
    /// How many times it thought.
    thoughts: u64,
}

/// A confidence moved by the four rules of temperance (D50 amended,
/// D56), in thousandths of a point (0 to 100,000), with its failures in a
/// row: the will is one, each activity has one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confidence {
    /// Thousandths of a point.
    pub milli: u32,
    /// Failures in a row.
    pub failures: u16,
    /// Successes in a row (D56, PR 1 ter: a streak weighs more).
    pub successes: u16,
}

impl Confidence {
    /// In points, 0 to 100.
    #[must_use]
    pub fn points(self) -> u8 {
        u8::try_from((self.milli / 1000).min(100)).unwrap_or(100)
    }

    /// A failure: the first `bears` in a row cost nothing, each past them
    /// `cost` times its rank past them.
    pub fn fail(&mut self, bears: u32, cost: u32) {
        self.failures = self.failures.saturating_add(1);
        self.successes = 0;
        let past = u32::from(self.failures).saturating_sub(bears);
        self.milli = self.milli.saturating_sub(cost.saturating_mul(past));
    }

    /// A success: `gain` added, the failures in a row forgotten, one
    /// more success in a row.
    pub fn succeed(&mut self, gain: u64) {
        let gain = u32::try_from(gain).unwrap_or(u32::MAX);
        self.milli = self.milli.saturating_add(gain).min(100_000);
        self.failures = 0;
        self.successes = self.successes.saturating_add(1);
    }

    /// Listened to: `gain` added.
    pub fn raise(&mut self, gain: u32) {
        self.milli = self.milli.saturating_add(gain).min(100_000);
    }

    /// A season: `permille` of the gap to `innate` closed.
    pub fn relax(&mut self, innate: u8, permille: u32) {
        let target = i64::from(innate) * 1000;
        let now = i64::from(self.milli);
        let moved = now + (target - now) * i64::from(permille) / 1000;
        self.milli = u32::try_from(moved.clamp(0, 100_000)).unwrap_or(0);
    }
}

/// Whom a body believes best at an activity (D56, PR 1 ter): an access
/// to the record "who knows" in the common memory and its own credit in
/// it as of the record's day, fading a point a day by the content's
/// `reputation.fade_per_day`. Sixteen bytes: the access's twelve, the
/// activity's one, the credit's one, two of padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Belief {
    /// The activity, an index in `content::ACTIVITIES`.
    activity: u8,
    /// The record "who knows": who, at what, renewed when.
    access: Access,
    /// Its credit in it on the record's day, 1 to 100.
    credit: u8,
}

/// A fact a caller keeps on a companion (D56, PR 1 ter): an outing with
/// it on a day, and what it saw; never a level. Twelve bytes: the
/// companion's four, the day's four, the flags' one, three of padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fact {
    /// The companion.
    who: BodyId,
    /// The day of the outing, or of the evening it set it aside.
    day: u32,
    /// [`FACT_WAITED`], [`FACT_LITTLE`], [`FACT_ASIDE`].
    flags: u8,
}

/// A fact: the caller waited for it on the way (D56, PR 1 ter).
const FACT_WAITED: u8 = 0x01;
/// A fact: it came back with less than a day of its need.
const FACT_LITTLE: u8 = 0x02;
/// A fact: the caller told it "not with us tomorrow" that evening; the
/// outings before it are not judged again.
const FACT_ASIDE: u8 = 0x04;

/// It never went home from gathering today ([`News::Stopped`]).
pub const STOP_NONE: u8 = 0;
/// It went home carrying what it wanted.
pub const STOP_FULL: u8 = 1;
/// Its group went home (its caller full, or enough of the group), itself
/// not full.
pub const STOP_GROUP: u8 = 2;
/// Nothing left to gather that it knew of, not full.
pub const STOP_NOTHING: u8 = 3;
/// Night fell.
pub const STOP_NIGHT: u8 = 4;

/// What a caller's review says of a companion (D56, PR 1 ter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Review {
    /// Called first, by name.
    Good,
    /// "Not with us tomorrow".
    Aside,
    /// Nothing said.
    Unsaid,
}

impl Mind {
    fn new(home: Pos) -> Self {
        Self {
            ways: Ways::at(home),
            will: 0,
            innate: 0,
            sociable: 0,
            will_rest: 0,
            will_failures: 0,
            confidence: Vec::new(),
            gathered_today_kj: 0,
            distress_days: 0,
            days: Vec::new(),
            cold_night: false,
            trial: None,
            home_at_dusk: home,
            heard_ways: Vec::new(),
            judged: None,
            water_found: Vec::new(),
            denials: Vec::new(),
            changed_day: None,
            copied_night: None,
            looked_from: None,
            set_off_from: None,
            plants_from: None,
            water: BTreeMap::new(),
            food: BTreeMap::new(),
            places: BTreeMap::new(),
            left_alone: BTreeMap::new(),
            doing: Doing::Nothing,
            past: (false, false, false, false, false),
            due: true,
            stopped: false,
            blocked: None,
            barred_new: Vec::new(),
            out_of_reach: BTreeMap::new(),
            unseen: Vec::new(),
            wake_at: None,
            chose_at: None,
            plan: None,
            habit: Vec::new(),
            bare_today: 0,
            reputed: Vec::new(),
            companions: Vec::new(),
            stop: None,
            drank_at: 0,
            walked_today: 0,
            from_home: 0,
            day_places: Vec::new(),
            night: None,
            lived: Vec::new(),
            walks: 0,
            thoughts: 0,
        }
    }

    /// Something every mind hears at once (the dawn, the nightfall) or
    /// a gauge past a threshold: it thinks in its own tick, `body`'s id
    /// modulo `spread` ticks after `now` ([`WAKE_SPREAD_SECONDS`]).
    fn wake(&mut self, body: BodyId, now: u64, spread: u64) {
        let at = now + u64::from(body.0) % spread;
        if at == now {
            self.due = true;
        } else {
            self.wake_at = Some(self.wake_at.map_or(at, |w| w.min(at)));
        }
    }

    /// A gather of `key` was refused: left alone until `until`. A few are
    /// held at most ([`LEFT_ALONE`]), the soonest free forgotten first.
    fn leave_alone(&mut self, key: (ChunkPos, Source), until: u64) {
        self.left_alone.insert(key, until);
        while self.left_alone.len() > LEFT_ALONE {
            let soonest = *self
                .left_alone
                .iter()
                .min_by_key(|(key, until)| (**until, **key))
                .map(|(key, _)| key)
                .expect("not empty");
            self.left_alone.remove(&soonest);
        }
    }

    /// The places it holds marked seen, not reached (D53, amended 24
    /// November 2026).
    #[must_use]
    pub fn marks_held(&self) -> u32 {
        u32::try_from(self.places.values().filter(|p| p.is_barred()).count()).unwrap_or(u32::MAX)
    }

    /// The drinking places it knows.
    pub fn drinking_places(&self) -> impl Iterator<Item = &DrinkingPlace> {
        self.water.values()
    }

    /// How many times it thought.
    pub fn thoughts(&self) -> u64 {
        self.thoughts
    }

    /// Its camp.
    pub fn home(&self) -> Pos {
        self.ways.home
    }

    /// Its ways (D50).
    pub fn ways(&self) -> &Ways {
        &self.ways
    }

    /// Its will, 0 to 100 (D50).
    pub fn will(&self) -> u8 {
        self.will
    }

    /// Its innate will, 0 to 100 (D50, amended).
    pub fn innate(&self) -> u8 {
        self.innate
    }

    /// Its innate sociability, 0 to 100 (D53, amended 24 November 2026).
    pub fn sociable(&self) -> u8 {
        self.sociable
    }

    /// Its confidence in each activity of [`crate::content::ACTIVITIES`],
    /// 0 to 100 (D56); `None` for one the content does not hold.
    pub fn confidences(&self, temper: &Temperance) -> [Option<u8>; 3] {
        crate::content::ACTIVITIES.map(|name| {
            temper
                .activity(name)
                .and_then(|i| self.confidence.get(i))
                .map(|c| c.points())
        })
    }

    /// How many of the places it knows it believes bearing food now, by
    /// what it saw or heard (a measure, D56: does a tribe go looking when
    /// it knows nothing more to gather?).
    #[must_use]
    pub fn bearing_known(&self, brains: &Brains) -> u32 {
        count_u32(
            self.places
                .values()
                .filter(|p| p.bearing(&brains.food).is_some()),
        )
    }

    /// The trial under way, if any (D50).
    pub fn trial(&self) -> Option<&Trial> {
        self.trial.as_ref()
    }

    /// Its days of distress in a row (D50).
    pub fn distress_days(&self) -> u32 {
        self.distress_days
    }

    /// What its last `days` days closed gave: gathered a day and the days
    /// of each distress, over the days it has when it has fewer; all
    /// zero with none.
    fn lately(&self, days: u32) -> Lately {
        let n = usize::try_from(days)
            .unwrap_or(usize::MAX)
            .min(self.days.len());
        if n == 0 {
            return Lately::default();
        }
        let last = &self.days[self.days.len() - n..];
        let kj: u64 = last.iter().map(|d| d.gathered_kj).sum();
        let count = |f: fn(&Day) -> bool| {
            u32::try_from(last.iter().filter(|d| f(d)).count()).unwrap_or(u32::MAX)
        };
        Lately {
            kj_per_day: kj / u64::try_from(n).expect("fits"),
            thin_days: count(|d| d.thin),
            thirsty_days: count(|d| d.thirsty),
            cold_days: count(|d| d.cold),
            distress_days: count(|d| d.distress()),
        }
    }
}

/// What `body` burns a day, in kilojoules: what its gathered kJ are set
/// against when the fire compares days (a child needs less, D47).
fn need_kj_per_day(rules: &Content, body: &Body) -> u64 {
    body.condition().need_j_per_day(&rules.needs) / 1000
}

/// The judge of `trial` (D50, amended by D55): against the mean of the
/// days it heard at the fire of camp-mates who changed nothing, its own
/// gathered kJ in per cent of its need as theirs; against its own days
/// before only when it heard nobody. Kept if nothing worsened and one
/// thing improved ([`Lately::better_than`]). With what it will tell.
fn judge(
    rules: &Content,
    per_year: u32,
    body: &Body,
    mind: &Mind,
    trial: &Trial,
) -> (bool, Judged) {
    let trials = &rules.needs.trials;
    let days = trials.days_for(trial.way.key(), per_year);
    let during = mind.lately(days);
    let kept = match trial.heard.mean(days) {
        Some(theirs) => {
            let need = need_kj_per_day(rules, body);
            let own = Lately {
                kj_per_day: during.kj_per_day * 100 / need.max(1),
                ..during
            };
            own.better_than(&theirs)
        }
        None => during.better_than(&trial.before_days),
    };
    (
        kept,
        Judged {
            way: trial.way,
            after: mind.ways,
            up: step_up(trial.way, &trial.before, &mind.ways),
            kept,
            before: trial.before_days,
            during,
        },
    )
}

/// Sets `way` in `mind` to its value in `theirs` (D50): with
/// `imitate_loss_percent` a numeric way lands halfway. Whether it lost.
fn copy_way(
    mind: &mut Mind,
    way: Way,
    theirs: &Ways,
    seed: u64,
    keys: &[u64],
    trials: &Trials,
) -> bool {
    let lost = matches!(way, Way::WorthHalfTiles | Way::StockKj)
        && scatter::roll(seed, purpose::TRIAL_COPY_LOSS, keys) % 100
            < u64::from(trials.imitate_loss_percent);
    match (way, lost) {
        (Way::WorthHalfTiles, true) => {
            mind.ways.worth_half_tiles =
                halfway(mind.ways.worth_half_tiles, theirs.worth_half_tiles);
        }
        (Way::StockKj, true) => {
            mind.ways.stock_kj = halfway(mind.ways.stock_kj, theirs.stock_kj);
        }
        _ => way.copy(&mut mind.ways, theirs),
    }
    lost
}

/// How many days a told state of `kind` is worth believing (D55, amended
/// by lot 5a): `fire.told_days_per_hour` for each hour of work one of its
/// plants holds in a usual year (its crop's `grams` over `per_hour_g`),
/// at most `fire.told_days_max` and the days its crop has left in its
/// season, at least one. A kind bearing nothing now is old news.
fn told_days(world: &World, rules: &Content, food: &Food, kind: u16) -> u32 {
    let fire = &rules.needs.fire;
    let Some(bears) = food.now(kind) else {
        return 1;
    };
    let crop = &rules.crops[bears.crop];
    let held_milli = u64::from(crop.grams.1) * 1000 / u64::from(crop.per_hour_g.max(1));
    let days = held_milli * u64::from(fire.told_days_per_hour) / 1000;
    let left = crops::days_left(crop, world.calendar(), world.tick()).unwrap_or(0);
    u32::try_from(days.min(u64::from(fire.told_days_max)).min(left).max(1)).unwrap_or(1)
}

/// How many days the record `record` is worth believing: a place, as long
/// as the longest-lasting kind it tells bearing ([`told_days`]); anything
/// else, [`memory::TRUST_DAYS`].
fn record_days(world: &World, rules: &Content, food: &Food, record: Option<&Record>) -> u32 {
    match record {
        Some(Record::Place { states, .. }) => states
            .iter()
            .filter(|&&(_, bucket)| bucket > 0)
            .map(|&(kind, _)| told_days(world, rules, food, kind))
            .max()
            .unwrap_or(memory::TRUST_DAYS),
        _ => memory::TRUST_DAYS,
    }
}

/// What a listener takes a told state of a kind to be worth an hour: the
/// kind's best, less its credit in the teller.
fn heard_kj(food: &Food, kind: u16, bucket: u8, credit: u8) -> u64 {
    if bucket == 0 {
        return 0;
    }
    food.best_kj
        .get(usize::from(kind))
        .map_or(0, |kj| kj * u64::from(credit) / 100)
}

impl Place {
    /// What it holds, told in the fire's states: each kind and how many of
    /// its plants held food, bucketed.
    fn states(&self, fire: &Fire) -> Vec<(u16, u8)> {
        self.kinds
            .iter()
            .map(|seen| (seen.kind, fire.bucket(u32::from(seen.holding))))
            .collect()
    }

    /// Whether what it saw differs from what the record it holds says, or
    /// it saw food and holds no record yet: something to tell tonight.
    fn changed(&self, fire: &Fire, memory: &Store) -> bool {
        match self.access.and_then(|a| memory.get(a.record)) {
            Some(Record::Place { states, .. }) => *states != self.states(fire),
            Some(_) => false,
            None => self.rumour_kj > 0 || self.kinds.iter().any(|seen| seen.holding > 0),
        }
    }

    /// Seen bare where a rumour said food: denied, not told in a slot.
    fn is_denied(&self, fire: &Fire) -> bool {
        self.rumour_kj > 0 && self.states(fire).iter().all(|&(_, bucket)| bucket == 0)
    }

    /// Seen bare where the record it holds, its own, said food: what an
    /// hour of what the record said was worth, to deny it (lot 5a). A
    /// rumour dispelled is denied from its look ([`Mind::correct`]).
    fn denies(&self, food: &Food, memory: &Store) -> Option<u64> {
        if self.rumour_kj > 0 || self.kinds.iter().any(|seen| seen.holding > 0) {
            return None;
        }
        match self.access.and_then(|a| memory.get(a.record)) {
            Some(Record::Place { states, .. }) => states
                .iter()
                .map(|&(kind, bucket)| heard_kj(food, kind, bucket, 100))
                .max()
                .filter(|&kj| kj > 0),
            _ => None,
        }
    }

    /// What the place is believed worth from `from`: what an hour of its
    /// fullest plant gives, less the farther, of what it saw or, found
    /// bare, of what the record it held said (a rumour dispelled is worth
    /// telling as much as the rumour was).
    fn value_from(
        &self,
        key: PlaceKey,
        food: &Food,
        memory: &Store,
        from: Pos,
        half: u64,
        side: i32,
    ) -> u64 {
        let seen = self
            .kinds
            .iter()
            .filter(|s| s.holding > 0)
            .map(|s| s.best_kj)
            .max();
        let told = match self.access.and_then(|a| memory.get(a.record)) {
            Some(Record::Place { states, .. }) => states
                .iter()
                .map(|&(kind, bucket)| heard_kj(food, kind, bucket, 100))
                .max(),
            _ => None,
        };
        worth(
            seen.unwrap_or(0).max(told.unwrap_or(0)).max(self.rumour_kj),
            far(place_centre(key, side), from),
            half,
        )
    }
}

impl Mind {
    /// What weighs its going looking at the fire, in per cent (D56): its
    /// habit's look outs ([`Mind::look_weight`]); with the will tempered,
    /// that habit times its confidence in exploring against its
    /// confidence in gathering (a multiplier between `habit_floor` % and
    /// three times, never a ban). Never under `habit_floor`.
    fn lean_to_look(&self, rules: &Content) -> u64 {
        let (temper, dec) = (&rules.needs.temperance, &rules.needs.decisions);
        let habit = self.look_weight(dec);
        let held = |name| temper.activity(name).and_then(|i| self.confidence.get(i));
        match (temper.on(), held("explore"), held("gather")) {
            (true, Some(explore), Some(gather)) => {
                let floor = u64::from(dec.habit_floor);
                let lean = ((u64::from(explore.points()) + 5) * 100
                    / (u64::from(gather.points()) + 5))
                    .clamp(floor, 300);
                (habit * lean / 100).max(floor)
            }
            _ => habit,
        }
    }

    /// How much its habit weighs on going looking (D56), in per cent of
    /// the chance: each day it looked and found adds `habit_weight`, each
    /// day it looked for nothing takes it away, never under `habit_floor`.
    fn look_weight(&self, dec: &crate::content::Decisions) -> u64 {
        let looked = Act::Looked.code();
        let (mut found, mut empty) = (0_i64, 0_i64);
        for &day in &self.habit {
            if day & 0x0f == looked {
                if day & HABIT_FOUND == 0 {
                    empty += 1;
                } else {
                    found += 1;
                }
            }
        }
        let weight = 100 + i64::from(dec.habit_weight) * (found - empty);
        u64::try_from(weight.max(i64::from(dec.habit_floor))).unwrap_or(0)
    }

    /// It changed its ways within `trial_days` of `day`, or is under a
    /// trial: the relative judge leaves its days out.
    fn changed_lately(&self, rules: &Content, day: u64) -> bool {
        self.trial.is_some()
            || self
                .changed_day
                .is_some_and(|c| day < c + u64::from(rules.needs.trials.trial_days))
    }

    /// The will acquired (D50, amended): a kept trial raises it by
    /// `will_step_kept`, an abandoned one lowers it by `will_step_lost`,
    /// within 0 to 100. A trait moved by one's own experience.
    fn learn_will(&mut self, rules: &Content, kept: bool) {
        let (trials, temper) = (&rules.needs.trials, &rules.needs.temperance);
        if temper.on() {
            let bears = temper.bears(self.innate);
            self.temper_will(|will| {
                if kept {
                    let failures = u32::from(will.failures);
                    will.succeed(temper.success_milli(bears, failures, temper.kept_percent));
                } else {
                    will.fail(bears, temper.failure_milli);
                }
            });
            return;
        }
        let will = if kept {
            (u32::from(self.will) + trials.will_step_kept).min(100)
        } else {
            u32::from(self.will).saturating_sub(trials.will_step_lost)
        };
        self.will = u8::try_from(will).expect("at most 100");
    }

    /// A day lived kept, twice the longest trial's days (D50).
    fn keep_day(&mut self, trials: &Trials, per_year: u32, day: Day) {
        self.days.push(day);
        let keep = usize::try_from(trials.longest_days(per_year))
            .unwrap_or(usize::MAX)
            .saturating_mul(2);
        if self.days.len() > keep.max(2) {
            self.days.remove(0);
        }
    }

    /// Its innate will set (D50, amended), its will and its confidences
    /// starting from it (D56).
    fn set_innate(&mut self, innate: u8, temper: &Temperance) {
        self.innate = innate;
        self.will = innate;
        self.will_rest = 0;
        self.will_failures = 0;
        let start = Confidence {
            milli: u32::from(innate) * 1000,
            failures: 0,
            successes: 0,
        };
        self.confidence = vec![start; temper.activities.len()];
    }

    /// Its will moved as a [`Confidence`] (D50, amended).
    fn temper_will(&mut self, change: impl FnOnce(&mut Confidence)) {
        let mut will = Confidence {
            milli: u32::from(self.will) * 1000 + u32::from(self.will_rest),
            failures: self.will_failures,
            successes: 0,
        };
        change(&mut will);
        self.will = will.points();
        self.will_rest = u16::try_from(will.milli - u32::from(self.will) * 1000).unwrap_or(0);
        self.will_failures = will.failures;
    }

    /// Listened to (D56): another joined its call (`activity`: gathering)
    /// or copied a way it told (`None`).
    fn listened(&mut self, temper: &Temperance, activity: Option<usize>) {
        if !temper.on() {
            return;
        }
        self.temper_will(|will| will.raise(temper.listened_milli));
        if let Some(c) = activity.and_then(|i| self.confidence.get_mut(i)) {
            c.raise(temper.listened_milli);
        }
    }

    /// A season gone (D56): the will and each confidence close a share of
    /// their gap to the innate will.
    fn relax(&mut self, temper: &Temperance) {
        let (innate, share) = (self.innate, temper.relax_permille);
        self.temper_will(|will| will.relax(innate, share));
        for c in &mut self.confidence {
            c.relax(innate, share);
        }
    }

    /// Its credit today in `belief` (D56, PR 1 ter): its credit on the
    /// record's day, less the content's `fade_per_day` a day since.
    fn credit_today(belief: &Belief, memory: &Store, rep: &Reputation, today: u32) -> u8 {
        let day = memory.get(belief.access.record).map_or(0, Record::day);
        let faded = today.saturating_sub(day).saturating_mul(rep.fade_per_day);
        u8::try_from(u32::from(belief.credit).saturating_sub(faded)).unwrap_or(0)
    }

    /// Whom it believes best at `activity` (an index in
    /// `content::ACTIVITIES`) today, and its credit in it; a credit faded
    /// to nothing believes nobody (D56, PR 1 ter).
    fn believed(
        &self,
        memory: &Store,
        rep: &Reputation,
        activity: u8,
        today: u32,
    ) -> Option<(BodyId, u8)> {
        let belief = self.reputed.iter().find(|b| b.activity == activity)?;
        let Some(Record::Reputed { who, .. }) = memory.get(belief.access.record) else {
            return None;
        };
        let credit = Self::credit_today(belief, memory, rep, today);
        (credit > 0).then_some((*who, credit))
    }

    /// Whom it believes best at each activity today, with its credit
    /// (D56, PR 1 ter): (activity, who, credit).
    #[must_use]
    pub fn reputed(&self, memory: &Store, rep: &Reputation, today: u32) -> Vec<(u8, BodyId, u8)> {
        self.reputed
            .iter()
            .filter_map(|b| {
                self.believed(memory, rep, b.activity, today)
                    .map(|(who, credit)| (b.activity, who, credit))
            })
            .collect()
    }

    /// One it followed, or an account of one (D56, PR 1 ter): `delta`
    /// points to its credit in `who` at `activity`, got from `by` (itself
    /// when it lived it). It keeps only whom it believes best: another
    /// replaces the one it holds only when the gain alone passes its
    /// credit today, and a wear to nothing drops it. The record "who
    /// knows" is renewed today in the common memory. `true` when its
    /// belief changed.
    fn believe(
        &mut self,
        (me, by): (BodyId, BodyId),
        (activity, who): (u8, BodyId),
        delta: i32,
        (memory, rep, today): (&mut Store, &Reputation, u32),
    ) -> bool {
        if who == me || delta == 0 || !rep.holds(usize::from(activity)) {
            return false;
        }
        let at = self.reputed.iter().position(|b| b.activity == activity);
        let held = at.and_then(|i| match memory.get(self.reputed[i].access.record) {
            Some(Record::Reputed { who, .. }) => Some((
                *who,
                Self::credit_today(&self.reputed[i], memory, rep, today),
            )),
            _ => None,
        });
        let credit = match held {
            Some((held, c)) if held == who => (i32::from(c) + delta).clamp(0, 100),
            Some((_, c)) if delta > i32::from(c) => delta.min(100),
            None if delta > 0 => delta.min(100),
            _ => return false,
        };
        if let Some(i) = at {
            let old = self.reputed.remove(i);
            memory.release(old.access.record);
        }
        if credit > 0 {
            let (record, _) = memory.intern(Record::Reputed {
                who,
                activity,
                day: today,
            });
            self.reputed.push(Belief {
                activity,
                access: Access {
                    record,
                    by,
                    day: today,
                },
                credit: u8::try_from(credit).unwrap_or(100),
            });
        }
        true
    }

    /// A day closed in distress or not: hungry or thirsty at nightfall, or
    /// nothing gathered (D50, amended 23 November 2026: thirst too).
    fn distressed(&mut self, distress: bool) {
        self.distress_days = if distress {
            self.distress_days.saturating_add(1)
        } else {
            0
        };
    }

    /// It drank (`grove` false) or gathered (`grove` true) at `place`
    /// (D53, amended 24 November 2026): its `gains` noted there for the
    /// night, with the least that walking there and back cost today.
    fn acted_at(&mut self, place: PlaceKey, grove: bool, saved: u64, gains: [u64; SENSES]) {
        if let Some(act) = self
            .day_places
            .iter_mut()
            .find(|a| a.place == place && a.grove == grove)
        {
            act.saved = act.saved.min(saved);
            for (sum, g) in act.gains.iter_mut().zip(gains) {
                *sum += g;
            }
        } else if self.day_places.len() < LIVED_MAX {
            self.day_places.push(DayAct {
                place,
                grove,
                saved,
                gains,
            });
        }
    }

    /// How many places its notebook holds (D53, amended 23 November 2026).
    #[must_use]
    pub fn lived_places(&self) -> usize {
        self.lived.len()
    }

    /// A night slept at `place` (D50, D53 amended): every note fades by
    /// `keep` per mille, then the night's losses and walk are added there,
    /// and to each place and grove of the day what it gave; a new place
    /// takes the room of the least lived when the notes are full.
    fn note_night(&mut self, place: PlaceKey, night: &Night, keep: u64) {
        for l in &mut self.lived {
            let fade = |v: &mut u64| *v = *v * keep / 1000;
            fade(&mut l.nights);
            fade(&mut l.days);
            fade(&mut l.walk);
            fade(&mut l.saved);
            l.gains.iter_mut().chain(l.losses.iter_mut()).for_each(fade);
        }
        self.lived.retain(|l| l.lived() > 0);
        let l = self.lived_slot(place, false);
        l.nights += 1000;
        l.walk += night.walk * 1000;
        for (sum, v) in l.losses.iter_mut().zip(night.losses) {
            *sum += v * 1000;
        }
        for act in &night.day {
            let l = self.lived_slot(act.place, act.grove);
            if act.grove || act.place != place {
                l.days += 1000;
            }
            l.saved += act.saved * 1000;
            for (sum, g) in l.gains.iter_mut().zip(act.gains) {
                *sum += g * 1000;
            }
        }
    }

    /// The notes of `place` (a grove or not), a new one taking the room
    /// of the least lived when the notes are full.
    fn lived_slot(&mut self, place: PlaceKey, grove: bool) -> &mut Lived {
        let i = if let Some(i) = self
            .lived
            .iter()
            .position(|l| l.place == place && l.grove == grove)
        {
            i
        } else {
            if self.lived.len() >= LIVED_MAX
                && let Some(least) = (0..self.lived.len()).min_by_key(|&i| {
                    (
                        self.lived[i].lived(),
                        self.lived[i].grove,
                        self.lived[i].place,
                    )
                })
            {
                self.lived.remove(least);
            }
            self.lived.push(Lived {
                place,
                grove,
                nights: 0,
                days: 0,
                gains: [0; SENSES],
                losses: [0; SENSES],
                walk: 0,
                saved: 0,
            });
            self.lived.len() - 1
        };
        &mut self.lived[i]
    }

    /// A fact on a companion (D56, PR 1 ter), the oldest forgotten past
    /// `kept`.
    fn note(&mut self, fact: Fact, kept: u32) {
        self.companions.push(fact);
        let kept = usize::try_from(kept).unwrap_or(usize::MAX);
        if self.companions.len() > kept {
            let extra = self.companions.len() - kept;
            self.companions.drain(..extra);
        }
    }

    /// Its review of `who` at the fire (D56, PR 1 ter): its last
    /// `review_outings` outings with it, since it last set it aside;
    /// `review_bad` bad ones among them (waited for, or back with less
    /// than a day of its need) set it aside; its last `review_bad` all
    /// good call it first. Judged over several outings, never on one.
    fn review(&self, who: BodyId, dec: &Decisions) -> Review {
        let most = usize::try_from(dec.review_outings).unwrap_or(usize::MAX);
        let bad_most = usize::try_from(dec.review_bad).unwrap_or(usize::MAX);
        let mut outings: Vec<u8> = Vec::new();
        for fact in self.companions.iter().rev().filter(|f| f.who == who) {
            if fact.flags & FACT_ASIDE != 0 || outings.len() >= most {
                break;
            }
            outings.push(fact.flags);
        }
        let bad = |flags: &u8| flags & (FACT_WAITED | FACT_LITTLE) != 0;
        if outings.iter().filter(|f| bad(f)).count() >= bad_most {
            Review::Aside
        } else if outings.len() >= bad_most && !outings[..bad_most].iter().any(bad) {
            Review::Good
        } else {
            Review::Unsaid
        }
    }

    /// Set aside at the fire (D56, PR 1 ter): a failure of its confidence
    /// in gathering, under the temperance's rules (borne free at first).
    fn set_aside(&mut self, temper: &Temperance) {
        let bears = temper.bears(self.innate);
        if let Some(c) = temper
            .activity("gather")
            .and_then(|i| self.confidence.get_mut(i))
        {
            c.fail(bears, temper.failure_milli);
        }
    }

    /// A day of `activity` judged (D56): its confidence moved by the
    /// four rules; the news of it.
    fn judge_day(
        &mut self,
        temper: &Temperance,
        activity: usize,
        success: bool,
        (body, at): (BodyId, u64),
    ) -> Option<News> {
        let bears = temper.bears(self.innate);
        let percent = temper.activities.get(activity)?.1;
        let c = self.confidence.get_mut(activity)?;
        if success {
            c.succeed(temper.streak_success_milli(percent, u32::from(c.successes)));
        } else {
            c.fail(bears, temper.failure_milli);
        }
        let name = temper.activities.get(activity)?.0.as_str();
        let known = crate::content::ACTIVITIES.iter().position(|n| *n == name)?;
        Some(News::Judged {
            body,
            activity: u8::try_from(known).unwrap_or(u8::MAX),
            success,
            confidence: c.points(),
            at,
        })
    }

    /// Drops every access it holds: the body is gone.
    fn forget_all(&mut self, memory: &mut Store) {
        for place in self.places.values_mut() {
            if let Some(access) = place.access.take() {
                memory.release(access.record);
            }
            if let Some(Mark::Held(access)) = place.barred.take() {
                memory.release(access.record);
            }
        }
        for access in self.heard_ways.drain(..) {
            memory.release(access.record);
        }
        for belief in self.reputed.drain(..) {
            memory.release(belief.access.record);
        }
    }

    /// Forgets the place `key`, dropping its accesses.
    fn forget_place(&mut self, key: PlaceKey, memory: &mut Store) {
        if let Some(place) = self.places.remove(&key) {
            if let Some(access) = place.access {
                memory.release(access.record);
            }
            if let Some(Mark::Held(access)) = place.barred {
                memory.release(access.record);
            }
        }
    }

    /// Holds a heard trial (a kept way, a lesson): at most `heard_max`,
    /// the oldest dropped.
    fn hold_way(&mut self, access: Access, fire: &Fire, memory: &mut Store) {
        self.heard_ways.push(access);
        let keep = usize::try_from(fire.heard_max).unwrap_or(usize::MAX).max(1);
        while self.heard_ways.len() > keep {
            let dropped = self.heard_ways.remove(0);
            memory.release(dropped.record);
        }
    }

    /// What `me` tells at the fire of `camp` tonight (D55), each a record
    /// found or written once in `memory`: its way judged today, the
    /// drinking places it found today, then the places that changed for
    /// it by their value from the camp; `told_per_night` at most. A place
    /// told becomes its own record held (its draft is now in the well).
    /// Apart, its denials (lot 5a): the places it found bare where the
    /// record it held said food, the rumours worth most first,
    /// `denied_per_night` at most, each a bare record; a denial takes no
    /// slot, and one beyond the cap is not told.
    #[allow(clippy::too_many_arguments, reason = "one evening's telling")]
    fn tell(
        &mut self,
        fire: &Fire,
        food: &Food,
        (camp, side, day, now): (Pos, i32, u32, u64),
        me: BodyId,
        memory: &mut Store,
        news: &mut Vec<News>,
    ) -> (Vec<RecordId>, Vec<RecordId>) {
        let mut told = Vec::new();
        let most = usize::try_from(fire.told_per_night).unwrap_or(usize::MAX);
        if let Some(judged) = self.judged.take() {
            let (id, _) = memory.intern(Record::Trial {
                body: me,
                day,
                way: judged.way,
                after: judged.after,
                up: judged.up,
                kept: judged.kept,
                will: self.will,
                before: judged.before,
                during: judged.during,
            });
            told.push(id);
        }
        for chunk in std::mem::take(&mut self.water_found) {
            if told.len() >= most {
                break;
            }
            if let Some(place) = self.water.get(&chunk) {
                let (id, _) = memory.intern(Record::Water {
                    chunk,
                    day,
                    witness: me,
                    stand: place.stand,
                    water: place.water,
                });
                told.push(id);
            }
        }
        let half = self.ways.worth_half_tiles;
        let looked: Vec<PlaceKey> = self
            .places
            .iter()
            .filter(|(_, p)| p.looked_day.is_some() && p.access.is_some_and(|a| !a.is_own(me)))
            .filter(|(_, p)| p.access.zip(p.looked_day).is_some_and(|(a, d)| d > a.day))
            .map(|(k, _)| *k)
            .collect();
        for key in looked {
            self.correct(key, me, fire, food, memory, news, now);
        }
        // Its own record said food and it found the place bare: denied.
        let own_denials: Vec<Denial> = self
            .places
            .iter()
            .filter(|(_, place)| place.looked_day.is_some())
            .filter_map(|(&key, place)| {
                place.denies(food, memory).map(|worth_kj| Denial {
                    place: key,
                    day: place.looked_day.unwrap_or(day),
                    states: place.states(fire),
                    worth_kj,
                })
            })
            .collect();
        self.denials.extend(own_denials);
        let mut changed: Vec<(std::cmp::Reverse<u64>, PlaceKey)> = self
            .places
            .iter()
            .filter(|(_, place)| place.looked_day.is_some() && place.changed(fire, memory))
            .filter(|(_, place)| !place.is_denied(fire) && !place.is_barred())
            .filter(|(key, _)| !self.denials.iter().any(|d| d.place == **key))
            .map(|(&key, place)| {
                (
                    std::cmp::Reverse(place.value_from(key, food, memory, camp, half, side)),
                    key,
                )
            })
            .collect();
        changed.sort_unstable();
        for (_, key) in changed {
            if told.len() >= most {
                break;
            }
            let place = self.places.get_mut(&key).expect("its place");
            let (id, _) = memory.intern(Record::Place {
                place: key,
                day: place.looked_day.unwrap_or(day),
                witness: me,
                states: place.states(fire),
            });
            memory.hold(id);
            place.rumour_kj = 0;
            if let Some(old) = place.access.replace(Access {
                record: id,
                by: me,
                day,
            }) {
                memory.release(old.record);
            }
            told.push(id);
        }
        told.truncate(most);
        let mut denied = self.deny(fire, (day, me), memory);
        denied.extend(self.tell_barred(fire, (day, me), memory));
        (told, denied)
    }

    /// Its marks of seen, not reached since its last fire (D53, amended
    /// 24 November 2026), each written once as a record, the latest
    /// first, `denied_per_night` at most, told apart from the slots like
    /// a denial; one beyond the cap is not told. A mark cleared since is
    /// not told. A mark told becomes its own record held.
    fn tell_barred(
        &mut self,
        fire: &Fire,
        (day, me): (u32, BodyId),
        memory: &mut Store,
    ) -> Vec<RecordId> {
        let mut marks = std::mem::take(&mut self.barred_new);
        marks.sort_by_key(|&(key, day)| (std::cmp::Reverse(day), key));
        marks.dedup_by_key(|m| m.0);
        let most = usize::try_from(fire.denied_per_night).unwrap_or(usize::MAX);
        let drafts: Vec<(PlaceKey, u32)> = marks
            .into_iter()
            .filter(|(key, _)| {
                self.places
                    .get(key)
                    .is_some_and(|p| matches!(p.barred, Some(Mark::Draft(_))))
            })
            .take(most)
            .collect();
        drafts
            .into_iter()
            .map(|(place, barred_day)| {
                let (id, _) = memory.intern(Record::Barred {
                    place,
                    day: barred_day,
                    witness: me,
                });
                memory.hold(id);
                if let Some(held) = self.places.get_mut(&place) {
                    held.barred = Some(Mark::Held(Access {
                        record: id,
                        by: me,
                        day,
                    }));
                }
                id
            })
            .collect()
    }

    /// Its denials of tonight (lot 5a), each written once as a bare
    /// record: the latest look a place, the rumours worth most first,
    /// `denied_per_night` at most; a place seen bearing again since is
    /// told among the places instead. A place it still knows holds the
    /// bare record as its own.
    fn deny(&mut self, fire: &Fire, (day, me): (u32, BodyId), memory: &mut Store) -> Vec<RecordId> {
        let mut denials = std::mem::take(&mut self.denials);
        denials.sort_by_key(|d| (d.place, std::cmp::Reverse(d.day)));
        denials.dedup_by_key(|d| d.place);
        denials.sort_by_key(|d| (std::cmp::Reverse(d.worth_kj), d.place));
        let most = usize::try_from(fire.denied_per_night).unwrap_or(usize::MAX);
        let mut denied = Vec::new();
        for denial in denials {
            let known = self.places.get_mut(&denial.place);
            if known
                .as_ref()
                .is_some_and(|place| place.kinds.iter().any(|seen| seen.holding > 0))
            {
                continue;
            }
            if denied.len() >= most {
                if let Some(place) = known {
                    place.rumour_kj = 0;
                }
                continue;
            }
            let (id, _) = memory.intern(Record::Place {
                place: denial.place,
                day: denial.day,
                witness: me,
                states: denial.states,
            });
            if let Some(place) = known {
                memory.hold(id);
                place.rumour_kj = 0;
                if let Some(old) = place.access.replace(Access {
                    record: id,
                    by: me,
                    day,
                }) {
                    memory.release(old.record);
                }
            }
            denied.push(id);
        }
        denied
    }

    /// `me` hears `record` from `teller` at the fire of `camp` (D55) with
    /// `credit` in it, and gains an access if it keeps it: a place heard
    /// enters its places as what the teller said, worth less by the
    /// credit, if it holds no later record of that place (its own look
    /// included) and the place is worth remembering from the camp; a
    /// water heard joins the drinking places it knows; a kept way heard
    /// joins its heard ways. Whether it kept an access.
    fn hear(
        &mut self,
        fire: &Fire,
        food: &Food,
        (camp, day): (Pos, u32),
        (credit, record, teller): (u8, RecordId, BodyId),
        memory: &mut Store,
        rules: &Content,
    ) -> bool {
        let access = Access {
            record,
            by: teller,
            day,
        };
        match memory.get(record) {
            Some(Record::Place {
                place: key,
                day: rec_day,
                states,
                ..
            }) => {
                let (key, rec_day, states) = (*key, *rec_day, states.clone());
                if let Some(place) = self.places.get(&key) {
                    // The latest by its day, the smaller number at a tie;
                    // its own look later than the record is the latest.
                    let own_later = place.looked_day.is_some_and(|d| d > rec_day);
                    let held_later = place
                        .access
                        .and_then(|a| memory.get(a.record).map(|r| (r.day(), a.record)))
                        .is_some_and(|(d, id)| d > rec_day || (d == rec_day && id <= record));
                    if own_later || held_later {
                        return false;
                    }
                }
                let place = self.places.entry(key).or_default();
                place.kinds = states
                    .iter()
                    .map(|&(kind, bucket)| {
                        let held = u16::try_from(fire.held_of(bucket)).unwrap_or(u16::MAX);
                        Seen {
                            kind,
                            plants: held.max(
                                place
                                    .kinds
                                    .iter()
                                    .find(|s| s.kind == kind)
                                    .map_or(0, |s| s.plants),
                            ),
                            holding: held,
                            best_kj: heard_kj(food, kind, bucket, credit),
                        }
                    })
                    .collect();
                place.spent.clear();
                place.not_before = None;
                if let Some(old) = place.access.replace(access) {
                    memory.release(old.record);
                }
                memory.hold(record);
                // Worth remembering from the camp, or forgotten at once.
                trim_places(food, rules, camp, self, memory);
                self.places
                    .get(&key)
                    .is_some_and(|p| p.access == Some(access))
            }
            Some(Record::Water {
                chunk,
                stand,
                water,
                ..
            }) => {
                let (chunk, place) = (
                    *chunk,
                    DrinkingPlace {
                        stand: *stand,
                        water: *water,
                    },
                );
                if self.water.contains_key(&chunk) {
                    return false;
                }
                self.water.insert(chunk, place);
                trim_water(self, camp);
                false
            }
            Some(Record::Trial { .. }) => {
                // Kept or given up: a kept way to copy or try, a lesson
                // either way (D50, amended).
                memory.hold(record);
                self.hold_way(access, fire, memory);
                true
            }
            _ => false,
        }
    }

    /// The place `key` was looked at by `me` since it heard of it (D55):
    /// corrected at this first look. What it sees as the record says: the
    /// record is its own now. Otherwise the access is dropped, and if the
    /// record told food where none is, the rumour is dispelled
    /// ([`News::Dispelled`]); the place as seen is told tonight.
    #[allow(clippy::too_many_arguments, reason = "one look at one place heard of")]
    fn correct(
        &mut self,
        key: PlaceKey,
        me: BodyId,
        fire: &Fire,
        food: &Food,
        memory: &mut Store,
        news: &mut Vec<News>,
        now: u64,
    ) {
        let Some(place) = self.places.get_mut(&key) else {
            return;
        };
        let Some(access) = place.access.filter(|a| !a.is_own(me)) else {
            return;
        };
        if place.looked_day.is_none() {
            return;
        }
        let Some(Record::Place { states, .. }) = memory.get(access.record) else {
            return;
        };
        if *states == place.states(fire) {
            place.access = Some(Access { by: me, ..access });
            return;
        }
        let told_food = states.iter().any(|&(_, bucket)| bucket > 0);
        let found_food = place.kinds.iter().any(|s| s.holding > 0);
        place.rumour_kj = states
            .iter()
            .map(|&(kind, bucket)| heard_kj(food, kind, bucket, 100))
            .max()
            .unwrap_or(0);
        place.access = None;
        memory.release(access.record);
        if told_food && !found_food {
            self.denials.push(Denial {
                place: key,
                day: place.looked_day.unwrap_or(0),
                states: place.states(fire),
                worth_kj: place.rumour_kj,
            });
            // Away from the fire for days, it keeps the worth most.
            let most = usize::try_from(fire.denied_per_night).unwrap_or(usize::MAX);
            if self.denials.len() > most.saturating_mul(4).max(1) {
                self.denials
                    .sort_by_key(|d| (std::cmp::Reverse(d.worth_kj), d.place));
                self.denials.truncate(most);
            }
            news.push(News::Dispelled {
                body: me,
                place: key,
                record: access.record,
                from: access.by,
                at: now,
            });
        }
    }

    /// The places it knows of (D53): seen for itself, drafts and records
    /// alike, or heard of (D55).
    pub fn places_known(&self) -> impl Iterator<Item = PlaceKey> + '_ {
        self.places.keys().copied()
    }

    /// The accesses it holds to places (D55), by place: its beliefs.
    pub fn believes(&self) -> impl Iterator<Item = (PlaceKey, Access)> + '_ {
        self.places
            .iter()
            .filter_map(|(key, place)| place.access.map(|a| (*key, a)))
    }

    /// The kept ways it heard (D55), the oldest first.
    pub fn heard_ways(&self) -> &[Access] {
        &self.heard_ways
    }
}

/// Past the content's `thought.places_remembered`, the places worth
/// least from `at` are forgotten, their accesses dropped (D53, D55).
fn trim_places(food: &Food, rules: &Content, at: Pos, mind: &mut Mind, memory: &mut Store) {
    let keep = places_remembered(rules);
    if mind.places.len() <= keep {
        return;
    }
    let (half, side) = (mind.ways.worth_half_tiles, place_side(rules));
    let mut ranked: Vec<Rank> = mind
        .places
        .iter()
        .map(|(key, place)| rank_place(food, at, half, side, *key, place))
        .collect();
    ranked.select_nth_unstable(keep);
    let gone: Vec<PlaceKey> = ranked[keep..].iter().map(|(_, key)| *key).collect();
    for key in gone {
        mind.forget_place(key, memory);
    }
}

/// Kept small: the drinking places nearest the camp ([`MEMORY_PLACES`]).
fn trim_water(mind: &mut Mind, camp: Pos) {
    while mind.water.len() > MEMORY_PLACES {
        let farthest = *mind
            .water
            .iter()
            .max_by_key(|(chunk, place)| (far(place.stand, camp), **chunk))
            .map(|(chunk, _)| chunk)
            .expect("not empty");
        mind.water.remove(&farthest);
    }
}

/// How far from the camp the camp's water is looked for, in tiles.
pub const CAMP_WATER_TILES: i32 = 2048;

/// The minds of the bodies the brain pilots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brains {
    minds: BTreeMap<BodyId, Mind>,
    seed: u64,
    /// Each camp's water, found once: its nearest drinking place.
    camp_water: BTreeMap<Pos, Option<DrinkingPlace>>,
    /// The trees and plants bearing food of each chunk a body looked at,
    /// placed once for all the minds: they never change (no felling yet),
    /// and placing a chunk's plants costs a few milliseconds.
    bearers: Placed,
    /// The places to drink at of each chunk a body looked at, found once
    /// for all the minds: the map never changes under a world.
    shores: Lru<ChunkPos, Vec<Shore>>,
    /// The content's kinds bearing food, numbered, and what each bears now.
    food: Food,
    /// The minds whose last look left chunks `unseen`, learnt at the next
    /// turns ([`Brains::learn_unseen`]): the others are not swept.
    pending: BTreeSet<BodyId>,
    /// How much the brains think a game second at most
    /// ([`THOUGHT_WORK_PER_SECOND`]).
    work_per_second: u64,
    /// Work spent past the last turn's share, repaid by the next.
    owed: u64,
    /// The first mind left waiting by the last turn: the next turn starts
    /// with it.
    next: Option<BodyId>,
    /// How many due minds the last turn left waiting.
    waiting: usize,
    /// The work the last turn did.
    work: u64,
    /// What the trials told since the last [`Brains::news`] (D50).
    news: Vec<News>,
    /// The night fell this turn: the minds close their day after the
    /// dead are let go.
    dusk: bool,
    /// The day started this turn: the night's fires are held before the
    /// minds think ([`Brains::vigil`]).
    dawn: bool,
    /// The well (D55): every record, written once.
    memory: Store,
    /// Each camp's hearth: its last `fire.hearth_nights` nights, the
    /// oldest first.
    hearths: BTreeMap<Pos, Vec<RecordId>>,
    /// The will of each body gone, as it was when it died: what its news
    /// is still weighed by (its prestige, D50), as a trial record carries
    /// its teller's.
    gone_wills: BTreeMap<BodyId, u8>,
    /// Who lay down at each camp tonight, by the place of their home at
    /// dusk (`place_tiles`, D55 amended 24 November 2026): the fire's
    /// circle, neighbours a few metres apart sharing one.
    at_fire: BTreeMap<PlaceKey, BTreeSet<BodyId>>,
    /// The groups of the day (D56), by their caller.
    groups: BTreeMap<BodyId, Group>,
    /// The day's groups as they came home, told at the next fire (D56,
    /// PR 1 ter): each caller, its members, how many came back fed, how
    /// many empty.
    accounts: Vec<Account>,
    /// Whether nightfall tells why each stopped gathering
    /// ([`News::Stopped`]): a measure the vivarium asks for (lot 5b).
    measure_stops: bool,
}

/// A group's return, told at the next fire (D56, PR 1 ter): its caller,
/// its members, how many came back fed, how many empty.
type Account = (BodyId, Vec<BodyId>, u32, u32);

/// The content's kinds of trees and plants bearing food, numbered, and
/// what each bears at one tick: a thought weighs hundreds of plants, and
/// reading the content's crops by their names for each of them cost most
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Food {
    /// The kinds, in the order of their first food crop in the content.
    kinds: Vec<String>,
    /// Each kind's number.
    numbers: BTreeMap<String, u16>,
    /// Each kind's food crops, by their place in the content, in its order.
    crops: Vec<Vec<usize>>,
    /// What an hour of each kind gives at best, in kilojoules: of its best
    /// crop ([`kj_per_hour`]).
    best_kj: Vec<u64>,
    /// What an hour gives of each crop of the content, in kilojoules: of
    /// its kind's best crop giving the same.
    crop_kj: Vec<u64>,
    /// Numbered from the content already.
    ready: bool,
    /// The tick `now` is for.
    tick: Option<u64>,
    /// What each kind bears at `tick`, if anything (the first of its food crops in season, in the content's order).
    now: Vec<Option<Bears>>,
}

/// What a kind bears now: the crop, its year, what an hour of it gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bears {
    /// The crop, by its place in the content.
    crop: usize,
    /// The crop's year.
    year: u64,
    /// What an hour of it gives, in kilojoules.
    kj: u64,
}

impl Food {
    /// Numbers the kinds of `rules` bearing food, once.
    fn ready(&mut self, rules: &Content) {
        if self.ready {
            return;
        }
        self.ready = true;
        for (i, crop) in rules.crops.iter().enumerate() {
            if !is_food(rules, &crop.gives) {
                continue;
            }
            if !self.numbers.contains_key(&crop.on) {
                let number = u16::try_from(self.kinds.len()).expect("fewer kinds than that");
                self.numbers.insert(crop.on.clone(), number);
                self.kinds.push(crop.on.clone());
                self.crops.push(Vec::new());
                self.best_kj.push(kj_per_hour(rules, &crop.on, None));
            }
            let kind = usize::from(self.numbers[&crop.on]);
            self.crops[kind].push(i);
        }
        self.crop_kj = rules
            .crops
            .iter()
            .map(|crop| kj_per_hour(rules, &crop.on, Some(&crop.gives)))
            .collect();
    }

    /// What each kind bears at the world's tick, worked out once a tick.
    fn refresh(&mut self, world: &World, rules: &Content) {
        self.ready(rules);
        let tick = world.tick();
        if self.tick == Some(tick.0) {
            return;
        }
        self.tick = Some(tick.0);
        let now = self
            .crops
            .iter()
            .map(|crops| {
                crops.iter().find_map(|&i| {
                    let year = crops::in_season(&rules.crops[i], world.calendar(), tick)?;
                    Some(Bears {
                        crop: i,
                        year,
                        kj: self.crop_kj[i],
                    })
                })
            })
            .collect();
        self.now = now;
    }

    /// What `kind` bears at the tick last [refreshed](Self::refresh).
    fn now(&self, kind: u16) -> Option<Bears> {
        self.now.get(usize::from(kind)).copied().flatten()
    }

    /// `kind`'s id.
    #[cfg(test)]
    fn name(&self, kind: u16) -> &str {
        &self.kinds[usize::from(kind)]
    }
}

/// A chunk's trees and plants bearing a crop as the brains hold them:
/// the world's own, held once with it, and beside each its kind's number
/// ([`Food::numbers`]), [`NOT_FOOD`] for one bearing no food.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    bearers: Arc<[Bearer]>,
    kinds: Box<[u16]>,
}

/// The kind's number of a bearer whose crops are not food.
const NOT_FOOD: u16 = u16::MAX;

impl Held {
    /// The chunk's bearers of food, each with its kind's number.
    fn food(&self) -> impl Iterator<Item = (&Bearer, u16)> {
        self.bearers
            .iter()
            .zip(&self.kinds)
            .filter_map(|(bearer, &kind)| (kind != NOT_FOOD).then_some((bearer, kind)))
    }
}

/// A tile a body may stand on to drink from the water around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shore {
    /// Where to stand.
    stand: Pos,
    /// Which of the tiles around it, [`READING`]'s, are drinkable: a bit each.
    water: u16,
}

/// A tile and its eight neighbours in reading order: the order in which
/// a look over the tiles in sight meets the water around a tile.
const READING: [(i32, i32); 9] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (0, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// The tiles of `chunk` a body may stand on to drink, in reading order.
fn shores_in(map: &Map, rules: &Content, chunk: ChunkPos) -> Vec<Shore> {
    let side = i32::try_from(CHUNK_SIZE).expect("small chunk side");
    let origin = chunk.origin();
    // The chunk and a tile around it: drinkable or not.
    let wide = side + 2;
    let wet: Vec<bool> = (-1..=side)
        .flat_map(|y| (-1..=side).map(move |x| (x, y)))
        .map(|(x, y)| {
            map.get(Pos::new(origin.x + x, origin.y + y))
                .is_some_and(Terrain::is_drinkable)
        })
        .collect();
    let mut shores = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let mut water = 0u16;
            for (bit, (dx, dy)) in READING.iter().enumerate() {
                let i = usize::try_from((y + 1 + dy) * wide + x + 1 + dx).expect("inside");
                if wet[i] {
                    water |= 1 << bit;
                }
            }
            let stand = Pos::new(origin.x + x, origin.y + y);
            if water != 0 && walkable(map, rules, stand) {
                shores.push(Shore { stand, water });
            }
        }
    }
    shores
}

/// The trees and plants bearing food of the chunks the minds looked at,
/// and how many more this turn may place.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    /// Bounded, the least lately used going first: the land the minds
    /// saw grows with their wanderings.
    chunks: Lru<ChunkPos, Held>,
    /// Chunks this turn may still place ([`COLD_CHUNKS_PER_SECOND`]).
    left: u64,
}

/// How many chunks the turn after the tick `now` may place: the
/// second's [`COLD_CHUNKS_PER_SECOND`], spread evenly over its ticks.
fn cold_chunks_at(world: &World, now: u64) -> u64 {
    tick_share(world, now, COLD_CHUNKS_PER_SECOND)
}

/// The tick `now`'s share of `per_second`, spread evenly over the second's
/// ticks: the shares of a second's ticks add up to it.
fn tick_share(world: &World, now: u64, per_second: u64) -> u64 {
    let tps = u64::from(world.calendar().ticks_per_second()).max(1);
    now * per_second / tps - now.saturating_sub(1) * per_second / tps
}

impl Placed {
    fn new() -> Self {
        Self {
            chunks: Lru::new(BEARER_CHUNKS),
            left: u64::MAX,
        }
    }

    /// The trees and plants bearing food of `chunk`; placed now if they
    /// are not yet and the turn allows one more, else `None`.
    fn get(&mut self, world: &World, food: &Food, chunk: ChunkPos) -> Option<&Held> {
        if !self.chunks.contains(&chunk) {
            if self.left == 0 {
                return None;
            }
            self.left -= 1;
            self.chunks
                .insert(chunk, food_bearers_in(world, food, chunk));
        }
        self.chunks.get(&chunk)
    }

    /// Places `chunk`'s now, out of any turn's count.
    fn warm(&mut self, world: &World, food: &Food, chunk: ChunkPos) {
        if !self.chunks.contains(&chunk) {
            self.chunks
                .insert(chunk, food_bearers_in(world, food, chunk));
        }
    }

    /// The trees and plants bearing food of `chunk`, placed now if need be
    /// out of any turn's count: a body arrived at a place it saw, whose
    /// chunk is all but always placed.
    fn held(&mut self, world: &World, food: &Food, chunk: ChunkPos) -> &Held {
        self.warm(world, food, chunk);
        self.chunks.get(&chunk).expect("just placed")
    }
}

/// Is `material` food?
fn is_food(rules: &Content, material: &str) -> bool {
    rules.material(material).is_some_and(|m| m.food.is_some())
}

/// The trees and plants of `chunk` bearing a crop, as the world places
/// them, each with its kind's number among the kinds bearing food.
fn food_bearers_in(world: &World, food: &Food, chunk: ChunkPos) -> Held {
    let bearers = world.bearers_in(chunk);
    let kinds = bearers
        .iter()
        .map(|b: &Bearer| food.numbers.get(&*b.kind).copied().unwrap_or(NOT_FOOD))
        .collect();
    Held { bearers, kinds }
}

/// How far a body standing on `at` sees, in tiles.
fn sight_at(map: &Map, rules: &Content, at: Pos) -> i32 {
    i32::try_from(
        map.get(at)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain)),
    )
    .unwrap_or(1)
}

/// How far a body standing on `at` sees the trees and plants bearing
/// food, in tiles: its sight, and no farther than the content's
/// `thought.plant_sight_tiles` (D53: a bush is told from the grass closer
/// than a lake or a walker; sight 60 on grass read 18 chunks a look, and
/// placing their plants was the brain's first cost).
fn plant_sight_at(map: &Map, rules: &Content, at: Pos) -> i32 {
    sight_at(map, rules, at).min(i32::try_from(rules.needs.thought.plant_sight_tiles).unwrap_or(1))
}

impl Brains {
    /// Asks nightfall to tell why each stopped gathering and what it
    /// carried ([`News::Stopped`]): a measure of the review (lot 5b).
    pub fn measure_stops(&mut self) {
        self.measure_stops = true;
    }

    /// No minds yet; `seed` chooses where they go looking.
    pub fn new(seed: u64) -> Self {
        Self {
            minds: BTreeMap::new(),
            seed,
            camp_water: BTreeMap::new(),
            bearers: Placed::new(),
            shores: Lru::new(BEARER_CHUNKS),
            food: Food::default(),
            pending: BTreeSet::new(),
            work_per_second: THOUGHT_WORK_PER_SECOND,
            owed: 0,
            next: None,
            waiting: 0,
            work: 0,
            news: Vec::new(),
            dusk: false,
            dawn: false,
            memory: Store::new(),
            hearths: BTreeMap::new(),
            gone_wills: BTreeMap::new(),
            at_fire: BTreeMap::new(),
            groups: BTreeMap::new(),
            accounts: Vec::new(),
            measure_stops: false,
        }
    }

    /// The brain pilots `body` from now on: it thinks at the next turn.
    /// Born at the camp, it knows **where the camp drinks**: the drinking
    /// place nearest the camp, within [`CAMP_WATER_TILES`] (the tribe's
    /// knowledge of its land, until knowledge is passed on by people, V3).
    pub fn adopt(&mut self, world: &World, rules: &Content, body: BodyId) {
        self.adopt_at(world, rules, body, world.map().camp());
    }

    /// The same, the body's home being the camp at `home` (a tribe's own
    /// camp, D41: the vivarium's two tribes live on two camps).
    /// The trees and plants in sight of the camp and of the body are
    /// placed now, not at the first turn (2.5 to 3.5 ms a chunk, some
    /// twenty-five chunks).
    pub fn adopt_at(&mut self, world: &World, rules: &Content, body: BodyId, home: Pos) {
        self.food.ready(rules);
        let stands = world.body(body).map(crate::Body::pos);
        for from in std::iter::once(home).chain(stands) {
            let reach = sight_at(world.map(), rules, from) + HOME_TILES;
            let low = Pos::new(from.x - reach, from.y - reach).chunk();
            let high = Pos::new(from.x + reach, from.y + reach).chunk();
            for cy in low.y..=high.y {
                for cx in low.x..=high.x {
                    self.bearers.warm(world, &self.food, ChunkPos::new(cx, cy));
                }
            }
        }
        let water = *self
            .camp_water
            .entry(home)
            .or_insert_with(|| nearest_drinking_place(world.map(), rules, home));
        let now = world.tick().0;
        let first = now + u64::from(body.0) % wake_spread(world);
        let run_seed = self.seed;
        let mind = self.minds.entry(body).or_insert_with(|| {
            // Its first thought too in its own tick: a tribe born at once
            // does not think all in one.
            let mut mind = Mind::new(home);
            // Its innate will (D50, amended 24 November 2026): a skewed
            // draw from the run's seed and its id, a child's the same way
            // (no inheritance).
            let innate = rules
                .needs
                .trials
                .will_at(scatter::roll(run_seed, purpose::TRIAL_WILL, &[u64::from(body.0)]) % 1001);
            mind.set_innate(innate, &rules.needs.temperance);
            // Its sociability (D53, amended 24 November 2026): even from
            // 0 to 100, from the run's seed and its id, never inherited.
            mind.sociable = u8::try_from(
                scatter::roll(run_seed, purpose::SOCIABILITY, &[u64::from(body.0)]) % 101,
            )
            .unwrap_or(100);
            if first > now + 1 {
                mind.due = false;
                mind.wake_at = Some(first);
            }
            mind
        });
        if let Some(place) = water {
            mind.water.insert(place.stand.chunk(), place);
        }
    }

    /// The well (D55): every record the minds hold.
    pub fn memory(&self) -> &Store {
        &self.memory
    }

    /// Each camp's hearth (D55): the records of its last nights, the
    /// oldest first.
    pub fn hearths(&self) -> &BTreeMap<Pos, Vec<RecordId>> {
        &self.hearths
    }

    /// The mind of `body`, if the brain pilots it.
    pub fn mind(&self, body: BodyId) -> Option<&Mind> {
        self.minds.get(&body)
    }

    /// How many due minds the last turn left waiting for the next
    /// ([`THOUGHT_WORK_PER_SECOND`]): none while the crowd fits.
    pub fn waiting(&self) -> usize {
        self.waiting
    }

    /// The work the last turn did ([`THOUGHT_WORK_PER_SECOND`]'s count).
    pub fn work(&self) -> u64 {
        self.work
    }

    /// The id of the kind numbered `kind` (the brains number them), if the
    /// brains numbered it.
    pub fn kind_name(&self, kind: u16) -> Option<&str> {
        self.food.kinds.get(usize::from(kind)).map(String::as_str)
    }

    /// What the trials told since the last call (D50): the vivarium
    /// drains it after each turn.
    pub fn news(&mut self) -> Vec<News> {
        std::mem::take(&mut self.news)
    }

    /// One turn, after a tick of `world` that produced `events`: the
    /// minds something changed for think, as many as the turn's share of
    /// [`THOUGHT_WORK_PER_SECOND`] allows, and what they decide is given
    /// back by their bodies' ids, to be queued for the next tick. Minds
    /// whose body is gone (dead) are forgotten.
    pub fn think(
        &mut self,
        world: &World,
        rules: &Content,
        events: &[Event],
    ) -> Vec<(BodyId, Decision)> {
        self.notice(events, world);
        self.count_gathers(events, world, rules);
        let now = world.tick().0;
        self.bearers.left = cold_chunks_at(world, now);
        let chunks = world.map().chunks_wide() as usize * world.map().chunks_high() as usize;
        self.bearers.chunks.widen(chunks);
        self.shores.widen(chunks);
        self.learn_unseen(world, rules);
        let spread = wake_spread(world);
        // Who is gone, and who is due: the due noted on the way, by their
        // ids, rather than swept again.
        let mut due: Vec<BodyId> = Vec::new();
        let (memory, gone_wills) = (&mut self.memory, &mut self.gone_wills);
        self.minds.retain(|&id, mind| {
            let body = world
                .body(id)
                .filter(|b| b.condition().vitality.points() > 0);
            let Some(body) = body else {
                mind.forget_all(memory);
                gone_wills.insert(id, mind.will);
                return false;
            };
            if body.condition().cold.is_full() {
                mind.cold_night = true;
            }
            let past = pasts(world, rules, id, mind.past.2);
            if past != mind.past {
                // A gauge past a threshold: in its own tick too, as the
                // dawn (a tribe born at once grows hungry at once).
                mind.past = past;
                mind.wake(id, now, spread);
            }
            if mind.wake_at.is_some_and(|tick| now >= tick) {
                mind.due = true;
            }
            if mind.due {
                due.push(id);
            }
            true
        });
        if std::mem::take(&mut self.dusk) {
            self.close_day(world, rules);
        }
        if std::mem::take(&mut self.dawn) {
            self.vigil(world, rules);
        }
        // The due think, by their ids from the first one the last turn
        // left waiting, while the turn's share of work lasts.
        let cap = tick_share(world, now, self.work_per_second);
        let allowance = cap.saturating_sub(self.owed);
        self.owed = self.owed.saturating_sub(cap);
        let from = self.next.take().unwrap_or(BodyId(0));
        let first = due.partition_point(|&id| id < from);
        due.rotate_left(first);
        let mut decisions = Vec::new();
        let mut land = Land {
            groups: &mut self.groups,
            bearers: &mut self.bearers,
            shores: &mut self.shores,
            food: &mut self.food,
            memory: &mut self.memory,
            news: &mut self.news,
        };
        let mut used = 0;
        self.waiting = 0;
        let mut lay_down = Vec::new();
        let mut pending = Vec::new();
        for (i, &id) in due.iter().enumerate() {
            if used >= allowance {
                self.next = Some(id);
                self.waiting = due.len() - i;
                break;
            }
            let (Some(mind), Some(body)) = (self.minds.get_mut(&id), world.body(id)) else {
                continue;
            };
            mind.due = false;
            mind.wake_at = None;
            mind.thoughts += 1;
            land.food.refresh(world, rules);
            used += 1 + look(world, rules, id, body.pos(), mind, &mut land);
            if !mind.unseen.is_empty() {
                pending.push(id);
            }
            if let Some(decision) = decide(world, rules, &mut land, id, mind, self.seed) {
                if decision.why == Why::Night
                    && matches!(decision.intention, Intention::Sleep { .. })
                {
                    lay_down.push(id);
                }
                decisions.push((id, decision));
            }
            (mind.stopped, mind.blocked) = (false, None);
        }
        self.owed += used.saturating_sub(allowance);
        self.work = used;
        self.pending.extend(pending);
        for id in lay_down {
            if rules.needs.fire.on() {
                self.join_fire(world, rules, id);
            } else {
                self.imitate_at_camp(world, rules, id);
            }
        }
        decisions.sort_by_key(|(id, _)| *id);
        decisions
    }

    /// What each body gathered this tick goes to its day's count (D50);
    /// the nightfall is noted for [`Brains::close_day`].
    fn count_gathers(&mut self, events: &[Event], world: &World, rules: &Content) {
        let side = place_side(rules);
        let grove = grove_side(rules);
        let now = world.tick().0;
        let per_day = world.calendar().ticks_per_day().max(1);
        for event in events {
            match event {
                Event::NightFell { .. } => self.dusk = true,
                Event::DayStarted { .. } => self.dawn = true,
                Event::BodyMoved { body, to, .. } => {
                    if let Some(mind) = self.minds.get_mut(body) {
                        mind.walked_today += 1;
                        mind.from_home = if place_of(*to, side) == place_of(mind.ways.home, side) {
                            0
                        } else {
                            mind.from_home + 1
                        };
                    }
                }
                Event::BodyDrank { body, .. } => {
                    if let (Some(mind), Some(b)) = (self.minds.get_mut(body), world.body(*body)) {
                        // Its thirst since its last drink, in per cent of
                        // a day's: a drink at most a day's.
                        let since = now.saturating_sub(std::mem::replace(&mut mind.drank_at, now));
                        let mut gains = [0; SENSES];
                        gains[Sense::Water as usize] = (since * 1000 / per_day).min(1000);
                        let saved = walk_permille(rules, b, mind.from_home * 2);
                        mind.acted_at(place_of(b.pos(), side), false, saved, gains);
                    }
                }
                Event::WorkStopped {
                    body, gives, grams, ..
                } => {
                    if let Some(mind) = self.minds.get_mut(body)
                        && let Some(food) = rules.material(gives).and_then(|m| m.food)
                    {
                        let kj = u64::from(food.energy_kj(*grams));
                        mind.gathered_today_kj += kj;
                        if let Some(b) = world.body(*body)
                            && kj > 0
                        {
                            let mut gains = [0; SENSES];
                            gains[Sense::Food as usize] =
                                kj * 1000 / need_kj_per_day(rules, b).max(1);
                            let saved = walk_permille(rules, b, mind.from_home * 2);
                            mind.acted_at(place_of(b.pos(), grove), true, saved, gains);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The day's groups end at nightfall (D56), each told.
    fn end_groups(&mut self, now: u64, ticks_per_second: u64) {
        let groups = std::mem::take(&mut self.groups);
        let together = TOGETHER_SECONDS * ticks_per_second;
        self.news.extend(
            groups
                .iter()
                .map(|(&leader, g)| g.ended_news(leader, now, together)),
        );
    }

    /// The nightfall closes every mind's day (D50): its gather and every
    /// distress of the day (thin, thirsty, hungry, cold the night before,
    /// bare) go to its days, its distress is counted, a trial under way
    /// long enough is judged on them all, else one may start; then the children
    /// take their mother's home (their father's without one), the carried
    /// infants their carrier's, and each mind notes its home for the
    /// night's camp-mates.
    fn close_day(&mut self, world: &World, rules: &Content) {
        let trials = &rules.needs.trials;
        let per_year = world.calendar().days_per_year();
        let seed = self.seed;
        let day = today(world);
        let now = world.tick().0;
        self.food.refresh(world, rules);
        let (kin, season) = temper_today(world, rules, day);
        let ids: Vec<BodyId> = self.minds.keys().copied().collect();
        let follows = self.follows_today(world, rules, day);
        let gathered: BTreeMap<BodyId, u64> = self
            .minds
            .iter()
            .map(|(&id, m)| (id, m.gathered_today_kj))
            .collect();
        let indirect: BTreeSet<BodyId> = follows
            .iter()
            .filter(|f| f.how != FOLLOW_TOLD && f.fed)
            .map(|f| f.whom)
            .collect();
        for &id in &ids {
            let Some(body) = world.body(id) else {
                continue;
            };
            let mind = self.minds.get_mut(&id).expect("its mind");
            let gathered_kj = std::mem::take(&mut mind.gathered_today_kj);
            if world.is_infant(body, rules) {
                // Carried and nursed: no day of its own yet.
                mind.distress_days = 0;
                mind.cold_night = false;
                continue;
            }
            let c = body.condition();
            let (hungry, thirsty) = (c.hunger.points() >= HUNGRY, c.thirst.points() >= THIRSTY);
            let cold = std::mem::take(&mut mind.cold_night);
            mind.night = Some(night_of(rules, body, mind));
            mind.keep_day(
                trials,
                per_year,
                Day {
                    gathered_kj,
                    thin: c.is_thin(&rules.needs),
                    thirsty,
                    cold,
                    hungry,
                    bare: gathered_kj == 0,
                },
            );
            let (day_of, kin_of) = (
                (day, gathered_kj, now),
                (kin.get(&id).copied(), season, indirect.contains(&id)),
            );
            let told = stopped(world, rules, (id, body), mind, now);
            if self.measure_stops {
                self.news.extend(told);
            }
            close_habit(rules, body, id, mind, day_of, kin_of, &mut self.news);
            mind.distressed(hungry || thirsty || gathered_kj == 0);
            if let Some(trial) = mind.trial
                && day >= trial.started_day + u64::from(trials.days_for(trial.way.key(), per_year))
            {
                let (kept, judged) = judge(rules, per_year, body, mind, &trial);
                mind.learn_will(rules, kept);
                if kept {
                    mind.changed_day = Some(day);
                } else {
                    trial.way.copy(&mut mind.ways, &trial.before);
                }
                mind.trial = None;
                mind.judged = Some(judged);
                self.news.push(News::Tried {
                    body: id,
                    way: trial.way,
                    kept,
                    heard_days: trial.heard.days,
                    heard_nights: trial.heard.nights,
                    at: now,
                });
            } else if mind.trial.is_none() && wants_to_try(trials, seed, (id, day), mind) {
                let child = world.age_milli(body) < 16_000;
                if let Some(news) =
                    start_trial(world, rules, (&self.memory, seed), id, mind, child, day)
                {
                    mind.changed_day = Some(day);
                    self.news.push(news);
                }
            }
        }
        self.review_day(world, rules, day, (&follows, &gathered));
        self.end_groups(now, u64::from(world.calendar().ticks_per_second()));
        self.children_follow(world, &ids);
    }

    /// At dawn, before the night is noted, each grown body feels the
    /// company it had (D53, amended 24 November 2026): the others at its
    /// fire's circle, anyone counting alike, none at no fire (it slept
    /// out, or alone at its camp). What too few left it lacking enters its
    /// night as [`Sense::Company`], by its sociability
    /// ([`crate::content::Company::lacking_permille`]). Only with the fire
    /// lit: without it there is no gathering to feel. It grades the place
    /// it lies at in its notebook, and nothing else: no order, no trigger.
    fn feel_company(&mut self, world: &World, rules: &Content, circles: &[(Pos, Vec<BodyId>)]) {
        if !rules.needs.fire.on() {
            return;
        }
        let company = &rules.needs.company;
        let day = u32::try_from(today(world).saturating_sub(1)).unwrap_or(u32::MAX);
        let mut at: BTreeMap<BodyId, u32> = BTreeMap::new();
        for (_, present) in circles {
            let others = u32::try_from(present.len().saturating_sub(1)).unwrap_or(u32::MAX);
            for &id in present {
                at.insert(id, others);
            }
        }
        for (&id, mind) in &mut self.minds {
            let Some(night) = mind.night.as_mut() else {
                continue;
            };
            let Some(body) = world.body(id) else {
                continue;
            };
            if body.condition().vitality.points() == 0 || world.is_infant(body, rules) {
                continue;
            }
            let others = at.get(&id).copied().unwrap_or(0);
            night.losses[Sense::Company as usize] = company.lacking_permille(mind.sociable, others);
            self.news.push(News::Company {
                body: id,
                day,
                others,
                sociable: mind.sociable,
            });
        }
    }

    /// At dawn each notes the night it just slept where it lies (D50, D53
    /// amended 23 November 2026): its notes of every place fade first.
    fn note_nights(&mut self, world: &World, rules: &Content) {
        let side = place_side(rules);
        let keep = u64::from(rules.needs.trials.lived_keep_permille);
        for (id, mind) in &mut self.minds {
            let Some(mut night) = mind.night.take() else {
                continue;
            };
            let Some(body) = world.body(*id) else {
                continue;
            };
            night.losses[Sense::Warmth as usize] = u64::from(body.condition().cold.points()) * 10;
            mind.note_night(place_of(body.pos(), side), &night, keep);
        }
    }

    /// The children follow (D50, D47): a child takes its mother's home
    /// (its father's without one), a carried infant its carrier's; then
    /// each mind notes its home for the night's camp-mates.
    fn children_follow(&mut self, world: &World, ids: &[BodyId]) {
        let follow: Vec<(BodyId, Pos)> = ids
            .iter()
            .filter_map(|&id| {
                let body = world.body(id)?;
                let leader = match body.carried_by() {
                    Some(carrier) => carrier,
                    None if world.age_milli(body) < 16_000 => body
                        .mother()
                        .filter(|m| self.minds.contains_key(m))
                        .or_else(|| body.father())?,
                    None => return None,
                };
                let home = self.minds.get(&leader)?.ways.home;
                (home != self.minds.get(&id)?.ways.home).then_some((id, home))
            })
            .collect();
        for (id, home) in follow {
            if let Some(mind) = self.minds.get_mut(&id) {
                mind.ways.home = home;
            }
        }
        for mind in self.minds.values_mut() {
            mind.home_at_dusk = mind.ways.home;
        }
    }

    /// Who followed whom today (D56, PR 1 ter), read at nightfall before
    /// the day closes: each member of a group its caller (gathering), each
    /// who asked at the fire the one it asked (the activity it asked it
    /// for), each who went alone to a place another found and told that
    /// finder (exploring); and how each came back: with a day of its need,
    /// or with nothing.
    fn follows_today(&self, world: &World, rules: &Content, day: u64) -> Vec<Follow> {
        let rep = &rules.needs.reputation;
        if !rep.on() && !rules.needs.decisions.reviews() {
            return Vec::new();
        }
        let gather = activity_index("gather");
        let explore = activity_index("explore");
        let mut follows = Vec::new();
        for (&id, mind) in &self.minds {
            let Some(body) = world.body(id) else {
                continue;
            };
            let Some(plan) = mind.plan.filter(|p| u64::from(p.day) == day) else {
                continue;
            };
            let gathered = mind.gathered_today_kj;
            let need = need_kj_per_day(rules, body);
            let (fed, empty) = (gathered >= need, gathered == 0);
            let kj_percent = u32::try_from(gathered * 100 / need.max(1)).unwrap_or(u32::MAX);
            let mut push = |whom: BodyId, how: u8, activity: u8| {
                if whom != id {
                    follows.push(Follow {
                        body: id,
                        whom,
                        how,
                        activity,
                        fed,
                        empty,
                        kj_percent,
                    });
                }
            };
            if plan.act == Act::Joined
                && let Some(leader) = plan.leader
            {
                push(leader, FOLLOW_JOINED, gather);
            }
            if let Some((asked, activity)) = plan.asked {
                push(asked, FOLLOW_ASKED, activity);
            } else if plan.act == Act::Alone
                && let Some(witness) = plan
                    .place
                    .and_then(|key| mind.places.get(&key))
                    .and_then(|place| place.access)
                    .and_then(|a| self.memory.get(a.record))
                    .and_then(Record::witness)
            {
                push(witness, FOLLOW_TOLD, explore);
            }
        }
        follows
    }

    /// The day reviewed at nightfall (D56, PR 1 ter): each follower's
    /// credit in whom it followed moves (back fed: `seen_gain`; back
    /// empty: `wear`), each caller notes a fact on each member of its
    /// group (it waited for it, it came back with less than a day of its
    /// need), and the groups' returns wait for the next fire's accounts.
    fn review_day(
        &mut self,
        world: &World,
        rules: &Content,
        day: u64,
        (follows, gathered): (&[Follow], &BTreeMap<BodyId, u64>),
    ) {
        let rep = &rules.needs.reputation;
        let dec = &rules.needs.decisions;
        let now = world.tick().0;
        let day32 = u32::try_from(day).unwrap_or(u32::MAX);
        for f in follows {
            let delta = if f.fed {
                i32::try_from(rep.seen_gain).unwrap_or(0)
            } else if f.empty {
                -i32::try_from(rep.wear).unwrap_or(0)
            } else {
                0
            };
            if let Some(mind) = self.minds.get_mut(&f.body) {
                mind.believe(
                    (f.body, f.body),
                    (f.activity, f.whom),
                    delta,
                    (&mut self.memory, rep, day32),
                );
            }
            self.news.push(News::Followed {
                body: f.body,
                whom: f.whom,
                how: f.how,
                fed: f.fed,
                empty: f.empty,
                kj_percent: f.kj_percent,
                at: now,
            });
        }
        let mut accounts = Vec::new();
        for (&leader, g) in &self.groups {
            let mut fed = 0;
            let mut empty = 0;
            let mut facts = Vec::new();
            for &m in g.members.iter().skip(1) {
                let (Some(body), Some(&gathered)) = (world.body(m), gathered.get(&m)) else {
                    continue;
                };
                let little = gathered < need_kj_per_day(rules, body);
                fed += u32::from(!little);
                empty += u32::from(gathered == 0);
                let mut flags = 0;
                if g.waited_for.contains(&m) {
                    flags |= FACT_WAITED;
                }
                if little {
                    flags |= FACT_LITTLE;
                }
                facts.push(Fact {
                    who: m,
                    day: day32,
                    flags,
                });
            }
            if dec.reviews()
                && let Some(mind) = self.minds.get_mut(&leader)
            {
                for fact in facts {
                    mind.note(fact, dec.facts_kept);
                }
            }
            let members = g.members.iter().skip(1).copied().collect();
            accounts.push((leader, members, fed, empty));
        }
        if rep.on() {
            self.accounts = accounts;
        }
    }

    /// `id` lies down at its camp for the night (D55): it joins the
    /// fire's circle of its camp if it lies within `imitate_tiles` of its
    /// home at dusk. The circle gathers by the home's place
    /// (`place_tiles`, D55 amended 24 November 2026), not by its tile:
    /// neighbours living a few metres apart share one fire. The circle is
    /// read at dawn ([`Brains::vigil`]); whoever sleeps out, collapses on
    /// the way or never reaches the camp before dawn misses the night.
    fn join_fire(&mut self, world: &World, rules: &Content, id: BodyId) {
        if !rules.needs.fire.on() {
            return;
        }
        let (Some(body), Some(mind)) = (world.body(id), self.minds.get(&id)) else {
            return;
        };
        let here = body.pos();
        let camp = mind.home_at_dusk;
        let reach = i32::try_from(rules.needs.trials.imitate_tiles).unwrap_or(i32::MAX);
        if (here.x - camp.x).abs() > reach || (here.y - camp.y).abs() > reach {
            return;
        }
        self.at_fire
            .entry(place_of(camp, place_side(rules)))
            .or_default()
            .insert(id);
    }

    /// The night's fires, at dawn (D55): at each camp where two or more
    /// lay down, each present in turn (by its id) tells what changed for
    /// it in the day that closed, at most `told_per_night` things: its
    /// way judged, the water it found, then the places whose state it
    /// found other than the record it held (or saw bearing for the first
    /// time), by their value from the camp; each thing told is a record
    /// found or written once. Each present then hears the others: it
    /// keeps the first `heard_by_age` by its credit in them, the younger
    /// record next, and gains an access to each; a place heard replaces
    /// the record it held for that place only if later dated (the smaller
    /// number at a tie); a kept way heard joins its heard ways. A body in
    /// trial adds the days told by the mates who changed nothing. The
    /// night is written as a record of the camp's hearth, which keeps
    /// `hearth_nights` of them. Then each present may copy a way it
    /// heard ([`Brains::imitate`]). Nothing here reads the world but the
    /// calendar: last, every mind's places heard age ([`Brains::age_heard`]).
    fn vigil(&mut self, world: &World, rules: &Content) {
        let fire = &rules.needs.fire;
        let side = place_side(rules);
        let at_fire = std::mem::take(&mut self.at_fire);
        // The circles: the living grown at the place of their home at
        // dusk, the camp a place's middle.
        let circles: Vec<(Pos, Vec<BodyId>)> = at_fire
            .into_iter()
            .map(|(key, circle)| {
                let present = circle
                    .into_iter()
                    .filter(|id| {
                        world.body(*id).is_some_and(|b| {
                            b.condition().vitality.points() > 0 && !world.is_infant(b, rules)
                        }) && self
                            .minds
                            .get(id)
                            .is_some_and(|m| place_of(m.home_at_dusk, side) == key)
                    })
                    .collect();
                (place_centre(key, side), present)
            })
            .collect();
        self.feel_company(world, rules, &circles);
        self.note_nights(world, rules);
        let accounts = std::mem::take(&mut self.accounts);
        if !fire.on() {
            return;
        }
        let dawn = u32::try_from(today(world)).unwrap_or(u32::MAX);
        let day = u32::try_from(today(world).saturating_sub(1)).unwrap_or(u32::MAX);
        let now = world.tick().0;
        for (camp, present) in circles {
            if present.len() < 2 {
                // Nobody to tell: the drafts wait for another night.
                continue;
            }
            let (told, denied, fared) =
                self.fire_tell(world, rules, (camp, side, day, now), &present);
            let full_told = told.iter().any(|&(record, _)| {
                matches!(self.memory.get(record), Some(Record::Place { states, .. })
                    if states.iter().any(|&(_, bucket)| bucket >= 2))
            });
            let learnt = self.fire_hear(world, rules, (camp, day, now), &present, &told, &fared);
            self.fire_accounts(rules, dawn, &present, (&learnt, &accounts));
            self.fire_deny(world, rules, (camp, day, now), &present, &denied);
            // The night, in the hearth: the denials after the slots.
            let (night, _) = self.memory.intern(Record::Night {
                camp,
                day,
                told: told.into_iter().chain(denied).collect(),
                present: present.clone(),
                fared,
            });
            let hearth = self.hearths.entry(camp).or_default();
            hearth.push(night);
            let keep = usize::try_from(fire.hearth_nights).unwrap_or(usize::MAX);
            while hearth.len() > keep {
                let oldest = hearth.remove(0);
                self.memory.release(oldest);
            }
            self.fire_decide(world, rules, (camp, day, now), &present, full_told);
            for &id in &present {
                self.imitate(world, rules, id);
            }
        }
        self.age_heard(world, rules);
    }

    /// Each dawn, after the fire (D55, amended by lot 5a): a place heard
    /// and not yet looked at is believed as its record's credit is today
    /// ([`memory::credit_lasting`]): what it bears goes, picked or out of
    /// season, and the news of it ages as fast. A perception of the
    /// listener's own, nothing of the world read.
    fn age_heard(&mut self, world: &World, rules: &Content) {
        if !rules.needs.fire.on() {
            return;
        }
        let seed = self.seed;
        let today = u32::try_from(today(world)).unwrap_or(u32::MAX);
        self.food.refresh(world, rules);
        let mut wills = self.gone_wills.clone();
        wills.extend(self.minds.iter().map(|(id, m)| (*id, m.will)));
        let (memory, food) = (&self.memory, &self.food);
        // A record's lasting, once a dawn: many hold the same.
        let mut lasting: BTreeMap<RecordId, u32> = BTreeMap::new();
        for (&id, mind) in &mut self.minds {
            let will = mind.will;
            for place in mind.places.values_mut() {
                let Some(access) = place.access.filter(|a| !a.is_own(id)) else {
                    continue;
                };
                let Some(record @ Record::Place { day, states, .. }) = memory.get(access.record)
                else {
                    continue;
                };
                let days = *lasting
                    .entry(access.record)
                    .or_insert_with(|| record_days(world, rules, food, Some(record)));
                let credit = memory::credit_lasting(
                    seed,
                    (id, will),
                    (access.by, wills.get(&access.by).copied().unwrap_or(0)),
                    (*day, access.day, today),
                    days,
                );
                for seen in &mut place.kinds {
                    if let Some(&(_, bucket)) = states.iter().find(|s| s.0 == seen.kind) {
                        seen.best_kj = heard_kj(food, seen.kind, bucket, credit);
                    }
                }
            }
        }
    }

    /// Each present tells, by its id ([`Mind::tell`]), and says how it
    /// fared on the day that closed: what was told, with its teller, the
    /// denials apart (lot 5a), and the days told.
    #[allow(clippy::type_complexity, reason = "three lists of one evening")]
    fn fire_tell(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, side, day, now): (Pos, i32, u32, u64),
        present: &[BodyId],
    ) -> (
        Vec<(RecordId, BodyId)>,
        Vec<(RecordId, BodyId)>,
        Vec<(BodyId, Fared)>,
    ) {
        // What was told, with its teller; the denials; and the days told.
        let fire = &rules.needs.fire;
        let mut told: Vec<(RecordId, BodyId)> = Vec::new();
        let mut denied: Vec<(RecordId, BodyId)> = Vec::new();
        let mut fared: Vec<(BodyId, Fared)> = Vec::new();
        for &id in present {
            let Some(body) = world.body(id) else {
                continue;
            };
            let need = need_kj_per_day(rules, body);
            let mind = self.minds.get_mut(&id).expect("present");
            let (slots, denials) = mind.tell(
                fire,
                &self.food,
                (camp, side, day, now),
                id,
                &mut self.memory,
                &mut self.news,
            );
            told.extend(slots.iter().map(|&record| (record, id)));
            denied.extend(denials.iter().map(|&record| (record, id)));
            for record in slots.into_iter().chain(denials) {
                self.news.push(News::Told {
                    body: id,
                    record,
                    camp,
                    day,
                    at: now,
                });
            }
            if let Some(last) = mind.days.last() {
                fared.push((
                    id,
                    Fared {
                        kj_percent: u32::try_from(last.gathered_kj * 100 / need.max(1))
                            .unwrap_or(u32::MAX),
                        thin: last.thin,
                        thirsty: last.thirsty,
                        cold: last.cold,
                        changed: mind.changed_lately(rules, u64::from(day)),
                    },
                ));
            }
        }
        (told, denied, fared)
    }

    /// The denials of the night (lot 5a), apart from what is heard: each
    /// present but its teller who holds a record of the place hears it
    /// ([`Mind::hear`]): the later record replaces the older, the 226 →
    /// 231 rule, and a place believed bare is forgotten first. A listener
    /// who holds no record of the place hears nothing: a denial adds no
    /// place, and keeps no slot of its attention (`heard_by_age`).
    fn fire_deny(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, day, now): (Pos, u32, u64),
        present: &[BodyId],
        denied: &[(RecordId, BodyId)],
    ) {
        let fire = &rules.needs.fire;
        let seed = self.seed;
        let dawn = u32::try_from(today(world)).unwrap_or(u32::MAX);
        for &(record, teller) in denied {
            if matches!(self.memory.get(record), Some(Record::Barred { .. })) {
                self.hear_barred(present, (record, teller), (day, now));
                continue;
            }
            let Some(&Record::Place {
                place: key,
                day: rec_day,
                ..
            }) = self.memory.get(record)
            else {
                continue;
            };
            let teller_will = self.minds.get(&teller).map_or(0, |m| m.will);
            for &id in present {
                let Some(mind) = self.minds.get_mut(&id) else {
                    continue;
                };
                if id == teller || mind.places.get(&key).is_none_or(|p| p.access.is_none()) {
                    continue;
                }
                let credit = memory::credit_lasting(
                    seed,
                    (id, mind.will),
                    (teller, teller_will),
                    (rec_day, day, dawn),
                    memory::TRUST_DAYS,
                );
                if mind.hear(
                    fire,
                    &self.food,
                    (camp, day),
                    (credit, record, teller),
                    &mut self.memory,
                    rules,
                ) {
                    self.news.push(News::Heard {
                        body: id,
                        record,
                        from: teller,
                        credit,
                        at: now,
                    });
                }
            }
        }
    }

    /// A mark of seen, not reached told at the fire (D53, amended 24
    /// November 2026), heard like a denial: each present but its teller
    /// who holds the place and has not stood on it since the mark's day
    /// marks it too, holding the record heard. A listener who does not
    /// know the place hears nothing: a mark adds no place.
    fn hear_barred(
        &mut self,
        present: &[BodyId],
        (record, teller): (RecordId, BodyId),
        (night, now): (u32, u64),
    ) {
        let Some(&Record::Barred {
            place: key, day, ..
        }) = self.memory.get(record)
        else {
            return;
        };
        for &id in present {
            if id == teller {
                continue;
            }
            let Some(place) = self.minds.get_mut(&id).and_then(|m| m.places.get_mut(&key)) else {
                continue;
            };
            if place.is_barred() || place.stood.is_some_and(|stood| stood >= day) {
                continue;
            }
            self.memory.hold(record);
            place.barred = Some(Mark::Held(Access {
                record,
                by: teller,
                day: night,
            }));
            self.news.push(News::Barred {
                body: id,
                place: key,
                cause: None,
                at: now,
            });
        }
    }

    /// How long each thing told is worth believing, in days: by what it
    /// bears (lot 5a, D55).
    fn told_lasting(
        &mut self,
        world: &World,
        rules: &Content,
        told: &[(RecordId, BodyId)],
    ) -> BTreeMap<RecordId, u32> {
        self.food.refresh(world, rules);
        told.iter()
            .map(|&(record, _)| {
                let days = record_days(world, rules, &self.food, self.memory.get(record));
                (record, days)
            })
            .collect()
    }

    /// Each present hears the others, by its id ([`Mind::hear`]): it
    /// keeps the first `heard_by_age` things told by its credit in them
    /// (judged now and never again, [`memory::credit`], the teller's will
    /// read where the teller stands), the younger record next; in trial,
    /// it adds the days told by the mates who changed nothing.
    fn fire_hear(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, day, now): (Pos, u32, u64),
        present: &[BodyId],
        told: &[(RecordId, BodyId)],
        fared: &[(BodyId, Fared)],
    ) -> Vec<(BodyId, BodyId)> {
        let fire = &rules.needs.fire;
        // Each listener and the finder of a place new to it.
        let mut learnt: Vec<(BodyId, BodyId)> = Vec::new();
        let seed = self.seed;
        // Its age counted to this dawn, the day the listener acts on it.
        let dawn = u32::try_from(today(world)).unwrap_or(u32::MAX);
        let lasting = self.told_lasting(world, rules, told);
        let mut found: BTreeSet<(BodyId, RecordId)> = BTreeSet::new();
        for &id in present {
            let Some(body) = world.body(id) else {
                continue;
            };
            let keep = usize::try_from(fire.heard_at(world.age_milli(body))).unwrap_or(usize::MAX);
            let own_will = self.minds.get(&id).expect("present").will;
            let mut heard: Vec<(
                std::cmp::Reverse<u8>,
                std::cmp::Reverse<u32>,
                RecordId,
                BodyId,
            )> = told
                .iter()
                .filter(|(_, teller)| *teller != id)
                .filter_map(|&(record, teller)| {
                    let rec_day = self.memory.get(record)?.day();
                    let teller_will = self.minds.get(&teller).map_or(0, |m| m.will);
                    let credit = memory::credit_lasting(
                        seed,
                        (id, own_will),
                        (teller, teller_will),
                        (rec_day, day, dawn),
                        lasting[&record],
                    );
                    Some((
                        std::cmp::Reverse(credit),
                        std::cmp::Reverse(rec_day),
                        record,
                        teller,
                    ))
                })
                .collect();
            heard.sort_unstable();
            heard.dedup_by_key(|h| h.2);
            let mind = self.minds.get_mut(&id).expect("present");
            for (credit, _, record, teller) in heard.into_iter().take(keep) {
                // A place new to the listener (D56): what its teller found.
                if let Some(Record::Place { place, witness, .. }) = self.memory.get(record)
                    && *witness == teller
                    && !mind.places.contains_key(place)
                {
                    found.insert((teller, record));
                    learnt.push((id, teller));
                }
                if mind.hear(
                    fire,
                    &self.food,
                    (camp, day),
                    (credit.0, record, teller),
                    &mut self.memory,
                    rules,
                ) {
                    self.news.push(News::Heard {
                        body: id,
                        record,
                        from: teller,
                        credit: credit.0,
                        at: now,
                    });
                }
            }
            if let Some(trial) = mind.trial.as_mut() {
                let mut any = false;
                for &(mate, day_told) in fared {
                    if mate != id && !day_told.changed {
                        trial.heard.add(day_told);
                        any = true;
                    }
                }
                if any {
                    trial.heard.nights += 1;
                }
            }
        }
        // What each teller found (D56): places it saw itself, told
        // tonight, that a listener did not know. Its last day is marked.
        let mut by_teller: BTreeMap<BodyId, u32> = BTreeMap::new();
        for (teller, _) in found {
            *by_teller.entry(teller).or_default() += 1;
        }
        for (teller, places) in by_teller {
            if let Some(mind) = self.minds.get_mut(&teller)
                && rules.needs.decisions.on()
                && let Some(last) = mind.habit.last_mut()
            {
                *last |= HABIT_FOUND;
            }
            self.news.push(News::Found {
                body: teller,
                places,
                at: now,
            });
        }
        learnt
    }

    /// The fire's accounts (D56, PR 1 ter): each listener's credit in the
    /// finder of a place new to it rises by `heard_gain` (exploring); each
    /// present outside a group of yesterday whose caller is here hears how
    /// it came home: half of it fed or more, its credit in the caller rises
    /// by `heard_gain` (gathering); all of it empty, it falls by half the
    /// `wear` for whoever believed that caller best.
    fn fire_accounts(
        &mut self,
        rules: &Content,
        dawn: u32,
        present: &[BodyId],
        (learnt, accounts): (&[(BodyId, BodyId)], &[Account]),
    ) {
        let rep = &rules.needs.reputation;
        if !rep.on() {
            return;
        }
        let heard = i32::try_from(rep.heard_gain).unwrap_or(0);
        let worn = -i32::try_from(rep.wear / 2).unwrap_or(0);
        let (explore, gather) = (activity_index("explore"), activity_index("gather"));
        for &(listener, finder) in learnt {
            if let Some(mind) = self.minds.get_mut(&listener) {
                mind.believe(
                    (listener, finder),
                    (explore, finder),
                    heard,
                    (&mut self.memory, rep, dawn),
                );
            }
        }
        for (leader, members, fed, empty) in accounts {
            if !present.contains(leader) || members.is_empty() {
                continue;
            }
            let n = u32::try_from(members.len()).unwrap_or(u32::MAX);
            let delta = if fed * 2 >= n {
                heard
            } else if *empty == n {
                worn
            } else {
                continue;
            };
            for &id in present {
                if id == *leader || members.contains(&id) {
                    continue;
                }
                if let Some(mind) = self.minds.get_mut(&id) {
                    mind.believe(
                        (id, *leader),
                        (gather, *leader),
                        delta,
                        (&mut self.memory, rep, dawn),
                    );
                }
            }
        }
    }

    /// The evening's third part (D56): each present says what it will do
    /// tomorrow, in the order of its will (the bold speak first, the
    /// smaller id at a tie). One who may go looking draws it by its will,
    /// twice as often when no full place was told tonight, weighed by its
    /// habit; otherwise it weighs its `choices` best places from the camp
    /// at its own pace and takes the first that is either a call with room
    /// left (it joins), or free: it calls when it saw the place itself,
    /// goes alone when it only heard of it; a place taken whole is passed
    /// over. A place's room is what holds there by its memory, divided by
    /// `plants_per_gatherer`, between 1 and `group_max`. A child under
    /// `follows_mother_years` whose mother is present takes her decision.
    /// One who called and nobody joined goes alone. Nothing here reads the
    /// world: each speaks from its memory and its body.
    #[allow(clippy::too_many_lines, reason = "one evening's speech, in order")]
    fn fire_decide(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, day, now): (Pos, u32, u64),
        present: &[BodyId],
        full_told: bool,
    ) {
        let dec = &rules.needs.decisions;
        if !dec.on() {
            return;
        }
        let seed = self.seed;
        let today = day + 1;
        let today64 = u64::from(today);
        let child_years = u64::from(dec.follows_mother_years) * 1000;
        let mother_of = |id: BodyId| -> Option<BodyId> {
            let body = world.body(id)?;
            if world.age_milli(body) >= child_years {
                return None;
            }
            body.mother().filter(|m| present.contains(m))
        };
        let mut speakers: Vec<(std::cmp::Reverse<u8>, BodyId)> = present
            .iter()
            .filter(|&&id| mother_of(id).is_none())
            .map(|&id| {
                (
                    std::cmp::Reverse(self.minds.get(&id).map_or(0, |m| m.will)),
                    id,
                )
            })
            .collect();
        speakers.sort_unstable();
        let rep = &rules.needs.reputation;
        let side = place_side(rules);
        let near = i32::try_from(dec.near_camp_tiles).unwrap_or(i32::MAX);
        // The review (PR 1 ter): who is called first, who set aside.
        let (good_of, aside_by) = self.review_at_fire(world, rules, &speakers, (today, now));
        let mut review = EveningReview {
            aside: count_u32(aside_by.values().flatten()),
            ..EveningReview::default()
        };
        let set_aside = |by: BodyId, id: BodyId| aside_by.get(&by).is_some_and(|s| s.contains(&id));
        let mut questions = if rep.on() { rep.questions_per_night } else { 0 };
        // Each place chosen: its room, how many took it, its caller.
        let mut rooms: BTreeMap<PlaceKey, (u32, u32, Option<BodyId>)> = BTreeMap::new();
        let mut said: BTreeMap<BodyId, (Act, Option<PlaceKey>, Option<BodyId>, u8)> =
            BTreeMap::new();
        // Companions called by name: the call and its place, their room
        // kept for them.
        let mut reserved: BTreeMap<BodyId, (BodyId, PlaceKey)> = BTreeMap::new();
        let mut asked_of: BTreeMap<BodyId, (BodyId, u8)> = BTreeMap::new();
        let mut skipped = 0;
        for &(_, id) in &speakers {
            let (Some(body), Some(mind)) = (world.body(id), self.minds.get(&id)) else {
                continue;
            };
            if carried_kj(world, rules, id) >= mind.ways.stock_kj + need_kj_per_day(rules, body) {
                // It carries what it wants for tomorrow and tomorrow's
                // meals on top: it stays.
                unreserve(&mut reserved, &mut rooms, id);
                review.stocked += 1;
                said.insert(id, (Act::Nothing, None, None, 0));
                continue;
            }
            let pace = u64::from(body.condition().pace_percent(&rules.needs).max(1));
            let percent = dec.look_percent(mind.will) * if full_told { 1 } else { 2 };
            let chance = percent * mind.lean_to_look(rules) / 100;
            if chance > 0
                && scatter::roll(seed, purpose::FIRE_LOOK, &[u64::from(id.0), today64]) % 100
                    < chance
            {
                let octant = least_known(mind, rules, camp, seed, id, today64);
                unreserve(&mut reserved, &mut rooms, id);
                said.insert(id, (Act::Looked, None, None, octant));
                continue;
            }
            // Called first, by name (PR 1 ter): it joins the call.
            if let Some((lead, key)) = reserved.remove(&id) {
                said.insert(id, (Act::Joined, Some(key), Some(lead), 0));
                self.news.push(News::Named {
                    leader: lead,
                    body: id,
                    at: now,
                });
                continue;
            }
            let mut list = choices(&self.food, rules, world, mind, id, camp, pace);
            if review.aside > 0 && aside_by.values().any(|s| s.contains(&id)) && near > 0 {
                // Set aside, it weighs first the places near the camp.
                list.sort_by_key(|(key, _, _)| !within(place_centre(*key, side), camp, near));
            }
            // A reputation weighs (PR 1 ter): the call of whom it believes
            // best at gathering first, with a chance of its credit.
            if rep.join_percent > 0
                && let Some((best, credit)) =
                    mind.believed(&self.memory, rep, activity_index("gather"), today)
                && let Some(i) = list.iter().position(|(key, _, _)| {
                    rooms.get(key).is_some_and(|&(room, taken, caller)| {
                        caller == Some(best) && taken < room && !set_aside(best, id)
                    })
                })
                && i > 0
                && scatter::roll(seed, purpose::FIRE_REPUTE, &[u64::from(id.0), today64]) % 100
                    < u64::from(credit) * u64::from(rep.join_percent) / 100
            {
                let first = list.remove(i);
                list.insert(0, first);
                review.weighed += 1;
            }
            let parents = [body.mother(), body.father()];
            let weighed = !list.is_empty();
            let mut choice = (Act::Nothing, None, None, 0);
            for (key, room, own) in list {
                match rooms.get_mut(&key) {
                    Some((room, taken, caller)) if *taken < *room => {
                        // Set aside by its caller, unless its parent is
                        // with it (PR 1 ter): refused.
                        if let Some(c) = *caller
                            && set_aside(c, id)
                            && !parents.iter().flatten().any(|p| {
                                said.get(p).is_some_and(|s| {
                                    s.2 == Some(c) && matches!(s.0, Act::Called | Act::Joined)
                                })
                            })
                        {
                            review.refused += 1;
                            continue;
                        }
                        *taken += 1;
                        choice = match caller {
                            Some(c) => (Act::Joined, Some(key), Some(*c), 0),
                            None => (Act::Alone, Some(key), None, 0),
                        };
                        break;
                    }
                    Some(_) => skipped += 1,
                    None => {
                        let caller = own.then_some(id);
                        let mut taken = 1;
                        // It calls its good companions first, by name.
                        for &g in good_of.get(&id).into_iter().flatten() {
                            if !own || taken >= room {
                                break;
                            }
                            if said.contains_key(&g) || reserved.contains_key(&g) {
                                continue;
                            }
                            reserved.insert(g, (id, key));
                            taken += 1;
                            review.named += 1;
                        }
                        rooms.insert(key, (room, taken, caller));
                        choice = if own {
                            (Act::Called, Some(key), Some(id), 0)
                        } else {
                            (Act::Alone, Some(key), None, 0)
                        };
                        break;
                    }
                }
            }
            // With no place to go, it asks (PR 1 ter).
            if choice.0 == Act::Nothing
                && questions > 0
                && let Some((whom, activity, place, room)) =
                    self.ask(world, rules, (camp, day, today), (id, present), &rooms)
            {
                questions -= 1;
                review.asked += 1;
                self.news.push(News::Asked {
                    body: id,
                    of: whom,
                    activity,
                    place,
                    at: now,
                });
                if let Some(key) = place {
                    review.answered += 1;
                    choice = match rooms.get_mut(&key) {
                        Some((room, taken, caller))
                            if *taken < *room && !caller.is_some_and(|c| set_aside(c, id)) =>
                        {
                            *taken += 1;
                            match caller {
                                Some(c) => (Act::Joined, Some(key), Some(*c), 0),
                                None => (Act::Alone, Some(key), None, 0),
                            }
                        }
                        Some(_) => {
                            skipped += 1;
                            choice
                        }
                        None => {
                            rooms.insert(key, (room, 1, None));
                            (Act::Alone, Some(key), None, 0)
                        }
                    };
                    if choice.0 != Act::Nothing {
                        asked_of.insert(id, (whom, activity));
                    }
                }
            }
            if choice.0 == Act::Nothing {
                if weighed {
                    review.all_taken += 1;
                } else {
                    review.no_place += 1;
                }
            }
            said.insert(id, choice);
        }
        // The children take their mother's decision.
        for &id in present {
            let Some(mother) = mother_of(id) else {
                continue;
            };
            let choice = match said.get(&mother).copied() {
                Some((Act::Called | Act::Joined, place, leader, _)) => {
                    (Act::Joined, place, leader, 0)
                }
                Some((Act::Alone, place, _, _)) => (Act::Alone, place, None, 0),
                _ => (Act::Nothing, None, None, 0),
            };
            said.insert(id, choice);
        }
        // A call nobody joined is a walk alone; the others are groups.
        let mut members: BTreeMap<BodyId, Vec<BodyId>> = BTreeMap::new();
        for (&id, &(act, _, leader, _)) in &said {
            if let (Act::Called | Act::Joined, Some(leader)) = (act, leader) {
                members.entry(leader).or_default().push(id);
            }
        }
        let mut calls_full = 0;
        for &(room, taken, caller) in rooms.values() {
            if caller.is_some() && taken >= room && room > 1 {
                calls_full += 1;
            }
        }
        for (&id, choice) in &mut said {
            if choice.0 == Act::Called && members.get(&id).is_none_or(|m| m.len() < 2) {
                *choice = (Act::Alone, choice.1, None, 0);
            }
        }
        for (leader, mut team) in members {
            if team.len() < 2 {
                continue;
            }
            let Some(&(_, Some(place), _, _)) = said.get(&leader) else {
                continue;
            };
            team.sort_unstable();
            team.retain(|&m| m != leader);
            team.insert(0, leader);
            // Each who chose to join it listened to the caller (D56): not
            // a child taking its mother's decision.
            let temper = &rules.needs.temperance;
            let joined = team
                .iter()
                .filter(|m| speakers.iter().any(|s| s.1 == **m))
                .count();
            if let Some(mind) = self.minds.get_mut(&leader) {
                for _ in 1..joined {
                    mind.listened(temper, temper.activity("gather"));
                }
            }
            self.groups.insert(
                leader,
                Group {
                    day: today,
                    called: place,
                    place,
                    members: team,
                    taken: BTreeMap::new(),
                    full: BTreeSet::new(),
                    followed: BTreeSet::new(),
                    left: BTreeSet::new(),
                    arrived: BTreeMap::new(),
                    dawn: now,
                    home: false,
                    ended: None,
                    held: false,
                    waited: 0,
                    started: false,
                    waited_for: BTreeSet::new(),
                },
            );
        }
        for (&id, &(act, place, leader, octant)) in &said {
            if let Some(mind) = self.minds.get_mut(&id) {
                mind.plan = Some(Plan {
                    day: today,
                    act,
                    place,
                    leader: if act == Act::Joined || act == Act::Called {
                        leader
                    } else {
                        None
                    },
                    octant,
                    legs: 0,
                    done: false,
                    asked: asked_of.get(&id).copied(),
                });
            }
            self.news.push(News::Decided {
                body: id,
                act,
                place,
                leader: if act == Act::Joined { leader } else { None },
                camp,
                day: today,
                at: now,
            });
        }
        let absent = u32::try_from(
            self.minds
                .iter()
                .filter(|(id, m)| {
                    place_of(m.home_at_dusk, side) == place_of(camp, side)
                        && !present.contains(id)
                        && world.body(**id).is_some_and(|b| {
                            b.condition().vitality.points() > 0 && !world.is_infant(b, rules)
                        })
                })
                .count(),
        )
        .unwrap_or(u32::MAX);
        self.news.push(News::Evening {
            camp,
            by: present[0],
            day: today,
            skipped,
            calls_full,
            absent,
            full_told,
            review,
        });
    }

    /// The evening's review (D56, PR 1 ter): each present who called
    /// before judges each companion present on its last outings with it
    /// ([`Mind::review`]): the good ones it calls first tomorrow, by name,
    /// in the order they speak; the others bad over several outings it
    /// tells "not with us tomorrow", a gesture that costs each a failure
    /// of its confidence in gathering, and a fact. A parent never sets
    /// aside its own child. By caller: whom it calls first, whom it set
    /// aside.
    #[allow(clippy::type_complexity, reason = "the review's two lists")]
    fn review_at_fire(
        &mut self,
        world: &World,
        rules: &Content,
        speakers: &[(std::cmp::Reverse<u8>, BodyId)],
        (today, now): (u32, u64),
    ) -> (
        BTreeMap<BodyId, Vec<BodyId>>,
        BTreeMap<BodyId, BTreeSet<BodyId>>,
    ) {
        let dec = &rules.needs.decisions;
        let mut good_of: BTreeMap<BodyId, Vec<BodyId>> = BTreeMap::new();
        let mut aside_by: BTreeMap<BodyId, BTreeSet<BodyId>> = BTreeMap::new();
        if !dec.reviews() {
            return (good_of, aside_by);
        }
        for &(_, lead) in speakers {
            let Some(mind) = self.minds.get(&lead).filter(|m| !m.companions.is_empty()) else {
                continue;
            };
            for &(_, who) in speakers {
                if who == lead {
                    continue;
                }
                match mind.review(who, dec) {
                    Review::Good if dec.review_holds("names") => {
                        good_of.entry(lead).or_default().push(who);
                    }
                    Review::Aside if dec.review_holds("set_aside") => {
                        let child = world
                            .body(who)
                            .is_some_and(|b| b.mother() == Some(lead) || b.father() == Some(lead));
                        if !child {
                            aside_by.entry(lead).or_default().insert(who);
                        }
                    }
                    Review::Good | Review::Aside | Review::Unsaid => {}
                }
            }
        }
        for (&lead, set) in &aside_by {
            for &who in set {
                if let Some(mind) = self.minds.get_mut(&lead) {
                    let fact = Fact {
                        who,
                        day: today,
                        flags: FACT_ASIDE,
                    };
                    mind.note(fact, dec.facts_kept);
                }
                if dec.review_holds("aside_costs")
                    && let Some(mind) = self.minds.get_mut(&who)
                {
                    mind.set_aside(&rules.needs.temperance);
                }
                self.news.push(News::Aside {
                    leader: lead,
                    body: who,
                    day: today,
                    at: now,
                });
            }
        }
        (good_of, aside_by)
    }

    /// One with no place to go asks whom it believes best "where is
    /// food?" (D56, PR 1 ter): the present it holds the highest credit in,
    /// at any activity of the reputation. That one answers its best place
    /// from the camp it holds a record of and that tonight's calls have
    /// not taken whole (it heard them), a telling to one apart from the
    /// accounts' slots, and the asker hears it at that credit. Whom it
    /// asked, for what, the place if it was answered one it now knows, and
    /// that place's room; `None` when it believes nobody here.
    fn ask(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, day, dawn): (Pos, u32, u32),
        (id, present): (BodyId, &[BodyId]),
        rooms: &BTreeMap<PlaceKey, (u32, u32, Option<BodyId>)>,
    ) -> Option<(BodyId, u8, Option<PlaceKey>, u32)> {
        let rep = &rules.needs.reputation;
        let mind = self.minds.get(&id)?;
        let (activity, whom, credit) = mind
            .reputed
            .iter()
            .filter_map(|b| {
                mind.believed(&self.memory, rep, b.activity, dawn)
                    .map(|(w, c)| (b.activity, w, c))
            })
            .filter(|(_, w, _)| present.contains(w))
            .max_by_key(|&(a, w, c)| (c, std::cmp::Reverse(a), std::cmp::Reverse(w)))?;
        let them = self.minds.get(&whom)?;
        let body = world.body(whom)?;
        let pace = u64::from(body.condition().pace_percent(&rules.needs).max(1));
        let answer = choices(&self.food, rules, world, them, whom, camp, pace)
            .into_iter()
            .filter(|(key, _, _)| rooms.get(key).is_none_or(|r| r.1 < r.0))
            .find_map(|(key, room, _)| {
                them.places.get(&key)?.access.map(|a| (key, room, a.record))
            });
        let Some((key, room, record)) = answer else {
            return Some((whom, activity, None, 0));
        };
        let mind = self.minds.get_mut(&id)?;
        mind.hear(
            &rules.needs.fire,
            &self.food,
            (camp, day),
            (credit, record, whom),
            &mut self.memory,
            rules,
        );
        let knows = mind.places.contains_key(&key);
        Some((whom, activity, knows.then_some(key), room))
    }

    /// At the fire, `id` may copy one way it heard (D50, amended by D55):
    /// among the kept ways it holds an access to that differ from its own,
    /// the one whose teller fared best during its trial (the fewest days
    /// of distress, gathered a day breaking ties, [`Lately::fared`]), its
    /// credit next; the chance is `imitate_percent` scaled by that
    /// teller's will; with `imitate_loss_percent` a numeric way lands
    /// halfway between its own and the teller's. It copies what it heard
    /// said, never what it did not hear: nobody reads another's mind. A
    /// body under a trial copies nothing: the world is judging it.
    /// Copying the home moves the body's home: that is how a camp moves.
    fn imitate(&mut self, world: &World, rules: &Content, id: BodyId) {
        let trials = &rules.needs.trials;
        let day = today(world);
        let seed = self.seed;
        let Some(mind) = self.minds.get(&id) else {
            return;
        };
        if mind.trial.is_some() {
            return;
        }
        let best = mind
            .heard_ways
            .iter()
            .filter_map(|access| {
                let Record::Trial {
                    body: teller,
                    way,
                    after,
                    kept: true,
                    will,
                    during,
                    day: rec_day,
                    ..
                } = self.memory.get(access.record)?
                else {
                    return None;
                };
                if way.same(&mind.ways, after) {
                    return None;
                }
                let credit = memory::credit(
                    seed,
                    (id, mind.will),
                    (*teller, *will),
                    *rec_day,
                    access.day,
                );
                Some((
                    (during.fared(), credit, std::cmp::Reverse(access.record)),
                    (*teller, *will),
                    *way,
                    *after,
                ))
            })
            .max_by_key(|(rank, ..)| *rank);
        let Some((_, (from, prestige), way, theirs)) = best else {
            return;
        };
        let mind = self.minds.get_mut(&id).expect("its mind");
        let before = mind.ways;
        let keys = [u64::from(id.0), day];
        if scatter::roll(seed, purpose::TRIAL_COPY, &keys) % 100
            >= u64::from(trials.imitate_percent) * u64::from(prestige) / 100
        {
            return;
        }
        let lost = copy_way(mind, way, &theirs, seed, &keys, trials);
        mind.changed_day = Some(day);
        self.news.push(News::Copied {
            body: id,
            from,
            way,
            lost,
            before,
            after: mind.ways,
            at: world.tick().0,
        });
        // Its teller was listened to (D56).
        if let Some(teller) = self.minds.get_mut(&from) {
            teller.listened(&rules.needs.temperance, None);
        }
    }

    /// The control without the fire (`--no-fire`, D55): the imitation of
    /// before the fire, kept exactly so that a pair with and without the
    /// fire measures the fire and nothing else. At night at its camp,
    /// `id` may copy one way of the mate who fared best over `trial_days`
    /// (the fewest days of distress, gathered a day breaking ties,
    /// [`Lately::fared`]), better than it did; the chance is
    /// `imitate_percent` scaled by that mate's will. The way is drawn
    /// among those that differ; with `imitate_loss_percent` a numeric way
    /// lands halfway between its own and the mate's. A body under a trial
    /// copies nothing: the world is judging it. Copying the home moves the
    /// body's home: that is how a camp moves. It reads the mate's mind,
    /// which the fire never does.
    fn imitate_at_camp(&mut self, world: &World, rules: &Content, id: BodyId) {
        let trials = &rules.needs.trials;
        let day = today(world);
        let Some(body) = world.body(id) else {
            return;
        };
        let here = body.pos();
        let Some(mind) = self.minds.get(&id) else {
            return;
        };
        if mind.copied_night == Some(day) || mind.trial.is_some() {
            return;
        }
        let camp = mind.ways.home;
        if (here.x - camp.x).abs() > HOME_TILES || (here.y - camp.y).abs() > HOME_TILES {
            return;
        }
        let own = mind.lately(trials.trial_days).fared();
        let dusk = mind.home_at_dusk;
        let reach = i32::try_from(trials.imitate_tiles).unwrap_or(i32::MAX);
        let best = self
            .minds
            .iter()
            .filter(|(mate, m)| **mate != id && m.home_at_dusk == dusk)
            .filter_map(|(&mate, m)| {
                let at = world.body(mate)?.pos();
                ((at.x - here.x).abs() <= reach && (at.y - here.y).abs() <= reach)
                    .then(|| (m.lately(trials.trial_days).fared(), mate, m.ways, m.will))
            })
            .filter(|(fared, ..)| *fared > own)
            .max_by_key(|(fared, mate, ..)| (*fared, std::cmp::Reverse(*mate)));
        let mind = self.minds.get_mut(&id).expect("its mind");
        mind.copied_night = Some(day);
        let Some((_, from, theirs, will)) = best else {
            return;
        };
        let before = mind.ways;
        let seed = self.seed;
        let keys = [u64::from(id.0), day];
        if scatter::roll(seed, purpose::TRIAL_COPY, &keys) % 100
            >= u64::from(trials.imitate_percent) * u64::from(will) / 100
        {
            return;
        }
        let differ: Vec<Way> = Way::ALL
            .into_iter()
            .filter(|way| !way.same(&mind.ways, &theirs))
            .collect();
        if differ.is_empty() {
            return;
        }
        let pick = usize::try_from(scatter::roll(seed, purpose::TRIAL_COPY_WAY, &keys))
            .unwrap_or(0)
            % differ.len();
        let way = differ[pick];
        let lost = copy_way(mind, way, &theirs, seed, &keys, trials);
        mind.changed_day = Some(day);
        self.news.push(News::Copied {
            body: id,
            from,
            way,
            lost,
            before,
            after: mind.ways,
            at: world.tick().0,
        });
    }

    /// The plants of the chunks the minds saw last turns and could not
    /// place then ([`COLD_CHUNKS_PER_SECOND`]), as many as this turn allows,
    /// the minds in turn: learnt without a thought.
    fn learn_unseen(&mut self, world: &World, rules: &Content) {
        if self.pending.is_empty() {
            return;
        }
        let pending: Vec<BodyId> = self.pending.iter().copied().collect();
        for id in pending {
            let Some(mind) = self
                .minds
                .get_mut(&id)
                .filter(|mind| !mind.unseen.is_empty())
            else {
                self.pending.remove(&id);
                continue;
            };
            let Some(at) = mind.plants_from.filter(|_| world.body(id).is_some()) else {
                mind.unseen.clear();
                self.pending.remove(&id);
                continue;
            };
            self.food.refresh(world, rules);
            let sight = plant_sight_at(world.map(), rules, at);
            let before = mind.unseen.len();
            let mut seen = BTreeMap::new();
            mind.unseen.retain(|&chunk| {
                !count_plants(
                    world,
                    rules,
                    &mut self.bearers,
                    &self.food,
                    &mut seen,
                    (chunk, at, sight),
                )
            });
            if mind.unseen.len() < before {
                let day = u32::try_from(today(world)).unwrap_or(u32::MAX);
                remember_places(
                    &self.food,
                    rules,
                    (id, at, day, world.tick().0),
                    mind,
                    seen,
                    &mut self.memory,
                    &mut self.news,
                );
            }
            if mind.unseen.is_empty() {
                self.pending.remove(&id);
            }
        }
    }

    /// Marks the minds the tick's events concern.
    fn notice(&mut self, events: &[Event], world: &World) {
        for event in events {
            let body = match event {
                Event::DayStarted { .. } | Event::NightFell { .. } => {
                    // Each in its tick, by its id: not all at once.
                    let now = world.tick().0;
                    let spread = wake_spread(world);
                    for (id, mind) in &mut self.minds {
                        mind.wake(*id, now, spread);
                    }
                    continue;
                }
                Event::BodyStopped { body, .. }
                | Event::BodyBlocked { body, .. }
                | Event::MoveRefused { body, .. }
                | Event::DrinkRefused { body, .. }
                | Event::ActRefused { body, .. }
                | Event::BodyDrank { body, .. }
                | Event::BodyAte { body, .. }
                | Event::WorkStopped { body, .. }
                | Event::GatherRefused { body, .. }
                | Event::Spoiled {
                    holder: Some(body), ..
                } => *body,
                _ => continue,
            };
            if matches!(event, Event::BodyStopped { .. })
                && world.body(body).is_some_and(Body::is_pausing)
            {
                // A pause on a straight walk, its goal kept: the body
                // looks again for a way at the next tick and goes on by
                // itself. Not a stop, and nothing to think about: read as
                // one, each trunk on a far walk sent it back to its camp
                // (22 November 2026).
                continue;
            }
            if let Some(mind) = self.minds.get_mut(&body) {
                mind.due = true;
                if matches!(
                    event,
                    Event::BodyStopped { .. }
                        | Event::BodyBlocked { .. }
                        | Event::MoveRefused { .. }
                ) {
                    mind.stopped = true;
                }
                if let Event::BodyBlocked { reason, .. } | Event::MoveRefused { reason, .. } = event
                {
                    mind.blocked = Some(*reason);
                }
                if let Event::BodyAte { item, .. } | Event::ActRefused { item, .. } = event {
                    // Eaten whole, refused or gone: looked at again when seen.
                    mind.food.remove(item);
                }
                if let Event::DrinkRefused { at, .. } = event {
                    mind.water.retain(|_, place| place.water != *at);
                }
                if let (Event::GatherRefused { .. }, Doing::Gathering(aim)) = (event, mind.doing) {
                    // Whatever the reason, not again before a later hour.
                    mind.leave_alone(aim.key, world.tick().0 + RETRY_HOURS * hour(world));
                }
            }
        }
    }
}

/// Which thresholds the body's gauges are past: (thirsty, parched,
/// hungry, asleep, collapsed). Collapsed: asleep with its fatigue at
/// [`COLLAPSED`] or more; it thinks again when it has slept under it.
/// Hungry: at [`HUNGRY`] or more, and, `was_hungry` (hungry when it last
/// thought), until its hunger falls under the content's
/// `thought.fed_under` (D53).
fn pasts(
    world: &World,
    rules: &Content,
    id: BodyId,
    was_hungry: bool,
) -> (bool, bool, bool, bool, bool) {
    world
        .body(id)
        .map_or((false, false, false, false, false), |body| {
            let c = body.condition();
            let hunger = c.hunger.points();
            (
                c.thirst.points() >= THIRSTY,
                c.thirst.points() >= PARCHED,
                hunger >= HUNGRY || (was_hungry && hunger >= rules.needs.thought.fed_under),
                c.asleep,
                c.asleep && c.fatigue.points() >= COLLAPSED,
            )
        })
}

/// Can a body stand on `pos`?
fn walkable(map: &Map, rules: &Content, pos: Pos) -> bool {
    map.get(pos)
        .is_some_and(|t| rules.movement.cost(t).is_some())
}

/// A tile and its eight neighbours, itself first, then in a fixed order.
const AROUND: [(i32, i32); 9] = [
    (0, 0),
    (0, -1),
    (1, 0),
    (0, 1),
    (-1, 0),
    (1, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
];

/// The drinkable tile `pos` can drink from (itself first, then its
/// neighbours in a fixed order), if any.
fn water_by(map: &Map, pos: Pos) -> Option<Pos> {
    AROUND
        .iter()
        .map(|(dx, dy)| Pos::new(pos.x + dx, pos.y + dy))
        .find(|p| map.get(*p).is_some_and(Terrain::is_drinkable))
}

/// The drinking place nearest `from`: the squares around it, growing a
/// tile at a time, up to [`CAMP_WATER_TILES`]; on a square, the nearest
/// tile first, ties in reading order.
fn nearest_drinking_place(map: &Map, rules: &Content, from: Pos) -> Option<DrinkingPlace> {
    for r in 0..=CAMP_WATER_TILES {
        let mut best: Option<DrinkingPlace> = None;
        for y in from.y - r..=from.y + r {
            let edge = y == from.y - r || y == from.y + r;
            let xs: Vec<i32> = if edge {
                (from.x - r..=from.x + r).collect()
            } else {
                vec![from.x - r, from.x + r]
            };
            for x in xs {
                let stand = Pos::new(x, y);
                if !walkable(map, rules, stand) {
                    continue;
                }
                if let Some(water) = water_by(map, stand)
                    && best.is_none_or(|b| far(stand, from) < far(b.stand, from))
                {
                    best = Some(DrinkingPlace { stand, water });
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Squared distance between two tiles.
fn far(a: Pos, b: Pos) -> i64 {
    let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    dx * dx + dy * dy
}

/// The body at `at` looks around: the drinking places and the food in
/// sight go into its memory, the food it remembers in sight but no longer
/// sees is forgotten. Sight is the ground's under its feet, a square of
/// that half-side. Gives back how many chunks it read for trees and
/// plants ([`THOUGHT_WORK_PER_SECOND`]).
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)] // one reading, top to bottom, of what it sees or does
fn look(
    world: &World,
    rules: &Content,
    id: BodyId,
    at: Pos,
    mind: &mut Mind,
    land: &mut Land<'_>,
) -> u64 {
    let map = world.map();
    let camp = mind.ways.home;
    let sight = sight_at(map, rules, at);
    let in_sight = |p: Pos| (p.x - at.x).abs() <= sight && (p.y - at.y).abs() <= sight;
    let (low, high) = (
        Pos::new(at.x - sight, at.y - sight).chunk(),
        Pos::new(at.x + sight, at.y + sight).chunk(),
    );
    // Water: the nearest drinking place of each chunk in sight, looked
    // for only when the body stands some steps from where it last looked
    // ([`LOOK_STEP`]; the ground does not change under it), and from the
    // water: each drinkable tile, then the
    // walkable tiles around it. A thought cost four milliseconds when it
    // tried every tile in sight as a place to stand. The places to stand
    // of each chunk are found once for all the minds (`shores_in`): a
    // place is the nearest of its chunk, its water the first drinkable
    // tile around it in reading order, both in sight.
    let new_place = mind
        .looked_from
        .is_none_or(|from| (from.x - at.x).abs().max((from.y - at.y).abs()) >= LOOK_STEP);
    if new_place {
        mind.looked_from = Some(at);
        for cy in low.y..=high.y {
            for cx in low.x..=high.x {
                let chunk = ChunkPos::new(cx, cy);
                if !land.shores.contains(&chunk) {
                    land.shores.insert(chunk, shores_in(map, rules, chunk));
                }
                let mut best: Option<((i64, i32, i32), DrinkingPlace)> = None;
                for shore in land.shores.get(&chunk).into_iter().flatten() {
                    let stand = shore.stand;
                    if !in_sight(stand) {
                        continue;
                    }
                    let key = (far(stand, at), stand.y, stand.x);
                    if best.is_some_and(|(b, _)| b <= key) {
                        continue;
                    }
                    let water = READING
                        .iter()
                        .enumerate()
                        .filter(|(bit, _)| shore.water & (1 << bit) != 0)
                        .map(|(_, (dx, dy))| Pos::new(stand.x + dx, stand.y + dy))
                        .find(|water| in_sight(*water));
                    if let Some(water) = water {
                        best = Some((key, DrinkingPlace { stand, water }));
                    }
                }
                if let Some((key, place)) = best {
                    let mut found = false;
                    mind.water
                        .entry(chunk)
                        .and_modify(|known| {
                            if key < (far(known.stand, at), known.stand.y, known.stand.x) {
                                *known = place;
                            }
                        })
                        .or_insert_with(|| {
                            found = true;
                            place
                        });
                    if found && !mind.water_found.contains(&chunk) {
                        // Told at the fire tonight (D55).
                        mind.water_found.push(chunk);
                    }
                }
            }
        }
    }
    // Food: what lies in the chunks in sight, within sight.
    mind.food.retain(|_, pos| !in_sight(*pos));
    let is_food = |material: &str| is_food(rules, material);
    if rules.spawns.iter().any(|spawn| is_food(&spawn.material)) {
        // The ground lays food of itself: every item of the chunks in sight.
        for cy in low.y..=high.y {
            for cx in low.x..=high.x {
                for item in world.items_in(ChunkPos::new(cx, cy)) {
                    if let Some(pos) = item.pos().filter(|p| in_sight(*p))
                        && is_food(item.material())
                    {
                        mind.food.insert(item.id(), pos);
                    }
                }
            }
        }
    } else {
        // It lays none (dead wood and pebbles, today): only the items
        // bodies laid can be food, and making the ground's own for every
        // chunk in sight would cost a thought forty milliseconds.
        for cy in low.y..=high.y {
            for cx in low.x..=high.x {
                for item in world.laid_in(ChunkPos::new(cx, cy)) {
                    if let Some(pos) = item.pos().filter(|p| in_sight(*p))
                        && is_food(item.material())
                    {
                        mind.food.insert(item.id(), pos);
                    }
                }
            }
        }
    }
    // Trees and plants bearing food, seen again only some steps away:
    // the brains place a chunk's plants once, but a meadow holds
    // thousands to look over. Seen closer than the horizon (D53,
    // [`plant_sight_at`]): the chunks within that sight.
    let moved = mind
        .plants_from
        .is_none_or(|from| (from.x - at.x).abs().max((from.y - at.y).abs()) >= LOOK_STEP);
    let mut read = 0;
    if moved {
        mind.plants_from = Some(at);
        mind.unseen.clear();
        let sight = plant_sight_at(map, rules, at);
        let (low, high) = (
            Pos::new(at.x - sight, at.y - sight).chunk(),
            Pos::new(at.x + sight, at.y + sight).chunk(),
        );
        // The nearest chunks first: a turn may not place them all.
        let mut chunks: Vec<ChunkPos> = (low.y..=high.y)
            .flat_map(|cy| (low.x..=high.x).map(move |cx| ChunkPos::new(cx, cy)))
            .collect();
        read = u64::try_from(chunks.len()).expect("fits");
        let own = at.chunk();
        chunks.sort_by_key(|c| ((c.x - own.x).abs().max((c.y - own.y).abs()), c.y, c.x));
        let mut seen = BTreeMap::new();
        for chunk in chunks {
            if !count_plants(
                world,
                rules,
                land.bearers,
                land.food,
                &mut seen,
                (chunk, at, sight),
            ) {
                mind.unseen.push(chunk);
            }
        }
        let day = u32::try_from(today(world)).unwrap_or(u32::MAX);
        let now = world.tick().0;
        remember_places(
            land.food,
            rules,
            (id, at, day, now),
            mind,
            seen,
            land.memory,
            land.news,
        );
    }
    // Kept small: the nearest to the camp.
    trim_water(mind, camp);
    while mind.food.len() > MEMORY_PLACES {
        let farthest = *mind
            .food
            .iter()
            .max_by_key(|(id, pos)| (far(**pos, camp), **id))
            .map(|(id, _)| id)
            .expect("not empty");
        mind.food.remove(&farthest);
    }
    read
}

/// What a body at `at` seeing `sight` tiles sees of the trees and plants
/// bearing food of `chunk`, counted by place and by kind into `seen`
/// (D53: a body remembers the corners, not the bushes), and of those in
/// season how many hold food worth a step now ([`Seen::holding`]: one
/// sees the fruit on a bush); `false` if the chunk's could not be placed
/// this turn.
fn count_plants(
    world: &World,
    rules: &Content,
    bearers: &mut Placed,
    food: &Food,
    seen: &mut BTreeMap<PlaceKey, Vec<Seen>>,
    (chunk, at, sight): (ChunkPos, Pos, i32),
) -> bool {
    let side = place_side(rules);
    let Some(placed) = bearers.get(world, food, chunk) else {
        return false;
    };
    for (bearer, kind) in placed.food() {
        let tile = bearer.at.tile();
        if (tile.x - at.x).abs() > sight || (tile.y - at.y).abs() > sight {
            continue;
        }
        let kj = food.now(kind).and_then(|bears| {
            let (_, left_g) = world.ripe_of_crop(chunk, bearer.source, bearer.at, bears.crop)?;
            kj_of_an_hour(bears.kj, rules.crops[bears.crop].per_hour_g, left_g)
        });
        let kinds = seen.entry(place_of(tile, side)).or_default();
        match kinds.binary_search_by_key(&kind, |k| k.kind) {
            Ok(i) => {
                kinds[i].plants = kinds[i].plants.saturating_add(1);
                kinds[i].holding = kinds[i].holding.saturating_add(u16::from(kj.is_some()));
                kinds[i].best_kj = kinds[i].best_kj.max(kj.unwrap_or(0));
            }
            Err(i) => kinds.insert(
                i,
                Seen {
                    kind,
                    plants: 1,
                    holding: u16::from(kj.is_some()),
                    best_kj: kj.unwrap_or(0),
                },
            ),
        }
    }
    true
}

/// A place's rank in a memory ([`remember_places`]): the lower, the
/// better kept; total, the key breaking ties.
type Rank = (std::cmp::Reverse<(bool, u64)>, PlaceKey);

/// The rank of the place `key`, for a body at `at` whose way halves a
/// plant's worth at `half_tiles` tiles: what bears now comes first, by
/// what an hour of its best kind gives, less the farther the place; then
/// the rest by their best crop: a fruit corner out of season does not
/// crowd out the spring's leaves.
fn rank_place(
    food: &Food,
    at: Pos,
    half_tiles: u64,
    side: i32,
    key: PlaceKey,
    place: &Place,
) -> Rank {
    let (now, kj) = place.rank_kj(food);
    (
        std::cmp::Reverse((now, worth(kj, far(place_centre(key, side), at), half_tiles))),
        key,
    )
}

/// The places `seen` from `at` by `id` on `day` (what and how many of
/// each kind) join the mind's; past the content's
/// `thought.places_remembered`, the places worth least from `at` are
/// forgotten (D53). A place heard of is corrected at this first look
/// (D55): the look is now its own, and if it found none of the food the
/// record told, the rumour is dispelled ([`News::Dispelled`]) and the
/// access dropped; the bare place is told tonight.
fn remember_places(
    food: &Food,
    rules: &Content,
    (id, at, day, now): (BodyId, Pos, u32, u64),
    mind: &mut Mind,
    seen: BTreeMap<PlaceKey, Vec<Seen>>,
    memory: &mut Store,
    news: &mut Vec<News>,
) {
    for (key, kinds) in seen {
        let place = mind.places.entry(key).or_default();
        for seen in kinds {
            place.saw(seen.kind, seen.plants, seen.holding, seen.best_kj);
        }
        place.looked_day = Some(day);
        mind.correct(key, id, &rules.needs.fire, food, memory, news, now);
    }
    trim_places(food, rules, at, mind, memory);
}

/// The minds' shared land, borrowed for a turn: the plants placed, the
/// places to drink found, what each kind bears now.
struct Land<'a> {
    /// The groups of the day (D56).
    groups: &'a mut BTreeMap<BodyId, Group>,
    bearers: &'a mut Placed,
    shores: &'a mut Lru<ChunkPos, Vec<Shore>>,
    food: &'a mut Food,
    /// The well (D55): the accesses a look confirms or drops.
    memory: &'a mut Store,
    /// What the looks tell (a rumour dispelled).
    news: &'a mut Vec<News>,
}

/// What the body decides, now that it looked.
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)] // one reading, top to bottom, of what it sees or does
fn decide(
    world: &World,
    rules: &Content,
    land: &mut Land<'_>,
    id: BodyId,
    mind: &mut Mind,
    seed: u64,
) -> Option<Decision> {
    let Land {
        bearers,
        food,
        groups,
        memory,
        news,
        ..
    } = land;
    let food: &Food = food;
    let body = world.body(id)?;
    // An infant thinks nothing yet: it is carried and nursed (lot 3 B).
    if world.is_infant(body, rules) {
        return None;
    }
    let c = body.condition();
    let here = body.pos();
    stand_on(world, rules, (id, here), mind, (memory, news));
    let map = world.map();
    let camp = mind.ways.home;
    let decision = |intention, why| Some(Decision { intention, why });
    let (thirsty, parched, hungry, asleep, collapsed) = pasts(world, rules, id, mind.past.2);
    // The world refuses a gather to a body whose load is full.
    let can_carry = body.load_g() < body.capacity_g(&rules.movement);
    let night = world.is_night(rules);
    let now = world.tick().0;
    if let Doing::Gathering(_) = mind.doing {
        if body.work().is_some() {
            // At work: it goes on unless thirst, hunger or night calls.
            if !thirsty && !hungry && !night {
                return None;
            }
        } else {
            // The work ended, or was refused: the place is read again at
            // the next choice, a plant picked bare skipped then.
            mind.doing = Doing::Nothing;
        }
    }
    if let Doing::ToGather(aim) = mind.doing
        && mind.stopped
    {
        stopped_short(world, rules, id, mind, aim, news);
    }
    if let Doing::ToPlace(place) = mind.doing
        && mind.stopped
        && !at_place(here, place, place_side(rules))
    {
        // Stopped short of the place: the way there is not one it knows.
        // Left for the day, or it would try every corner beyond the same
        // river one after the other. Barred by water or rock (D32), the
        // place is seen, not reached, until one stands on it; stopped by
        // anything else, only the day.
        mind.doing = Doing::Nothing;
        leave_for_today(world, mind, place);
        if let Some(cause @ (Blockage::Water | Blockage::Rock)) = mind.blocked {
            bar(world, (id, place, cause), mind, news);
        }
    }
    // What to gather, chosen once a thought: hunger and the day's
    // gathering ask the same question of the same memory. And once an
    // hour (D53, [`goal`]): what it is going to is kept while it holds,
    // with whether the choice is fresh.
    let mut chosen: Option<(Option<Goal<'_>>, bool)> = None;
    let mut to_gather =
        |mind: &mut Mind, bearers: &mut Placed, groups: &mut BTreeMap<BodyId, Group>| {
            *chosen.get_or_insert_with(|| {
                planned(world, rules, food, bearers, groups, (id, here), mind, now)
                    .unwrap_or_else(|| goal(world, rules, food, bearers, here, mind, now))
            })
        };
    // Asleep, it sleeps on unless parched (D45: a parched body does not
    // sleep), and even parched while it lies collapsed of fatigue: a walk
    // would wake it for a tick and lay it down again.
    if asleep && (!parched || collapsed) {
        mind.doing = Doing::Nothing;
        return None;
    }
    if thirsty {
        // At a drinking place: drink.
        if let Some(water) = water_by(map, here) {
            mind.doing = Doing::Nothing;
            return decision(
                Intention::Drink {
                    body: id,
                    at: water,
                },
                Why::Thirst,
            );
        }
        let nearest = mind
            .water
            .values()
            .min_by_key(|place| (far(place.stand, here), place.stand.y, place.stand.x))
            .copied();
        if let Some(place) = nearest {
            let at_camp =
                (here.x - camp.x).abs() <= HOME_TILES && (here.y - camp.y).abs() <= HOME_TILES;
            if mind.doing == Doing::ToDrink(place) && mind.stopped {
                if at_camp {
                    // Stopped short of it from its camp: forget it, look
                    // again.
                    mind.water.retain(|_, p| *p != place);
                    mind.due = true;
                    return None;
                }
                // Stopped short far from its camp: the way it knows to the
                // water starts at the camp. It goes back there first.
                mind.doing = Doing::Home;
                return decision(
                    Intention::MoveTo {
                        body: id,
                        target: camp,
                    },
                    Why::Thirst,
                );
            }
            if mind.doing == Doing::Home && !mind.stopped && !at_camp {
                // On its way back to the camp: it goes on.
                return None;
            }
            mind.doing = Doing::ToDrink(place);
            return decision(
                Intention::MoveTo {
                    body: id,
                    target: place.stand,
                },
                Why::Thirst,
            );
        }
        return go_looking(map, rules, id, here, mind, seed, Why::Thirst);
    }
    if hungry && let Some(item) = carried_food(world, rules, id) {
        mind.doing = Doing::Nothing;
        return decision(Intention::Eat { body: id, item }, Why::Hunger);
    }
    // Hungry by night, it sleeps on it unless its reserve is weak: the
    // night's gathering gave crumbs and cost the sleep.
    if hungry && (!night || c.is_weak(&rules.needs)) {
        let nearest = mind
            .food
            .iter()
            .min_by_key(|(item, pos)| (far(**pos, here), **item))
            .map(|(item, pos)| (*item, *pos));
        if let Some((item, pos)) = nearest {
            if (pos.x - here.x).abs() <= 1 && (pos.y - here.y).abs() <= 1 {
                mind.doing = Doing::Nothing;
                return decision(Intention::Eat { body: id, item }, Why::Hunger);
            }
            if mind.doing == Doing::ToEat(item, pos) && mind.stopped {
                mind.food.remove(&item);
                mind.due = true;
                return None;
            }
            mind.doing = Doing::ToEat(item, pos);
            return decision(
                Intention::MoveTo {
                    body: id,
                    target: pos,
                },
                Why::Hunger,
            );
        }
        if can_carry && let (Some(goal), fresh) = to_gather(mind, bearers, groups) {
            if fresh {
                mind.chose_at = Some(now);
            }
            let decision = go_for(world, rules, id, mind, goal, Why::Hunger);
            return follow_or_wait(world, rules, groups, id, mind, decision, now);
        }
        // It knows the seasons: it goes looking by day, and only when
        // something is ripe somewhere. Out of season it bears its hunger
        // by its camp.
        let ripe_somewhere = food.now.iter().any(Option::is_some);
        if !night && ripe_somewhere {
            return go_looking(map, rules, id, here, mind, seed, Why::Hunger);
        }
    }
    // The day's plan said at the fire (D56): a group's turn, a look out.
    let full = !can_carry || carried_kj(world, rules, id) >= mind.ways.stock_kj;
    let mut gathers = !night && (!full || on_the_way(world, groups, mind));
    let mut group_home = false;
    if !night {
        match group_turn(world, rules, groups, (id, here), mind, full, now) {
            Turn::Go => {}
            Turn::Home => {
                gathers = false;
                group_home = true;
            }
            Turn::Decide(decision) => return decision,
        }
        if gathers && let Turn::Decide(decision) = look_out(world, rules, id, here, mind) {
            return decision;
        }
    }
    if gathers && let (Some(goal), fresh) = to_gather(mind, bearers, groups) {
        if fresh {
            mind.chose_at = Some(now);
        }
        let decision = go_for(world, rules, id, mind, goal, Why::Gather);
        return follow_or_wait(world, rules, groups, id, mind, decision, now);
    }
    let home = (here.x - camp.x).abs() <= HOME_TILES && (here.y - camp.y).abs() <= HOME_TILES;
    // Its way is to sleep out (D50): at night it lies down where it
    // stands, as if the camp were under its feet.
    if home || (night && mind.ways.sleep_out) {
        mind.doing = Doing::Nothing;
        if night && !c.asleep {
            return decision(Intention::Sleep { body: id }, Why::Night);
        }
        return None;
    }
    if mind.doing == Doing::Home {
        // On its way (something else made it think): it goes on. Stopped
        // short of the camp: it stays there, and tries again at the next
        // change rather than asking the same walk at once.
        if mind.stopped {
            mind.doing = Doing::Nothing;
        }
        return None;
    }
    mind.doing = Doing::Home;
    let why = if night { Why::Night } else { Why::Home };
    if mind.stop.is_none() {
        let reason = if night {
            STOP_NIGHT
        } else if full {
            STOP_FULL
        } else if group_home {
            STOP_GROUP
        } else {
            STOP_NOTHING
        };
        mind.stop = Some((reason, carried_kj(world, rules, id)));
    }
    decision(
        Intention::MoveTo {
            body: id,
            target: camp,
        },
        why,
    )
}

/// A tree or plant to gather from, and the material its crop gives.
#[derive(Debug, Clone, Copy)]
struct Target<'a> {
    aim: Aim,
    gives: &'a str,
}

/// What a body goes for when it gathers: a tree or plant within the place
/// it stands at, or a place it remembers, with the tile to stand on.
#[derive(Debug, Clone, Copy)]
enum Goal<'a> {
    Plant(Target<'a>),
    Place(PlaceKey, Pos),
}
/// The day of the world, counted from its first.
fn today(world: &World) -> u64 {
    world.tick().0 / world.calendar().ticks_per_day().max(1)
}

/// [`WAKE_SPREAD_SECONDS`] in ticks.
fn wake_spread(world: &World) -> u64 {
    (WAKE_SPREAD_SECONDS * u64::from(world.calendar().ticks_per_second())).max(1)
}

/// How often one of a group waiting at dawn looks up again, in game
/// seconds (D56): its caller set off walks 6 tiles in that time, within
/// [`crate::FOLLOW_TILES`], so a member waiting by it sees it go and
/// walks behind it.
pub const GROUP_WAIT_SECONDS: u64 = 4;

/// Members who stand at the group's place within this many game seconds
/// of their caller walked with it (D56: the measure of "look behind").
pub const TOGETHER_SECONDS: u64 = 60;

/// [`GROUP_WAIT_SECONDS`] in ticks.
fn group_wait(world: &World) -> u64 {
    (GROUP_WAIT_SECONDS * u64::from(world.calendar().ticks_per_second())).max(1)
}

/// A game hour, in ticks: a 24th of the day.
fn hour(world: &World) -> u64 {
    (world.calendar().ticks_per_day() / 24).max(1)
}

/// What an hour of gathering a plant of `kind` gives, in kilojoules: of
/// the crop giving `gives`, or of its best crop.
fn kj_per_hour(rules: &Content, kind: &str, gives: Option<&str>) -> u64 {
    rules
        .crops
        .iter()
        .filter(|crop| crop.on == kind && gives.is_none_or(|g| g == crop.gives))
        .filter_map(|crop| {
            let food = rules.material(&crop.gives)?.food?;
            Some(u64::from(crop.per_hour_g) * u64::from(food.energy_kj_per_kg) / 1000)
        })
        .max()
        .unwrap_or(0)
}

/// What an hour of a plant giving `kj_per_hour` of a full one, gathered
/// at `per_hour_g`, gives when `left_g` is left on it: an hour of it or
/// all of it, and nothing under [`GATHER_MIN_MINUTES`] of work.
fn kj_of_an_hour(kj_per_hour: u64, per_hour_g: u32, left_g: u32) -> Option<u64> {
    if u64::from(left_g) * 60 < u64::from(per_hour_g) * u64::from(GATHER_MIN_MINUTES) {
        return None;
    }
    Some(kj_per_hour * u64::from(left_g.min(per_hour_g)) / u64::from(per_hour_g.max(1)))
}

/// What a plant giving `kj_per_hour` is worth at `far_squared` (the
/// squared distance [`far`] gives): halved at `half_tiles` tiles (the
/// body's way, [`WORTH_HALF_TILES`] by default). Until 22 November 2026
/// the squared distance was taken for tiles, so the worth was halved at
/// about 6 tiles, not 32, and the way "twice as far" only reached 1.4
/// times farther (the brain audit's P7).
fn worth(kj_per_hour: u64, far_squared: i64, half_tiles: u64) -> u64 {
    let tiles = u64::try_from(far_squared).unwrap_or(0).isqrt();
    let half_tiles = half_tiles.max(1);
    kj_per_hour * half_tiles / (half_tiles + tiles)
}

/// Is the choice made at `Mind::chose_at` still fresh: less than the
/// content's `thought.plant_choice_hours` old (D53).
fn still_chosen(world: &World, rules: &Content, mind: &Mind, now: u64) -> bool {
    let hours = u64::from(rules.needs.thought.plant_choice_hours);
    mind.chose_at
        .is_some_and(|at| now < at.saturating_add(hours.saturating_mul(hour(world))))
}

/// Is the place `key` one to go to now: known, bearing something not
/// spent, within reach today, not left alone for now.
fn place_open(world: &World, food: &Food, mind: &Mind, key: PlaceKey, now: u64) -> bool {
    mind.places.get(&key).is_some_and(|place| {
        place.bearing(food).is_some()
            && !place.is_barred()
            && place.not_before.is_none_or(|tick| now >= tick)
            && mind.out_of_reach.get(&key) != Some(&today(world))
    })
}

/// What the body goes for to gather (D53), and whether the choice is
/// fresh. Kept while it holds and was chosen less than the content's
/// `thought.plant_choice_hours` ago: the tree or plant it is going to
/// (`Doing::ToGather`), if its kind bears, it is not left alone and its
/// place is open; else the place it is going to (`Doing::ToPlace`), if
/// open. Else chosen anew: the place worth most ([`place_to_gather`]);
/// standing at it, the plant worth most there ([`pick_in_place`]), and
/// another place when that one holds nothing worth a step; a place with
/// no tile to stand on is left for the day. One looks at the pantry once
/// in a while: a perception, nothing tuned to a result.
fn goal<'a>(
    world: &World,
    rules: &'a Content,
    food: &Food,
    bearers: &mut Placed,
    here: Pos,
    mind: &mut Mind,
    now: u64,
) -> (Option<Goal<'a>>, bool) {
    let side = place_side(rules);
    let kept = still_chosen(world, rules, mind, now);
    if let Doing::ToGather(aim) = mind.doing
        && kept
        && let Some(bears) = food.now(aim.kind)
        && mind
            .left_alone
            .get(&aim.key)
            .is_none_or(|&tick| now >= tick)
        && place_open(world, food, mind, place_of(aim.at.tile(), side), now)
    {
        return (
            Some(Goal::Plant(Target {
                aim,
                gives: rules.crops[bears.crop].gives.as_str(),
            })),
            false,
        );
    }
    let mut fresh = false;
    let mut place = match mind.doing {
        Doing::ToPlace(key) if kept && place_open(world, food, mind, key, now) => Some(key),
        _ => {
            fresh = true;
            place_to_gather(world, food, here, mind, now, side, None).map(|(key, _)| key)
        }
    };
    let mut tried = 0;
    while let Some(key) = place {
        tried += 1;
        if tried > places_remembered(rules) {
            break;
        }
        if at_place(here, key, side) {
            if let Some((target, worth)) =
                pick_in_place(world, rules, food, bearers, mind, &[], key, here, now)
            {
                // The look on arrival refreshed what this place holds:
                // another remembered fuller, less the walk, is gone to
                // instead.
                if let Some((other, better)) =
                    place_to_gather(world, food, here, mind, now, side, Some(key))
                    && better > worth
                {
                    fresh = true;
                    place = Some(other);
                    continue;
                }
                return (Some(Goal::Plant(target)), true);
            }
        } else if let Some(stand) = place_stand(world, rules, key, side) {
            return (Some(Goal::Place(key, stand)), fresh);
        } else {
            leave_for_today(world, mind, key);
        }
        fresh = true;
        place = place_to_gather(world, food, here, mind, now, side, None).map(|(key, _)| key);
    }
    (None, fresh)
}

/// Its plan for today, if it holds one not done with (D56).
fn plan_today(world: &World, mind: &Mind) -> Option<Plan> {
    let day = u32::try_from(today(world)).unwrap_or(u32::MAX);
    mind.plan.filter(|p| p.day == day && !p.done)
}

/// Done with its plan for today (D56): its own day again.
fn plan_done(mind: &mut Mind) {
    if let Some(plan) = mind.plan.as_mut() {
        plan.done = true;
    }
}

/// Whether `a` and `b` stand within `tiles` of each other on each axis.
fn within(a: Pos, b: Pos, tiles: i32) -> bool {
    (a.x - b.x).abs() <= tiles && (a.y - b.y).abs() <= tiles
}

/// What its plan for today says to gather (D56), if it holds one with a
/// place: the place it said it goes to, or its group's when the caller
/// moved it on and it stands in sight of the caller; standing there, the
/// plant worth most that no mate of its group is at. The place found bare:
/// the caller moves the group on to the best place it knows from there,
/// or the group ends; anyone else is done with its plan. `None`: no plan
/// for this, its own choice ([`goal`]).
#[allow(clippy::too_many_arguments, reason = "one reading of one plan")]
fn planned<'a>(
    world: &World,
    rules: &'a Content,
    food: &Food,
    bearers: &mut Placed,
    groups: &mut BTreeMap<BodyId, Group>,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    now: u64,
) -> Option<(Option<Goal<'a>>, bool)> {
    let plan = plan_today(world, mind)?;
    let mut key = plan.place?;
    let side = place_side(rules);
    let day = today(world);
    let mut group = plan.leader.and_then(|leader| groups.get_mut(&leader));
    if let Some(g) = group.as_deref_mut() {
        if g.home || g.ended.is_some() {
            return None;
        }
        if g.place != key {
            let sight = plant_sight_at(world.map(), rules, here);
            let leader = g.members[0];
            let sees = id == leader
                || world
                    .body(leader)
                    .is_some_and(|b| within(b.pos(), here, sight));
            if !sees {
                g.left.insert(id);
                plan_done(mind);
                return None;
            }
            key = g.place;
            if let Some(p) = mind.plan.as_mut() {
                p.place = Some(key);
            }
        }
    }
    let blocked = mind.out_of_reach.get(&key) == Some(&day)
        || mind.places.get(&key).is_some_and(Place::is_barred);
    if blocked || !mind.places.contains_key(&key) {
        if let Some(g) = group.as_deref_mut() {
            if g.members[0] == id {
                // Could not walk there: told as what it was, not as bare.
                g.ended = Some(if blocked {
                    GroupEnd::Blocked
                } else {
                    GroupEnd::Bare
                });
            } else {
                g.left.insert(id);
            }
        }
        plan_done(mind);
        return None;
    }
    if at_place(here, key, side) {
        let taken: Vec<(ChunkPos, Source)> = group.as_deref().map_or_else(Vec::new, |g| {
            g.taken
                .iter()
                .filter(|(m, _)| **m != id)
                .map(|(_, k)| *k)
                .collect()
        });
        if let Some(g) = group.as_deref_mut() {
            g.arrived.entry(id).or_insert(now);
        }
        if let Some((target, _)) =
            pick_in_place(world, rules, food, bearers, mind, &taken, key, here, now)
        {
            if let Some(g) = group.as_deref_mut() {
                g.taken.insert(id, target.aim.key);
            }
            return Some((Some(Goal::Plant(target)), true));
        }
        if let Some(g) = group
            && g.members[0] == id
        {
            if let Some((next, _)) = place_to_gather(world, food, here, mind, now, side, Some(key))
                && let Some(stand) = place_stand(world, rules, next, side)
            {
                g.place = next;
                g.taken.clear();
                if let Some(p) = mind.plan.as_mut() {
                    p.place = Some(next);
                }
                return Some((Some(Goal::Place(next, stand)), true));
            }
            g.ended = Some(GroupEnd::Bare);
        }
        plan_done(mind);
        return None;
    }
    if let Some(stand) = place_stand(world, rules, key, side) {
        return Some((Some(Goal::Place(key, stand)), false));
    }
    plan_done(mind);
    None
}

/// What a member of a group does on its way to the group's place (D56):
/// it walks behind its caller ([`Intention::Follow`]) when the caller
/// walks within reach; it waits by the caller, an hour after the fire at
/// most, while the caller has not set off; otherwise it walks as decided.
fn follow_or_wait(
    world: &World,
    rules: &Content,
    groups: &mut BTreeMap<BodyId, Group>,
    id: BodyId,
    mind: &mut Mind,
    decision: Option<Decision>,
    now: u64,
) -> Option<Decision> {
    let d = decision?;
    let Some(plan) = plan_today(world, mind) else {
        return Some(d);
    };
    let (Some(leader), Some(place)) = (plan.leader, plan.place) else {
        return Some(d);
    };
    if mind.doing != Doing::ToPlace(place) {
        return Some(d);
    }
    let (Some(g), Some(lead), Some(body)) =
        (groups.get_mut(&leader), world.body(leader), world.body(id))
    else {
        return Some(d);
    };
    let early = now < g.dawn + hour(world) && !g.arrived.contains_key(&leader);
    if leader == id {
        // The caller waits, an hour after dawn at most, while a member
        // still sleeps or has not come within reach of it.
        let waits = early
            && !g.started
            && g.members.iter().skip(1).any(|m| {
                !g.left.contains(m)
                    && world.body(*m).is_some_and(|b| {
                        b.condition().asleep || !within(b.pos(), body.pos(), crate::FOLLOW_TILES)
                    })
            });
        if waits {
            mind.doing = Doing::Nothing;
            mind.wake_at = Some(now + group_wait(world));
            return None;
        }
        return look_behind(world, rules, g, (id, body), mind, d, now);
    }
    let Intention::MoveTo { target, .. } = d.intention else {
        return Some(d);
    };
    if g.home {
        return Some(d);
    }
    let near = within(lead.pos(), body.pos(), crate::FOLLOW_TILES);
    if near && lead.is_walking() && !lead.condition().asleep {
        g.followed.insert(id);
        return Some(Decision {
            intention: Intention::Follow {
                body: id,
                leader,
                target,
            },
            why: d.why,
        });
    }
    if early && !lead.is_walking() {
        if near {
            // The caller has not set off (or still sleeps): it waits by it.
            mind.doing = Doing::Nothing;
            mind.wake_at = Some(now + group_wait(world));
            return None;
        }
        // It goes to stand by its caller first.
        return Some(Decision {
            intention: Intention::MoveTo {
                body: id,
                target: lead.pos(),
            },
            why: d.why,
        });
    }
    Some(d)
}

/// Whether `id`, under no trial, starts one today (D50): in distress long
/// enough, by the daily chance of its will; with nothing wrong, by the
/// far smaller chance of a bold will to search for better (amended 24
/// November 2026), judged the same.
fn wants_to_try(trials: &Trials, seed: u64, (id, day): (BodyId, u64), mind: &Mind) -> bool {
    if mind.distress_days >= trials.trial_after_days {
        scatter::roll(seed, purpose::TRIAL_TRY, &[u64::from(id.0), day]) % 100
            < trials.try_percent(mind.will)
    } else {
        scatter::roll(seed, purpose::TRIAL_CALM, &[u64::from(id.0), day]) % 1000
            < trials.calm_try_permille(mind.will)
    }
}

/// What walking `tiles` costs `body`, in per mille of a day's food (D50,
/// amended 24 November 2026: walking costs energy).
fn walk_permille(rules: &Content, body: &Body, tiles: u64) -> u64 {
    let c = body.condition();
    let j =
        tiles * u64::from(rules.needs.walk_j_per_m) * u64::from(c.size_permille.min(1000)) / 1000;
    j * 1000 / c.need_j_per_day(&rules.needs).max(1)
}

/// The day closed at nightfall, to be noted where it lies at the next
/// dawn (D50, D53 amended): the hunger and thirst left, in per mille of a
/// day (a gauge point is a hundredth), the day's walk, and what it did
/// place by place. The cold is read at dawn.
fn night_of(rules: &Content, body: &Body, mind: &mut Mind) -> Night {
    let c = body.condition();
    let mut losses = [0; SENSES];
    losses[Sense::Food as usize] = u64::from(c.hunger.points()) * 10;
    losses[Sense::Water as usize] = u64::from(c.thirst.points()) * 10;
    Night {
        losses,
        walk: walk_permille(rules, body, std::mem::take(&mut mind.walked_today)),
        day: std::mem::take(&mut mind.day_places),
    }
}

/// Why `id` first went home from gathering today and what it carried,
/// told at nightfall (lot 5b, a measure of the review), when the callers
/// review their companions.
fn stopped(
    world: &World,
    rules: &Content,
    (id, body): (BodyId, &Body),
    mind: &mut Mind,
    now: u64,
) -> Option<News> {
    let (why, stop_kj) = mind.stop.take().unwrap_or((STOP_NONE, 0));
    rules.needs.decisions.reviews().then(|| News::Stopped {
        body: id,
        why,
        stop_kj,
        evening_kj: carried_kj(world, rules, id),
        need_kj: need_kj_per_day(rules, body),
        at: now,
    })
}

/// What a body did its day, kept in its habit when decisions are on, and
/// told as [`News::Habit`] (D56). With the will tempered, its activities
/// are judged (D56): yesterday's look out by whether it found a place it
/// told new at the fire this dawn, today's walk to a place by whether it
/// gathered a day of its need, today's silence at the fire by whether its
/// children were fed (`kin_fed`: none of them hungry; `None`: no child);
/// a season ended (`season`), its will and confidences come back toward
/// its innate will first.
fn close_habit(
    rules: &Content,
    body: &Body,
    id: BodyId,
    mind: &mut Mind,
    (day, gathered_kj, now): (u64, u64, u64),
    (kin_fed, season, indirect): (Option<bool>, bool, bool),
    news: &mut Vec<News>,
) {
    if season {
        mind.relax(&rules.needs.temperance);
    }
    let plan = mind.plan.take().filter(|p| u64::from(p.day) == day);
    let act = plan.map_or(Act::Nothing, |p| p.act);
    let bare = std::mem::take(&mut mind.bare_today);
    let decisions = &rules.needs.decisions;
    let need = need_kj_per_day(rules, body).max(1);
    let fed = gathered_kj >= need_kj_per_day(rules, body);
    if decisions.on() {
        let temper = &rules.needs.temperance;
        if temper.on() {
            let looked = mind
                .habit
                .last()
                .filter(|b| *b & 0x0f == Act::Looked.code());
            // An indirect success (PR 1 ter): one who joined its call or
            // followed its answer came back fed; it counts as its own day
            // of gathering fed, once a day.
            let gathering = matches!(act, Act::Alone | Act::Called | Act::Joined);
            let judged = [
                looked.map(|b| ("explore", b & HABIT_FOUND != 0)),
                gathering.then_some(("gather", fed || indirect)),
                (indirect && !gathering).then_some(("gather", true)),
                plan.filter(|_| act == Act::Nothing)
                    .and(kin_fed)
                    .map(|k| ("stay", k)),
            ];
            for (name, success) in judged.into_iter().flatten() {
                if let Some(i) = temper.activity(name) {
                    news.extend(mind.judge_day(temper, i, success, (id, now)));
                }
            }
        }
        let mut byte = act.code();
        if bare > 0 {
            byte |= HABIT_BARE;
        }
        if fed {
            byte |= HABIT_FED;
        }
        mind.habit.push(byte);
        let keep = usize::try_from(decisions.habit_days).unwrap_or(usize::MAX);
        if mind.habit.len() > keep {
            let extra = mind.habit.len() - keep;
            mind.habit.drain(..extra);
        }
    }
    news.push(News::Habit {
        body: id,
        act,
        decided: plan.is_some(),
        kj_percent: u32::try_from(gathered_kj * 100 / need).unwrap_or(u32::MAX),
        bare,
        at: now,
    });
}

/// With the will tempered (D56): whether each parent's children are fed
/// tonight ([`kin_fed`]), and whether a season ends with this day (the
/// will and the confidences come back toward the innate then).
fn temper_today(world: &World, rules: &Content, day: u64) -> (BTreeMap<BodyId, bool>, bool) {
    if !rules.needs.temperance.on() {
        return (BTreeMap::new(), false);
    }
    let season = u64::from(world.calendar().days_per_year() / 4).max(1);
    (kin_fed(world, rules), day > 0 && day.is_multiple_of(season))
}

/// Whether the children under `follows_mother_years` of each parent are
/// fed at nightfall (D56): none of them hungry. Parents without such a
/// child are not in it.
fn kin_fed(world: &World, rules: &Content) -> BTreeMap<BodyId, bool> {
    let young = u64::from(rules.needs.decisions.follows_mother_years) * 1000;
    let mut fed: BTreeMap<BodyId, bool> = BTreeMap::new();
    for child in world.bodies() {
        if child.condition().vitality.points() == 0 || world.age_milli(child) >= young {
            continue;
        }
        let ok = child.condition().hunger.points() < HUNGRY;
        for parent in [child.mother(), child.father()].into_iter().flatten() {
            let entry = fed.entry(parent).or_insert(true);
            *entry = *entry && ok;
        }
    }
    fed
}

/// One called by name changed its mind (D56, PR 1 ter): the room kept for
/// it is given back.
fn unreserve(
    reserved: &mut BTreeMap<BodyId, (BodyId, PlaceKey)>,
    rooms: &mut BTreeMap<PlaceKey, (u32, u32, Option<BodyId>)>,
    id: BodyId,
) {
    if let Some((_, key)) = reserved.remove(&id)
        && let Some(room) = rooms.get_mut(&key)
    {
        room.1 = room.1.saturating_sub(1);
    }
}

/// One who followed another today (D56, PR 1 ter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Follow {
    /// The follower.
    body: BodyId,
    /// The one followed.
    whom: BodyId,
    /// How ([`FOLLOW_JOINED`], [`FOLLOW_ASKED`], [`FOLLOW_TOLD`]).
    how: u8,
    /// The activity followed for, an index in `content::ACTIVITIES`.
    activity: u8,
    /// Back with a day of its need.
    fed: bool,
    /// Back with nothing.
    empty: bool,
    /// What it gathered, in per cent of a day of its need.
    kj_percent: u32,
}

/// The index of the activity `name` in `content::ACTIVITIES`.
fn activity_index(name: &str) -> u8 {
    crate::content::ACTIVITIES
        .iter()
        .position(|n| *n == name)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(u8::MAX)
}

impl Group {
    /// The news of the group's end at nightfall (D56).
    fn ended_news(&self, leader: BodyId, now: u64, together: u64) -> News {
        let count = |set: &BTreeSet<BodyId>| u32::try_from(set.len()).unwrap_or(u32::MAX);
        let first = self.arrived.get(&leader).copied();
        let last = self.arrived.values().copied().max();
        News::GroupEnded {
            leader,
            place: self.called,
            last_place: self.place,
            members: u32::try_from(self.members.len()).unwrap_or(u32::MAX),
            followed: count(&self.followed),
            full: count(&self.full),
            left: count(&self.left),
            end: self.ended.unwrap_or(if self.home {
                GroupEnd::Home
            } else {
                GroupEnd::Night
            }),
            stretch_ticks: match (first, last) {
                (Some(first), Some(last)) => last.saturating_sub(first),
                _ => 0,
            },
            together: first.map_or(0, |first| {
                count_u32(
                    self.arrived
                        .iter()
                        .filter(|&(m, &at)| *m != leader && at.abs_diff(first) <= together),
                )
            }),
            waited_ticks: self.waited,
            at: now,
        }
    }
}

/// How many, as a `u32`.
fn count_u32<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).unwrap_or(u32::MAX)
}

/// Whether its group's caller has not reached the group's place yet
/// (D56): until then a member goes on, full or not.
fn on_the_way(world: &World, groups: &BTreeMap<BodyId, Group>, mind: &Mind) -> bool {
    plan_today(world, mind)
        .filter(|p| matches!(p.act, Act::Called | Act::Joined))
        .and_then(|p| p.leader)
        .and_then(|l| groups.get(&l))
        .is_some_and(|g| g.ended.is_none() && !g.home && !g.arrived.contains_key(&g.members[0]))
}

/// The caller on its way looks behind (D56, decided by the project lead
/// on 23 November 2026): while it sees a member not yet there walking
/// behind it, farther than `behind_tiles`, it stops and waits,
/// `behind_wait_minutes` at most in a day; walking with none left behind, it goes on and looks
/// again in [`GROUP_WAIT_SECONDS`]. `None`: it waits, or walks on.
fn look_behind(
    world: &World,
    rules: &Content,
    g: &mut Group,
    (id, body): (BodyId, &Body),
    mind: &mut Mind,
    d: Decision,
    now: u64,
) -> Option<Decision> {
    let dec = &rules.needs.decisions;
    let on_way = !g.arrived.contains_key(&id) && (body.is_walking() || g.held);
    if dec.behind_tiles == 0 || !on_way {
        g.started = true;
        if dec.behind_tiles > 0 && !g.arrived.contains_key(&id) {
            // Setting off, it will look behind in a moment.
            mind.wake_at = Some(now + group_wait(world));
        }
        return Some(d);
    }
    let reach = i32::try_from(dec.behind_tiles).unwrap_or(i32::MAX);
    let per_second = u64::from(world.calendar().ticks_per_second());
    let most = u64::from(dec.behind_wait_minutes) * 60 * per_second;
    // It sees a member walking behind it, out of reach: it waits for it.
    // One it does not see, or that stands still, it cannot wait for.
    let sight = sight_at(world.map(), rules, body.pos());
    let behind: Vec<BodyId> = g
        .members
        .iter()
        .skip(1)
        .filter(|m| {
            !g.left.contains(m)
                && !g.arrived.contains_key(m)
                && world.body(**m).is_some_and(|b| {
                    b.is_walking()
                        && within(b.pos(), body.pos(), sight)
                        && !within(b.pos(), body.pos(), reach)
                })
        })
        .copied()
        .collect();
    if !behind.is_empty() && g.waited < most {
        // A fact of its review (PR 1 ter): whom it waited for.
        g.waited_for.extend(behind);
        if g.held {
            g.waited += group_wait(world);
        }
        g.held = true;
        mind.doing = Doing::Nothing;
        mind.wake_at = Some(now + group_wait(world));
        // Walking, it stops where it stands.
        return body.is_walking().then_some(Decision {
            intention: Intention::MoveTo {
                body: id,
                target: body.pos(),
            },
            why: d.why,
        });
    }
    mind.wake_at = Some(now + group_wait(world));
    if g.held {
        g.held = false;
        return Some(d);
    }
    None
}

/// What a member of a group does once it is there (D56).
enum Turn {
    /// Its plan goes on.
    Go,
    /// It goes home, gathering no more today.
    Home,
    /// It does this.
    Decide(Option<Decision>),
}

/// A member of a group at its turn (D56): one carrying what it wanted is
/// full; the caller calls the group home when it is full itself or when
/// `home_when_full_percent` of those still with it are; then the members
/// walk home behind it when within reach. One full before that waits by
/// the caller while it sees it, or goes home alone.
fn group_turn(
    world: &World,
    rules: &Content,
    groups: &mut BTreeMap<BodyId, Group>,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    full: bool,
    now: u64,
) -> Turn {
    let Some(plan) = plan_today(world, mind) else {
        return Turn::Go;
    };
    if !matches!(plan.act, Act::Called | Act::Joined) {
        return Turn::Go;
    }
    let Some(g) = plan.leader.and_then(|l| groups.get_mut(&l)) else {
        return Turn::Go;
    };
    let leader = g.members[0];
    let lead = world
        .body(leader)
        .filter(|b| b.condition().vitality.points() > 0);
    if lead.is_none() && g.ended.is_none() {
        g.ended = Some(GroupEnd::Night);
    }
    if full {
        g.full.insert(id);
    }
    // On the way the group goes on: one full before it is there has
    // still promised its place, or followed its caller to it.
    if !g.arrived.contains_key(&leader) && g.ended.is_none() && !g.home {
        return Turn::Go;
    }
    if id == leader && !g.home && g.ended.is_none() {
        let with = g.members.len().saturating_sub(g.left.len()).max(1);
        let percent = usize::try_from(rules.needs.decisions.home_when_full_percent).unwrap_or(100);
        if full || g.full.len() * 100 >= with * percent {
            g.home = true;
        }
    }
    if g.home || g.ended.is_some() {
        plan_done(mind);
        if id != leader
            && let Some(lead) = lead
            && lead.is_walking()
            && within(lead.pos(), here, crate::FOLLOW_TILES)
        {
            g.followed.insert(id);
            mind.doing = Doing::Home;
            if mind.stop.is_none() {
                let reason = if full { STOP_FULL } else { STOP_GROUP };
                mind.stop = Some((reason, carried_kj(world, rules, id)));
            }
            return Turn::Decide(Some(Decision {
                intention: Intention::Follow {
                    body: id,
                    leader,
                    target: mind.ways.home,
                },
                why: Why::Home,
            }));
        }
        return Turn::Home;
    }
    if full && id != leader {
        let sight = plant_sight_at(world.map(), rules, here);
        if lead.is_some_and(|b| within(b.pos(), here, sight)) {
            mind.doing = Doing::Nothing;
            mind.wake_at = Some(now + hour(world) / 6);
            return Turn::Decide(None);
        }
        g.left.insert(id);
        plan_done(mind);
        return Turn::Home;
    }
    Turn::Go
}

/// One who said it goes looking (D56) walks out its way from the camp,
/// leg after leg, each a sight farther than the last, looking from each
/// leg's end; its legs walked, it gathers as any day. [`Turn::Go`]: no
/// such plan, or its legs walked; [`Turn::Decide`]: what it does.
fn look_out(world: &World, rules: &Content, id: BodyId, here: Pos, mind: &mut Mind) -> Turn {
    let Some(plan) = plan_today(world, mind).filter(|p| p.act == Act::Looked) else {
        return Turn::Go;
    };
    if mind.doing == Doing::Exploring(Why::Gather) && !mind.stopped {
        return Turn::Decide(None);
    }
    if plan.legs >= rules.needs.decisions.look_legs {
        plan_done(mind);
        return Turn::Go;
    }
    let map = world.map();
    let reach = sight_at(map, rules, here);
    let legs = i32::try_from(plan.legs).unwrap_or(i32::MAX);
    let out = reach.saturating_mul(legs.saturating_add(2));
    let (dx, dy) = DIRECTIONS[usize::from(plan.octant) % 8];
    let camp = mind.ways.home;
    let (w, h) = (
        i32::try_from(map.width()).expect("fits") - 1,
        i32::try_from(map.height()).expect("fits") - 1,
    );
    let target = Pos::new(
        camp.x.saturating_add(dx.saturating_mul(out)).clamp(0, w),
        camp.y.saturating_add(dy.saturating_mul(out)).clamp(0, h),
    );
    if let Some(p) = mind.plan.as_mut() {
        p.legs += 1;
    }
    if target == here {
        plan_done(mind);
        return Turn::Go;
    }
    mind.doing = Doing::Exploring(Why::Gather);
    Turn::Decide(Some(Decision {
        intention: Intention::MoveTo { body: id, target },
        why: Why::Explore,
    }))
}

/// The place it knows bearing food now worth most, open
/// ([`place_open`]), but for `not` if given: what an hour of the fullest
/// plant seen there gave, less the farther (`Ways::worth_half_tiles`),
/// the more plants holding food breaking ties; with its worth. Nothing
/// of the world is read: a memory, and the content's seasons.
fn place_to_gather(
    world: &World,
    food: &Food,
    here: Pos,
    mind: &Mind,
    now: u64,
    side: i32,
    not: Option<PlaceKey>,
) -> Option<(PlaceKey, u64)> {
    let today = today(world);
    mind.places
        .iter()
        .filter(|(key, place)| {
            not != Some(**key)
                && !place.is_barred()
                && mind.out_of_reach.get(key) != Some(&today)
                && place.not_before.is_none_or(|tick| now >= tick)
        })
        .filter_map(|(key, place)| {
            let seen = place.bearing(food)?;
            let worth = worth(
                seen.best_kj,
                far(place_centre(*key, side), here),
                mind.ways.worth_half_tiles,
            );
            Some((*key, worth, seen.holding))
        })
        .max_by_key(|&(key, worth, holding)| (worth, holding, std::cmp::Reverse(key)))
        .map(|(key, worth, _)| (key, worth))
}

/// A habit's byte (D56): the day it found a place new to it.
const HABIT_FOUND: u8 = 0x10;
/// A habit's byte (D56): the day it found a place it went to bare.
const HABIT_BARE: u8 = 0x20;
/// A habit's byte (D56): the day it gathered a day of its need or more.
const HABIT_FED: u8 = 0x40;

/// The best places `id` knows bearing food now, weighed from the camp at
/// its own pace (D56): what an hour of the fullest plant seen there gave,
/// less the farther, a far place the farther for one who walks slowly; at
/// most the content's `decisions.choices`, the best first. With each, the
/// room it has (what holds there by its memory, divided by
/// `plants_per_gatherer`, between 1 and `group_max`) and whether the body
/// saw it itself (a witness may call; a hearer goes alone).
fn choices(
    food: &Food,
    rules: &Content,
    world: &World,
    mind: &Mind,
    id: BodyId,
    camp: Pos,
    pace: u64,
) -> Vec<(PlaceKey, u32, bool)> {
    let dec = &rules.needs.decisions;
    let side = place_side(rules);
    let today = today(world);
    let now = world.tick().0;
    let pace = i64::try_from(pace.max(1)).unwrap_or(1);
    let mut ranked: Vec<(u64, u32, PlaceKey, u32, bool)> = mind
        .places
        .iter()
        .filter(|(key, place)| {
            mind.out_of_reach.get(key) != Some(&today)
                && !place.is_barred()
                && place.not_before.is_none_or(|tick| now >= tick)
        })
        .filter_map(|(key, place)| {
            let seen = place.bearing(food)?;
            let far_paced =
                far(place_centre(*key, side), camp).saturating_mul(10_000) / (pace * pace);
            let worth = worth(seen.best_kj, far_paced, mind.ways.worth_half_tiles);
            let room = (place.bearing_count(food) / dec.plants_per_gatherer.max(1))
                .clamp(1, dec.group_max.max(1));
            let own = place.access.is_none_or(|a| a.is_own(id));
            Some((worth, u32::from(seen.holding), *key, room, own))
        })
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    ranked
        .into_iter()
        .take(usize::try_from(dec.choices).unwrap_or(usize::MAX))
        .map(|(_, _, key, room, own)| (key, room, own))
        .collect()
}

/// The way, one of eight from the camp, where `mind` knows the fewest
/// places (D56): where one who goes looking goes; between ways as little
/// known, the first from a turn drawn from the seed, the body and the day.
fn least_known(mind: &Mind, rules: &Content, camp: Pos, seed: u64, id: BodyId, day: u64) -> u8 {
    let side = place_side(rules);
    let mut counts = [0_u32; 8];
    for &key in mind.places.keys() {
        let at = place_centre(key, side);
        let (dx, dy) = (at.x - camp.x, at.y - camp.y);
        if dx != 0 || dy != 0 {
            counts[octant(dx, dy)] += 1;
        }
    }
    let start =
        usize::try_from(scatter::roll(seed, purpose::FIRE_LOOK, &[u64::from(id.0), day, 1]) % 8)
            .expect("under eight");
    let least = counts.iter().copied().min().unwrap_or(0);
    let way = (0..8)
        .map(|i| (start + i) % 8)
        .find(|&i| counts[i] == least)
        .unwrap_or(start);
    u8::try_from(way).expect("under eight")
}

/// How a plant ranks at its place ([`pick_in_place`]): its worth, the
/// nearer first, the key breaking ties.
type PickRank = (
    u64,
    std::cmp::Reverse<i64>,
    std::cmp::Reverse<(ChunkPos, Source)>,
);

/// Standing at the place `key`, the tree or plant there worth most now,
/// with that worth: of a kind bearing now and not spent there, not left
/// alone, by what an hour of it gives of what is left on it, less the
/// farther, and none holding less than [`GATHER_MIN_MINUTES`] of work.
/// The look refreshes what the place is remembered to hold
/// ([`Place::saw`]). Nothing worth a step: the place is left alone an
/// hour if a plant of it was, or its crop still ripens (D48: more
/// comes); else found bare for the year, by kind.
#[allow(
    clippy::too_many_arguments,
    reason = "one reading of one place, standing there"
)]
fn pick_in_place<'a>(
    world: &World,
    rules: &'a Content,
    food: &Food,
    bearers: &mut Placed,
    mind: &mut Mind,
    taken: &[(ChunkPos, Source)],
    key: PlaceKey,
    here: Pos,
    now: u64,
) -> Option<(Target<'a>, u64)> {
    let side = place_side(rules);
    let chunk = place_centre(key, side).chunk();
    let (x0, y0) = (key.0 * side, key.1 * side);
    let within = |t: Pos| (x0..x0 + side).contains(&t.x) && (y0..y0 + side).contains(&t.y);
    let place = mind.places.get(&key)?;
    let half = mind.ways.worth_half_tiles;
    let held = bearers.held(world, food, chunk);
    let mut best: Option<(PickRank, Target<'a>)> = None;
    // The kinds bearing now seen there, with their crop, how many of
    // their plants hold food and what the fullest gives an hour.
    let mut bearing: Vec<(u16, Bears, u16, u64)> = Vec::new();
    let mut left_alone = false;
    for (bearer, kind) in held.food() {
        let tile = bearer.at.tile();
        if !within(tile) {
            continue;
        }
        let Some(bears) = food.now(kind) else {
            continue;
        };
        if place.is_spent(kind, bears.year) {
            continue;
        }
        let k = (chunk, bearer.source);
        if mind.left_alone.get(&k).is_some_and(|&tick| now < tick) {
            left_alone = true;
            continue;
        }
        if taken.contains(&k) {
            // A mate of its group is at that one (D56): it sees it there.
            continue;
        }
        let i = bearing.iter().position(|b| b.0 == kind).unwrap_or_else(|| {
            bearing.push((kind, bears, 0, 0));
            bearing.len() - 1
        });
        let crop = &rules.crops[bears.crop];
        let Some((_, left_g)) = world.ripe_of_crop(chunk, bearer.source, bearer.at, bears.crop)
        else {
            continue;
        };
        let Some(kj) = kj_of_an_hour(bears.kj, crop.per_hour_g, left_g) else {
            continue;
        };
        bearing[i].2 = bearing[i].2.saturating_add(1);
        bearing[i].3 = bearing[i].3.max(kj);
        let d = far(tile, here);
        let rank = (
            worth(kj, d, half),
            std::cmp::Reverse(d),
            std::cmp::Reverse(k),
        );
        if best.as_ref().is_none_or(|(b, _)| rank > *b) {
            best = Some((
                rank,
                Target {
                    aim: Aim {
                        key: k,
                        at: bearer.at,
                        kind,
                    },
                    gives: crop.gives.as_str(),
                },
            ));
        }
    }
    let place = mind.places.get_mut(&key)?;
    for &(kind, _, holding, best_kj) in &bearing {
        place.saw(kind, 0, holding, best_kj);
    }
    place.looked_day = Some(u32::try_from(today(world)).unwrap_or(u32::MAX));
    if let Some((rank, target)) = best {
        return Some((target, rank.0));
    }
    // Nothing worth a step here.
    let later = now + RETRY_HOURS * hour(world);
    if left_alone {
        place.not_before = Some(later);
    }
    if bearing.is_empty() && !left_alone {
        // Nothing of what it remembered bearing stands there now in
        // season: left for the year, whatever it knew of the place.
        let known: Vec<u16> = place.kinds.iter().map(|k| k.kind).collect();
        for kind in known {
            if let Some(bears) = food.now(kind) {
                place.spend(kind, bears.year);
            }
        }
    }
    for (kind, bears, _, _) in bearing {
        if crops::ripening(&rules.crops[bears.crop], world.calendar(), world.tick()) {
            place.not_before = Some(later);
        } else {
            place.spend(kind, bears.year);
        }
    }
    if place.not_before != Some(later) {
        // Found bare (D56 counts it).
        mind.bare_today += 1;
    }
    None
}

/// The tile to stand on at the place `key`: the walkable one nearest its
/// middle, `None` when none is (water, rock).
fn place_stand(world: &World, rules: &Content, key: PlaceKey, side: i32) -> Option<Pos> {
    let centre = place_centre(key, side);
    let (x0, y0) = (key.0 * side, key.1 * side);
    (y0..y0 + side)
        .flat_map(|y| (x0..x0 + side).map(move |x| Pos::new(x, y)))
        .filter(|&p| walkable(world.map(), rules, p))
        .min_by_key(|&p| (far(p, centre), p.y, p.x))
}

/// The place `key` could not be walked to: left alone for the day.
fn leave_for_today(world: &World, mind: &mut Mind, key: PlaceKey) {
    let today = today(world);
    mind.out_of_reach.retain(|_, day| *day == today);
    mind.out_of_reach.insert(key, today);
}

/// Goes for `goal`: to the place, or to the plant ([`go_gather`]).
fn go_for(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    goal: Goal<'_>,
    why: Why,
) -> Option<Decision> {
    match goal {
        Goal::Plant(target) => go_gather(world, rules, id, mind, target, why),
        Goal::Place(key, stand) => {
            mind.doing = Doing::ToPlace(key);
            Some(Decision {
                intention: Intention::MoveTo {
                    body: id,
                    target: stand,
                },
                why,
            })
        }
    }
}

/// Walks to the tree or plant `target`, or gathers from it when within
/// reach.
fn go_gather(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    Target { aim, gives }: Target<'_>,
    why: Why,
) -> Option<Decision> {
    let body = world.body(id)?;
    let (dx, dy) = (
        i64::from(body.at().x - aim.at.x),
        i64::from(body.at().y - aim.at.y),
    );
    if dx * dx + dy * dy <= GATHER_REACH_MM * GATHER_REACH_MM {
        mind.doing = Doing::Gathering(aim);
        return Some(Decision {
            intention: Intention::Gather {
                body: id,
                chunk: aim.key.0,
                source: aim.key.1,
                gives: gives.to_owned(),
            },
            why,
        });
    }
    // A tree's trunk takes its tile: it stands beside it.
    let tile = aim.at.tile();
    let here = body.pos();
    let target = match aim.key.1 {
        Source::Plant(_) => tile,
        Source::Tree(_) => AROUND
            .iter()
            .filter(|&&(dx, dy)| (dx, dy) != (0, 0))
            .map(|&(dx, dy)| Pos::new(tile.x + dx, tile.y + dy))
            .filter(|&p| walkable(world.map(), rules, p))
            .min_by_key(|&p| (far(p, here), p.y, p.x))
            .unwrap_or(tile),
    };
    mind.doing = Doing::ToGather(aim);
    Some(Decision {
        intention: Intention::MoveTo { body: id, target },
        why,
    })
}

/// A walk to a plant ended. Out of reach of it, it stopped short: the
/// way there is not one it knows. Its whole place is left for the day,
/// or it would try every bush beyond the same river one after the other.
fn stopped_short(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    aim: Aim,
    news: &mut Vec<News>,
) {
    let Some(body) = world.body(id) else {
        return;
    };
    let (dx, dy) = (
        i64::from(body.at().x - aim.at.x),
        i64::from(body.at().y - aim.at.y),
    );
    if dx * dx + dy * dy <= GATHER_REACH_MM * GATHER_REACH_MM {
        return;
    }
    mind.doing = Doing::Nothing;
    let side = place_side(rules);
    let place = place_of(aim.at.tile(), side);
    leave_for_today(world, mind, place);
    // Barred by water or rock short of a plant of a place it does not
    // stand on: the place is seen, not reached.
    if let Some(cause @ (Blockage::Water | Blockage::Rock)) = mind.blocked
        && !at_place(body.pos(), place, side)
    {
        bar(world, (id, place, cause), mind, news);
    }
}

/// `id` marks the place `key` it holds seen, not reached (D53, amended
/// 24 November 2026): its walk there was barred by water or rock.
/// Lasting until one stands there, unlike the day's
/// [`leave_for_today`]; told at its next fire like a denial.
fn bar(
    world: &World,
    (id, key, cause): (BodyId, PlaceKey, Blockage),
    mind: &mut Mind,
    news: &mut Vec<News>,
) {
    let day = today_u32(world);
    let Some(place) = mind.places.get_mut(&key) else {
        return;
    };
    if place.is_barred() {
        return;
    }
    place.barred = Some(Mark::Draft(day));
    mind.barred_new.push((key, day));
    news.push(News::Barred {
        body: id,
        place: key,
        cause: Some(cause),
        at: world.tick().0,
    });
}

/// `id` stands on the place under its feet: the day noted, and a mark of
/// seen, not reached, cleared (D53, amended 24 November 2026), its record
/// released.
fn stand_on(
    world: &World,
    rules: &Content,
    (id, here): (BodyId, Pos),
    mind: &mut Mind,
    (memory, news): (&mut Store, &mut Vec<News>),
) {
    let key = place_of(here, place_side(rules));
    let Some(place) = mind.places.get_mut(&key) else {
        return;
    };
    place.stood = Some(today_u32(world));
    if let Some(mark) = place.barred.take() {
        if let Mark::Held(access) = mark {
            memory.release(access.record);
        }
        mind.barred_new.retain(|(k, _)| *k != key);
        news.push(News::Reached {
            body: id,
            place: key,
            at: world.tick().0,
        });
    }
}

/// The food the body carries that it should eat first: what spoils
/// first, not yet spoiled.
fn carried_food(world: &World, rules: &Content, id: BodyId) -> Option<ItemId> {
    let body = world.body(id)?;
    body.inventory()
        .filter_map(|item| world.item(item))
        .filter_map(|item| {
            let food = rules.material(item.material())?.food?;
            if world.is_spoiled(item.id(), rules) {
                return None;
            }
            let ends =
                item.made().0 + u64::from(food.keeps_days) * world.calendar().ticks_per_day();
            Some((ends, item.id()))
        })
        .min()
        .map(|(_, item)| item)
}

/// The energy of the food the body carries, not spoiled, in kilojoules.
pub fn carried_kj(world: &World, rules: &Content, id: BodyId) -> u64 {
    world.body(id).map_or(0, |body| {
        body.inventory()
            .filter_map(|item| world.item(item))
            .filter(|item| !world.is_spoiled(item.id(), rules))
            .filter_map(|item| {
                let food = rules.material(item.material())?.food?;
                Some(u64::from(food.energy_kj(item.mass_g())))
            })
            .sum()
    })
}

/// The body goes looking for what it needs ([`explore`]), unless its
/// last leg went nowhere: hemmed in, it waits for the next change (the
/// night, the day, a need growing pressing) rather than ask leg after leg
/// at once, each one out of a wider ring.
fn go_looking(
    map: &Map,
    rules: &Content,
    id: BodyId,
    here: Pos,
    mind: &mut Mind,
    seed: u64,
    need: Why,
) -> Option<Decision> {
    if matches!(mind.doing, Doing::Exploring(_)) && mind.stopped && mind.set_off_from == Some(here)
    {
        mind.doing = Doing::Nothing;
        return None;
    }
    mind.set_off_from = Some(here);
    Some(explore(map, rules, id, here, mind, seed, need))
}

/// The body goes looking for what it needs: a spiral out of its camp,
/// eight legs a ring, each ring a sight's length wider than the last (the
/// first direction chosen by the seed and the body), clamped to the map;
/// from each leg's end it looks, so the rings it walks sweep the land.
fn explore(
    map: &Map,
    rules: &Content,
    id: BodyId,
    here: Pos,
    mind: &mut Mind,
    seed: u64,
    need: Why,
) -> Decision {
    let first = usize::try_from(mix(seed ^ u64::from(id.0)) % 8).expect("under eight");
    let turn = usize::try_from(mind.walks % 8).expect("under eight");
    let ring = i32::try_from(mind.walks / 8 + 1).unwrap_or(i32::MAX);
    let (dx, dy) = DIRECTIONS[(first + turn) % 8];
    mind.walks += 1;
    let reach = i32::try_from(
        map.get(here)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain)),
    )
    .unwrap_or(1);
    let (w, h) = (
        i32::try_from(map.width()).expect("fits") - 1,
        i32::try_from(map.height()).expect("fits") - 1,
    );
    let camp = mind.ways.home;
    let out = reach.saturating_mul(ring);
    let target = Pos::new(
        camp.x.saturating_add(dx.saturating_mul(out)).clamp(0, w),
        camp.y.saturating_add(dy.saturating_mul(out)).clamp(0, h),
    );
    mind.doing = Doing::Exploring(need);
    Decision {
        intention: Intention::MoveTo { body: id, target },
        why: Why::Explore,
    }
}

/// Halfway between two values, rounded down: a copy that lost a step
/// (D50): 32 copying 64 lands on 48, a search one and a half times as
/// wide instead of twice.
fn halfway(own: u64, theirs: u64) -> u64 {
    own.midpoint(theirs)
}

/// The eight directions, east first, turning clockwise on the screen.
const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// `id` starts a trial on `day` (D50): one way moved by one step, drawn
/// from the world's seed, its id and the day: the worth distance twice or
/// half, the night out flipped, the store twice or half, or the home set
/// at the place it lived best ([`lived_home`]). A child (under sixteen)
/// does not try its home: it takes its mother's every night; a body whose
/// home can go nowhere (no place it lived beats its home) tries another
/// way that day. The trial records what the days before gave.
#[allow(clippy::too_many_arguments, reason = "one trial's start")]
fn start_trial(
    world: &World,
    rules: &Content,
    (memory, seed): (&Store, u64),
    id: BodyId,
    mind: &mut Mind,
    child: bool,
    day: u64,
) -> Option<News> {
    let trials = &rules.needs.trials;
    // A kept way it heard at the fire comes first (D55): "I heard it
    // worked for N, I try it", the way as told, a step lost at times.
    if let Some((way, theirs)) = heard_candidate(memory, seed, id, mind, child) {
        let before = mind.ways;
        copy_way(mind, way, &theirs, seed, &[u64::from(id.0), day], trials);
        if mind.ways != before {
            mind.trial = Some(Trial {
                way,
                before,
                started_day: day,
                before_days: mind
                    .lately(trials.days_for(way.key(), world.calendar().days_per_year())),
                heard: HeardDays::default(),
            });
            return Some(News::Trying {
                body: id,
                way,
                before,
                after: mind.ways,
                heard: true,
                at: world.tick().0,
            });
        }
    }
    // The candidates, each weighed by the lessons it heard (D50,
    // amended): a weight, never a ban.
    let r = scatter::roll(seed, purpose::TRIAL_WAY, &[u64::from(id.0), day]);
    let mut candidates = lessons(rules, memory, mind, day, child);
    let draw = |candidates: &[(Way, bool, u64)], r: u64| {
        let total: u64 = candidates.iter().map(|c| c.2).sum();
        let mut at = r % total.max(1);
        for &(way, up, weight) in candidates {
            if at < weight {
                return (way, up);
            }
            at -= weight;
        }
        let last = candidates.last().expect("a candidate at least");
        (last.0, last.1)
    };
    let (mut way, mut up) = draw(&candidates, r);
    let mut home = None;
    if way == Way::Home {
        home = lived_home(world, rules, mind);
        if home.is_none() {
            candidates.retain(|c| c.0 != Way::Home);
            (way, up) = draw(&candidates, r >> 8);
        }
    }
    let before = mind.ways;
    match way {
        Way::WorthHalfTiles => {
            mind.ways.worth_half_tiles = if up {
                before.worth_half_tiles.saturating_mul(2)
            } else {
                (before.worth_half_tiles / 2).max(1)
            };
        }
        Way::SleepOut => mind.ways.sleep_out = !before.sleep_out,
        Way::StockKj => {
            mind.ways.stock_kj = if up {
                before.stock_kj.saturating_mul(2)
            } else {
                (before.stock_kj / 2).max(1)
            };
        }
        Way::Home => mind.ways.home = home.expect("drawn only with a home to go to"),
    }
    if mind.ways == before {
        // Nothing moved (a step that stays, at 1): no trial.
        return None;
    }
    mind.trial = Some(Trial {
        way,
        before,
        started_day: day,
        before_days: mind.lately(trials.days_for(way.key(), world.calendar().days_per_year())),
        heard: HeardDays::default(),
    });
    Some(News::Trying {
        body: id,
        way,
        before,
        after: mind.ways,
        heard: false,
        at: world.tick().0,
    })
}

/// The steps `mind` may try on `day`, each with its weight (D50, amended
/// by the lessons of D55): the worth distance twice or half, the night
/// out or home, the store twice or half, the home moved (not for a
/// child). Each starts at 100; each trial heard at the fire of the same
/// change (the same way, the same direction) adds `lesson_weight` if it
/// was kept and takes it away if it was given up, whole for
/// `lesson_days`, half for as long again, nothing after, scaled by how
/// much a body of this will heeds the lessons (`lesson_heed_by_will`);
/// never under `lesson_floor`: a weight, never a ban.
fn lessons(
    rules: &Content,
    memory: &Store,
    mind: &Mind,
    day: u64,
    child: bool,
) -> Vec<(Way, bool, u64)> {
    let trials = &rules.needs.trials;
    let heed = trials.heed_percent(mind.will);
    let mut candidates: Vec<(Way, bool, u64)> = vec![
        (Way::WorthHalfTiles, true, 100),
        (Way::WorthHalfTiles, false, 100),
        (Way::SleepOut, !mind.ways.sleep_out, 100),
        (Way::StockKj, true, 100),
        (Way::StockKj, false, 100),
    ];
    if !child {
        candidates.push((Way::Home, true, 100));
    }
    if trials.lesson_weight == 0 || trials.lesson_days == 0 {
        return candidates;
    }
    let floor = u64::from(trials.lesson_floor);
    for access in &mind.heard_ways {
        let Some(Record::Trial {
            way,
            up,
            kept,
            day: rec_day,
            ..
        }) = memory.get(access.record)
        else {
            continue;
        };
        let age = day.saturating_sub(u64::from(*rec_day));
        let whole = u64::from(trials.lesson_days);
        let factor = if age < whole {
            100
        } else if age < 2 * whole {
            50
        } else {
            0
        };
        let delta = u64::from(trials.lesson_weight) * factor / 100 * heed / 100;
        if let Some(c) = candidates.iter_mut().find(|c| c.0 == *way && c.1 == *up) {
            c.2 = if *kept {
                c.2 + delta
            } else {
                c.2.saturating_sub(delta).max(floor)
            };
        }
    }
    candidates
}

/// The kept way `id` heard that it would try first (D55): among the
/// trial records it holds an access to, kept and differing from its own
/// ways (not the home for a child), the one whose teller fared best
/// during its trial, its credit next, the smaller record at a tie: the
/// way and the teller's ways to take it from.
fn heard_candidate(
    memory: &Store,
    seed: u64,
    id: BodyId,
    mind: &Mind,
    child: bool,
) -> Option<(Way, Ways)> {
    mind.heard_ways
        .iter()
        .filter_map(|access| {
            let Record::Trial {
                body: teller,
                way,
                after,
                kept: true,
                will,
                during,
                day: rec_day,
                ..
            } = memory.get(access.record)?
            else {
                return None;
            };
            if way.same(&mind.ways, after) || (child && *way == Way::Home) {
                return None;
            }
            let credit = memory::credit(
                seed,
                (id, mind.will),
                (*teller, *will),
                *rec_day,
                access.day,
            );
            Some((
                (during.fared(), credit, std::cmp::Reverse(access.record)),
                *way,
                *after,
            ))
        })
        .max_by_key(|(rank, ..)| *rank)
        .map(|(_, way, after)| (way, after))
}

/// The grid of the groves (D53, amended 24 November 2026): gathering is
/// noted over `trials.grove_tiles`, so a grove gathered here and there
/// adds up as the water drunk at again and again.
fn grove_side(rules: &Content) -> i32 {
    i32::try_from(rules.needs.trials.grove_tiles.max(1)).unwrap_or(i32::MAX)
}

/// How a body would fare living at `l`, by its own notes (D50, D53,
/// amended 24 November 2026), in per mille of a day of need, every
/// [`Sense`] counted alike: what the places of its grove gave a day, each
/// sense up to a day's worth, less what the nights there (its own, else
/// the most slept place of its grove) left it lacking and what its days'
/// walking cost. A place it never slept near takes the home's nights
/// (`home`), their walking less what living there would have saved.
fn fare(l: &Lived, notes: &[Lived], (side, grove): (i32, i32), home: Option<&Lived>) -> i64 {
    let grove_of = |n: &Lived| {
        if n.grove {
            n.place
        } else {
            place_of(place_centre(n.place, side), grove)
        }
    };
    let mine = grove_of(l);
    let near = || notes.iter().filter(move |n| grove_of(n) == mine);
    let gains: u64 = (0..SENSES)
        .map(|s| near().map(|n| n.per_day(n.gains[s])).sum::<u64>().min(1000))
        .sum();
    let slept = if l.nights >= 1000 {
        Some(l)
    } else {
        near()
            .filter(|n| n.nights >= 1000)
            .max_by_key(|n| (n.nights, n.place))
    };
    let night = |n: &Lived| n.losses.iter().map(|&v| n.per_night(v)).sum::<u64>();
    let losses = match (slept, home) {
        (Some(n), _) => night(n) + n.per_night(n.walk),
        (None, Some(h)) => night(h) + h.per_night(h.walk).saturating_sub(l.per_day(l.saved)),
        (None, None) => 0,
    };
    i64::try_from(gains).unwrap_or(i64::MAX) - i64::try_from(losses).unwrap_or(i64::MAX)
}

/// Where a trial sets `id`'s home (D50, amended 23 and 24 November 2026,
/// the project lead: "ils doivent se souvenir des endroits où vivre et
/// choisir eux-mêmes", and the home at the optimal place for good): at
/// the place or grove of its notebook where it would [`fare`] best, by a
/// hundredth of a day at least, lived for the nights or days its will
/// asks; the walkable tile nearest its middle. Nothing says water or
/// plants are good: only what its body got, lacked and spent there
/// grades a place. `None` when no place it lived beats its home, or none
/// was lived long enough: no home trial that day.
fn lived_home(world: &World, rules: &Content, mind: &Mind) -> Option<Pos> {
    let sides = (place_side(rules), grove_side(rules));
    let home = place_of(mind.ways.home, sides.0);
    let is_home = |l: &Lived| !l.grove && l.place == home;
    let home_notes = mind.lived.iter().find(|l| is_home(l) && l.nights >= 1000);
    let nights = u64::from(rules.needs.trials.home_nights(mind.will)) * 1000;
    let best = mind
        .lived
        .iter()
        .filter(|l| is_home(l) || l.lived() >= nights)
        .max_by_key(|l| {
            (
                fare(l, &mind.lived, sides, home_notes).div_euclid(10),
                is_home(l),
                !l.grove,
                l.lived(),
                std::cmp::Reverse(l.place),
            )
        })?;
    if is_home(best) {
        return None;
    }
    let side = if best.grove { sides.1 } else { sides.0 };
    let centre = place_centre(best.place, side);
    let map = world.map();
    (0..=side / 2)
        .flat_map(|r| ring(centre, r))
        .find(|&at| map.is_passable(at))
}

/// The tiles at `r` from `centre` on each axis, row by row.
fn ring(centre: Pos, r: i32) -> Vec<Pos> {
    let mut tiles = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx.abs() == r || dy.abs() == r {
                tiles.push(Pos::new(centre.x + dx, centre.y + dy));
            }
        }
    }
    tiles
}

/// Which of the eight [`DIRECTIONS`] `(dx, dy)` points to: by the signs,
/// a diagonal when the lesser leg is at least half the greater.
fn octant(dx: i32, dy: i32) -> usize {
    let (ax, ay) = (i64::from(dx).abs(), i64::from(dy).abs());
    let diagonal = ax.min(ay) * 2 >= ax.max(ay);
    let (sx, sy) = (dx.signum(), dy.signum());
    let (ux, uy) = if diagonal {
        (sx, sy)
    } else if ax >= ay {
        (sx, 0)
    } else {
        (0, sy)
    };
    DIRECTIONS
        .iter()
        .position(|&d| d == (ux, uy))
        .expect("one of eight")
}

/// `SplitMix64`'s finaliser: a seed spread over 64 bits.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gauge;
    use crate::time::Calendar;

    fn rules() -> Content {
        Content::sample()
    }

    /// A field `side` tiles of grass, the camp at (5, 5), a river down
    /// column `river` (none when it is off the map), one body at `at`.
    fn field(side: u32, river: i32, at: Pos) -> World {
        let mut map = Map::filled(side, side, Terrain::Grass).with_camp(5, 5);
        for y in 0..i32::try_from(side).unwrap() {
            map.set(Pos::new(river, y), Terrain::Water);
        }
        let mut world = World::empty(Calendar::new(1, 7200).unwrap(), map);
        world.spawn_body(at);
        world
    }

    /// Runs `ticks` ticks with the brain piloting every body: each tick's
    /// events go to the brain, its decisions are queued for the next.
    /// Gives back every event and every decision.
    fn run(
        world: &mut World,
        brains: &mut Brains,
        ticks: u64,
    ) -> (Vec<Event>, Vec<(BodyId, Decision)>) {
        let rules = rules();
        let (mut all, mut decided) = (Vec::new(), Vec::new());
        let mut events = Vec::new();
        for _ in 0..ticks {
            events.clear();
            world.step(&rules, &mut events);
            for (body, decision) in brains.think(world, &rules, &events) {
                world.queue(decision.intention.clone());
                decided.push((body, decision));
            }
            all.extend(events.iter().cloned());
        }
        (all, decided)
    }

    /// `look` with nothing placed or found beforehand.
    fn look_alone(world: &World, rules: &Content, at: Pos, mind: &mut Mind) {
        let (mut bearers, mut shores) = (Placed::new(), Lru::new(BEARER_CHUNKS));
        let mut food = food_now(world, rules);
        let (mut memory, mut news) = (Store::new(), Vec::new());
        let mut land = Land {
            groups: &mut BTreeMap::new(),
            bearers: &mut bearers,
            shores: &mut shores,
            food: &mut food,
            memory: &mut memory,
            news: &mut news,
        };
        look(world, rules, BodyId(1), at, mind, &mut land);
    }

    /// `decide` with nothing placed beforehand, what each kind bears now.
    fn decide_alone(
        world: &World,
        rules: &Content,
        id: BodyId,
        mind: &mut Mind,
    ) -> Option<Decision> {
        let (mut bearers, mut shores) = (Placed::new(), Lru::new(BEARER_CHUNKS));
        let mut food = food_now(world, rules);
        let (mut memory, mut news) = (Store::new(), Vec::new());
        let mut land = Land {
            groups: &mut BTreeMap::new(),
            bearers: &mut bearers,
            shores: &mut shores,
            food: &mut food,
            memory: &mut memory,
            news: &mut news,
        };
        decide(world, rules, &mut land, id, mind, 7)
    }

    /// A place known to hold `count` of `kind`, all seen holding food,
    /// the fullest giving what an hour of a full one gives.
    fn place_with(food: &Food, kind: u16, count: u16) -> Place {
        Place {
            kinds: vec![Seen {
                kind,
                plants: count,
                holding: count,
                best_kj: food
                    .now(kind)
                    .map_or(food.best_kj[usize::from(kind)], |b| b.kj),
            }],
            spent: Vec::new(),
            not_before: None,
            access: None,
            looked_day: None,
            rumour_kj: 0,
            barred: None,
            stood: None,
        }
    }

    /// What each kind bears at `world`'s tick.
    fn food_now(world: &World, rules: &Content) -> Food {
        let mut food = Food::default();
        food.refresh(world, rules);
        food
    }

    fn thirst(world: &mut World, points: u32) {
        world.bodies[0].condition.thirst = Gauge::at(points);
    }

    fn piloted(world: &World) -> Brains {
        let mut brains = Brains::new(7);
        let rules = rules();
        for body in world.bodies() {
            brains.adopt(world, &rules, body.id());
        }
        brains
    }

    /// Runs the valley `world` for `ticks` ticks, the brain piloting.
    fn run_valley(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        ticks: u64,
    ) -> (Vec<Event>, Vec<(BodyId, Decision)>) {
        let (mut all, mut decided) = (Vec::new(), Vec::new());
        let mut events = Vec::new();
        for _ in 0..ticks {
            events.clear();
            world.step(rules, &mut events);
            for (body, decision) in brains.think(world, rules, &events) {
                world.queue(decision.intention.clone());
                decided.push((body, decision));
            }
            all.extend(events.iter().cloned());
        }
        (all, decided)
    }

    /// The valley at midday of day 28, blackberries ripe, and a body
    /// born beside a bramble, with `hunger`, piloted.
    fn by_a_ripe_bramble(hunger: u32) -> (World, Content, BodyId, Brains) {
        use crate::crops::tests::{body_by, run_to, valley_with};
        let (mut world, rules, _, bramble) = valley_with("bramble");
        run_to(&mut world, &rules, 28);
        let id = body_by(&mut world, &bramble);
        let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
        // Its reserve short of `hunger` points' worth: as hungry.
        body.condition.deficit_j =
            u64::from(hunger) * u64::from(rules.needs.kj_per_hunger_point) * 1000;
        body.condition.hunger = Gauge::at(hunger);
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, id);
        near_sighted(&mut brains, id);
        (world, rules, id, brains)
    }

    /// Makes `id` weigh nearness hard (a plant's worth halved at a tile):
    /// the bramble beside it stays its choice, while the valley holds
    /// richer plants a short walk away that the default way (halved at 32
    /// tiles, [`worth_is_halved_at_half_tiles_not_at_their_square`]) would
    /// send it to. The tests on the bramble are about hunger, stores and
    /// night, not about the choice of plant.
    fn near_sighted(brains: &mut Brains, id: BodyId) {
        brains.minds.get_mut(&id).unwrap().ways.worth_half_tiles = 1;
    }

    #[test]
    fn a_hungry_body_gathers_the_ripe_bramble_beside_it_and_eats() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(60);
        let (events, decided) = run_valley(&mut world, &rules, &mut brains, 30);
        assert!(
            matches!(
                decided[0].1,
                Decision { intention: Intention::Gather { ref gives, .. }, why: Why::Hunger }
                    if gives == "blackberries"
            ),
            "it gathers first: {:?}",
            decided[0]
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::WorkStopped { body, grams, .. }
                if *body == id && *grams > 0)),
            "it picked some"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyAte { body, .. } if *body == id)),
            "then ate what it carries: {decided:?}"
        );
    }

    #[test]
    fn by_day_carrying_little_it_gathers_a_store() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        assert_eq!(carried_kj(&world, &rules, id), 0);
        let (_, decided) = run_valley(&mut world, &rules, &mut brains, 30);
        assert!(
            matches!(
                decided[0].1,
                Decision {
                    intention: Intention::Gather { .. },
                    why: Why::Gather
                }
            ),
            "{:?}",
            decided[0]
        );
        assert!(
            carried_kj(&world, &rules, id) > 0,
            "it carries what it picked"
        );
    }

    #[test]
    fn it_does_not_ask_a_plant_holding_nothing_and_a_refused_gather_waits_an_hour() {
        use crate::Refusal;
        use crate::crops::tests::{body_by, run_to, valley_with};
        let (mut world, rules, chunk, bramble) = valley_with("bramble");
        // Day 21: the blackberries still ripen. Picked bare, the bramble
        // has nothing ripe, yet it is not spent: more comes. (Picked well
        // past bare: on this short calendar a tick ripens grams, on the
        // game's a dandelion ripens a gram in some minutes.)
        run_to(&mut world, &rules, 21);
        let crop = rules
            .crops
            .iter()
            .find(|c| c.gives == "blackberries")
            .unwrap();
        assert!(crops::ripening(crop, world.calendar(), world.tick()));
        let year = world.ripe_on(chunk, &bramble)[0].year;
        world.record_pick(chunk, bramble.source, "blackberries", year, 1_000_000);
        assert_eq!(world.ripe_on(chunk, &bramble), []);
        let id = body_by(&mut world, &bramble);
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, id);
        near_sighted(&mut brains, id);
        let ticks = hour(&world);
        let (events, decided) = run_valley(&mut world, &rules, &mut brains, ticks);
        let asked = |d: &Decision| matches!(&d.intention, Intention::Gather { source, .. } if *source == bramble.source);
        // Since 21 November 2026 it reads what is left on a plant before
        // walking to it: a bare one is not asked, so nothing is refused.
        assert!(
            !decided.iter().any(|(_, d)| asked(d)),
            "not asked while it holds nothing: {decided:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::GatherRefused { .. })),
            "nothing to refuse"
        );
        let side = place_side(&rules);
        let place = &brains.mind(id).unwrap().places[&place_of(bramble.at.tile(), side)];
        assert!(
            place.spent.is_empty(),
            "the place is not spent: more comes, it comes back to it"
        );
        // A gather the world refuses all the same (a load full as it
        // began, a crumb picked by another just before): not asked again
        // before a later hour.
        let mind = brains.minds.get_mut(&id).unwrap();
        let key = (chunk, bramble.source);
        mind.doing = Doing::Gathering(Aim {
            key,
            at: bramble.at,
            kind: brains.food.numbers["bramble"],
        });
        let refused = Event::GatherRefused {
            body: id,
            gives: "blackberries".to_owned(),
            reason: Refusal::NothingRipe,
        };
        brains.notice(&[refused], &world);
        let mind = brains.mind(id).unwrap();
        assert!(mind.left_alone[&key] > world.tick().0);
        // A few are held at most: the soonest free forgotten first.
        let mut mind = mind.clone();
        for i in 0..LEFT_ALONE {
            let key = (
                chunk,
                Source::Plant(crate::crops::PlantKey {
                    x_mm: u16::try_from(i).unwrap(),
                    y_mm: 1,
                }),
            );
            mind.leave_alone(key, 1_000_000 + u64::try_from(i).unwrap());
        }
        assert_eq!(mind.left_alone.len(), LEFT_ALONE);
        assert!(
            !mind.left_alone.contains_key(&key),
            "the bramble, soonest free, went"
        );
    }

    #[test]
    fn it_leaves_a_plant_holding_crumbs_and_ranks_the_rest_by_what_is_left() {
        // An hour of a full dandelion gives 1,500 g; ten minutes, 250 g.
        assert_eq!(kj_of_an_hour(2850, 1500, 249), None, "crumbs");
        assert_eq!(kj_of_an_hour(2850, 1500, 250), Some(475), "ten minutes");
        assert_eq!(kj_of_an_hour(2850, 1500, 750), Some(1425), "half an hour");
        assert_eq!(
            kj_of_an_hour(2850, 1500, 9000),
            Some(2850),
            "an hour at most"
        );
        // Standing at the bramble's place, it picks the plant there worth
        // most by what is left on it; picked down to crumbs, that one is
        // left alone for the next; all bare, the place is spent for the
        // year (the blackberries ripe, nothing more comes).
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let side = place_side(&rules);
        let key = place_of(here, side);
        let food = food_now(&world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.places
            .insert(key, place_with(&food, food.numbers["bramble"], 1));
        let mut picked = Vec::new();
        while let Some((target, _)) = pick_in_place(
            &world,
            &rules,
            &food,
            &mut brains.bearers,
            mind,
            &[],
            key,
            here,
            0,
        ) {
            assert!(
                !picked.contains(&target.aim.key),
                "picked bare, a plant is left alone: {picked:?}"
            );
            assert!(at_place(target.aim.at.tile(), key, side));
            let (year, left) = world
                .ripe_of_crop(
                    target.aim.key.0,
                    target.aim.key.1,
                    target.aim.at,
                    food.now(target.aim.kind).unwrap().crop,
                )
                .unwrap();
            world.record_pick(target.aim.key.0, target.aim.key.1, target.gives, year, left);
            picked.push(target.aim.key);
            assert!(picked.len() < 1000);
        }
        assert!(
            !picked.is_empty(),
            "it picked the place's plants one by one"
        );
        let place = &mind.places[&key];
        let bramble = food.numbers["bramble"];
        assert!(
            place.is_spent(bramble, food.now(bramble).unwrap().year),
            "all bare: spent for the year, {place:?}"
        );
        assert!(
            place_to_gather(&world, &food, here, mind, 0, side, None).map(|p| p.0) != Some(key),
            "not chosen again this year"
        );
    }

    #[test]
    fn hungry_by_night_it_sleeps_unless_its_reserve_is_weak() {
        use crate::crops::tests::{body_by, run_to, valley_with};
        let night_by_the_bramble = |deficit_percent: u64| {
            let (mut world, rules, _, bramble) = valley_with("bramble");
            run_to(&mut world, &rules, 28);
            let mut events = Vec::new();
            for _ in 0..world.calendar().ticks_per_day() {
                events.clear();
                world.step(&rules, &mut events);
                if events.iter().any(|e| matches!(e, Event::NightFell { .. })) {
                    break;
                }
            }
            assert!(world.is_night(&rules), "night fell");
            let id = body_by(&mut world, &bramble);
            let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
            body.condition.deficit_j =
                body.condition.reserve_j(&rules.needs) * deficit_percent / 100;
            body.condition.hunger = Gauge::at(HUNGRY + 10);
            let mut brains = Brains::new(7);
            brains.adopt(&world, &rules, id);
            near_sighted(&mut brains, id);
            let ticks = hour(&world);
            let (_, decided) = run_valley(&mut world, &rules, &mut brains, ticks);
            decided
        };
        // Hungry with half its reserve left: it sleeps on it.
        let decided = night_by_the_bramble(50);
        assert!(
            !decided.iter().any(|(_, d)| d.why == Why::Hunger),
            "no gathering by night: {decided:?}"
        );
        assert!(
            decided.iter().any(|(_, d)| d.why == Why::Night),
            "it goes home or sleeps: {decided:?}"
        );
        // Weak, under a fifth of its reserve: it gathers the bramble by night.
        let decided = night_by_the_bramble(90);
        assert!(
            decided
                .iter()
                .any(|(_, d)| d.why == Why::Hunger
                    && matches!(d.intention, Intention::Gather { .. })),
            "weak, it gathers by night: {decided:?}"
        );
    }

    #[test]
    fn a_body_whose_load_is_full_does_not_choose_to_gather() {
        for hunger in [0, 60] {
            let (mut world, rules, id, mut brains) = by_a_ripe_bramble(hunger);
            let body = world.bodies.iter_mut().find(|b| b.id() == id).unwrap();
            body.load_g = body.capacity_g(&rules.movement);
            let (events, decided) = run_valley(&mut world, &rules, &mut brains, 30);
            assert!(
                !decided
                    .iter()
                    .any(|(_, d)| matches!(d.intention, Intention::Gather { .. })),
                "hunger {hunger}: {decided:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::GatherRefused { .. })),
                "hunger {hunger}"
            );
        }
    }

    #[test]
    fn the_dawn_wakes_the_minds_one_after_the_other() {
        let mut world = field(64, 40, Pos::new(5, 5));
        for x in 6..12 {
            world.spawn_body(Pos::new(x, 6));
        }
        let rules = rules();
        let mut brains = piloted(&world);
        run(&mut world, &mut brains, 20);
        let thoughts = |b: &Brains| -> BTreeMap<BodyId, u64> {
            b.minds.iter().map(|(id, m)| (*id, m.thoughts)).collect()
        };
        let before = thoughts(&brains);
        let dawn = world.tick().0;
        let spread = WAKE_SPREAD_SECONDS * u64::from(world.calendar().ticks_per_second());
        brains.think(&world, &rules, &[Event::DayStarted { day: 2 }]);
        assert_eq!(thoughts(&brains), before, "none in the dawn's own tick");
        let mut woke: BTreeMap<BodyId, u64> = BTreeMap::new();
        let mut events = Vec::new();
        for _ in 0..spread {
            world.advance(1, &rules, &mut events);
            brains.think(&world, &rules, &[]);
            for (id, n) in thoughts(&brains) {
                if n > before[&id] {
                    woke.entry(id).or_insert(world.tick().0 - dawn);
                }
            }
        }
        for id in before.keys() {
            assert_eq!(
                woke.get(id),
                Some(&(u64::from(id.0) % spread)),
                "{id:?} thinks in its own tick"
            );
        }
    }

    #[test]
    fn new_land_is_placed_a_few_chunks_a_turn_and_learnt_whole() {
        use crate::crops::tests::{body_by, run_to, valley_with};
        let (mut world, mut rules, _, bramble) = valley_with("bramble");
        // Plants seen as far as the terrain's sight (60 on grass, 25
        // chunks): more than a turn places (D53's 30 tiles see nine).
        rules.needs.thought.plant_sight_tiles = 60;
        run_to(&mut world, &rules, 28);
        let id = body_by(&mut world, &bramble);
        let at = world.body(id).unwrap().pos();
        let mut brains = Brains::new(7);
        // Not adopted: nothing in sight placed beforehand.
        brains.minds.insert(id, Mind::new(world.map().camp()));
        let per_turn = usize::try_from(cold_chunks_at(&world, world.tick().0)).unwrap();
        assert!(per_turn > 0);
        brains.think(&world, &rules, &[]);
        assert_eq!(brains.bearers.chunks.len(), per_turn);
        let unseen = brains.minds[&id].unseen.len();
        assert!(unseen > 0, "the rest waits");
        let mut turns = 0;
        while !brains.minds[&id].unseen.is_empty() {
            brains.think(&world, &rules, &[]);
            turns += 1;
            assert!(brains.bearers.chunks.len() <= per_turn * (turns + 1));
        }
        assert_eq!(turns, unseen.div_ceil(per_turn));
        assert_eq!(brains.minds[&id].thoughts, 1, "learnt without a thought");
        // It knows what one look placing everything at once knows.
        let mut all = Placed::new();
        for chunk in brains.bearers.chunks.keys() {
            all.warm(&world, &brains.food, *chunk);
        }
        let mut whole = Mind::new(world.map().camp());
        let (mut shores, mut food) = (Lru::new(BEARER_CHUNKS), brains.food.clone());
        let (mut memory, mut news) = (Store::new(), Vec::new());
        let mut land = Land {
            groups: &mut BTreeMap::new(),
            bearers: &mut all,
            shores: &mut shores,
            food: &mut food,
            memory: &mut memory,
            news: &mut news,
        };
        look(&world, &rules, id, at, &mut whole, &mut land);
        assert!(!whole.places.is_empty());
        // Standing on a place is noted by the thought, not by the look.
        let mut learnt = brains.minds[&id].places.clone();
        for place in learnt.values_mut() {
            place.stood = None;
        }
        assert_eq!(whole.places, learnt);
    }

    #[test]
    fn plants_are_seen_no_farther_than_the_thoughts_sight_water_at_the_terrains() {
        use crate::crops::tests::{body_by, run_to, valley_with};
        let (mut world, mut rules, _, bramble) = valley_with("bramble");
        run_to(&mut world, &rules, 28);
        let id = body_by(&mut world, &bramble);
        let at = world.body(id).unwrap().pos();
        let side = place_side(&rules);
        // Does a place's square touch the square of `tiles` around `at`?
        let touches = |key: &PlaceKey, tiles: i32| {
            let (x0, y0) = (key.0 * side, key.1 * side);
            x0 < at.x + tiles + 1
                && x0 + side > at.x - tiles
                && y0 < at.y + tiles + 1
                && y0 + side > at.y - tiles
        };
        let short = i32::try_from(rules.needs.thought.plant_sight_tiles).unwrap();
        let sight = sight_at(world.map(), &rules, at);
        assert!(
            short < sight,
            "the test wants a sight shorter than the terrain's"
        );
        let mut near = Mind::new(world.map().camp());
        look_alone(&world, &rules, at, &mut near);
        assert!(!near.places.is_empty());
        assert!(
            near.places.keys().all(|key| touches(key, short)),
            "a place seen past the thought's sight"
        );
        // With the terrain's sight for plants too, it sees farther.
        rules.needs.thought.plant_sight_tiles = u32::try_from(sight).unwrap();
        let mut far_sighted = Mind::new(world.map().camp());
        look_alone(&world, &rules, at, &mut far_sighted);
        assert!(
            far_sighted.places.keys().any(|key| !touches(key, short)),
            "nothing seen between 30 and 60 tiles"
        );
        // The water is seen at the terrain's sight either way.
        assert_eq!(near.water, far_sighted.water);
    }

    #[test]
    fn the_brains_hold_the_plants_of_a_bounded_land() {
        let world = field(64, 40, Pos::new(5, 5));
        let rules = rules();
        let mut placed = Placed::new();
        let mut food = Food::default();
        food.ready(&rules);
        let many = i32::try_from(BEARER_CHUNKS).unwrap() + 10;
        for x in 0..many {
            placed.warm(&world, &food, ChunkPos::new(x, 0));
        }
        assert_eq!(placed.chunks.len(), BEARER_CHUNKS);
        assert!(
            !placed.chunks.contains(&ChunkPos::new(0, 0)),
            "the least lately used went first"
        );
        assert!(placed.chunks.contains(&ChunkPos::new(many - 1, 0)));
    }

    #[test]
    fn worth_is_halved_at_half_tiles_not_at_their_square() {
        // 32 tiles off, the worth is halved; 6 tiles off (whose square
        // is near 32) it is barely dented.
        assert_eq!(worth(1000, far(Pos::new(0, 0), Pos::new(32, 0)), 32), 500);
        assert_eq!(worth(1000, far(Pos::new(0, 0), Pos::new(6, 0)), 32), 842);
        // Twice as far (64 tiles) is worth a third, not a fifth.
        assert_eq!(worth(1000, far(Pos::new(0, 0), Pos::new(64, 0)), 32), 333);
        // A way reaching twice as far halves at 64 tiles.
        assert_eq!(worth(1000, far(Pos::new(0, 0), Pos::new(64, 0)), 64), 500);
    }

    #[test]
    fn it_goes_to_the_richer_place_unless_it_is_too_far() {
        let (world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let side = place_side(&rules);
        let own = place_of(here, side);
        let east = |places: i32| (own.0 + places, own.1);
        let mut mind = Mind::new(here);
        let food = food_now(&world, &rules);
        let (bramble, hazel) = (food.numbers["bramble"], food.numbers["hazel"]);
        mind.places.insert(own, place_with(&food, bramble, 3));
        mind.places.insert(east(2), place_with(&food, hazel, 1));
        // Blackberries give 1,800 kJ an hour, hazelnuts 52,000: the hazel
        // corner two places off is worth the walk.
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 0, side, None).map(|p| p.0),
            Some(east(2))
        );
        // Sixty places off (a thousand tiles), it is not.
        mind.places.remove(&east(2));
        mind.places.insert(east(60), place_with(&food, hazel, 1));
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 0, side, None).map(|p| p.0),
            Some(own)
        );
        // Two bramble corners alike but for their plants: its own,
        // nearest, first; then the two, either.
        mind.places.remove(&east(60));
        mind.places
            .insert((own.0, own.1 + 2), place_with(&food, bramble, 9));
        mind.places
            .insert((own.0, own.1 - 2), place_with(&food, bramble, 3));
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 0, side, None).map(|p| p.0),
            Some(own),
            "its own, nearest"
        );
        mind.places.remove(&own);
        let chosen = place_to_gather(&world, &food, here, &mind, 0, side, None)
            .map(|p| p.0)
            .unwrap();
        assert!(
            chosen == (own.0, own.1 + 2) || chosen == (own.0, own.1 - 2),
            "{chosen:?}"
        );
        // Spent for the year, a place is left; left alone for now, too.
        let year = food.now(bramble).unwrap().year;
        mind.places.get_mut(&chosen).unwrap().spend(bramble, year);
        let other = place_to_gather(&world, &food, here, &mind, 0, side, None)
            .map(|p| p.0)
            .unwrap();
        assert_ne!(other, chosen);
        mind.places.get_mut(&other).unwrap().not_before = Some(10);
        mind.places.insert(east(60), place_with(&food, hazel, 1));
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 0, side, None).map(|p| p.0),
            Some(east(60)),
            "the far hazel, all that is left"
        );
        mind.places.remove(&east(60));
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 10, side, None).map(|p| p.0),
            Some(other),
            "the hour over"
        );
    }

    #[test]
    fn it_sticks_to_the_place_it_chose_for_an_hour_unless_it_is_spent() {
        let (mut world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let side = place_side(&rules);
        let own = place_of(here, side);
        let east = |places: i32| (own.0 + places, own.1);
        let mut mind = Mind::new(here);
        let food = food_now(&world, &rules);
        let (bramble, hazel) = (food.numbers["bramble"], food.numbers["hazel"]);
        mind.places.insert(east(2), place_with(&food, bramble, 3));
        let going_to = |world: &World, mind: &mut Mind| {
            decide_alone(world, &rules, id, mind);
            match mind.doing {
                Doing::ToPlace(key) => key,
                other => panic!("not going to a place: {other:?}"),
            }
        };
        // By day with no store, it sets off for the bramble corner.
        assert_eq!(going_to(&world, &mut mind), east(2));
        let chose_at = mind.chose_at.expect("a fresh choice");
        // A hazel corner, far richer, comes to mind: half an hour later
        // it keeps its choice.
        mind.places.insert(east(4), place_with(&food, hazel, 1));
        let mut events = Vec::new();
        let half = hour(&world) / 2;
        world.advance(half, &rules, &mut events);
        assert_eq!(going_to(&world, &mut mind), east(2));
        assert_eq!(mind.chose_at, Some(chose_at), "not chosen again");
        // The hour over, it chooses again, and takes the richer one.
        world.advance(hour(&world) - half, &rules, &mut events);
        assert_eq!(going_to(&world, &mut mind), east(4));
        assert_eq!(mind.chose_at, Some(world.tick().0));
        // Spent for the year, it is left at once for the other.
        let year = food.now(hazel).unwrap().year;
        mind.places.get_mut(&east(4)).unwrap().spend(hazel, year);
        assert_eq!(going_to(&world, &mut mind), east(2));
        // Out of reach today, likewise: nothing left to go to.
        leave_for_today(&world, &mut mind, east(2));
        decide_alone(&world, &rules, id, &mut mind);
        assert!(
            !matches!(mind.doing, Doing::ToPlace(_)),
            "the place is left: {:?}",
            mind.doing
        );
        // Arrived at a place, it picks a plant and keeps it likewise: a
        // body at the bramble's place, going to the bramble.
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        brains.food.refresh(&world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.places
            .insert(place_of(here, side), place_with(&food, bramble, 1));
        let target = pick_in_place(
            &world,
            &rules,
            &brains.food,
            &mut brains.bearers,
            mind,
            &[],
            place_of(here, side),
            here,
            0,
        )
        .expect("a plant at its place")
        .0;
        mind.doing = Doing::ToGather(target.aim);
        mind.chose_at = Some(world.tick().0);
        let mut events = Vec::new();
        world.advance(1, &rules, &mut events);
        let (chosen, fresh) = goal(
            &world,
            &rules,
            &brains.food,
            &mut brains.bearers,
            here,
            mind,
            world.tick().0,
        );
        assert!(!fresh, "kept");
        assert!(matches!(chosen, Some(Goal::Plant(t)) if t.aim.key == target.aim.key));
        // Left alone (a refused gather), it is not kept.
        mind.leave_alone(target.aim.key, world.tick().0 + 100);
        let (_, fresh) = goal(
            &world,
            &rules,
            &brains.food,
            &mut brains.bearers,
            here,
            mind,
            world.tick().0,
        );
        assert!(fresh, "chosen again");
    }

    #[test]
    fn it_forgets_what_bears_out_of_season_before_what_bears_now() {
        let (world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let side = place_side(&rules);
        let own = place_of(here, side);
        let food = food_now(&world, &rules);
        let mut mind = Mind::new(here);
        // Wild garlic all around, an hour of it worth more than of
        // blackberries, but in the spring; one bramble corner bearing
        // now, farther than any.
        let keep = places_remembered(&rules);
        let garlic = food.numbers["wild_garlic"];
        let ring = i32::try_from(keep).unwrap();
        for i in 0..ring {
            mind.places
                .insert((own.0 + i % 8, own.1 + i / 8), place_with(&food, garlic, 2));
        }
        assert_eq!(mind.places.len(), keep);
        let bramble = (own.0 + 20, own.1 + 20);
        let seen = BTreeMap::from([(
            bramble,
            vec![Seen {
                kind: food.numbers["bramble"],
                plants: 1,
                holding: 1,
                best_kj: food.now(food.numbers["bramble"]).unwrap().kj,
            }],
        )]);
        let (mut memory, mut news) = (Store::new(), Vec::new());
        remember_places(
            &food,
            &rules,
            (BodyId(1), here, 0, 0),
            &mut mind,
            seen,
            &mut memory,
            &mut news,
        );
        assert_eq!(mind.places.len(), keep);
        assert!(mind.places.contains_key(&bramble), "the bramble stays");
        assert!(
            !mind.places.contains_key(&(own.0 + 7, own.1 + ring / 8 - 1)),
            "the farthest garlic went"
        );
        assert_eq!(
            place_to_gather(&world, &food, here, &mind, 0, side, None).map(|p| p.0),
            Some(bramble)
        );
    }

    #[test]
    fn stopped_short_of_a_plant_it_leaves_its_place_for_the_day() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let food = food_now(&world, &rules);
        let side = place_side(&rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        let here = world.body(id).unwrap().pos();
        // A bramble corner said far away, out of reach.
        let far_at = FinePos::new(here.centre().x + 40_000, here.centre().y);
        let key = place_of(far_at.tile(), side);
        mind.places.clear();
        mind.places
            .insert(key, place_with(&food, food.numbers["bramble"], 2));
        let aim = Aim {
            key: (
                far_at.tile().chunk(),
                Source::Plant(crate::crops::PlantKey { x_mm: 1, y_mm: 1 }),
            ),
            at: far_at,
            kind: food.numbers["bramble"],
        };
        mind.doing = Doing::ToGather(aim);
        mind.stopped = true;
        stopped_short(&world, &rules, id, mind, aim, &mut Vec::new());
        assert_eq!(mind.doing, Doing::Nothing);
        assert_eq!(mind.out_of_reach.get(&key), Some(&today(&world)));
        assert_eq!(
            place_to_gather(&world, &food, here, mind, 0, side, None).map(|p| p.0),
            None,
            "the place today"
        );
        let next_day = world.tick().0 + world.calendar().ticks_per_day();
        let mut events = Vec::new();
        world.advance(next_day - world.tick().0, &rules, &mut events);
        let food = food_now(&world, &rules);
        let mind = brains.minds.get(&id).unwrap();
        assert_eq!(
            place_to_gather(&world, &food, here, mind, 0, side, None).map(|p| p.0),
            Some(key),
            "the next day, again"
        );
        // Stopped short of a place on the way there, the same.
        let mut mind = mind.clone();
        mind.doing = Doing::ToPlace(key);
        mind.stopped = true;
        decide_alone(&world, &rules, id, &mut mind);
        assert_eq!(mind.out_of_reach.get(&key), Some(&today(&world)));
    }

    #[test]
    fn barred_short_of_a_place_it_is_seen_not_reached_until_it_stands_on_it() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let food = food_now(&world, &rules);
        let side = place_side(&rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        let here = world.body(id).unwrap().pos();
        let far_at = FinePos::new(here.centre().x + 40_000, here.centre().y);
        let key = place_of(far_at.tile(), side);
        mind.places.clear();
        mind.places
            .insert(key, place_with(&food, food.numbers["bramble"], 2));
        let aim = Aim {
            key: (
                far_at.tile().chunk(),
                Source::Plant(crate::crops::PlantKey { x_mm: 1, y_mm: 1 }),
            ),
            at: far_at,
            kind: food.numbers["bramble"],
        };
        // Its walk to a plant there ended barred (an obstacle, no way seen).
        mind.doing = Doing::ToGather(aim);
        mind.stopped = true;
        mind.blocked = Some(Blockage::Water);
        let mut news = Vec::new();
        stopped_short(&world, &rules, id, mind, aim, &mut news);
        let day = u32::try_from(today(&world)).unwrap();
        assert_eq!(mind.places[&key].barred, Some(Mark::Draft(day)));
        assert_eq!(mind.barred_new, vec![(key, day)], "to tell at its fire");
        assert!(matches!(
            news[..],
            [News::Barred {
                cause: Some(Blockage::Water),
                ..
            }]
        ));
        // The next day it is still not chosen, nor called: the mark lasts.
        let next_day = world.tick().0 + world.calendar().ticks_per_day();
        let mut events = Vec::new();
        world.advance(next_day - world.tick().0, &rules, &mut events);
        let food = food_now(&world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert_eq!(
            place_to_gather(&world, &food, here, mind, 0, side, None),
            None
        );
        assert!(!place_open(&world, &food, mind, key, 0));
        let camp = mind.ways.home;
        assert_eq!(choices(&food, &rules, &world, mind, id, camp, 100).len(), 0);
        // Standing on it clears the mark: it is chosen again.
        let mut news = Vec::new();
        stand_on(
            &world,
            &rules,
            (id, far_at.tile()),
            mind,
            (&mut Store::new(), &mut news),
        );
        assert_eq!(mind.places[&key].barred, None);
        assert!(mind.barred_new.is_empty(), "a mark cleared is not told");
        assert!(matches!(news[..], [News::Reached { .. }]));
        assert_eq!(
            place_to_gather(&world, &food, here, mind, 0, side, None).map(|p| p.0),
            Some(key)
        );
        // Barred on its way to the place itself, the same; merely stopped
        // (arrived short, nothing barring), only the day.
        let mut barred = mind.clone();
        barred.doing = Doing::ToPlace(key);
        barred.stopped = true;
        barred.blocked = Some(Blockage::Rock);
        decide_alone(&world, &rules, id, &mut barred);
        assert!(barred.places[&key].barred.is_some());
        let mut stopped = mind.clone();
        stopped.doing = Doing::ToPlace(key);
        stopped.stopped = true;
        stopped.blocked = None;
        decide_alone(&world, &rules, id, &mut stopped);
        assert_eq!(stopped.places[&key].barred, None);
        assert_eq!(stopped.out_of_reach.get(&key), Some(&today(&world)));
    }

    #[test]
    fn a_mark_is_told_like_a_denial_heard_by_who_holds_the_place_and_cleared_only_by_its_own_feet()
    {
        let (world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let food = food_now(&world, &rules);
        let side = place_side(&rules);
        let here = world.body(id).unwrap().pos();
        let key = place_of(Pos::new(here.x + 40, here.y), side);
        let day = u32::try_from(today(&world)).unwrap();
        let mut held = place_with(&food, food.numbers["bramble"], 2);
        held.looked_day = Some(day);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.places.clear();
        mind.places.insert(key, held.clone());
        mind.places.get_mut(&key).unwrap().barred = Some(Mark::Draft(day));
        mind.barred_new.push((key, day));
        // Its telling: the mark apart from the slots, the place not told as
        // food; its draft now the record it holds (D55).
        let mut news = Vec::new();
        let memory = &mut brains.memory;
        let (slots, apart) = mind.tell(
            &rules.needs.fire,
            &food,
            (here, side, day, 0),
            id,
            memory,
            &mut news,
        );
        assert!(slots.iter().all(
            |r| !matches!(memory.get(*r), Some(Record::Place { place, .. }) if *place == key)
        ));
        assert!(matches!(
            apart[..],
            [r] if memory.get(r) == Some(&Record::Barred { place: key, day, witness: id })
        ));
        let record = apart[0];
        let own = Access {
            record,
            by: id,
            day,
        };
        assert_eq!(mind.places[&key].barred, Some(Mark::Held(own)));
        assert_eq!(memory.refs(record), 2, "the telling's and its own");
        // Heard: one who holds the place marks it; one who stood on it
        // since does not; one who does not know it learns nothing.
        let template = brains.minds[&id].clone();
        let (holds, stood, stranger) = (BodyId(901), BodyId(902), BodyId(903));
        for (other, place) in [
            (holds, Some(held.clone())),
            (
                stood,
                Some(Place {
                    stood: Some(day),
                    ..held.clone()
                }),
            ),
            (stranger, None),
        ] {
            let mut m = template.clone();
            m.places.clear();
            m.barred_new.clear();
            if let Some(p) = place {
                m.places.insert(key, p);
            }
            brains.minds.insert(other, m);
        }
        brains.news.clear();
        let present = [id, holds, stood, stranger];
        brains.hear_barred(&present, (record, id), (day, 0));
        assert_eq!(
            brains.minds[&holds].places[&key].barred,
            Some(Mark::Held(Access {
                record,
                by: id,
                day
            }))
        );
        assert_eq!(brains.memory.refs(record), 3, "and the listener's");
        assert_eq!(brains.minds[&stood].places[&key].barred, None);
        assert!(brains.minds[&stranger].places.is_empty());
        // That a mate stood there does not tell the way: nothing at the
        // fire clears a mark, only its own feet on the place.
        assert!(
            !brains
                .news
                .iter()
                .any(|n| matches!(n, News::Reached { .. }))
        );
        let mind = brains.minds.get_mut(&holds).unwrap();
        let mut news = Vec::new();
        stand_on(
            &world,
            &rules,
            (holds, Pos::new(here.x + 40, here.y)),
            mind,
            (&mut brains.memory, &mut news),
        );
        assert_eq!(mind.places[&key].barred, None);
        assert!(matches!(news[..], [News::Reached { .. }]));
        assert_eq!(brains.memory.refs(record), 2, "a mark cleared, released");
        // The teller gone, its access goes with it.
        let mut teller = brains.minds.remove(&id).unwrap();
        teller.forget_all(&mut brains.memory);
        assert_eq!(brains.memory.refs(record), 1, "the telling's alone");
    }

    #[test]
    fn water_or_rock_marks_the_place_for_good_and_no_way_only_for_the_day() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let food = food_now(&world, &rules);
        let side = place_side(&rules);
        let here = world.body(id).unwrap().pos();
        let key = place_of(Pos::new(here.x + 40, here.y), side);
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.places.clear();
        mind.places
            .insert(key, place_with(&food, food.numbers["bramble"], 2));
        let walked = |mind: &Mind, cause: Blockage| {
            let mut m = mind.clone();
            m.doing = Doing::ToPlace(key);
            m.stopped = true;
            m.blocked = Some(cause);
            decide_alone(&world, &rules, id, &mut m);
            m
        };
        // No way in sight (a trunk, the map's edge, an order refused):
        // left for the day, no mark.
        let no_way = walked(mind, Blockage::NoWay);
        assert_eq!(no_way.places[&key].barred, None);
        assert_eq!(no_way.marks_held(), 0);
        assert_eq!(no_way.out_of_reach.get(&key), Some(&today(&world)));
        // Water or rock: marked, and the mark does not fade.
        for cause in [Blockage::Water, Blockage::Rock] {
            assert!(walked(mind, cause).places[&key].is_barred());
        }
        let mut news = Vec::new();
        bar(&world, (id, key, Blockage::Water), mind, &mut news);
        let later = world.tick().0
            + 2 * u64::from(world.calendar().days_per_year()) * world.calendar().ticks_per_day();
        let mut events = Vec::new();
        world.advance(later - world.tick().0, &rules, &mut events);
        let mind = brains.minds.get_mut(&id).unwrap();
        assert_eq!(mind.marks_held(), 1, "two years on, still held");
        // Only standing there clears it.
        let mut news = Vec::new();
        stand_on(
            &world,
            &rules,
            (id, Pos::new(here.x + 40, here.y)),
            mind,
            (&mut Store::new(), &mut news),
        );
        assert_eq!(mind.marks_held(), 0);
        assert!(matches!(news[..], [News::Reached { .. }]));
    }

    #[test]
    fn hungry_at_the_threshold_a_body_stays_hungry_until_fed_under_the_contents() {
        let mut world = field(16, 40, Pos::new(5, 5));
        let rules = rules();
        let id = world.bodies[0].id();
        let fed = rules.needs.thought.fed_under;
        assert!(fed < HUNGRY);
        let hungry_at = |world: &mut World, points: u32, was: bool| {
            world.bodies[0].condition.hunger = Gauge::at(points);
            pasts(world, &rules, id, was).2
        };
        assert!(
            !hungry_at(&mut world, HUNGRY - 1, false),
            "under the threshold, not yet"
        );
        assert!(hungry_at(&mut world, HUNGRY, false), "at the threshold");
        assert!(
            hungry_at(&mut world, HUNGRY - 1, true),
            "a mouthful does not feed it"
        );
        assert!(hungry_at(&mut world, fed, true), "nor the next");
        assert!(
            !hungry_at(&mut world, fed - 1, true),
            "fed under the content's point"
        );
        assert!(
            !hungry_at(&mut world, fed, false),
            "between the two, not hungry if it was not"
        );
        // Piloted beside a ripe bramble, hungry at 52: once its hunger
        // is under the threshold it is still hungry, and goes on eating.
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(HUNGRY + 2);
        let mut ticks = 0;
        while world.body(id).unwrap().condition().hunger.points() >= HUNGRY {
            run_valley(&mut world, &rules, &mut brains, 10);
            ticks += 10;
            assert!(ticks < 3000, "it never ate under the threshold");
        }
        assert!(
            brains.minds[&id].past.2,
            "under the threshold, still hungry"
        );
        let (_, decided) = run_valley(&mut world, &rules, &mut brains, 300);
        assert!(
            decided.iter().any(|(_, d)| d.why == Why::Hunger),
            "it goes on eating for hunger: {decided:?}"
        );
    }

    #[test]
    fn a_thirsty_body_walks_to_the_water_it_sees_and_drinks() {
        let mut world = field(64, 40, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert!(
            matches!(
                decided[0].1,
                Decision { intention: Intention::MoveTo { target, .. }, why: Why::Thirst } if target.x == 39
            ),
            "it walks to the bank: {:?}",
            decided[0]
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyDrank { at, .. } if at.x == 40)),
            "it drank at the river"
        );
        assert!(world.bodies[0].condition.thirst.points() < THIRSTY);
        assert!(
            decided.iter().any(|(_, d)| d.why == Why::Home),
            "and walks home after: {decided:?}"
        );
        let mind = brains.mind(BodyId(1)).unwrap();
        assert!(
            mind.drinking_places().count() >= 2,
            "it remembers the bank, a place a chunk"
        );
    }

    #[test]
    fn it_thinks_only_when_something_changes() {
        let mut world = field(64, 40, Pos::new(5, 5));
        let mut brains = piloted(&world);
        // Born: it thinks once; home by day, nothing pressing.
        let (_, decided) = run(&mut world, &mut brains, 1000);
        assert!(
            decided.is_empty(),
            "nothing to do at home by day: {decided:?}"
        );
        assert_eq!(
            brains.mind(BodyId(1)).unwrap().thoughts(),
            1,
            "its gauges rose, no threshold crossed"
        );
        // Thirst crosses the threshold: one more thought, a walk, in its
        // own tick (body 1: the next one, WAKE_SPREAD_SECONDS).
        thirst(&mut world, THIRSTY);
        let (_, decided) = run(&mut world, &mut brains, 2);
        assert_eq!(decided.len(), 1);
        assert_eq!(brains.mind(BodyId(1)).unwrap().thoughts(), 2);
    }

    #[test]
    fn knowing_no_water_it_goes_looking_and_finds_it() {
        // The river 150 tiles east: out of sight (60 on grass) from the camp.
        let mut world = field(200, 150, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        // Born at the camp it knows where the camp drinks: forgotten here,
        // to see it go looking.
        brains.minds.get_mut(&BodyId(1)).unwrap().water.clear();
        let (events, decided) = run(&mut world, &mut brains, 7000);
        assert_eq!(
            decided[0].1.why,
            Why::Explore,
            "nothing known: it goes looking"
        );
        assert!(
            events.iter().any(|e| matches!(e, Event::BodyDrank { .. })),
            "and drinks within the day: {:?}",
            decided.iter().map(|(_, d)| d.why).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_hungry_body_eats_the_food_it_saw() {
        let mut world = field(64, 60, Pos::new(5, 5));
        let berries = world.spawn_item("berries", 100, Pos::new(15, 9));
        world.bodies[0].condition.deficit_j =
            u64::from(HUNGRY + 10) * u64::from(rules().needs.kj_per_hunger_point) * 1000;
        world.bodies[0].condition.hunger = Gauge::at(HUNGRY + 10);
        let mut brains = piloted(&world);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert_eq!(decided[0].1.why, Why::Hunger);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyAte { item, .. } if *item == berries)),
            "it ate the berries: {decided:?}"
        );
    }

    #[test]
    fn at_night_it_walks_home_and_sleeps_there() {
        let mut world = field(64, 60, Pos::new(30, 30));
        let mut brains = piloted(&world);
        // Born away from the camp by day: home first.
        let (_, decided) = run(&mut world, &mut brains, 1);
        assert_eq!(decided[0].1.why, Why::Home);
        let (events, decided) = run(&mut world, &mut brains, 7200);
        assert!(events.iter().any(|e| matches!(e, Event::NightFell { .. })));
        assert!(
            decided
                .iter()
                .any(|(_, d)| d.why == Why::Night && matches!(d.intention, Intention::Sleep { .. })),
            "it sleeps at night: {decided:?}"
        );
        let at = world.bodies[0].pos();
        assert!(
            (at.x - 5).abs() <= HOME_TILES && (at.y - 5).abs() <= HOME_TILES,
            "at home: {at:?}"
        );
    }

    #[test]
    fn a_sleeper_wakes_only_when_parched() {
        let mut world = field(64, 40, Pos::new(5, 5));
        // Asleep and tired, short of collapsed: it would sleep on (a rested
        // body wakes with the day).
        world.bodies[0].condition.asleep = true;
        world.bodies[0].condition.fatigue = Gauge::at(COLLAPSED - 10);
        thirst(&mut world, THIRSTY + 5);
        let mut brains = piloted(&world);
        let (_, decided) = run(&mut world, &mut brains, 10);
        assert!(decided.is_empty(), "thirsty but not parched: it sleeps on");
        thirst(&mut world, PARCHED);
        let (events, decided) = run(&mut world, &mut brains, 600);
        assert_eq!(
            decided[0].1.why,
            Why::Thirst,
            "parched: it gets up and goes to drink"
        );
        assert!(events.iter().any(|e| matches!(e, Event::BodyDrank { .. })));
    }

    #[test]
    fn collapsed_of_fatigue_it_stays_down_until_it_has_slept_some_then_drinks() {
        // A field of 140 tiles, the river down column 130, the body at
        // the camp (5, 5) parched and a point short of collapsing: it sets
        // off for the water it knows, collapses on the way (the world lays
        // it down at full fatigue), and lies there until its fatigue is
        // under `COLLAPSED`, asked nothing meanwhile; then it rises, walks
        // on and drinks (22 November 2026: three bodies of the vivarium
        // died of thirst standing, woken at every tick by their pilot).
        let mut world = field(140, 130, Pos::new(5, 5));
        thirst(&mut world, PARCHED + 5);
        world.bodies[0].condition.fatigue = Gauge::at(99);
        let mut brains = piloted(&world);
        let id = world.bodies[0].id();
        // Awake a point of fatigue: 60 ticks at 120 points a day of 7,200.
        let (events, decided) = run(&mut world, &mut brains, 70);
        assert_eq!(decided.len(), 1, "one walk to the water: {decided:?}");
        assert!(matches!(decided[0].1.intention, Intention::MoveTo { .. }));
        let body = world.body(id).unwrap();
        let c = body.condition();
        assert!(
            c.asleep && c.fatigue.points() >= COLLAPSED,
            "laid down at full fatigue, resting since: {:?}",
            c.summary()
        );
        assert!(!body.is_walking(), "collapsed on its way");
        let collapsed_at = body.pos();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BodyStopped { .. }))
        );
        // A tenth of the gauge takes 240 ticks at 300 points a day asleep.
        let (_, decided) = run(&mut world, &mut brains, 200);
        assert!(decided.is_empty(), "it stays down: {decided:?}");
        let body = world.body(id).unwrap();
        assert!(body.condition().asleep && body.pos() == collapsed_at);
        let (events, decided) = run(&mut world, &mut brains, 700);
        assert!(
            events.iter().any(|e| matches!(e, Event::BodyDrank { .. })),
            "risen under {COLLAPSED}, it walked on and drank: {decided:?}"
        );
        let drink = decided
            .iter()
            .position(|(_, d)| matches!(d.intention, Intention::Drink { .. }))
            .expect("a drink");
        assert!(drink <= 2, "a walk or two, then the drink: {decided:?}");
    }

    #[test]
    fn the_same_world_gives_the_same_thoughts_and_the_dead_are_forgotten() {
        let twice = || {
            let mut world = field(200, 150, Pos::new(5, 5));
            world.spawn_body(Pos::new(6, 5));
            thirst(&mut world, 60);
            let mut brains = piloted(&world);
            run(&mut world, &mut brains, 3000).1
        };
        assert_eq!(twice(), twice());
        let mut world = field(64, 40, Pos::new(5, 5));
        let mut brains = piloted(&world);
        world.bodies[0].condition.vitality = Gauge::at(0);
        run(&mut world, &mut brains, 1);
        assert!(brains.mind(BodyId(1)).is_none());
    }

    #[test]
    fn born_at_the_camp_it_knows_where_the_camp_drinks() {
        // The river 150 tiles east, out of sight of the camp.
        let mut world = field(200, 150, Pos::new(5, 5));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        let places: Vec<_> = brains
            .mind(BodyId(1))
            .unwrap()
            .drinking_places()
            .copied()
            .collect();
        assert_eq!(
            places,
            vec![DrinkingPlace {
                stand: Pos::new(149, 5),
                water: Pos::new(150, 5)
            }]
        );
        let (events, decided) = run(&mut world, &mut brains, 400);
        assert_eq!(decided[0].1.why, Why::Thirst, "it walks straight there");
        assert!(events.iter().any(|e| matches!(e, Event::BodyDrank { .. })));
    }

    #[test]
    fn going_looking_it_walks_out_of_the_camp_in_rings() {
        let world = field(1000, 999, Pos::new(5, 5));
        let mut mind = Mind::new(Pos::new(5, 5));
        let targets: Vec<Pos> = (0..16)
            .map(|_| {
                match explore(
                    world.map(),
                    &rules(),
                    BodyId(1),
                    Pos::new(500, 500),
                    &mut mind,
                    7,
                    Why::Thirst,
                )
                .intention
                {
                    Intention::MoveTo { target, .. } => target,
                    other => panic!("{other:?}"),
                }
            })
            .collect();
        let ring = |p: Pos| (p.x - 5).abs().max((p.y - 5).abs());
        assert!(
            targets[..8].iter().all(|p| ring(*p) <= 60),
            "the first ring a sight wide: {targets:?}"
        );
        assert!(
            targets[8..].iter().any(|p| ring(*p) > 60)
                && targets[8..].iter().all(|p| ring(*p) <= 120),
            "the second twice as wide: {targets:?}"
        );
    }

    #[test]
    fn each_body_goes_home_to_its_own_camp() {
        let mut world = field(200, 100, Pos::new(5, 5));
        world.spawn_body(Pos::new(150, 150));
        let rules = rules();
        let mut brains = Brains::new(7);
        brains.adopt(&world, &rules, BodyId(1));
        brains.adopt_at(&world, &rules, BodyId(2), Pos::new(150, 150));
        assert_eq!(brains.mind(BodyId(2)).unwrap().home(), Pos::new(150, 150));
        // Its own camp's water: the river at column 100, the bank at 101.
        let places: Vec<_> = brains
            .mind(BodyId(2))
            .unwrap()
            .drinking_places()
            .copied()
            .collect();
        assert_eq!(
            places,
            vec![DrinkingPlace {
                stand: Pos::new(101, 150),
                water: Pos::new(100, 150)
            }]
        );
        world.bodies[1].condition.thirst = Gauge::at(60);
        let (events, _) = run(&mut world, &mut brains, 7200);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::BodyDrank {
                body: BodyId(2),
                ..
            }
        )));
        let at = world.bodies[1].pos();
        assert!(
            (at.x - 150).abs() <= HOME_TILES && (at.y - 150).abs() <= HOME_TILES,
            "back at its camp for the night: {at:?}"
        );
    }

    #[test]
    fn stopped_short_of_the_water_far_from_its_camp_it_goes_back_there_first() {
        let mut world = field(64, 40, Pos::new(25, 5));
        thirst(&mut world, 60);
        let rules = rules();
        let id = world.bodies[0].id();
        let mut mind = Mind::new(Pos::new(5, 5));
        let place = DrinkingPlace {
            stand: Pos::new(39, 5),
            water: Pos::new(40, 5),
        };
        mind.water.insert(place.stand.chunk(), place);
        mind.doing = Doing::ToDrink(place);
        mind.stopped = true;
        let decision = decide_alone(&world, &rules, id, &mut mind).expect("a decision");
        assert_eq!(
            decision,
            Decision {
                intention: Intention::MoveTo {
                    body: id,
                    target: Pos::new(5, 5)
                },
                why: Why::Thirst
            }
        );
        assert_eq!(mind.drinking_places().count(), 1, "it keeps the place");
        // On its way: it goes on.
        mind.stopped = false;
        assert_eq!(decide_alone(&world, &rules, id, &mut mind), None);
        // Stopped short of it from the camp: it forgets it.
        world.bodies[0].at = Pos::new(5, 5).centre();
        mind.doing = Doing::ToDrink(place);
        mind.stopped = true;
        assert_eq!(decide_alone(&world, &rules, id, &mut mind), None);
        assert_eq!(mind.drinking_places().count(), 0);
    }

    #[test]
    fn pausing_before_a_rock_on_its_way_to_the_water_it_does_not_turn_back() {
        // A field of 140 tiles, the camp at (5, 5), a wall of rock down
        // column 70 from the map's edge to row 70, one pool at (120, 62);
        // the body at (20, 62) knows the pool, out of its sight (60
        // tiles). It walks straight (D32), stops short of the wall, its
        // goal kept, looks again, goes round the wall's end and drinks:
        // the stop before the wall is a pause, not a "stopped short far
        // from the camp" that sends it back there first (22 November
        // 2026: 241 such returns in 25 days of the vivarium).
        let mut map = Map::filled(140, 140, Terrain::Grass).with_camp(5, 5);
        for y in 0..=70 {
            map.set(Pos::new(70, y), Terrain::Rock);
        }
        map.set(Pos::new(120, 62), Terrain::Water);
        let mut world = World::empty(Calendar::new(1, 7200).unwrap(), map);
        let id = world.spawn_body(Pos::new(20, 62));
        thirst(&mut world, 60);
        let mut brains = piloted(&world);
        let place = DrinkingPlace {
            stand: Pos::new(119, 62),
            water: Pos::new(120, 62),
        };
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.water.clear();
        mind.water.insert(place.stand.chunk(), place);
        let (events, decided) = run(&mut world, &mut brains, 3000);
        // Up to its drink (quenched, it walks home: another matter).
        let drank = events
            .iter()
            .position(|e| matches!(e, Event::BodyDrank { .. }))
            .expect("it drank");
        let before = &events[..drank];
        let stops = before
            .iter()
            .filter(|e| matches!(e, Event::BodyStopped { .. }))
            .count();
        assert_eq!(stops, 2, "it paused before the wall, then arrived");
        assert!(
            !before
                .iter()
                .any(|e| matches!(e, Event::BodyBlocked { .. })),
            "nothing barred it for good"
        );
        let camp = Pos::new(5, 5);
        assert!(
            !decided.iter().any(|(_, d)| d.why == Why::Thirst
                && matches!(
                    d.intention,
                    Intention::MoveTo { target, .. } if target == camp
                )),
            "thirsty, it never went back to the camp first: {decided:?}"
        );
    }

    #[test]
    fn hemmed_in_it_waits_for_the_next_change_rather_than_ask_leg_after_leg() {
        let mut world = field(64, 99, Pos::new(25, 5));
        thirst(&mut world, 60);
        let rules = rules();
        let id = world.bodies[0].id();
        let mut mind = Mind::new(Pos::new(5, 5));
        let first = decide_alone(&world, &rules, id, &mut mind).expect("it goes looking");
        assert_eq!(first.why, Why::Explore);
        // The leg ended where it set off.
        mind.stopped = true;
        assert_eq!(decide_alone(&world, &rules, id, &mut mind), None);
        assert_eq!(mind.doing, Doing::Nothing);
        // The next change: it tries a leg again.
        mind.stopped = false;
        assert!(decide_alone(&world, &rules, id, &mut mind).is_some());
    }

    #[test]
    fn it_looks_for_water_only_some_steps_away() {
        let world = field(64, 40, Pos::new(5, 5));
        let rules = rules();
        let mut mind = Mind::new(Pos::new(5, 5));
        look_alone(&world, &rules, Pos::new(5, 5), &mut mind);
        let seen: Vec<_> = mind.drinking_places().copied().collect();
        assert!(!seen.is_empty() && seen.iter().all(|p| p.stand.x == 39 || p.stand.x == 41));
        assert!(seen.iter().all(|p| p.water.x == 40));
        mind.water.clear();
        look_alone(&world, &rules, Pos::new(5, 5), &mut mind);
        assert_eq!(
            mind.drinking_places().count(),
            0,
            "the same place: not looked at again"
        );
        look_alone(&world, &rules, Pos::new(6, 5), &mut mind);
        assert_eq!(mind.drinking_places().count(), 0, "a step away: not yet");
        look_alone(&world, &rules, Pos::new(5 + LOOK_STEP, 5), &mut mind);
        assert!(mind.drinking_places().count() > 0, "a new place: looked at");
    }

    /// What `kind` bears now, read from the content by names: what the
    /// brains did before they numbered the kinds (the food table must say
    /// the same).
    fn by_names<'a>(world: &World, rules: &'a Content, kind: &str) -> Option<(&'a str, u64)> {
        rules.crops.iter().find_map(|crop| {
            if crop.on != kind || rules.material(&crop.gives).is_none_or(|m| m.food.is_none()) {
                return None;
            }
            let year = crops::in_season(crop, world.calendar(), world.tick())?;
            Some((crop.gives.as_str(), year))
        })
    }

    #[test]
    fn the_food_table_says_what_the_crops_say_all_year() {
        use crate::crops::tests::valley_with;
        let (mut world, rules, _, _) = valley_with("bramble");
        let mut food = Food::default();
        let day = world.calendar().ticks_per_day();
        let mut events = Vec::new();
        let (mut bearing, mut bare) = (0, 0);
        // Two years, each day at a later hour.
        for _ in 0..2 * world.calendar().days_per_year() {
            food.refresh(&world, &rules);
            for crop in &rules.crops {
                let numbered = food.numbers.get(&crop.on).copied();
                let now = numbered.and_then(|kind| food.now(kind)).map(|bears| {
                    assert_eq!(
                        bears.kj,
                        kj_per_hour(&rules, &crop.on, Some(&rules.crops[bears.crop].gives))
                    );
                    (rules.crops[bears.crop].gives.as_str(), bears.year)
                });
                assert_eq!(now, by_names(&world, &rules, &crop.on), "{}", crop.on);
                if let Some(kind) = numbered {
                    assert_eq!(
                        food.best_kj[usize::from(kind)],
                        kj_per_hour(&rules, &crop.on, None)
                    );
                    assert_eq!(food.name(kind), crop.on);
                }
                if now.is_some() {
                    bearing += 1;
                } else {
                    bare += 1;
                }
            }
            events.clear();
            world.advance(day + day / 7, &rules, &mut events);
        }
        assert!(bearing > 0 && bare > 0);
    }

    #[test]
    #[allow(clippy::too_many_lines, reason = "one scenario, round after round")]
    fn remembering_what_it_sees_keeps_what_learning_all_then_forgetting_kept() {
        use crate::crops::tests::{run_to, valley_with};
        let (mut world, rules, _, _) = valley_with("bramble");
        run_to(&mut world, &rules, 28);
        let food = food_now(&world, &rules);
        let kinds = u64::try_from(food.kinds.len()).unwrap();
        let here = Pos::new(100, 100);
        let (side, keep) = (place_side(&rules), places_remembered(&rules));
        // Before: every place seen went into the memory, then the least
        // worth were forgotten, ranked by the kinds' names.
        let forget = |places: &mut BTreeMap<PlaceKey, Place>| {
            let mut ranked: Vec<_> = places
                .iter()
                .map(|(key, place)| {
                    let (now, best) = place
                        .kinds
                        .iter()
                        .map(|seen| {
                            let kind = food.name(seen.kind);
                            match by_names(&world, &rules, kind) {
                                Some(_) if seen.holding > 0 => (true, seen.best_kj),
                                _ => (false, kj_per_hour(&rules, kind, None)),
                            }
                        })
                        .max()
                        .unwrap();
                    (
                        std::cmp::Reverse((
                            now,
                            worth(best, far(place_centre(*key, side), here), WORTH_HALF_TILES),
                        )),
                        *key,
                    )
                })
                .collect();
            if ranked.len() > keep {
                ranked.select_nth_unstable(keep);
                for (_, key) in &ranked[keep..] {
                    places.remove(key);
                }
            }
        };
        for round in 0u64..5 {
            let mut mind = Mind::new(here);
            let mut seen: BTreeMap<PlaceKey, Vec<Seen>> = BTreeMap::new();
            for i in 0..20 + round * 60 {
                let r = mix(i ^ (round << 32));
                let key = (
                    i32::try_from(r % 40).unwrap() - 20,
                    i32::try_from((r >> 8) % 40).unwrap() - 20,
                );
                let kind = u16::try_from((r >> 32) % kinds).unwrap();
                let plants = u16::try_from(1 + (r >> 40) % 9).unwrap();
                let holding = u16::try_from((r >> 44) % 3).unwrap().min(plants);
                let best_kj = if holding > 0 {
                    100 + (r >> 48) % 3000
                } else {
                    0
                };
                if i % 3 == 0 && mind.places.len() < keep {
                    mind.places
                        .entry(key)
                        .or_default()
                        .saw(kind, plants, holding, best_kj);
                } else {
                    let kinds = seen.entry(key).or_default();
                    match kinds.binary_search_by_key(&kind, |k| k.kind) {
                        Ok(i) => {
                            kinds[i].plants = kinds[i].plants.max(plants);
                            kinds[i].holding = holding;
                            kinds[i].best_kj = best_kj;
                        }
                        Err(i) => kinds.insert(
                            i,
                            Seen {
                                kind,
                                plants,
                                holding,
                                best_kj,
                            },
                        ),
                    }
                }
            }
            let mut all = mind.places.clone();
            for (key, kinds) in &seen {
                for seen in kinds {
                    all.entry(*key).or_default().saw(
                        seen.kind,
                        seen.plants,
                        seen.holding,
                        seen.best_kj,
                    );
                }
            }
            forget(&mut all);
            for key in seen.keys() {
                if let Some(place) = all.get_mut(key) {
                    place.looked_day = Some(0);
                }
            }
            let (mut memory, mut news) = (Store::new(), Vec::new());
            remember_places(
                &food,
                &rules,
                (BodyId(1), here, 0, 0),
                &mut mind,
                seen,
                &mut memory,
                &mut news,
            );
            assert_eq!(mind.places, all, "round {round}");
        }
    }

    #[test]
    fn a_look_finds_the_drinking_places_a_scan_of_every_tile_found() {
        use crate::crops::tests::valley_with;
        let (world, rules, _, _) = valley_with("bramble");
        let map = world.map();
        let camp = map.camp();
        // Before: every drinkable tile in sight, then the walkable tiles
        // around it, kept the nearest of each chunk.
        let scan = |at: Pos, water: &mut BTreeMap<ChunkPos, DrinkingPlace>| {
            let sight = sight_at(map, &rules, at);
            let in_sight = |p: Pos| (p.x - at.x).abs() <= sight && (p.y - at.y).abs() <= sight;
            for y in at.y - sight..=at.y + sight {
                for x in at.x - sight..=at.x + sight {
                    let drink = Pos::new(x, y);
                    if !map.get(drink).is_some_and(Terrain::is_drinkable) {
                        continue;
                    }
                    for (dx, dy) in AROUND {
                        let stand = Pos::new(x + dx, y + dy);
                        if !in_sight(stand) || !walkable(map, &rules, stand) {
                            continue;
                        }
                        let place = DrinkingPlace {
                            stand,
                            water: drink,
                        };
                        water
                            .entry(stand.chunk())
                            .and_modify(|known| {
                                if (far(stand, at), stand.y, stand.x)
                                    < (far(known.stand, at), known.stand.y, known.stand.x)
                                {
                                    *known = place;
                                }
                            })
                            .or_insert(place);
                    }
                }
            }
            while water.len() > MEMORY_PLACES {
                let farthest = *water
                    .iter()
                    .max_by_key(|(chunk, place)| (far(place.stand, camp), **chunk))
                    .map(|(chunk, _)| chunk)
                    .unwrap();
                water.remove(&farthest);
            }
        };
        let (mut bearers, mut shores) = (Placed::new(), Lru::new(BEARER_CHUNKS));
        let mut food = food_now(&world, &rules);
        let mut mind = Mind::new(camp);
        let mut before = BTreeMap::new();
        let mut found = 0;
        for step in 0..80 {
            let at = Pos::new((step * 37) % 256, (step * 61) % 256);
            // Water only: its plants it looked at there already.
            mind.looked_from = None;
            mind.plants_from = Some(at);
            let (mut memory, mut news) = (Store::new(), Vec::new());
            let mut land = Land {
                groups: &mut BTreeMap::new(),
                bearers: &mut bearers,
                shores: &mut shores,
                food: &mut food,
                memory: &mut memory,
                news: &mut news,
            };
            look(&world, &rules, BodyId(1), at, &mut mind, &mut land);
            scan(at, &mut before);
            assert_eq!(mind.water, before, "looking from {at:?}");
            found = found.max(before.len());
        }
        assert!(found > 3, "the valley's river seen");
    }

    /// `count` bodies on a field with a river, piloted, all due at once.
    fn crowd(count: i32) -> (World, Brains) {
        let mut world = field(64, 40, Pos::new(5, 5));
        for k in 1..count {
            world.spawn_body(Pos::new(5 + k % 8, 5 + k / 8));
        }
        let mut brains = piloted(&world);
        for mind in brains.minds.values_mut() {
            mind.due = true;
            mind.wake_at = None;
        }
        (world, brains)
    }

    #[test]
    fn past_its_share_of_work_the_due_minds_wait_their_turn_and_none_for_ever() {
        let rules = rules();
        let (mut world, mut brains) = crowd(30);
        // Calendar of one tick a second: a turn's share is the second's,
        // some four thoughts (a look over new land reads 25 chunks here).
        brains.work_per_second = 100;
        world.advance(1, &rules, &mut Vec::new());
        let thoughts =
            |brains: &Brains| -> Vec<u64> { brains.minds.values().map(Mind::thoughts).collect() };
        let decided = brains.think(&world, &rules, &[]);
        assert!(decided.is_sorted_by_key(|(id, _)| *id));
        let first: Vec<u64> = thoughts(&brains);
        let thought = first.iter().filter(|&&n| n == 1).count();
        assert!(thought > 0 && thought < 30, "{thought} thought");
        assert_eq!(brains.waiting(), 30 - thought);
        // Those that thought are the first by id; the others wait, due.
        assert!(first.is_sorted_by(|a, b| a >= b));
        assert!(brains.minds.values().all(|m| m.due == (m.thoughts == 0)));
        // The next turns go on from the first that waited; what a turn
        // spent past its share, less than a thought, is repaid.
        let mut turns = 1;
        while brains.waiting() > 0 {
            let waited = brains.next.unwrap();
            assert!(brains.owed < 26);
            brains.think(&world, &rules, &[]);
            turns += 1;
            assert_eq!(
                brains.minds[&waited].thoughts, 1,
                "the first that waited thinks first"
            );
            assert!(turns < 30, "none waits for ever");
        }
        assert!(thoughts(&brains).iter().all(|&n| n == 1), "each once");
        assert!(turns > 2);
    }

    #[test]
    fn while_the_crowd_fits_its_share_the_budget_changes_nothing() {
        let rules = rules();
        let run = |work_per_second| {
            let (mut world, mut brains) = crowd(30);
            for body in &mut world.bodies {
                body.condition.thirst = Gauge::at(60);
            }
            brains.work_per_second = work_per_second;
            let mut waited = 0;
            let (mut all, mut events) = (Vec::new(), Vec::new());
            for _ in 0..3000 {
                events.clear();
                world.step(&rules, &mut events);
                for (body, decision) in brains.think(&world, &rules, &events) {
                    world.queue(decision.intention.clone());
                    all.push((world.tick(), body, decision));
                }
                waited += brains.waiting();
            }
            (all, world.bodies().to_vec(), waited)
        };
        let (decided, bodies, waited) = run(THOUGHT_WORK_PER_SECOND);
        assert_eq!(waited, 0, "the crowd fits");
        assert!(decided.len() > 30);
        let (unbound, unbound_bodies, _) = run(1 << 40);
        assert_eq!(decided, unbound);
        assert_eq!(bodies, unbound_bodies);
    }

    /// The content's trials, every chance at `percent`: a body in distress
    /// for a day tries, and copies, whenever the chance says so.
    fn trials_at(percent: u32, loss: u32) -> Content {
        let mut rules = rules();
        rules.needs.trials = Trials {
            trial_after_days: 1,
            trial_days: 2,
            try_percent_by_will: vec![(0, percent)],
            imitate_tiles: 12,
            imitate_percent: percent,
            imitate_loss_percent: loss,
            ..Trials::default()
        };
        rules
    }

    /// Night falls on `world` for `brains`: the minds close their day.
    fn nightfall(world: &World, rules: &Content, brains: &mut Brains) -> Vec<News> {
        let day = u64::try_from(world.today()).unwrap();
        brains.think(world, rules, &[Event::NightFell { day }]);
        // The days as the habit keeps them (D56) are told every night.
        let mut news = brains.news();
        news.retain(|n| !matches!(n, News::Habit { .. }));
        news
    }

    /// Only `way` differs between `a` and `b`, and as a step would move it.
    fn one_step(way: Way, a: &Ways, b: &Ways) -> bool {
        let others = Way::ALL.iter().filter(|w| **w != way).all(|w| w.same(a, b));
        let twice = |x: u64, y: u64| y == x * 2 || y == (x / 2).max(1);
        others
            && match way {
                Way::WorthHalfTiles => twice(a.worth_half_tiles, b.worth_half_tiles),
                Way::StockKj => twice(a.stock_kj, b.stock_kj),
                Way::SleepOut => a.sleep_out != b.sleep_out,
                Way::Home => a.home != b.home,
            }
    }

    #[test]
    fn the_ways_default_to_the_constants_and_the_worth_is_the_old_one() {
        let home = Pos::new(5, 5);
        assert_eq!(
            Ways::at(home),
            Ways {
                worth_half_tiles: WORTH_HALF_TILES,
                sleep_out: false,
                stock_kj: STOCK_KJ,
                home,
            }
        );
        let mind = Mind::new(home);
        assert_eq!(*mind.ways(), Ways::at(home));
        assert_eq!(mind.home(), home);
        for tiles in [0, 1, 32, 1000] {
            assert_eq!(
                worth(2850, tiles * tiles, WORTH_HALF_TILES),
                2850 * WORTH_HALF_TILES / (WORTH_HALF_TILES + u64::try_from(tiles).unwrap())
            );
        }
        assert_eq!(
            worth(1000, 64 * 64, 64),
            500,
            "a way twice as wide halves at 64"
        );
        // The content's trials absent: nobody tries nor copies.
        assert!(!rules().needs.trials.on());
    }

    /// The innate will (D50, amended 24 November 2026): drawn from the
    /// run's seed and the id, not the map's: the same body, the same seed,
    /// the same will; another seed, another will.
    #[test]
    fn the_will_is_drawn_from_the_run_seed_and_the_id_at_adoption() {
        let world = field(64, 40, Pos::new(5, 5));
        let a = piloted(&world).mind(BodyId(1)).unwrap().will();
        let b = piloted(&world).mind(BodyId(1)).unwrap().will();
        assert_eq!(a, b, "the same every time");
        let rules = rules();
        let will_of = |seed: u64, id: BodyId| {
            let mut brains = Brains::new(seed);
            brains.adopt(&world, &rules, id);
            brains.mind(id).unwrap().innate()
        };
        let id = BodyId(1);
        assert_eq!(will_of(3, id), will_of(3, id), "one seed, one will");
        assert_ne!(
            will_of(1, id),
            will_of(9, id),
            "another run seed, another will, the map the same"
        );
        let mut many = field(64, 40, Pos::new(5, 5));
        for k in 0..40 {
            many.spawn_body(Pos::new(6 + k % 8, 6 + k / 8));
        }
        let brains = piloted(&many);
        let wills: Vec<u8> = brains.minds.values().map(Mind::will).collect();
        assert!(wills.iter().all(|&w| w <= 100));
        assert!(
            wills.iter().any(|&w| w < 30) && wills.iter().any(|&w| w > 70),
            "{wills:?}"
        );
    }

    #[test]
    fn distress_is_counted_at_nightfall_hungry_or_with_no_gather_at_all() {
        let mut world = field(64, 40, Pos::new(5, 5));
        let rules = rules();
        let mut brains = piloted(&world);
        let id = BodyId(1);
        // Fed and nothing gathered: a day of distress.
        nightfall(&world, &rules, &mut brains);
        assert_eq!(brains.mind(id).unwrap().distress_days(), 1);
        // Gathered something, not hungry: the count falls back to nought.
        brains.minds.get_mut(&id).unwrap().gathered_today_kj = 500;
        nightfall(&world, &rules, &mut brains);
        assert_eq!(brains.mind(id).unwrap().distress_days(), 0);
        let days = &brains.mind(id).unwrap().days;
        assert_eq!(days.len(), 2);
        assert_eq!((days[0].gathered_kj, days[1].gathered_kj), (0, 500));
        // Gathered, but hungry at nightfall: distress again, two days running.
        world.bodies[0].condition.hunger = Gauge::at(HUNGRY);
        for _ in 0..2 {
            brains.minds.get_mut(&id).unwrap().gathered_today_kj = 500;
            nightfall(&world, &rules, &mut brains);
        }
        assert_eq!(brains.mind(id).unwrap().distress_days(), 2);
        // The days kept: twice trial_days (1 by default) at least two.
        assert!(brains.mind(id).unwrap().days.len() <= 4);
        // Fed and gathering but thirsty at nightfall: distress too (D50,
        // amended 23 November 2026).
        world.bodies[0].condition.hunger = Gauge::at(0);
        brains.minds.get_mut(&id).unwrap().gathered_today_kj = 500;
        nightfall(&world, &rules, &mut brains);
        assert_eq!(brains.mind(id).unwrap().distress_days(), 0);
        world.bodies[0].condition.thirst = Gauge::at(THIRSTY);
        brains.minds.get_mut(&id).unwrap().gathered_today_kj = 500;
        nightfall(&world, &rules, &mut brains);
        assert_eq!(brains.mind(id).unwrap().distress_days(), 1);
    }

    #[test]
    fn with_the_trials_absent_from_the_content_nothing_ever_happens() {
        let mut world = field(64, 40, Pos::new(5, 5));
        let rules = rules();
        let mut brains = piloted(&world);
        world.spawn_body(Pos::new(6, 5));
        brains.adopt(&world, &rules, BodyId(2));
        let before: Vec<Ways> = brains.minds.values().map(|m| m.ways).collect();
        for _ in 0..10 {
            let news = nightfall(&world, &rules, &mut brains);
            assert!(news.is_empty(), "{news:?}");
            for id in [BodyId(1), BodyId(2)] {
                brains.imitate(&world, &rules, id);
            }
        }
        let news = brains.news();
        assert!(news.is_empty(), "{news:?}");
        assert!(brains.minds.values().all(|m| m.trial.is_none()));
        assert_eq!(
            brains.minds.values().map(|m| m.ways).collect::<Vec<_>>(),
            before
        );
        assert!(brains.minds.values().all(|m| m.distress_days() == 10));
    }

    #[test]
    fn in_distress_long_enough_a_trial_moves_one_way_by_one_step() {
        let rules = trials_at(100, 0);
        let mut world = field(64, 40, Pos::new(5, 5));
        for k in 1..12 {
            world.spawn_body(Pos::new(5 + k % 6, 6 + k / 6));
        }
        let mut brains = piloted(&world);
        let mut events = Vec::new();
        world.advance(1, &rules, &mut events);
        // Day one closes: a day of distress each, one is the content's
        // threshold, and the chance is whole: everyone starts a trial.
        let news = nightfall(&world, &rules, &mut brains);
        assert_eq!(news.len(), brains.minds.len());
        let mut ways_tried = BTreeMap::new();
        for n in &news {
            let News::Trying {
                body,
                way,
                before,
                after,
                ..
            } = n
            else {
                panic!("{n:?}");
            };
            assert!(one_step(*way, before, after), "{n:?}");
            assert_eq!(*before, Ways::at(Pos::new(5, 5)));
            let mind = brains.mind(*body).unwrap();
            assert_eq!(*mind.ways(), *after);
            let trial = mind.trial().unwrap();
            assert_eq!((trial.way, trial.before), (*way, *before));
            assert_eq!(trial.started_day, 0);
            assert_eq!(
                trial.before_days,
                Lately {
                    distress_days: 1,
                    ..Lately::default()
                },
                "the bare day before"
            );
            *ways_tried.entry(*way).or_insert(0) += 1;
        }
        assert!(ways_tried.len() >= 3, "the draws spread: {ways_tried:?}");
        // Under a trial, no second one.
        let news = nightfall(&world, &rules, &mut brains);
        assert!(news.is_empty(), "{news:?}");
        // With the chance at nought, nobody tries.
        let never = trials_at(0, 0);
        let mut brains = piloted(&world);
        let news = nightfall(&world, &never, &mut brains);
        assert!(news.is_empty(), "{news:?}");
    }

    /// A trial of the worth distance judged at its second nightfall
    /// (`trial_days`: 2) against `before`: `first` is its first day as
    /// closed; the second day's gather, thinness, thirst and cold are set
    /// on the body and its mind. Whether it was kept, and the worth after.
    fn judged(
        before: Lately,
        first: Day,
        kj: u64,
        thin: bool,
        thirsty: bool,
        cold: bool,
    ) -> (bool, u64) {
        let rules = trials_at(0, 0);
        let id = BodyId(1);
        let mut world = World::empty(
            Calendar::new(1, 7200).unwrap(),
            Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
        );
        let mut events = Vec::new();
        world.advance(2 * world.calendar().ticks_per_day(), &rules, &mut events);
        world.spawn_body(Pos::new(5, 5));
        if thin {
            world.bodies[0].condition.deficit_j =
                world.bodies[0].condition.reserve_j(&rules.needs) * 6 / 10;
        }
        if thirsty {
            thirst(&mut world, THIRSTY);
        }
        let mut brains = piloted(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        let ways = *mind.ways();
        mind.ways.worth_half_tiles *= 2;
        mind.trial = Some(Trial {
            way: Way::WorthHalfTiles,
            before: ways,
            started_day: 0,
            before_days: before,
            heard: HeardDays::default(),
        });
        mind.days = vec![first];
        mind.gathered_today_kj = kj;
        mind.cold_night = cold;
        let news = nightfall(&world, &rules, &mut brains);
        let mind = brains.mind(id).unwrap();
        assert!(mind.trial().is_none(), "judged");
        let kept = match news[..] {
            [
                News::Tried {
                    body,
                    way: Way::WorthHalfTiles,
                    kept,
                    ..
                },
            ] if body == id => kept,
            _ => panic!("{news:?}"),
        };
        (kept, mind.ways().worth_half_tiles)
    }

    #[test]
    fn a_slow_way_is_judged_on_its_horizon_not_on_trial_days() {
        let mut rules = trials_at(0, 0);
        rules.needs.trials.horizon_seasons = vec![("stock_kj".to_owned(), 1)];
        let id = BodyId(1);
        let mut world = World::empty(
            Calendar::new(1, 7200)
                .unwrap()
                .with_days_per_year(16)
                .unwrap(),
            Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
        );
        let mut events = Vec::new();
        world.advance(5 * world.calendar().ticks_per_day(), &rules, &mut events);
        world.spawn_body(Pos::new(5, 5));
        let today = u64::try_from(world.today()).unwrap();
        let mut brains = piloted(&world);
        let mind = brains.minds.get_mut(&id).unwrap();
        let ways = *mind.ways();
        mind.ways.stock_kj *= 2;
        mind.trial = Some(Trial {
            way: Way::StockKj,
            before: ways,
            started_day: today - 2,
            before_days: Lately::default(),
            heard: HeardDays::default(),
        });
        let news = nightfall(&world, &rules, &mut brains);
        assert!(
            news.is_empty(),
            "two days of a season of four: not judged yet, {news:?}"
        );
        let mind = brains.minds.get_mut(&id).unwrap();
        let trial = mind.trial.as_mut().expect("still under trial");
        trial.started_day = today - 4;
        let news = nightfall(&world, &rules, &mut brains);
        assert!(
            matches!(
                news[..],
                [News::Tried {
                    way: Way::StockKj,
                    ..
                }]
            ),
            "judged on its fourth day: {news:?}"
        );
        assert!(brains.mind(id).unwrap().trial().is_none());
    }

    #[test]
    fn the_worlds_judgement_keeps_a_better_way_and_reverts_a_worse_one() {
        let before = Lately {
            kj_per_day: 1000,
            thin_days: 1,
            ..Lately::default()
        };
        let day = |kj, thin| Day {
            gathered_kj: kj,
            thin,
            ..Day::default()
        };
        // More a day, thin no more often: kept.
        assert_eq!(
            judged(before, day(1500, false), 1500, true, false, false),
            (true, WORTH_HALF_TILES * 2)
        );
        // More a day but thin more often: reverted.
        assert_eq!(
            judged(before, day(1500, true), 1500, true, false, false),
            (false, WORTH_HALF_TILES)
        );
        // Nothing better: reverted.
        assert_eq!(
            judged(before, day(1000, true), 1000, false, false, false),
            (false, WORTH_HALF_TILES)
        );
        // The same a day, thin less often: kept.
        assert_eq!(
            judged(before, day(1000, false), 1000, false, false, false),
            (true, WORTH_HALF_TILES * 2)
        );
    }

    #[test]
    fn a_trial_that_gathers_more_but_thirsts_or_freezes_more_is_reverted() {
        let before = Lately {
            kj_per_day: 1000,
            cold_days: 1,
            ..Lately::default()
        };
        let day = |kj| Day {
            gathered_kj: kj,
            ..Day::default()
        };
        // Half as much more a day, but thirsty at the second nightfall:
        // one thirsty day where there was none: reverted.
        assert_eq!(
            judged(before, day(1500), 1500, false, true, false),
            (false, WORTH_HALF_TILES)
        );
        // More a day, cold no more often (one night, as before): kept.
        assert_eq!(
            judged(before, day(1500), 1500, false, false, true),
            (true, WORTH_HALF_TILES * 2)
        );
        // More a day, but cold two nights where it was one: reverted.
        assert_eq!(
            judged(
                before,
                Day {
                    cold: true,
                    ..day(1500)
                },
                1500,
                false,
                false,
                true
            ),
            (false, WORTH_HALF_TILES)
        );
        // The same a day, no cold night where there was one: kept.
        assert_eq!(
            judged(before, day(1000), 1000, false, false, false),
            (true, WORTH_HALF_TILES * 2)
        );
        // The days as closed carry every distress, and a day of any is a
        // day of distress.
        let world = field(64, 40, Pos::new(5, 5));
        let mut brains = piloted(&world);
        let mind = brains.minds.get_mut(&BodyId(1)).unwrap();
        mind.days = vec![
            day(1000),
            Day {
                thirsty: true,
                ..day(2000)
            },
            Day {
                hungry: true,
                bare: true,
                ..day(0)
            },
        ];
        assert_eq!(
            mind.lately(5),
            Lately {
                kj_per_day: 1000,
                thin_days: 0,
                thirsty_days: 1,
                cold_days: 0,
                distress_days: 2,
            }
        );
    }

    #[test]
    fn without_the_fire_the_best_mate_is_the_least_distressed_gathered_kj_breaking_ties() {
        let mut world = field(64, 40, Pos::new(5, 5));
        world.spawn_body(Pos::new(6, 5));
        world.spawn_body(Pos::new(7, 5));
        let (me, a, b) = (BodyId(1), BodyId(2), BodyId(3));
        let rules = trials_at(100, 0);
        assert!(!rules.needs.fire.on(), "the control: no fire");
        let day = |kj| Day {
            gathered_kj: kj,
            ..Day::default()
        };
        let camp = |a_day: Day, b_day: Day| {
            let mut brains = piloted(&world);
            brains.minds.get_mut(&me).unwrap().days = vec![day(1000)];
            let m = brains.minds.get_mut(&a).unwrap();
            m.days = vec![a_day];
            m.ways.sleep_out = true;
            m.will = 100;
            let m = brains.minds.get_mut(&b).unwrap();
            m.days = vec![b_day];
            m.ways.stock_kj = STOCK_KJ * 2;
            m.will = 100;
            brains.imitate_at_camp(&world, &rules, me);
            (brains.news(), *brains.mind(me).unwrap().ways())
        };
        // A gathered most but was thirsty, B gathered less than me: none
        // fared better than me, nothing copied.
        let (news, ways) = camp(
            Day {
                thirsty: true,
                ..day(3000)
            },
            day(500),
        );
        assert!(news.is_empty(), "{news:?}");
        assert_eq!(ways, Ways::at(Pos::new(5, 5)));
        // A gathered most but froze one night; B, less, in no distress:
        // B fared best, its store is copied.
        let (news, ways) = camp(
            Day {
                cold: true,
                ..day(3000)
            },
            day(2000),
        );
        assert!(
            matches!(news[..], [News::Copied { body, from, way: Way::StockKj, .. }] if body == me && from == b),
            "{news:?}"
        );
        assert_eq!(ways.stock_kj, STOCK_KJ * 2);
        assert!(!ways.sleep_out);
        // Neither in distress: the gathered kJ break the tie, A's night out
        // is copied.
        let (news, ways) = camp(day(3000), day(2000));
        assert!(
            matches!(news[..], [News::Copied { from, way: Way::SleepOut, .. }] if from == a),
            "{news:?}"
        );
        assert!(ways.sleep_out);
    }

    #[test]
    fn without_the_fire_a_body_at_the_camp_copies_the_best_mates_way_losing_a_step_at_times() {
        let mut world = field(64, 40, Pos::new(5, 5));
        world.spawn_body(Pos::new(6, 5));
        world.spawn_body(Pos::new(7, 5));
        let (me, mate, far) = (BodyId(1), BodyId(2), BodyId(3));
        let copy = |rules: &Content| {
            let mut brains = piloted(&world);
            let day = |kj| Day {
                gathered_kj: kj,
                ..Day::default()
            };
            brains.minds.get_mut(&me).unwrap().days = vec![day(1000)];
            let m = brains.minds.get_mut(&mate).unwrap();
            m.days = vec![day(3000)];
            m.ways.worth_half_tiles = WORTH_HALF_TILES * 2;
            m.will = 100;
            // Farther than imitate_tiles, gathering most of all: not a mate tonight.
            let f = brains.minds.get_mut(&far).unwrap();
            f.days = vec![day(9000)];
            f.ways.stock_kj = 1;
            brains.minds.get_mut(&far).unwrap().home_at_dusk = Pos::new(50, 5);
            brains.imitate_at_camp(&world, rules, me);
            (brains.news(), *brains.mind(me).unwrap().ways())
        };
        let (news, ways) = copy(&trials_at(100, 0));
        assert_eq!(ways.worth_half_tiles, WORTH_HALF_TILES * 2, "copied whole");
        assert!(
            matches!(news[..], [News::Copied { body, from, way: Way::WorthHalfTiles, lost: false, .. }] if body == me && from == mate),
            "{news:?}"
        );
        let (news, ways) = copy(&trials_at(100, 100));
        assert_eq!(
            ways.worth_half_tiles,
            WORTH_HALF_TILES * 3 / 2,
            "halfway: one and a half"
        );
        assert!(
            matches!(news[..], [News::Copied { lost: true, .. }]),
            "{news:?}"
        );
        assert_eq!(ways.stock_kj, STOCK_KJ, "the far one's way not copied");
        // The chance scaled by the mate's will: at will 0, never.
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            ..Day::default()
        }];
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        brains.minds.get_mut(&mate).unwrap().will = 0;
        brains.imitate_at_camp(&world, &trials_at(100, 0), me);
        let news = brains.news();
        assert!(news.is_empty(), "{news:?}");
        // Once a night only, and never under a trial.
        let rules = trials_at(100, 0);
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            ..Day::default()
        }];
        brains.minds.get_mut(&mate).unwrap().will = 100;
        brains.minds.get_mut(&mate).unwrap().ways.home = Pos::new(9, 9);
        brains.imitate_at_camp(&world, &rules, me);
        assert_eq!(
            brains.mind(me).unwrap().home(),
            Pos::new(9, 9),
            "the camp moved"
        );
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        brains.imitate_at_camp(&world, &rules, me);
        assert!(!brains.mind(me).unwrap().ways().sleep_out, "once a night");
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            ..Day::default()
        }];
        brains.minds.get_mut(&mate).unwrap().will = 100;
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        let mind = brains.minds.get_mut(&me).unwrap();
        mind.trial = Some(Trial {
            way: Way::StockKj,
            before: mind.ways,
            started_day: 0,
            before_days: Lately::default(),
            heard: HeardDays::default(),
        });
        brains.imitate_at_camp(&world, &rules, me);
        assert!(brains.news().is_empty(), "under a trial, it copies nothing");
        // With the fire lit, lying down joins the fire and copies nothing
        // by itself: the mate's mind is not read.
        let mut lit = trials_at(100, 0);
        lit.needs.fire = Fire::placeholders();
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            ..Day::default()
        }];
        brains.minds.get_mut(&mate).unwrap().will = 100;
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        brains.join_fire(&world, &lit, me);
        assert!(brains.news().is_empty(), "the fire reads no mind");
        assert!(brains.at_fire.values().any(|set| set.contains(&me)));
    }

    #[test]
    fn children_take_their_mothers_home_at_nightfall_and_a_carried_infant_its_carriers() {
        use crate::content::Sex;
        let rules = rules();
        let mut world = field(64, 40, Pos::new(5, 5));
        let mother = BodyId(1);
        let child = world.spawn_body(Pos::new(6, 5));
        let orphan = world.spawn_body(Pos::new(7, 5));
        let infant = world.spawn_body(Pos::new(8, 5));
        let father = world.spawn_body(Pos::new(9, 5));
        let per_year = u64::from(world.calendar().days_per_year());
        world.set_birth(mother, 30 * per_year, Sex::Female);
        world.set_birth(father, 30 * per_year, Sex::Male);
        world.set_birth(child, 5 * per_year, Sex::Male);
        world.set_birth(orphan, 5 * per_year, Sex::Female);
        world.set_birth(infant, 0, Sex::Female);
        world.set_parents(child, Some(mother), Some(father));
        world.set_parents(orphan, Some(BodyId(99)), Some(father));
        assert!(world.carry(father, infant));
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mother).unwrap().ways.home = Pos::new(20, 20);
        brains.minds.get_mut(&father).unwrap().ways.home = Pos::new(30, 30);
        nightfall(&world, &rules, &mut brains);
        let home = |id| brains.mind(id).unwrap().home();
        assert_eq!(home(child), Pos::new(20, 20), "its mother's");
        assert_eq!(
            home(orphan),
            Pos::new(30, 30),
            "no mother known: its father's"
        );
        assert_eq!(home(infant), Pos::new(30, 30), "its carrier's");
        assert_eq!(home(mother), Pos::new(20, 20), "a grown body keeps its own");
        assert!(brains.minds.values().all(|m| m.home_at_dusk == m.ways.home));
    }

    #[test]
    fn the_same_seed_gives_the_same_trials_and_a_sleeper_out_lies_down_where_it_stands() {
        let rules = trials_at(60, 25);
        let twice = || {
            let mut world = field(200, 150, Pos::new(5, 5));
            for k in 1..6 {
                world.spawn_body(Pos::new(5 + k, 5));
            }
            let mut brains = piloted(&world);
            let mut news = Vec::new();
            let mut events = Vec::new();
            for _ in 0..3 * world.calendar().ticks_per_day() {
                events.clear();
                world.step(&rules, &mut events);
                for (_, decision) in brains.think(&world, &rules, &events) {
                    world.queue(decision.intention);
                }
                news.extend(brains.news());
            }
            (news, world.bodies().to_vec())
        };
        let (news, bodies) = twice();
        assert!(
            news.iter().any(|n| matches!(n, News::Trying { .. })),
            "hungry on a bare field, somebody tries: {news:?}"
        );
        assert_eq!((news, bodies), twice());
        // A body whose way is to sleep out lies down where night finds it;
        // the others walk home first.
        let mut world = field(64, 60, Pos::new(30, 30));
        let mut events = Vec::new();
        while !world.is_night(&rules) {
            world.advance(1, &rules, &mut events);
        }
        // Neither thirsty nor hungry: the night alone speaks.
        thirst(&mut world, 0);
        world.bodies[0].condition.hunger = Gauge::at(0);
        let id = world.bodies[0].id();
        let mut mind = Mind::new(Pos::new(5, 5));
        assert_eq!(
            decide_alone(&world, &rules, id, &mut mind),
            Some(Decision {
                intention: Intention::MoveTo {
                    body: id,
                    target: Pos::new(5, 5)
                },
                why: Why::Night
            })
        );
        let mut mind = Mind::new(Pos::new(5, 5));
        mind.ways.sleep_out = true;
        assert_eq!(
            decide_alone(&world, &rules, id, &mut mind),
            Some(Decision {
                intention: Intention::Sleep { body: id },
                why: Why::Night
            })
        );
    }

    // Company and the fire by place (D53, D55 amended 24 November 2026).

    /// The circle gathers by the home's place, not its tile: two
    /// neighbours a few tiles apart in one place of `place_tiles` sit at
    /// one fire, written once in the hearth at the place's middle.
    #[test]
    fn neighbours_in_one_place_share_one_fire() {
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(2, &rules);
        let ids: Vec<BodyId> = brains.minds.keys().copied().collect();
        // (5, 5) and (9, 10): two tiles, one place of 16.
        for (id, home) in ids.iter().zip([Pos::new(5, 5), Pos::new(9, 10)]) {
            let mind = brains.minds.get_mut(id).unwrap();
            mind.ways.home = home;
            mind.home_at_dusk = home;
            mind.night = Some(night_with([0; SENSES], 0, Vec::new()));
        }
        let news = night(&mut world, &rules, &mut brains, &ids);
        assert_eq!(
            brains.hearths().len(),
            1,
            "one fire: {:?}",
            brains.hearths()
        );
        let hearth = &brains.hearths()[&Pos::new(8, 8)];
        let Some(Record::Night { present, .. }) = brains.memory.get(hearth[0]) else {
            panic!("a night in the hearth");
        };
        assert_eq!(present.len(), 2);
        let others: Vec<u32> = news
            .iter()
            .filter_map(|n| match n {
                News::Company { others, .. } => Some(*others),
                _ => None,
            })
            .collect();
        assert_eq!(others, vec![1, 1], "each had the other");
    }

    /// The sociability is innate (D53, amended 24 November 2026): drawn
    /// from the run's seed and the id, 0 to 100, the same for the same
    /// seed, different from body to body and from seed to seed.
    #[test]
    fn sociability_is_drawn_from_the_seed_and_the_id() {
        let rules = fire_rules();
        let (world, _) = camp_of(12, &rules);
        let draw = |seed: u64| {
            let mut brains = Brains::new(seed);
            for body in world.bodies() {
                brains.adopt(&world, &rules, body.id());
            }
            brains
                .minds
                .values()
                .map(Mind::sociable)
                .collect::<Vec<_>>()
        };
        let first = draw(7);
        assert_eq!(first, draw(7), "the same seed, the same sociabilities");
        assert_ne!(first, draw(8), "another seed, others");
        assert!(first.iter().all(|&s| s <= 100));
        let distinct: BTreeSet<u8> = first.iter().copied().collect();
        assert!(distinct.len() > 6, "drawn body by body: {first:?}");
    }

    /// Company, felt at night at the fire, enters the night's notebook by
    /// sociability: with enough others nothing, with fewer in proportion,
    /// alone (not at the fire) the whole need; anyone counts alike.
    #[test]
    fn company_is_felt_at_the_fire_and_noted_with_the_night() {
        let mut rules = fire_rules();
        rules.needs.company = crate::content::Company::placeholders();
        let (mut world, mut brains) = camp_of(4, &rules);
        let ids: Vec<BodyId> = brains.minds.keys().copied().collect();
        for (id, sociable) in ids.iter().zip([100, 0, 50, 100]) {
            let mind = brains.minds.get_mut(id).unwrap();
            mind.sociable = sociable;
            mind.night = Some(night_with([0; SENSES], 0, Vec::new()));
        }
        // Three at the fire (two others each), the fourth not.
        night(&mut world, &rules, &mut brains, &ids[..3]);
        let lacked = |id: &BodyId| {
            let mind = &brains.minds[id];
            let l = mind
                .lived
                .iter()
                .find(|l| !l.grove)
                .expect("its night noted");
            l.per_night(l.losses[Sense::Company as usize])
        };
        let got: Vec<u64> = ids.iter().map(lacked).collect();
        assert_eq!(got, vec![666, 0, 250, 1000]);
    }

    /// Company grades places and gives no order (D53, amended 24 November
    /// 2026): the same night with and without it leaves every way, plan
    /// and trial alike, and only the notebook differs.
    #[test]
    fn company_orders_nothing() {
        let run = |on: bool| {
            let (mut world, mut rules, mut brains, _, _) = deciding(4);
            rules.needs.company = if on {
                crate::content::Company::placeholders()
            } else {
                crate::content::Company::default()
            };
            let ids: Vec<BodyId> = brains.minds.keys().copied().collect();
            for mind in brains.minds.values_mut() {
                mind.sociable = 100;
                mind.night = Some(night_with([0; SENSES], 0, Vec::new()));
            }
            let news: Vec<News> = night(&mut world, &rules, &mut brains, &ids[..2])
                .into_iter()
                .filter(|n| !matches!(n, News::Company { .. }))
                .collect();
            let minds: Vec<_> = brains
                .minds
                .values()
                .map(|m| (m.ways, m.plan, m.trial.map(|t| t.way)))
                .collect();
            let notes: Vec<_> = brains.minds.values().map(|m| m.lived.clone()).collect();
            (news, minds, notes)
        };
        let (with, without) = (run(true), run(false));
        assert_eq!(with.0, without.0, "the same news");
        assert_eq!(with.1, without.1, "the same ways, plans and trials");
        assert_ne!(with.2, without.2, "only the notebook differs");
    }

    // The fire and the well (D55).

    /// The repository's content (its crops name the kinds), trials on as
    /// [`trials_at`] (100, 0), the fire on with the content's placeholders.
    fn fire_rules() -> Content {
        let mut rules = crate::crops::tests::content();
        let forced = trials_at(100, 0).needs.trials;
        rules.needs.trials = Trials {
            trial_after_days: forced.trial_after_days,
            trial_days: forced.trial_days,
            try_percent_by_will: forced.try_percent_by_will,
            imitate_percent: forced.imitate_percent,
            imitate_loss_percent: forced.imitate_loss_percent,
            ..rules.needs.trials
        };
        rules.needs.fire = Fire::placeholders();
        rules
    }

    /// A field where brambles bear, `count` bodies at one camp, every
    /// body's will at 0 (nobody goes looking), the fire and the decisions
    /// of the placeholders (D56); the bramble's number and the camp.
    fn deciding(count: u32) -> (World, Content, Brains, u16, Pos) {
        let (mut world, mut rules, first, _) = by_a_ripe_bramble(0);
        rules.needs.fire = Fire::placeholders();
        rules.needs.decisions = crate::content::Decisions::placeholders();
        let at = world.body(first).unwrap().pos();
        for k in 1..count {
            let k = i32::try_from(k).unwrap();
            world.spawn_body(Pos::new(at.x + k % 3, at.y + k / 3));
        }
        let mut brains = piloted_with(&world, &rules);
        let camp = brains.minds[&first].ways.home;
        for mind in brains.minds.values_mut() {
            mind.will = 0;
            mind.ways.home = camp;
            mind.home_at_dusk = camp;
        }
        let bramble = brains.food.numbers["bramble"];
        (world, rules, brains, bramble, camp)
    }

    /// The decisions of one evening at `camp` among `present` (D56).
    fn decide_evening(
        world: &World,
        rules: &Content,
        brains: &mut Brains,
        camp: Pos,
        present: &[BodyId],
    ) -> Vec<News> {
        brains.food.refresh(world, rules);
        let day = u32::try_from(today(world)).unwrap();
        brains.fire_decide(world, rules, (camp, day - 1, world.tick().0), present, true);
        brains.news()
    }

    fn act_of(brains: &Brains, id: BodyId) -> (Act, Option<PlaceKey>, Option<BodyId>) {
        let plan = brains.minds[&id].plan.expect("a plan");
        (plan.act, plan.place, plan.leader)
    }

    /// A mind of `innate` will holding the placeholders' confidences
    /// (D50 amended, D56).
    fn tempered(innate: u8) -> (Mind, Temperance) {
        let temper = Temperance::placeholders();
        let mut mind = Mind::new(Pos::new(0, 0));
        mind.set_innate(innate, &temper);
        (mind, temper)
    }

    #[test]
    fn the_first_failures_cost_nothing_and_the_bold_bear_more_of_them() {
        let temper = Temperance::placeholders();
        assert_eq!((temper.bears(5), temper.bears(95)), (2, 5));
        let (mut timid, mut bold) = (
            Confidence {
                milli: 50_000,
                failures: 0,
                successes: 0,
            },
            Confidence {
                milli: 50_000,
                failures: 0,
                successes: 0,
            },
        );
        for _ in 0..5 {
            timid.fail(temper.bears(5), temper.failure_milli);
            bold.fail(temper.bears(95), temper.failure_milli);
        }
        // The timid: free, free, then 1, 2, 3 points; the bold: all free.
        assert_eq!(timid.milli, 44_000);
        assert_eq!(bold.milli, 50_000);
        // A sixth costs the bold its first point, the timid four more.
        timid.fail(temper.bears(5), temper.failure_milli);
        bold.fail(temper.bears(95), temper.failure_milli);
        assert_eq!((timid.milli, bold.milli), (40_000, 49_000));
    }

    #[test]
    fn a_success_weighs_more_the_later_it_comes_and_forgets_the_streak() {
        let temper = Temperance::placeholders();
        let early = temper.success_milli(2, 0, 25);
        let late = temper.success_milli(2, 6, 25);
        assert!(early > 0 && late == early * 7, "{early} {late}");
        let mut c = Confidence {
            milli: 20_000,
            failures: 6,
            successes: 0,
        };
        c.succeed(late);
        assert_eq!(c.failures, 0);
        assert_eq!(c.milli, 20_000 + u32::try_from(late).unwrap());
    }

    #[test]
    fn average_luck_keeps_the_will_where_it_is_bad_luck_wears_it_good_luck_raises_it() {
        let temper = Temperance::placeholders();
        // Gains and losses over many draws, unclamped, by a fixed
        // sequence (a linear congruence: the same everywhere).
        let run = |bears: u32, percent: u64| {
            let (mut gains, mut losses, mut failures) = (0_u64, 0_u64, 0_u32);
            let mut x: u64 = 12_345;
            for _ in 0..400_000 {
                x = x
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                if (x >> 33) % 100 < percent {
                    gains += temper.success_milli(bears, failures, temper.kept_percent);
                    failures = 0;
                } else {
                    failures += 1;
                    losses +=
                        u64::from(temper.failure_milli) * u64::from(failures.saturating_sub(bears));
                }
            }
            (gains, losses)
        };
        for bears in [2, 4, 6] {
            let (gains, losses) = run(bears, 25);
            let gap = gains.abs_diff(losses) * 100 / losses;
            assert!(gap <= 3, "bears {bears}: gains {gains}, losses {losses}");
            let (gains, losses) = run(bears, 15);
            assert!(gains < losses, "bad luck wears it: bears {bears}");
            let (gains, losses) = run(bears, 40);
            assert!(gains > losses, "good luck raises it: bears {bears}");
        }
    }

    #[test]
    fn a_day_that_succeeds_is_worth_the_odds_against_it() {
        let temper = Temperance::placeholders();
        assert_eq!(
            temper.day_success_milli(75),
            333,
            "explore: a third of a failure"
        );
        assert_eq!(temper.day_success_milli(50), 1_000, "gather: one failure");
        assert_eq!(temper.day_success_milli(90), 111, "stay: a ninth");
        // Over a hundred days at the expected rate, the successes weigh
        // what the failures would if each one cost.
        for percent in [50_u32, 75, 90] {
            let gains = temper.day_success_milli(percent) * u64::from(percent);
            let costs = u64::from(temper.failure_milli) * u64::from(100 - percent);
            assert!(
                gains.abs_diff(costs) <= 100,
                "{percent} %: {gains} against {costs}"
            );
        }
        let (mut mind, temper) = tempered(20);
        let explore = temper.activity("explore").unwrap();
        let before = mind.confidence[explore].milli;
        mind.judge_day(&temper, explore, true, (BodyId(1), 0));
        assert_eq!(
            mind.confidence[explore].milli,
            before + 333,
            "a place found counts"
        );
    }

    #[test]
    fn the_temperament_comes_back_half_its_gap_in_two_years() {
        let (mut mind, temper) = tempered(10);
        mind.will = 90;
        mind.confidence[1].milli = 90_000;
        for _ in 0..8 {
            mind.relax(&temper);
        }
        assert!((48..=52).contains(&mind.will), "{}", mind.will);
        assert!((48..=52).contains(&mind.confidence[1].points()));
        assert_eq!(mind.confidence[0].points(), 10, "at its innate already");
    }

    #[test]
    fn each_activity_keeps_its_own_streak() {
        let (mut mind, temper) = tempered(5);
        let at = (BodyId(1), 0);
        for _ in 0..3 {
            mind.judge_day(&temper, 0, false, at);
        }
        let news = mind.judge_day(&temper, 1, false, at);
        assert_eq!(
            (mind.confidence[0].failures, mind.confidence[1].failures),
            (3, 1)
        );
        assert!(matches!(
            news,
            Some(News::Judged {
                activity: 1,
                success: false,
                confidence: 5,
                ..
            })
        ));
        mind.judge_day(&temper, 1, true, at);
        assert_eq!(
            (mind.confidence[0].failures, mind.confidence[1].failures),
            (3, 0)
        );
        assert_eq!(
            mind.confidence[0].points(),
            4,
            "explore paid its third failure"
        );
        assert!(mind.confidence[1].milli > 5_000, "gather gained");
        assert_eq!(mind.will, 5, "the will moves by trials, not by days");
    }

    #[test]
    fn going_looking_is_its_habit_times_its_confidence_and_never_falls_to_nothing() {
        let (_, mut rules, _, _) = by_a_ripe_bramble(0);
        rules.needs.decisions = crate::content::Decisions::placeholders();
        rules.needs.temperance = Temperance::placeholders();
        let (mut mind, _) = tempered(20);
        assert_eq!(
            mind.lean_to_look(&rules),
            100,
            "even: no habit, even confidence"
        );
        mind.confidence[0].milli = 60_000;
        assert_eq!(mind.lean_to_look(&rules), 260);
        mind.habit.push(Act::Looked.code() | HABIT_FOUND);
        assert_eq!(
            mind.lean_to_look(&rules),
            150 * 260 / 100,
            "a look out that found weighs as in PR 1, times the confidence"
        );
        mind.habit.clear();
        mind.confidence[0].milli = 0;
        mind.confidence[1].milli = 100_000;
        assert_eq!(
            mind.lean_to_look(&rules),
            u64::from(rules.needs.decisions.habit_floor),
            "a weight, never a ban"
        );
    }

    #[test]
    fn being_joined_at_the_fire_gives_its_caller_confidence() {
        let (world, mut rules, mut brains, bramble, camp) = deciding(3);
        rules.needs.temperance = Temperance::placeholders();
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let p = (here.0 + 1, here.1);
        let food = brains.food.clone();
        for (id, mind) in &mut brains.minds {
            mind.places.insert(p, place_with(&food, bramble, 6));
            mind.set_innate(
                if *id == BodyId(3) { 60 } else { 20 },
                &rules.needs.temperance,
            );
        }
        decide_evening(
            &world,
            &rules,
            &mut brains,
            camp,
            &[BodyId(1), BodyId(2), BodyId(3)],
        );
        assert_eq!(act_of(&brains, BodyId(3)).0, Act::Called);
        let caller = &brains.minds[&BodyId(3)];
        // Two joined: half a point each, to its will and its gathering.
        assert_eq!((caller.will, caller.will_rest), (61, 0));
        assert_eq!(caller.confidence[1].milli, 61_000);
        assert_eq!(brains.minds[&BodyId(1)].will, 20, "joining gives nothing");
    }

    #[test]
    fn a_streak_of_successes_weighs_more_and_a_failure_ends_it() {
        let temper = Temperance::placeholders();
        assert_eq!(temper.streak_success_milli(50, 0), 1_000);
        assert_eq!(
            temper.streak_success_milli(50, 2),
            1_500,
            "halfway to twice"
        );
        assert_eq!(temper.streak_success_milli(50, 4), 2_000);
        assert_eq!(temper.streak_success_milli(50, 9), 2_000, "held beyond");
        let (mut mind, temper) = tempered(20);
        let gather = temper.activity("gather").unwrap();
        let at = (BodyId(1), 0);
        let mut gains = Vec::new();
        for _ in 0..3 {
            let before = mind.confidence[gather].milli;
            mind.judge_day(&temper, gather, true, at);
            gains.push(mind.confidence[gather].milli - before);
        }
        assert_eq!(gains, [1_000, 1_250, 1_500]);
        mind.judge_day(&temper, gather, false, at);
        assert_eq!(mind.confidence[gather].successes, 0, "a failure ends it");
        let before = mind.confidence[gather].milli;
        mind.judge_day(&temper, gather, true, at);
        assert_eq!(mind.confidence[gather].milli - before, 1_000);
    }

    #[test]
    fn a_follower_back_fed_counts_once_as_its_callers_day_of_gathering() {
        let (world, mut rules, id, _) = by_a_ripe_bramble(0);
        rules.needs.decisions = crate::content::Decisions::placeholders();
        rules.needs.temperance = Temperance::placeholders();
        let body = world.body(id).unwrap();
        let judged_gather = |act: Act, indirect: bool| {
            let (mut mind, _) = tempered(20);
            mind.plan = Some(Plan {
                day: 3,
                act,
                place: None,
                leader: None,
                octant: 0,
                legs: 0,
                done: false,
                asked: None,
            });
            let mut news = Vec::new();
            close_habit(
                &rules,
                body,
                id,
                &mut mind,
                (3, 0, 0),
                (Some(true), false, indirect),
                &mut news,
            );
            news.iter()
                .filter_map(|n| match n {
                    News::Judged {
                        activity: 1,
                        success,
                        ..
                    } => Some(*success),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            judged_gather(Act::Called, false),
            [false],
            "back with nothing"
        );
        assert_eq!(
            judged_gather(Act::Called, true),
            [true],
            "a member came back fed"
        );
        assert_eq!(
            judged_gather(Act::Nothing, true),
            [true],
            "it stayed, the one it answered came back fed: once"
        );
        assert_eq!(judged_gather(Act::Nothing, false), Vec::<bool>::new());
    }

    #[test]
    fn each_body_believes_its_own_best_a_belief_wears_and_fades() {
        let rep = crate::content::Reputation::placeholders();
        let mut memory = Store::new();
        let (a, b) = (BodyId(1), BodyId(2));
        let (x, y) = (BodyId(7), BodyId(8));
        let gather = activity_index("gather");
        let (mut ma, mut mb) = (Mind::new(Pos::new(0, 0)), Mind::new(Pos::new(0, 0)));
        assert!(ma.believe((a, a), (gather, x), 20, (&mut memory, &rep, 10)));
        assert!(mb.believe((b, b), (gather, x), 20, (&mut memory, &rep, 10)));
        assert_eq!(ma.believed(&memory, &rep, gather, 10), Some((x, 20)));
        let record = ma.reputed[0].access.record;
        assert_eq!(memory.refs(record), 2, "one record, held by each");
        // Each its own: b followed y and came back fed; a did not.
        assert!(!mb.believe((b, b), (gather, y), 10, (&mut memory, &rep, 10)));
        assert!(mb.believe((b, b), (gather, y), 30, (&mut memory, &rep, 11)));
        assert_eq!(mb.believed(&memory, &rep, gather, 11), Some((y, 30)));
        assert_eq!(ma.believed(&memory, &rep, gather, 11), Some((x, 19)));
        assert_eq!(memory.refs(record), 1, "b let it go");
        // Worn by a follower back empty, faded by the days.
        assert!(ma.believe((a, a), (gather, x), -15, (&mut memory, &rep, 12)));
        assert_eq!(ma.believed(&memory, &rep, gather, 12), Some((x, 3)));
        assert_eq!(ma.believed(&memory, &rep, gather, 14), Some((x, 1)));
        assert_eq!(ma.believed(&memory, &rep, gather, 15), None, "faded");
        assert!(
            !ma.believe((a, a), (gather, a), 50, (&mut memory, &rep, 15)),
            "never itself"
        );
        assert!(
            !ma.believe(
                (a, a),
                (activity_index("stay"), x),
                50,
                (&mut memory, &rep, 15)
            ),
            "an activity not reputed"
        );
        ma.forget_all(&mut memory);
        assert_eq!(memory.refs(record), 0);
    }

    #[test]
    fn a_caller_judges_a_companion_over_several_outings_never_on_one() {
        let dec = crate::content::Decisions::placeholders();
        let mut mind = Mind::new(Pos::new(0, 0));
        let who = BodyId(5);
        let fact = |day, flags| Fact { who, day, flags };
        mind.note(fact(1, FACT_WAITED), dec.facts_kept);
        assert_eq!(mind.review(who, &dec), Review::Unsaid, "one bad day");
        mind.note(fact(2, 0), dec.facts_kept);
        mind.note(fact(3, FACT_LITTLE), dec.facts_kept);
        assert_eq!(
            mind.review(who, &dec),
            Review::Aside,
            "two of its last three"
        );
        mind.note(fact(4, FACT_ASIDE), dec.facts_kept);
        assert_eq!(mind.review(who, &dec), Review::Unsaid, "judged anew");
        mind.note(fact(5, 0), dec.facts_kept);
        mind.note(fact(6, 0), dec.facts_kept);
        assert_eq!(mind.review(who, &dec), Review::Good, "two good in a row");
        for day in 7..50 {
            mind.note(fact(day, 0), dec.facts_kept);
        }
        assert_eq!(mind.companions.len(), 32, "facts_kept");
    }

    /// Three at a camp knowing a place `p` with room for all; 3 bold.
    fn reviewing() -> (World, Content, Brains, PlaceKey, Pos) {
        let (world, mut rules, mut brains, bramble, camp) = deciding(3);
        rules.needs.temperance = Temperance::placeholders();
        rules.needs.reputation = crate::content::Reputation::placeholders();
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let p = (here.0 + 1, here.1);
        let food = brains.food.clone();
        for (id, mind) in &mut brains.minds {
            mind.places.insert(p, place_with(&food, bramble, 12));
            mind.set_innate(
                if *id == BodyId(3) { 60 } else { 20 },
                &rules.needs.temperance,
            );
        }
        (world, rules, brains, p, camp)
    }

    #[test]
    fn the_caller_calls_the_good_first_and_sets_aside_the_slow_but_not_its_child() {
        let (mut world, rules, mut brains, p, camp) = reviewing();
        let (one, two, lead) = (BodyId(1), BodyId(2), BodyId(3));
        let keep = rules.needs.decisions.facts_kept;
        let mind = brains.minds.get_mut(&lead).unwrap();
        for day in 1..=2 {
            mind.note(
                Fact {
                    who: one,
                    day,
                    flags: FACT_WAITED,
                },
                keep,
            );
            mind.note(
                Fact {
                    who: two,
                    day,
                    flags: 0,
                },
                keep,
            );
        }
        let all = [one, two, lead];
        let news = decide_evening(&world, &rules, &mut brains, camp, &all);
        assert!(news.iter().any(|n| matches!(n,
            News::Aside { leader, body, .. } if *leader == lead && *body == one)));
        assert_eq!(act_of(&brains, two), (Act::Joined, Some(p), Some(lead)));
        assert_ne!(act_of(&brains, one).2, Some(lead), "refused");
        assert_eq!(
            brains.minds[&one].confidence[1].failures, 1,
            "a failure, borne free"
        );
        let review = news.iter().find_map(|n| match n {
            News::Evening { review, .. } => Some(*review),
            _ => None,
        });
        assert_eq!(
            review.map(|r| (r.named, r.aside, r.refused)),
            Some((1, 1, 1))
        );
        // The next evening it was set aside: judged anew, it may come.
        let news = decide_evening(&world, &rules, &mut brains, camp, &all);
        assert!(!news.iter().any(|n| matches!(n, News::Aside { .. })));
        // Its own child it never sets aside.
        world.set_parents(one, None, Some(lead));
        let mind = brains.minds.get_mut(&lead).unwrap();
        for day in 3..=4 {
            mind.note(
                Fact {
                    who: one,
                    day,
                    flags: FACT_LITTLE,
                },
                keep,
            );
        }
        let news = decide_evening(&world, &rules, &mut brains, camp, &all);
        assert!(!news.iter().any(|n| matches!(n, News::Aside { .. })));
        assert_eq!(act_of(&brains, one).2, Some(lead));
    }

    #[test]
    fn the_review_holds_only_to_the_parts_it_is_given() {
        let note = |brains: &mut Brains, keep: u32, (one, two, lead): (BodyId, BodyId, BodyId)| {
            let mind = brains.minds.get_mut(&lead).unwrap();
            for day in 1..=2 {
                mind.note(
                    Fact {
                        who: one,
                        day,
                        flags: FACT_WAITED,
                    },
                    keep,
                );
                mind.note(
                    Fact {
                        who: two,
                        day,
                        flags: 0,
                    },
                    keep,
                );
            }
        };
        let trio = (BodyId(1), BodyId(2), BodyId(3));
        let all = [trio.0, trio.1, trio.2];
        let evening = |parts: &[&str]| {
            let (world, mut rules, mut brains, _, camp) = reviewing();
            rules.needs.decisions.review_parts = parts.iter().map(|&p| p.to_owned()).collect();
            note(&mut brains, rules.needs.decisions.facts_kept, trio);
            let news = decide_evening(&world, &rules, &mut brains, camp, &all);
            let review = news.iter().find_map(|n| match n {
                News::Evening { review, .. } => Some((review.named, review.aside)),
                _ => None,
            });
            let named = news.iter().any(|n| matches!(n, News::Named { .. }));
            (review, named, brains.minds[&trio.0].confidence[1].failures)
        };
        assert_eq!(evening(&[]), (Some((0, 0)), false, 0), "the facts alone");
        assert_eq!(
            evening(&["set_aside"]),
            (Some((0, 1)), false, 0),
            "set aside, free"
        );
        assert_eq!(
            evening(&["set_aside", "aside_costs"]),
            (Some((0, 1)), false, 1),
            "set aside, at a cost"
        );
        assert_eq!(evening(&["names"]), (Some((1, 0)), true, 0), "named only");
    }

    #[test]
    fn one_with_no_place_asks_whom_it_believes_best_and_goes_where_it_is_told() {
        let (world, rules, mut brains, p, camp) = reviewing();
        let (asker, answerer) = (BodyId(1), BodyId(2));
        let side = place_side(&rules);
        let q = (p.0 + 2, p.1);
        let bramble = brains.food.numbers["bramble"];
        let food = brains.food.clone();
        let day = u32::try_from(today(&world)).unwrap();
        let (record, _) = brains.memory.intern(Record::Place {
            place: q,
            day: day - 1,
            witness: answerer,
            states: vec![(bramble, 2)],
        });
        for (id, mind) in &mut brains.minds {
            mind.places.clear();
            if *id == answerer {
                let mut place = place_with(&food, bramble, 12);
                place.access = Some(Access {
                    record,
                    by: answerer,
                    day: day - 1,
                });
                mind.places.insert(q, place);
            }
        }
        let rep = rules.needs.reputation.clone();
        let mind = brains.minds.get_mut(&asker).unwrap();
        mind.believe(
            (asker, asker),
            (activity_index("explore"), answerer),
            40,
            (&mut brains.memory, &rep, day),
        );
        let all = [asker, answerer, BodyId(3)];
        let before = (brains.clone(), world.tick());
        let news = decide_evening(&world, &rules, &mut brains, camp, &all);
        assert!(news.iter().any(|n| matches!(n,
            News::Asked { body, of, place: Some(at), .. }
                if *body == asker && *of == answerer && *at == q)));
        let plan = brains.minds[&asker].plan.unwrap();
        assert_eq!(plan.place, Some(q), "where it was told");
        assert_eq!(plan.asked, Some((answerer, activity_index("explore"))));
        assert!(within(place_centre(q, side), camp, 200));
        // The third believes nobody: it does not ask.
        assert_eq!(
            news.iter()
                .filter(|n| matches!(n, News::Asked { .. }))
                .count(),
            1
        );
        // The same world, the same answers.
        let (mut again, _) = before;
        let replay = decide_evening(&world, &rules, &mut again, camp, &all);
        assert_eq!(news, replay);
        assert_eq!(again.minds[&asker].plan, brains.minds[&asker].plan);
    }

    #[test]
    fn at_nightfall_a_follower_back_fed_believes_its_caller_and_the_caller_notes_who_it_waited_for()
    {
        let (world, rules, mut brains, p, _) = reviewing();
        let (member, lead) = (BodyId(1), BodyId(3));
        let day = today(&world);
        let day32 = u32::try_from(day).unwrap();
        let plan = |act, leader| Plan {
            day: day32,
            act,
            place: Some(p),
            leader,
            octant: 0,
            legs: 0,
            done: false,
            asked: None,
        };
        brains.minds.get_mut(&member).unwrap().plan = Some(plan(Act::Joined, Some(lead)));
        brains.minds.get_mut(&lead).unwrap().plan = Some(plan(Act::Called, Some(lead)));
        let need = need_kj_per_day(&rules, world.body(member).unwrap());
        brains.minds.get_mut(&member).unwrap().gathered_today_kj = need;
        brains.groups.insert(
            lead,
            Group {
                day: day32,
                called: p,
                place: p,
                members: vec![lead, member],
                taken: BTreeMap::new(),
                full: BTreeSet::new(),
                followed: BTreeSet::new(),
                left: BTreeSet::new(),
                arrived: BTreeMap::new(),
                dawn: 0,
                home: false,
                ended: None,
                held: false,
                waited: 0,
                started: true,
                waited_for: BTreeSet::from([member]),
            },
        );
        let follows = brains.follows_today(&world, &rules, day);
        assert_eq!(follows.len(), 1);
        assert!(follows[0].fed && follows[0].whom == lead && follows[0].how == FOLLOW_JOINED);
        let gathered = BTreeMap::from([(member, need), (lead, 0)]);
        brains.review_day(&world, &rules, day, (&follows, &gathered));
        let rep = &rules.needs.reputation;
        assert_eq!(
            brains.minds[&member].believed(&brains.memory, rep, activity_index("gather"), day32),
            Some((lead, 20))
        );
        assert_eq!(
            brains.minds[&lead].companions,
            [Fact {
                who: member,
                day: day32,
                flags: FACT_WAITED
            }],
            "it waited for it; it came back fed"
        );
        assert_eq!(brains.accounts, [(lead, vec![member], 1, 0)]);
    }

    #[test]
    fn the_bold_speak_first_a_witness_calls_for_what_it_saw_and_a_place_taken_whole_is_passed_over()
    {
        let (world, rules, mut brains, bramble, camp) = deciding(3);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let (p, q) = ((here.0 + 1, here.1), (here.0 + 3, here.1));
        let food = brains.food.clone();
        for mind in brains.minds.values_mut() {
            // P: four brambles holding, room for two; Q: one, room for one.
            mind.places.insert(p, place_with(&food, bramble, 4));
            mind.places.insert(q, place_with(&food, bramble, 1));
        }
        // 3 is the boldest and speaks first, then 1, then 2.
        brains.minds.get_mut(&BodyId(3)).unwrap().will = 60;
        brains.minds.get_mut(&BodyId(1)).unwrap().will = 50;
        brains.minds.get_mut(&BodyId(2)).unwrap().will = 30;
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        let news = decide_evening(
            &world,
            &rules,
            &mut brains,
            camp,
            &[BodyId(1), BodyId(2), BodyId(3)],
        );
        assert_eq!(
            act_of(&brains, BodyId(3)),
            (Act::Called, Some(p), Some(BodyId(3)))
        );
        assert_eq!(
            act_of(&brains, BodyId(1)),
            (Act::Joined, Some(p), Some(BodyId(3)))
        );
        // P's room taken whole: 2 passes it over, calls Q, nobody joins:
        // it goes alone.
        assert_eq!(act_of(&brains, BodyId(2)), (Act::Alone, Some(q), None));
        assert_eq!(
            brains.groups[&BodyId(3)].members,
            vec![BodyId(3), BodyId(1)]
        );
        assert!(
            news.iter().any(|n| matches!(
                n,
                News::Evening {
                    skipped: 1,
                    calls_full: 1,
                    ..
                }
            )),
            "{news:?}"
        );
    }

    #[test]
    fn a_place_only_heard_of_is_gone_to_alone_and_takes_room_by_its_told_state() {
        let (world, rules, mut brains, bramble, camp) = deciding(3);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let r = (here.0 + 1, here.1 + 1);
        let food = brains.food.clone();
        for (i, mind) in brains.minds.values_mut().enumerate() {
            // Heard of from body 9, as two brambles holding: room for one.
            let mut place = place_with(&food, bramble, 2);
            place.access = Some(Access {
                record: 1,
                by: BodyId(9),
                day: 1,
            });
            mind.places.insert(r, place);
            mind.will = u8::try_from(30 - i).unwrap();
        }
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        decide_evening(
            &world,
            &rules,
            &mut brains,
            camp,
            &[BodyId(1), BodyId(2), BodyId(3)],
        );
        assert_eq!(act_of(&brains, BodyId(1)), (Act::Alone, Some(r), None));
        // Nobody leads on hearsay; the room taken, the others say nothing.
        assert_eq!(act_of(&brains, BodyId(2)).0, Act::Nothing);
        assert_eq!(act_of(&brains, BodyId(3)).0, Act::Nothing);
        assert!(brains.groups.is_empty());
    }

    #[test]
    fn a_child_takes_its_mothers_decision() {
        let (mut world, rules, mut brains, bramble, camp) = deciding(3);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let p = (here.0 + 1, here.1);
        let food = brains.food.clone();
        for mind in brains.minds.values_mut() {
            mind.places.insert(p, place_with(&food, bramble, 4));
        }
        // 3 is a child of five whose mother is 2; 1 speaks first.
        world.set_birth(BodyId(3), 5 * 50, crate::Sex::Female);
        world
            .bodies
            .iter_mut()
            .find(|b| b.id() == BodyId(3))
            .unwrap()
            .mother = Some(BodyId(2));
        brains.minds.get_mut(&BodyId(1)).unwrap().will = 50;
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        decide_evening(
            &world,
            &rules,
            &mut brains,
            camp,
            &[BodyId(1), BodyId(2), BodyId(3)],
        );
        assert_eq!(
            act_of(&brains, BodyId(1)),
            (Act::Called, Some(p), Some(BodyId(1)))
        );
        assert_eq!(
            act_of(&brains, BodyId(2)),
            (Act::Joined, Some(p), Some(BodyId(1)))
        );
        // Past the room, with its mother all the same.
        assert_eq!(
            act_of(&brains, BodyId(3)),
            (Act::Joined, Some(p), Some(BodyId(1)))
        );
        assert_eq!(brains.groups[&BodyId(1)].members.len(), 3);
    }

    #[test]
    fn going_looking_is_drawn_by_will_toward_the_least_known_way_and_weighed_by_habit() {
        let (world, rules, mut brains, bramble, camp) = deciding(2);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let food = brains.food.clone();
        for mind in brains.minds.values_mut() {
            // Places east only: west, south and north are the least known.
            mind.places
                .insert((here.0 + 2, here.1), place_with(&food, bramble, 2));
        }
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0), (100, 100)];
        brains.minds.get_mut(&BodyId(1)).unwrap().will = 100;
        decide_evening(&world, &rules, &mut brains, camp, &[BodyId(1), BodyId(2)]);
        let plan = brains.minds[&BodyId(1)].plan.unwrap();
        assert_eq!(plan.act, Act::Looked);
        assert_ne!(
            DIRECTIONS[usize::from(plan.octant)],
            (1, 0),
            "not the known way"
        );
        // A will of 0 never looks.
        assert_ne!(act_of(&brains, BodyId(2)).0, Act::Looked);
        // The habit: two days looked and found, one for nothing: +50 %.
        let dec = &rules.needs.decisions;
        let mut mind = Mind::new(camp);
        let looked = Act::Looked.code();
        mind.habit = vec![
            looked | HABIT_FOUND,
            looked | HABIT_FOUND,
            looked,
            Act::Alone.code(),
        ];
        assert_eq!(mind.look_weight(dec), 150);
        mind.habit = vec![looked; 10];
        assert_eq!(
            mind.look_weight(dec),
            u64::from(dec.habit_floor),
            "never under the floor"
        );
    }

    #[test]
    fn the_same_evening_gives_the_same_decisions_and_without_them_nothing_is_said() {
        let (world, rules, brains, bramble, camp) = deciding(5);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let food = brains.food.clone();
        let mut brains = brains;
        for (i, mind) in brains.minds.values_mut().enumerate() {
            let i = i32::try_from(i).unwrap();
            mind.places
                .insert((here.0 + 1 + i % 2, here.1), place_with(&food, bramble, 3));
            mind.will = u8::try_from(20 + 15 * i).unwrap();
        }
        let present: Vec<BodyId> = brains.minds.keys().copied().collect();
        let mut again = brains.clone();
        let a = decide_evening(&world, &rules, &mut brains, camp, &present);
        let b = decide_evening(&world, &rules, &mut again, camp, &present);
        assert_eq!(a, b);
        let mut off = rules.clone();
        off.needs.decisions = crate::content::Decisions::default();
        let mut quiet = brains.clone();
        for mind in quiet.minds.values_mut() {
            mind.plan = None;
        }
        quiet.groups.clear();
        let news = decide_evening(&world, &off, &mut quiet, camp, &present);
        assert!(news.is_empty(), "{news:?}");
        assert!(quiet.minds.values().all(|m| m.plan.is_none()));
    }

    #[test]
    fn a_member_waits_for_its_caller_then_walks_behind_it() {
        let (mut world, rules, mut brains, bramble, camp) = deciding(2);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let p = (here.0 + 3, here.1);
        let food = brains.food.clone();
        for mind in brains.minds.values_mut() {
            mind.places.insert(p, place_with(&food, bramble, 4));
        }
        brains.minds.get_mut(&BodyId(1)).unwrap().will = 50;
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        decide_evening(&world, &rules, &mut brains, camp, &[BodyId(1), BodyId(2)]);
        assert_eq!(act_of(&brains, BodyId(2)).0, Act::Joined);
        let decide_now = |world: &World, brains: &mut Brains, id: BodyId| {
            let mut mind = brains.minds.remove(&id).unwrap();
            let mut news = Vec::new();
            let mut land = Land {
                groups: &mut brains.groups,
                bearers: &mut brains.bearers,
                shores: &mut brains.shores,
                food: &mut brains.food,
                memory: &mut brains.memory,
                news: &mut news,
            };
            let decision = decide(world, &rules, &mut land, id, &mut mind, 7);
            brains.minds.insert(id, mind);
            decision
        };
        // The caller has not set off: the member waits by it.
        assert_eq!(decide_now(&world, &mut brains, BodyId(2)), None);
        // The caller sets off; the member walks behind it.
        let lead = decide_now(&world, &mut brains, BodyId(1)).expect("it goes");
        assert!(
            matches!(lead.intention, Intention::MoveTo { .. }),
            "{lead:?}"
        );
        world.queue(lead.intention);
        let mut events = Vec::new();
        world.advance(2, &rules, &mut events);
        assert!(world.body(BodyId(1)).unwrap().is_walking());
        let follow = decide_now(&world, &mut brains, BodyId(2)).expect("it goes");
        assert!(
            matches!(
                follow.intention,
                Intention::Follow {
                    leader: BodyId(1),
                    ..
                }
            ),
            "{follow:?}"
        );
        assert!(brains.groups[&BodyId(1)].followed.contains(&BodyId(2)));
    }

    #[test]
    fn the_caller_looks_behind_waits_for_one_left_far_then_goes_on() {
        let (mut world, rules, mut brains, bramble, camp) = deciding(2);
        let side = place_side(&rules);
        let here = place_of(camp, side);
        let p = (here.0 + 3, here.1);
        let food = brains.food.clone();
        for mind in brains.minds.values_mut() {
            mind.places.insert(p, place_with(&food, bramble, 4));
        }
        brains.minds.get_mut(&BodyId(1)).unwrap().will = 50;
        let mut rules = rules;
        rules.needs.decisions.look_percent_by_will = vec![(0, 0)];
        assert_eq!(rules.needs.decisions.behind_tiles, 24);
        decide_evening(&world, &rules, &mut brains, camp, &[BodyId(1), BodyId(2)]);
        let decide_now = |world: &World, brains: &mut Brains, id: BodyId| {
            let mut mind = brains.minds.remove(&id).unwrap();
            let mut news = Vec::new();
            let mut land = Land {
                groups: &mut brains.groups,
                bearers: &mut brains.bearers,
                shores: &mut brains.shores,
                food: &mut brains.food,
                memory: &mut brains.memory,
                news: &mut news,
            };
            let decision = decide(world, &rules, &mut land, id, &mut mind, 7);
            brains.minds.insert(id, mind);
            decision
        };
        let mut events = Vec::new();
        let lead = decide_now(&world, &mut brains, BodyId(1)).expect("it goes");
        world.queue(lead.intention);
        world.advance(2, &rules, &mut events);
        assert!(world.body(BodyId(1)).unwrap().is_walking());
        // None left behind: it walks on and looks again later.
        assert_eq!(decide_now(&world, &mut brains, BodyId(1)), None);
        // Its member is taken far behind, and it sees it walking after it:
        // it stops where it stands.
        let at = world.body(BodyId(1)).unwrap().pos();
        world.queue(Intention::Teleport {
            body: BodyId(2),
            target: Pos::new(at.x + 40, at.y),
        });
        world.advance(1, &rules, &mut events);
        // It walks toward its caller, in its sight.
        world.queue(Intention::MoveTo {
            body: BodyId(2),
            target: at,
        });
        world.advance(1, &rules, &mut events);
        let far = world.body(BodyId(2)).unwrap().pos();
        assert!(
            (far.x - at.x).abs() > 30 && world.body(BodyId(2)).unwrap().is_walking(),
            "{far:?} {at:?} {events:?}"
        );
        let stop = decide_now(&world, &mut brains, BodyId(1)).expect("it stops");
        let at = world.body(BodyId(1)).unwrap().pos();
        assert!(
            matches!(stop.intention, Intention::MoveTo { target, .. } if target == at),
            "{stop:?}"
        );
        assert!(brains.groups[&BodyId(1)].held);
        world.queue(stop.intention);
        world.advance(2, &rules, &mut events);
        assert!(!world.body(BodyId(1)).unwrap().is_walking());
        // Still far: it waits, and its wait is counted.
        assert_eq!(decide_now(&world, &mut brains, BodyId(1)), None);
        assert!(brains.groups[&BodyId(1)].waited > 0);
        // Its member comes within reach: it goes on to the place.
        world.queue(Intention::Teleport {
            body: BodyId(2),
            target: Pos::new(at.x - 3, at.y),
        });
        world.advance(1, &rules, &mut events);
        let go = decide_now(&world, &mut brains, BodyId(1)).expect("it goes on");
        assert!(
            matches!(go.intention, Intention::MoveTo { target, .. } if target != at),
            "{go:?}"
        );
        assert!(!brains.groups[&BodyId(1)].held);
    }

    /// The brains piloting every body of `world` under `rules`.
    fn piloted_with(world: &World, rules: &Content) -> Brains {
        let mut brains = Brains::new(7);
        for body in world.bodies() {
            brains.adopt(world, rules, body.id());
        }
        brains
    }

    /// A field with `count` bodies at the camp (5, 5), the world a day
    /// old, piloted.
    fn camp_of(count: i32, rules: &Content) -> (World, Brains) {
        let mut world = field(64, 60, Pos::new(5, 5));
        for k in 1..count {
            world.spawn_body(Pos::new(5 + k % 3, 5 + k / 3));
        }
        let mut events = Vec::new();
        world.advance(world.calendar().ticks_per_day(), rules, &mut events);
        let brains = piloted_with(&world, rules);
        (world, brains)
    }

    /// The night: every body of `present` lies down at the camp, the day
    /// turns (everybody kept fed and watered: the fire is what is
    /// tested), and the fire is read at dawn. The news of the night.
    fn night(
        world: &mut World,
        rules: &Content,
        brains: &mut Brains,
        present: &[BodyId],
    ) -> Vec<News> {
        for &id in present {
            brains.join_fire(world, rules, id);
        }
        // The day turns in eighths, every gauge put back between them:
        // a bare field in the repository's content freezes and parches
        // a body in a night, and the fire is what is tested here.
        let mut events = Vec::new();
        for _ in 0..8 {
            world.advance(world.calendar().ticks_per_day() / 8, rules, &mut events);
            for body in &mut world.bodies {
                body.condition.thirst = Gauge::at(0);
                body.condition.hunger = Gauge::at(0);
                body.condition.cold = Gauge::at(0);
                body.condition.fatigue = Gauge::at(0);
                body.condition.deficit_j = 0;
            }
        }
        brains.vigil(world, rules);
        brains.news()
    }

    /// A place `key` seen by `id` holding `holding` of `kind`, remembered
    /// (its look's day: today).
    fn saw_place(
        world: &World,
        rules: &Content,
        brains: &mut Brains,
        id: BodyId,
        key: PlaceKey,
        kind: u16,
        holding: u16,
    ) {
        let food = brains.food.clone();
        let best = food.best_kj[usize::from(kind)];
        let seen = BTreeMap::from([(
            key,
            vec![Seen {
                kind,
                plants: holding.max(1),
                holding,
                best_kj: if holding > 0 { best } else { 0 },
            }],
        )]);
        let day = u32::try_from(today(world)).unwrap();
        let mind = brains.minds.get_mut(&id).unwrap();
        remember_places(
            &food,
            rules,
            (id, Pos::new(5, 5), day, world.tick().0),
            mind,
            seen,
            &mut brains.memory,
            &mut brains.news,
        );
    }

    /// A kept trial of `teller` written in the well and heard by `id`:
    /// "`teller` tried `way` and kept it".
    fn heard_kept(
        brains: &mut Brains,
        (id, teller, day): (BodyId, BodyId, u32),
        way: Way,
        after: Ways,
        will: u8,
    ) -> RecordId {
        let (record, _) = brains.memory.intern(Record::Trial {
            body: teller,
            day,
            way,
            after,
            up: true,
            kept: true,
            will,
            before: Lately::default(),
            during: Lately {
                kj_per_day: 3000,
                ..Lately::default()
            },
        });
        brains.minds.get_mut(&id).unwrap().heard_ways.push(Access {
            record,
            by: teller,
            day,
        });
        record
    }

    #[test]
    fn at_the_fire_a_place_is_told_as_a_record_written_once_and_the_listener_gets_the_record_not_the_world()
     {
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(3, &rules);
        let (teller, other, listener) = (BodyId(1), BodyId(2), BodyId(3));
        let kind = brains.food.numbers["bramble"];
        let key = (2, 2);
        // Two looks at the same thing the same day: one record.
        saw_place(&world, &rules, &mut brains, teller, key, kind, 5);
        saw_place(&world, &rules, &mut brains, other, key, kind, 7);
        let news = night(&mut world, &rules, &mut brains, &[teller, other, listener]);
        let told: Vec<(BodyId, RecordId)> = news
            .iter()
            .filter_map(|n| match n {
                News::Told { body, record, .. } => Some((*body, *record)),
                _ => None,
            })
            .collect();
        assert_eq!(told.len(), 2, "{news:?}");
        assert_eq!(
            told[0].1, told[1].1,
            "the same place, the same state, the same day: one number"
        );
        let record = told[0].1;
        assert_eq!(brains.memory.len(), 2, "the place, and the night");
        assert_eq!(
            brains.memory.get(record).and_then(Record::witness),
            Some(teller),
            "signed by the first who told it"
        );
        // The listener holds an access to the teller's record, by the
        // teller, and its place says what the record says (full), not
        // what the world holds (nothing).
        let mind = brains.mind(listener).unwrap();
        let believes: Vec<(PlaceKey, Access)> = mind.believes().collect();
        assert_eq!(believes.len(), 1, "{believes:?}");
        assert_eq!(believes[0].0, key);
        assert_eq!(believes[0].1.record, record);
        let from = believes[0].1.by;
        assert!(
            from == teller || from == other,
            "from the teller it credits most: {from:?}"
        );
        let place = &mind.places[&key];
        assert_eq!(place.kinds[0].holding, 4, "full: four and more");
        assert!(
            place.kinds[0].best_kj < brains.food.best_kj[usize::from(kind)],
            "worth less by its credit"
        );
        assert!(place.kinds[0].best_kj > 0);
        // The credit was judged once: asked again, the same.
        let heard: Vec<u8> = news
            .iter()
            .filter_map(|n| match n {
                News::Heard { body, credit, .. } if *body == listener => Some(*credit),
                _ => None,
            })
            .collect();
        assert_eq!(heard.len(), 1);
        let again = memory::credit_lasting(
            brains.seed,
            (listener, brains.mind(listener).unwrap().will()),
            (from, brains.mind(from).unwrap().will()),
            (1, 1, 2),
            record_days(&world, &rules, &brains.food, brains.memory.get(record)),
        );
        assert_eq!(heard[0], again);
        // The tellers hold their own record, the listener its access, and
        // the night its two tellings.
        assert_eq!(brains.memory.refs(record), 5);
        assert_eq!(brains.hearths()[&Pos::new(8, 8)].len(), 1);
    }

    #[test]
    fn a_place_heard_is_believed_less_as_it_ages_as_fast_as_what_it_bears_goes() {
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(2, &rules);
        let (teller, listener) = (BodyId(1), BodyId(2));
        let leaves = brains.food.numbers["dandelion"];
        let roots = brains.food.numbers["cattail"];
        brains.food.refresh(&world, &rules);
        let quick = told_days(&world, &rules, &brains.food, leaves);
        let slow = told_days(&world, &rules, &brains.food, roots);
        assert!(
            quick < slow,
            "leaves picked in minutes are old news before roots: {quick} {slow} days"
        );
        saw_place(&world, &rules, &mut brains, teller, (2, 2), leaves, 5);
        saw_place(&world, &rules, &mut brains, teller, (3, 3), roots, 5);
        night(&mut world, &rules, &mut brains, &[teller, listener]);
        let believed = |brains: &Brains, key: PlaceKey, kind: u16| {
            brains.mind(listener).unwrap().places[&key].kinds[0].best_kj * 100
                / brains.food.best_kj[usize::from(kind)]
        };
        let (leaves_1, roots_1) = (
            believed(&brains, (2, 2), leaves),
            believed(&brains, (3, 3), roots),
        );
        assert!(
            leaves_1 < roots_1,
            "told the same evening, at dawn the leaves are believed less: {leaves_1} {roots_1}"
        );
        // A day more, nobody at the fire: what it holds ages.
        night(&mut world, &rules, &mut brains, &[]);
        let roots_2 = believed(&brains, (3, 3), roots);
        assert!(
            roots_2 < roots_1,
            "a day older, believed less: {roots_2} {roots_1}"
        );
        assert_eq!(
            believed(&brains, (2, 2), leaves),
            leaves_1,
            "the leaves lost all their age's credit already"
        );
    }

    #[test]
    fn a_denial_takes_no_slot_outlives_the_place_forgotten_and_reaches_only_who_held_the_record() {
        let mut rules = fire_rules();
        rules.needs.fire.told_per_night = 1;
        rules.needs.fire.denied_per_night = 1;
        rules.needs.thought.places_remembered = 3;
        let (mut world, mut brains) = camp_of(4, &rules);
        let (teller, finder, holder, stranger) = (BodyId(1), BodyId(2), BodyId(3), BodyId(4));
        let kind = brains.food.numbers["bramble"];
        let (rumour, other, good) = ((4, 4), (5, 5), (0, 0));
        // Night 1: the teller tells the rumour (its one slot); the
        // stranger is away. Night 1 b: the other place, the same way.
        saw_place(&world, &rules, &mut brains, teller, rumour, kind, 5);
        night(&mut world, &rules, &mut brains, &[teller, finder, holder]);
        saw_place(&world, &rules, &mut brains, teller, other, kind, 5);
        night(&mut world, &rules, &mut brains, &[teller, finder, holder]);
        let held = |brains: &Brains, id: BodyId, key: PlaceKey| {
            brains
                .mind(id)
                .unwrap()
                .believes()
                .find(|(k, _)| *k == key)
                .map(|(_, a)| a.record)
        };
        let r_rumour = held(&brains, holder, rumour).expect("heard");
        let r_other = held(&brains, holder, other).expect("heard");
        // The finder finds both bare, then sees good places: the bare
        // ones are forgotten first, their denials kept apart.
        saw_place(&world, &rules, &mut brains, finder, rumour, kind, 0);
        saw_place(&world, &rules, &mut brains, finder, other, kind, 0);
        for key in [good, (1, 0), (0, 1)] {
            saw_place(&world, &rules, &mut brains, finder, key, kind, 6);
        }
        assert!(
            !brains.mind(finder).unwrap().places.contains_key(&rumour),
            "forgotten: three places remembered, the farthest last"
        );
        brains.news();
        let news = night(&mut world, &rules, &mut brains, &[finder, holder, stranger]);
        let told: Vec<RecordId> = news
            .iter()
            .filter_map(|n| match n {
                News::Told { body, record, .. } if *body == finder => Some(*record),
                _ => None,
            })
            .collect();
        assert_eq!(
            told.len(),
            2,
            "a good place in its one slot, and a denial: {news:?}"
        );
        let denial = told[1];
        assert!(
            matches!(brains.memory.get(denial), Some(Record::Place { place, states, .. }) if *place == rumour && states.iter().all(|s| s.1 == 0)),
            "the rumour worth most (the smaller place at a tie), denied"
        );
        assert_eq!(
            held(&brains, holder, rumour),
            Some(denial),
            "who held it replaces it"
        );
        assert_eq!(
            held(&brains, holder, other),
            Some(r_other),
            "one denial a night: the other rumour is kept"
        );
        assert_ne!(r_rumour, denial);
        assert!(
            held(&brains, stranger, good).is_some(),
            "the good news is heard"
        );
        assert!(
            !brains.mind(stranger).unwrap().places.contains_key(&rumour),
            "a denial adds no place to who never held it"
        );
    }

    #[test]
    fn a_strong_credit_is_kept_before_a_weak_one_and_a_child_keeps_fewer() {
        let mut rules = fire_rules();
        rules.needs.fire.heard_max = 1;
        let (mut world, mut brains) = camp_of(3, &rules);
        let (strong, weak, listener) = (BodyId(1), BodyId(2), BodyId(3));
        let kind = brains.food.numbers["bramble"];
        brains.minds.get_mut(&strong).unwrap().will = 100;
        brains.minds.get_mut(&weak).unwrap().will = 0;
        saw_place(&world, &rules, &mut brains, strong, (2, 2), kind, 5);
        saw_place(&world, &rules, &mut brains, weak, (3, 3), kind, 5);
        night(&mut world, &rules, &mut brains, &[strong, weak, listener]);
        let believes: Vec<(PlaceKey, Access)> = brains.mind(listener).unwrap().believes().collect();
        assert_eq!(believes.len(), 1);
        assert_eq!(
            believes[0].1.by, strong,
            "one thing kept: the strong will's"
        );
        // A two-year-old keeps `heard_by_age` things and not one more.
        let mut rules = fire_rules();
        rules.needs.fire.heard_by_age = vec![(2, 3), (16, 24)];
        rules.needs.fire.told_per_night = 10;
        let (mut world, mut brains) = camp_of(3, &rules);
        let (teller, grown, child) = (BodyId(1), BodyId(2), BodyId(3));
        let per_year = i64::from(world.calendar().days_per_year());
        world.bodies[2].born_day = world.today() - 2 * per_year;
        for k in 0..8_i32 {
            saw_place(&world, &rules, &mut brains, teller, (2 + k, 2), kind, 5);
        }
        night(&mut world, &rules, &mut brains, &[teller, grown, child]);
        assert_eq!(brains.mind(grown).unwrap().believes().count(), 8);
        assert_eq!(
            brains.mind(child).unwrap().believes().count(),
            3,
            "three at two years"
        );
    }

    #[test]
    fn a_place_heard_and_found_bare_makes_a_new_dated_record_the_present_replace_and_the_absent_keep()
     {
        // The 226/231 example of the proposal.
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(4, &rules);
        let (b14, b7, b9, b22) = (BodyId(1), BodyId(2), BodyId(3), BodyId(4));
        let kind = brains.food.numbers["bramble"];
        let key = (2, 2);
        saw_place(&world, &rules, &mut brains, b14, key, kind, 5);
        let news = night(&mut world, &rules, &mut brains, &[b14, b7, b9, b22]);
        let r226 = news
            .iter()
            .find_map(|n| match n {
                News::Told { record, .. } => Some(*record),
                _ => None,
            })
            .unwrap();
        for id in [b7, b9, b22] {
            assert_eq!(
                brains.mind(id).unwrap().believes().next().unwrap().1.record,
                r226
            );
        }
        // The next day, 7 goes and finds the place bare: a rumour dispelled.
        saw_place(&world, &rules, &mut brains, b7, key, kind, 0);
        let news = brains.news();
        assert!(
            matches!(news[..], [News::Dispelled { body, place, record, from, .. }] if body == b7 && place == key && record == r226 && from == b14),
            "{news:?}"
        );
        assert_eq!(
            brains.mind(b7).unwrap().believes().count(),
            0,
            "7 dropped 226"
        );
        assert_eq!(
            brains.memory.refs(r226),
            4,
            "14, 9, 22 and the night hold it"
        );
        // At that night's fire, 14 is absent: 7 tells the bare place, 9
        // replaces 226 by the later record, 22 (absent) keeps 226.
        let news = night(&mut world, &rules, &mut brains, &[b7, b9]);
        let r231 = news
            .iter()
            .find_map(|n| match n {
                News::Told { body, record, .. } if *body == b7 => Some(*record),
                _ => None,
            })
            .unwrap();
        assert_ne!(r231, r226, "immutable: a new record, dated day 2");
        assert_eq!(brains.memory.get(r231).map(Record::day), Some(2));
        assert_eq!(
            brains.mind(b9).unwrap().believes().next().unwrap().1.record,
            r231
        );
        assert_eq!(
            brains
                .mind(b22)
                .unwrap()
                .believes()
                .next()
                .unwrap()
                .1
                .record,
            r226
        );
        assert_eq!(
            brains
                .mind(b14)
                .unwrap()
                .believes()
                .next()
                .unwrap()
                .1
                .record,
            r226
        );
        assert_eq!(
            brains.mind(b9).unwrap().places[&key].kinds[0].holding,
            0,
            "9 believes it bare now"
        );
        // The older record told again at a later fire does not replace
        // the later one.
        let news = night(&mut world, &rules, &mut brains, &[b14, b9]);
        assert!(
            !news
                .iter()
                .any(|n| matches!(n, News::Heard { body, .. } if *body == b9)),
            "9 keeps the later record: {news:?}"
        );
    }

    #[test]
    fn forgetting_frees_the_record_at_its_last_access_and_the_dead_release_theirs() {
        let mut rules = fire_rules();
        rules.needs.fire.hearth_nights = 1;
        let (mut world, mut brains) = camp_of(2, &rules);
        let (teller, listener) = (BodyId(1), BodyId(2));
        let kind = brains.food.numbers["bramble"];
        saw_place(&world, &rules, &mut brains, teller, (2, 2), kind, 5);
        let news = night(&mut world, &rules, &mut brains, &[teller, listener]);
        let record = news
            .iter()
            .find_map(|n| match n {
                News::Told { record, .. } => Some(*record),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            brains.memory.refs(record),
            3,
            "the teller, the listener, the night"
        );
        // A night more: the hearth keeps one, the older night is freed.
        saw_place(&world, &rules, &mut brains, teller, (3, 3), kind, 5);
        night(&mut world, &rules, &mut brains, &[teller, listener]);
        assert_eq!(brains.memory.refs(record), 2);
        // The listener forgets the place: one access fewer.
        brains
            .minds
            .get_mut(&listener)
            .unwrap()
            .forget_place((2, 2), &mut brains.memory);
        assert_eq!(brains.memory.refs(record), 1);
        // The teller dies: its accesses are released, the record is freed.
        world.bodies[0].condition.vitality = Gauge::at(0);
        brains.think(&world, &rules, &[]);
        assert_eq!(brains.memory.get(record), None, "freed at its last access");
    }

    #[test]
    fn imitate_copies_a_way_heard_said_and_nothing_unheard() {
        let rules = fire_rules();
        let (world, mut brains) = camp_of(2, &rules);
        let (me, mate) = (BodyId(1), BodyId(2));
        // The mate fares best and sleeps out, but said nothing: no copy.
        let m = brains.minds.get_mut(&mate).unwrap();
        m.days = vec![Day {
            gathered_kj: 9000,
            ..Day::default()
        }];
        m.ways.sleep_out = true;
        m.will = 100;
        brains.imitate(&world, &rules, me);
        assert!(brains.news().is_empty(), "nobody reads another's mind");
        assert!(!brains.mind(me).unwrap().ways().sleep_out);
        // Heard it said at the fire that it kept its night out: copied.
        let theirs = *brains.mind(mate).unwrap().ways();
        heard_kept(&mut brains, (me, mate, 1), Way::SleepOut, theirs, 100);
        brains.imitate(&world, &rules, me);
        let news = brains.news();
        assert!(
            matches!(news[..], [News::Copied { body, from, way: Way::SleepOut, .. }] if body == me && from == mate),
            "{news:?}"
        );
        assert!(brains.mind(me).unwrap().ways().sleep_out);
        // Copying the home moves the camp; a teller of will 0 is never
        // copied; under a trial it copies nothing.
        let mut home = theirs;
        home.home = Pos::new(9, 9);
        heard_kept(&mut brains, (me, mate, 2), Way::Home, home, 100);
        brains.imitate(&world, &rules, me);
        assert_eq!(
            brains.mind(me).unwrap().home(),
            Pos::new(9, 9),
            "the camp moved"
        );
        brains.news();
        let mut wide = theirs;
        wide.worth_half_tiles = WORTH_HALF_TILES * 2;
        heard_kept(&mut brains, (me, mate, 3), Way::WorthHalfTiles, wide, 0);
        brains.imitate(&world, &rules, me);
        assert_eq!(
            brains.mind(me).unwrap().ways().worth_half_tiles,
            WORTH_HALF_TILES
        );
        // A lost step lands halfway.
        let rules = {
            let mut r = fire_rules();
            r.needs.trials.imitate_loss_percent = 100;
            r
        };
        heard_kept(&mut brains, (me, mate, 4), Way::WorthHalfTiles, wide, 100);
        brains.imitate(&world, &rules, me);
        let news = brains.news();
        assert_eq!(
            brains.mind(me).unwrap().ways().worth_half_tiles,
            WORTH_HALF_TILES * 3 / 2,
            "{news:?}"
        );
        let mind = brains.minds.get_mut(&me).unwrap();
        mind.ways.worth_half_tiles = WORTH_HALF_TILES;
        mind.trial = Some(Trial {
            way: Way::StockKj,
            before: mind.ways,
            started_day: 0,
            before_days: Lately::default(),
            heard: HeardDays::default(),
        });
        brains.imitate(&world, &rules, me);
        assert!(brains.news().is_empty(), "under a trial, it copies nothing");
    }

    /// The brain's daily draws (D50, amended 24 November 2026) come from
    /// the run's seed, not the map's: on the same map, another run seed
    /// tries other ways on other days.
    #[test]
    fn the_way_tried_is_drawn_from_the_run_seed() {
        let rules = fire_rules();
        let (world, brains) = camp_of(2, &rules);
        let me = BodyId(1);
        let tried = |seed: u64| -> Vec<Option<Way>> {
            (1..=40)
                .map(|day| {
                    let mut mind = Mind::new(Pos::new(5, 5));
                    start_trial(
                        &world,
                        &rules,
                        (&brains.memory, seed),
                        me,
                        &mut mind,
                        true,
                        day,
                    )
                    .and(mind.trial.map(|t| t.way))
                })
                .collect()
        };
        assert_eq!(tried(3), tried(3), "one seed, the same ways");
        assert_ne!(
            tried(1),
            tried(9),
            "another run seed, other ways, the map the same"
        );
    }

    #[test]
    fn in_distress_a_kept_way_heard_is_the_first_trial_and_a_child_does_not_take_a_home() {
        let rules = fire_rules();
        let (world, mut brains) = camp_of(2, &rules);
        let (me, mate) = (BodyId(1), BodyId(2));
        let mut theirs = *brains.mind(mate).unwrap().ways();
        theirs.stock_kj = STOCK_KJ * 2;
        heard_kept(&mut brains, (me, mate, 1), Way::StockKj, theirs, 50);
        let mind = brains.minds.get_mut(&me).unwrap();
        let news = start_trial(
            &world,
            &rules,
            (&brains.memory, brains.seed),
            me,
            mind,
            false,
            3,
        );
        assert!(
            matches!(news, Some(News::Trying { way: Way::StockKj, after, .. }) if after.stock_kj == STOCK_KJ * 2),
            "{news:?}"
        );
        assert_eq!(mind.trial.unwrap().way, Way::StockKj);
        // A child hears of a moved home and does not try it: a way of its own.
        let mut mind = Mind::new(Pos::new(5, 5));
        let mut home = theirs;
        home.home = Pos::new(9, 9);
        let (record, _) = brains.memory.intern(Record::Trial {
            body: mate,
            day: 1,
            way: Way::Home,
            after: home,
            up: true,
            kept: true,
            will: 50,
            before: Lately::default(),
            during: Lately::default(),
        });
        mind.heard_ways.push(Access {
            record,
            by: mate,
            day: 1,
        });
        let news = start_trial(
            &world,
            &rules,
            (&brains.memory, brains.seed),
            me,
            &mut mind,
            true,
            3,
        );
        assert!(
            matches!(news, Some(News::Trying { way, .. }) if way != Way::Home),
            "{news:?}"
        );
    }

    #[test]
    fn asleep_out_or_away_a_body_hears_nothing_and_the_same_seed_gives_the_same_nights() {
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(3, &rules);
        let (teller, out, away) = (BodyId(1), BodyId(2), BodyId(3));
        let kind = brains.food.numbers["bramble"];
        saw_place(&world, &rules, &mut brains, teller, (2, 2), kind, 5);
        // `out` sleeps out: it never lay down at the camp. `away` lies
        // down past `imitate_tiles` of the camp: not in the circle.
        brains.minds.get_mut(&away).unwrap().home_at_dusk = Pos::new(40, 40);
        let news = night(&mut world, &rules, &mut brains, &[teller, away]);
        assert!(news.is_empty(), "a teller alone tells nobody: {news:?}");
        assert_eq!(brains.mind(out).unwrap().believes().count(), 0);
        assert_eq!(brains.mind(away).unwrap().believes().count(), 0);
        // Same seed, same nights.
        let twice = || {
            let mut world = field(200, 150, Pos::new(5, 5));
            for k in 1..6 {
                world.spawn_body(Pos::new(5 + k, 5));
            }
            let mut brains = piloted(&world);
            let mut news = Vec::new();
            let mut events = Vec::new();
            for _ in 0..4 * world.calendar().ticks_per_day() {
                events.clear();
                world.step(&rules, &mut events);
                for (_, decision) in brains.think(&world, &rules, &events) {
                    world.queue(decision.intention);
                }
                news.extend(brains.news());
            }
            let records: Vec<(RecordId, Record, u32)> = brains
                .memory
                .records()
                .map(|(id, r, refs)| (id, r.clone(), refs))
                .collect();
            (news, records, brains.hearths().clone())
        };
        let (news, records, hearths) = twice();
        assert!(!hearths.is_empty(), "nights were held: {news:?}");
        assert_ne!(records.len(), 0);
        assert_eq!((news, records, hearths), twice());
    }

    /// A trial of `me` judged at nightfall with `heard` days of unchanged
    /// mates summed, its own last day gathering `kj`: kept?
    fn judged_against(heard: HeardDays, before: Lately, kj: u64) -> bool {
        let rules = fire_rules();
        let mut world = World::empty(
            Calendar::new(1, 7200).unwrap(),
            Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
        );
        let mut events = Vec::new();
        world.advance(2 * world.calendar().ticks_per_day(), &rules, &mut events);
        world.spawn_body(Pos::new(5, 5));
        let mut brains = piloted_with(&world, &rules);
        let me = BodyId(1);
        let mind = brains.minds.get_mut(&me).unwrap();
        let ways = mind.ways;
        mind.ways.worth_half_tiles *= 2;
        mind.trial = Some(Trial {
            way: Way::WorthHalfTiles,
            before: ways,
            started_day: 0,
            before_days: before,
            heard,
        });
        mind.days = vec![Day {
            gathered_kj: kj,
            ..Day::default()
        }];
        mind.gathered_today_kj = kj;
        let news = nightfall(&world, &rules, &mut brains);
        match news[..] {
            [
                News::Tried {
                    kept, heard_days, ..
                },
            ] => {
                assert_eq!(heard_days, heard.days);
                kept
            }
            _ => panic!("{news:?}"),
        }
    }

    #[test]
    fn a_trial_is_judged_against_the_heard_days_of_unchanged_mates_and_against_itself_when_it_heard_nobody()
     {
        let rules = fire_rules();
        let (world, _) = camp_of(1, &rules);
        let need = world.bodies[0].condition().need_j_per_day(&rules.needs) / 1000;
        let before = Lately {
            kj_per_day: 100,
            ..Lately::default()
        };
        // Gathering 2,000 kJ a day where the week before gave 100: better
        // than itself. But the mates who changed nothing gathered three
        // needs a day: no better than them, given up.
        let luck = HeardDays {
            kj_percent: 300 * 4,
            days: 4,
            nights: 2,
            ..HeardDays::default()
        };
        assert!(
            judged_against(HeardDays::default(), before, 2000),
            "against itself: kept"
        );
        assert!(
            !judged_against(luck, before, 2000),
            "a week of shared luck keeps nothing"
        );
        // The mates gathered half a need a day and were thin: better than them, kept.
        let lean = HeardDays {
            kj_percent: 50 * 4,
            thin: 4,
            days: 4,
            nights: 2,
            ..HeardDays::default()
        };
        assert!(
            judged_against(lean, before, need),
            "better than the mates: kept"
        );
        // Equal to the mates on every count: not better, given up.
        let same = HeardDays {
            kj_percent: 100 * 4,
            days: 4,
            nights: 2,
            ..HeardDays::default()
        };
        assert!(!judged_against(same, before, need));
    }

    #[test]
    fn at_the_fire_a_body_in_trial_sums_the_days_of_the_mates_who_changed_nothing() {
        let rules = fire_rules();
        let (mut world, mut brains) = camp_of(3, &rules);
        let (me, steady, changed) = (BodyId(1), BodyId(2), BodyId(3));
        let day = |kj| Day {
            gathered_kj: kj,
            thin: true,
            ..Day::default()
        };
        let mind = brains.minds.get_mut(&me).unwrap();
        mind.days = vec![day(100)];
        mind.trial = Some(Trial {
            way: Way::StockKj,
            before: mind.ways,
            started_day: 1,
            before_days: Lately::default(),
            heard: HeardDays::default(),
        });
        brains.minds.get_mut(&steady).unwrap().days = vec![day(2000)];
        let m = brains.minds.get_mut(&changed).unwrap();
        m.days = vec![day(9000)];
        m.changed_day = Some(1);
        night(&mut world, &rules, &mut brains, &[me, steady, changed]);
        let heard = brains.mind(me).unwrap().trial().unwrap().heard;
        assert_eq!(
            (heard.days, heard.nights, heard.thin),
            (1, 1, 1),
            "the steady mate's day only"
        );
        let need = world.bodies[1].condition().need_j_per_day(&rules.needs) / 1000;
        assert_eq!(heard.kj_percent, 2000 * 100 / need);
    }

    #[test]
    fn a_body_tells_at_most_told_per_night_things_its_judged_way_first() {
        let mut rules = fire_rules();
        rules.needs.fire.told_per_night = 3;
        let (mut world, mut brains) = camp_of(2, &rules);
        let (teller, listener) = (BodyId(1), BodyId(2));
        let kind = brains.food.numbers["bramble"];
        for k in 0..2_i32 {
            saw_place(&world, &rules, &mut brains, teller, (2 + k, 2), kind, 5);
        }
        let mind = brains.minds.get_mut(&teller).unwrap();
        mind.judged = Some(Judged {
            way: Way::StockKj,
            after: mind.ways,
            up: true,
            kept: true,
            before: Lately::default(),
            during: Lately::default(),
        });
        let news = night(&mut world, &rules, &mut brains, &[teller, listener]);
        let told: Vec<RecordId> = news
            .iter()
            .filter_map(|n| match n {
                News::Told { record, .. } => Some(*record),
                _ => None,
            })
            .collect();
        assert_eq!(told.len(), 3, "{news:?}");
        assert!(
            matches!(brains.memory.get(told[0]), Some(Record::Trial { .. })),
            "its way first"
        );
        assert_eq!(brains.mind(listener).unwrap().heard_ways().len(), 1);
        assert_eq!(brains.mind(listener).unwrap().believes().count(), 2);
        // Told once: the next night, nothing changed, nothing is told.
        let news = night(&mut world, &rules, &mut brains, &[teller, listener]);
        assert!(
            !news.iter().any(|n| matches!(n, News::Told { .. })),
            "{news:?}"
        );
        // Four places more than it may tell a night: the rest wait for
        // the next night.
        for k in 0..4_i32 {
            saw_place(&world, &rules, &mut brains, teller, (2 + k, 3), kind, 5);
        }
        let told = |news: &[News]| {
            news.iter()
                .filter(|n| matches!(n, News::Told { .. }))
                .count()
        };
        assert_eq!(
            told(&night(&mut world, &rules, &mut brains, &[teller, listener])),
            3
        );
        assert_eq!(
            told(&night(&mut world, &rules, &mut brains, &[teller, listener])),
            1
        );
    }

    // The will and the lessons (D50 amended, 23 November 2026).

    #[test]
    fn the_innate_will_is_drawn_skewed_most_timid_an_adventurer_or_two_in_twenty_five() {
        let rules = fire_rules();
        let trials = &rules.needs.trials;
        assert_eq!(trials.will_at(0), 5);
        assert_eq!(trials.will_at(800), 20);
        assert_eq!(trials.will_at(930), 40);
        assert_eq!(trials.will_at(1000), 90);
        let wills: Vec<u8> = (0..1000).map(|p| trials.will_at(p)).collect();
        let timid = wills.iter().filter(|&&w| w <= 20).count();
        let bold = wills.iter().filter(|&&w| w >= 60).count();
        assert!(timid >= 800, "{timid}");
        assert!(
            (20..=70).contains(&bold),
            "{bold} of 1,000: one or two in twenty-five"
        );
        // Over a tribe, drawn from the seed and the ids: the same shape,
        // and the same every time.
        let mut many = field(64, 40, Pos::new(5, 5));
        for k in 0..99 {
            many.spawn_body(Pos::new(6 + k % 10, 6 + k / 10));
        }
        let wills: Vec<u8> = piloted_with(&many, &rules)
            .minds
            .values()
            .map(Mind::will)
            .collect();
        assert_eq!(
            wills,
            piloted_with(&many, &rules)
                .minds
                .values()
                .map(Mind::will)
                .collect::<Vec<_>>()
        );
        let timid = wills.iter().filter(|&&w| w <= 20).count();
        let bold = wills.iter().filter(|&&w| w >= 60).count();
        assert!(
            timid >= 70 && (1..=12).contains(&bold),
            "timid {timid}, bold {bold}: {wills:?}"
        );
        // Flat without a table, as before.
        assert_eq!(Trials::default().will_at(500), 50);
    }

    #[test]
    fn a_will_of_zero_never_tries() {
        let mut rules = fire_rules();
        rules.needs.trials.try_percent_by_will = crate::crops::tests::content()
            .needs
            .trials
            .try_percent_by_will;
        assert_eq!(rules.needs.trials.try_percent(0), 0);
        assert!(rules.needs.trials.try_percent(100) > 0);
        let mut world = World::empty(
            Calendar::new(1, 7200).unwrap(),
            Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
        );
        let mut events = Vec::new();
        world.advance(2 * world.calendar().ticks_per_day(), &rules, &mut events);
        world.spawn_body(Pos::new(5, 5));
        let mut brains = piloted_with(&world, &rules);
        let mind = brains.minds.get_mut(&BodyId(1)).unwrap();
        mind.will = 0;
        mind.distress_days = 100;
        for _ in 0..30 {
            let news = nightfall(&world, &rules, &mut brains);
            assert!(news.is_empty(), "{news:?}");
            brains.minds.get_mut(&BodyId(1)).unwrap().distress_days = 100;
        }
    }

    #[test]
    fn a_kept_trial_raises_the_will_and_an_abandoned_one_lowers_it_within_bounds() {
        // The fixed steps: the rule without the content's temperance.
        let mut rules = fire_rules();
        rules.needs.temperance = Temperance::default();
        let judged = |will: u8, kept: bool| {
            let mut world = World::empty(
                Calendar::new(1, 7200).unwrap(),
                Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
            );
            let mut events = Vec::new();
            world.advance(2 * world.calendar().ticks_per_day(), &rules, &mut events);
            world.spawn_body(Pos::new(5, 5));
            let mut brains = piloted_with(&world, &rules);
            let mind = brains.minds.get_mut(&BodyId(1)).unwrap();
            mind.will = will;
            let ways = mind.ways;
            mind.ways.worth_half_tiles *= 2;
            mind.trial = Some(Trial {
                way: Way::WorthHalfTiles,
                before: ways,
                started_day: 0,
                before_days: Lately {
                    kj_per_day: if kept { 100 } else { 5000 },
                    ..Lately::default()
                },
                heard: HeardDays::default(),
            });
            let kj = 1000;
            mind.days = vec![Day {
                gathered_kj: kj,
                ..Day::default()
            }];
            mind.gathered_today_kj = kj;
            let news = nightfall(&world, &rules, &mut brains);
            assert!(
                matches!(news[..], [News::Tried { kept: k, .. }] if k == kept),
                "{news:?}"
            );
            brains.mind(BodyId(1)).unwrap().will()
        };
        assert_eq!(judged(50, true), 55);
        assert_eq!(judged(50, false), 45);
        assert_eq!(judged(98, true), 100, "bounded above");
        assert_eq!(judged(3, false), 0, "bounded below");
    }

    /// `me` holds a heard trial record of `way` `up`, `kept`, dated `day`.
    fn heard_lesson(brains: &mut Brains, me: BodyId, (way, up, kept, day): (Way, bool, bool, u32)) {
        let (record, _) = brains.memory.intern(Record::Trial {
            body: BodyId(2),
            day,
            way,
            after: Ways::at(Pos::new(5, 5)),
            up,
            kept,
            will: 50,
            before: Lately::default(),
            during: Lately::default(),
        });
        brains.minds.get_mut(&me).unwrap().heard_ways.push(Access {
            record,
            by: BodyId(2),
            day,
        });
    }

    fn weight_of(candidates: &[(Way, bool, u64)], way: Way, up: bool) -> u64 {
        candidates
            .iter()
            .find(|c| c.0 == way && c.1 == up)
            .unwrap()
            .2
    }

    #[test]
    fn a_heard_failure_lowers_the_candidate_a_success_raises_it_by_the_listeners_will_and_it_fades()
    {
        let rules = fire_rules();
        let (_, mut brains) = camp_of(1, &rules);
        let me = BodyId(1);
        let weights = |brains: &Brains, will: u8, day: u64| {
            let mut mind = brains.mind(me).unwrap().clone();
            mind.will = will;
            lessons(&rules, &brains.memory, &mind, day, false)
        };
        let before = weights(&brains, 0, 10);
        assert!(before.iter().all(|c| c.2 == 100), "{before:?}");
        assert_eq!(before.len(), 6);
        assert_eq!(
            lessons(&rules, &brains.memory, brains.mind(me).unwrap(), 10, true).len(),
            5,
            "a child: no home"
        );
        // A failure heard of storing more, a success of sleeping out.
        heard_lesson(&mut brains, me, (Way::StockKj, true, false, 10));
        heard_lesson(&mut brains, me, (Way::SleepOut, true, true, 10));
        let timid = weights(&brains, 0, 10);
        assert_eq!(
            weight_of(&timid, Way::StockKj, true),
            50,
            "a failure: half the weight"
        );
        assert_eq!(
            weight_of(&timid, Way::StockKj, false),
            100,
            "not the other direction"
        );
        assert_eq!(
            weight_of(&timid, Way::SleepOut, true),
            150,
            "a success: half more"
        );
        let bold = weights(&brains, 100, 10);
        assert_eq!(
            weight_of(&bold, Way::StockKj, true),
            85,
            "an adventurer heeds 30 %"
        );
        assert_eq!(weight_of(&bold, Way::SleepOut, true), 115);
        // Two failures more: never under the floor.
        heard_lesson(&mut brains, me, (Way::StockKj, true, false, 11));
        heard_lesson(&mut brains, me, (Way::StockKj, true, false, 12));
        assert_eq!(
            weight_of(&weights(&brains, 0, 12), Way::StockKj, true),
            10,
            "a weight, never a ban"
        );
        // A lesson fades: half after lesson_days, nothing after twice.
        assert_eq!(
            weight_of(&weights(&brains, 0, 10 + 50), Way::SleepOut, true),
            125
        );
        assert_eq!(
            weight_of(&weights(&brains, 0, 10 + 100), Way::SleepOut, true),
            100
        );
    }

    #[test]
    fn an_adventurer_tries_what_the_timid_gave_up() {
        let rules = fire_rules();
        let (world, mut brains) = camp_of(1, &rules);
        let me = BodyId(1);
        // Every change but one heard given up, twice each (a teller judges
        // one trial of a way a day: each on its own day).
        for (i, (way, up)) in [
            (Way::WorthHalfTiles, true),
            (Way::WorthHalfTiles, false),
            (Way::SleepOut, true),
            (Way::StockKj, true),
            (Way::Home, true),
        ]
        .into_iter()
        .enumerate()
        {
            let day = u32::try_from(i).unwrap();
            heard_lesson(&mut brains, me, (way, up, false, 1 + 2 * day));
            heard_lesson(&mut brains, me, (way, up, false, 2 + 2 * day));
        }
        let tries = |brains: &Brains, will: u8| {
            let mut given_up = 0;
            for day in 10..50 {
                let mut mind = brains.mind(me).unwrap().clone();
                mind.will = will;
                if let Some(News::Trying {
                    way, before, after, ..
                }) = start_trial(
                    &world,
                    &rules,
                    (&brains.memory, brains.seed),
                    me,
                    &mut mind,
                    true,
                    day,
                ) && (way != Way::StockKj || step_up(way, &before, &after))
                {
                    given_up += 1;
                }
            }
            given_up
        };
        let (timid, bold) = (tries(&brains, 0), tries(&brains, 100));
        assert!(
            timid < bold,
            "the timid tried the given-up {timid} times in 40 days, the adventurer {bold}"
        );
        assert!(timid <= 12, "{timid}");
    }

    /// `nights` nights noted at `place`, each with these marks.
    fn lived_at(mind: &mut Mind, place: PlaceKey, nights: u32, night: &Night, keep: u64) {
        for _ in 0..nights {
            mind.note_night(place, night, keep);
        }
    }

    /// A night with these losses, this walk and the day's acts.
    fn night_with(losses: [u64; SENSES], walk: u64, day: Vec<DayAct>) -> Night {
        Night { losses, walk, day }
    }

    /// Drunk at a place (16 tiles); `saved` the walk there and back.
    fn drank(place: PlaceKey, saved: u64, drunk: u64) -> DayAct {
        DayAct {
            place,
            grove: false,
            saved,
            gains: [0, drunk, 0, 0],
        }
    }

    /// Gathered in a grove (64 tiles); `saved` the walk there and back.
    fn picked(grove: PlaceKey, saved: u64, gathered: u64) -> DayAct {
        DayAct {
            place: grove,
            grove: true,
            saved,
            gains: [gathered, 0, 0, 0],
        }
    }

    /// A night cold through, nothing else lacking.
    const COLD: [u64; SENSES] = [0, 0, 1000, 0];

    /// Where a home trial would go: its place on the 16 and the 64 grids.
    fn goes(world: &World, rules: &Content, mind: &Mind) -> Option<(PlaceKey, PlaceKey)> {
        lived_home(world, rules, mind).map(|to| {
            (
                place_of(to, place_side(rules)),
                place_of(to, grove_side(rules)),
            )
        })
    }

    /// The fare of the place `place` (a grove or not) in `mind`'s notes.
    fn fare_of(rules: &Content, mind: &Mind, place: PlaceKey, grove: bool) -> i64 {
        let sides = (place_side(rules), grove_side(rules));
        let home = mind.lived.iter().find(|l| !l.grove && l.place == (0, 0));
        let l = mind
            .lived
            .iter()
            .find(|l| l.place == place && l.grove == grove)
            .unwrap();
        fare(l, &mind.lived, sides, home)
    }

    #[test]
    fn water_food_or_both_win_by_how_the_body_fared_there() {
        let mut rules = rules();
        rules.needs.trials.home_nights_by_will = vec![(0, 1)];
        let world = field(256, 250, Pos::new(5, 5));
        let home = Pos::new(5, 5);
        let at_home = |day: Vec<DayAct>| {
            let mut mind = Mind::new(home);
            lived_at(&mut mind, (0, 0), 2, &night_with(COLD, 0, day), 1000);
            mind
        };
        // Water: a little food in its home's grove, a day's water at
        // (8, 8): at home 600 - 1000 (the cold), there 1000 - 1000.
        let mind = at_home(vec![picked((0, 0), 20, 600), drank((8, 8), 50, 1000)]);
        assert_eq!(fare_of(&rules, &mind, (0, 0), false), -400);
        assert_eq!(fare_of(&rules, &mind, (8, 8), false), 0);
        assert_eq!(goes(&world, &rules, &mind).map(|g| g.0), Some((8, 8)));
        // Food: half its water at home, a day's food in the grove (2, 2).
        let mind = at_home(vec![drank((0, 0), 0, 500), picked((2, 2), 50, 1000)]);
        assert_eq!(goes(&world, &rules, &mind).map(|g| g.1), Some((2, 2)));
        // Home gives as much: no move; more than a day's worth counts as
        // a day's.
        let mind = at_home(vec![
            drank((0, 0), 0, 1000),
            picked((0, 0), 0, 500),
            picked((2, 2), 50, 3000),
        ]);
        assert_eq!(fare_of(&rules, &mind, (2, 2), true), 0);
        assert_eq!(goes(&world, &rules, &mind), None);
        // Both: half its water at home, a grove far away feeding it with
        // water in it: there it would get both.
        let mind = at_home(vec![
            drank((0, 0), 0, 500),
            drank((9, 9), 60, 500),
            picked((2, 2), 50, 1000),
        ]);
        assert_eq!(goes(&world, &rules, &mind), Some(((9, 9), (2, 2))));
        // Never lived anywhere: no trial.
        assert_eq!(goes(&world, &rules, &Mind::new(home)), None);
    }

    #[test]
    fn the_nights_and_the_walk_count_in_the_same_fare_and_the_timid_waits() {
        let mut rules = rules();
        rules.needs.trials.home_nights_by_will = vec![(0, 3), (70, 1)];
        let world = field(256, 250, Pos::new(5, 5));
        let fed = vec![drank((0, 0), 0, 1000), picked((0, 0), 0, 1000)];
        // Hungry half a day at home every night, not at (1, 0) where it
        // slept once, fed and watered alike: (1, 0) fares better.
        let mut mind = Mind::new(Pos::new(5, 5));
        lived_at(
            &mut mind,
            (0, 0),
            3,
            &night_with([500, 0, 1000, 0], 0, fed.clone()),
            1000,
        );
        lived_at(
            &mut mind,
            (1, 0),
            1,
            &night_with(COLD, 0, fed.clone()),
            1000,
        );
        mind.will = 0;
        assert_eq!(goes(&world, &rules, &mind), None, "one night: not enough");
        mind.will = 90;
        assert_eq!(goes(&world, &rules, &mind).map(|g| g.0), Some((1, 0)));
        lived_at(
            &mut mind,
            (1, 0),
            2,
            &night_with(COLD, 0, fed.clone()),
            1000,
        );
        mind.will = 0;
        assert_eq!(
            goes(&world, &rules, &mind).map(|g| g.0),
            Some((1, 0)),
            "three nights"
        );
        // The water far away: what walking there costs is what living
        // there saves. Free, the cold alike, home and water tie: no move.
        let mut mind = Mind::new(Pos::new(5, 5));
        let day = vec![picked((0, 0), 0, 1000), drank((8, 8), 0, 1000)];
        lived_at(&mut mind, (0, 0), 3, &night_with(COLD, 0, day), 1000);
        assert_eq!(goes(&world, &rules, &mind), None);
        // A third of a day's food walked there and back: the water wins.
        let mut mind = Mind::new(Pos::new(5, 5));
        let day = vec![picked((0, 0), 0, 1000), drank((8, 8), 300, 1000)];
        lived_at(&mut mind, (0, 0), 3, &night_with(COLD, 300, day), 1000);
        assert_eq!(fare_of(&rules, &mind, (0, 0), false), -300);
        assert_eq!(fare_of(&rules, &mind, (8, 8), false), 0);
        assert_eq!(goes(&world, &rules, &mind).map(|g| g.0), Some((8, 8)));
    }

    #[test]
    fn old_days_fade_a_grove_and_a_place_are_apart_and_the_notes_keep_eight() {
        let mut mind = Mind::new(Pos::new(5, 5));
        // A grove and the place of the same key are two notes.
        let day = vec![drank((0, 0), 0, 1000), picked((0, 0), 0, 400)];
        lived_at(&mut mind, (0, 0), 1, &night_with(COLD, 0, day), 950);
        assert_eq!(mind.lived_places(), 2);
        let grove = *mind.lived.iter().find(|l| l.grove).unwrap();
        assert_eq!((grove.days, grove.gains), (1000, [400_000, 0, 0, 0]));
        // A place turned bad: the last nights weigh more.
        let mut turned = Mind::new(Pos::new(5, 5));
        lived_at(
            &mut turned,
            (0, 0),
            30,
            &night_with([0, 0, 0, 0], 0, vec![]),
            950,
        );
        lived_at(&mut turned, (0, 0), 30, &night_with(COLD, 0, vec![]), 950);
        let l = turned.lived[0];
        assert!(
            l.per_night(l.losses[Sense::Warmth as usize]) >= 700,
            "{l:?}"
        );
        // Not gathered there any more: its days fade away and it goes.
        lived_at(&mut mind, (0, 0), 200, &night_with(COLD, 0, vec![]), 950);
        assert!(!mind.lived.iter().any(|l| l.grove));
        // Nine groves: the least lived gives up its room.
        let mut many = Mind::new(Pos::new(5, 5));
        for k in 0..9 {
            let day = vec![picked((k, 1), 0, 100)];
            lived_at(
                &mut many,
                (0, 0),
                u32::try_from(9 - k).unwrap(),
                &night_with(COLD, 0, day),
                1000,
            );
        }
        assert_eq!(many.lived_places(), LIVED_MAX);
        assert!(
            many.lived.iter().any(|l| l.place == (8, 1)),
            "the newest is noted"
        );
        assert!(!many.lived.iter().any(|l| l.grove && l.place == (7, 1)));
    }

    #[test]
    fn a_drink_counts_the_thirst_since_the_last_and_the_walk_there_and_back_is_what_it_would_save()
    {
        let mut rules = rules();
        rules.needs.walk_j_per_m = 130;
        let (world, mut brains) = camp_of(1, &rules);
        let me = BodyId(1);
        let per_day = world.calendar().ticks_per_day();
        let now = world.tick().0;
        let side = place_side(&rules);
        let body = &world.bodies[0];
        // Out of the home's place the steps count; back in it, nought.
        let step = |to: Pos| Event::BodyMoved {
            body: me,
            to,
            at: FinePos::new(0, 0),
            remaining: 0,
        };
        let out: Vec<Event> = (0..25).map(|k| step(Pos::new(20 + k, 5))).collect();
        brains.count_gathers(&out, &world, &rules);
        assert_eq!(brains.mind(me).unwrap().from_home, 25);
        // Its last drink half a day ago: half a day's thirst, where it
        // stands; the 25 tiles there and back what living there saves.
        brains.minds.get_mut(&me).unwrap().drank_at = now - per_day / 2;
        let drink = Event::BodyDrank {
            body: me,
            at: Pos::new(6, 5),
        };
        brains.count_gathers(std::slice::from_ref(&drink), &world, &rules);
        let here = place_of(body.pos(), side);
        let saved = walk_permille(&rules, body, 50);
        // 50 m at 130 J of 10,000 kJ: 6.5 J a thousandth, 0 by default.
        assert_eq!(
            saved,
            50 * 130 * 1000 / body.condition().need_j_per_day(&rules.needs)
        );
        let mind = brains.minds.get_mut(&me).unwrap();
        assert_eq!(mind.day_places, vec![drank(here, saved, 500)]);
        // Again at once: nothing more. A day or more since: a day's.
        mind.acted_at(here, false, saved, [0; SENSES]);
        mind.drank_at = 0;
        brains.count_gathers(std::slice::from_ref(&drink), &world, &rules);
        let mind = brains.minds.get_mut(&me).unwrap();
        assert_eq!(mind.day_places[0].gains, [0, 1500, 0, 0]);
        // At most eight places a day; the night takes them, and the walk.
        for k in 0..20 {
            mind.acted_at((k, 3), true, 0, [1, 0, 0, 0]);
        }
        assert_eq!(mind.day_places.len(), LIVED_MAX);
        mind.walked_today = 1000;
        let night = night_of(&rules, body, mind);
        assert_eq!(night.day.len(), LIVED_MAX);
        assert_eq!(night.walk, walk_permille(&rules, body, 1000));
        assert_eq!((mind.day_places.len(), mind.walked_today), (0, 0));
    }

    #[test]
    fn with_nothing_wrong_only_the_bold_try_and_seldom() {
        let mut trials = rules().needs.trials;
        trials.try_percent_by_will = vec![(0, 0), (100, 30)];
        trials.trial_after_days = 2;
        trials.calm_try_permille_by_will = vec![(0, 0), (40, 0), (100, 20)];
        let tries = |trials: &Trials, will: u8, distress: u32| {
            let mut mind = Mind::new(Pos::new(5, 5));
            mind.will = will;
            mind.distress_days = distress;
            (0..5000)
                .filter(|&day| wants_to_try(trials, 9, (BodyId(3), day), &mind))
                .count()
        };
        // Calm: the timid never, the bold about one day in fifty.
        assert_eq!(tries(&trials, 20, 0), 0);
        let bold = tries(&trials, 100, 0);
        assert!((60..=140).contains(&bold), "{bold}");
        assert!(tries(&trials, 70, 0) < bold);
        // In distress: the distress' chance, far greater.
        assert!(tries(&trials, 100, 2) > 1000);
        // None when the content holds none.
        trials.calm_try_permille_by_will = vec![(0, 0)];
        assert_eq!(tries(&trials, 100, 0), 0);
        assert_eq!(trials.calm_try_permille(100), 0);
    }
}
