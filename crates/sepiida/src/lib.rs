//! sepiida: wallpaper → palette → Windows accent colour. Shared by the
//! console program (`sepiida.exe`) and the windowless one (`sepiidaw.exe`).

#[cfg(any(windows, test))]
mod agent;
#[cfg(windows)]
mod apply;
mod cli;
#[cfg(any(windows, test))]
mod gui;
#[cfg(any(windows, test))]
mod logging;
#[cfg(any(windows, test))]
mod paths;
#[cfg(windows)]
mod restore;
#[cfg(windows)]
mod shell;
#[cfg(windows)]
mod win;

use anyhow::{Context, Result, bail};
use sepiida_core::{Palette, Tones};

use crate::cli::{ApplyOptions, Command, ShellCommand};

#[cfg(windows)]
slint::include_modules!();

/// Entry point of `sepiida.exe`, the command-line program.
pub fn cli_main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(error) => bail!("{error}\n\n{}", cli::USAGE),
    };
    match command {
        Command::Settings => run_settings(),
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(())
        }
        Command::Palette { image } => print_palette(&image),
        Command::Apply(options) => apply(options),
        Command::Restore { dry_run } => restore(dry_run),
        Command::Run => run_agent(),
        Command::Shell(command) => shell(command),
    }
}

/// Entry point of `sepiidaw.exe`, which has no console: with no arguments
/// (as started at sign-in) it runs the background agent and tray icon;
/// `sepiidaw settings` opens only the settings window. Errors are shown in
/// a message box, since there is nowhere to print them.
#[cfg(windows)]
pub fn gui_main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => agent::run(),
        [settings] if settings == "settings" => run_settings(),
        _ => Err(anyhow::anyhow!("unknown arguments: {}", args.join(" "))),
    };
    if let Err(error) = result {
        win::message_box::error(&format!("{error:#}"));
    }
}

#[cfg(not(windows))]
pub fn gui_main() {
    eprintln!("sepiidaw only runs on Windows");
}

fn print_palette(path: &str) -> Result<()> {
    let palette = Palette::from_path(path).with_context(|| format!("could not read {path}"))?;
    println!("source  {}", palette.source);
    print_tones("light", &palette.light);
    print_tones("dark", &palette.dark);
    Ok(())
}

fn print_tones(mode: &str, t: &Tones) {
    println!(
        "{mode:<7} accent {}  on-accent {}  surface {}  on-surface {}",
        t.accent, t.on_accent, t.surface, t.on_surface
    );
}

#[cfg(not(windows))]
fn apply(_options: ApplyOptions) -> Result<()> {
    bail!("`sepiida apply` only runs on Windows; try `sepiida palette <image>`")
}

#[cfg(windows)]
fn apply(options: ApplyOptions) -> Result<()> {
    apply::run_cli(options)
}

#[cfg(not(windows))]
fn run_agent() -> Result<()> {
    bail!("`sepiida run` only runs on Windows")
}

#[cfg(windows)]
fn run_agent() -> Result<()> {
    agent::run()
}

#[cfg(not(windows))]
fn restore(_dry_run: bool) -> Result<()> {
    bail!("`sepiida restore` only runs on Windows")
}

#[cfg(windows)]
fn restore(dry_run: bool) -> Result<()> {
    restore::run_cli(dry_run)
}

#[cfg(not(windows))]
fn shell(_command: ShellCommand) -> Result<()> {
    bail!("`sepiida shell` only runs on Windows")
}

#[cfg(windows)]
fn shell(command: ShellCommand) -> Result<()> {
    match command {
        ShellCommand::Dump(action) => shell::dump::run(action),
        other => shell::taskbar::run_cli(other),
    }
}

/// The settings window on its own, without the tray. Changes reach a
/// running agent through the settings file.
#[cfg(windows)]
fn run_settings() -> Result<()> {
    let paths = paths::Paths::locate()?;
    let _window = gui::settings::open(&paths)?;
    slint::run_event_loop()?;
    Ok(())
}

#[cfg(not(windows))]
fn run_settings() -> Result<()> {
    bail!("the settings window only runs on Windows; try `sepiida palette <image>`")
}
