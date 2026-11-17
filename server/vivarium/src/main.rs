//! `vivarium`: runs a world under glass and writes its report.

use std::path::PathBuf;
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
  --children N             children at each camp too, 3 to 11 (default 0)
  --infants N              infants of half a year at each camp, each carried
                           by one of its women (default 0)
  --tribes N               1 or 2 tribes, each at its own camp (default: 2 on
                           the vivarium's world, 1 on a patch)
  --days N                 game days to run (default 10)
  --cadence N              ticks a game second (default 1; the game runs at 10)
  --seed N                 the run's seed (default 1)
  --no-brain               the control run: nobody thinks, the people stand
  --content DIR            the content directory (default: content/, ../content/
                           or ../../content/, the first holding materials.ron)
  --out FILE               the report (default vivarium-report.html)
  --help                   this";

/// What the command line asks.
struct Options {
    settings: Settings,
    out: PathBuf,
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut settings = Settings::default();
    let mut side = 50;
    let mut world = "patch".to_owned();
    let mut out = PathBuf::from("vivarium-report.html");
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
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(None),
            "--world" => world = value(&mut i, "--world")?,
            "--side" => {
                side = u32::try_from(number(value(&mut i, "--side")?, "--side")?)
                    .map_err(|_| "--side: too large".to_owned())?;
            }
            "--people" => {
                settings.people = u32::try_from(number(value(&mut i, "--people")?, "--people")?)
                    .map_err(|_| "--people: too many".to_owned())?;
            }
            "--children" => {
                settings.children =
                    u32::try_from(number(value(&mut i, "--children")?, "--children")?)
                        .map_err(|_| "--children: too many".to_owned())?;
            }
            "--infants" => {
                settings.infants = u32::try_from(number(value(&mut i, "--infants")?, "--infants")?)
                    .map_err(|_| "--infants: too many".to_owned())?;
            }
            "--days" => {
                settings.days = u32::try_from(number(value(&mut i, "--days")?, "--days")?)
                    .map_err(|_| "--days: too many".to_owned())?;
            }
            "--cadence" => {
                settings.ticks_per_second =
                    u32::try_from(number(value(&mut i, "--cadence")?, "--cadence")?)
                        .map_err(|_| "--cadence: too high".to_owned())?;
            }
            "--no-brain" => settings.brain = false,
            "--tribes" => {
                tribes = Some(
                    u32::try_from(number(value(&mut i, "--tribes")?, "--tribes")?)
                        .map_err(|_| "--tribes: 1 or 2".to_owned())?,
                );
            }
            "--seed" => settings.seed = number(value(&mut i, "--seed")?, "--seed")?,
            "--content" => settings.content = Some(PathBuf::from(value(&mut i, "--content")?)),
            "--out" => out = PathBuf::from(value(&mut i, "--out")?),
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
    Ok(Some(Options { settings, out }))
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
    if let Err(error) = std::fs::write(&options.out, report.html()) {
        eprintln!("vivarium: cannot write {}: {error}", options.out.display());
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
