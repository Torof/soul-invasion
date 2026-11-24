//! World time: ticks, calendar and game time.
//!
//! The simulation advances by fixed ticks (D24). The calendar is world data
//! (D09): each world decides how many ticks make a second, how many
//! seconds make a day, how many days make a cycle of four seasons and how
//! many age-years a cycle holds (D09, amended 13 and 24 November 2026).
//! Time acceleration lives in the server, never here.
//!
//! Three clocks (D09, amended 24 November 2026):
//! - the **day**: needs, gathering, walking, rot;
//! - the **age-year**: ageing, pregnancy, childhood, fertility, healing;
//!   a body's age is counted in age-years, never in cycles;
//! - the **cycle** of four seasons: nature (fruit windows, crops, tree
//!   growth). Where the code, the protocol or the storage says "year"
//!   without "age" (the event `SeasonStarted { year }`, a crop's
//!   year, the stored and sent `days_per_year`), it means the cycle.
//!
//! The calendar v1 (D09, in force since 24 November 2026): a game day of
//! 16,200 s, a cycle of 64 game days (seasons of 16), 8 age-years of 8
//! game days a cycle. A world saved before keeps its own day and its
//! pinned legacy cycle: 50 days, one age-year a cycle.

use serde::{Deserialize, Serialize};

/// A simulation tick counter since the world was created.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Tick(pub u64);

/// Error building a [`Calendar`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalendarError {
    /// `ticks_per_second` was zero.
    #[error("ticks_per_second must be at least 1")]
    ZeroTicksPerSecond,
    /// `seconds_per_day` was zero.
    #[error("seconds_per_day must be at least 1")]
    ZeroSecondsPerDay,
    /// `days_per_cycle` was under four: every season needs a day.
    #[error("days_per_cycle must be at least 4, a day per season")]
    TooShortCycle,
    /// `age_years_per_cycle` was zero.
    #[error("age_years_per_cycle must be at least 1")]
    ZeroAgeYears,
    /// The cycle's days are not a whole number of age-years, each of four
    /// days at least (a day per quarter of an age-year).
    #[error("days_per_cycle must be a whole number of age-years of at least 4 days each")]
    UnevenAgeYears,
}

/// A quarter of the cycle (D09). Seasons start at dawn: a day belongs to
/// one season only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    /// The first quarter; a world is born on its first day.
    Spring,
    /// The second.
    Summer,
    /// The third.
    Autumn,
    /// The last.
    Winter,
}

impl Season {
    /// The four, in the order of the cycle.
    pub const ALL: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];
}

/// The rhythm of a world's time. Read back through [`Calendar::new`]'s
/// checks: a stored or crafted calendar with a zero length is refused,
/// not divided by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredCalendar")]
pub struct Calendar {
    ticks_per_second: u32,
    seconds_per_day: u32,
    /// Game days in a cycle of four seasons, stored under its first name,
    /// `days_per_year`.
    #[serde(rename = "days_per_year")]
    days_per_cycle: u32,
    /// Age-years in a cycle (snapshot v18).
    age_years_per_cycle: u32,
}

/// A calendar as stored: the same fields, checked on reading. A field
/// absent from an old world takes its legacy value, pinned: never a
/// default that a later calendar may change.
#[derive(Deserialize)]
struct StoredCalendar {
    ticks_per_second: u32,
    seconds_per_day: u32,
    #[serde(default = "legacy_days_per_cycle", rename = "days_per_year")]
    days_per_cycle: u32,
    #[serde(default = "legacy_age_years_per_cycle")]
    age_years_per_cycle: u32,
}

impl TryFrom<StoredCalendar> for Calendar {
    type Error = CalendarError;

    fn try_from(stored: StoredCalendar) -> Result<Self, CalendarError> {
        Self::new(stored.ticks_per_second, stored.seconds_per_day)?
            .with_cycle(stored.days_per_cycle, stored.age_years_per_cycle)
    }
}

fn legacy_days_per_cycle() -> u32 {
    Calendar::LEGACY_DAYS_PER_CYCLE
}

fn legacy_age_years_per_cycle() -> u32 {
    Calendar::LEGACY_AGE_YEARS_PER_CYCLE
}

/// The quarter (0 to 3) of a span of `days` days that its day `day`
/// (from 0) falls in, cut at whole days: `day × 4 / days`.
fn quarter_of(day: u64, days: u64) -> u64 {
    day % days * 4 / days
}

/// The first day (from 0) of the quarter `quarter` of a span of `days`.
fn quarter_first_day(quarter: u64, days: u64) -> u64 {
    (quarter * days).div_ceil(4)
}

impl Calendar {
    /// Default simulation resolution: ten ticks per game second.
    pub const DEFAULT_TICKS_PER_SECOND: u32 = 10;
    /// Default day length (the calendar v1, D09): 16,200 game seconds,
    /// four and a half real hours at the game's pace, so the hour of the
    /// day a player meets shifts each evening.
    pub const DEFAULT_SECONDS_PER_DAY: u32 = 16_200;
    /// Default cycle of four seasons (the calendar v1, D09): 64 game days,
    /// seasons of 16, about twelve real days.
    pub const DEFAULT_DAYS_PER_CYCLE: u32 = 64;
    /// Default age-years in a cycle (the calendar v1, D09): eight, an
    /// age-year of 8 game days, half a season.
    pub const DEFAULT_AGE_YEARS_PER_CYCLE: u32 = 8;
    /// The cycle of a world saved before it was stored (snapshot v12):
    /// fifty days, pinned, whatever the default becomes.
    pub const LEGACY_DAYS_PER_CYCLE: u32 = 50;
    /// The age-years a cycle of a world saved before they were stored
    /// (snapshot v18): one, pinned, whatever the default becomes.
    pub const LEGACY_AGE_YEARS_PER_CYCLE: u32 = 1;

    /// Builds a calendar with the default cycle and age-years, rejecting
    /// zero lengths.
    pub fn new(ticks_per_second: u32, seconds_per_day: u32) -> Result<Self, CalendarError> {
        if ticks_per_second == 0 {
            return Err(CalendarError::ZeroTicksPerSecond);
        }
        if seconds_per_day == 0 {
            return Err(CalendarError::ZeroSecondsPerDay);
        }
        Ok(Self {
            ticks_per_second,
            seconds_per_day,
            days_per_cycle: Self::DEFAULT_DAYS_PER_CYCLE,
            age_years_per_cycle: Self::DEFAULT_AGE_YEARS_PER_CYCLE,
        })
    }

    /// The same calendar with a cycle of `days_per_cycle` days, at least
    /// four (a day per season), and its age-years unchanged.
    pub fn with_days_per_cycle(self, days_per_cycle: u32) -> Result<Self, CalendarError> {
        self.with_cycle(days_per_cycle, self.age_years_per_cycle)
    }

    /// The same calendar with `age_years_per_cycle` age-years in its
    /// cycle, which they must divide into whole age-years of four days at
    /// least (a day per quarter of an age-year).
    pub fn with_age_years_per_cycle(self, age_years_per_cycle: u32) -> Result<Self, CalendarError> {
        self.with_cycle(self.days_per_cycle, age_years_per_cycle)
    }

    /// The same calendar with a cycle of `days_per_cycle` days holding
    /// `age_years_per_cycle` age-years, checked as both setters check.
    pub fn with_cycle(
        self,
        days_per_cycle: u32,
        age_years_per_cycle: u32,
    ) -> Result<Self, CalendarError> {
        if days_per_cycle < 4 {
            return Err(CalendarError::TooShortCycle);
        }
        if age_years_per_cycle == 0 {
            return Err(CalendarError::ZeroAgeYears);
        }
        if !days_per_cycle.is_multiple_of(age_years_per_cycle)
            || days_per_cycle / age_years_per_cycle < 4
        {
            return Err(CalendarError::UnevenAgeYears);
        }
        Ok(Self {
            days_per_cycle,
            age_years_per_cycle,
            ..self
        })
    }

    /// Game days in one cycle of four seasons (the nature clock). Not the
    /// age-year: see [`Calendar::days_per_age_year`].
    pub fn days_per_cycle(&self) -> u32 {
        self.days_per_cycle
    }

    /// Age-years in one cycle (the age clock against the nature clock).
    pub fn age_years_per_cycle(&self) -> u32 {
        self.age_years_per_cycle
    }

    /// Game days in one age-year: what a body's age, its pregnancy, its
    /// childhood and its fertility are counted in. A whole number, four
    /// at least.
    pub fn days_per_age_year(&self) -> u32 {
        self.days_per_cycle / self.age_years_per_cycle
    }

    /// The season of a day of the cycle (0 for its first). The cycle is
    /// cut in four at whole days, `day × 4 / days_per_cycle`: equal
    /// seasons when the cycle is a multiple of four, else as near as whole
    /// days allow (50 days: 13, 12, 13, 12).
    pub fn season_of(&self, day_of_cycle: u64) -> Season {
        let quarter = quarter_of(day_of_cycle, u64::from(self.days_per_cycle));
        Season::ALL[usize::try_from(quarter).expect("under four")]
    }

    /// Ticks in one game second.
    pub fn ticks_per_second(&self) -> u32 {
        self.ticks_per_second
    }

    /// Game seconds in one game day.
    pub fn seconds_per_day(&self) -> u32 {
        self.seconds_per_day
    }

    /// Ticks in one game day.
    pub fn ticks_per_day(&self) -> u64 {
        u64::from(self.ticks_per_second) * u64::from(self.seconds_per_day)
    }

    /// The game time at a given tick.
    pub fn time_at(&self, tick: Tick) -> GameTime {
        let ticks_per_day = self.ticks_per_day();
        let days = tick.0 / ticks_per_day;
        let days_per_cycle = u64::from(self.days_per_cycle);
        let day_of_cycle = days % days_per_cycle;
        let season_quarter = quarter_of(day_of_cycle, days_per_cycle);
        let days_per_age_year = u64::from(self.days_per_age_year());
        let day_of_age_year = days % days_per_age_year;
        let age_quarter = quarter_of(day_of_age_year, days_per_age_year);
        GameTime {
            day: days + 1,
            tick_of_day: tick.0 % ticks_per_day,
            cycle: days / days_per_cycle + 1,
            day_of_cycle: day_of_cycle + 1,
            season: self.season_of(day_of_cycle),
            age_year: days / days_per_age_year + 1,
            age_quarter: u8::try_from(age_quarter).expect("under four"),
            day_of_season: day_of_cycle - quarter_first_day(season_quarter, days_per_cycle),
            day_of_age_quarter: day_of_age_year - quarter_first_day(age_quarter, days_per_age_year),
            season_start: day_of_cycle == 0
                || self.season_of(day_of_cycle - 1) != self.season_of(day_of_cycle),
            ticks_per_day,
        }
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            ticks_per_second: Self::DEFAULT_TICKS_PER_SECOND,
            seconds_per_day: Self::DEFAULT_SECONDS_PER_DAY,
            days_per_cycle: Self::DEFAULT_DAYS_PER_CYCLE,
            age_years_per_cycle: Self::DEFAULT_AGE_YEARS_PER_CYCLE,
        }
    }
}

/// A moment in a world's time, derived from a tick and a calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameTime {
    /// Day number, starting at 1.
    pub day: u64,
    /// Ticks elapsed since the start of the day.
    pub tick_of_day: u64,
    /// Cycle number (the cycle of four seasons, the "year" of the
    /// protocol and of the crops), starting at 1.
    pub cycle: u64,
    /// Day of the cycle, starting at 1.
    pub day_of_cycle: u64,
    /// The season the day belongs to.
    pub season: Season,
    /// Age-year number since the world's first day, starting at 1: the
    /// clock of ages, a cycle holding `age_years_per_cycle` of them.
    pub age_year: u64,
    /// The quarter of the age-year the day belongs to, 0 to 3, cut at
    /// whole days as the seasons cut the cycle (the same as the season's
    /// index while an age-year is the cycle).
    pub age_quarter: u8,
    day_of_season: u64,
    day_of_age_quarter: u64,
    season_start: bool,
    ticks_per_day: u64,
}

impl GameTime {
    /// Position within the day, from 0.0 (day start) to just under 1.0.
    // Not generation: a fraction for display and light, never stored.
    #[allow(
        clippy::cast_precision_loss,
        clippy::float_arithmetic,
        reason = "small counts and coordinates, far below the float precision limit; float maths of the simulation, the same result on every run"
    )]
    pub fn day_fraction(&self) -> f64 {
        self.tick_of_day as f64 / self.ticks_per_day as f64
    }

    /// True on the very first tick of a day.
    pub fn is_day_start(&self) -> bool {
        self.tick_of_day == 0
    }

    /// Days since the season started, from 0 on its first day.
    pub fn day_of_season(&self) -> u64 {
        self.day_of_season
    }

    /// Days since the quarter of the age-year started, from 0 on its
    /// first day.
    pub fn day_of_age_quarter(&self) -> u64 {
        self.day_of_age_quarter
    }

    /// True on the very first tick of a season (and so of a cycle).
    pub fn is_season_start(&self) -> bool {
        self.season_start && self.is_day_start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_calendar_with_a_zero_length_is_refused() {
        let read = |json: &str| serde_json::from_str::<Calendar>(json);
        assert!(read(r#"{"ticks_per_second":0,"seconds_per_day":60}"#).is_err());
        assert!(read(r#"{"ticks_per_second":10,"seconds_per_day":0}"#).is_err());
        assert!(read(r#"{"ticks_per_second":10,"seconds_per_day":60,"days_per_year":0}"#).is_err());
        assert!(
            read(r#"{"ticks_per_second":10,"seconds_per_day":60,"age_years_per_cycle":0}"#)
                .is_err()
        );
        assert!(
            read(r#"{"ticks_per_second":10,"seconds_per_day":60,"days_per_year":64,"age_years_per_cycle":5}"#)
                .is_err(),
            "64 days are not five whole age-years"
        );
        // An old calendar takes the legacy values, pinned: fifty days a
        // cycle, one age-year a cycle.
        let old = read(r#"{"ticks_per_second":10,"seconds_per_day":60}"#).unwrap();
        assert_eq!(old.days_per_cycle(), 50);
        assert_eq!(old.age_years_per_cycle(), 1);
        assert_eq!(old.days_per_age_year(), 50);
    }

    #[test]
    fn a_calendar_is_stored_with_its_age_years_under_the_year_s_first_name() {
        let c = Calendar::new(10, 16_200)
            .unwrap()
            .with_cycle(64, 8)
            .unwrap();
        let json = serde_json::to_value(c).unwrap();
        assert_eq!(json["days_per_year"], 64);
        assert_eq!(json["age_years_per_cycle"], 8);
        assert_eq!(serde_json::from_value::<Calendar>(json).unwrap(), c);
    }

    #[test]
    fn the_age_years_cut_the_cycle_in_whole_days() {
        let c = Calendar::new(1, 1).unwrap().with_cycle(50, 1).unwrap();
        assert_eq!(
            c.with_age_years_per_cycle(0),
            Err(CalendarError::ZeroAgeYears)
        );
        // 50 days are not 8 whole age-years; 12 days are not 4 of 4 days.
        assert_eq!(
            c.with_age_years_per_cycle(8),
            Err(CalendarError::UnevenAgeYears)
        );
        assert_eq!(c.with_cycle(12, 4), Err(CalendarError::UnevenAgeYears));
        let v1 = c.with_cycle(64, 8).unwrap();
        assert_eq!(
            (
                v1.days_per_cycle(),
                v1.age_years_per_cycle(),
                v1.days_per_age_year()
            ),
            (64, 8, 8)
        );
        // The cycle's length keeps the age-years, and is checked with them:
        // 36 days are not 8 whole age-years, 24 days are 8 of 3 days.
        assert_eq!(v1.with_days_per_cycle(32).unwrap().days_per_age_year(), 4);
        assert_eq!(
            v1.with_days_per_cycle(24),
            Err(CalendarError::UnevenAgeYears)
        );
        assert_eq!(
            v1.with_days_per_cycle(36),
            Err(CalendarError::UnevenAgeYears)
        );
        // The legacy shape: one age-year, the whole cycle.
        assert_eq!(c.age_years_per_cycle(), 1);
        assert_eq!(c.days_per_age_year(), c.days_per_cycle());
    }

    #[test]
    fn a_tick_has_its_age_year_and_its_quarter() {
        // The validated v1: 64 days a cycle, 8 age-years of 8 days.
        let c = Calendar::new(1, 10).unwrap().with_cycle(64, 8).unwrap();
        let at_day = |day: u64| c.time_at(Tick((day - 1) * 10));
        for (day, cycle, season, age_year, quarter, of_quarter) in [
            (1, 1, Season::Spring, 1, 0, 0),
            (2, 1, Season::Spring, 1, 0, 1),
            (3, 1, Season::Spring, 1, 1, 0),
            (8, 1, Season::Spring, 1, 3, 1),
            (9, 1, Season::Spring, 2, 0, 0),
            (16, 1, Season::Spring, 2, 3, 1),
            (17, 1, Season::Summer, 3, 0, 0),
            (64, 1, Season::Winter, 8, 3, 1),
            (65, 2, Season::Spring, 9, 0, 0),
        ] {
            let t = at_day(day);
            assert_eq!(
                (
                    t.cycle,
                    t.season,
                    t.age_year,
                    t.age_quarter,
                    t.day_of_age_quarter()
                ),
                (cycle, season, age_year, quarter, of_quarter),
                "day {day}"
            );
        }
        // Four equal seasons of sixteen days.
        assert_eq!(at_day(17).day_of_season(), 0);
        assert!(c.time_at(Tick(16 * 10)).is_season_start());
        // One age-year a cycle: the age-year, its quarters and their days
        // are the cycle, its seasons and theirs, whole day for whole day.
        let today = Calendar::new(1, 10).unwrap().with_cycle(50, 1).unwrap();
        for day in 1..=150 {
            let t = today.time_at(Tick((day - 1) * 10));
            assert_eq!(t.age_year, t.cycle, "day {day}");
            assert_eq!(
                Season::ALL[usize::from(t.age_quarter)],
                t.season,
                "day {day}"
            );
            assert_eq!(t.day_of_age_quarter(), t.day_of_season(), "day {day}");
        }
    }

    #[test]
    fn default_calendar_is_ten_ticks_and_four_hours_and_a_half() {
        let c = Calendar::default();
        assert_eq!(c.ticks_per_second(), 10);
        assert_eq!(c.seconds_per_day(), 16_200);
        assert_eq!(c.ticks_per_day(), 162_000);
    }

    #[test]
    fn zero_lengths_are_rejected() {
        assert_eq!(Calendar::new(0, 10), Err(CalendarError::ZeroTicksPerSecond));
        assert_eq!(Calendar::new(10, 0), Err(CalendarError::ZeroSecondsPerDay));
        let c = Calendar::new(1, 1).unwrap();
        assert_eq!(c.with_days_per_cycle(3), Err(CalendarError::TooShortCycle));
        assert_eq!(c.with_cycle(4, 1).unwrap().days_per_cycle(), 4);
    }

    #[test]
    fn the_default_cycle_is_v1() {
        for c in [Calendar::default(), Calendar::new(2, 5).unwrap()] {
            assert_eq!(c.days_per_cycle(), 64);
            assert_eq!(c.age_years_per_cycle(), 8);
            assert_eq!(c.days_per_age_year(), 8);
        }
        // Seasons of sixteen days, quarters of age-year of two.
        let c = Calendar::new(1, 10).unwrap();
        let t = c.time_at(Tick(16 * 10));
        assert_eq!(
            (t.season, t.age_year, t.age_quarter),
            (Season::Summer, 3, 0)
        );
        assert_eq!(c.time_at(Tick(2 * 10)).age_quarter, 1);
    }

    #[test]
    fn a_tick_has_a_year_and_a_season() {
        let c = Calendar::new(1, 10).unwrap().with_cycle(50, 1).unwrap(); // 10 ticks a day, 50 days a cycle
        let at_day = |day: u64| c.time_at(Tick((day - 1) * 10));
        // 50 days in four: 13, 12, 13, 12.
        for (day, season, year, of_year) in [
            (1, Season::Spring, 1, 1),
            (13, Season::Spring, 1, 13),
            (14, Season::Summer, 1, 14),
            (25, Season::Summer, 1, 25),
            (26, Season::Autumn, 1, 26),
            (38, Season::Autumn, 1, 38),
            (39, Season::Winter, 1, 39),
            (50, Season::Winter, 1, 50),
            (51, Season::Spring, 2, 1),
            (114, Season::Summer, 3, 14),
        ] {
            let t = at_day(day);
            assert_eq!(
                (t.day, t.season, t.cycle, t.day_of_cycle),
                (day, season, year, of_year),
                "day {day}"
            );
        }
        // The days since the season started.
        for (day, of_season) in [
            (1, 0),
            (13, 12),
            (14, 0),
            (25, 11),
            (26, 0),
            (38, 12),
            (39, 0),
            (50, 11),
            (51, 0),
        ] {
            assert_eq!(at_day(day).day_of_season(), of_season, "day {day}");
        }
        // A season starts at the dawn of its first day only.
        assert!(c.time_at(Tick(13 * 10)).is_season_start());
        assert!(!c.time_at(Tick(13 * 10 + 1)).is_season_start());
        assert!(!c.time_at(Tick(12 * 10)).is_season_start());
        assert!(c.time_at(Tick(50 * 10)).is_season_start(), "a new year");
        // A year of a multiple of four has equal seasons.
        let even = c.with_days_per_cycle(8).unwrap();
        let seasons: Vec<Season> = (0..8).map(|d| even.season_of(d)).collect();
        assert_eq!(
            seasons,
            [
                Season::Spring,
                Season::Spring,
                Season::Summer,
                Season::Summer,
                Season::Autumn,
                Season::Autumn,
                Season::Winter,
                Season::Winter
            ]
        );
    }

    #[test]
    fn time_at_counts_days_from_one() {
        let c = Calendar::new(2, 5).unwrap(); // 10 ticks per day
        assert_eq!(c.time_at(Tick(0)).day, 1);
        assert_eq!(c.time_at(Tick(9)).day, 1);
        assert_eq!(c.time_at(Tick(10)).day, 2);
        assert_eq!(c.time_at(Tick(10)).tick_of_day, 0);
        assert!(c.time_at(Tick(10)).is_day_start());
        assert!(!c.time_at(Tick(11)).is_day_start());
    }

    #[test]
    fn day_fraction_spans_the_day() {
        let c = Calendar::new(1, 4).unwrap();
        assert!((c.time_at(Tick(0)).day_fraction() - 0.0).abs() < f64::EPSILON);
        assert!((c.time_at(Tick(2)).day_fraction() - 0.5).abs() < f64::EPSILON);
        assert!((c.time_at(Tick(3)).day_fraction() - 0.75).abs() < f64::EPSILON);
    }
}
