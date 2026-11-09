//! Command-line options.
//!
//! Hand-rolled on purpose: a few flags do not justify a dependency yet.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use sim::Calendar;

/// Usage text printed by `--help`.
pub const USAGE: &str = "\
soul-invasion server

USAGE:
    server [OPTIONS]

OPTIONS:
    --headless             Run as fast as possible with no clients, then exit.
                           Requires --days.
    --days <N>             Stop after N full game days (optional in paced mode).
    --listen <ADDR>        Paced mode: WebSocket listen address
                           (default 127.0.0.1:7777; port 0 picks a free port).
    --data <PATH>          SQLite file holding the world's snapshots and
                           event journal (default world.sqlite). The world
                           resumes from it when it exists.
    --ephemeral            Keep the world in memory only; nothing is saved.
    --reset                Erase the data file first and start a new world.
    --time-scale <X>       Paced mode: run X times faster than real time
                           (default 1.0). Does not change the rules, only the
                           pace (D09).
    --max-connections <N>  Paced mode: clients served at once, at most
                           (default 64). Further connections are refused.
    --content <DIR>        Directory of the content files (materials,
                           spawn rules). Default: the first of content,
                           ../content, ../../content that exists.
    --ticks-per-second <N> Calendar: simulation ticks per game second
                           (default 10).
    --seconds-per-day <N>  Calendar: game seconds per game day (default 7200).
    --valley-size <N>      A new world's valley, N tiles on a side (default
                           3072, at least 256). A smaller one starts far
                           faster: for tests and development.
    --dev                  Development tools: every client gets an overview
                           of the whole map and may teleport its body. Never
                           for play.
    -h, --help             Print this help.
";

/// Default number of connections served at once.
pub const DEFAULT_MAX_CONNECTIONS: usize = 64;

/// Default listen address.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7777";

/// Default database file.
pub const DEFAULT_DATA: &str = "world.sqlite";

/// Where the world is persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataLocation {
    /// A SQLite file on disk.
    File(PathBuf),
    /// Memory only: nothing survives the process.
    Ephemeral,
}

/// Parsed command-line options.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Run with no clients, as fast as possible, then exit.
    pub headless: bool,
    /// Stop after this many full game days.
    pub days: Option<u64>,
    /// WebSocket listen address (paced mode only).
    pub listen: SocketAddr,
    /// Where snapshots and the event journal go.
    pub data: DataLocation,
    /// Erase the data file before starting: a new world.
    pub reset: bool,
    /// Paced mode speed factor.
    pub time_scale: f64,
    /// Connections served at once, at most (paced mode only).
    pub max_connections: usize,
    /// Content directory, or `None` to look in the default places.
    pub content: Option<PathBuf>,
    /// The world's calendar.
    pub calendar: Calendar,
    /// Development tools on (`--dev`): the map's overview and teleports.
    pub dev: bool,
    /// A new world's valley, tiles on a side.
    pub valley_size: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            headless: false,
            days: None,
            listen: DEFAULT_LISTEN.parse().expect("valid default address"),
            data: DataLocation::File(PathBuf::from(DEFAULT_DATA)),
            reset: false,
            time_scale: 1.0,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            content: None,
            calendar: Calendar::default(),
            dev: false,
            valley_size: sim::VALLEY_SIZE,
        }
    }
}

/// Error parsing the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// `--help` was asked; the caller should print [`USAGE`] and exit 0.
    HelpRequested,
    /// An option was not recognised.
    UnknownOption(String),
    /// An option needs a value and none was given.
    MissingValue(String),
    /// A value could not be parsed.
    InvalidValue {
        /// The option.
        option: String,
        /// The text that was given.
        value: String,
    },
    /// `--headless` without `--days` would never stop.
    HeadlessNeedsDays,
    /// The calendar lengths are not usable.
    Calendar(sim::CalendarError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HelpRequested => write!(f, "help requested"),
            Self::UnknownOption(o) => write!(f, "unknown option `{o}`"),
            Self::MissingValue(o) => write!(f, "option `{o}` needs a value"),
            Self::InvalidValue { option, value } => {
                write!(f, "invalid value `{value}` for `{option}`")
            }
            Self::HeadlessNeedsDays => write!(f, "`--headless` requires `--days <N>`"),
            Self::Calendar(e) => write!(f, "invalid calendar: {e}"),
        }
    }
}

impl std::error::Error for CliError {}

impl Options {
    /// Parses options from an argument list (without the program name).
    pub fn parse<I, S>(args: I) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut opts = Self::default();
        let mut ticks_per_second = Calendar::DEFAULT_TICKS_PER_SECOND;
        let mut seconds_per_day = Calendar::DEFAULT_SECONDS_PER_DAY;
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            let arg = arg.as_ref();
            match arg {
                "-h" | "--help" => return Err(CliError::HelpRequested),
                "--headless" => opts.headless = true,
                "--days" => opts.days = Some(value(arg, args.next())?),
                "--listen" => opts.listen = value(arg, args.next())?,
                "--data" => opts.data = DataLocation::File(value(arg, args.next())?),
                "--ephemeral" => opts.data = DataLocation::Ephemeral,
                "--reset" => opts.reset = true,
                "--dev" => opts.dev = true,
                "--valley-size" => opts.valley_size = value(arg, args.next())?,
                "--time-scale" => opts.time_scale = value(arg, args.next())?,
                "--max-connections" => opts.max_connections = value(arg, args.next())?,
                "--content" => opts.content = Some(value(arg, args.next())?),
                "--ticks-per-second" => ticks_per_second = value(arg, args.next())?,
                "--seconds-per-day" => seconds_per_day = value(arg, args.next())?,
                other => return Err(CliError::UnknownOption(other.to_owned())),
            }
        }

        if opts.headless && opts.days.is_none() {
            return Err(CliError::HeadlessNeedsDays);
        }
        if !(opts.time_scale.is_finite() && opts.time_scale > 0.0) {
            return Err(CliError::InvalidValue {
                option: "--time-scale".to_owned(),
                value: opts.time_scale.to_string(),
            });
        }
        if opts.max_connections == 0 {
            return Err(CliError::InvalidValue {
                option: "--max-connections".to_owned(),
                value: "0".to_owned(),
            });
        }
        if opts.valley_size < 256 {
            return Err(CliError::InvalidValue {
                option: "--valley-size".to_owned(),
                value: opts.valley_size.to_string(),
            });
        }
        opts.calendar =
            Calendar::new(ticks_per_second, seconds_per_day).map_err(CliError::Calendar)?;
        Ok(opts)
    }
}

fn value<T, S>(option: &str, raw: Option<S>) -> Result<T, CliError>
where
    T: std::str::FromStr,
    S: AsRef<str>,
{
    let raw = raw.ok_or_else(|| CliError::MissingValue(option.to_owned()))?;
    raw.as_ref().parse().map_err(|_| CliError::InvalidValue {
        option: option.to_owned(),
        value: raw.as_ref().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_gives_defaults() {
        let opts = Options::parse(Vec::<&str>::new()).unwrap();
        assert_eq!(opts, Options::default());
        assert_eq!(opts.listen.to_string(), DEFAULT_LISTEN);
    }

    #[test]
    fn full_headless_command_line() {
        let opts = Options::parse([
            "--headless",
            "--days",
            "3",
            "--ticks-per-second",
            "4",
            "--seconds-per-day",
            "30",
        ])
        .unwrap();
        assert!(opts.headless);
        assert_eq!(opts.days, Some(3));
        assert_eq!(opts.calendar, Calendar::new(4, 30).unwrap());
    }

    #[test]
    fn listen_address_is_parsed() {
        let opts = Options::parse(["--listen", "0.0.0.0:0"]).unwrap();
        assert_eq!(opts.listen.to_string(), "0.0.0.0:0");
        assert_eq!(
            Options::parse(["--listen", "nowhere"]),
            Err(CliError::InvalidValue {
                option: "--listen".into(),
                value: "nowhere".into()
            })
        );
    }

    #[test]
    fn development_tools_are_off_unless_asked() {
        assert!(!Options::parse(Vec::<&str>::new()).unwrap().dev);
        assert!(Options::parse(["--dev"]).unwrap().dev);
    }

    #[test]
    fn valley_size_is_parsed_and_must_be_large_enough() {
        assert_eq!(
            Options::parse(Vec::<&str>::new()).unwrap().valley_size,
            sim::VALLEY_SIZE
        );
        assert_eq!(
            Options::parse(["--valley-size", "768"])
                .unwrap()
                .valley_size,
            768
        );
        assert!(Options::parse(["--valley-size", "100"]).is_err());
    }

    #[test]
    fn data_location_is_parsed() {
        assert_eq!(
            Options::parse(Vec::<&str>::new()).unwrap().data,
            DataLocation::File(PathBuf::from("world.sqlite"))
        );
        assert_eq!(
            Options::parse(["--data", "/tmp/w.sqlite"]).unwrap().data,
            DataLocation::File(PathBuf::from("/tmp/w.sqlite"))
        );
        assert_eq!(
            Options::parse(["--ephemeral"]).unwrap().data,
            DataLocation::Ephemeral
        );
    }

    #[test]
    fn headless_requires_days() {
        assert_eq!(
            Options::parse(["--headless"]),
            Err(CliError::HeadlessNeedsDays)
        );
    }

    #[test]
    fn errors_are_specific() {
        assert_eq!(
            Options::parse(["--bogus"]),
            Err(CliError::UnknownOption("--bogus".into()))
        );
        assert_eq!(
            Options::parse(["--days"]),
            Err(CliError::MissingValue("--days".into()))
        );
        assert_eq!(
            Options::parse(["--days", "many"]),
            Err(CliError::InvalidValue {
                option: "--days".into(),
                value: "many".into()
            })
        );
        assert_eq!(
            Options::parse(["--time-scale", "0"]),
            Err(CliError::InvalidValue {
                option: "--time-scale".into(),
                value: "0".into()
            })
        );
        assert_eq!(
            Options::parse(["--seconds-per-day", "0"]),
            Err(CliError::Calendar(sim::CalendarError::ZeroSecondsPerDay))
        );
        assert_eq!(Options::parse(["--help"]), Err(CliError::HelpRequested));
    }

    #[test]
    fn reset_is_parsed() {
        assert!(Options::parse(["--reset"]).unwrap().reset);
        assert!(!Options::parse(Vec::<&str>::new()).unwrap().reset);
    }

    #[test]
    fn content_directory_is_parsed() {
        assert_eq!(
            Options::parse(["--content", "/srv/content"])
                .unwrap()
                .content,
            Some(PathBuf::from("/srv/content"))
        );
        assert_eq!(Options::parse(Vec::<&str>::new()).unwrap().content, None);
    }

    #[test]
    fn max_connections_is_parsed_and_must_be_positive() {
        assert_eq!(
            Options::parse(["--max-connections", "3"])
                .unwrap()
                .max_connections,
            3
        );
        assert_eq!(
            Options::parse(["--max-connections", "0"]),
            Err(CliError::InvalidValue {
                option: "--max-connections".into(),
                value: "0".into(),
            })
        );
    }
}
