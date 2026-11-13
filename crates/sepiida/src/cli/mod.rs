//! Command-line parsing. Kept free of side effects so it can be tested.

use sepiida_core::{
    Color,
    accent::ThemeMode,
    config::{Config, ModeSetting},
    shell::protocol::MAX_CONNECTION,
    source::SourceChoice,
};

pub const USAGE: &str = "\
usage:
  sepiida                          open the settings window
  sepiida palette <image>          print the palette for an image
  sepiida apply [options]          theme Windows from the current wallpaper
  sepiida run                      stay in the background (tray icon) and re-theme when the wallpaper changes
  sepiida restore [--dry-run]      put back the accent colour you had before sepiida, and the stock taskbar
  sepiida shell <command>          experimental taskbar styling (see below)

apply options:
  --source auto|windows|wallpaper-engine   where to read the wallpaper (default: from settings)
  --mode light|dark                        also switch light/dark mode (default: from settings)
  --dry-run                                print what would change without changing it

`apply` follows the settings file: a locked colour is used instead of the
wallpaper, and nothing is written for features that are turned off.

shell commands (experimental; `run` does this by itself when \"Restyle the
taskbar\" is on in the settings):
  attach [--from N] [--remote]
                     load the styling DLL into Explorer, detaching any earlier
                     copy first; --from tries
                     VisualDiagConnectionN onwards (default 1); --remote
                     asks from sepiida.exe instead of from inside Explorer
  color <rrggbb>     paint the taskbar background in this colour (`#` optional;
                     PowerShell needs quotes around a leading `#`)
  restore            put the original taskbar background back
  detach             restore and stop watching (the DLL stays loaded until Explorer restarts)
  dump               log the element tree of Start and the notification centre
                     (attaches to StartMenuExperienceHost, ShellExperienceHost
                     and ShellHost; open each surface afterwards)
  dump collect       copy those logs into %LOCALAPPDATA%\\sepiida\\logs
  dump stop          stop dumping";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Settings,
    Help,
    Palette { image: String },
    Apply(ApplyOptions),
    Restore { dry_run: bool },
    Run,
    Shell(ShellCommand),
}

/// `sepiida shell …`: drives the styling DLL inside Explorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommand {
    /// Try diagnostics connections from this number on; `remote` makes the
    /// request from sepiida.exe instead of from inside Explorer.
    Attach {
        from: u32,
        remote: bool,
    },
    Color(Color),
    Restore,
    Detach,
    Dump(DumpAction),
}

/// `sepiida shell dump …`: logs the XAML tree of Start and the
/// notification centre, to find what to style there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpAction {
    /// Attach in dump mode to every sandboxed shell host.
    Start,
    /// Copy the dump logs from the hosts' temp folders to sepiida's logs.
    Collect,
    /// Detach the dumping copies.
    Stop,
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
    /// Resolved against the wallpaper's lightness once it is known.
    pub mode: ModeSetting,
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
            mode: self.mode.map_or(config.mode, ModeSetting::from),
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
    #[error(
        "`shell` takes one of: attach [--from N] [--remote], color <rrggbb>, restore, detach, \
         dump [collect|stop]"
    )]
    ShellArguments,
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
        ["shell", rest @ ..] => parse_shell(rest).map(Command::Shell),
        [other, ..] => Err(CliError::UnknownCommand((*other).to_owned())),
    }
}

fn parse_attach(args: &[&str]) -> Result<ShellCommand, CliError> {
    let mut from = None;
    let mut remote = false;
    let mut args = args.iter().copied();
    while let Some(arg) = args.next() {
        match arg {
            "--from" => {
                let value = args.next().ok_or(CliError::MissingValue("--from"))?;
                let parsed = value
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=MAX_CONNECTION).contains(n))
                    .ok_or_else(|| {
                        CliError::InvalidValue(format!(
                            "--from takes a number from 1 to {MAX_CONNECTION}"
                        ))
                    })?;
                if from.replace(parsed).is_some() {
                    return Err(CliError::Repeated("--from"));
                }
            }
            "--remote" if !remote => remote = true,
            "--remote" => return Err(CliError::Repeated("--remote")),
            _ => return Err(CliError::ShellArguments),
        }
    }
    Ok(ShellCommand::Attach {
        from: from.unwrap_or(1),
        remote,
    })
}

fn parse_shell(args: &[&str]) -> Result<ShellCommand, CliError> {
    match args {
        ["attach", options @ ..] => parse_attach(options),
        ["color", value] => {
            // PowerShell drops an unquoted argument starting with `#` as a
            // comment, so the `#` is optional.
            let with_hash = if value.starts_with('#') {
                (*value).to_owned()
            } else {
                format!("#{value}")
            };
            with_hash
                .parse()
                .map(ShellCommand::Color)
                .map_err(|e: sepiida_core::InvalidColor| CliError::InvalidValue(e.to_string()))
        }
        ["restore"] => Ok(ShellCommand::Restore),
        ["detach"] => Ok(ShellCommand::Detach),
        ["dump"] => Ok(ShellCommand::Dump(DumpAction::Start)),
        ["dump", "collect"] => Ok(ShellCommand::Dump(DumpAction::Collect)),
        ["dump", "stop"] => Ok(ShellCommand::Dump(DumpAction::Stop)),
        _ => Err(CliError::ShellArguments),
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
mod tests;
