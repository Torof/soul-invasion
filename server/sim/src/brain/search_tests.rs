//! The search for an absent kin (D56, amended 27 November 2026): noticed
//! at the fire, searched where it was last seen or said it went, never
//! where it is; the infant found by its dead mother brought home.

use super::search::SearchStep;
use super::{Act, Brains, Decision, Mind, News, Plan, Turn, place_centre, place_of, place_side};
use crate::content::{Content, Search, Sex};
use crate::map::{Map, Terrain};
use crate::{BodyId, Calendar, Death, Event, Intention, Pos, World};

const CAMP: Pos = Pos::new(5, 5);

/// A field of grass 256 tiles a side, the camp at (5, 5); a husband and
/// his wife of thirty at the camp, piloted; the search in the content.
fn couple() -> (World, Content, Brains, BodyId, BodyId) {
    let map = Map::filled(256, 256, Terrain::Grass).with_camp(5, 5);
    let mut world = World::empty(
        Calendar::new(1, 7200).unwrap().with_cycle(50, 1).unwrap(),
        map,
    );
    let husband = world.spawn_body(Pos::new(5, 5));
    let wife = world.spawn_body(Pos::new(6, 5));
    let per_year = u64::from(world.calendar().days_per_age_year());
    world.set_birth(husband, 30 * per_year, Sex::Male);
    world.set_birth(wife, 30 * per_year, Sex::Female);
    for (a, b) in [(husband, wife), (wife, husband)] {
        let index = world.index_of(a).unwrap();
        world.bodies[index].spouse = Some(b);
    }
    // The content's own needs: its ages make an infant.
    let mut rules = Content::sample();
    rules.needs = ron::from_str(include_str!("../../../../content/needs.ron")).unwrap();
    rules.needs.search = Some(Search::placeholders());
    let mut brains = Brains::new(7);
    for id in [husband, wife] {
        brains.adopt_at(&world, &rules, id, CAMP);
    }
    (world, rules, brains, husband, wife)
}

fn put(world: &mut World, rules: &Content, id: BodyId, at: Pos) {
    world.teleport(id, at, rules, &mut Vec::new());
    assert_eq!(world.body(id).unwrap().pos(), at);
}

fn next_day(world: &mut World) {
    world.tick.0 += world.calendar().ticks_per_day();
}

/// What `id` said this morning: alone, to the place of `to`.
fn says(world: &World, rules: &Content, mind: &mut Mind, to: Pos) {
    mind.plan = Some(Plan {
        day: u32::try_from(world.today()).unwrap(),
        act: Act::Alone,
        place: Some(place_of(to, place_side(rules))),
        leader: None,
        octant: 0,
        legs: 0,
        done: false,
        asked: None,
        moving: false,
    });
}

fn steps(brains: &mut Brains) -> Vec<(BodyId, BodyId, SearchStep)> {
    brains
        .news()
        .into_iter()
        .filter_map(|n| match n {
            News::Search {
                body, who, step, ..
            } => Some((body, who, step)),
            _ => None,
        })
        .collect()
}

/// The searcher's turn by day, and what it told.
fn turn(
    world: &World,
    rules: &Content,
    brains: &mut Brains,
    id: BodyId,
) -> (Option<Decision>, Vec<SearchStep>) {
    let body = world.body(id).unwrap();
    let mind = brains.minds.get_mut(&id).unwrap();
    let mut news = Vec::new();
    let turn = super::search::turn(world, rules, (id, body, body.pos()), mind, false, &mut news);
    let told = news
        .into_iter()
        .filter_map(|n| match n {
            News::Search { step, .. } => Some(step),
            _ => None,
        })
        .collect();
    match turn {
        Turn::Decide(decision) => (decision, told),
        _ => (None, told),
    }
}

fn walks_to(decision: Option<&Decision>) -> Option<Pos> {
    match decision?.intention {
        Intention::MoveTo { target, .. } => Some(target),
        _ => None,
    }
}

/// One tick of the world with `intention` asked.
fn ask(world: &mut World, rules: &Content, intention: Intention) -> Vec<Event> {
    let mut events = Vec::new();
    world.queue(intention);
    world.step(rules, &mut events);
    events
}

#[test]
fn without_the_content_s_section_nobody_notes_nor_notices() {
    let (mut world, mut rules, mut brains, husband, wife) = couple();
    rules.needs.search = None;
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    assert!(brains.minds[&husband].kin.is_empty());
    assert!(brains.minds[&husband].search.is_none());
    assert_eq!(steps(&mut brains), []);
}

#[test]
fn it_goes_where_she_said_she_went_not_where_she_is() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    let said = Pos::new(150, 5);
    says(&world, &rules, brains.minds.get_mut(&wife).unwrap(), said);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    assert_eq!(steps(&mut brains), [], "both at the fire");
    // She is somewhere else altogether, out of anyone's sight.
    let truly = Pos::new(5, 200);
    put(&mut world, &rules, wife, truly);
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    let told = steps(&mut brains);
    assert!(matches!(
        told[0],
        (h, w, SearchStep::Noticed { nights: 1, said: true, kin: 1, stayed: None, .. }) if h == husband && w == wife
    ));
    assert!(matches!(told[1], (h, w, SearchStep::Begun { .. }) if h == husband && w == wife));
    brains.watch_searches(&world, &rules);
    let (decision, _) = turn(&world, &rules, &mut brains, husband);
    let to = place_centre(place_of(said, place_side(&rules)), place_side(&rules));
    assert_eq!(walks_to(decision.as_ref()), Some(to), "the place she said");
    assert_ne!(walks_to(decision.as_ref()), Some(truly));
    // One search an absence: the next dawn begins none.
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    assert_eq!(steps(&mut brains), []);
}

#[test]
fn with_no_place_said_it_goes_to_the_last_place_it_saw_her_not_where_she_is() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    // By day he sees her within his sight, there.
    let seen = Pos::new(50, 5);
    put(&mut world, &rules, wife, seen);
    let mind = brains.minds.get_mut(&husband).unwrap();
    super::search::see_kin(&world, &rules, CAMP, mind);
    // Then she goes on, out of his sight: he does not know where.
    let truly = Pos::new(120, 200);
    put(&mut world, &rules, wife, truly);
    super::search::see_kin(&world, &rules, CAMP, mind);
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    let (decision, _) = turn(&world, &rules, &mut brains, husband);
    assert_eq!(walks_to(decision.as_ref()), Some(seen), "where he saw her");
    // Standing there and seeing nothing of her, the search is over.
    put(&mut world, &rules, husband, seen);
    brains.watch_searches(&world, &rules);
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert!(decision.is_none());
    assert_eq!(told, [SearchStep::NotFound { reached: true }]);
    assert!(brains.minds[&husband].search.is_none());
}

#[test]
fn one_last_seen_at_the_camp_with_no_place_said_is_noticed_but_not_searched() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    put(&mut world, &rules, wife, Pos::new(200, 200));
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    let told = steps(&mut brains);
    assert_eq!(
        told,
        [(
            husband,
            wife,
            SearchStep::Noticed {
                tie: super::Tie::Spouse,
                nights: 1,
                said: false,
                kin: 1,
                stayed: Some(super::Stayed::Camp),
            }
        )]
    );
    assert!(brains.minds[&husband].search.is_none(), "nowhere to go to");
}

#[test]
fn seeing_her_alive_on_its_way_ends_the_search_with_nothing_more() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    says(
        &world,
        &rules,
        brains.minds.get_mut(&wife).unwrap(),
        Pos::new(150, 5),
    );
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    put(&mut world, &rules, wife, Pos::new(100, 5));
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband])]);
    brains.watch_searches(&world, &rules);
    assert!(
        brains.minds[&husband].search.is_some(),
        "out of his sight from the camp: he sets off"
    );
    put(&mut world, &rules, husband, Pos::new(60, 5));
    brains.minds.get_mut(&husband).unwrap().due = false;
    brains.watch_searches(&world, &rules);
    assert!(brains.minds[&husband].due, "seen: a thought");
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert!(decision.is_none());
    assert_eq!(told, [SearchStep::FoundAlive]);
    assert!(brains.minds[&husband].search.is_none());
}

#[test]
fn by_her_body_it_takes_up_her_infant_carries_it_home_and_gives_it_to_a_woman() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    let aunt = world.spawn_body(Pos::new(7, 7));
    let infant = world.spawn_body(Pos::new(6, 6));
    let per_year = u64::from(world.calendar().days_per_age_year());
    world.set_birth(aunt, 40 * per_year, Sex::Female);
    world.set_birth(infant, 20, Sex::Female);
    world.set_parents(infant, Some(wife), Some(husband));
    assert!(world.carry(wife, infant));
    brains.adopt_at(&world, &rules, aunt, CAMP);
    let said = Pos::new(150, 5);
    says(&world, &rules, brains.minds.get_mut(&wife).unwrap(), said);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife, aunt])]);
    // She dies short of the place she said, her infant on her: too far
    // from any woman, it is left there.
    let died = Pos::new(120, 8);
    put(&mut world, &rules, wife, died);
    world.step(&rules, &mut Vec::new());
    let mut events = Vec::new();
    world.die(wife, Death::Dehydrated, &rules, &mut events);
    brains.search_events(&events, &rules);
    assert_eq!(world.body(infant).unwrap().carried_by(), None);
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, aunt])]);
    let told = steps(&mut brains);
    assert_eq!(
        told.iter()
            .filter(|t| matches!(t.2, SearchStep::Begun { .. }))
            .map(|t| t.0)
            .collect::<Vec<_>>(),
        [husband],
        "her husband goes, nobody else"
    );
    // From the camp he sees nothing of her: he walks to the place said.
    brains.watch_searches(&world, &rules);
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert_eq!(told, []);
    assert!(walks_to(decision.as_ref()).is_some_and(|to| to.x > 120));
    // On his way her body comes within his sight: he goes to the infant.
    put(&mut world, &rules, husband, Pos::new(90, 5));
    brains.watch_searches(&world, &rules);
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert_eq!(told, [SearchStep::FoundDead]);
    assert_eq!(walks_to(decision.as_ref()), Some(died));
    // Beside it, he takes it up.
    put(&mut world, &rules, husband, Pos::new(121, 8));
    let (decision, _) = turn(&world, &rules, &mut brains, husband);
    let take = decision.unwrap().intention;
    assert_eq!(
        take,
        Intention::Carry {
            body: husband,
            infant,
            carrier: husband
        }
    );
    ask(&mut world, &rules, take);
    assert_eq!(world.body(infant).unwrap().carried_by(), Some(husband));
    // Carrying it, his day is to go home.
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert!(decision.is_none());
    assert_eq!(told, [SearchStep::InfantTaken { infant }]);
    assert_eq!(
        brains.minds[&husband].out.map(|o| o.1),
        Some(super::TripChoice::Home)
    );
    // Home, a man: he gives it to the woman he sees there.
    put(&mut world, &rules, husband, Pos::new(7, 6));
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert_eq!(told, [SearchStep::InfantHome { infant }]);
    let give = decision.unwrap().intention;
    assert_eq!(
        give,
        Intention::Carry {
            body: husband,
            infant,
            carrier: aunt
        }
    );
    ask(&mut world, &rules, give);
    assert_eq!(world.body(infant).unwrap().carried_by(), Some(aunt));
    let (decision, told) = turn(&world, &rules, &mut brains, husband);
    assert!(decision.is_none());
    assert_eq!(told, [SearchStep::InfantGiven { infant, to: aunt }]);
    assert!(brains.minds[&husband].search.is_none());
    // Nursed by her from now on, by the world's own rule: no thirst.
    world.step(&rules, &mut Vec::new());
    assert!(world.body(infant).unwrap().condition().thirst.is_empty());
}

#[test]
fn a_woman_who_finds_it_keeps_and_nurses_it() {
    let (mut world, rules, mut brains, husband, wife) = couple();
    let infant = world.spawn_body(Pos::new(6, 6));
    world.set_birth(infant, 20, Sex::Female);
    // The wife searches for her husband, who carried their infant.
    world.set_parents(infant, Some(BodyId(99)), Some(husband));
    assert!(world.carry(husband, infant));
    let said = Pos::new(150, 5);
    says(
        &world,
        &rules,
        brains.minds.get_mut(&husband).unwrap(),
        said,
    );
    brains.fire_absent(&world, &rules, &[(CAMP, vec![husband, wife])]);
    put(&mut world, &rules, husband, Pos::new(150, 6));
    world.step(&rules, &mut Vec::new());
    let mut events = Vec::new();
    world.die(husband, Death::Dehydrated, &rules, &mut events);
    brains.search_events(&events, &rules);
    // Out of the content's reach of adoption, the infant lies alone.
    assert_eq!(world.body(infant).unwrap().carried_by(), None);
    next_day(&mut world);
    brains.fire_absent(&world, &rules, &[(CAMP, vec![wife])]);
    put(&mut world, &rules, wife, Pos::new(149, 6));
    brains.watch_searches(&world, &rules);
    let (decision, told) = turn(&world, &rules, &mut brains, wife);
    assert_eq!(told, [SearchStep::FoundDead]);
    ask(&mut world, &rules, decision.unwrap().intention);
    assert_eq!(world.body(infant).unwrap().carried_by(), Some(wife));
    turn(&world, &rules, &mut brains, wife);
    put(&mut world, &rules, wife, CAMP);
    let (decision, told) = turn(&world, &rules, &mut brains, wife);
    assert!(decision.is_none(), "she has milk: nobody to give it to");
    assert_eq!(told, [SearchStep::InfantHome { infant }]);
    assert!(brains.minds[&wife].search.is_none());
    assert_eq!(world.body(infant).unwrap().carried_by(), Some(wife));
}

#[test]
fn the_world_refuses_an_infant_out_of_reach_or_in_another_s_arms() {
    let (mut world, rules, _, husband, wife) = couple();
    let infant = world.spawn_body(Pos::new(20, 20));
    world.set_birth(infant, 20, Sex::Male);
    let take = |by| Intention::Carry {
        body: by,
        infant,
        carrier: by,
    };
    ask(&mut world, &rules, take(husband));
    assert_eq!(world.body(infant).unwrap().carried_by(), None, "too far");
    assert!(world.carry(wife, infant));
    world.step(&rules, &mut Vec::new());
    ask(&mut world, &rules, take(husband));
    assert_eq!(
        world.body(infant).unwrap().carried_by(),
        Some(wife),
        "not taken from one who carries it"
    );
    // She gives it to him, beside her.
    ask(
        &mut world,
        &rules,
        Intention::Carry {
            body: wife,
            infant,
            carrier: husband,
        },
    );
    assert_eq!(world.body(infant).unwrap().carried_by(), Some(husband));
    // A grown body is no infant to carry.
    ask(
        &mut world,
        &rules,
        Intention::Carry {
            body: husband,
            infant: wife,
            carrier: husband,
        },
    );
    assert_eq!(world.body(wife).unwrap().carried_by(), None);
}
