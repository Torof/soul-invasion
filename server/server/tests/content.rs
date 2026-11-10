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

#[test]
fn the_valley_drops_dead_wood_and_pebbles_where_they_belong() {
    let content = repository_content();
    let world = World::with_valley(Calendar::default(), &content, 768);
    // Made per chunk, none stored until touched.
    assert_eq!(world.items(), []);
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
    // Nothing ready-made lies about: berries, fibres and flint will come
    // from the bushes, the plants and the rock.
    assert!(
        items
            .iter()
            .all(|i| matches!(i.material(), "branch" | "pebble")),
        "only dead wood and pebbles lie on the ground"
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
