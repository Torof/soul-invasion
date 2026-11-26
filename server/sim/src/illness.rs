//! Illness as episodes (D49, amended 26 November 2026; the study
//! `content/ai-reserve/review/vivarium/lot-5/etude-maladie-2026-11-26.md`,
//! validated by the project lead the same day): falling ill is set apart
//! from dying of it. What a body is exposed to (its age, the season, the
//! fouling of its camp, the crowd and the sick of its hearth) decides
//! whether it falls ill; its body (its age, its fat) decides whether it
//! dies of it. Two ailments, the gut and the chest, so that the warm and
//! the cold seasons each play their own part.
//!
//! The cost: one draw a body a day at its own hour, as the death draw it
//! replaces; a count by hearth place and one pass over the bodies at
//! dawn; a number a place slept at (the fouling), kept while it is worth
//! a body-night. Nothing a tick, nothing over the whole map.
//!
//! No feeling is added (the project lead: no engineered perceptions):
//! nobody shuns a crowd or a dirty camp. Sickness is a state of the body,
//! slower while it lasts; the world does the rest. Played only when the
//! content holds an `illness` section and the life rule is on; without
//! the section, the old background death by age alone, bit for bit.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::content::{Content, Illness};
use crate::r#gen::scatter::{self, purpose};
use crate::{Body, BodyId, Event, Pos, World};

/// What a body falls ill of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ailment {
    /// The gut: a fouled camp and the warm season; the young weaned most.
    Gut,
    /// The chest: the cold nights, a crowded hearth.
    Chest,
}

impl Ailment {
    /// Both, in the order a day's draw reads them.
    pub const ALL: [Self; 2] = [Self::Gut, Self::Chest];

    /// Its place in a pair of counts.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Gut => 0,
            Self::Chest => 1,
        }
    }
}

/// An episode under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sickness {
    /// What of.
    pub ailment: Ailment,
    /// The day it ends, at the body's own hour: it heals or dies then.
    pub until_day: i64,
}

/// The days until which a body healed cannot fall ill of each ailment
/// again (counted from the world's first; 0 never ill).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Immunity {
    /// Of the gut.
    #[serde(default)]
    pub gut_until: i64,
    /// Of the chest.
    #[serde(default)]
    pub chest_until: i64,
}

impl Immunity {
    /// Never healed of anything: what a snapshot leaves out.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// The day until which it is safe from `ailment`.
    #[must_use]
    pub fn until(&self, ailment: Ailment) -> i64 {
        match ailment {
            Ailment::Gut => self.gut_until,
            Ailment::Chest => self.chest_until,
        }
    }

    fn set(&mut self, ailment: Ailment, day: i64) {
        match ailment {
            Ailment::Gut => self.gut_until = day,
            Ailment::Chest => self.chest_until = day,
        }
    }
}

/// A hearth as counted at dawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HearthCount {
    /// The bodies of the hearth.
    pub members: u32,
    /// Of them, ill of each ailment ([`Ailment::index`]).
    pub sick: [u32; 2],
}

/// The day's counts by hearth place, made at dawn (never stored: a world
/// loaded counts again at its first draw).
#[derive(Debug, Clone, Default)]
pub(crate) struct Hearths {
    /// The day counted; `None` before the first.
    day: Option<i64>,
    /// By hearth place.
    counts: BTreeMap<(i32, i32), HearthCount>,
    /// The bodies whose night was cold.
    cold: BTreeSet<BodyId>,
}

/// One thousandth of a body-night: the fouling's unit.
const BODY_NIGHT: u64 = 1000;

/// Parts of a million: the draws' scale.
const MILLION: u64 = 1_000_000;

/// The place a position lies in: the squares of the content's
/// `thought.place_tiles` the hearths are keyed by (D55, D58).
#[must_use]
pub fn place_of(rules: &Content, at: Pos) -> (i32, i32) {
    let side = i32::try_from(rules.needs.thought.place_tiles)
        .unwrap_or(1)
        .max(1);
    (at.x.div_euclid(side), at.y.div_euclid(side))
}

impl World {
    /// The place of a body's hearth: its home's, as the brains told
    /// ([`World::set_homes`]), else where it stands.
    #[must_use]
    pub fn hearth_place(&self, body: &Body, rules: &Content) -> (i32, i32) {
        place_of(
            rules,
            self.homes.get(&body.id).copied().unwrap_or(body.pos()),
        )
    }

    /// The body-nights slept at `place`, in thousandths, as fouled to this
    /// dawn (D49, amended 26 November 2026): 0 for a clean place.
    #[must_use]
    pub fn fouling(&self, place: (i32, i32)) -> u64 {
        self.fouling.get(&place).copied().unwrap_or(0)
    }

    /// Every fouled place and its body-nights in thousandths, in order.
    pub fn fouled_places(&self) -> impl Iterator<Item = ((i32, i32), u64)> + '_ {
        self.fouling.iter().map(|(&p, &f)| (p, f))
    }

    /// The illness's pass at dawn: each place's fouling fades by the
    /// content's share and gains a body-night for each body standing in
    /// it (where it slept); a place worth less than a body-night is clean
    /// again and forgotten. Then the hearths are counted (members, sick)
    /// and the bodies whose night was cold noted. Nothing without the
    /// content's section.
    pub(crate) fn illness_dawn(&mut self, rules: &Content) {
        let Some(illness) = &rules.needs.illness else {
            return;
        };
        let keep = u64::from(illness.fouling_keep_permille);
        for fouling in self.fouling.values_mut() {
            *fouling = *fouling * keep / 1000;
        }
        for body in &self.bodies {
            *self.fouling.entry(place_of(rules, body.pos())).or_insert(0) += BODY_NIGHT;
        }
        self.fouling.retain(|_, f| *f >= BODY_NIGHT);
        self.count_hearths(rules, true);
    }

    /// The hearths counted for today, if they are not yet (a world loaded
    /// in the day): members and sick only, no cold night known.
    pub(crate) fn hearths_for_today(&mut self, rules: &Content) {
        if self.hearths.day != Some(self.today()) {
            self.count_hearths(rules, false);
        }
    }

    fn count_hearths(&mut self, rules: &Content, dawn: bool) {
        let cold_points = rules
            .needs
            .illness
            .as_ref()
            .map_or(u32::MAX, |i| i.cold_points);
        let mut counts: BTreeMap<(i32, i32), HearthCount> = BTreeMap::new();
        let mut cold = BTreeSet::new();
        for body in &self.bodies {
            let count = counts.entry(self.hearth_place(body, rules)).or_default();
            count.members += 1;
            if let Some(sick) = body.condition.sick {
                count.sick[sick.ailment.index()] += 1;
            }
            if dawn && body.condition.cold.points() >= cold_points {
                cold.insert(body.id);
            }
        }
        self.hearths = Hearths {
            day: Some(self.today()),
            counts,
            cold,
        };
    }

    /// A hearth's count today.
    #[must_use]
    pub fn hearth_count(&self, place: (i32, i32)) -> HearthCount {
        self.hearths.counts.get(&place).copied().unwrap_or_default()
    }

    /// A body's illness at its own hour: an episode ending heals it or
    /// kills it; a healthy body draws whether it falls ill. True when it
    /// dies of it (its death is the caller's). `age` in thousandths of an
    /// age-year.
    pub(crate) fn illness_day(
        &mut self,
        index: usize,
        (rules, illness): (&Content, &Illness),
        age: u64,
        events: &mut Vec<Event>,
    ) -> bool {
        let today = self.today();
        if let Some(sick) = self.bodies[index].condition.sick {
            if today < sick.until_day {
                return false;
            }
            return self.episode_ends(index, (rules, illness), sick.ailment, age, events);
        }
        let chances = self.fall_chances(index, (rules, illness), age);
        let id = self.bodies[index].id;
        let day = u64::try_from(today).unwrap_or(0);
        let roll =
            scatter::roll(self.seed(), purpose::ILLNESS_FALL, &[u64::from(id.0), day]) % MILLION;
        let Some((ailment, caught)) = pick(chances, roll) else {
            return false;
        };
        let days = i64::from(illness.days(ailment));
        let body = &mut self.bodies[index];
        body.condition.sick = Some(Sickness {
            ailment,
            until_day: today + days,
        });
        events.push(Event::FellIll {
            body: id,
            ailment,
            caught,
            at: body.pos(),
        });
        false
    }

    /// The end of an episode: death with the content's chance at its age
    /// (none from the old age, whose curve kills alone), times its
    /// thinness; else healed and safe from the ailment a while.
    fn episode_ends(
        &mut self,
        index: usize,
        (rules, illness): (&Content, &Illness),
        ailment: Ailment,
        age: u64,
        events: &mut Vec<Event>,
    ) -> bool {
        let needs = &rules.needs;
        let today = self.today();
        let id = self.bodies[index].id;
        let c = &self.bodies[index].condition;
        let reserve_percent =
            u32::try_from(c.reserve_left_j(needs) * 100 / c.reserve_j(needs).max(1)).unwrap_or(100);
        let ppm = death_ppm(illness, (c.is_thin(needs), c.is_weak(needs)), age, rules);
        let day = u64::try_from(today).unwrap_or(0);
        let died = ppm > 0
            && scatter::roll(self.seed(), purpose::ILLNESS_DEATH, &[u64::from(id.0), day])
                % MILLION
                < ppm;
        events.push(Event::IllnessEnded {
            body: id,
            ailment,
            died,
            reserve_percent,
            age_milli: age,
        });
        if !died {
            let c = &mut self.bodies[index].condition;
            c.sick = None;
            c.immune
                .set(ailment, today + i64::from(illness.immune_days));
        }
        died
    }

    /// A healthy body's chances today to fall ill of each ailment, in
    /// millionths: its own (age, season, exposure) and its hearth's sick.
    fn fall_chances(
        &self,
        index: usize,
        (rules, illness): (&Content, &Illness),
        age: u64,
    ) -> [(u64, u64); 2] {
        let body = &self.bodies[index];
        let c = &body.condition;
        let today = self.today();
        let season = self.time().season;
        let days = u64::from(self.calendar.days_per_age_year().max(1));
        let place = self.hearth_place(body, rules);
        let hearth = self.hearth_count(place);
        let mut chances = [(0, 0); 2];
        for ailment in Ailment::ALL {
            if today < c.immune.until(ailment) {
                continue;
            }
            let base = illness.episodes_permille(ailment, age) * 1000 / days
                * u64::from(illness.season_percent(ailment, season))
                / 100;
            let exposed = match ailment {
                Ailment::Gut => {
                    let fouled = 1000 + self.fouling(place) / u64::from(illness.fouling_reference);
                    let nursed = body
                        .carried_by
                        .and_then(|by| self.body(by))
                        .is_some_and(|carrier| !carrier.condition.is_weak(&rules.needs));
                    let nursed = if nursed {
                        illness.nursed_gut_percent
                    } else {
                        100
                    };
                    base * fouled / 1000 * u64::from(nursed) / 100
                }
                Ailment::Chest => {
                    let reference = illness.crowd_reference;
                    let over = hearth.members.saturating_sub(reference);
                    let crowd = 1000 + u64::from(over) * 1000 / u64::from(reference);
                    let cold = if self.hearths.cold.contains(&body.id) {
                        illness.cold_chest_percent
                    } else {
                        100
                    };
                    base * crowd / 1000 * u64::from(cold) / 100
                }
            };
            let sick = u64::from(hearth.sick[ailment.index()]);
            let caught = (sick * u64::from(illness.contagion_permille))
                .min(u64::from(illness.contagion_max_permille))
                * 1000;
            chances[ailment.index()] = (exposed, caught);
        }
        chances
    }
}

/// The chance an episode ending kills, in millionths: the content's at
/// `age`, times `thin_death_percent` thin or `weak_death_percent` weak;
/// none from the life rule's old age.
fn death_ppm(illness: &Illness, (thin, weak): (bool, bool), age: u64, rules: &Content) -> u64 {
    if age / 1000 >= u64::from(rules.needs.life.old_age_from_years) {
        return 0;
    }
    let times = if weak {
        illness.weak_death_percent
    } else if thin {
        illness.thin_death_percent
    } else {
        100
    };
    (illness.death_permille(age) * u64::from(times) * 10).min(MILLION)
}

/// What `roll` (under a million) falls on along the day's chances laid end
/// to end, the gut's own and caught, then the chest's: the ailment, and
/// whether it was caught from the hearth's sick. None past them all.
fn pick(chances: [(u64, u64); 2], roll: u64) -> Option<(Ailment, bool)> {
    let mut edge = 0;
    for ailment in Ailment::ALL {
        let (own, caught) = chances[ailment.index()];
        edge += own;
        if roll < edge {
            return Some((ailment, false));
        }
        edge += caught;
        if roll < edge {
            return Some((ailment, true));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Content;
    use crate::map::{Map, Terrain};
    use crate::time::{Calendar, Tick};
    use crate::{Death, Gauge, Sex};

    /// Four ticks a day, sixteen days a cycle, eight an age-year: a season
    /// of four days, as the vivarium's age-year of eight.
    fn calendar() -> Calendar {
        Calendar::new(2, 2).unwrap().with_cycle(16, 2).unwrap()
    }

    /// `needs.ron` with its illness, in a world where nobody thirsts,
    /// freezes or burns its reserve.
    fn rules() -> Content {
        let mut rules = Content::sample();
        rules.needs = ron::from_str(include_str!("../../../content/needs.ron")).unwrap();
        rules.needs.thirst_per_day = 0;
        rules.needs.cold_per_day_at_night = 0;
        rules.needs.hunger_per_day = 0;
        rules.needs.life.death_by_age = vec![(0, 0)];
        rules
    }

    fn illness(rules: &Content) -> &Illness {
        rules.needs.illness.as_ref().unwrap()
    }

    /// A world with the life rule on, at the first day of `season` (0
    /// spring … 3 winter) of its second cycle.
    fn world_in(season: u64) -> World {
        let mut w = World::empty(calendar(), Map::filled(256, 64, Terrain::Grass));
        w.set_life(true);
        w.tick = Tick((16 + season * 4) * w.calendar().ticks_per_day());
        w
    }

    fn person(w: &mut World, x: i32, years: u64) -> BodyId {
        let id = w.spawn_body(Pos::new(x, 5));
        let year = u64::from(w.calendar().days_per_age_year());
        w.set_birth(id, years * year, Sex::Female);
        id
    }

    fn index(w: &World, id: BodyId) -> usize {
        w.index_of(id).unwrap()
    }

    /// Today's chances of `id`, the hearths counted.
    fn chances(w: &mut World, rules: &Content, id: BodyId) -> [(u64, u64); 2] {
        w.count_hearths(rules, true);
        let i = index(w, id);
        let age = w.age_milli(&w.bodies[i]);
        w.fall_chances(i, (rules, illness(rules)), age)
    }

    #[test]
    fn a_young_child_falls_ill_more_often_than_a_grown_body() {
        let rules = rules();
        let mut w = world_in(0);
        let child = person(&mut w, 10, 3);
        let grown = person(&mut w, 60, 30);
        let (c, g) = (
            chances(&mut w, &rules, child),
            chances(&mut w, &rules, grown),
        );
        // 500 against 200 ‰ an age-year of 8 days, in spring (100 %).
        assert_eq!(c[0].0, 62_500);
        assert_eq!(g[0].0, 25_000);
        assert_eq!(c[1].0, 62_500);
        assert_eq!(g[1].0, 25_000);
    }

    #[test]
    fn the_gut_rises_in_summer_and_the_chest_in_winter() {
        let rules = rules();
        let at = |season| {
            let mut w = world_in(season);
            let id = person(&mut w, 10, 30);
            chances(&mut w, &rules, id)
        };
        let (summer, winter) = (at(1), at(3));
        assert_eq!(summer[0].0, 25_000 * 150 / 100);
        assert_eq!(winter[0].0, 25_000 * 70 / 100);
        assert_eq!(summer[1].0, 25_000 * 50 / 100);
        assert_eq!(winter[1].0, 25_000 * 200 / 100);
    }

    #[test]
    fn a_fouled_camp_raises_the_gut_alone() {
        let rules = rules();
        let mut w = world_in(0);
        let id = person(&mut w, 10, 30);
        let place = w.hearth_place(&w.bodies[0], &rules);
        // 400 body-nights, the reference: twice the chance.
        w.fouling.insert(place, 400 * BODY_NIGHT);
        let c = chances(&mut w, &rules, id);
        assert_eq!(c[0].0, 50_000);
        assert_eq!(c[1].0, 25_000, "the chest is not the camp's");
    }

    #[test]
    fn the_fouling_gains_a_body_night_a_sleeper_and_fades_until_clean() {
        let rules = rules();
        let keep = u64::from(illness(&rules).fouling_keep_permille);
        let mut w = world_in(0);
        for x in 0..3 {
            person(&mut w, x, 30);
        }
        let place = place_of(&rules, Pos::new(0, 5));
        w.illness_dawn(&rules);
        assert_eq!(w.fouling(place), 3 * BODY_NIGHT);
        w.illness_dawn(&rules);
        assert_eq!(
            w.fouling(place),
            3 * BODY_NIGHT * keep / 1000 + 3 * BODY_NIGHT
        );
        // The sleepers gone, it fades, and once worth less than a
        // body-night the place is forgotten.
        w.bodies.clear();
        let mut dawns = 0;
        while w.fouling(place) > 0 {
            w.illness_dawn(&rules);
            dawns += 1;
        }
        // 5,874 thousandths fading to 958 a dawn: under one after 42.
        assert_eq!(dawns, 42);
        assert_eq!(w.fouled_places().count(), 0);
    }

    #[test]
    fn a_crowded_hearth_raises_the_chest_alone() {
        let rules = rules();
        let at = |members: i32| {
            let mut w = world_in(0);
            let ids: Vec<_> = (0..members).map(|x| person(&mut w, x % 16, 30)).collect();
            chances(&mut w, &rules, ids[0])
        };
        assert_eq!(at(10)[1].0, 25_000, "under the reference");
        assert_eq!(at(25)[1].0, 25_000, "at it");
        assert_eq!(at(50)[1].0, 50_000, "as many again: twice");
        assert_eq!(at(50)[0].0, 25_000, "the gut is not the crowd's");
    }

    #[test]
    fn a_cold_night_raises_the_chest() {
        let rules = rules();
        let mut w = world_in(0);
        let id = person(&mut w, 10, 30);
        w.bodies[0].condition.cold = Gauge::at(100);
        let c = chances(&mut w, &rules, id);
        assert_eq!(c[1].0, 25_000 * 150 / 100);
        assert_eq!(c[0].0, 25_000);
    }

    #[test]
    fn an_infant_nursed_by_a_carrier_not_weak_is_spared_the_gut() {
        let rules = rules();
        let mut w = world_in(0);
        let mother = person(&mut w, 10, 25);
        let infant = person(&mut w, 10, 0);
        assert!(w.carry(mother, infant));
        // 500 ‰ an age-year at birth, 30 % of it nursed.
        assert_eq!(chances(&mut w, &rules, infant)[0].0, 62_500 * 30 / 100);
        let i = index(&w, mother);
        let reserve = w.bodies[i].condition.reserve_j(&rules.needs);
        w.bodies[i].condition.deficit_j = reserve * 90 / 100;
        assert_eq!(
            chances(&mut w, &rules, infant)[0].0,
            62_500,
            "weak, she spares nothing"
        );
    }

    #[test]
    fn the_hearths_sick_add_their_share_up_to_the_cap() {
        let rules = rules();
        let mut w = world_in(0);
        let id = person(&mut w, 0, 30);
        for x in 1..4 {
            let sick = person(&mut w, x, 30);
            let i = index(&w, sick);
            w.bodies[i].condition.sick = Some(Sickness {
                ailment: Ailment::Gut,
                until_day: w.today() + 2,
            });
        }
        let c = chances(&mut w, &rules, id);
        assert_eq!(c[0].1, 30_000, "three sick of the gut, 1 % each");
        assert_eq!(c[1].1, 0, "none of the chest");
        for x in 4..30 {
            let sick = person(&mut w, x % 16, 30);
            let i = index(&w, sick);
            w.bodies[i].condition.sick = Some(Sickness {
                ailment: Ailment::Gut,
                until_day: w.today() + 2,
            });
        }
        assert_eq!(chances(&mut w, &rules, id)[0].1, 200_000, "the cap");
    }

    #[test]
    fn a_healed_body_is_safe_from_that_ailment_a_while_alone() {
        let mut rules = rules();
        rules.needs.illness.as_mut().unwrap().death_by_age = vec![(0, 0)];
        let mut w = world_in(0);
        let id = person(&mut w, 0, 30);
        let today = w.today();
        w.bodies[0].condition.sick = Some(Sickness {
            ailment: Ailment::Chest,
            until_day: today,
        });
        let mut events = Vec::new();
        let age = w.age_milli(&w.bodies[0]);
        let ill = rules.needs.illness.clone().unwrap();
        assert!(!w.illness_day(0, (&rules, &ill), age, &mut events));
        assert_eq!(w.bodies[0].condition.sick, None, "healed");
        let days = i64::from(ill.immune_days);
        assert_eq!(
            w.bodies[0].condition.immune.until(Ailment::Chest),
            today + days
        );
        assert!(matches!(
            events[..],
            [Event::IllnessEnded {
                ailment: Ailment::Chest,
                died: false,
                ..
            }]
        ));
        let c = chances(&mut w, &rules, id);
        assert_eq!(c[1], (0, 0), "safe from the chest");
        assert_eq!(c[0].0, 25_000, "not from the gut");
        w.tick = Tick(w.tick.0 + u64::try_from(days).unwrap() * w.calendar().ticks_per_day());
        // Four days on, summer: the chest at half.
        assert_eq!(chances(&mut w, &rules, id)[1].0, 12_500, "safe no more");
    }

    #[test]
    fn an_episode_kills_by_age_three_times_more_thin_and_five_weak() {
        let rules = rules();
        let ill = illness(&rules);
        let fat = death_ppm(ill, (false, false), 30_000, &rules);
        assert_eq!(fat, 15_000, "15 ‰ an episode at thirty");
        assert_eq!(
            death_ppm(ill, (true, false), 30_000, &rules),
            fat * 250 / 100
        );
        assert_eq!(death_ppm(ill, (true, true), 30_000, &rules), fat * 5);
        assert_eq!(
            death_ppm(ill, (false, false), 0, &rules),
            200_000,
            "a newborn"
        );
        assert_eq!(
            death_ppm(ill, (true, true), 70_000, &rules),
            0,
            "from the old age the curve kills alone"
        );
    }

    #[test]
    fn an_episode_lasts_its_days_then_kills_as_drawn() {
        let mut rules = rules();
        {
            let ill = rules.needs.illness.as_mut().unwrap();
            // Certain: everyone falls ill of the gut and dies of it.
            ill.gut_by_age = vec![(0, 1000)];
            ill.chest_by_age = vec![(0, 0)];
            ill.gut_season_percent.spring = 800;
            ill.death_by_age = vec![(0, 1000)];
        }
        let mut w = world_in(0);
        let id = person(&mut w, 0, 30);
        let mut events = Vec::new();
        let per_day = w.calendar().ticks_per_day();
        let mut fell = None;
        let mut died = None;
        for _ in 0..per_day * 8 {
            w.step(&rules, &mut events);
            for e in events.drain(..) {
                match e {
                    Event::FellIll { body, ailment, .. } if body == id => {
                        assert_eq!(ailment, Ailment::Gut);
                        fell = Some(w.today());
                        assert!(w.body(id).unwrap().condition.sick.is_some());
                        let pace = w.body(id).unwrap().condition.pace_percent(&rules.needs);
                        assert_eq!(pace, 70, "a sick body goes slower");
                    }
                    Event::BodyDied { body, cause, .. } if body == id => {
                        assert_eq!(cause, Death::Illness);
                        died = Some(w.today());
                    }
                    _ => {}
                }
            }
        }
        let (fell, died) = (fell.expect("fell ill"), died.expect("died"));
        assert_eq!(died - fell, 2, "the gut lasts two days");
    }

    #[test]
    fn without_the_section_nobody_falls_ill_and_no_place_fouls() {
        let mut rules = rules();
        rules.needs.illness = None;
        rules.needs.life.death_by_age = vec![(0, 0)];
        let mut w = world_in(0);
        for x in 0..20 {
            person(&mut w, x, 3);
        }
        let mut events = Vec::new();
        w.advance(20 * w.calendar().ticks_per_day(), &rules, &mut events);
        assert!(!events.iter().any(|e| matches!(e, Event::FellIll { .. })));
        assert!(w.bodies.iter().all(|b| b.condition.sick.is_none()));
        assert_eq!(w.fouled_places().count(), 0);
    }

    #[test]
    fn a_healthy_world_is_stored_as_before_and_a_sick_one_comes_back() {
        let rules = rules();
        let mut w = world_in(0);
        person(&mut w, 0, 30);
        let healthy = serde_json::to_string(&w).unwrap();
        assert!(!healthy.contains("sick") && !healthy.contains("immune"));
        assert!(!healthy.contains("fouling"));
        w.bodies[0].condition.sick = Some(Sickness {
            ailment: Ailment::Gut,
            until_day: 40,
        });
        w.bodies[0].condition.immune.set(Ailment::Chest, 33);
        w.illness_dawn(&rules);
        let back: World = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(back.bodies[0].condition.sick, w.bodies[0].condition.sick);
        assert_eq!(
            back.bodies[0].condition.immune,
            w.bodies[0].condition.immune
        );
        assert_eq!(back.fouling, w.fouling);
    }

    #[test]
    fn the_draw_falls_on_the_own_chances_then_the_caught_ones_in_order() {
        let chances = [(100, 50), (200, 10)];
        assert_eq!(pick(chances, 0), Some((Ailment::Gut, false)));
        assert_eq!(pick(chances, 120), Some((Ailment::Gut, true)));
        assert_eq!(pick(chances, 150), Some((Ailment::Chest, false)));
        assert_eq!(pick(chances, 355), Some((Ailment::Chest, true)));
        assert_eq!(pick(chances, 360), None);
    }
}
