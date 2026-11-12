#[cfg(windows)]
mod apply;
mod cli;
#[cfg(windows)]
mod win;

use anyhow::{Context, Result, bail};
use sepiida_core::{Palette, Tones};

use crate::cli::{ApplyOptions, Command};

#[cfg(windows)]
slint::include_modules!();

fn main() -> Result<()> {
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
    }
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
    apply::run(options)
}

#[cfg(windows)]
fn run_settings() -> Result<()> {
    use slint::ComponentHandle;

    SettingsWindow::new()?.run()?;
    Ok(())
}

#[cfg(not(windows))]
fn run_settings() -> Result<()> {
    bail!("the settings window only runs on Windows; try `sepiida palette <image>`")
}
