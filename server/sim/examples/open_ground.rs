//! The open ground seen from above: the vivarium's zone with every tree
//! as a dot, and the trees a hectare on each biome's open ground. For
//! tuning `content/open_ground.ron` by eye (15 November 2026).
//!
//! `cargo run --release -p sim --example open_ground -- <OUT_DIR>` writes
//! `zone.ppm` (one pixel a tile: water, forest, open ground by biome,
//! trees in black) and `zoom.ppm` (a 400 m square of grassland at the
//! forest's edge, two pixels a tile), and prints the counts. PPM files
//! convert to PNG with `ImageMagick` (`convert zone.ppm zone.png`).

#![allow(
    clippy::cast_precision_loss,
    reason = "small counts and coordinates, far below the float precision limit"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use sim::content::Species;
use sim::plan::Biome;
use sim::trees::{Tree, TreeGrid};
use sim::woods::Habitat;
use sim::{CHUNK_SIZE, ChunkPos, Map, OpenGround, Pos, Terrain};

type Rgb = [u8; 3];

/// A biome's trees on its open ground, and its meadow groups by cell.
type Groups = (u64, BTreeMap<(ChunkPos, (i8, i8)), u64>);

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let species: Vec<Species> =
        ron::from_str(include_str!("../../../content/species.ron")).expect("species.ron");
    let open: OpenGround =
        ron::from_str(include_str!("../../../content/open_ground.ron")).expect("open_ground.ron");
    let map = Map::vivarium();
    let seed = sim::plan::VIVARIUM_SEED;
    let side = map.width();
    let chunks = i32::try_from(side.div_ceil(CHUNK_SIZE)).expect("few chunks");
    let mut trees: Vec<Tree> = Vec::new();
    for cy in 0..chunks {
        for cx in 0..chunks {
            trees.extend(sim::trees::trees_in(
                &map,
                &species,
                &open,
                seed,
                ChunkPos::new(cx, cy),
            ));
        }
    }
    counts(&map, &trees);
    let n = usize::try_from(side).expect("small");
    let mut img: Vec<Rgb> = (0..n * n)
        .map(|i| {
            let p = Pos::new(
                i32::try_from(i % n).expect("small"),
                i32::try_from(i / n).expect("small"),
            );
            ground(&map, p)
        })
        .collect();
    for t in &trees {
        let p = t.at.tile();
        let (x, y) = (
            usize::try_from(p.x).expect("on the map"),
            usize::try_from(p.y).expect("on the map"),
        );
        img[y * n + x] = [0, 0, 0];
    }
    std::fs::write(out.join("zone.ppm"), ppm(n, n, &img)).expect("write zone.ppm");
    // The zoom: 400 m around the zone's forest edge, two pixels a tile.
    let (ex, ey) = sim::plan::ZONE_2.tile(sim::plan::ZONE_2.forest_edge);
    let (x0, y0) = (
        usize::try_from(ex.saturating_sub(200)).expect("small"),
        usize::try_from(ey.saturating_sub(200)).expect("small"),
    );
    let z = 400.min(n - x0).min(n - y0);
    let mut zoom = vec![[0_u8; 3]; z * z * 4];
    for y in 0..z {
        for x in 0..z {
            let c = img[(y0 + y) * n + x0 + x];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                zoom[(2 * y + dy) * 2 * z + 2 * x + dx] = c;
            }
        }
    }
    std::fs::write(out.join("zoom.ppm"), ppm(2 * z, 2 * z, &zoom)).expect("write zoom.ppm");
}

/// Trees and groves on each biome's open ground, printed.
fn counts(map: &Map, trees: &[Tree]) {
    let side = i32::try_from(map.width()).expect("small");
    let mut tiles: BTreeMap<Biome, u64> = BTreeMap::new();
    for y in 0..side {
        for x in 0..side {
            let p = Pos::new(x, y);
            if let Some(b) = map.biome(p).filter(|_| open_ground(map, p)) {
                *tiles.entry(b).or_default() += 1;
            }
        }
    }
    let mut on: BTreeMap<Biome, Groups> = BTreeMap::new();
    for t in trees {
        let p = t.at.tile();
        if let Some(b) = map.biome(p).filter(|_| open_ground(map, p)) {
            let entry = on.entry(b).or_default();
            entry.0 += 1;
            if t.key.grid == TreeGrid::Meadow {
                *entry.1.entry((p.chunk(), t.key.cell)).or_default() += 1;
            }
        }
    }
    println!("{} trees on the zone", trees.len());
    for (b, n) in &tiles {
        let (count, groups) = on.get(b).cloned().unwrap_or_default();
        let ha = *n as f64 / 10_000.0;
        let groves = groups.values().filter(|k| **k >= 2).count();
        let lone = groups.values().filter(|k| **k == 1).count();
        println!(
            "{b:?}: {ha:.1} ha of open ground, {count} trees ({:.2} a hectare), {groves} groves, {lone} lone trees",
            count as f64 / ha
        );
    }
}

/// Open ground: a meadow, no wood of the valley's on it.
fn open_ground(map: &Map, p: Pos) -> bool {
    map.habitat(p) == Habitat::Meadow && map.get(p) == Some(Terrain::Grass)
}

fn ground(map: &Map, p: Pos) -> Rgb {
    match map.get(p) {
        Some(Terrain::Water | Terrain::Ford | Terrain::Brook) => [70, 120, 200],
        Some(Terrain::Rock) | None => [120, 120, 120],
        Some(Terrain::Forest) => [60, 110, 50],
        Some(Terrain::Grass) => match map.biome(p) {
            Some(Biome::Grassland) => [190, 220, 140],
            Some(Biome::WoodedSteppe) => [200, 200, 120],
            Some(Biome::Steppe) => [220, 210, 150],
            Some(Biome::Marsh) => [150, 190, 170],
            Some(b) if b.is_forest() => [140, 190, 110],
            _ => [210, 210, 190],
        },
    }
}

fn ppm(width: usize, height: usize, pixels: &[Rgb]) -> Vec<u8> {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    out.extend(pixels.iter().flatten());
    out
}
