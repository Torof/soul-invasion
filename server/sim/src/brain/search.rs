//! The search for an absent kin (D56, amended 27 November 2026; the
//! project lead: "people notice that someone is absent from the fire,
//! and go and search the last place they knew her to be"). Measured on a
//! long run: five infants died of thirst where their mother had died far
//! from the camp, nobody knowing.
//!
//! **No telepathy.** A mind keeps, of each close one (spouse, parent,
//! child) it sat with at a fire, where and when it last saw it and the
//! place it said there it was going to ([`KinSeen`]). At the dawn's fire
//! a close one not seen for `search.after_nights` nights is noticed
//! absent; one kin present, grown, goes to the place it said it was going
//! to, or else to the last place it saw it. Never to where it is: the
//! searcher reads the world only within its own sight, as it walks.
//!
//! What it may see there: the absent alive (nothing more for now), its
//! body dead, or nothing. By a body dead it looks for an infant lying
//! alone, takes it up ([`Intention::Carry`]) and carries it home; a
//! woman nurses it from then on by the world's own rule (a carrier with
//! milk), a man gives it at the camp to a grown woman. One search an
//! absence: the absence ends when the two sit at a fire again.
//!
//! Nothing here without the content's `search` section.

use std::collections::BTreeMap;

use super::{
    Brains, Decision, Doing, GroupEnd, HOME_TILES, Mind, News, TripChoice, Turn, Why, far,
    place_centre, place_of, place_side, plan_done, sight_at, today, today_u32, within,
};
use crate::content::{Content, Sex};
use crate::map::Pos;
use crate::{Body, BodyId, Event, Intention, Place, World};

/// What a close one is to the mind that remembers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tie {
    /// Its spouse.
    Spouse,
    /// Its child.
    Child,
    /// Its mother or father.
    Parent,
}

/// Where and when a mind last saw a close one, and what that one said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KinSeen {
    /// What it is to the mind.
    tie: Tie,
    /// The day it was last seen.
    day: u32,
    /// Where it stood then.
    at: Pos,
    /// The place it said at that day's fire it was going to.
    going: Option<Pos>,
    /// Its absence needs no search: one was made, or it said it goes to
    /// live elsewhere. Until the two sit at a fire again.
    settled: bool,
}

/// What a searcher saw of the one it looks for, within its own sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sight {
    /// Alive, there.
    Alive(Pos),
    /// Its body dead, or its infant lying alone, there.
    Dead,
}

/// How far a search is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Walking to the place.
    Going,
    /// Walking to an infant seen lying alone.
    Fetching(BodyId),
    /// It asked to take the infant up.
    Taking(BodyId),
    /// Carrying the infant home.
    Home(BodyId),
    /// At the camp, giving the infant to a woman; whether it asked yet.
    Giving(BodyId, BodyId, bool),
}

/// A search under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Search {
    /// Whom it looks for.
    who: BodyId,
    /// The place it goes to: said, or last seen.
    to: Pos,
    stage: Stage,
    /// What it saw of the absent on its way ([`Brains::watch_searches`]).
    seen: Option<Sight>,
}

/// Why the first kin to notice an absence did not go: a measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stayed {
    /// The last place it knew the absent at is the camp itself.
    Camp,
    /// Under the content's `search.from_years`.
    Young,
    /// Already on a search.
    Searching,
    /// It holds a goal.
    Goal,
    /// Its day is a move to a new home.
    Moving,
}

/// A step of a search, told as [`News::Search`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStep {
    /// The absence was noticed at the fire (told once an absence, by the
    /// first kin by rank).
    Noticed {
        /// What the absent is to the one who noticed.
        tie: Tie,
        /// The nights since it last saw the absent.
        nights: u32,
        /// Whether the absent had said where it was going.
        said: bool,
        /// How many kin at this fire noticed the same absent.
        kin: u32,
        /// With nobody gone: why the first kin did not go.
        stayed: Option<Stayed>,
    },
    /// It set off to the place said or last seen, this far, in tiles.
    Begun {
        /// The straight distance.
        tiles: u32,
        /// What the absent is to the searcher.
        tie: Tie,
        /// The nights since it last saw the absent.
        nights: u32,
        /// Whether it goes to a place the absent said (else: last seen).
        said: bool,
        /// The place it goes to.
        to: Pos,
    },
    /// It saw the absent alive.
    FoundAlive,
    /// It saw the absent's body, or its infant lying alone.
    FoundDead,
    /// It stood at the place and saw nothing of it (`reached`), or was
    /// stopped short of the place.
    NotFound {
        /// Whether it stood at the place.
        reached: bool,
    },
    /// It took up an infant lying alone.
    InfantTaken {
        /// The infant.
        infant: BodyId,
    },
    /// It came home carrying the infant.
    InfantHome {
        /// The infant.
        infant: BodyId,
    },
    /// At the camp it gave the infant to a woman.
    InfantGiven {
        /// The infant.
        infant: BodyId,
        /// The woman.
        to: BodyId,
    },
}

fn alive(body: &Body) -> bool {
    body.condition().vitality.points() > 0
}

fn near(a: Pos, b: Pos) -> bool {
    within(a, b, 1)
}

fn tiles(a: Pos, b: Pos) -> u32 {
    u32::try_from(far(a, b).isqrt()).unwrap_or(u32::MAX)
}

/// The infant alive lying alone nearest `here` within `sight`.
fn lone_infant(world: &World, rules: &Content, here: Pos, sight: i32) -> Option<BodyId> {
    world
        .bodies()
        .iter()
        .filter(|b| {
            b.carried_by().is_none()
                && alive(b)
                && world.is_infant(b, rules)
                && within(b.pos(), here, sight)
        })
        .min_by_key(|b| (far(b.pos(), here), b.id()))
        .map(Body::id)
}

/// Whether `body` shows no milk: a man, or a girl under the content's
/// `woman_from_years`.
fn no_milk(world: &World, rules: &Content, body: &Body) -> bool {
    body.sex() != Sex::Female
        || world.age_milli(body) < u64::from(rules.needs.ages.woman_from_years) * 1000
}

/// The grown woman at the camp around `here` to give an infant to: one
/// carrying none first, then the nearest.
fn nurse_seen(world: &World, rules: &Content, me: BodyId, here: Pos) -> Option<BodyId> {
    world
        .bodies()
        .iter()
        .filter(|b| {
            b.id() != me
                && alive(b)
                && b.carried_by().is_none()
                && !no_milk(world, rules, b)
                && within(b.pos(), here, HOME_TILES)
        })
        .min_by_key(|b| {
            let nursing = world
                .bodies()
                .iter()
                .any(|i| i.carried_by() == Some(b.id()));
            (nursing, far(b.pos(), here), b.id())
        })
        .map(Body::id)
}

impl Brains {
    /// The dead of this tick: each one's corpse, known as its own by who
    /// sees it lying.
    pub(super) fn search_events(&mut self, events: &[Event], rules: &Content) {
        if rules.needs.search.is_none() {
            return;
        }
        for event in events {
            if let Event::BodyDied { body, corpse, .. } = event {
                self.corpses.insert(*body, *corpse);
            }
        }
    }

    /// What each searcher on its way sees of the one it looks for,
    /// within its own sight, as it walks: the absent alive, its corpse
    /// lying, or its infant lying alone. A perception that wakes a
    /// thought; nothing farther than its sight is read.
    pub(super) fn watch_searches(&mut self, world: &World, rules: &Content) {
        if self.searching.is_empty() {
            return;
        }
        let corpses = &self.corpses;
        let minds = &mut self.minds;
        self.searching.retain(|id| {
            let (Some(mind), Some(body)) = (minds.get_mut(id), world.body(*id)) else {
                return false;
            };
            let Some(search) = mind.search.as_mut() else {
                return false;
            };
            if search.stage != Stage::Going || search.seen.is_some() || body.condition().asleep {
                return true;
            }
            let here = body.pos();
            let sight = sight_at(world.map(), rules, here);
            let who = search.who;
            search.seen = if let Some(b) = world.body(who).filter(|b| alive(b)) {
                within(b.pos(), here, sight).then(|| Sight::Alive(b.pos()))
            } else {
                let corpse = corpses
                    .get(&who)
                    .and_then(|c| world.item(*c))
                    .is_some_and(|item| {
                        matches!(item.place(), Place::Ground { pos } if within(pos, here, sight))
                    });
                let orphan = || {
                    world.bodies().iter().any(|b| {
                        b.mother() == Some(who)
                            && b.carried_by().is_none()
                            && alive(b)
                            && world.is_infant(b, rules)
                            && within(b.pos(), here, sight)
                    })
                };
                (corpse || orphan()).then_some(Sight::Dead)
            };
            if search.seen.is_some() {
                mind.due = true;
            }
            true
        });
    }

    /// Each present notes the close ones sitting at the fire with it:
    /// seen today, here, with the place each said it goes to; one whose
    /// home is elsewhere is known gone to live there.
    fn sit_with_kin(&mut self, world: &World, rules: &Content, present: &[BodyId]) {
        let (side, day) = (place_side(rules), today_u32(world));
        // Who sits with whom.
        let mut ties: Vec<(BodyId, BodyId, Tie)> = Vec::new();
        for &p in present {
            let Some(body) = world.body(p) else {
                continue;
            };
            for k in [body.mother(), body.father()].into_iter().flatten() {
                if present.contains(&k) {
                    ties.push((p, k, Tie::Parent));
                    ties.push((k, p, Tie::Child));
                }
            }
            if let Some(k) = body.spouse().filter(|k| present.contains(k)) {
                ties.push((p, k, Tie::Spouse));
            }
        }
        for (p, k, tie) in ties {
            let (Some(kin), Some(theirs)) = (world.body(k), self.minds.get(&k)) else {
                continue;
            };
            let going = theirs
                .plan
                .filter(|plan| plan.day == day)
                .and_then(|plan| plan.place)
                .map(|key| place_centre(key, side));
            let their_home = place_of(theirs.ways.home, side);
            let Some(mind) = self.minds.get_mut(&p) else {
                continue;
            };
            mind.kin.insert(
                k,
                KinSeen {
                    tie,
                    day,
                    at: kin.pos(),
                    going,
                    settled: place_of(mind.ways.home, side) != their_home,
                },
            );
        }
    }

    /// The absences noticed at the dawn's fires, once the day is said.
    /// Each present first notes the close ones sitting there with it:
    /// seen today, here, and the place each said it goes to; one who
    /// takes a home elsewhere is known gone to live there. Then each
    /// close one not at this fire and not seen for `search.after_nights`
    /// nights is noticed: one kin goes (its spouse first, then a parent,
    /// then a child; grown, holding no goal, not moving today), leaving
    /// the group of its day; the others know it went.
    #[allow(
        clippy::too_many_lines,
        reason = "one fire read in order: who sits, who is missed, who goes"
    )]
    pub(super) fn fire_absent(
        &mut self,
        world: &World,
        rules: &Content,
        circles: &[(Pos, Vec<BodyId>)],
    ) {
        let Some(search) = &rules.needs.search else {
            return;
        };
        let (day, now) = (today_u32(world), world.tick().0);
        let grown = u64::from(search.from_years) * 1000;
        for (camp, present) in circles {
            self.sit_with_kin(world, rules, present);
            // Who is missed, and by whom: the nearest tie first.
            let mut missed: BTreeMap<BodyId, Vec<(Tie, BodyId)>> = BTreeMap::new();
            for &p in present {
                let Some(mind) = self.minds.get(&p) else {
                    continue;
                };
                for (&k, seen) in &mind.kin {
                    if !present.contains(&k)
                        && !seen.settled
                        && day.saturating_sub(seen.day) >= search.after_nights
                    {
                        missed.entry(k).or_default().push((seen.tie, p));
                    }
                }
            }
            for (who, mut by) in missed {
                by.sort_unstable();
                // The measure of the notice, as its first kin holds it.
                let first = self.minds.get(&by[0].1).and_then(|m| m.kin.get(&who));
                let noticed = self.news.len();
                self.news.push(News::Search {
                    body: by[0].1,
                    who,
                    step: SearchStep::Noticed {
                        tie: by[0].0,
                        nights: first.map_or(0, |seen| day.saturating_sub(seen.day)),
                        said: first.is_some_and(|seen| seen.going.is_some()),
                        kin: u32::try_from(by.len()).unwrap_or(u32::MAX),
                        stayed: None,
                    },
                    at: now,
                });
                let mut gone = false;
                let mut why = None;
                for (tie, p) in by {
                    let (Some(body), Some(mind)) = (world.body(p), self.minds.get_mut(&p)) else {
                        continue;
                    };
                    let Some(seen) = mind.kin.get_mut(&who) else {
                        continue;
                    };
                    seen.settled = true;
                    let to = seen.going.unwrap_or(seen.at);
                    let (said, nights) = (seen.going.is_some(), day.saturating_sub(seen.day));
                    let plan = mind.plan.filter(|plan| plan.day == day && !plan.done);
                    if gone {
                        continue;
                    }
                    let stays = if world.age_milli(body) < grown {
                        Some(Stayed::Young)
                    } else if mind.search.is_some() {
                        Some(Stayed::Searching)
                    } else if mind.goal.is_some() {
                        Some(Stayed::Goal)
                    } else if plan.is_some_and(|plan| plan.moving) {
                        Some(Stayed::Moving)
                    } else if within(to, *camp, HOME_TILES) {
                        Some(Stayed::Camp)
                    } else {
                        None
                    };
                    if stays.is_some() {
                        why = why.or(stays);
                        continue;
                    }
                    gone = true;
                    mind.search = Some(Search {
                        who,
                        to,
                        stage: Stage::Going,
                        seen: None,
                    });
                    plan_done(mind);
                    // It leaves the group of its day: its own, called, ends
                    // as one whose caller left; another's goes on without it.
                    if let Some(group) = plan
                        .and_then(|plan| plan.leader)
                        .and_then(|leader| self.groups.get_mut(&leader))
                    {
                        if group.members[0] == p {
                            group.ended.get_or_insert(GroupEnd::Night);
                        } else {
                            group.left.insert(p);
                        }
                    }
                    self.searching.insert(p);
                    self.news.push(News::Search {
                        body: p,
                        who,
                        step: SearchStep::Begun {
                            tiles: tiles(body.pos(), to),
                            tie,
                            nights,
                            said,
                            to,
                        },
                        at: now,
                    });
                }
                if !gone
                    && let Some(News::Search {
                        step: SearchStep::Noticed { stayed, .. },
                        ..
                    }) = self.news.get_mut(noticed)
                {
                    *stayed = why;
                }
            }
        }
    }
}

/// A close one seen within its sight, as it thinks: where, and today.
pub(super) fn see_kin(world: &World, rules: &Content, here: Pos, mind: &mut Mind) {
    if rules.needs.search.is_none() || mind.kin.is_empty() {
        return;
    }
    let day = today_u32(world);
    let sight = sight_at(world.map(), rules, here);
    for (k, seen) in &mut mind.kin {
        let Some(at) = world
            .body(*k)
            .filter(|b| alive(b))
            .map(Body::pos)
            .filter(|at| within(*at, here, sight))
        else {
            continue;
        };
        if seen.day != day {
            seen.going = None;
        }
        (seen.day, seen.at) = (day, at);
    }
}

/// Carrying an infant found home, it goes home: its choice of the day,
/// as one out who chose to turn back ([`TripChoice::Home`]).
pub(super) fn homeward(world: &World, here: Pos, mind: &mut Mind) {
    if let Some(Search {
        stage: Stage::Home(_),
        ..
    }) = mind.search
        && !within(here, mind.ways.home, HOME_TILES)
    {
        mind.out = Some((today(world), TripChoice::Home));
    }
}

/// A searcher at its turn: by day it walks to the place, and what it
/// saw of the absent on its way ends the search; by a body dead it
/// fetches an infant lying alone, carries it home and, showing no milk,
/// gives it there to a grown woman. The night, thirst and hunger
/// interrupt the walk; it goes on after. [`Turn::Go`]: no search, or its
/// day goes on as any.
#[allow(
    clippy::too_many_lines,
    reason = "one search read in order: going, fetching, home, giving"
)]
pub(super) fn turn(
    world: &World,
    rules: &Content,
    (id, body, here): (BodyId, &Body, Pos),
    mind: &mut Mind,
    night: bool,
    news: &mut Vec<News>,
) -> Turn {
    let Some(mut search) = mind.search else {
        return Turn::Go;
    };
    let now = world.tick().0;
    let who = search.who;
    let tell = |news: &mut Vec<News>, step| {
        news.push(News::Search {
            body: id,
            who,
            step,
            at: now,
        });
    };
    let end = |mind: &mut Mind| {
        mind.search = None;
        if mind.doing == Doing::Search {
            mind.doing = Doing::Nothing;
        }
        Turn::Go
    };
    let walk = |mind: &mut Mind, to: Pos| {
        if mind.doing == Doing::Search && !mind.stopped && body.is_walking() {
            return Turn::Decide(None);
        }
        mind.doing = Doing::Search;
        Turn::Decide(Some(Decision {
            intention: Intention::MoveTo {
                body: id,
                target: to,
            },
            why: Why::Search,
        }))
    };
    if search.stage == Stage::Going {
        match search.seen.take() {
            Some(Sight::Alive(at)) => {
                if let Some(seen) = mind.kin.get_mut(&who) {
                    (seen.day, seen.at) = (today_u32(world), at);
                }
                tell(news, SearchStep::FoundAlive);
                return end(mind);
            }
            Some(Sight::Dead) => {
                mind.kin.remove(&who);
                tell(news, SearchStep::FoundDead);
                let sight = sight_at(world.map(), rules, here);
                let Some(infant) = lone_infant(world, rules, here, sight) else {
                    return end(mind);
                };
                search.stage = Stage::Fetching(infant);
            }
            None => {
                if night {
                    return Turn::Go;
                }
                let reached = within(here, search.to, HOME_TILES);
                if reached || (mind.doing == Doing::Search && mind.stopped) {
                    tell(news, SearchStep::NotFound { reached });
                    return end(mind);
                }
                mind.search = Some(search);
                return walk(mind, search.to);
            }
        }
    }
    let lying = |infant: BodyId| {
        world
            .body(infant)
            .filter(|b| alive(b) && world.is_infant(b, rules))
    };
    if let Stage::Taking(infant) = search.stage {
        match lying(infant).map(Body::carried_by) {
            Some(Some(by)) if by == id => {
                tell(news, SearchStep::InfantTaken { infant });
                search.stage = Stage::Home(infant);
            }
            Some(None) => search.stage = Stage::Fetching(infant),
            _ => return end(mind),
        }
    }
    if let Stage::Fetching(infant) = search.stage {
        let Some(at) = lying(infant)
            .filter(|b| b.carried_by().is_none())
            .map(Body::pos)
        else {
            return end(mind);
        };
        if near(here, at) {
            search.stage = Stage::Taking(infant);
            mind.search = Some(search);
            mind.doing = Doing::Nothing;
            mind.due = true;
            return Turn::Decide(Some(Decision {
                intention: Intention::Carry {
                    body: id,
                    infant,
                    carrier: id,
                },
                why: Why::Search,
            }));
        }
        if mind.doing == Doing::Search && mind.stopped {
            // Stopped short of it: it cannot get there.
            return end(mind);
        }
        mind.search = Some(search);
        return walk(mind, at);
    }
    if let Stage::Home(infant) = search.stage {
        if lying(infant).and_then(Body::carried_by) != Some(id) {
            return end(mind);
        }
        mind.search = Some(search);
        if !within(here, mind.ways.home, HOME_TILES) {
            // Its choice of the day from now on: it thinks again with it.
            let home = Some((today(world), TripChoice::Home));
            if mind.out != home {
                mind.out = home;
                mind.due = true;
                return Turn::Decide(None);
            }
            return Turn::Go;
        }
        tell(news, SearchStep::InfantHome { infant });
        let nurse = no_milk(world, rules, body)
            .then(|| nurse_seen(world, rules, id, here))
            .flatten();
        let Some(nurse) = nurse else {
            return end(mind);
        };
        search.stage = Stage::Giving(infant, nurse, false);
    }
    if let Stage::Giving(infant, nurse, asked) = search.stage {
        match lying(infant).and_then(Body::carried_by) {
            Some(by) if by == id => {}
            Some(by) if by == nurse => {
                tell(news, SearchStep::InfantGiven { infant, to: nurse });
                return end(mind);
            }
            _ => return end(mind),
        }
        let Some(at) = world
            .body(nurse)
            .filter(|b| alive(b) && b.carried_by().is_none())
            .map(Body::pos)
            .filter(|at| !asked && within(*at, mind.ways.home, 2 * HOME_TILES))
        else {
            return end(mind);
        };
        if near(here, at) {
            search.stage = Stage::Giving(infant, nurse, true);
            mind.search = Some(search);
            mind.doing = Doing::Nothing;
            mind.due = true;
            return Turn::Decide(Some(Decision {
                intention: Intention::Carry {
                    body: id,
                    infant,
                    carrier: nurse,
                },
                why: Why::Search,
            }));
        }
        if mind.doing == Doing::Search && mind.stopped {
            return end(mind);
        }
        mind.search = Some(search);
        return walk(mind, at);
    }
    end(mind)
}

/// Whether `mind` is on a search: it chooses no turn of a trip.
pub(super) fn searching(mind: &Mind) -> bool {
    mind.search.is_some()
}

/// Whether `mind` walks to the place of its search: at night it lies
/// down where it stands, and goes on with the day.
pub(super) fn halts_at_night(mind: &Mind) -> bool {
    mind.search.is_some_and(|s| s.stage == Stage::Going)
}
