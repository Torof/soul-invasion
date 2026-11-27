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
//! Who is not missed (the same day, after the first run read): a close
//! one out no longer than it is known to stay out, one seen from the
//! fire, one known to live at another hearth. What a searcher found is
//! told at its next fire, and the kin who hear it take it. Noticed by
//! children alone, the fire hears it: a grown kin who knows says why it
//! does not worry, else the grown one who cares most for the young goes.
//!
//! Nothing here without the content's `search` section.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::time::Instant;

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
    /// The last day the two sat at a fire together.
    fire_day: u32,
    /// The longest it is known to have stayed away from the fire and
    /// come back, in nights, and the day it came back from it.
    usual: u32,
    usual_day: u32,
    /// Where it is known to live, when not at the mind's own hearth,
    /// and the day that was learnt: said at a shared fire, said when
    /// found, or told by who found it.
    lives: Option<(Pos, u32)>,
    /// Its not being missed was told to the measure, this absence.
    quiet: bool,
}

/// What a searcher found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// Alive; and where it lives, when at another hearth.
    Alive {
        /// Its home, when not the finder's.
        lives: Option<Pos>,
    },
    /// Its body, or its infant lying alone.
    Dead,
}

/// What a searcher found, to tell at its next fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Finding {
    who: BodyId,
    at: Pos,
    day: u32,
    what: Found,
}

/// Why a close one away from the fire is not missed: a measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// Out no longer than it is known to stay out.
    Usual,
    /// Known to live at another hearth.
    LivesElsewhere,
    /// Seen from the fire: at the camp.
    SeenAtCamp,
}

/// What a searcher saw of the one it looks for, within its own sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sight {
    /// Alive, there; and the home it says when met.
    Alive(Pos, Pos),
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
    /// Noticed by children alone: a grown kin at the fire did not miss
    /// the absent, and said why.
    AdultKnows,
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
        /// The child who told the fire, when the searcher is no kin of
        /// the absent's.
        child: Option<BodyId>,
    },
    /// A close one away from the fire was not missed (told once an
    /// absence).
    Unnoticed {
        /// Why.
        cause: Quiet,
        /// What the absent is to that kin.
        tie: Tie,
        /// The nights since it last saw the absent.
        nights: u32,
    },
    /// It heard at the fire what a searcher found of its close one.
    Heard {
        /// Found dead (else alive).
        dead: bool,
        /// Who told.
        from: BodyId,
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
        let started = Instant::now();
        for event in events {
            if let Event::BodyDied { body, corpse, .. } = event {
                self.corpses.insert(*body, *corpse);
            }
        }
        self.search_nanos += elapsed_nanos(started);
    }

    /// What each searcher on its way sees of the one it looks for,
    /// within its own sight, as it walks: the absent alive, its corpse
    /// lying, or its infant lying alone. A perception that wakes a
    /// thought; nothing farther than its sight is read.
    pub(super) fn watch_searches(&mut self, world: &World, rules: &Content) {
        if self.searching.is_empty() {
            return;
        }
        let started = Instant::now();
        // Met alive, the absent says where it lives.
        let homes: BTreeMap<BodyId, Pos> = self
            .searching
            .iter()
            .filter_map(|id| self.minds.get(id)?.search.map(|s| s.who))
            .filter_map(|who| Some((who, self.minds.get(&who)?.ways.home)))
            .collect();
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
                within(b.pos(), here, sight)
                    .then(|| Sight::Alive(b.pos(), homes.get(&who).copied().unwrap_or(b.pos())))
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
        self.search_nanos += elapsed_nanos(started);
    }

    /// Each present notes the close ones sitting at the fire with it:
    /// seen today, here, with the place each said it goes to. One come
    /// back after nights away is known to stay out that long (the
    /// longest kept, bounded, faded after `usual_seasons`); one whose
    /// home is elsewhere says so: it lives there.
    fn sit_with_kin(&mut self, world: &World, rules: &Content, present: &[BodyId]) {
        let Some(search) = &rules.needs.search else {
            return;
        };
        let (side, day) = (place_side(rules), today_u32(world));
        let season = world.calendar().days_per_cycle() / 4;
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
            let their_home = theirs.ways.home;
            let Some(mind) = self.minds.get_mut(&p) else {
                continue;
            };
            let elsewhere = place_of(mind.ways.home, side) != place_of(their_home, side);
            let seen = mind.kin.entry(k).or_insert(KinSeen {
                tie,
                day,
                at: kin.pos(),
                going,
                settled: false,
                fire_day: day,
                usual: 0,
                usual_day: day,
                lives: None,
                quiet: false,
            });
            if day.saturating_sub(seen.usual_day) > search.usual_seasons * season {
                seen.usual = 0;
            }
            let away = day.saturating_sub(seen.fire_day).saturating_sub(1);
            if away > 0 && away >= seen.usual {
                seen.usual = away.min(search.usual_max_nights);
                seen.usual_day = day;
            }
            (seen.tie, seen.day, seen.at, seen.going) = (tie, day, kin.pos(), going);
            (seen.settled, seen.quiet, seen.fire_day) = (false, false, day);
            seen.lives = elsewhere.then_some((their_home, day));
        }
    }

    /// What the searchers found, told at their next fire with someone to
    /// hear: `told_per_fire` things each at most. A listener who holds
    /// the one found as a close one takes it: found alive, it is seen
    /// that day, there, and lives where it said; found dead, it waits
    /// for it no more.
    fn tell_findings(&mut self, world: &World, rules: &Content, present: &[BodyId]) {
        let Some(search) = &rules.needs.search else {
            return;
        };
        if present.len() < 2 {
            return;
        }
        let now = world.tick().0;
        let most = usize::try_from(search.told_per_fire).unwrap_or(usize::MAX);
        for &teller in present {
            let Some(mind) = self.minds.get_mut(&teller) else {
                continue;
            };
            let count = mind.findings.len().min(most);
            let told: Vec<Finding> = mind.findings.drain(..count).collect();
            for finding in told {
                for &listener in present {
                    let Some(mind) = self.minds.get_mut(&listener).filter(|_| listener != teller)
                    else {
                        continue;
                    };
                    let Some(seen) = mind.kin.get_mut(&finding.who) else {
                        continue;
                    };
                    let dead = match finding.what {
                        Found::Dead => true,
                        Found::Alive { lives } => {
                            if finding.day > seen.day {
                                (seen.day, seen.at) = (finding.day, finding.at);
                            }
                            seen.settled = true;
                            if let Some(home) = lives {
                                seen.lives = Some((home, finding.day));
                            }
                            false
                        }
                    };
                    if dead {
                        mind.kin.remove(&finding.who);
                    }
                    self.news.push(News::Search {
                        body: listener,
                        who: finding.who,
                        step: SearchStep::Heard { dead, from: teller },
                        at: now,
                    });
                }
            }
        }
    }

    /// Who of `present` misses whom this dawn, the nearest tie first. A
    /// close one seen from the fire is at the camp: seen, not absent.
    /// One known to live elsewhere, or out no longer than it is known to
    /// stay out, is not missed: said once an absence, a measure.
    fn missed_at(
        &mut self,
        world: &World,
        rules: &Content,
        present: &[BodyId],
    ) -> BTreeMap<BodyId, Vec<(Tie, BodyId)>> {
        let mut missed: BTreeMap<BodyId, Vec<(Tie, BodyId)>> = BTreeMap::new();
        let Some(search) = &rules.needs.search else {
            return missed;
        };
        let (day, now) = (today_u32(world), world.tick().0);
        let season = world.calendar().days_per_cycle() / 4;
        for &p in present {
            let (Some(body), Some(mind)) = (world.body(p), self.minds.get_mut(&p)) else {
                continue;
            };
            let here = body.pos();
            let sight = sight_at(world.map(), rules, here);
            for (&k, seen) in &mut mind.kin {
                let nights = day.saturating_sub(seen.day);
                if present.contains(&k) || nights < search.after_nights {
                    continue;
                }
                if seen.lives.is_some_and(|(_, since)| {
                    day.saturating_sub(since) > search.lives_seasons * season
                }) {
                    seen.lives = None;
                }
                // Searched already, this absence; one known to live
                // elsewhere is said so all the same.
                if seen.settled && seen.lives.is_none() {
                    continue;
                }
                let usual = if day.saturating_sub(seen.usual_day) > search.usual_seasons * season {
                    0
                } else {
                    seen.usual
                };
                let at_camp = world
                    .body(k)
                    .filter(|b| alive(b) && within(b.pos(), here, sight))
                    .map(Body::pos);
                let quiet = if let Some(at) = at_camp {
                    (seen.day, seen.at) = (day, at);
                    Some(Quiet::SeenAtCamp)
                } else if seen.lives.is_some() {
                    Some(Quiet::LivesElsewhere)
                } else if day.saturating_sub(seen.fire_day) <= usual {
                    Some(Quiet::Usual)
                } else {
                    None
                };
                let Some(cause) = quiet else {
                    missed.entry(k).or_default().push((seen.tie, p));
                    continue;
                };
                if !seen.quiet {
                    seen.quiet = true;
                    self.news.push(News::Search {
                        body: p,
                        who: k,
                        step: SearchStep::Unnoticed {
                            cause,
                            tie: seen.tie,
                            nights,
                        },
                        at: now,
                    });
                }
            }
        }
        missed
    }

    /// `p` sets off to search `who` at `to`: it leaves the group of its
    /// day (its own, called, ends as one whose caller left; another's
    /// goes on without it).
    fn set_off(&mut self, world: &World, p: BodyId, who: BodyId, to: Pos) {
        let day = today_u32(world);
        let Some(mind) = self.minds.get_mut(&p) else {
            return;
        };
        let plan = mind.plan.filter(|plan| plan.day == day && !plan.done);
        mind.search = Some(Search {
            who,
            to,
            stage: Stage::Going,
            seen: None,
        });
        plan_done(mind);
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
    }

    /// Why `p` would not go searching today, the place aside.
    fn stays(&self, world: &World, rules: &Content, p: BodyId) -> Option<Stayed> {
        let grown = u64::from(rules.needs.search.as_ref()?.from_years) * 1000;
        let (body, mind) = (world.body(p)?, self.minds.get(&p)?);
        let day = today_u32(world);
        let plan = mind.plan.filter(|plan| plan.day == day && !plan.done);
        if world.age_milli(body) < grown {
            Some(Stayed::Young)
        } else if mind.search.is_some() {
            Some(Stayed::Searching)
        } else if mind.goal.is_some() {
            Some(Stayed::Goal)
        } else if plan.is_some_and(|plan| plan.moving) {
            Some(Stayed::Moving)
        } else {
            None
        }
    }

    /// A child noticed `who` absent and nobody of its kin goes: it tells
    /// the fire. A grown one present who holds `who` as a close one and
    /// did not miss it has its reasons, and says them: the children take
    /// what it knows (how long `who` stays out, where it lives), nobody
    /// goes. With none, the grown one present who cares most for the
    /// young (D58, its innate care), free today, goes to the place the
    /// child says. Who went, and for which child; or why nobody.
    fn child_tells(
        &mut self,
        world: &World,
        rules: &Content,
        (camp, present): (Pos, &[BodyId]),
        who: BodyId,
        young: &[BodyId],
    ) -> Result<(BodyId, BodyId), Stayed> {
        let grown = |brains: &Self, p: BodyId| {
            !young.contains(&p) && brains.stays(world, rules, p) != Some(Stayed::Young)
        };
        let knows = present
            .iter()
            .filter(|&&p| grown(self, p))
            .find_map(|p| self.minds.get(p)?.kin.get(&who).copied());
        if let Some(known) = knows {
            for child in young {
                if let Some(seen) = self.minds.get_mut(child).and_then(|m| m.kin.get_mut(&who)) {
                    (seen.usual, seen.usual_day) = (known.usual, known.usual_day);
                    seen.lives = known.lives;
                }
            }
            return Err(Stayed::AdultKnows);
        }
        let said = young.iter().find_map(|&child| {
            let seen = self.minds.get(&child)?.kin.get(&who)?;
            let to = seen.going.unwrap_or(seen.at);
            (!within(to, camp, HOME_TILES)).then_some((child, to))
        });
        let Some((child, to)) = said else {
            return Err(Stayed::Camp);
        };
        let helper = present
            .iter()
            .copied()
            .filter(|&p| grown(self, p) && self.stays(world, rules, p).is_none())
            .max_by_key(|&p| (self.minds.get(&p).map_or(0, |m| m.care), Reverse(p)));
        let Some(helper) = helper else {
            return Err(Stayed::Young);
        };
        self.set_off(world, helper, who, to);
        Ok((helper, child))
    }

    /// The absences noticed at the dawn's fires, once the day is said.
    /// Each present first notes the close ones sitting there with it,
    /// and hears what the searchers found. Then each close one not at
    /// this fire nor seen from it, not known to live elsewhere, not seen
    /// for `search.after_nights` nights and out longer than it is known
    /// to stay out, is noticed: one kin goes (its spouse first, then a
    /// parent, then a child; grown, holding no goal, not moving today),
    /// leaving the group of its day; the others know it went. Noticed by
    /// children alone, they tell the fire ([`Brains::child_tells`]).
    #[allow(
        clippy::too_many_lines,
        reason = "one fire read in order: who sits, what is told, who is missed, who goes"
    )]
    pub(super) fn fire_absent(
        &mut self,
        world: &World,
        rules: &Content,
        circles: &[(Pos, Vec<BodyId>)],
    ) {
        if rules.needs.search.is_none() {
            return;
        }
        let started = Instant::now();
        let (day, now) = (today_u32(world), world.tick().0);
        for (camp, present) in circles {
            self.sit_with_kin(world, rules, present);
            self.tell_findings(world, rules, present);
            for (who, mut by) in self.missed_at(world, rules, present) {
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
                let mut young = Vec::new();
                for &(tie, p) in &by {
                    let stays = self.stays(world, rules, p);
                    let Some(seen) = self.minds.get_mut(&p).and_then(|m| m.kin.get_mut(&who))
                    else {
                        continue;
                    };
                    seen.settled = true;
                    let to = seen.going.unwrap_or(seen.at);
                    let (said, nights) = (seen.going.is_some(), day.saturating_sub(seen.day));
                    if gone {
                        continue;
                    }
                    if stays == Some(Stayed::Young) {
                        young.push(p);
                    }
                    let stays = stays.or(within(to, *camp, HOME_TILES).then_some(Stayed::Camp));
                    if stays.is_some() {
                        why = why.or(stays);
                        continue;
                    }
                    gone = true;
                    let from = world.body(p).map_or(to, Body::pos);
                    self.set_off(world, p, who, to);
                    self.news.push(News::Search {
                        body: p,
                        who,
                        step: SearchStep::Begun {
                            tiles: tiles(from, to),
                            tie,
                            nights,
                            said,
                            to,
                            child: None,
                        },
                        at: now,
                    });
                }
                if !gone && !young.is_empty() {
                    match self.child_tells(world, rules, (*camp, present), who, &young) {
                        Ok((helper, child)) => {
                            gone = true;
                            let seen = self.minds.get(&child).and_then(|m| m.kin.get(&who));
                            let (Some(seen), Some(from)) = (seen, world.body(helper)) else {
                                continue;
                            };
                            let to = seen.going.unwrap_or(seen.at);
                            self.news.push(News::Search {
                                body: helper,
                                who,
                                step: SearchStep::Begun {
                                    tiles: tiles(from.pos(), to),
                                    tie: seen.tie,
                                    nights: day.saturating_sub(seen.day),
                                    said: seen.going.is_some(),
                                    to,
                                    child: Some(child),
                                },
                                at: now,
                            });
                        }
                        Err(stayed) => why = Some(stayed),
                    }
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
        self.search_nanos += elapsed_nanos(started);
    }

    /// The wall time the search took (its events, its watch, its fire),
    /// in nanoseconds: a measure.
    #[must_use]
    pub fn search_nanos(&self) -> u64 {
        self.search_nanos
    }
}

/// Nanoseconds since `started`, saturated.
fn elapsed_nanos(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// `mind` keeps what it found of `who`, to tell at its next fire: the
/// last `search.findings_kept`.
fn found(rules: &Content, mind: &mut Mind, who: BodyId, at: Pos, day: u32, what: Found) {
    let kept = rules.needs.search.as_ref().map_or(0, |s| s.findings_kept);
    mind.findings.retain(|f| f.who != who);
    mind.findings.push(Finding { who, at, day, what });
    let over = mind
        .findings
        .len()
        .saturating_sub(usize::try_from(kept).unwrap_or(usize::MAX));
    mind.findings.drain(..over);
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
            Some(Sight::Alive(at, home)) => {
                // Another hearth than its own: it lives there.
                let side = place_side(rules);
                let lives =
                    (place_of(home, side) != place_of(mind.ways.home, side)).then_some(home);
                let day = today_u32(world);
                if let Some(seen) = mind.kin.get_mut(&who) {
                    (seen.day, seen.at) = (day, at);
                    if let Some(home) = lives {
                        seen.lives = Some((home, day));
                    }
                }
                found(rules, mind, who, at, day, Found::Alive { lives });
                tell(news, SearchStep::FoundAlive);
                return end(mind);
            }
            Some(Sight::Dead) => {
                mind.kin.remove(&who);
                found(rules, mind, who, here, today_u32(world), Found::Dead);
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
