//! The repository's content files load, validate and fill the valley.
//! Runs on every commit (D27): an invalid file fails the build.

use std::path::Path;

use sim::{Calendar, Terrain, World};

fn repository_content() -> sim::Content {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    server::content::load(&dir).expect("the repository's content is valid")
}

#[test]
fn the_content_directory_loads_and_validates() {
    let content = repository_content();
    assert!(
        content.materials.len() >= 9,
        "the first version's materials"
    );
    for material in &content.materials {
        assert!(
            material.name.chars().next().is_some_and(char::is_uppercase),
            "{}: names are shown to players, capitalised",
            material.id
        );
    }
    for name in ["flint", "branch", "fibres", "berries", "meat"] {
        assert!(content.material(name).is_some(), "{name} is missing");
    }
}

/// The long durations are written in seasons (D09, the clocks audit's
/// principle D, 25 November 2026) and give calendar v1's days exactly as
/// the days they replaced: nuts, grain, peas and seeds keep 64 days,
/// walnuts six seasons (96, from 90), folded bark lasts 64, the hearth,
/// the habits and the lessons hold 64; under another cycle they follow it.
#[test]
fn the_long_durations_in_seasons_give_calendar_v1_its_days() {
    let content = repository_content();
    let v1 = Calendar::DEFAULT_DAYS_PER_CYCLE;
    assert_eq!(v1, 64);
    let kept = |id: &str, per_cycle: u32| {
        content
            .material(id)
            .and_then(|m| m.food)
            .map(|food| food.days_kept(per_cycle))
    };
    for id in ["hazelnuts", "peas", "grain", "flax_seeds", "chestnuts"] {
        assert_eq!(kept(id, v1), Some(64), "{id}");
        assert_eq!(kept(id, 50), Some(50), "{id} follows the cycle");
    }
    assert_eq!(kept("walnuts", v1), Some(96));
    assert_eq!(kept("berries", v1), Some(3), "a short keeping stays days");
    let bark = content.material("folded_birch_bark").unwrap();
    assert_eq!(bark.days_worn(v1), 32);
    let needs = &content.needs;
    assert_eq!(needs.fire.hearth_nights(v1), 64);
    assert_eq!(needs.decisions.habit_days(v1), 64);
    assert_eq!(needs.trials.lesson_days(v1), 64);
    assert_eq!(needs.life.part_after_days, 12, "an absence stays in days");
}

/// Food weighs its energy per kilogram now (D15, D35): a piece of each
/// food gives the kilojoules its single piece gave before, and a meal is
/// a positive number of grams.
#[test]
fn energy_per_kilogram_gives_a_piece_what_it_gave_before() {
    let content = repository_content();
    let before = [
        ("berries", 250),
        ("meat", 1200),
        // The crops of lot 2 (D45): a handful of each.
        ("hazelnuts", 1150),
        ("blackberries", 180),
        // Lot 2's whole list (nourriture.md, 16 November 2026).
        ("nettle_leaves", 170),
        ("garlic_leaves", 150),
        ("dandelion_leaves", 190),
        ("watercress", 45),
        ("linden_leaves", 200),
        ("pine_bark", 500),
        ("linden_bark", 500),
        ("strawberries", 130),
        ("raspberries", 220),
        ("peas", 1400),
        ("grain", 1400),
        ("chanterelles", 130),
        ("crab_apples", 200),
        ("haws", 380),
        ("rose_hips", 650),
        ("porcini", 120),
        ("flax_seeds", 2200),
        ("cattail_root", 300),
        ("oyster_mushrooms", 140),
        // More of the valley (16 November 2026).
        ("asparagus_shoots", 85),
        ("hop_shoots", 100),
        ("sorrel_leaves", 90),
        ("lambs_lettuce", 90),
        ("chickweed", 80),
        ("purslane", 80),
        ("fat_hen_leaves", 180),
        ("red_currants", 230),
        ("blackcurrants", 260),
        ("gooseberries", 190),
        ("bilberries", 240),
        ("cornels", 280),
        ("grapes", 290),
        ("arbutus_berries", 330),
        ("sloes", 200),
        ("wild_onions", 170),
        ("silverweed_root", 300),
        ("arrowhead_tubers", 400),
        ("water_chestnuts", 400),
        // The fruit and nut trees (16 November 2026).
        ("cherries", 260),
        ("mulberries", 180),
        ("figs", 310),
        ("pears", 240),
        ("walnuts", 1150),
        ("chestnuts", 800),
    ];
    // Hazelnuts and walnuts weighed in their shells since 24 November
    // 2026 (11,500 kJ a kilogram, not the kernel's).
    for (id, kj) in before {
        let material = content.material(id).expect(id);
        let food = material.food.expect("food");
        assert_eq!(food.energy_kj(material.mass_g()), kj, "{id}");
    }
    let foods = content.materials.iter().filter(|m| m.food.is_some());
    assert_eq!(foods.count(), before.len(), "a new food: add it here");
    assert!(content.needs.meal_g > 0);
    // Every spawn rule knows the grams of the piece it drops.
    for spawn in &content.spawns {
        let material = content.material(&spawn.material).expect("validated");
        assert_eq!(spawn.piece_g, material.mass_g(), "{}", spawn.material);
    }
}

#[test]
fn the_valley_drops_dead_wood_and_pebbles_where_they_belong() {
    let content = repository_content();
    let world = World::with_valley(Calendar::default(), &content, 768);
    // Made per chunk, none stored until touched.
    assert_eq!(world.items().count(), 0);
    let items: Vec<_> = (0..24)
        .flat_map(|y| (0..24).map(move |x| sim::ChunkPos::new(x, y)))
        .flat_map(|c| world.items_in(c))
        .collect();
    assert!(items.len() > 1000, "{} items", items.len());
    assert!(
        items
            .iter()
            .all(|i| i.pos().and_then(|p| world.map().get(p)) != Some(Terrain::Water)),
        "nothing lies in the water"
    );
    assert!(items.iter().any(|i| i.material() == "branch"));
    assert!(items.iter().any(|i| i.material() == "pebble"));
    // Flint lies on the banks and by the rock (D57).
    assert!(items.iter().any(|i| i.material() == "flint"));
    // Nothing else ready-made lies about: berries and fibres will come
    // from the bushes and the plants.
    assert!(
        items
            .iter()
            .all(|i| matches!(i.material(), "branch" | "pebble" | "flint")),
        "only dead wood, pebbles and flint lie on the ground"
    );
}

#[test]
fn a_missing_directory_is_a_clear_error() {
    let err = server::content::load(Path::new("/nowhere/at/all")).unwrap_err();
    assert!(
        matches!(err, server::content::LoadError::Read { .. }),
        "{err}"
    );
    let err = server::content::find_dir(None);
    // From the crate directory, ../../content exists.
    assert!(err.is_ok(), "{err:?}");
}
