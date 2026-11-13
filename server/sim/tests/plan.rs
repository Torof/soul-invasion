//! The atlas's continent (`sim::plan`): born the same from the same seed,
//! water that never runs uphill, lakes filled to their outlet, rivers
//! that all reach the sea. On small continents, so the tests stay quick.

#![allow(clippy::many_single_char_names)]

use sim::plan::{Continent, Region, region};

/// Cells a side of the test continents.
const SIDE: usize = 128;

fn continent(seed: u64) -> Continent {
    Continent::with(seed, SIDE, None)
}

#[test]
fn the_same_seed_gives_the_same_continent() {
    let a = continent(3);
    assert_eq!(a.fingerprint(), continent(3).fingerprint());
    assert_ne!(a.fingerprint(), continent(4).fingerprint());
    let site = region::site(&a);
    let (r1, r2) = (Region::around(&a, site, 256), Region::around(&a, site, 256));
    assert_eq!(r1.height, r2.height);
    assert_eq!(r1.water, r2.water);
}

#[test]
fn water_never_runs_uphill() {
    for seed in [1, 2, 5] {
        let c = continent(seed);
        for i in 0..c.grid.len() {
            if c.sea[i] {
                continue;
            }
            let r = c.receiver[i] as usize;
            assert_ne!(r, i, "seed {seed}: land cell {i} drains somewhere");
            let ((x, y), (rx, ry)) = (c.grid.xy(i), c.grid.xy(r));
            assert!(
                (x - rx).abs() <= 1 && (y - ry).abs() <= 1,
                "seed {seed}: a neighbour"
            );
            assert!(
                c.surface[r] <= c.surface[i],
                "seed {seed}: cell {i} runs uphill to {r}"
            );
        }
    }
}

#[test]
fn lakes_are_filled_to_their_outlet() {
    for seed in [1, 2, 5] {
        let c = continent(seed);
        let n = c.grid.len();
        let filled = |i: usize| !c.sea[i] && c.surface[i] > c.height[i];
        let mut seen = vec![false; n];
        let mut lakes = 0;
        for start in 0..n {
            if seen[start] || !filled(start) {
                continue;
            }
            // One hollow: its cells all at one level.
            let level = c.surface[start];
            let mut stack = vec![start];
            let mut rim = Vec::new();
            seen[start] = true;
            while let Some(i) = stack.pop() {
                assert_eq!(c.surface[i], level, "seed {seed}: one level a hollow");
                let (x, y) = c.grid.xy(i);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if !c.grid.inside(x + dx, y + dy) || (dx, dy) == (0, 0) {
                            continue;
                        }
                        let j = c.grid.index(x + dx, y + dy);
                        if filled(j) {
                            if !seen[j] {
                                seen[j] = true;
                                stack.push(j);
                            }
                        } else {
                            rim.push(c.surface[j]);
                        }
                    }
                }
            }
            let lowest = *rim.iter().min().expect("a hollow has a rim");
            assert_eq!(
                lowest, level,
                "seed {seed}: a hollow fills to its lowest rim, its outlet"
            );
            lakes += 1;
        }
        assert!(lakes > 0, "seed {seed}: some hollow to check");
    }
}

#[test]
fn every_river_reaches_the_sea() {
    for seed in [1, 2, 5] {
        let c = continent(seed);
        let n = c.grid.len();
        let mut rivers = 0;
        for start in 0..n {
            if c.sea[start] || c.cubic_metres(start) < 1 {
                continue;
            }
            rivers += 1;
            let mut i = start;
            let mut steps = 0;
            while !c.sea[i] {
                i = c.receiver[i] as usize;
                steps += 1;
                assert!(steps <= n, "seed {seed}: the river from {start} never ends");
            }
        }
        assert!(rivers > 0, "seed {seed}: some river to follow");
    }
}

#[test]
fn the_discharge_is_kept_down_the_network() {
    let c = continent(2);
    for i in 0..c.grid.len() {
        if c.sea[i] {
            continue;
        }
        let r = c.receiver[i] as usize;
        if !c.sea[r] {
            assert!(
                c.discharge[r] >= c.discharge[i],
                "a river never loses water downstream"
            );
        }
    }
}
