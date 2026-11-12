use anyhow::{Context, Result, bail};
use sepiida_core::{Palette, Tones};

#[cfg(windows)]
slint::include_modules!();

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["palette", path] => print_palette(path),
        [] => run_settings(),
        _ => bail!("usage: sepiida [palette <image>]"),
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
