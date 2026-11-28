//! `vivarium`: runs a world under glass and writes its report.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vivarium::{Settings, Start, WorldKind, prepare, run};

const USAGE: &str = "\
vivarium [options]   runs a world of the simulation headless and accelerated,
                     and writes an HTML report (D41)

  --world vivarium|vivarium-wide|vivarium-huge|patch
                           the vivarium's world (zone 2 of seed 14, some six
                           seconds to be born), the same twice as wide
                           (4,352 m a side), the same at the scale of a
                           valley (16,384 m a side), or a patch of the
                           valley (default: patch)
  --side N                 the patch's side in tiles (default 50)
  --people N               people born at each camp (default 5): the whole
                           band with the pyramid start; the grown alone,
                           women and men in turn, with the founders' start
  --start pyramid|founders how a camp is born (default: pyramid, the life
                           lot): a band of every age shaped on the Hadza
                           and the !Kung, some 44 % under sixteen, children
                           with their mothers, couples close in age, some
                           unwed, reserves unequal, a few women with child;
                           or the founders of lot 3 C, grown of --ages all
                           wed the first evening, reserves full. --ages,
                           --children and --infants mean the founders
  --ages LO-HI             the founders' ages, spread from LO to HI years
                           (default 18-40; 20-20 for a couple of 20)
  --fed                    food without limit: hunger a day is 0, nobody
                           grows thin (lot 3 C, trial 1)
  --children N             the founders' children at each camp too, 3 to 11
                           (default 0)
  --infants N              the founders' infants of half a year at each
                           camp, each carried by one of its women (default 0)
  --tribes N               1 or 2 tribes, each at its own camp (default: 2 on
                           the vivarium's world, 1 on a patch)
  --days N                 game days to run (default 10)
  --cadence N              ticks a game second (default 1; the game runs at 10)
  --seconds-per-day N      game seconds a game day (default 16200, four
                           hours and a half: calendar v1)
  --days-per-cycle N       game days in a cycle of four seasons, the nature
                           clock (default 64; at least 4)
  --age-years-per-cycle N  age-years in a cycle, the clock of ages (default
                           8: an age-year of 8 days; 1 makes the age-year
                           the cycle); it must cut the cycle in whole
                           age-years of 4 days at least
  --seed N                 the run's seed (default 1)
  --no-brain               the control run: nobody thinks, the people stand
  --no-trials              the control run of D50: nobody tries a way nor
                           copies one (the content's trials replaced by none)
  --hands-known N          a diagnostic of D57: the first N grown founders of
                           each tribe know the deeds of a container from
                           birth (default 0, the experiment: nobody knows)
  --no-fire                the control run of D55: no evening fire, nobody
                           tells nor hears anything (the content's fire
                           replaced by none)
  --no-decisions           the control run of D56: the fire tells and hears
                           but nobody says what it will do tomorrow (the
                           content's decisions replaced by none)
  --no-moves               the control run of moving as a consequence:
                           nobody is surprised, nobody moves home (the
                           content's moving section replaced by none)
  --no-pooling             the control run of D58: nothing pooled at the
                           hearth, each eats what it gathers, a weaned
                           child gathers for itself (the content's
                           pooling section replaced by none)
  --no-care                pool at the hearth without the care for the
                           young's coming days (pooling.care_days 0)
  --no-trips               the control run of the point of no return: nobody
                           chooses to go home, stay out or go on; a body
                           gathers until night and walks home in the dark
                           (the content's trips section replaced by none)
  --no-problems            the control run of the problems (D50, amended
                           26 November 2026): nothing is remembered as a
                           problem, no trial begins from one (the
                           content's problems section replaced by none)
  --wake-all               the control run of the hearth's wake (D58,
                           amended 26 November 2026): food laid at or taken
                           from a hearth's reserve wakes everyone of that
                           hearth at the camp, asleep or not, as before
                           (thought.hearth_wakes true)
  --old-wakes              the control run of the audit of wakes (26
                           November 2026): every switch of its steps at the
                           rule before (thought.wake_on_interrupt and
                           thought.wake_asleep true,
                           decisions.behind_glance false,
                           pooling.hearth_look,
                           pooling.young_seen and
                           pooling.giver_care,
                           temperance.kin_seen false)
  --old-illness            the control run of illness as episodes (D49,
                           amended 26 November 2026): the old rule, a
                           background death by age alone (the content's
                           illness section replaced by none)
  --no-passages            the control run of the passages: nobody keeps,
                           tells or walks through a passage (the content's
                           passages section replaced by none)
  --no-learning            the control run of learning causes (D59): no
                           episode, no belief (the content's learning
                           section replaced by none, the problems it marks
                           taken out)
  --learning-believe-only  learning causes believed without acting (D59,
                           amended 27 November 2026): no lever, no weight
                           on moves, no trial (the content's
                           learning.acting replaced by none)
  --no-search              the control run of the search for an absent kin
                           (D56, amended 27 November 2026): nobody notices
                           an absent, nobody goes (the content's search
                           section replaced by none)
  --journal FILE           also write the journal: every news of the brains and the
                           world's events of things and lives, a JSON line each
  --no-continent-cache     bear the vivarium's continent anew instead of
                           reading it from ${XDG_CACHE_HOME:-~/.cache}/soul-sim/
                           continents/ (where a run keeps it; same pages)
  --no-land-cache          make every chunk of land (tiles, woods, water)
                           and place every tree and plant anew instead of
                           reading those kept under
                           ${XDG_CACHE_HOME:-~/.cache}/soul-sim/lands/ (a
                           file a world and a land-making code, the plants'
                           also a content, checked when read; a run that
                           finds none prepares it on all cores; same pages)
  --prepare-land           simulate nothing: make the whole land of --world,
                           its trees and its plants (with --content) on all
                           cores and keep them, for the runs of a series to
                           read (done once per world, content and
                           land-making code; seeds share it)
  --helpers N              threads placing the chunks' plants ahead of the
                           bodies (default 1; 0 = the simulation thread
                           alone, for runs side by side); the pages are the
                           same either way
  --content DIR            the content directory (default: content/, ../content/
                           or ../../content/, the first holding materials.ron)
  --out FILE               the report (default vivarium-report.html); its
                           data is written beside it, FILE with .json
  --page minimal|full      the page: minimal (default since lab 6: the
                           settings, the end, the map; the lab reads the
                           data) or full (the long page of before)
  --help                   this";

/// What the command line asks.
struct Options {
    settings: Settings,
    out: PathBuf,
    /// The long page of before (`--page full`) rather than the minimal one.
    full_page: bool,
    /// Prepare the land and keep it, run nothing (`--prepare-land`).
    prepare: bool,
}

#[allow(
    clippy::too_many_lines,
    reason = "one flag a line, read top to bottom; splitting it would scatter the flags"
)]
fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut settings = Settings::default();
    let mut side = 50;
    let mut world = "patch".to_owned();
    let mut out = PathBuf::from("vivarium-report.html");
    let mut full_page = false;
    let mut prepare = false;
    let mut tribes: Option<u32> = None;
    // The start asked, and whether a founders' flag was given.
    let mut start: Option<Start> = None;
    let mut founders_flag = false;
    let mut i = 0;
    let value = |i: &mut usize, name: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{name} needs a value"))
    };
    let number = |text: String, name: &str| -> Result<u64, String> {
        text.parse()
            .map_err(|_| format!("{name}: not a number: {text}"))
    };
    let small = |i: &mut usize, name: &str, too: &str| -> Result<u32, String> {
        u32::try_from(number(value(i, name)?, name)?).map_err(|_| format!("{name}: {too}"))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(None),
            "--world" => world = value(&mut i, "--world")?,
            "--side" => side = small(&mut i, "--side", "too large")?,
            "--people" => settings.people = small(&mut i, "--people", "too many")?,
            "--children" => {
                settings.children = small(&mut i, "--children", "too many")?;
                founders_flag = true;
            }
            "--infants" => {
                settings.infants = small(&mut i, "--infants", "too many")?;
                founders_flag = true;
            }
            "--start" => {
                start = Some(match value(&mut i, "--start")?.as_str() {
                    "pyramid" => Start::Pyramid,
                    "founders" => Start::Founders,
                    other => return Err(format!("--start: pyramid or founders, not {other}")),
                });
            }
            "--days" => settings.days = small(&mut i, "--days", "too many")?,
            "--cadence" => settings.ticks_per_second = small(&mut i, "--cadence", "too high")?,
            "--seconds-per-day" => {
                settings.seconds_per_day = small(&mut i, "--seconds-per-day", "too long")?;
            }
            "--days-per-cycle" => {
                settings.days_per_cycle = small(&mut i, "--days-per-cycle", "too long")?;
            }
            "--age-years-per-cycle" => {
                settings.age_years_per_cycle = small(&mut i, "--age-years-per-cycle", "too many")?;
            }
            "--no-brain" => settings.brain = false,
            "--no-trials" => settings.trials = false,
            "--hands-known" => {
                settings.hands_known = small(&mut i, "--hands-known", "too many")?;
            }
            "--no-fire" => settings.fire = false,
            "--no-decisions" => settings.decisions = false,
            "--no-moves" => settings.moves = false,
            "--no-pooling" => settings.pooling = false,
            "--no-care" => settings.care = false,
            "--no-trips" => settings.trips = false,
            "--no-problems" => settings.problems = false,
            "--wake-all" => settings.wake_all = true,
            "--old-wakes" => settings.old_wakes = true,
            "--old-illness" => settings.old_illness = true,
            "--no-passages" => settings.passages = false,
            "--no-learning" => settings.learning = false,
            "--learning-believe-only" => settings.learning_act = false,
            "--no-search" => settings.search = false,
            "--journal" => settings.journal = Some(PathBuf::from(value(&mut i, "--journal")?)),
            "--no-continent-cache" => settings.continents = None,
            "--no-land-cache" => settings.lands = None,
            "--prepare-land" => prepare = true,
            "--helpers" => settings.helpers = small(&mut i, "--helpers", "too many")?,
            "--fed" => settings.fed = true,
            "--ages" => {
                let text = value(&mut i, "--ages")?;
                let (lo, hi) = text
                    .split_once('-')
                    .ok_or_else(|| format!("--ages: LO-HI, not {text}"))?;
                let years = |part: &str| -> Result<u32, String> {
                    u32::try_from(number(part.to_owned(), "--ages")?)
                        .map_err(|_| "--ages: too old".to_owned())
                };
                settings.adult_years = (years(lo)?, years(hi)?);
                founders_flag = true;
                if settings.adult_years.0 > settings.adult_years.1 {
                    return Err(format!("--ages: LO above HI: {text}"));
                }
            }
            "--tribes" => {
                tribes = Some(
                    u32::try_from(number(value(&mut i, "--tribes")?, "--tribes")?)
                        .map_err(|_| "--tribes: 1 or 2".to_owned())?,
                );
            }
            "--seed" => settings.seed = number(value(&mut i, "--seed")?, "--seed")?,
            "--content" => settings.content = Some(PathBuf::from(value(&mut i, "--content")?)),
            "--out" => out = PathBuf::from(value(&mut i, "--out")?),
            "--page" => {
                full_page = match value(&mut i, "--page")?.as_str() {
                    "minimal" => false,
                    "full" => true,
                    other => return Err(format!("--page: minimal or full, not {other}")),
                };
            }
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
        i += 1;
    }
    settings.start = match (start, founders_flag) {
        (Some(Start::Pyramid), true) => {
            return Err(
                "--ages, --children and --infants are the founders' start, not the pyramid's"
                    .to_owned(),
            );
        }
        (Some(start), _) => start,
        (None, true) => Start::Founders,
        (None, false) => Start::Pyramid,
    };
    settings.world = match world.as_str() {
        "vivarium" => WorldKind::Vivarium,
        "vivarium-wide" => WorldKind::VivariumWide,
        "vivarium-huge" => WorldKind::VivariumHuge,
        "patch" => WorldKind::Patch { side },
        other => {
            return Err(format!(
                "--world: vivarium, vivarium-wide, vivarium-huge or patch, not {other}"
            ));
        }
    };
    settings.tribes = tribes.unwrap_or(match settings.world {
        WorldKind::Vivarium | WorldKind::VivariumWide | WorldKind::VivariumHuge => 2,
        WorldKind::Patch { .. } => 1,
    });
    if !(1..=2).contains(&settings.tribes) {
        return Err("--tribes: 1 or 2".to_owned());
    }
    if side < 8 {
        return Err("--side: 8 tiles at least".to_owned());
    }
    settings
        .calendar()
        .map_err(|e| format!("the calendar: {e}"))?;
    Ok(Some(Options {
        settings,
        out,
        full_page,
        prepare,
    }))
}

/// Where the report's data goes: beside the page, the same stem with
/// `.json` (`r.html` → `r.json`, `report` → `report.json`).
fn json_path(out: &Path) -> PathBuf {
    out.with_extension("json")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    if options.prepare {
        return match prepare(&options.settings) {
            Ok(line) => {
                println!("{line}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("vivarium: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let report = match run(&options.settings) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("vivarium: {error}");
            return ExitCode::FAILURE;
        }
    };
    let page = if options.full_page {
        report.full_html()
    } else {
        report.html()
    };
    if let Err(error) = std::fs::write(&options.out, page) {
        eprintln!("vivarium: cannot write {}: {error}", options.out.display());
        return ExitCode::FAILURE;
    }
    let data = json_path(&options.out);
    if let Err(error) = std::fs::write(&data, report.json()) {
        eprintln!("vivarium: cannot write {}: {error}", data.display());
        return ExitCode::FAILURE;
    }
    let alive = report.samples.last().map_or(0, |s| s.alive);
    let dead = report.people.values().filter(|p| p.death.is_some()).count();
    println!(
        "{}: {} days, {} people: {alive} alive, {dead} dead; born in {:.2} s, run in {:.2} s",
        options.out.display(),
        report.settings.days,
        report.settings.people,
        report.birth_s,
        report.wall_s
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_data_goes_beside_the_page() {
        assert_eq!(json_path(Path::new("r.html")), PathBuf::from("r.json"));
        assert_eq!(
            json_path(Path::new("out/report")),
            PathBuf::from("out/report.json")
        );
        assert_eq!(
            json_path(Path::new("a.b/vivarium-report.html")),
            PathBuf::from("a.b/vivarium-report.json")
        );
    }

    #[test]
    fn the_defaults_are_the_small_trial() {
        let options = parse(&args(&[])).unwrap().unwrap();
        assert_eq!(options.settings, Settings::default());
        assert_eq!(options.out, PathBuf::from("vivarium-report.html"));
    }

    #[test]
    fn every_option_is_read() {
        let options = parse(&args(&[
            "--world",
            "vivarium",
            "--people",
            "150",
            "--days",
            "100",
            "--cadence",
            "10",
            "--seed",
            "14",
            "--content",
            "/c",
            "--out",
            "r.html",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.settings.world, WorldKind::Vivarium);
        assert_eq!(
            options.settings.tribes, 2,
            "two tribes by default on the vivarium's world"
        );
        assert_eq!(options.settings.people, 150);
        assert_eq!(options.settings.days, 100);
        assert_eq!(options.settings.ticks_per_second, 10);
        assert_eq!(options.settings.seed, 14);
        assert_eq!(options.settings.content, Some(PathBuf::from("/c")));
        assert_eq!(options.out, PathBuf::from("r.html"));
        let patch = parse(&args(&["--side", "80"])).unwrap().unwrap();
        assert_eq!(patch.settings.world, WorldKind::Patch { side: 80 });
        let couple = parse(&args(&["--ages", "20-20", "--fed", "--people", "2"]))
            .unwrap()
            .unwrap();
        assert_eq!(couple.settings.adult_years, (20, 20));
        assert!(couple.settings.fed);
        assert_eq!(
            couple.settings.start,
            Start::Founders,
            "--ages: the founders"
        );
        assert!(parse(&args(&["--ages", "20"])).is_err());
        assert!(parse(&args(&["--ages", "30-20"])).is_err());
    }

    #[test]
    fn the_start_is_the_pyramid_unless_the_founders_are_asked() {
        let start = |a: &[&str]| parse(&args(a)).map(|o| o.unwrap().settings.start);
        assert_eq!(start(&[]), Ok(Start::Pyramid));
        assert_eq!(start(&["--people", "25"]), Ok(Start::Pyramid));
        assert_eq!(start(&["--start", "founders"]), Ok(Start::Founders));
        assert_eq!(start(&["--children", "3"]), Ok(Start::Founders));
        assert_eq!(start(&["--infants", "1"]), Ok(Start::Founders));
        assert_eq!(
            start(&["--start", "founders", "--children", "3"]),
            Ok(Start::Founders)
        );
        assert!(start(&["--children", "3", "--start", "pyramid"]).is_err());
        assert!(start(&["--start", "elders"]).is_err());
    }

    #[test]
    fn the_calendar_is_read_and_checked() {
        let defaults = parse(&args(&[])).unwrap().unwrap().settings;
        assert_eq!(
            defaults.calendar().unwrap(),
            sim::Calendar::new(1, 16_200).unwrap(),
            "the calendar v1 at the vivarium's cadence"
        );
        let v1 = parse(&args(&[
            "--seconds-per-day",
            "16200",
            "--days-per-cycle",
            "64",
            "--age-years-per-cycle",
            "8",
        ]))
        .unwrap()
        .unwrap()
        .settings;
        let calendar = v1.calendar().unwrap();
        assert_eq!(calendar.seconds_per_day(), 16_200);
        assert_eq!(calendar.days_per_cycle(), 64);
        assert_eq!(calendar.days_per_age_year(), 8);
        // A cycle not cut in whole age-years of four days is refused.
        assert!(
            parse(&args(&[
                "--days-per-cycle",
                "50",
                "--age-years-per-cycle",
                "8"
            ]))
            .is_err()
        );
        assert!(parse(&args(&["--days-per-cycle", "3"])).is_err());
        assert!(parse(&args(&["--seconds-per-day", "0"])).is_err());
    }

    #[test]
    fn bad_options_are_refused_and_help_is_told() {
        assert!(parse(&args(&["--help"])).unwrap().is_none());
        assert!(
            !parse(&args(&["--no-brain"]))
                .unwrap()
                .unwrap()
                .settings
                .brain
        );
        assert!(
            !parse(&args(&["--no-trials"]))
                .unwrap()
                .unwrap()
                .settings
                .trials
        );
        assert!(
            parse(&args(&["--no-land-cache"]))
                .unwrap()
                .unwrap()
                .settings
                .lands
                .is_none()
        );
        assert!(parse(&args(&[])).unwrap().unwrap().settings.lands.is_some());
        assert!(parse(&args(&["--prepare-land"])).unwrap().unwrap().prepare);
        assert!(!parse(&args(&[])).unwrap().unwrap().prepare);
        assert!(
            parse(&args(&["--no-continent-cache"]))
                .unwrap()
                .unwrap()
                .settings
                .continents
                .is_none()
        );
        assert!(
            parse(&args(&[]))
                .unwrap()
                .unwrap()
                .settings
                .continents
                .is_some()
        );
        assert_eq!(
            parse(&args(&["--helpers", "0"]))
                .unwrap()
                .unwrap()
                .settings
                .helpers,
            0
        );
        assert!(parse(&args(&["--helpers", "two"])).is_err());
        assert!(parse(&args(&["--days"])).is_err());
        assert!(parse(&args(&["--days", "ten"])).is_err());
        assert!(parse(&args(&["--world", "mars"])).is_err());
        assert!(parse(&args(&["--side", "4"])).is_err());
        assert!(parse(&args(&["--bogus"])).is_err());
        assert!(parse(&args(&["--tribes", "3"])).is_err());
        assert_eq!(
            parse(&args(&["--tribes", "2"]))
                .unwrap()
                .unwrap()
                .settings
                .tribes,
            2
        );
    }
}
