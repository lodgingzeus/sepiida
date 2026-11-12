//! Command-line parsing. Kept free of side effects so it can be tested.

use sepiida_core::{Color, accent::ThemeMode, config::Config, source::SourceChoice};

pub const USAGE: &str = "\
usage:
  sepiida                          open the settings window
  sepiida palette <image>          print the palette for an image
  sepiida apply [options]          theme Windows from the current wallpaper
  sepiida run                      stay in the background and re-theme when the wallpaper changes
  sepiida restore [--dry-run]      put back the accent colour you had before sepiida

apply options:
  --source auto|windows|wallpaper-engine   where to read the wallpaper (default: from settings)
  --mode light|dark                        also switch light/dark mode (default: from settings)
  --dry-run                                print what would change without changing it

`apply` follows the settings file: a locked colour is used instead of the
wallpaper, and nothing is written for features that are turned off.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Settings,
    Help,
    Palette { image: String },
    Apply(ApplyOptions),
    Restore { dry_run: bool },
    Run,
}

/// What was given on the command line. `None` means "use the setting".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApplyOptions {
    pub source: Option<SourceChoice>,
    pub mode: Option<ThemeMode>,
    pub dry_run: bool,
}

/// The settings one `apply` runs with: the settings file, overridden by
/// whatever was given on the command line.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "`apply` only runs on Windows")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplySettings {
    pub source: SourceChoice,
    /// `None` leaves the user's light/dark setting alone.
    pub mode: Option<ThemeMode>,
    pub accent: bool,
    pub locked_color: Option<Color>,
}

impl ApplyOptions {
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "`apply` only runs on Windows")
    )]
    pub fn settings(&self, config: &Config) -> ApplySettings {
        ApplySettings {
            source: self.source.unwrap_or(config.source),
            mode: self.mode.or(config.mode.theme_mode()),
            accent: config.accent,
            locked_color: config.locked_color,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("`palette` takes exactly one image path")]
    PaletteArguments,
    #[error("unknown option `{option}` for `{command}`")]
    UnknownOption {
        command: &'static str,
        option: String,
    },
    #[error("option `{0}` needs a value")]
    MissingValue(&'static str),
    #[error("option `{0}` was given more than once")]
    Repeated(&'static str),
    #[error("{0}")]
    InvalidValue(String),
}

pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Command, CliError> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    match args.as_slice() {
        [] => Ok(Command::Settings),
        ["help" | "--help" | "-h"] => Ok(Command::Help),
        ["palette", image] => Ok(Command::Palette {
            image: (*image).to_owned(),
        }),
        ["palette", ..] => Err(CliError::PaletteArguments),
        ["apply", options @ ..] => parse_apply(options).map(Command::Apply),
        ["run"] => Ok(Command::Run),
        ["restore"] => Ok(Command::Restore { dry_run: false }),
        ["restore", "--dry-run"] => Ok(Command::Restore { dry_run: true }),
        ["restore", "--dry-run", "--dry-run", ..] => Err(CliError::Repeated("--dry-run")),
        ["restore", other, ..] => Err(CliError::UnknownOption {
            command: "restore",
            option: (*other).to_owned(),
        }),
        [other, ..] => Err(CliError::UnknownCommand((*other).to_owned())),
    }
}

fn parse_apply(args: &[&str]) -> Result<ApplyOptions, CliError> {
    let mut source = None;
    let mut mode = None;
    let mut dry_run = false;

    let mut args = args.iter().copied();
    while let Some(arg) = args.next() {
        match arg {
            "--source" => {
                let value = args.next().ok_or(CliError::MissingValue("--source"))?;
                let parsed: SourceChoice =
                    value
                        .parse()
                        .map_err(|e: sepiida_core::source::UnknownSourceChoice| {
                            CliError::InvalidValue(e.to_string())
                        })?;
                if source.replace(parsed).is_some() {
                    return Err(CliError::Repeated("--source"));
                }
            }
            "--mode" => {
                let value = args.next().ok_or(CliError::MissingValue("--mode"))?;
                let parsed: ThemeMode =
                    value
                        .parse()
                        .map_err(|e: sepiida_core::accent::UnknownThemeMode| {
                            CliError::InvalidValue(e.to_string())
                        })?;
                if mode.replace(parsed).is_some() {
                    return Err(CliError::Repeated("--mode"));
                }
            }
            "--dry-run" => dry_run = true,
            other => {
                return Err(CliError::UnknownOption {
                    command: "apply",
                    option: other.to_owned(),
                });
            }
        }
    }

    Ok(ApplyOptions {
        source,
        mode,
        dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(args: &[&str]) -> Result<ApplyOptions, CliError> {
        match parse(args)? {
            Command::Apply(options) => Ok(options),
            other => panic!("expected apply, got {other:?}"),
        }
    }

    #[test]
    fn no_arguments_opens_settings() {
        assert_eq!(parse::<&str>(&[]), Ok(Command::Settings));
    }

    #[test]
    fn help_is_recognised() {
        for flag in ["help", "--help", "-h"] {
            assert_eq!(parse(&[flag]), Ok(Command::Help));
        }
    }

    #[test]
    fn palette_takes_exactly_one_path() {
        assert_eq!(
            parse(&["palette", "a b.png"]),
            Ok(Command::Palette {
                image: "a b.png".to_owned()
            })
        );
        assert_eq!(parse(&["palette"]), Err(CliError::PaletteArguments));
        assert_eq!(
            parse(&["palette", "a", "b"]),
            Err(CliError::PaletteArguments)
        );
    }

    #[test]
    fn apply_without_options_defers_to_the_settings() {
        assert_eq!(apply(&["apply"]), Ok(ApplyOptions::default()));
        assert_eq!(ApplyOptions::default().source, None);
        assert_eq!(ApplyOptions::default().mode, None);
        assert!(!ApplyOptions::default().dry_run);
    }

    #[test]
    fn default_settings_use_auto_and_leave_the_mode_alone() {
        let settings = ApplyOptions::default().settings(&Config::default());
        assert_eq!(
            settings,
            ApplySettings {
                source: SourceChoice::Auto,
                mode: None,
                accent: true,
                locked_color: None,
            }
        );
    }

    #[test]
    fn command_line_options_override_the_settings_file() {
        use sepiida_core::config::ModeSetting;

        let config = Config {
            source: SourceChoice::WallpaperEngine,
            mode: ModeSetting::Light,
            accent: false,
            locked_color: Some(Color::new(1, 2, 3)),
            ..Config::default()
        };
        let from_file = ApplyOptions::default().settings(&config);
        assert_eq!(from_file.source, SourceChoice::WallpaperEngine);
        assert_eq!(from_file.mode, Some(ThemeMode::Light));
        assert!(!from_file.accent);
        assert_eq!(from_file.locked_color, Some(Color::new(1, 2, 3)));

        let options = ApplyOptions {
            source: Some(SourceChoice::Windows),
            mode: Some(ThemeMode::Dark),
            dry_run: false,
        };
        let overridden = options.settings(&config);
        assert_eq!(overridden.source, SourceChoice::Windows);
        assert_eq!(overridden.mode, Some(ThemeMode::Dark));
    }

    #[test]
    fn apply_reads_all_options_in_any_order() {
        assert_eq!(
            apply(&[
                "apply",
                "--dry-run",
                "--mode",
                "dark",
                "--source",
                "windows"
            ]),
            Ok(ApplyOptions {
                source: Some(SourceChoice::Windows),
                mode: Some(ThemeMode::Dark),
                dry_run: true,
            })
        );
        assert_eq!(
            apply(&["apply", "--source", "wallpaper-engine"]).map(|o| o.source),
            Ok(Some(SourceChoice::WallpaperEngine))
        );
    }

    #[test]
    fn apply_rejects_bad_input() {
        assert_eq!(
            apply(&["apply", "--source"]),
            Err(CliError::MissingValue("--source"))
        );
        assert_eq!(
            apply(&["apply", "--mode"]),
            Err(CliError::MissingValue("--mode"))
        );
        assert!(matches!(
            apply(&["apply", "--source", "kde"]),
            Err(CliError::InvalidValue(_))
        ));
        assert!(matches!(
            apply(&["apply", "--mode", "auto"]),
            Err(CliError::InvalidValue(_))
        ));
        assert_eq!(
            apply(&["apply", "--source", "auto", "--source", "windows"]),
            Err(CliError::Repeated("--source"))
        );
        assert_eq!(
            apply(&["apply", "--verbose"]),
            Err(CliError::UnknownOption {
                command: "apply",
                option: "--verbose".to_owned()
            })
        );
    }

    #[test]
    fn run_takes_no_options() {
        assert_eq!(parse(&["run"]), Ok(Command::Run));
        assert_eq!(
            parse(&["run", "--now"]),
            Err(CliError::UnknownCommand("run".to_owned()))
        );
    }

    #[test]
    fn restore_takes_only_dry_run() {
        assert_eq!(parse(&["restore"]), Ok(Command::Restore { dry_run: false }));
        assert_eq!(
            parse(&["restore", "--dry-run"]),
            Ok(Command::Restore { dry_run: true })
        );
        assert_eq!(
            parse(&["restore", "--dry-run", "--dry-run"]),
            Err(CliError::Repeated("--dry-run"))
        );
        assert!(matches!(
            parse(&["restore", "--dry-run", "--force"]),
            Err(CliError::UnknownOption { .. })
        ));
        assert!(matches!(
            parse(&["restore", "--force"]),
            Err(CliError::UnknownOption { .. })
        ));
    }

    #[test]
    fn unknown_commands_are_rejected() {
        assert_eq!(
            parse(&["frobnicate"]),
            Err(CliError::UnknownCommand("frobnicate".to_owned()))
        );
    }
}
