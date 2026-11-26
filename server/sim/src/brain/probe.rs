//! Probe counters for the measure "pensée de groupe" (branch
//! `probe/group-thought`, never merged). Reads nothing the simulation
//! reads: every value here is written, never consulted by a rule.
#![allow(
    missing_docs,
    clippy::missing_panics_doc,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use super::Decision;
use crate::BodyId;

pub const CLOCK: u16 = 1 << 0;
pub const NEED: u16 = 1 << 1;
pub const ARRIVED: u16 = 1 << 2;
pub const BLOCKED: u16 = 1 << 3;
pub const REFUSED: u16 = 1 << 4;
pub const ATE_DRANK: u16 = 1 << 5;
pub const SPOILED: u16 = 1 << 6;
pub const TRIP_TURN: u16 = 1 << 7;
pub const REDO: u16 = 1 << 8;
pub const GLANCE: u16 = 1 << 9;
pub const TIMER: u16 = 1 << 10;
pub const WORK_STOPPED: u16 = 1 << 11;
pub const GESTURED: u16 = 1 << 12;
const NAMES: [&str; 13] = [
    "clock", "need", "arrived", "blocked", "refused", "ate_drank", "spoiled", "trip_turn", "redo",
    "glance", "timer", "work_stopped", "gestured",
];
const PRIORITY: [u16; 13] = [
    NEED, CLOCK, TRIP_TURN, ARRIVED, WORK_STOPPED, ATE_DRANK, GESTURED, BLOCKED, REFUSED,
    SPOILED, REDO, GLANCE, TIMER,
];

type Key = (&'static str, &'static str, &'static str, &'static str, String, bool, bool);

#[derive(Default)]
struct Probe {
    current: Option<u32>,
    cause: HashMap<u32, u16>,
    pending: HashMap<u32, u16>,
    need_label: HashMap<u32, &'static str>,
    timer_label: HashMap<u32, &'static str>,
    glance_label: HashMap<u32, &'static str>,
    last: HashMap<u32, Decision>,
    thoughts: BTreeMap<Key, u64>,
    /// (class, role, phase) -> body-ticks
    ticks: BTreeMap<(&'static str, &'static str, &'static str), u64>,
    /// leader -> (dawn, last active tick, last tick with a member in it, started tick)
    live: HashMap<u32, (u64, u64, u64, Option<u64>)>,
    /// one line per group ended: size, moves, end, active, together, started
    ended: Vec<(u32, bool, u8, u64, u64, i64, u32)>,
    ticks_per_day: u64,
}

thread_local! {
    static P: RefCell<Probe> = RefCell::new(Probe::default());
}

pub fn cause(id: BodyId, bit: u16) {
    P.with(|p| *p.borrow_mut().cause.entry(id.0).or_default() |= bit);
}

/// A wake by `bit` at once (`now`) or scheduled for later (by the spread).
pub fn wake(id: BodyId, bit: u16, at_once: bool, label: Option<&'static str>) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(l) = label {
            p.need_label.insert(id.0, l);
        }
        if at_once {
            *p.cause.entry(id.0).or_default() |= bit;
        } else {
            *p.pending.entry(id.0).or_default() |= bit;
        }
    });
}

/// Its `wake_at` came: the pending wakes, or a timer.
pub fn timer_fired(id: BodyId) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let bits = p.pending.remove(&id.0).unwrap_or(0);
        *p.cause.entry(id.0).or_default() |= if bits == 0 { TIMER } else { bits };
    });
}

pub fn set_timer_label(id: BodyId, label: &'static str) {
    P.with(|p| {
        p.borrow_mut().timer_label.insert(id.0, label);
    });
}

pub fn glanced(id: BodyId, label: &'static str) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        *p.cause.entry(id.0).or_default() |= GLANCE;
        p.glance_label.insert(id.0, label);
    });
}

pub fn set_current(id: Option<BodyId>) {
    P.with(|p| p.borrow_mut().current = id.map(|i| i.0));
}

pub fn cause_current(bit: u16) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(id) = p.current {
            *p.cause.entry(id).or_default() |= bit;
        }
    });
}

/// The primary cause of the thought starting now, and its finer label;
/// every cause of `id` forgotten.
pub fn take_cause(id: BodyId) -> (&'static str, &'static str) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let mask = p.cause.remove(&id.0).unwrap_or(0);
        p.pending.remove(&id.0);
        for bit in PRIORITY {
            if mask & bit != 0 {
                let name = NAMES[bit.trailing_zeros() as usize];
                let sub = match bit {
                    NEED => p.need_label.get(&id.0).copied().unwrap_or("-"),
                    TIMER => p.timer_label.get(&id.0).copied().unwrap_or("-"),
                    GLANCE => p.glance_label.get(&id.0).copied().unwrap_or("-"),
                    _ => "-",
                };
                return (name, sub);
            }
        }
        ("none", "-")
    })
}

pub fn last(id: BodyId) -> Option<Decision> {
    P.with(|p| p.borrow().last.get(&id.0).cloned())
}

pub fn set_last(id: BodyId, d: Decision) {
    P.with(|p| {
        p.borrow_mut().last.insert(id.0, d);
    });
}

#[allow(clippy::too_many_arguments)]
pub fn thought(
    (role, phase): (&'static str, &'static str),
    (cause, sub): (&'static str, &'static str),
    kind: String,
    same: bool,
    left: bool,
) {
    P.with(|p| {
        *p.borrow_mut()
            .thoughts
            .entry((role, phase, cause, sub, kind, same, left))
            .or_default() += 1;
    });
}

pub fn body_tick(class: &'static str, (role, phase): (&'static str, &'static str)) {
    P.with(|p| *p.borrow_mut().ticks.entry((class, role, phase)).or_default() += 1);
}

pub fn ticks_per_day(t: u64) {
    P.with(|p| p.borrow_mut().ticks_per_day = t);
}

pub fn group_tick(leader: BodyId, dawn: u64, now: u64, active: bool, inside: bool, started: bool) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let e = p.live.entry(leader.0).or_insert((dawn, dawn, dawn, None));
        if e.0 != dawn {
            *e = (dawn, dawn, dawn, None);
        }
        if active {
            e.1 = now;
        }
        if inside {
            e.2 = now;
        }
        if started && e.3.is_none() {
            e.3 = Some(now);
        }
    });
}

pub fn group_end(leader: BodyId, size: u32, moves: bool, end: u8, dawn: u64, now: u64) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let (d, active, inside, started) =
            p.live.remove(&leader.0).unwrap_or((dawn, now, now, None));
        let d = if d == dawn { d } else { dawn };
        let started = started.map_or(-1, |s| (s - d) as i64);
        let day = (dawn / p.ticks_per_day.max(1)) as u32;
        p.ended
            .push((size, moves, end, active.saturating_sub(d), inside.saturating_sub(d), started, day));
    });
}

/// Everything counted since the last call, as one JSON object; the
/// counts start again from zero (the per-body state is kept).
pub fn dump() -> String {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let mut s = format!("{{\"ticks_per_day\":{},\"thoughts\":[", p.ticks_per_day);
        for (i, ((r, ph, c, sub, k, same, left), n)) in p.thoughts.iter().enumerate() {
            let _ = write!(
                s,
                "{}[\"{r}\",\"{ph}\",\"{c}\",\"{sub}\",\"{k}\",{},{},{n}]",
                if i > 0 { "," } else { "" },
                u8::from(*same),
                u8::from(*left)
            );
        }
        s.push_str("],\"ticks\":[");
        for (i, ((c, r, ph), n)) in p.ticks.iter().enumerate() {
            let _ = write!(s, "{}[\"{c}\",\"{r}\",\"{ph}\",{n}]", if i > 0 { "," } else { "" });
        }
        s.push_str("],\"groups\":[");
        for (i, (size, moves, end, active, inside, started, day)) in p.ended.iter().enumerate() {
            let _ = write!(
                s,
                "{}[{size},{},{end},{active},{inside},{started},{day}]",
                if i > 0 { "," } else { "" },
                u8::from(*moves)
            );
        }
        s.push_str("]}");
        p.thoughts.clear();
        p.ticks.clear();
        p.ended.clear();
        s
    })
}
