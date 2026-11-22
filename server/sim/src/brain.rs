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
//! 2. **Hungry** (hunger at [`HUNGRY`] or more): it eats what it carries
//!    (what spoils first, first); else, by day or while its reserve is
//!    weak, the nearest food it saw lying; else it gathers from the tree
//!    or plant it knows bearing food now (D45) worth most: what an hour
//!    of it gives of what is left on it, less the farther, and none
//!    holding less than [`GATHER_MIN_MINUTES`] of work; knowing none, it
//!    goes looking. A hungry body whose reserve is not weak sleeps at
//!    night like the others (the D47 trial of 21 November 2026: hungry
//!    all day, a body never slept and walked twenty-two hours a day).
//! 3. **Night**: home to the camp, and sleeps within [`HOME_TILES`] of it.
//! 4. **By day, a store**: while it carries less than [`STOCK_KJ`] and
//!    its load is not full, it gathers from the nearest tree or plant it
//!    knows bearing food now.
//!    It works at it until the plant is bare or its load full (or night,
//!    thirst or hunger calls it away).
//! 5. Otherwise, by day: home to the camp if it is away, and waits there.
//!
//! It remembers the trees and plants bearing food it saw (D45), as it
//! remembers water; one it found bare, or could not reach, it leaves
//! alone until that crop's next year; one whose gather the world refused
//! (nothing ripe on it yet, its load full), for [`RETRY_HOURS`]. Hungry,
//! it gathers only while its load is not full either.
//!
//! Every choice comes with its reason ([`Why`]), so the vivarium's report
//! can say why a body did what it did. Everything is deterministic: the
//! same world and the same seed give the same thoughts.
//!
//! **Trials, selection and imitation (D50).** The constants that shape a
//! day are **ways** on each mind ([`Ways`]): how far a plant is worth
//! going, whether it sleeps out, how much it stores, where home is. At
//! nightfall each mind closes its day: what it gathered, whether it is
//! thin, whether it is in **distress** (hungry at the day's end, or a day
//! with no gather at all). Distressed for `trial_after_days` running, it
//! may start a **trial** (a daily chance by its **will**, a placeholder
//! drawn from the world's seed and its id until the souls' Willpower
//! comes, D30): one way moved by one step, drawn from the seed, its id
//! and the day. After `trial_days` the world has **judged**: the way is
//! kept if it gathered more a day than before and was thin no more often,
//! else reverted. Lying down at its camp at night, a body may **copy** one
//! way from the camp-mate who gathered most a day lately, the more
//! readily the greater that mate's will, sometimes losing a step to the
//! copy. Children follow their mother's home every night, a carried
//! infant its carrier's. The brain tells these in its [`News`], drained
//! by the vivarium after each turn ([`Brains::news`]). The content's
//! section `trials` holds every number ([`crate::content::Trials`]);
//! absent, nobody tries anything.

use std::collections::BTreeMap;

use crate::content::{Content, Trials};
use crate::crops::{self, Bearer, Source};
use crate::r#gen::scatter::{self, purpose};
use crate::lru::Lru;
use crate::map::{CHUNK_SIZE, ChunkPos, FinePos, Map, Pos, Terrain};
use crate::{BodyId, Event, GATHER_REACH_MM, Intention, ItemId, World};

/// Thirst from which a body goes to drink: half a game day without
/// water (thirst rises 100 points a day), so it drinks before the night
/// and twice a day.
pub const THIRSTY: u32 = 50;
/// Thirst from which a sleeping body wakes to drink: it would not
/// otherwise last the night.
pub const PARCHED: u32 = 80;
/// Hunger from which a body goes to eat.
pub const HUNGRY: u32 = 50;
/// How near the camp a body counts as home, in tiles (a little more than
/// the ring the bodies are born in).
pub const HOME_TILES: i32 = 6;
/// How many drinking places and how many foods a body remembers: the
/// nearest to the camp are kept.
pub const MEMORY_PLACES: usize = 64;
/// How many trees and plants bearing food a body remembers: those worth
/// most from where it last looked are kept (what an hour gives at best,
/// less the farther, [`WORTH_HALF_TILES`]). It gathers where it roams, and
/// a tribe whose water is far from its camp roams far from it.
pub const MEMORY_PLANTS: usize = 256;
/// How far, in tiles, a plant's worth is halved: a body weighs what an
/// hour of gathering gives against the walk there.
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
/// How many chunks' trees and plants bearing food the brains hold, the
/// least lately used forgotten first (placed again if seen again): as
/// many as the world's own trees ([`crate::ground::TREE_CACHE_CHUNKS`]).
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
    ToGather(ChunkPos, Source),
    Gathering(ChunkPos, Source),
}

/// A tree or plant bearing food that a body saw (D45).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownPlant {
    /// Where it stands.
    pub at: FinePos,
    /// Its kind, by its number among the content's kinds bearing food
    /// (the brains number them, [`Brains::kind_name`]): a body remembers
    /// hundreds, and a name each cost an allocation a plant.
    pub kind: u16,
    /// The crop's year it was found bare in, or out of reach: left alone
    /// until the next.
    pub spent: Option<u64>,
    /// The tick before which it is left alone: a gather of it was refused
    /// (nothing ripe on it yet, the body's load full, ...). It tries
    /// again [`RETRY_HOURS`] later, or another plant meanwhile.
    pub not_before: Option<u64>,
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
    /// Its camp: where it goes at night and by day, and goes looking from.
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
    /// What the `trial_days` before gathered a day, in kilojoules.
    pub before_kj_per_day: u64,
    /// How many of those days ended thin.
    pub before_thin_days: u32,
}

/// One day as a mind closed it at nightfall (D50).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Day {
    /// Gathered over the day, in kilojoules.
    gathered_kj: u64,
    /// Thin at nightfall.
    thin: bool,
}

/// What the brain tells of its trials (D50), drained with
/// [`Brains::news`] after each turn: the vivarium's chronicle reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum News {
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
        /// Its ways after the copy.
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
    /// Gathered since the last nightfall, in kilojoules.
    gathered_today_kj: u64,
    /// Days of distress in a row: hungry at nightfall, or no gather at all.
    distress_days: u32,
    /// Its last days, the oldest first: at most twice `trial_days`.
    days: Vec<Day>,
    /// The trial under way, if any.
    trial: Option<Trial>,
    /// Its home at the last nightfall: the camp-mates are those sharing it.
    home_at_dusk: Pos,
    /// The day it last lay down at its camp and looked at its mates: it
    /// copies at most once a night.
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
    /// The trees and plants bearing food it saw.
    plants: BTreeMap<(ChunkPos, Source), KnownPlant>,
    doing: Doing,
    /// Which thresholds its gauges were past when it last thought:
    /// thirsty, parched, hungry, asleep.
    past: (bool, bool, bool, bool),
    /// Something changed: it thinks at the next turn.
    due: bool,
    /// Its walk ended since it last thought (it arrived, or was stopped).
    stopped: bool,
    /// The chunks whose plants it could not walk to, and the day it
    /// tried: left alone until the next day, when it may stand elsewhere.
    out_of_reach: BTreeMap<ChunkPos, u64>,
    /// The chunks in sight from `plants_from` whose plants were not
    /// placed yet ([`COLD_CHUNKS_PER_SECOND`]): learnt at the next turns.
    unseen: Vec<ChunkPos>,
    /// The tick it thinks at for the dawn or the nightfall
    /// ([`WAKE_SPREAD_SECONDS`]), unless it thought since.
    wake_at: Option<u64>,
    /// How many times it went looking: the next direction.
    walks: u64,
    /// How many times it thought.
    thoughts: u64,
}

impl Mind {
    fn new(home: Pos) -> Self {
        Self {
            ways: Ways::at(home),
            will: 0,
            gathered_today_kj: 0,
            distress_days: 0,
            days: Vec::new(),
            trial: None,
            home_at_dusk: home,
            copied_night: None,
            looked_from: None,
            set_off_from: None,
            plants_from: None,
            water: BTreeMap::new(),
            food: BTreeMap::new(),
            plants: BTreeMap::new(),
            doing: Doing::Nothing,
            past: (false, false, false, false),
            due: true,
            stopped: false,
            out_of_reach: BTreeMap::new(),
            unseen: Vec::new(),
            wake_at: None,
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

    /// The trial under way, if any (D50).
    pub fn trial(&self) -> Option<&Trial> {
        self.trial.as_ref()
    }

    /// Its days of distress in a row (D50).
    pub fn distress_days(&self) -> u32 {
        self.distress_days
    }

    /// What it gathered a day over its last `days` days closed, in
    /// kilojoules, and how many of them ended thin: over the days it has
    /// when it has fewer; (0, 0) with none.
    fn lately(&self, days: u32) -> (u64, u32) {
        let n = usize::try_from(days)
            .unwrap_or(usize::MAX)
            .min(self.days.len());
        if n == 0 {
            return (0, 0);
        }
        let last = &self.days[self.days.len() - n..];
        let kj: u64 = last.iter().map(|d| d.gathered_kj).sum();
        let thin = u32::try_from(last.iter().filter(|d| d.thin).count()).unwrap_or(u32::MAX);
        (kj / u64::try_from(n).expect("fits"), thin)
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
}

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
    fn name(&self, kind: u16) -> &str {
        &self.kinds[usize::from(kind)]
    }
}

/// A tree or plant bearing food as the brains hold it: its kind numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    source: Source,
    at: FinePos,
    kind: u16,
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
    chunks: Lru<ChunkPos, Vec<Held>>,
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
    fn get(&mut self, world: &World, food: &Food, chunk: ChunkPos) -> Option<&[Held]> {
        if !self.chunks.contains(&chunk) {
            if self.left == 0 {
                return None;
            }
            self.left -= 1;
            self.chunks
                .insert(chunk, food_bearers_in(world, food, chunk));
        }
        self.chunks.get(&chunk).map(Vec::as_slice)
    }

    /// Places `chunk`'s now, out of any turn's count.
    fn warm(&mut self, world: &World, food: &Food, chunk: ChunkPos) {
        if !self.chunks.contains(&chunk) {
            self.chunks
                .insert(chunk, food_bearers_in(world, food, chunk));
        }
    }
}

/// Is `material` food?
fn is_food(rules: &Content, material: &str) -> bool {
    rules.material(material).is_some_and(|m| m.food.is_some())
}

/// The trees and plants of `chunk` that bear food in some season.
fn food_bearers_in(world: &World, food: &Food, chunk: ChunkPos) -> Vec<Held> {
    world
        .bearers_in(chunk)
        .iter()
        .filter_map(|b: &Bearer| {
            Some(Held {
                source: b.source,
                at: b.at,
                kind: *food.numbers.get(&b.kind)?,
            })
        })
        .collect()
}

/// How far a body standing on `at` sees, in tiles.
fn sight_at(map: &Map, rules: &Content, at: Pos) -> i32 {
    i32::try_from(
        map.get(at)
            .map_or(1, |terrain| rules.movement.sight_tiles(terrain)),
    )
    .unwrap_or(1)
}

impl Brains {
    /// No minds yet; `seed` chooses where they go looking.
    pub fn new(seed: u64) -> Self {
        Self {
            minds: BTreeMap::new(),
            seed,
            camp_water: BTreeMap::new(),
            bearers: Placed::new(),
            shores: Lru::new(BEARER_CHUNKS),
            food: Food::default(),
            work_per_second: THOUGHT_WORK_PER_SECOND,
            owed: 0,
            next: None,
            waiting: 0,
            work: 0,
            news: Vec::new(),
            dusk: false,
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
        let mind = self.minds.entry(body).or_insert_with(|| {
            // Its first thought too in its own tick: a tribe born at once
            // does not think all in one.
            let mut mind = Mind::new(home);
            mind.will = u8::try_from(
                scatter::roll(world.seed(), purpose::TRIAL_WILL, &[u64::from(body.0)]) % 101,
            )
            .expect("under 101");
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

    /// The id of the kind numbered `kind` ([`KnownPlant::kind`]), if the
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
        self.count_gathers(events, rules);
        let now = world.tick().0;
        self.bearers.left = cold_chunks_at(world, now);
        self.learn_unseen(world, rules);
        let spread = wake_spread(world);
        // Who is gone, and who is due.
        self.minds.retain(|&id, mind| {
            let Some(body) = world.body(id) else {
                return false;
            };
            if body.condition().vitality.points() == 0 {
                return false;
            }
            let past = pasts(world, id);
            if past != mind.past {
                // A gauge past a threshold: in its own tick too, as the
                // dawn (a tribe born at once grows hungry at once).
                mind.past = past;
                mind.wake(id, now, spread);
            }
            if mind.wake_at.is_some_and(|tick| now >= tick) {
                mind.due = true;
            }
            true
        });
        if std::mem::take(&mut self.dusk) {
            self.close_day(world, rules);
        }
        // The due think, by their ids from the first one the last turn
        // left waiting, while the turn's share of work lasts.
        let cap = tick_share(world, now, self.work_per_second);
        let allowance = cap.saturating_sub(self.owed);
        self.owed = self.owed.saturating_sub(cap);
        let from = self.next.take().unwrap_or(BodyId(0));
        let due: Vec<BodyId> = self
            .minds
            .range(from..)
            .chain(self.minds.range(..from))
            .filter(|(_, mind)| mind.due)
            .map(|(&id, _)| id)
            .collect();
        let mut decisions = Vec::new();
        let seed = self.seed;
        let mut land = Land {
            bearers: &mut self.bearers,
            shores: &mut self.shores,
            food: &mut self.food,
        };
        let mut used = 0;
        self.waiting = 0;
        let mut lay_down = Vec::new();
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
            used += 1 + look(world, rules, body.pos(), mind, &mut land);
            if let Some(decision) = decide(world, rules, land.food, id, mind, seed) {
                if decision.why == Why::Night
                    && matches!(decision.intention, Intention::Sleep { .. })
                {
                    lay_down.push(id);
                }
                decisions.push((id, decision));
            }
            mind.stopped = false;
        }
        self.owed += used.saturating_sub(allowance);
        self.work = used;
        for id in lay_down {
            self.imitate(world, rules, id);
        }
        decisions.sort_by_key(|(id, _)| *id);
        decisions
    }

    /// What each body gathered this tick goes to its day's count (D50);
    /// the nightfall is noted for [`Brains::close_day`].
    fn count_gathers(&mut self, events: &[Event], rules: &Content) {
        for event in events {
            match event {
                Event::NightFell { .. } => self.dusk = true,
                Event::WorkStopped {
                    body, gives, grams, ..
                } => {
                    if let Some(mind) = self.minds.get_mut(body)
                        && let Some(food) = rules.material(gives).and_then(|m| m.food)
                    {
                        mind.gathered_today_kj += u64::from(food.energy_kj(*grams));
                    }
                }
                _ => {}
            }
        }
    }

    /// The nightfall closes every mind's day (D50): its gather and its
    /// thinness go to its days, its distress is counted, a trial under
    /// way long enough is judged, else one may start; then the children
    /// take their mother's home (their father's without one), the carried
    /// infants their carrier's, and each mind notes its home for the
    /// night's camp-mates.
    fn close_day(&mut self, world: &World, rules: &Content) {
        let trials = &rules.needs.trials;
        let seed = world.seed();
        let day = today(world);
        let now = world.tick().0;
        self.food.refresh(world, rules);
        let ids: Vec<BodyId> = self.minds.keys().copied().collect();
        for &id in &ids {
            let Some(body) = world.body(id) else {
                continue;
            };
            let mind = self.minds.get_mut(&id).expect("its mind");
            let gathered_kj = std::mem::take(&mut mind.gathered_today_kj);
            if world.is_infant(body, rules) {
                // Carried and nursed: no day of its own yet.
                mind.distress_days = 0;
                continue;
            }
            let c = body.condition();
            mind.days.push(Day {
                gathered_kj,
                thin: c.is_thin(&rules.needs),
            });
            let keep = usize::try_from(trials.trial_days)
                .unwrap_or(usize::MAX)
                .saturating_mul(2);
            if mind.days.len() > keep.max(2) {
                mind.days.remove(0);
            }
            let hungry = c.hunger.points() >= HUNGRY;
            mind.distress_days = if hungry || gathered_kj == 0 {
                mind.distress_days.saturating_add(1)
            } else {
                0
            };
            if let Some(trial) = mind.trial
                && day >= trial.started_day + u64::from(trials.trial_days)
            {
                let (kj, thin) = mind.lately(trials.trial_days);
                let kept = kj > trial.before_kj_per_day && thin <= trial.before_thin_days;
                if !kept {
                    trial.way.copy(&mut mind.ways, &trial.before);
                }
                mind.trial = None;
                self.news.push(News::Tried {
                    body: id,
                    way: trial.way,
                    kept,
                    at: now,
                });
            } else if mind.trial.is_none()
                && mind.distress_days >= trials.trial_after_days
                && scatter::roll(seed, purpose::TRIAL_TRY, &[u64::from(id.0), day]) % 100
                    < trials.try_percent(mind.will)
            {
                let child = world.age_milli(body) < 16_000;
                if let Some(news) = start_trial(world, rules, &self.food, id, mind, child, day) {
                    self.news.push(news);
                }
            }
        }
        // The children follow (D50, D47).
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

    /// `id` lies down at its camp for the night (D50): once a night, it
    /// may copy one way from the camp-mate (the same home at nightfall,
    /// within `imitate_tiles` of it, not under a trial itself) who
    /// gathered most a day over the last `trial_days`, more than it did;
    /// the chance is `imitate_percent` scaled by that mate's will. The way
    /// is drawn among those that differ; with `imitate_loss_percent` a
    /// numeric way lands halfway between its own and the mate's. A body
    /// under a trial copies nothing: the world is judging it. Copying the
    /// home moves the body's home: that is how a camp moves.
    fn imitate(&mut self, world: &World, rules: &Content, id: BodyId) {
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
        let own = mind.lately(trials.trial_days).0;
        let dusk = mind.home_at_dusk;
        let reach = i32::try_from(trials.imitate_tiles).unwrap_or(i32::MAX);
        let best = self
            .minds
            .iter()
            .filter(|(mate, m)| **mate != id && m.home_at_dusk == dusk)
            .filter_map(|(&mate, m)| {
                let at = world.body(mate)?.pos();
                ((at.x - here.x).abs() <= reach && (at.y - here.y).abs() <= reach)
                    .then(|| (m.lately(trials.trial_days).0, mate, m.ways, m.will))
            })
            .filter(|(kj, ..)| *kj > own)
            .max_by_key(|(kj, mate, ..)| (*kj, std::cmp::Reverse(*mate)));
        let mind = self.minds.get_mut(&id).expect("its mind");
        mind.copied_night = Some(day);
        let Some((_, from, theirs, will)) = best else {
            return;
        };
        let seed = world.seed();
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
        let lost = matches!(way, Way::WorthHalfTiles | Way::StockKj)
            && scatter::roll(seed, purpose::TRIAL_COPY_LOSS, &keys) % 100
                < u64::from(trials.imitate_loss_percent);
        match (way, lost) {
            (Way::WorthHalfTiles, true) => {
                mind.ways.worth_half_tiles =
                    halfway(mind.ways.worth_half_tiles, theirs.worth_half_tiles);
            }
            (Way::StockKj, true) => {
                mind.ways.stock_kj = halfway(mind.ways.stock_kj, theirs.stock_kj);
            }
            _ => way.copy(&mut mind.ways, &theirs),
        }
        self.news.push(News::Copied {
            body: id,
            from,
            way,
            lost,
            after: mind.ways,
            at: world.tick().0,
        });
    }

    /// The plants of the chunks the minds saw last turns and could not
    /// place then ([`COLD_CHUNKS_PER_SECOND`]), as many as this turn allows,
    /// the minds in turn: learnt without a thought.
    fn learn_unseen(&mut self, world: &World, rules: &Content) {
        for (&id, mind) in &mut self.minds {
            if mind.unseen.is_empty() {
                continue;
            }
            let Some(at) = mind.plants_from.filter(|_| world.body(id).is_some()) else {
                mind.unseen.clear();
                continue;
            };
            self.food.refresh(world, rules);
            let sight = sight_at(world.map(), rules, at);
            let before = mind.unseen.len();
            let plants = &mind.plants;
            let mut seen = Vec::new();
            mind.unseen.retain(|&chunk| {
                !learn_plants(
                    world,
                    &mut self.bearers,
                    &self.food,
                    plants,
                    &mut seen,
                    (chunk, at, sight),
                )
            });
            if mind.unseen.len() < before {
                remember(
                    &self.food,
                    at,
                    &mut mind.plants,
                    seen,
                    mind.ways.worth_half_tiles,
                );
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
                if let Event::BodyAte { item, .. } | Event::ActRefused { item, .. } = event {
                    // Eaten whole, refused or gone: looked at again when seen.
                    mind.food.remove(item);
                }
                if let Event::DrinkRefused { at, .. } = event {
                    mind.water.retain(|_, place| place.water != *at);
                }
                if let (Event::GatherRefused { .. }, Doing::Gathering(chunk, source)) =
                    (event, mind.doing)
                    && let Some(plant) = mind.plants.get_mut(&(chunk, source))
                {
                    // Whatever the reason, not again before a later hour.
                    plant.not_before = Some(world.tick().0 + RETRY_HOURS * hour(world));
                }
            }
        }
    }
}

/// Which thresholds the body's gauges are past: (thirsty, parched,
/// hungry, asleep).
fn pasts(world: &World, id: BodyId) -> (bool, bool, bool, bool) {
    world.body(id).map_or((false, false, false, false), |body| {
        let c = body.condition();
        (
            c.thirst.points() >= THIRSTY,
            c.thirst.points() >= PARCHED,
            c.hunger.points() >= HUNGRY,
            c.asleep,
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
fn look(world: &World, rules: &Content, at: Pos, mind: &mut Mind, land: &mut Land<'_>) -> u64 {
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
                    mind.water
                        .entry(chunk)
                        .and_modify(|known| {
                            if key < (far(known.stand, at), known.stand.y, known.stand.x) {
                                *known = place;
                            }
                        })
                        .or_insert(place);
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
    // thousands to look over.
    let moved = mind
        .plants_from
        .is_none_or(|from| (from.x - at.x).abs().max((from.y - at.y).abs()) >= LOOK_STEP);
    let mut read = 0;
    if moved {
        mind.plants_from = Some(at);
        mind.unseen.clear();
        // The nearest chunks first: a turn may not place them all.
        let mut chunks: Vec<ChunkPos> = (low.y..=high.y)
            .flat_map(|cy| (low.x..=high.x).map(move |cx| ChunkPos::new(cx, cy)))
            .collect();
        read = u64::try_from(chunks.len()).expect("fits");
        let own = at.chunk();
        chunks.sort_by_key(|c| ((c.x - own.x).abs().max((c.y - own.y).abs()), c.y, c.x));
        let mut seen = Vec::new();
        for chunk in chunks {
            if !learn_plants(
                world,
                land.bearers,
                land.food,
                &mind.plants,
                &mut seen,
                (chunk, at, sight),
            ) {
                mind.unseen.push(chunk);
            }
        }
        remember(
            land.food,
            at,
            &mut mind.plants,
            seen,
            mind.ways.worth_half_tiles,
        );
    }
    // Kept small: the nearest to the camp.
    while mind.water.len() > MEMORY_PLACES {
        let farthest = *mind
            .water
            .iter()
            .max_by_key(|(chunk, place)| (far(place.stand, camp), **chunk))
            .map(|(chunk, _)| chunk)
            .expect("not empty");
        mind.water.remove(&farthest);
    }
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

/// The trees and plants bearing food of `chunk` that a body at `at`
/// seeing `sight` tiles sees and does not know yet (`known`) go to
/// `seen`, for [`remember`]; `false` if the chunk's could not be placed
/// this turn. A chunk's plants are each of a key of their own (D45: a
/// plant keeps its room), so none goes twice.
fn learn_plants(
    world: &World,
    bearers: &mut Placed,
    food: &Food,
    known: &BTreeMap<(ChunkPos, Source), KnownPlant>,
    seen: &mut Vec<((ChunkPos, Source), KnownPlant)>,
    (chunk, at, sight): (ChunkPos, Pos, i32),
) -> bool {
    let Some(placed) = bearers.get(world, food, chunk) else {
        return false;
    };
    for held in placed {
        let tile = held.at.tile();
        let key = (chunk, held.source);
        if (tile.x - at.x).abs() <= sight
            && (tile.y - at.y).abs() <= sight
            && !known.contains_key(&key)
        {
            seen.push((
                key,
                KnownPlant {
                    at: held.at,
                    kind: held.kind,
                    spent: None,
                    not_before: None,
                },
            ));
        }
    }
    true
}

/// The minds' shared land, borrowed for a turn: the plants placed, the
/// places to drink found, what each kind bears now.
struct Land<'a> {
    bearers: &'a mut Placed,
    shores: &'a mut Lru<ChunkPos, Vec<Shore>>,
    food: &'a mut Food,
}

/// What the body decides, now that it looked.
#[allow(
    clippy::too_many_lines,
    reason = "one straight reading, top to bottom; splitting it would scatter the rule"
)] // one reading, top to bottom, of what it sees or does
fn decide(
    world: &World,
    rules: &Content,
    food: &Food,
    id: BodyId,
    mind: &mut Mind,
    seed: u64,
) -> Option<Decision> {
    let body = world.body(id)?;
    // An infant thinks nothing yet: it is carried and nursed (lot 3 B).
    if world.is_infant(body, rules) {
        return None;
    }
    let c = body.condition();
    let here = body.pos();
    let map = world.map();
    let camp = mind.ways.home;
    let decision = |intention, why| Some(Decision { intention, why });
    let (thirsty, parched, hungry, asleep) = pasts(world, id);
    // The world refuses a gather to a body whose load is full.
    let can_carry = body.load_g() < body.capacity_g(&rules.movement);
    let night = world.is_night(rules);
    if let Doing::Gathering(chunk, source) = mind.doing {
        if body.work().is_some() {
            // At work: it goes on unless thirst, hunger or night calls.
            if !thirsty && !hungry && !night {
                return None;
            }
        } else {
            // The work ended, or was refused: bare now, it is spent.
            mind.doing = Doing::Nothing;
            spend_if_bare(world, rules, food, mind, chunk, source);
        }
    }
    if let Doing::ToGather(chunk, source) = mind.doing
        && mind.stopped
    {
        stopped_short(world, food, id, mind, chunk, source);
    }
    if asleep && !parched {
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
        if can_carry && let Some(target) = plant_to_gather(world, rules, food, here, mind) {
            return go_gather(world, rules, id, mind, target, Why::Hunger);
        }
        // It knows the seasons: it goes looking by day, and only when
        // something is ripe somewhere. Out of season it bears its hunger
        // by its camp.
        let ripe_somewhere = food.now.iter().any(Option::is_some);
        if !night && ripe_somewhere {
            return go_looking(map, rules, id, here, mind, seed, Why::Hunger);
        }
    }
    if !night
        && can_carry
        && carried_kj(world, rules, id) < mind.ways.stock_kj
        && let Some(target) = plant_to_gather(world, rules, food, here, mind)
    {
        return go_gather(world, rules, id, mind, target, Why::Gather);
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
    decision(
        Intention::MoveTo {
            body: id,
            target: camp,
        },
        why,
    )
}

/// A tree or plant to gather from, and the crop: the key, the material,
/// the crop's year.
type Target<'a> = ((ChunkPos, Source), &'a str, u64);

/// The day of the world, counted from its first.
fn today(world: &World) -> u64 {
    world.tick().0 / world.calendar().ticks_per_day().max(1)
}

/// [`WAKE_SPREAD_SECONDS`] in ticks.
fn wake_spread(world: &World) -> u64 {
    (WAKE_SPREAD_SECONDS * u64::from(world.calendar().ticks_per_second())).max(1)
}

/// A game hour, in ticks: a 24th of the day.
fn hour(world: &World) -> u64 {
    (world.calendar().ticks_per_day() / 24).max(1)
}

/// The plants `seen` (none known yet) join `plants`; past
/// [`MEMORY_PLANTS`], the plants worth least from `at` (`half_tiles`, the
/// body's way) are forgotten, of both, as if all had been learnt first:
/// only those kept are learnt.
fn remember(
    food: &Food,
    at: Pos,
    plants: &mut BTreeMap<(ChunkPos, Source), KnownPlant>,
    seen: Vec<((ChunkPos, Source), KnownPlant)>,
    half_tiles: u64,
) {
    if plants.len() + seen.len() <= MEMORY_PLANTS {
        plants.extend(seen);
        return;
    }
    // The least worth go, all at once: a look over a reed bed adds
    // thousands. What bears now comes first, by what an hour of it
    // gives now; then the rest by its best crop: a fruit bush out
    // of season does not crowd out the spring's leaves.
    let rank = |key: (ChunkPos, Source), plant: &KnownPlant| {
        let (now, best) = match food.now(plant.kind) {
            Some(bears) => (true, bears.kj),
            None => (false, food.best_kj[usize::from(plant.kind)]),
        };
        (
            std::cmp::Reverse((now, worth(best, far(plant.at.tile(), at), half_tiles))),
            key,
        )
    };
    // Each with where it is: known (`None`), or seen and its place.
    let mut ranked: Vec<_> = plants
        .iter()
        .map(|(key, plant)| (rank(*key, plant), None))
        .chain(
            seen.iter()
                .enumerate()
                .map(|(i, (key, plant))| (rank(*key, plant), Some(i))),
        )
        .collect();
    ranked.select_nth_unstable_by_key(MEMORY_PLANTS, |(rank, _)| *rank);
    for ((_, key), from) in &ranked[MEMORY_PLANTS..] {
        if from.is_none() {
            plants.remove(key);
        }
    }
    for (_, from) in &ranked[..MEMORY_PLANTS] {
        if let Some(i) = *from {
            let (key, plant) = seen[i];
            plants.insert(key, plant);
        }
    }
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

/// What a plant giving `kj_per_hour` is worth `tiles` away: halved at
/// `half_tiles` (the body's way, [`WORTH_HALF_TILES`] by default).
fn worth(kj_per_hour: u64, tiles: i64, half_tiles: u64) -> u64 {
    let tiles = u64::try_from(tiles).unwrap_or(0);
    let half_tiles = half_tiles.max(1);
    kj_per_hour * half_tiles / (half_tiles + tiles)
}

/// The tree or plant it knows bearing food now worth most, not spent for
/// that crop's year nor left alone for now: what an hour of it gives,
/// less the farther.
fn plant_to_gather<'a>(
    world: &World,
    rules: &'a Content,
    food: &Food,
    here: Pos,
    mind: &Mind,
) -> Option<Target<'a>> {
    let today = today(world);
    let now = world.tick().0;
    mind.plants
        .iter()
        .filter(|(key, plant)| {
            mind.out_of_reach.get(&key.0) != Some(&today)
                && plant.not_before.is_none_or(|tick| now >= tick)
        })
        .filter_map(|(key, plant)| {
            let bears = food.now(plant.kind)?;
            if plant.spent == Some(bears.year) {
                return None;
            }
            // What an hour of it gives of what is left on it now: a
            // plant holding crumbs is left alone.
            let crop = &rules.crops[bears.crop];
            let (_, left_g) =
                world.ripe_of(key.0, key.1, plant.at, food.name(plant.kind), &crop.gives)?;
            let kj = kj_of_an_hour(bears.kj, crop.per_hour_g, left_g)?;
            Some((*key, plant, bears, kj))
        })
        .max_by_key(|(key, plant, _, kj)| {
            let tiles = far(plant.at.tile(), here);
            (
                worth(*kj, tiles, mind.ways.worth_half_tiles),
                std::cmp::Reverse(tiles),
                std::cmp::Reverse(*key),
            )
        })
        .map(|(key, _, bears, _)| (key, rules.crops[bears.crop].gives.as_str(), bears.year))
}

/// Walks to the tree or plant `target`, or gathers from it when within
/// reach.
fn go_gather(
    world: &World,
    rules: &Content,
    id: BodyId,
    mind: &mut Mind,
    (key, gives, _): Target<'_>,
    why: Why,
) -> Option<Decision> {
    let body = world.body(id)?;
    let plant = *mind.plants.get(&key)?;
    let (dx, dy) = (
        i64::from(body.at().x - plant.at.x),
        i64::from(body.at().y - plant.at.y),
    );
    if dx * dx + dy * dy <= GATHER_REACH_MM * GATHER_REACH_MM {
        mind.doing = Doing::Gathering(key.0, key.1);
        return Some(Decision {
            intention: Intention::Gather {
                body: id,
                chunk: key.0,
                source: key.1,
                gives: gives.to_owned(),
            },
            why,
        });
    }
    // A tree's trunk takes its tile: it stands beside it.
    let tile = plant.at.tile();
    let here = body.pos();
    let target = match key.1 {
        Source::Plant(_) => tile,
        Source::Tree(_) => AROUND
            .iter()
            .filter(|&&(dx, dy)| (dx, dy) != (0, 0))
            .map(|&(dx, dy)| Pos::new(tile.x + dx, tile.y + dy))
            .filter(|&p| walkable(world.map(), rules, p))
            .min_by_key(|&p| (far(p, here), p.y, p.x))
            .unwrap_or(tile),
    };
    mind.doing = Doing::ToGather(key.0, key.1);
    Some(Decision {
        intention: Intention::MoveTo { body: id, target },
        why,
    })
}

/// A walk to a plant ended. Out of reach of it, it stopped short: the
/// way there is not one it knows. The plant is left for the year, its
/// whole patch for the day, or it would try every bush beyond the same
/// river one after the other.
fn stopped_short(
    world: &World,
    food: &Food,
    id: BodyId,
    mind: &mut Mind,
    chunk: ChunkPos,
    source: Source,
) {
    mind.doing = Doing::Nothing;
    let (Some(body), Some(plant)) = (world.body(id), mind.plants.get_mut(&(chunk, source))) else {
        return;
    };
    let (dx, dy) = (
        i64::from(body.at().x - plant.at.x),
        i64::from(body.at().y - plant.at.y),
    );
    if dx * dx + dy * dy <= GATHER_REACH_MM * GATHER_REACH_MM {
        return;
    }
    if let Some(bears) = food.now(plant.kind) {
        plant.spent = Some(bears.year);
    }
    let today = today(world);
    mind.out_of_reach.retain(|_, day| *day == today);
    mind.out_of_reach.insert(chunk, today);
}

/// After a work ended or was refused: the plant is spent for the year
/// when nothing of its food is left on it and its crop is ripe (D48:
/// while it ripens, more comes).
fn spend_if_bare(
    world: &World,
    rules: &Content,
    food: &Food,
    mind: &mut Mind,
    chunk: ChunkPos,
    source: Source,
) {
    let Some(plant) = mind.plants.get_mut(&(chunk, source)) else {
        return;
    };
    let Some(bears) = food.now(plant.kind) else {
        return;
    };
    let kind = food.name(plant.kind);
    let gives = rules.crops[bears.crop].gives.as_str();
    let ripening = rules
        .crops
        .iter()
        .filter(|crop| crop.on == kind && crop.gives == gives)
        .any(|crop| crops::ripening(crop, world.calendar(), world.tick()));
    if !ripening
        && world
            .ripe_of(chunk, source, plant.at, kind, gives)
            .is_none()
    {
        plant.spent = Some(bears.year);
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

/// How far a trial moves the home, in tiles, by the will (D50): the whole
/// `new_home_walk_tiles` from `far_home_from_will`, a quarter under it.
fn will_step(will: u8, trials: &Trials) -> i32 {
    let tiles = if u32::from(will) >= trials.far_home_from_will {
        trials.new_home_walk_tiles
    } else {
        trials.new_home_walk_tiles / 4
    };
    i32::try_from(tiles).unwrap_or(i32::MAX)
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
/// half, the night out flipped, the store twice or half, or the home moved
/// [`will_step`] tiles towards where it knows the most plants still
/// bearing. A child (under sixteen) does not try its home: it takes its
/// mother's every night. The trial records what the days before gave.
fn start_trial(
    world: &World,
    rules: &Content,
    food: &Food,
    id: BodyId,
    mind: &mut Mind,
    child: bool,
    day: u64,
) -> Option<News> {
    let trials = &rules.needs.trials;
    let ways: &[Way] = if child { &Way::ALL[..3] } else { &Way::ALL };
    let r = scatter::roll(world.seed(), purpose::TRIAL_WAY, &[u64::from(id.0), day]);
    let way = ways[usize::try_from(r % u64::try_from(ways.len()).expect("four")).expect("small")];
    let up = (r >> 8).is_multiple_of(2);
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
        Way::Home => {
            let tiles = will_step(mind.will, trials);
            mind.ways.home = new_home(world, rules, food, id, mind, day, tiles);
        }
    }
    if mind.ways == before {
        // Nothing moved (a home that could go nowhere): no trial.
        return None;
    }
    let (before_kj_per_day, before_thin_days) = mind.lately(trials.trial_days);
    mind.trial = Some(Trial {
        way,
        before,
        started_day: day,
        before_kj_per_day,
        before_thin_days,
    });
    Some(News::Trying {
        body: id,
        way,
        before,
        after: mind.ways,
        at: world.tick().0,
    })
}

/// Where a trial sets `id`'s home (D50): `tiles` from its home towards the
/// direction (of eight) where it knows the most plants still bearing
/// (what is left on them now, [`World::ripe_of`]), else a direction drawn
/// from the seed, its id and the day; on the map, and on the nearest
/// walkable tile back along that line (the home itself if none).
fn new_home(
    world: &World,
    rules: &Content,
    food: &Food,
    id: BodyId,
    mind: &Mind,
    day: u64,
    tiles: i32,
) -> Pos {
    let home = mind.ways.home;
    let mut counts = [0_u32; 8];
    for (&(chunk, source), plant) in &mind.plants {
        let Some(bears) = food.now(plant.kind) else {
            continue;
        };
        if plant.spent == Some(bears.year) {
            continue;
        }
        let crop = &rules.crops[bears.crop];
        let at = plant.at.tile();
        if world
            .ripe_of(chunk, source, plant.at, food.name(plant.kind), &crop.gives)
            .is_none()
        {
            continue;
        }
        let (dx, dy) = (at.x - home.x, at.y - home.y);
        if dx == 0 && dy == 0 {
            continue;
        }
        counts[octant(dx, dy)] += 1;
    }
    let (dx, dy) = if counts.iter().all(|&n| n == 0) {
        let r = scatter::roll(
            world.seed(),
            purpose::TRIAL_DIRECTION,
            &[u64::from(id.0), day],
        );
        DIRECTIONS[usize::try_from(r % 8).expect("under eight")]
    } else {
        let best = (0..8)
            .max_by_key(|&i| (counts[i], std::cmp::Reverse(i)))
            .expect("eight");
        DIRECTIONS[best]
    };
    let map = world.map();
    let (w, h) = (
        i32::try_from(map.width()).expect("fits") - 1,
        i32::try_from(map.height()).expect("fits") - 1,
    );
    // Back along the line from the far end to the home: the first
    // walkable tile.
    for step in (1..=tiles).rev() {
        let at = Pos::new(
            home.x.saturating_add(dx.saturating_mul(step)).clamp(0, w),
            home.y.saturating_add(dy.saturating_mul(step)).clamp(0, h),
        );
        if walkable(map, rules, at) {
            return at;
        }
    }
    home
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
        let mut land = Land {
            bearers: &mut bearers,
            shores: &mut shores,
            food: &mut food,
        };
        look(world, rules, at, mind, &mut land);
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
        (world, rules, id, brains)
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
        let plant = &brains.mind(id).unwrap().plants[&(chunk, bramble.source)];
        assert_eq!(plant.spent, None, "not spent: it comes back to it");
        // A gather the world refuses all the same (a load full as it
        // began, a crumb picked by another just before): not asked again
        // before a later hour.
        let mind = brains.minds.get_mut(&id).unwrap();
        mind.doing = Doing::Gathering(chunk, bramble.source);
        let refused = Event::GatherRefused {
            body: id,
            gives: "blackberries".to_owned(),
            reason: Refusal::NothingRipe,
        };
        brains.notice(&[refused], &world);
        let plant = &brains.mind(id).unwrap().plants[&(chunk, bramble.source)];
        assert!(plant.not_before.is_some_and(|t| t > world.tick().0));
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
        // The hazel ten tiles off beats the bramble beside it by what an
        // hour of a full one gives; picked down to crumbs, it is left
        // alone and the bramble is gathered; both bare, nothing is.
        let (mut world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let chunk = here.chunk();
        let at = |tiles: i32| FinePos::new(here.centre().x + tiles * 1000, here.centre().y);
        let plant = |x_mm| {
            (
                chunk,
                Source::Plant(crate::crops::PlantKey { x_mm, y_mm: 1 }),
            )
        };
        let mut mind = Mind::new(here);
        let food = food_now(&world, &rules);
        let known = |at, kind: &str| KnownPlant {
            at,
            kind: food.numbers[kind],
            spent: None,
            not_before: None,
        };
        mind.plants.insert(plant(1), known(at(0), "bramble"));
        mind.plants.insert(plant(2), known(at(10), "hazel"));
        let chosen = |world: &World, mind: &Mind| {
            plant_to_gather(world, &rules, &food, here, mind).map(|t| t.0)
        };
        assert_eq!(chosen(&world, &mind), Some(plant(2)), "the hazel, full");
        let hazel = rules.crops.iter().find(|c| c.gives == "hazelnuts").unwrap();
        let (year, left) = world
            .ripe_of(chunk, plant(2).1, at(10), "hazel", "hazelnuts")
            .unwrap();
        let crumbs = hazel.per_hour_g * GATHER_MIN_MINUTES / 60 - 1;
        world.record_pick(chunk, plant(2).1, "hazelnuts", year, left - crumbs);
        assert_eq!(
            chosen(&world, &mind),
            Some(plant(1)),
            "the hazel holds crumbs: the bramble"
        );
        let (year, left) = world
            .ripe_of(chunk, plant(1).1, at(0), "bramble", "blackberries")
            .unwrap();
        world.record_pick(chunk, plant(1).1, "blackberries", year, left);
        assert_eq!(
            chosen(&world, &mind),
            None,
            "both bare: nothing worth a step"
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
        let (mut world, rules, _, bramble) = valley_with("bramble");
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
        let mut land = Land {
            bearers: &mut all,
            shores: &mut shores,
            food: &mut food,
        };
        look(&world, &rules, at, &mut whole, &mut land);
        assert!(!whole.plants.is_empty());
        assert!(whole.plants.keys().eq(brains.minds[&id].plants.keys()));
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
    fn it_gathers_the_richer_crop_unless_it_is_too_far() {
        let (world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let chunk = here.chunk();
        let at = |tiles: i32| FinePos::new(here.centre().x + tiles * 1000, here.centre().y);
        let plant = |x_mm| {
            (
                chunk,
                Source::Plant(crate::crops::PlantKey { x_mm, y_mm: 1 }),
            )
        };
        let mut mind = Mind::new(here);
        let food = food_now(&world, &rules);
        let known = |at, kind: &str| KnownPlant {
            at,
            kind: food.numbers[kind],
            spent: None,
            not_before: None,
        };
        mind.plants.insert(plant(1), known(at(0), "bramble"));
        mind.plants.insert(plant(2), known(at(10), "hazel"));
        // Blackberries give 1,800 kJ an hour, hazelnuts 52,000: the hazel
        // ten tiles off is worth the walk.
        assert_eq!(
            plant_to_gather(&world, &rules, &food, here, &mind).map(|t| t.0),
            Some(plant(2))
        );
        // A thousand tiles off, it is not.
        mind.plants.insert(plant(2), known(at(1000), "hazel"));
        assert_eq!(
            plant_to_gather(&world, &rules, &food, here, &mind).map(|t| t.0),
            Some(plant(1))
        );
    }

    #[test]
    fn it_forgets_what_bears_out_of_season_before_what_bears_now() {
        let (world, rules, id, _) = by_a_ripe_bramble(0);
        let here = world.body(id).unwrap().pos();
        let chunk = here.chunk();
        let plant = |x_mm| {
            (
                chunk,
                Source::Plant(crate::crops::PlantKey { x_mm, y_mm: 1 }),
            )
        };
        let food = food_now(&world, &rules);
        let known = |tiles: i32, kind: &str| KnownPlant {
            at: FinePos::new(here.centre().x + tiles * 1000, here.centre().y),
            kind: food.numbers[kind],
            spent: None,
            not_before: None,
        };
        let mut mind = Mind::new(here);
        // Wild garlic at its feet, an hour of it worth more than of
        // blackberries, but in the spring; one bramble bearing now,
        // farther.
        let garlic = u16::try_from(MEMORY_PLANTS).unwrap();
        for x_mm in 0..garlic {
            mind.plants.insert(plant(x_mm), known(0, "wild_garlic"));
        }
        let seen = vec![(plant(garlic), known(20, "bramble"))];
        remember(&food, here, &mut mind.plants, seen, WORTH_HALF_TILES);
        assert_eq!(mind.plants.len(), MEMORY_PLANTS);
        assert!(
            mind.plants.contains_key(&plant(garlic)),
            "the bramble stays"
        );
        assert_eq!(
            plant_to_gather(&world, &rules, &food, here, &mind).map(|t| t.0),
            Some(plant(garlic))
        );
    }

    #[test]
    fn stopped_short_of_a_plant_it_leaves_its_patch_for_the_day() {
        let (mut world, rules, id, mut brains) = by_a_ripe_bramble(0);
        let food = food_now(&world, &rules);
        let mind = brains.minds.get_mut(&id).unwrap();
        let here = world.body(id).unwrap().pos();
        // Two brambles of one patch, said far away, out of reach.
        let chunk = here.chunk();
        let far_at = FinePos::new(here.centre().x + 20_000, here.centre().y);
        let plant = |x_mm| {
            (
                chunk,
                Source::Plant(crate::crops::PlantKey { x_mm, y_mm: 1 }),
            )
        };
        mind.plants.clear();
        for x_mm in [1, 2] {
            mind.plants.insert(
                plant(x_mm),
                KnownPlant {
                    at: far_at,
                    kind: food.numbers["bramble"],
                    spent: None,
                    not_before: None,
                },
            );
        }
        mind.doing = Doing::ToGather(plant(1).0, plant(1).1);
        mind.stopped = true;
        stopped_short(&world, &food, id, mind, plant(1).0, plant(1).1);
        assert_eq!(mind.doing, Doing::Nothing);
        assert_eq!(
            mind.plants[&plant(1)].spent,
            Some(1),
            "this one for the year"
        );
        assert_eq!(
            plant_to_gather(&world, &rules, &food, here, mind),
            None,
            "the patch today"
        );
        let next_day = world.tick().0 + world.calendar().ticks_per_day();
        let mut events = Vec::new();
        world.advance(next_day - world.tick().0, &rules, &mut events);
        let food = food_now(&world, &rules);
        let mind = brains.minds.get(&id).unwrap();
        assert_eq!(
            plant_to_gather(&world, &rules, &food, here, mind).map(|t| t.0),
            Some(plant(2)),
            "the next day, the other one"
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
        // Asleep and tired: it would sleep on (a rested body wakes with the day).
        world.bodies[0].condition.asleep = true;
        world.bodies[0].condition.fatigue = Gauge::at(90);
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
        let decision = decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7)
            .expect("a decision");
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
        assert_eq!(
            decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7),
            None
        );
        // Stopped short of it from the camp: it forgets it.
        world.bodies[0].at = Pos::new(5, 5).centre();
        mind.doing = Doing::ToDrink(place);
        mind.stopped = true;
        assert_eq!(
            decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7),
            None
        );
        assert_eq!(mind.drinking_places().count(), 0);
    }

    #[test]
    fn hemmed_in_it_waits_for_the_next_change_rather_than_ask_leg_after_leg() {
        let mut world = field(64, 99, Pos::new(25, 5));
        thirst(&mut world, 60);
        let rules = rules();
        let id = world.bodies[0].id();
        let mut mind = Mind::new(Pos::new(5, 5));
        let first = decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7)
            .expect("it goes looking");
        assert_eq!(first.why, Why::Explore);
        // The leg ended where it set off.
        mind.stopped = true;
        assert_eq!(
            decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7),
            None
        );
        assert_eq!(mind.doing, Doing::Nothing);
        // The next change: it tries a leg again.
        mind.stopped = false;
        assert!(decide(&world, &rules, &food_now(&world, &rules), id, &mut mind, 7).is_some());
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
    fn remembering_what_it_sees_keeps_what_learning_all_then_forgetting_kept() {
        use crate::crops::tests::{run_to, valley_with};
        let (mut world, rules, _, _) = valley_with("bramble");
        run_to(&mut world, &rules, 28);
        let food = food_now(&world, &rules);
        let kinds = u64::try_from(food.kinds.len()).unwrap();
        let here = Pos::new(100, 100);
        // Before: every plant seen went into the memory, then the least
        // worth were forgotten, ranked by the kinds' names.
        let forget = |plants: &mut BTreeMap<(ChunkPos, Source), KnownPlant>| {
            let mut ranked: Vec<_> = plants
                .iter()
                .map(|(key, plant)| {
                    let kind = food.name(plant.kind);
                    let (now, best) = match by_names(&world, &rules, kind) {
                        Some((gives, _)) => (true, kj_per_hour(&rules, kind, Some(gives))),
                        None => (false, kj_per_hour(&rules, kind, None)),
                    };
                    (
                        std::cmp::Reverse((
                            now,
                            worth(best, far(plant.at.tile(), here), WORTH_HALF_TILES),
                        )),
                        *key,
                    )
                })
                .collect();
            if ranked.len() > MEMORY_PLANTS {
                ranked.select_nth_unstable(MEMORY_PLANTS);
                for (_, key) in &ranked[MEMORY_PLANTS..] {
                    plants.remove(key);
                }
            }
        };
        for round in 0u64..5 {
            let (mut known, mut seen) = (BTreeMap::new(), Vec::new());
            for i in 0..100 + round * 300 {
                let r = mix(i ^ (round << 32));
                let key = (
                    ChunkPos::new(i32::try_from(r % 7).unwrap(), 0),
                    Source::Plant(crate::crops::PlantKey {
                        x_mm: u16::try_from(i).unwrap(),
                        y_mm: u16::try_from(round).unwrap(),
                    }),
                );
                let off = |bits: u64| i32::try_from((r >> bits) % 200).unwrap() * 1000 - 100_000;
                let plant = KnownPlant {
                    at: FinePos::new(here.centre().x + off(8), here.centre().y + off(20)),
                    kind: u16::try_from((r >> 32) % kinds).unwrap(),
                    spent: (r >> 40).is_multiple_of(3).then_some(1),
                    not_before: (r >> 44).is_multiple_of(5).then_some(9),
                };
                if i % 3 == 0 && known.len() < MEMORY_PLANTS {
                    known.insert(key, plant);
                } else {
                    seen.push((key, plant));
                }
            }
            let mut all = known.clone();
            all.extend(seen.iter().copied());
            forget(&mut all);
            remember(&food, here, &mut known, seen, WORTH_HALF_TILES);
            assert_eq!(known, all, "round {round}");
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
            let mut land = Land {
                bearers: &mut bearers,
                shores: &mut shores,
                food: &mut food,
            };
            look(&world, &rules, at, &mut mind, &mut land);
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
            far_home_from_will: 70,
            new_home_walk_tiles: 20,
            imitate_tiles: 12,
            imitate_percent: percent,
            imitate_loss_percent: loss,
        };
        rules
    }

    /// Night falls on `world` for `brains`: the minds close their day.
    fn nightfall(world: &World, rules: &Content, brains: &mut Brains) -> Vec<News> {
        let day = u64::try_from(world.today()).unwrap();
        brains.think(world, rules, &[Event::NightFell { day }]);
        brains.news()
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
                worth(2850, tiles, WORTH_HALF_TILES),
                2850 * WORTH_HALF_TILES / (WORTH_HALF_TILES + u64::try_from(tiles).unwrap())
            );
        }
        assert_eq!(worth(1000, 64, 64), 500, "a way twice as wide halves at 64");
        // The content's trials absent: nobody tries nor copies.
        assert!(!rules().needs.trials.on());
    }

    #[test]
    fn the_will_is_drawn_from_the_world_seed_and_the_id_at_adoption() {
        let world = field(64, 40, Pos::new(5, 5));
        let a = piloted(&world).mind(BodyId(1)).unwrap().will();
        let b = piloted(&world).mind(BodyId(1)).unwrap().will();
        assert_eq!(a, b, "the same every time");
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
            assert_eq!((trial.before_kj_per_day, trial.before_thin_days), (0, 0));
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

    #[test]
    fn the_worlds_judgement_keeps_a_better_way_and_reverts_a_worse_one() {
        let rules = trials_at(0, 0);
        let id = BodyId(1);
        let judge = |after_kj: [u64; 2], after_thin: [bool; 2]| {
            // Day two closes (trial_days: 2) on a body born then.
            let mut world = World::empty(
                Calendar::new(1, 7200).unwrap(),
                Map::filled(64, 64, Terrain::Grass).with_camp(5, 5),
            );
            let mut events = Vec::new();
            world.advance(2 * world.calendar().ticks_per_day(), &rules, &mut events);
            world.spawn_body(Pos::new(5, 5));
            if after_thin[1] {
                world.bodies[0].condition.deficit_j =
                    world.bodies[0].condition.reserve_j(&rules.needs) * 6 / 10;
            }
            let mut brains = piloted(&world);
            let mind = brains.minds.get_mut(&id).unwrap();
            let before = *mind.ways();
            mind.ways.worth_half_tiles *= 2;
            mind.trial = Some(Trial {
                way: Way::WorthHalfTiles,
                before,
                started_day: 0,
                before_kj_per_day: 1000,
                before_thin_days: 1,
            });
            mind.days = vec![Day {
                gathered_kj: after_kj[0],
                thin: after_thin[0],
            }];
            mind.gathered_today_kj = after_kj[1];
            let news = nightfall(&world, &rules, &mut brains);
            let mind = brains.mind(id).unwrap();
            assert!(mind.trial().is_none(), "judged");
            (news, mind.ways().worth_half_tiles)
        };
        // More a day, thin no more often: kept.
        let (news, worth) = judge([1500, 1500], [false, true]);
        assert_eq!(worth, WORTH_HALF_TILES * 2);
        assert!(
            matches!(news[..], [News::Tried { body, way: Way::WorthHalfTiles, kept: true, .. }] if body == id),
            "{news:?}"
        );
        // More a day but thin more often: reverted.
        let (news, worth) = judge([1500, 1500], [true, true]);
        assert_eq!(worth, WORTH_HALF_TILES);
        assert!(
            matches!(news[..], [News::Tried { kept: false, .. }]),
            "{news:?}"
        );
        // Not more a day: reverted.
        let (news, worth) = judge([1000, 1000], [false, false]);
        assert_eq!(worth, WORTH_HALF_TILES);
        assert!(
            matches!(news[..], [News::Tried { kept: false, .. }]),
            "{news:?}"
        );
    }

    #[test]
    fn a_new_home_goes_towards_the_plants_it_knows_bearing_else_a_seeded_way() {
        let (world, rules, id, _) = by_a_ripe_bramble(0);
        let mut rules = rules;
        rules.needs.trials = trials_at(100, 0).needs.trials;
        let here = world.body(id).unwrap().pos();
        let chunk = here.chunk();
        let food = food_now(&world, &rules);
        let mut mind = Mind::new(here);
        // Knowing nothing bearing: the seeded direction, 20 tiles off.
        let seeded = new_home(&world, &rules, &food, id, &mind, 3, 20);
        assert_ne!(seeded, here);
        // Along one of the eight lines, 20 tiles off or the nearest
        // walkable tile back from there.
        let (dx, dy) = ((seeded.x - here.x).abs(), (seeded.y - here.y).abs());
        assert!(
            dx.max(dy) <= 20 && (dx == 0 || dy == 0 || dx == dy),
            "{seeded:?}"
        );
        assert!(walkable(world.map(), &rules, seeded));
        assert_eq!(
            new_home(&world, &rules, &food, id, &mind, 3, 20),
            seeded,
            "the same again"
        );
        // Three brambles known north-east of the camp, bearing: that way.
        let bramble = food.numbers["bramble"];
        for k in 1..=3u16 {
            let at = FinePos::new(
                here.centre().x + 5000,
                here.centre().y - i32::from(k) * 3000,
            );
            mind.plants.insert(
                (
                    chunk,
                    Source::Plant(crate::crops::PlantKey { x_mm: k, y_mm: 1 }),
                ),
                KnownPlant {
                    at,
                    kind: bramble,
                    spent: None,
                    not_before: None,
                },
            );
        }
        let towards = new_home(&world, &rules, &food, id, &mind, 3, 20);
        assert!(
            towards.x > here.x && towards.y < here.y,
            "north-east: {towards:?}"
        );
        assert!(walkable(world.map(), &rules, towards));
        // Spent for the year, they do not count.
        for plant in mind.plants.values_mut() {
            plant.spent = Some(1);
        }
        assert_eq!(new_home(&world, &rules, &food, id, &mind, 3, 20), seeded);
        // The step follows the will: a quarter under far_home_from_will.
        assert_eq!(will_step(69, &rules.needs.trials), 5);
        assert_eq!(will_step(70, &rules.needs.trials), 20);
        assert_eq!(octant(10, 0), 0);
        assert_eq!(octant(10, 6), 1);
        assert_eq!(octant(0, -3), 6);
        assert_eq!(octant(-9, -2), 4);
    }

    #[test]
    fn at_night_at_the_camp_a_body_copies_the_best_mates_way_losing_a_step_at_times() {
        let mut world = field(64, 40, Pos::new(5, 5));
        world.spawn_body(Pos::new(6, 5));
        world.spawn_body(Pos::new(7, 5));
        let (me, mate, far) = (BodyId(1), BodyId(2), BodyId(3));
        let copy = |rules: &Content| {
            let mut brains = piloted(&world);
            let day = |kj| Day {
                gathered_kj: kj,
                thin: false,
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
            brains.imitate(&world, rules, me);
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
            thin: false,
        }];
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        brains.minds.get_mut(&mate).unwrap().will = 0;
        brains.imitate(&world, &trials_at(100, 0), me);
        let news = brains.news();
        assert!(news.is_empty(), "{news:?}");
        // Once a night only, and never under a trial.
        let rules = trials_at(100, 0);
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            thin: false,
        }];
        brains.minds.get_mut(&mate).unwrap().will = 100;
        brains.minds.get_mut(&mate).unwrap().ways.home = Pos::new(9, 9);
        brains.imitate(&world, &rules, me);
        assert_eq!(
            brains.mind(me).unwrap().home(),
            Pos::new(9, 9),
            "the camp moved"
        );
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        brains.imitate(&world, &rules, me);
        assert!(!brains.mind(me).unwrap().ways().sleep_out, "once a night");
        let mut brains = piloted(&world);
        brains.minds.get_mut(&mate).unwrap().days = vec![Day {
            gathered_kj: 3000,
            thin: false,
        }];
        brains.minds.get_mut(&mate).unwrap().will = 100;
        brains.minds.get_mut(&mate).unwrap().ways.sleep_out = true;
        let mind = brains.minds.get_mut(&me).unwrap();
        mind.trial = Some(Trial {
            way: Way::StockKj,
            before: mind.ways,
            started_day: 0,
            before_kj_per_day: 0,
            before_thin_days: 0,
        });
        brains.imitate(&world, &rules, me);
        assert!(brains.news().is_empty(), "under a trial, it copies nothing");
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
        let food = food_now(&world, &rules);
        let mut mind = Mind::new(Pos::new(5, 5));
        assert_eq!(
            decide(&world, &rules, &food, id, &mut mind, 7),
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
            decide(&world, &rules, &food, id, &mut mind, 7),
            Some(Decision {
                intention: Intention::Sleep { body: id },
                why: Why::Night
            })
        );
    }
}
