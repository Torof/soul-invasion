//! Births, background deaths and kinship (lot 3 C, D49): a woman
//! conceives, carries and gives birth; bodies die of illness and of old
//! age besides want; a body takes a spouse. Every number is the content's
//! (`needs.ron`, `life`; D30 placeholders), every draw is fixed by the
//! world's seed, a body's id and the date (D24: no random state beyond the
//! seed). The whole rule plays only in a world whose `life` is on: the
//! vivarium's, never the game's.
//!
//! The two clocks: what is counted in years follows the season (a
//! pregnancy lasts seasons, fertility, mating and the death curve go by
//! the age in years), what is counted in days follows the day (the daily
//! death draw, the day a child is due).

use serde::{Deserialize, Serialize};

use crate::content::{Content, Sex};
use crate::r#gen::scatter::{self, purpose};
use crate::time::Season;
use crate::{Body, BodyId, Death, Event, Pos, World};

/// The child a woman carries unborn (D49).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pregnancy {
    /// The day she conceived, counted from the world's first.
    pub since_day: i64,
    /// Her spouse that day: the child's father.
    pub father: BodyId,
}

/// Tiles between two positions, the longer way (as a square reach).
fn tiles_apart(a: Pos, b: Pos) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

impl World {
    /// Whether bodies are born, couple and die of illness and old age
    /// (D49): off in the game, on in the vivarium alone.
    #[must_use]
    pub fn life(&self) -> bool {
        self.life
    }

    /// Turns the life rule on or off (D49). Off by default and in every
    /// world saved before it; the vivarium turns it on.
    pub fn set_life(&mut self, on: bool) {
        self.life = on;
        self.grow_due = true;
    }

    /// The tick of the day a body's daily draws fall on, its own, from
    /// its id: the deaths of a day are spread over it, not stacked at
    /// dawn.
    fn own_hour(&self, id: BodyId) -> u64 {
        scatter::roll(self.seed(), purpose::LIFE_HOUR, &[u64::from(id.0)])
            % self.calendar.ticks_per_day()
    }

    /// Whether `a` and `b` are kin too near to couple (D49): parent and
    /// child, or siblings, half-siblings included (same mother or same
    /// father). Ids are never reused, so a dead parent still bars.
    fn are_kin(a: &Body, b: &Body) -> bool {
        let parent_of = |p: &Body, c: &Body| c.mother == Some(p.id) || c.father == Some(p.id);
        let share = |x: Option<BodyId>, y: Option<BodyId>| x.is_some() && x == y;
        parent_of(a, b) || parent_of(b, a) || share(a.mother, b.mother) || share(a.father, b.father)
    }

    /// The season's conceptions (D49, amended), at the dawn of each of
    /// the season's first `conceive_nights` days: every woman of the
    /// content's fertile years, with more than its share of her reserve,
    /// carrying no infant and no unborn child, whose spouse is alive within
    /// its reach that dawn, conceives with the content's chance, drawn
    /// from the seed, her id, the year and the season: one draw a season,
    /// met on any of those dawns her spouse is back.
    pub(crate) fn conceive(&mut self, rules: &Content, year: u64, season: Season) {
        if !self.life {
            return;
        }
        let life = &rules.needs.life;
        let needs = &rules.needs;
        let reach = i32::try_from(life.mate_within_tiles).unwrap_or(i32::MAX);
        let season_index = Season::ALL.iter().position(|&s| s == season).unwrap_or(0) as u64;
        let today = self.today();
        let seed = self.seed();
        let mut conceived = Vec::new();
        for woman in &self.bodies {
            if woman.sex != Sex::Female
                || woman.pregnant.is_some()
                || !life.is_fertile(self.age_milli(woman))
            {
                continue;
            }
            let c = &woman.condition;
            if c.reserve_left_j(needs) * 100
                <= c.reserve_j(needs) * u64::from(life.conceive_reserve_percent)
            {
                continue;
            }
            if self.bodies.iter().any(|b| b.carried_by == Some(woman.id)) {
                continue;
            }
            let Some(spouse) = woman.spouse.and_then(|s| self.body(s)) else {
                continue;
            };
            if tiles_apart(spouse.pos(), woman.pos()) > reach {
                continue;
            }
            if scatter::chance(
                life.conceive_percent * 10,
                seed,
                purpose::LIFE_CONCEIVE,
                &[u64::from(woman.id.0), year, season_index],
            ) {
                conceived.push((woman.id, spouse.id));
            }
        }
        for (mother, father) in conceived {
            if let Some(index) = self.index_of(mother) {
                self.bodies[index].pregnant = Some(Pregnancy {
                    since_day: today,
                    father,
                });
                self.grow_due = true;
            }
        }
    }

    /// The day a child conceived on `since_day` is due: the content's
    /// seasons later, a season being a quarter of the year's days.
    fn due_day(&self, rules: &Content, since_day: i64) -> i64 {
        let days = u64::from(rules.needs.life.pregnancy_seasons)
            * u64::from(self.calendar.days_per_year())
            / u64::try_from(Season::ALL.len()).unwrap_or(1);
        since_day.saturating_add(i64::try_from(days).unwrap_or(i64::MAX))
    }

    /// The life rule's tick (D49), when `life` is on. At the start of a
    /// day: spouses apart too long part, a weak pregnant woman loses her
    /// child, a woman at her due day gives birth. At each body's own hour
    /// of the day: a body without a spouse takes one, and the body draws
    /// its death of the day.
    pub(crate) fn live(&mut self, rules: &Content, tick_of_day: u64, events: &mut Vec<Event>) {
        if !self.life {
            return;
        }
        if tick_of_day == 0 {
            self.part(rules, events);
            self.lose_or_bear(rules, events);
        }
        self.couple_and_die(rules, tick_of_day, events);
    }

    /// The couples at a day's start (D49, amended 24 November 2026): a
    /// body whose spouse stands beyond `mate_within_tiles` counts one more
    /// dawn apart, one back within it counts none; at `part_after_days`
    /// dawns in a row the two part, both free to take another by the
    /// daily rule. Their children stay with their mother: nothing of
    /// theirs changes. Nothing when `part_after_days` is 0.
    fn part(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let life = &rules.needs.life;
        if life.part_after_days == 0 {
            return;
        }
        let reach = i32::try_from(life.mate_within_tiles).unwrap_or(i32::MAX);
        let mut parted = Vec::new();
        for index in 0..self.bodies.len() {
            let body = &self.bodies[index];
            let Some(spouse) = body.spouse.and_then(|s| self.body(s)) else {
                continue;
            };
            let apart = tiles_apart(spouse.pos(), body.pos()) > reach;
            let (a, b) = (body.id, spouse.id);
            let days = &mut self.bodies[index].apart_days;
            *days = if apart { *days + 1 } else { 0 };
            if *days >= life.part_after_days && a < b {
                parted.push((a, b));
            }
        }
        for (a, b) in parted {
            for id in [a, b] {
                if let Some(index) = self.index_of(id) {
                    self.bodies[index].spouse = None;
                    self.bodies[index].apart_days = 0;
                }
            }
            events.push(Event::Parted { a, b });
        }
    }

    /// The pregnancies at a day's start: lost by a weak woman, when the
    /// content says so; brought to term at the due day.
    fn lose_or_bear(&mut self, rules: &Content, events: &mut Vec<Event>) {
        let life = &rules.needs.life;
        let today = self.today();
        let mut lost = Vec::new();
        let mut due = Vec::new();
        for woman in &self.bodies {
            let Some(pregnancy) = woman.pregnant else {
                continue;
            };
            if life.lose_when_weak && woman.condition.is_weak(&rules.needs) {
                lost.push((woman.id, woman.pos()));
            } else if today >= self.due_day(rules, pregnancy.since_day) {
                due.push(woman.id);
            }
        }
        for (mother, at) in lost {
            if let Some(index) = self.index_of(mother) {
                self.bodies[index].pregnant = None;
                self.grow_due = true;
                events.push(Event::PregnancyLost { mother, at });
            }
        }
        for mother in due {
            self.give_birth(mother, rules, events);
        }
    }

    /// A child is born where its mother stands, carried and nursed by her
    /// (D47); a boy with the content's chance, from the seed, her id and
    /// the day. With the content's chance she dies giving birth, and the
    /// infant goes to the nearest woman as a dead carrier's does (D47).
    fn give_birth(&mut self, mother: BodyId, rules: &Content, events: &mut Vec<Event>) {
        let life = &rules.needs.life;
        let Some(index) = self.index_of(mother) else {
            return;
        };
        let Some(pregnancy) = self.bodies[index].pregnant.take() else {
            return;
        };
        self.grow_due = true;
        let at = self.bodies[index].pos();
        let today = self.today();
        let seed = self.seed();
        let day = u64::try_from(today).unwrap_or(0);
        let boy = scatter::chance(
            life.boys_percent * 10,
            seed,
            purpose::LIFE_SEX,
            &[u64::from(mother.0), day],
        );
        let child = self.add_body(at);
        let index = self.index_of(child).expect("just added");
        let newborn = &mut self.bodies[index];
        newborn.born_day = today;
        newborn.sex = if boy { Sex::Male } else { Sex::Female };
        newborn.mother = Some(mother);
        newborn.father = Some(pregnancy.father);
        self.carry(mother, child);
        events.push(Event::BodyBorn {
            body: child,
            mother,
            father: pregnancy.father,
            at,
        });
        if scatter::chance(
            life.childbirth_death_percent * 10,
            seed,
            purpose::LIFE_CHILDBIRTH,
            &[u64::from(mother.0), day],
        ) {
            self.die(mother, Death::Childbirth, rules, events);
        }
    }

    /// At each body's own hour: it takes a spouse if it has none, then
    /// draws its death of the day, the year's chance at its age shared
    /// over the year's days, of illness under the content's old age and
    /// of old age from it.
    fn couple_and_die(&mut self, rules: &Content, tick_of_day: u64, events: &mut Vec<Event>) {
        let life = &rules.needs.life;
        let seed = self.seed();
        let day = u64::try_from(self.today()).unwrap_or(0);
        let days_per_year = self.calendar.days_per_year();
        let mut died = Vec::new();
        for index in 0..self.bodies.len() {
            let id = self.bodies[index].id;
            if self.own_hour(id) != tick_of_day {
                continue;
            }
            self.couple(index, rules, events);
            let age = self.age_milli(&self.bodies[index]);
            let ppm = life.death_ppm_per_day(age, days_per_year);
            if scatter::roll(seed, purpose::LIFE_DEATH, &[u64::from(id.0), day]) % 1_000_000 < ppm {
                let cause = if age / 1000 >= u64::from(life.old_age_from_years) {
                    Death::OldAge
                } else {
                    Death::Illness
                };
                died.push((id, cause));
            }
        }
        for (id, cause) in died {
            self.die(id, cause, rules, events);
        }
    }

    /// Couples now every body of the mating years without a spouse, by the
    /// daily rule (`World::couple`), the lowest id first: the vivarium's
    /// departure pairs its adults on day one rather than at their own hours
    /// (lot 3 C), so its children can be given parents among the couples.
    /// Nothing when `life` is off; `Event::Coupled` for each pair made.
    pub fn couple_all(&mut self, rules: &Content, events: &mut Vec<Event>) {
        if !self.life {
            return;
        }
        for index in 0..self.bodies.len() {
            self.couple(index, rules, events);
        }
    }

    /// A body of the content's mating years without a spouse takes the
    /// nearest body of the other sex within the content's reach, of those
    /// years, without a spouse, that is not its kin (`are_kin`); the
    /// nearer first, the lower id between equals.
    fn couple(&mut self, index: usize, rules: &Content, events: &mut Vec<Event>) {
        let life = &rules.needs.life;
        let seeker = &self.bodies[index];
        if seeker.spouse.is_some() || !life.mates(self.age_milli(seeker)) {
            return;
        }
        let reach = i32::try_from(life.mate_within_tiles).unwrap_or(i32::MAX);
        let here = seeker.pos();
        let found = self
            .bodies
            .iter()
            .filter(|other| {
                other.id != seeker.id
                    && other.sex != seeker.sex
                    && other.spouse.is_none()
                    && life.mates(self.age_milli(other))
                    && !Self::are_kin(seeker, other)
            })
            .map(|other| (tiles_apart(other.pos(), here), other.id))
            .filter(|&(d, _)| d <= reach)
            .min()
            .map(|(_, id)| id);
        let Some(spouse) = found else {
            return;
        };
        let a = seeker.id;
        self.bodies[index].spouse = Some(spouse);
        if let Some(other) = self.index_of(spouse) {
            self.bodies[other].spouse = Some(a);
        }
        events.push(Event::Coupled { a, b: spouse });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::content::Life;
    use crate::map::{Map, Terrain};
    use crate::time::Calendar;

    /// The content's rules with `needs.ron`'s life, in a world where nobody
    /// thirsts, freezes or burns its reserve: the demography alone.
    fn fed_rules() -> Content {
        let mut rules = Content::sample();
        rules.needs = ron::from_str(include_str!("../../../content/needs.ron")).unwrap();
        rules.needs.thirst_per_day = 0;
        rules.needs.cold_per_day_at_night = 0;
        rules.needs.hunger_per_day = 0;
        rules
    }

    /// The content with a year's death certain: 12.5 % a day of eight.
    fn deadly_rules() -> Content {
        let mut rules = fed_rules();
        rules.needs.life.death_by_age = vec![(0, 1000)];
        rules
    }

    /// Four ticks a day, eight days a year: a season of two days, a
    /// pregnancy of six.
    fn calendar() -> Calendar {
        Calendar::new(2, 2).unwrap().with_days_per_year(8).unwrap()
    }

    fn world_with(life: bool) -> World {
        let mut w = World::empty(calendar(), Map::filled(128, 16, Terrain::Grass));
        w.set_life(life);
        w
    }

    fn world() -> World {
        world_with(true)
    }

    /// A body `years` old, of `sex`, standing at (x, y).
    fn person(w: &mut World, x: i32, y: i32, years: u64, sex: Sex) -> BodyId {
        let id = w.spawn_body(Pos::new(x, y));
        let year = u64::from(w.calendar().days_per_year());
        w.set_birth(id, years * year, sex);
        id
    }

    /// Changes a body by hand, as a rule would: the bodies grow again.
    fn set(w: &mut World, id: BodyId, change: impl FnOnce(&mut Body)) {
        let index = w.index_of(id).unwrap();
        change(&mut w.bodies[index]);
        w.grow_due = true;
    }

    fn wed(w: &mut World, a: BodyId, b: BodyId) {
        set(w, a, |body| body.spouse = Some(b));
        set(w, b, |body| body.spouse = Some(a));
    }

    /// A wife of 25 and her husband of 27, two tiles apart.
    fn couple_of(w: &mut World) -> (BodyId, BodyId) {
        let wife = person(w, 10, 5, 25, Sex::Female);
        let husband = person(w, 12, 5, 27, Sex::Male);
        wed(w, wife, husband);
        (wife, husband)
    }

    /// Steps to the first tick of the next season, its conceptions drawn.
    fn to_season_start(w: &mut World, rules: &Content, events: &mut Vec<Event>) {
        loop {
            w.step(rules, events);
            if w.time().is_season_start() {
                return;
            }
        }
    }

    fn pregnancy(w: &World, id: BodyId) -> Option<Pregnancy> {
        w.body(id).unwrap().pregnant
    }

    fn born(events: &[Event]) -> Vec<(BodyId, BodyId, BodyId, Pos)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::BodyBorn {
                    body,
                    mother,
                    father,
                    at,
                } => Some((*body, *mother, *father, *at)),
                _ => None,
            })
            .collect()
    }

    fn deaths(events: &[Event]) -> Vec<(BodyId, Death)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::BodyDied { body, cause, .. } => Some((*body, *cause)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_fertile_fed_wife_with_her_husband_near_conceives_at_a_seasons_start() {
        let mut rules = fed_rules();
        rules.needs.life.conceive_percent = 100;
        // The season's dawn alone (the tests' seasons last two days).
        rules.needs.life.conceive_nights = 1;
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let mut events = Vec::new();
        w.step(&rules, &mut events);
        assert!(!w.time().is_season_start());
        assert_eq!(pregnancy(&w, wife), None, "only at a season's start");
        to_season_start(&mut w, &rules, &mut events);
        let today = w.today();
        assert_eq!(
            pregnancy(&w, wife),
            Some(Pregnancy {
                since_day: today,
                father: husband,
            })
        );
        assert_eq!(pregnancy(&w, husband), None, "a man does not");
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(
            pregnancy(&w, wife).map(|p| p.since_day),
            Some(today),
            "not twice"
        );
    }

    fn to_dawn(w: &mut World, rules: &Content, events: &mut Vec<Event>) {
        loop {
            w.step(rules, events);
            if w.time().is_day_start() {
                return;
            }
        }
    }

    #[test]
    fn a_husband_back_on_one_of_the_seasons_first_dawns_is_in_time_and_not_after() {
        let mut rules = fed_rules();
        rules.needs.life.conceive_percent = 100;
        let nights = u64::from(rules.needs.life.conceive_nights);
        // Seasons of ten days, longer than the first dawns (the tests'
        // calendar has seasons of two).
        let long = || {
            let calendar = calendar().with_days_per_year(40).unwrap();
            let mut w = World::empty(calendar, Map::filled(128, 16, Terrain::Grass));
            w.set_life(true);
            w
        };
        assert!(
            nights > 1 && nights < 10,
            "a few dawns, fewer than the season's days"
        );
        let mut events = Vec::new();
        let away = Pos::new(110, 5).centre();

        let mut w = long();
        let (wife, husband) = couple_of(&mut w);
        let home = w.body(husband).unwrap().at;
        set(&mut w, husband, |b| b.at = away);
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(pregnancy(&w, wife), None, "away at the season's dawn");
        set(&mut w, husband, |b| b.at = home);
        to_dawn(&mut w, &rules, &mut events);
        assert_eq!(w.time().day_of_season(), 1);
        assert!(pregnancy(&w, wife).is_some(), "back the next dawn: in time");

        let mut w = long();
        let (wife, husband) = couple_of(&mut w);
        set(&mut w, husband, |b| b.at = away);
        to_season_start(&mut w, &rules, &mut events);
        for _ in 1..nights {
            to_dawn(&mut w, &rules, &mut events);
        }
        assert_eq!(
            w.time().day_of_season(),
            nights - 1,
            "the last of the first dawns"
        );
        events.clear();
        set(&mut w, husband, |b| b.at = home);
        to_dawn(&mut w, &rules, &mut events);
        assert_eq!(w.time().day_of_season(), nights);
        assert_eq!(
            pregnancy(&w, wife),
            None,
            "back after the first dawns: next season"
        );
    }

    #[test]
    fn a_woman_too_old_thin_nursing_or_whose_husband_is_far_or_dead_does_not_conceive() {
        let mut rules = fed_rules();
        rules.needs.life.conceive_percent = 100;
        let mut events = Vec::new();

        let mut w = world();
        let (wife, _) = couple_of(&mut w);
        set(&mut w, wife, |b| b.born_day -= 25 * 8);
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(pregnancy(&w, wife), None, "fifty: past the fertile years");

        let mut w = world();
        let (wife, _) = couple_of(&mut w);
        let reserve = w.body(wife).unwrap().condition.reserve_j(&rules.needs);
        set(&mut w, wife, |b| b.condition.deficit_j = reserve * 30 / 100);
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(pregnancy(&w, wife), None, "under 80 % of her reserve");

        let mut w = world();
        let (wife, _) = couple_of(&mut w);
        let infant = person(&mut w, 10, 5, 0, Sex::Female);
        assert!(w.carry(wife, infant));
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(pregnancy(&w, wife), None, "nursing an infant");

        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        set(&mut w, husband, |b| b.at = Pos::new(110, 5).centre());
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(
            pregnancy(&w, wife),
            None,
            "her husband a hundred tiles away"
        );

        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        w.die(husband, Death::Starved, &rules, &mut events);
        assert_eq!(w.body(wife).unwrap().spouse, None, "widowed");
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(pregnancy(&w, wife), None, "no husband");

        let mut w = world();
        let wife = person(&mut w, 10, 5, 25, Sex::Female);
        let brother = person(&mut w, 11, 5, 25, Sex::Male);
        set(&mut w, wife, |b| b.mother = Some(BodyId(900)));
        set(&mut w, brother, |b| b.mother = Some(BodyId(900)));
        to_season_start(&mut w, &rules, &mut events);
        assert_eq!(
            w.body(wife).unwrap().spouse,
            None,
            "a brother is no husband"
        );
        assert_eq!(pregnancy(&w, wife), None, "no husband, only a brother");
    }

    #[test]
    fn the_contents_share_of_wives_conceive_each_season_the_same_ones_every_run() {
        let mut rules = fed_rules();
        assert_eq!(rules.needs.life.conceive_percent, 40);
        // The season's dawn alone (the tests' seasons last two days).
        rules.needs.life.conceive_nights = 1;
        let run = || {
            let mut w = world();
            for i in 0..200 {
                let x = i % 100 + 2;
                let y = i / 100 * 6 + 2;
                let wife = person(&mut w, x, y, 25, Sex::Female);
                let husband = person(&mut w, x, y + 1, 27, Sex::Male);
                wed(&mut w, wife, husband);
            }
            to_season_start(&mut w, &rules, &mut Vec::new());
            w.bodies
                .iter()
                .filter(|b| b.pregnant.is_some())
                .map(|b| b.id)
                .collect::<BTreeSet<_>>()
        };
        let first = run();
        assert!(
            (60..=100).contains(&first.len()),
            "{} of 200 conceived, about 40 % expected",
            first.len()
        );
        assert_eq!(first, run(), "fixed by the seed, the id and the season");
    }

    #[test]
    fn a_pregnant_woman_needs_the_contents_share_more() {
        let rules = fed_rules();
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let mut events = Vec::new();
        w.step(&rules, &mut events);
        let size = w.body(wife).unwrap().condition.size_permille;
        assert_eq!(w.body(wife).unwrap().condition.burn_permille, size);
        set(&mut w, wife, |b| {
            b.pregnant = Some(Pregnancy {
                since_day: 0,
                father: husband,
            });
        });
        w.step(&rules, &mut events);
        assert_eq!(
            w.body(wife).unwrap().condition.burn_permille,
            size * 110 / 100,
            "110 % of her own need"
        );
    }

    #[test]
    fn a_weak_pregnant_woman_loses_the_child_when_the_content_says_so() {
        let mut rules = fed_rules();
        let day = w_day();
        for lose in [true, false] {
            rules.needs.life.lose_when_weak = lose;
            let mut w = world();
            let (wife, husband) = couple_of(&mut w);
            let mut events = Vec::new();
            w.step(&rules, &mut events);
            let reserve = w.body(wife).unwrap().condition.reserve_j(&rules.needs);
            let at = w.body(wife).unwrap().pos();
            // Weak (under a fifth left), not spent: she lives.
            set(&mut w, wife, |b| {
                b.pregnant = Some(Pregnancy {
                    since_day: 0,
                    father: husband,
                });
                b.condition.deficit_j = reserve * 85 / 100;
            });
            assert!(w.body(wife).unwrap().condition.is_weak(&rules.needs));
            w.advance(day, &rules, &mut events);
            assert!(w.body(wife).is_some(), "alive");
            let lost = events
                .iter()
                .any(|e| matches!(e, Event::PregnancyLost { mother, at: there } if *mother == wife && *there == at));
            assert_eq!(lost, lose, "lose_when_weak {lose}");
            assert_eq!(pregnancy(&w, wife).is_none(), lose);
        }
    }

    fn w_day() -> u64 {
        calendar().ticks_per_day()
    }

    #[test]
    fn spouses_apart_too_many_dawns_in_a_row_part_and_may_take_another() {
        let mut rules = fed_rules();
        rules.needs.life.part_after_days = 3;
        rules.needs.life.death_by_age = vec![(0, 0)];
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let suitor = person(&mut w, 14, 5, 30, Sex::Male);
        let stand = |w: &mut World, id: BodyId, x: i32| {
            set(w, id, |b| b.at = Pos::new(x, 5).centre());
        };
        let parted = |events: &[Event]| {
            events
                .iter()
                .filter_map(|e| match e {
                    Event::Parted { a, b } => Some((*a, *b)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut events = Vec::new();
        // Two dawns a hundred tiles apart, then one back: the count starts
        // again.
        stand(&mut w, husband, 110);
        w.advance(2 * w_day(), &rules, &mut events);
        assert_eq!(w.body(wife).unwrap().apart_days, 2);
        stand(&mut w, husband, 12);
        w.advance(w_day(), &rules, &mut events);
        assert_eq!(w.body(wife).unwrap().apart_days, 0);
        stand(&mut w, husband, 110);
        w.advance(2 * w_day(), &rules, &mut events);
        assert!(parted(&events).is_empty(), "two dawns apart are not enough");
        assert_eq!(w.body(wife).unwrap().spouse, Some(husband));
        // The third dawn apart in a row: they part, and by the next day
        // she has taken the man near her, at her hour.
        w.advance(w_day(), &rules, &mut events);
        assert_eq!(
            parted(&events),
            vec![(wife.min(husband), wife.max(husband))]
        );
        w.advance(w_day(), &rules, &mut events);
        assert_eq!(w.body(wife).unwrap().spouse, Some(suitor));
        assert_eq!(w.body(husband).unwrap().spouse, None, "nobody near him");
        assert_eq!(w.body(husband).unwrap().apart_days, 0);
    }

    #[test]
    fn a_child_is_born_at_the_due_day_where_its_mother_stands_and_carried_by_her() {
        let rules = fed_rules();
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let there = Pos::new(40, 7);
        set(&mut w, wife, |b| b.at = there.centre());
        let mut events = Vec::new();
        w.step(&rules, &mut events);
        let today = w.today();
        set(&mut w, wife, |b| {
            b.pregnant = Some(Pregnancy {
                since_day: today,
                father: husband,
            });
        });
        // Three seasons of two days: six days; nothing before.
        w.advance(5 * w_day(), &rules, &mut events);
        assert!(born(&events).is_empty(), "{events:?}");
        w.advance(w_day(), &rules, &mut events);
        let births = born(&events);
        assert_eq!(births.len(), 1);
        let (child, mother, father, at) = births[0];
        assert_eq!((mother, father, at), (wife, husband, there));
        let infant = w.body(child).unwrap();
        assert_eq!(infant.carried_by, Some(wife));
        assert_eq!(infant.pos(), there);
        assert_eq!((infant.mother, infant.father), (Some(wife), Some(husband)));
        assert_eq!(w.age_milli(infant), 0);
        assert!(w.is_infant(infant, &rules));
        let mum = w.body(wife).unwrap();
        assert_eq!(mum.pregnant, None);
        assert_eq!(mum.carrying_g, 3500, "born at 3.5 kg");
        assert_eq!(
            mum.condition.burn_permille,
            mum.condition.size_permille * 125 / 100,
            "nursing now"
        );
        assert_eq!(deaths(&events), Vec::new(), "she lives");
    }

    #[test]
    fn the_contents_share_of_the_newborn_are_boys_the_same_ones_every_run() {
        let mut rules = fed_rules();
        rules.needs.life.childbirth_death_percent = 0;
        rules.needs.life.death_by_age = vec![(0, 0)];
        assert_eq!(rules.needs.life.boys_percent, 51);
        let run = || {
            let mut w = world();
            let father = person(&mut w, 1, 1, 30, Sex::Male);
            for i in 0..300 {
                let mother = person(&mut w, i % 100 + 2, i / 100 * 4 + 2, 25, Sex::Female);
                set(&mut w, mother, |b| {
                    b.pregnant = Some(Pregnancy {
                        since_day: 0,
                        father,
                    });
                });
            }
            let mut events = Vec::new();
            w.advance(6 * w_day() + 1, &rules, &mut events);
            born(&events)
                .iter()
                .map(|&(child, ..)| w.body(child).unwrap().sex)
                .collect::<Vec<_>>()
        };
        let first = run();
        assert_eq!(first.len(), 300);
        let boys = first.iter().filter(|&&s| s == Sex::Male).count();
        assert!(
            (120..=186).contains(&boys),
            "{boys} boys of 300, about 51 % expected"
        );
        assert_eq!(first, run(), "fixed by the seed, the mother and the day");
    }

    #[test]
    fn a_mother_may_die_in_childbirth_and_her_infant_goes_to_the_nearest_woman() {
        let mut rules = fed_rules();
        rules.needs.life.childbirth_death_percent = 100;
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let aunt = person(&mut w, 15, 5, 30, Sex::Female);
        set(&mut w, wife, |b| {
            b.pregnant = Some(Pregnancy {
                since_day: 0,
                father: husband,
            });
        });
        let mut events = Vec::new();
        w.advance(6 * w_day() + 1, &rules, &mut events);
        let births = born(&events);
        assert_eq!(births.len(), 1);
        assert_eq!(deaths(&events), vec![(wife, Death::Childbirth)]);
        let birth = events
            .iter()
            .position(|e| matches!(e, Event::BodyBorn { .. }))
            .unwrap();
        let death = events
            .iter()
            .position(|e| matches!(e, Event::BodyDied { .. }))
            .unwrap();
        assert!(birth < death, "born, then she dies");
        assert!(w.body(wife).is_none());
        let infant = w.body(births[0].0).unwrap();
        assert_eq!(infant.carried_by, Some(aunt), "not the nearer husband");
        assert_eq!(
            infant.mother,
            Some(wife),
            "the dead mother is still its mother"
        );
        assert_ne!(w.body(husband).unwrap().spouse, Some(wife), "widowed");
    }

    #[test]
    fn the_death_curve_runs_straight_between_the_contents_points() {
        let life: Life = fed_rules().needs.life;
        for (age_milli, permille) in [
            (0, 150),
            (500, 100),
            (1000, 50),
            (4000, 50),
            (4500, 30),
            (5000, 10),
            (20_000, 10),
            (39_500, 15),
            (47_000, 20),
            (62_000, 50),
            (69_500, 100),
            (70_000, 150),
            (90_000, 150),
        ] {
            assert_eq!(
                life.death_permille_per_year(age_milli),
                permille,
                "at {age_milli} thousandths of a year"
            );
        }
        assert_eq!(
            life.death_ppm_per_day(0, 50),
            3000,
            "150 ‰ a year over 50 days"
        );
        assert_eq!(
            life.death_ppm_per_day(0, 100),
            1500,
            "a longer year, the same a year"
        );
        assert!(!life.is_fertile(15_999) && life.is_fertile(16_000));
        assert!(life.is_fertile(42_999) && !life.is_fertile(43_000));
        assert!(!life.mates(15_999) && life.mates(16_000));
        assert!(life.mates(50_999) && !life.mates(51_000));
        assert_eq!(Life::default().death_permille_per_year(70_000), 0);
    }

    #[test]
    fn bodies_draw_their_death_daily_at_their_own_hour_with_the_years_chance_over_its_days() {
        let rules = deadly_rules();
        let mut w = world();
        let men: Vec<BodyId> = (0..800)
            .map(|i| person(&mut w, i % 100 + 2, i / 100 + 2, 30, Sex::Male))
            .collect();
        let mut hours = BTreeSet::new();
        let mut dead = 0;
        for _ in 0..w_day() {
            let mut events = Vec::new();
            w.step(&rules, &mut events);
            let hour = w.time().tick_of_day;
            for (body, cause) in deaths(&events) {
                assert_eq!(cause, Death::Illness);
                assert_eq!(w.own_hour(body), hour, "at its own hour");
                hours.insert(hour);
                dead += 1;
            }
        }
        assert!(hours.len() > 1, "spread over the day, not stacked at dawn");
        assert!(
            (70..=130).contains(&dead),
            "{dead} of 800 dead in a day at 1000 ‰ a year of 8 days: 12.5 % expected"
        );
        assert_eq!(w.bodies.len(), men.len() - dead);
    }

    #[test]
    fn a_body_dies_of_illness_before_the_contents_old_age_and_of_old_age_from_it() {
        let rules = deadly_rules();
        let mut w = world();
        let young = person(&mut w, 10, 5, 30, Sex::Male);
        let old = person(&mut w, 20, 5, 70, Sex::Female);
        let mut events = Vec::new();
        w.advance(100 * w_day(), &rules, &mut events);
        let died = deaths(&events);
        assert!(died.contains(&(young, Death::Illness)), "{died:?}");
        assert!(died.contains(&(old, Death::OldAge)), "{died:?}");
    }

    #[test]
    fn a_body_takes_the_nearest_unwed_body_of_the_other_sex_barring_its_kin() {
        let rules = fed_rules();
        let mut w = world();
        let her = person(&mut w, 10, 5, 25, Sex::Female);
        let brother = person(&mut w, 11, 5, 23, Sex::Male);
        let father = person(&mut w, 12, 5, 55, Sex::Male);
        let son = person(&mut w, 13, 5, 16, Sex::Male);
        let married = person(&mut w, 14, 5, 30, Sex::Male);
        let boy = person(&mut w, 15, 5, 15, Sex::Male);
        let half_brother = person(&mut w, 16, 5, 20, Sex::Male);
        let old = person(&mut w, 17, 5, 51, Sex::Male);
        let near = person(&mut w, 20, 5, 30, Sex::Male);
        let far = person(&mut w, 30, 5, 30, Sex::Male);
        let too_far = person(&mut w, 110, 5, 30, Sex::Male);
        let mother = BodyId(900);
        set(&mut w, her, |b| {
            (b.mother, b.father) = (Some(mother), Some(father));
        });
        set(&mut w, brother, |b| {
            (b.mother, b.father) = (Some(mother), Some(father));
        });
        set(&mut w, half_brother, |b| b.father = Some(father));
        set(&mut w, son, |b| b.mother = Some(her));
        set(&mut w, married, |b| b.spouse = Some(BodyId(901)));
        let mut events = Vec::new();
        w.advance(w_day(), &rules, &mut events);
        let coupled: Vec<(BodyId, BodyId)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Coupled { a, b } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(coupled.len(), 1, "{coupled:?}");
        let (a, b) = coupled[0];
        assert_eq!(BTreeSet::from([a, b]), BTreeSet::from([her, near]));
        assert_eq!(w.body(her).unwrap().spouse, Some(near));
        assert_eq!(w.body(near).unwrap().spouse, Some(her));
        for single in [brother, father, son, boy, half_brother, old, far, too_far] {
            assert_eq!(w.body(single).unwrap().spouse, None, "{single:?}");
        }
        assert_eq!(w.body(married).unwrap().spouse, Some(BodyId(901)));
        // Widowed, she takes the next nearest; a hundred tiles is too far.
        w.die(near, Death::Starved, &rules, &mut events);
        assert_eq!(w.body(her).unwrap().spouse, None);
        w.advance(w_day(), &rules, &mut events);
        assert_eq!(w.body(her).unwrap().spouse, Some(far));
        w.die(far, Death::Starved, &rules, &mut events);
        w.advance(w_day(), &rules, &mut events);
        assert_eq!(w.body(her).unwrap().spouse, None, "{too_far:?} is too far");
    }

    #[test]
    fn with_life_off_nobody_is_born_couples_or_dies_of_anything_but_want() {
        let mut rules = deadly_rules();
        rules.needs.life.conceive_percent = 100;
        rules.needs.life.childbirth_death_percent = 100;
        let mut w = world_with(false);
        assert!(!w.life());
        let (wife, husband) = couple_of(&mut w);
        let since = Pregnancy {
            since_day: 0,
            father: husband,
        };
        set(&mut w, wife, |b| b.pregnant = Some(since));
        let bachelor = person(&mut w, 20, 5, 30, Sex::Male);
        let maid = person(&mut w, 21, 5, 25, Sex::Female);
        let old = person(&mut w, 22, 5, 80, Sex::Male);
        let mut events = Vec::new();
        // Three years: every season's draw, a dozen due days, a thousand
        // death draws at a certain year's chance.
        w.advance(24 * w_day(), &rules, &mut events);
        assert!(
            !events.iter().any(|e| matches!(
                e,
                Event::BodyBorn { .. } | Event::PregnancyLost { .. } | Event::Coupled { .. }
            )),
            "{events:?}"
        );
        assert_eq!(deaths(&events), Vec::new());
        assert_eq!(w.bodies.len(), 5);
        assert_eq!(
            pregnancy(&w, wife),
            Some(since),
            "kept, never brought to term"
        );
        assert_eq!(
            w.body(wife).unwrap().condition.burn_permille,
            w.body(wife).unwrap().condition.size_permille,
            "nor does the pregnancy weigh"
        );
        for single in [bachelor, maid, old] {
            assert_eq!(w.body(single).unwrap().spouse, None);
        }
    }

    #[test]
    fn kin_and_the_life_switch_survive_a_snapshot() {
        let mut w = world();
        let (wife, husband) = couple_of(&mut w);
        let child = person(&mut w, 10, 5, 3, Sex::Female);
        set(&mut w, child, |b| {
            (b.mother, b.father) = (Some(wife), Some(husband));
        });
        set(&mut w, wife, |b| {
            b.pregnant = Some(Pregnancy {
                since_day: 2,
                father: husband,
            });
        });
        let json = serde_json::to_string(&w).unwrap();
        let back: World = serde_json::from_str(&json).unwrap();
        assert!(back.life());
        let kin = |w: &World, id: BodyId| {
            let b = w.body(id).unwrap();
            (b.mother, b.father, b.spouse, b.pregnant)
        };
        for id in [wife, husband, child] {
            assert_eq!(kin(&back, id), kin(&w, id), "{id:?}");
        }
        assert_eq!(kin(&back, wife).3.map(|p| p.since_day), Some(2));
        // A world saved before the rule (format 15): no kin, the rule off.
        let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
        old.as_object_mut().unwrap().remove("life");
        for body in old["bodies"].as_array_mut().unwrap() {
            let body = body.as_object_mut().unwrap();
            for field in ["mother", "father", "spouse", "pregnant"] {
                body.remove(field);
            }
        }
        let before: World = serde_json::from_value(old).unwrap();
        assert!(!before.life());
        assert_eq!(kin(&before, child), (None, None, None, None));
    }

    /// The vivarium's departure (lot 3 C): `couple_all` pairs every unwed
    /// body of the mating years at once by the daily rule, kin barred, and
    /// `set_parents` gives a child its mother and father; a body is never
    /// its own parent. With life off, nobody is coupled.
    #[test]
    fn the_departure_couples_everyone_at_once_and_gives_children_parents() {
        let rules = fed_rules();
        let mut w = world();
        let wife = person(&mut w, 10, 5, 25, Sex::Female);
        let husband = person(&mut w, 12, 5, 27, Sex::Male);
        let maid = person(&mut w, 20, 5, 19, Sex::Female);
        let boy = person(&mut w, 11, 5, 7, Sex::Male);
        let mut events = Vec::new();
        w.couple_all(&rules, &mut events);
        let coupled: Vec<(BodyId, BodyId)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Coupled { a, b } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(
            coupled,
            vec![(wife, husband)],
            "one pair, the nearest; the boy too young, the maid left alone"
        );
        assert_eq!(w.body(maid).unwrap().spouse(), None);
        w.set_parents(boy, Some(wife), Some(husband));
        let child = w.body(boy).unwrap();
        assert_eq!(
            (child.mother(), child.father()),
            (Some(wife), Some(husband))
        );
        w.set_parents(boy, Some(boy), None);
        assert_eq!(w.body(boy).unwrap().mother(), None, "never its own parent");
        let mut off = world_with(false);
        let a = person(&mut off, 10, 5, 25, Sex::Female);
        person(&mut off, 12, 5, 27, Sex::Male);
        let mut events = Vec::new();
        off.couple_all(&rules, &mut events);
        assert!(events.is_empty() && off.body(a).unwrap().spouse().is_none());
    }
}
