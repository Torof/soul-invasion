//! `vivarium`: runs a world under glass and writes its report.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vivarium::{Settings, WorldKind, run};

const USAGE: &str = "\
vivarium [options]   runs a world of the simulation headless and accelerated,
                     and writes an HTML report (D41)

  --world vivarium|patch   the vivarium's world (zone 2 of seed 14, some six
                           seconds to be born), or a patch of the valley
                           (default: patch)
  --side N                 the patch's side in tiles (default 50)
  --people N               people born at each camp (default 5): grown
                           women and men in turn, 18 to 40 years old
  --ages LO-HI             the grown people's ages, spread from LO to HI
                           years (default 18-40; 20-20 for a couple of 20)
  --fed                    food without limit: hunger a day is 0, nobody
                           grows thin (lot 3 C, trial 1)
  --children N             children at each camp too, 3 to 11 (default 0)
  --infants N              infants of half a year at each camp, each carried
                           by one of its women (default 0)
  --tribes N               1 or 2 tribes, each at its own camp (default: 2 on
                           the vivarium's world, 1 on a patch)
  --days N                 game days to run (default 10)
  --cadence N              ticks a game second (default 1; the game runs at 10)
  --seed N                 the run's seed (default 1)
  --no-brain               the control run: nobody thinks, the people stand
  --no-trials              the control run of D50: nobody tries a way nor
                           copies one (the content's trials replaced by none)
  --no-fire                the control run of D55: no evening fire, nobody
                           tells nor hears anything (the content's fire
                           replaced by none)
  --no-decisions           the control run of D56: the fire tells and hears
                           but nobody says what it will do tomorrow (the
                           content's decisions replaced by none)
  --no-continent-cache     bear the vivarium's continent anew instead of
                           reading it from ${XDG_CACHE_HOME:-~/.cache}/soul-sim/
                           continents/ (where a run keeps it; same pages)
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
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut settings = Settings::default();
    let mut side = 50;
    let mut world = "patch".to_owned();
    let mut out = PathBuf::from("vivarium-report.html");
    let mut full_page = false;
    let mut tribes: Option<u32> = None;
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
            "--children" => settings.children = small(&mut i, "--children", "too many")?,
            "--infants" => settings.infants = small(&mut i, "--infants", "too many")?,
            "--days" => settings.days = small(&mut i, "--days", "too many")?,
            "--cadence" => settings.ticks_per_second = small(&mut i, "--cadence", "too high")?,
            "--no-brain" => settings.brain = false,
            "--no-trials" => settings.trials = false,
            "--no-fire" => settings.fire = false,
            "--no-decisions" => settings.decisions = false,
            "--no-continent-cache" => settings.continents = None,
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
    settings.world = match world.as_str() {
        "vivarium" => WorldKind::Vivarium,
        "patch" => WorldKind::Patch { side },
        other => return Err(format!("--world: vivarium or patch, not {other}")),
    };
    settings.tribes = tribes.unwrap_or(match settings.world {
        WorldKind::Vivarium => 2,
        WorldKind::Patch { .. } => 1,
    });
    if !(1..=2).contains(&settings.tribes) {
        return Err("--tribes: 1 or 2".to_owned());
    }
    if side < 8 {
        return Err("--side: 8 tiles at least".to_owned());
    }
    Ok(Some(Options {
        settings,
        out,
        full_page,
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
        assert!(parse(&args(&["--ages", "20"])).is_err());
        assert!(parse(&args(&["--ages", "30-20"])).is_err());
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
