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
    --chunk-threads <N>    Paced mode: threads building the chunks of the
                           clients' views, beside the network (default: the
                           machine's hardware threads less three, from 1 to
                           8).
    --content <DIR>        Directory of the content files (materials,
                           spawn rules). Default: the first of content,
                           ../content, ../../content that exists.
    --ticks-per-second <N> Calendar: simulation ticks per game second
                           (default 10).
    --seconds-per-day <N>  Calendar: game seconds per game day (default 7200).
    --days-per-year <N>    Calendar: game days per year, four seasons
                           (default 50, at least 4).
    --valley-size <N>      A new world's valley, N tiles on a side (default
                           3072, at least 256). A smaller one starts far
                           faster: for tests and development.
    --world flat:<N>       A test world instead of the valley: N × N tiles
                           of grass (at least 8), the camp in the middle.
    --world test:marches   The steps test: 30 × 30 tiles of grass with a
                           ridge about 3 m high from the screen's left
                           corner to its right one, shown by the client
                           only. Never for play.
    --world test:montagne  The mountain test: 192 × 192 tiles of grass cut
                           from a large mountain's flank, shown by the
                           client only. Never for play.
    --world test:massif    The massif test: 1024 × 1024 tiles of grass
                           holding the whole mountain the mountain test
                           cuts its window from, its 270 m summit at the
                           middle, a flat margin at its edge, shown by the
                           client only. Never for play.
    --world test:formes    The shapes test: 256 × 256 tiles of grass, flat
                           at 50 m, with seven shapes on their own meadows
                           (three crests, a valley, a cone, a plane whose
                           slope changes, one that turns), shown by the
                           client only. Never for play.
    --world test:sens      The sense test: 256 × 256 tiles of grass, flat
                           at 50 m, with six crests and a valley across the
                           screen, their profiles chosen to tell which way a
                           slope rises (a flat top and a shoulder, a sharp
                           top, two dissymmetric ridges, a col, a valley with
                           a flat floor), shown by the client only. Never
                           for play.
    --world test:relief    The relief test, the sandbox's world: 512 × 512
                           tiles of grass flat at 50 m holding the shapes
                           test (x 0–256, y 0–256), the sense test
                           (x 256–512, y 0–256) and the mountain test
                           (x 32–224, y 288–480, levelled to 50 m at its
                           window's edge), shown by the client only. Never
                           for play.
    --world plan:<SEED>    Experimental: a world cut from the atlas of SEED,
                           6 km a side with a height per tile (generator
                           version 3, the relief look test; some seconds to
                           make). Never for play.
    --dev                  Development tools: every client gets an overview
                           of the whole map and may teleport its body. Never
                           for play.
    --dump-chunk <X,Y>     Development: print chunk (X, Y) as a client
                           decodes it from its binary frame, then exit.
    --no-warm-up           Paced mode: listen at once, without placing the
                           plants of the view around the camp first; the
                           first client waits for them instead. For tests.
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
    /// Threads of the chunk pool, or `None` to size it from the machine.
    pub chunk_threads: Option<usize>,
    /// Content directory, or `None` to look in the default places.
    pub content: Option<PathBuf>,
    /// The world's calendar.
    pub calendar: Calendar,
    /// Development tools on (`--dev`): the map's overview and teleports.
    pub dev: bool,
    /// Development (`--dump-chunk X,Y`): print that chunk as a client
    /// decodes it, then exit.
    pub dump_chunk: Option<(i32, i32)>,
    /// A new world's valley, tiles on a side.
    pub valley_size: u32,
    /// A test world instead of the valley (`--world`); always ephemeral.
    pub world: Option<TestWorld>,
    /// When the plants of the view around the camp are placed (paced mode).
    pub warm_up: WarmUp,
}

/// A tiny world for a visual test, instead of the valley (`--world`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestWorld {
    /// `flat:<N>`: N × N tiles of grass.
    Flat(u32),
    /// `test:marches`: the steps test's ridge.
    Marches,
    /// `test:montagne`: a window on a large mountain's flank.
    Mountain,
    /// `test:massif`: the whole mountain, cut to 1024 tiles on a side.
    Massif,
    /// `test:formes`: simple shapes, one on each meadow.
    Shapes,
    /// `test:sens`: crests across the screen whose profiles tell which
    /// way a slope rises.
    Sense,
    /// `test:relief`: the shapes, sense and mountain tests on one map,
    /// the sandbox's world.
    Relief,
    /// `plan:<SEED>`: a window of the atlas of that seed (the relief look
    /// test, generator version 3).
    Plan(u64),
}

/// When a paced server places the plants of the view around the camp,
/// where the first bodies stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmUp {
    /// Before it listens, so the first client is not kept waiting: the game.
    BeforeListening,
    /// When a client first asks for them (`--no-warm-up`): the tests,
    /// whose servers rarely do and start twenty at once.
    OnDemand,
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
            chunk_threads: None,
            content: None,
            calendar: Calendar::default(),
            dev: false,
            dump_chunk: None,
            valley_size: sim::VALLEY_SIZE,
            world: None,
            warm_up: WarmUp::BeforeListening,
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
        let mut days_per_year = Calendar::DEFAULT_DAYS_PER_YEAR;
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
                "--dump-chunk" => {
                    let raw: String = value(arg, args.next())?;
                    let parsed = raw
                        .split_once(',')
                        .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                    opts.dump_chunk = Some(parsed.ok_or(CliError::InvalidValue {
                        option: arg.to_owned(),
                        value: raw,
                    })?);
                }
                "--valley-size" => opts.valley_size = value(arg, args.next())?,
                "--world" => {
                    let raw: String = value(arg, args.next())?;
                    let world = match raw.as_str() {
                        "test:marches" => Some(TestWorld::Marches),
                        "test:montagne" => Some(TestWorld::Mountain),
                        "test:massif" => Some(TestWorld::Massif),
                        "test:formes" => Some(TestWorld::Shapes),
                        "test:sens" => Some(TestWorld::Sense),
                        "test:relief" => Some(TestWorld::Relief),
                        _ => {
                            if let Some(seed) = raw.strip_prefix("plan:") {
                                seed.trim().parse().ok().map(TestWorld::Plan)
                            } else {
                                raw.strip_prefix("flat:")
                                    .and_then(|n| n.parse().ok())
                                    .filter(|&n: &u32| (8..=4096).contains(&n))
                                    .map(TestWorld::Flat)
                            }
                        }
                    };
                    opts.world = Some(world.ok_or(CliError::InvalidValue {
                        option: arg.to_owned(),
                        value: raw,
                    })?);
                }
                "--no-warm-up" => opts.warm_up = WarmUp::OnDemand,
                "--time-scale" => opts.time_scale = value(arg, args.next())?,
                "--max-connections" => opts.max_connections = value(arg, args.next())?,
                "--chunk-threads" => opts.chunk_threads = Some(value(arg, args.next())?),
                "--content" => opts.content = Some(value(arg, args.next())?),
                "--ticks-per-second" => ticks_per_second = value(arg, args.next())?,
                "--seconds-per-day" => seconds_per_day = value(arg, args.next())?,
                "--days-per-year" => days_per_year = value(arg, args.next())?,
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
        if opts.chunk_threads == Some(0) {
            return Err(CliError::InvalidValue {
                option: "--chunk-threads".to_owned(),
                value: "0".to_owned(),
            });
        }
        if opts.valley_size < 256 {
            return Err(CliError::InvalidValue {
                option: "--valley-size".to_owned(),
                value: opts.valley_size.to_string(),
            });
        }
        if opts.world.is_some() {
            // A test world is never saved: its heights live in no snapshot.
            opts.data = DataLocation::Ephemeral;
        }
        opts.calendar = Calendar::new(ticks_per_second, seconds_per_day)
            .and_then(|c| c.with_days_per_year(days_per_year))
            .map_err(CliError::Calendar)?;
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
            "--days-per-year",
            "12",
        ])
        .unwrap();
        assert!(opts.headless);
        assert_eq!(opts.days, Some(3));
        assert_eq!(
            opts.calendar,
            Calendar::new(4, 30)
                .unwrap()
                .with_days_per_year(12)
                .unwrap()
        );
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
        assert_eq!(
            Options::parse(["--dump-chunk", "3,-4"]).unwrap().dump_chunk,
            Some((3, -4))
        );
        assert!(Options::parse(["--dump-chunk", "3"]).is_err());
    }

    #[test]
    fn the_atlas_world_is_asked_for_by_its_seed() {
        assert_eq!(Options::parse(Vec::<&str>::new()).unwrap().world, None);
        assert_eq!(
            Options::parse(["--world", "plan:7"]).unwrap().world,
            Some(TestWorld::Plan(7))
        );
        assert!(Options::parse(["--world", "valley"]).is_err());
        assert!(Options::parse(["--world", "plan:x"]).is_err());
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
    fn the_warm_up_is_on_unless_turned_off() {
        assert_eq!(
            Options::parse(Vec::<&str>::new()).unwrap().warm_up,
            WarmUp::BeforeListening
        );
        assert_eq!(
            Options::parse(["--no-warm-up"]).unwrap().warm_up,
            WarmUp::OnDemand
        );
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
        assert_eq!(
            Options::parse(["--days-per-year", "3"]),
            Err(CliError::Calendar(sim::CalendarError::TooShortYear))
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
    fn chunk_threads_is_parsed_and_must_be_positive() {
        assert_eq!(
            Options::parse(Vec::<&str>::new()).unwrap().chunk_threads,
            None
        );
        assert_eq!(
            Options::parse(["--chunk-threads", "3"])
                .unwrap()
                .chunk_threads,
            Some(3)
        );
        assert_eq!(
            Options::parse(["--chunk-threads", "0"]),
            Err(CliError::InvalidValue {
                option: "--chunk-threads".into(),
                value: "0".into(),
            })
        );
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
