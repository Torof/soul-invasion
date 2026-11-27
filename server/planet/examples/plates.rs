//! A probe of the plates: how many there are by size through the history
//! and today, how long they live, how they are born and end, and where
//! the small ones sit (wedged between several plates, or enclosed in one
//! plate's interior). Also the land beyond 60° and the largest land.
//!
//! ```text
//! cargo run --release -p planet --example plates -- settings.ron 14,15 [v]
//! ```

#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::format_push_string,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a measuring tool: one long count, printed as rounded shares"
)]

use planet::history::MAXP;
use planet::{CELLS, Planet, Settings};

/// The size classes, km²: micro, small, medium, large.
const CLASS_KM2: [i64; 3] = [500_000, 2_000_000, 10_000_000];

fn class_of(km2: i64) -> usize {
    CLASS_KM2.iter().filter(|&&k| km2 >= k).count()
}

/// A connected part of a plate at a step.
struct Part {
    id: u8,
    km2: i64,
    /// The other plates it touches.
    around: Vec<u8>,
}

fn parts(world: &Planet, plate: &[u8]) -> Vec<Part> {
    let s = &world.sphere;
    let mut seen = vec![false; CELLS];
    let mut out = Vec::new();
    for start in 0..CELLS {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let id = plate[start];
        let (mut km2, mut around, mut stack) = (0, Vec::new(), vec![start]);
        while let Some(c) = stack.pop() {
            km2 += i64::from(s.area_km2[c]);
            for &(n, _) in &s.links[c] {
                let n = n as usize;
                if plate[n] != id {
                    if !around.contains(&plate[n]) {
                        around.push(plate[n]);
                    }
                } else if !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        out.push(Part { id, km2, around });
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("a settings file");
    let settings: Settings = ron::from_str(&text).expect("settings");
    let verbose = args.len() > 3;
    // Today, by class: plates; over the history: plate-steps.
    let (mut today, mut through) = ([0_u64; 4], [0_u64; 4]);
    // Small parts (under 2 M km²): today and through, enclosed and wedged;
    // and the parts that are a second part of a plate.
    let (mut enclosed, mut wedged, mut second) = ([0_u64; 2], [0_u64; 2], [0_u64; 2]);
    // Plates that ended, by life in steps: 1, 2-3, 4-9, 10 and more; of
    // those never over 2 M km².
    let (mut lives, mut lives_small) = ([0_u64; 4], [0_u64; 4]);
    let (mut born, mut ended) = ([0_u64; 4], [0_u64; 2]);
    let (mut steps, mut seeds, mut polar_over) = (0_u64, 0_u64, 0_u64);
    // Captures: parts, whole plates; plates born of a cut-off part.
    let mut taken = [0_u64; 3];
    let (mut polar, mut biggest) = (Vec::new(), Vec::new());
    for seed in args[2]
        .split(',')
        .map(|s| s.parse::<u64>().expect("a seed"))
    {
        let world = Planet::with(seed, &settings);
        let h = &world.history;
        let last = h.last();
        seeds += 1;
        taken[0] += h.captures.iter().filter(|c| !c.whole).count() as u64;
        taken[1] += h.captures.iter().filter(|c| c.whole).count() as u64;
        taken[2] += h.cut_off as u64;
        if verbose {
            for c in h.captures.iter().filter(|c| c.whole || c.km2 > 200_000) {
                println!("seed {seed} capture: {c:?}");
            }
        }
        let mut first_t = [usize::MAX; MAXP];
        let mut last_t = [0_usize; MAXP];
        let mut most = [0_i64; MAXP];
        let mut line = String::new();
        for t in 0..=last {
            let ps = parts(&world, &h.plate[t]);
            let mut whole = [0_i64; MAXP];
            let mut largest = [0_i64; MAXP];
            for p in &ps {
                whole[p.id as usize] += p.km2;
                largest[p.id as usize] = largest[p.id as usize].max(p.km2);
            }
            let mut now = [0_u64; 4];
            for id in 0..MAXP {
                if whole[id] == 0 {
                    continue;
                }
                first_t[id] = first_t[id].min(t);
                last_t[id] = t;
                most[id] = most[id].max(whole[id]);
                now[class_of(whole[id])] += 1;
            }
            let w = usize::from(t < last);
            for p in ps.iter().filter(|p| p.km2 < CLASS_KM2[1]) {
                if p.around.len() <= 1 {
                    enclosed[w] += 1;
                } else {
                    wedged[w] += 1;
                }
                if p.km2 < largest[p.id as usize] {
                    second[w] += 1;
                }
                if verbose && t == last {
                    println!(
                        "seed {seed} today: part of plate {} {} k km2, touches {:?}{}",
                        p.id,
                        p.km2 / 1000,
                        p.around,
                        if p.km2 < largest[p.id as usize] {
                            ", a second part"
                        } else {
                            ""
                        },
                    );
                }
            }
            for k in 0..4 {
                through[k] += now[k];
                if t == last {
                    today[k] += now[k];
                }
            }
            steps += 1;
            line.push_str(&format!("{}/{}/{}/{} ", now[0], now[1], now[2], now[3]));
        }
        if verbose {
            println!("seed {seed} plates micro/small/medium/large by step: {line}");
        }
        let step_my = settings.step_my;
        let at = |my: u16| last - (i64::from(my) / step_my) as usize;
        for id in 0..MAXP {
            if first_t[id] == usize::MAX {
                continue;
            }
            let how = if first_t[id] == 0 {
                0
            } else if h.rifts.iter().any(|r| usize::from(r.new) == id) {
                1
            } else if h.escapes.iter().any(|e| usize::from(e.block) == id) {
                2
            } else {
                3
            };
            born[how] += 1;
            if last_t[id] == last {
                continue;
            }
            let life = last_t[id] - first_t[id] + 1;
            let k = match life {
                1 => 0,
                2..=3 => 1,
                4..=9 => 2,
                _ => 3,
            };
            lives[k] += 1;
            if most[id] < CLASS_KM2[1] {
                lives_small[k] += 1;
            }
            let welded = h
                .welds
                .iter()
                .any(|w| usize::from(w.absorbed) == id && at(w.my) > last_t[id]);
            ended[usize::from(!welded)] += 1;
            if verbose {
                println!(
                    "seed {seed} plate {id}: steps {}..{} most {} k km2 born {} ended {}",
                    first_t[id],
                    last_t[id],
                    most[id] / 1000,
                    ["start", "rift", "escape", "opening"][how],
                    if welded { "weld" } else { "swallowed or held" },
                );
            }
        }
        let beyond: i64 = (0..CELLS)
            .filter(|&c| {
                world.elevation[c] > 0 && planet::latitude(world.sphere.centre[c]).abs() > 600
            })
            .map(|c| i64::from(world.sphere.area_km2[c]))
            .sum();
        polar_over += u64::from(beyond > 6_000_000);
        polar.push(beyond / 100_000);
        biggest.push(world.lands.first().map_or(0, |l| l.area_km2 / 100_000));
        if verbose {
            println!(
                "seed {seed}: land beyond 60 deg {} k km2, largest land {} k km2",
                beyond / 1000,
                world.lands.first().map_or(0, |l| l.area_km2 / 1000),
            );
        }
    }
    let per = |a: u64, b: u64| a as f64 / b.max(1) as f64;
    println!(
        "plates today per seed, micro <0.5 / small 0.5-2 / medium 2-10 / large >10 M km2: {:.1} / {:.1} / {:.1} / {:.1}",
        per(today[0], seeds),
        per(today[1], seeds),
        per(today[2], seeds),
        per(today[3], seeds),
    );
    println!(
        "plates per step through the history: {:.1} / {:.1} / {:.1} / {:.1}",
        per(through[0], steps),
        per(through[1], steps),
        per(through[2], steps),
        per(through[3], steps),
    );
    println!(
        "parts under 2 M km2 per step, history: enclosed in one plate {:.2}, wedged {:.2}, of which second parts of a plate {:.2}; today per seed: enclosed {:.2}, wedged {:.2}, second parts {:.2}",
        per(enclosed[1], steps - seeds),
        per(wedged[1], steps - seeds),
        per(second[1], steps - seeds),
        per(enclosed[0], seeds),
        per(wedged[0], seeds),
        per(second[0], seeds),
    );
    println!(
        "plates ended per seed, by life 1 / 2-3 / 4-9 / 10+ steps: {:.2} / {:.2} / {:.2} / {:.2}; of those never over 2 M km2: {:.2} / {:.2} / {:.2} / {:.2}",
        per(lives[0], seeds),
        per(lives[1], seeds),
        per(lives[2], seeds),
        per(lives[3], seeds),
        per(lives_small[0], seeds),
        per(lives_small[1], seeds),
        per(lives_small[2], seeds),
        per(lives_small[3], seeds),
    );
    println!(
        "born per seed: at the start {:.1}, rift {:.1}, escape {:.1}, opening {:.1}; ended: weld {:.1}, swallowed or held {:.1}",
        per(born[0], seeds),
        per(born[1], seeds),
        per(born[2], seeds),
        per(born[3], seeds),
        per(ended[0], seeds),
        per(ended[1], seeds),
    );
    println!(
        "per seed: cut-off parts taken by a neighbour {:.1}, whole plates taken {:.1}, plates born of a cut-off part {:.1}",
        per(taken[0], seeds),
        per(taken[1], seeds),
        per(taken[2], seeds),
    );
    polar.sort_unstable();
    biggest.sort_unstable();
    let within = biggest
        .iter()
        .filter(|&&b| (140..=300).contains(&b))
        .count();
    println!(
        "land beyond 60 deg: over 6 M km2 in {polar_over} of {seeds} seeds, median {:.1} M km2, mean {:.1}; largest land median {:.1} M km2, {within} of {seeds} within 14-30",
        polar[polar.len() / 2] as f64 / 10.0,
        polar.iter().sum::<i64>() as f64 / 10.0 / seeds as f64,
        biggest[biggest.len() / 2] as f64 / 10.0,
    );
}
