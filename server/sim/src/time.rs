//! World time: ticks, calendar and game time.
//!
//! The simulation advances by fixed ticks (D24). The calendar is world data
//! (D09): each world decides how many ticks make a second and how many
//! seconds make a day, and how many days make a year of four seasons
//! (D09, amended 13 November 2026). Time acceleration lives in the server,
//! never here.

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
    /// `days_per_year` was under four: every season needs a day.
    #[error("days_per_year must be at least 4, a day per season")]
    TooShortYear,
}

/// A quarter of the year (D09). Seasons start at dawn: a day belongs to
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
    /// The four, in the order of the year.
    pub const ALL: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];
}

/// The rhythm of a world's time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    ticks_per_second: u32,
    seconds_per_day: u32,
    /// Absent from a calendar saved before the year existed (and from
    /// the old snapshots the storage's first migrations read): it takes
    /// the provisional year.
    #[serde(default = "default_days_per_year")]
    days_per_year: u32,
}

fn default_days_per_year() -> u32 {
    Calendar::DEFAULT_DAYS_PER_YEAR
}

impl Calendar {
    /// Default simulation resolution: ten ticks per game second.
    pub const DEFAULT_TICKS_PER_SECOND: u32 = 10;
    /// Default day length: two hours of game time, the first-version
    /// order of magnitude (see `docs/05-first-version.md`).
    pub const DEFAULT_SECONDS_PER_DAY: u32 = 2 * 60 * 60;
    /// Default year: fifty game days, about four real days with a
    /// two-hour day (D09, provisional, to be fixed before a world opens).
    pub const DEFAULT_DAYS_PER_YEAR: u32 = 50;

    /// Builds a calendar with the default year, rejecting zero lengths.
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
            days_per_year: Self::DEFAULT_DAYS_PER_YEAR,
        })
    }

    /// The same calendar with a year of `days_per_year` days, at least
    /// four (a day per season).
    pub fn with_days_per_year(self, days_per_year: u32) -> Result<Self, CalendarError> {
        if days_per_year < 4 {
            return Err(CalendarError::TooShortYear);
        }
        Ok(Self {
            days_per_year,
            ..self
        })
    }

    /// Game days in one year.
    pub fn days_per_year(&self) -> u32 {
        self.days_per_year
    }

    /// The season of a day of the year (0 for its first). The year is cut
    /// in four at whole days, `day × 4 / days_per_year`: equal seasons
    /// when the year is a multiple of four, else as near as whole days
    /// allow (50 days: 13, 12, 13, 12).
    pub fn season_of(&self, day_of_year: u64) -> Season {
        let quarter =
            day_of_year % u64::from(self.days_per_year) * 4 / u64::from(self.days_per_year);
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
        let days_per_year = u64::from(self.days_per_year);
        let day_of_year = days % days_per_year;
        GameTime {
            day: days + 1,
            tick_of_day: tick.0 % ticks_per_day,
            year: days / days_per_year + 1,
            day_of_year: day_of_year + 1,
            season: self.season_of(day_of_year),
            season_start: day_of_year == 0
                || self.season_of(day_of_year - 1) != self.season_of(day_of_year),
            ticks_per_day,
        }
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            ticks_per_second: Self::DEFAULT_TICKS_PER_SECOND,
            seconds_per_day: Self::DEFAULT_SECONDS_PER_DAY,
            days_per_year: Self::DEFAULT_DAYS_PER_YEAR,
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
    /// Year number, starting at 1.
    pub year: u64,
    /// Day of the year, starting at 1.
    pub day_of_year: u64,
    /// The season the day belongs to.
    pub season: Season,
    season_start: bool,
    ticks_per_day: u64,
}

impl GameTime {
    /// Position within the day, from 0.0 (day start) to just under 1.0.
    // Not generation: a fraction for display and light, never stored.
    #[allow(clippy::cast_precision_loss, clippy::float_arithmetic)]
    pub fn day_fraction(&self) -> f64 {
        self.tick_of_day as f64 / self.ticks_per_day as f64
    }

    /// True on the very first tick of a day.
    pub fn is_day_start(&self) -> bool {
        self.tick_of_day == 0
    }

    /// True on the very first tick of a season (and so of a year).
    pub fn is_season_start(&self) -> bool {
        self.season_start && self.is_day_start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_calendar_is_ten_ticks_and_two_hours() {
        let c = Calendar::default();
        assert_eq!(c.ticks_per_second(), 10);
        assert_eq!(c.seconds_per_day(), 7200);
        assert_eq!(c.ticks_per_day(), 72_000);
    }

    #[test]
    fn zero_lengths_are_rejected() {
        assert_eq!(Calendar::new(0, 10), Err(CalendarError::ZeroTicksPerSecond));
        assert_eq!(Calendar::new(10, 0), Err(CalendarError::ZeroSecondsPerDay));
        let c = Calendar::new(1, 1).unwrap();
        assert_eq!(c.with_days_per_year(3), Err(CalendarError::TooShortYear));
        assert_eq!(c.with_days_per_year(4).unwrap().days_per_year(), 4);
    }

    #[test]
    fn the_default_year_is_fifty_days() {
        assert_eq!(Calendar::default().days_per_year(), 50);
        assert_eq!(Calendar::new(2, 5).unwrap().days_per_year(), 50);
    }

    #[test]
    fn a_tick_has_a_year_and_a_season() {
        let c = Calendar::new(1, 10).unwrap(); // 10 ticks a day, 50 days a year
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
                (t.day, t.season, t.year, t.day_of_year),
                (day, season, year, of_year),
                "day {day}"
            );
        }
        // A season starts at the dawn of its first day only.
        assert!(c.time_at(Tick(13 * 10)).is_season_start());
        assert!(!c.time_at(Tick(13 * 10 + 1)).is_season_start());
        assert!(!c.time_at(Tick(12 * 10)).is_season_start());
        assert!(c.time_at(Tick(50 * 10)).is_season_start(), "a new year");
        // A year of a multiple of four has equal seasons.
        let even = c.with_days_per_year(8).unwrap();
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
