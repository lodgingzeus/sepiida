//! Wallpaper → palette → Windows accent colour, once.
//!
//! `prepare` works out what would change without changing anything, so the
//! command line and the background agent share it and differ only in how
//! they report.

use anyhow::{Context, Result};
use sepiida_core::{
    Palette,
    accent::{AccentValues, RegistryWrite},
    cache::PaletteCache,
    config::Config,
    source::{Resolved, SourceError, Wallpaper, resolve},
};

use crate::{
    cli::{ApplyOptions, ApplySettings},
    paths::Paths,
    win::{desktop::WindowsWallpaper, theme, wallpaper_engine::WallpaperEngine},
};

/// Where the palette came from.
#[derive(Debug)]
pub enum Origin {
    /// The user locked the palette to this colour.
    Locked,
    Wallpaper(Resolved),
}

#[derive(Debug)]
pub struct Prepared {
    pub origin: Origin,
    pub palette: Palette,
    /// `None` when the accent colour is turned off.
    pub accent: Option<AccentValues>,
    /// Only the writes whose value differs from what is in the registry.
    pub pending: Vec<RegistryWrite>,
}

impl Prepared {
    /// Set when Auto fell back to the Windows wallpaper because Wallpaper
    /// Engine could not be read.
    pub fn wallpaper_engine_error(&self) -> Option<&SourceError> {
        match &self.origin {
            Origin::Wallpaper(resolved) => resolved.wallpaper_engine_error.as_ref(),
            Origin::Locked => None,
        }
    }
}

pub fn prepare(settings: &ApplySettings, cache: &mut PaletteCache) -> Result<Prepared> {
    let (origin, palette) = match settings.locked_color {
        Some(color) => (Origin::Locked, Palette::from_color(color)),
        None => {
            let resolved = resolve(settings.source, &WindowsWallpaper, &WallpaperEngine)?;
            let palette = cache
                .palette(&resolved.wallpaper)
                .context("could not derive a palette from the wallpaper")?;
            (Origin::Wallpaper(resolved), palette)
        }
    };

    let accent = settings
        .accent
        .then(|| AccentValues::from_palette(&palette));
    let mut writes = Vec::new();
    if let Some(accent) = &accent {
        writes.extend(accent.registry_writes());
    }
    if let Some(mode) = settings.mode {
        writes.extend(mode.registry_writes());
    }
    let pending = theme::pending(writes)?;

    Ok(Prepared {
        origin,
        palette,
        accent,
        pending,
    })
}

/// Writes the pending values and broadcasts the change once.
pub fn commit(prepared: &Prepared) -> Result<()> {
    theme::apply(&prepared.pending)?;
    Ok(())
}

/// `sepiida apply`: runs once and prints what it does.
pub fn run_cli(options: ApplyOptions) -> Result<()> {
    let paths = Paths::locate()?;
    let (config, error) = Config::load_or_default(&paths.config);
    if let Some(error) = error {
        eprintln!(
            "warning: {:#}; using the default settings",
            anyhow::Error::new(error)
        );
    }
    let (mut cache, error) = PaletteCache::load_or_empty(&paths.palette_cache);
    if let Some(error) = error {
        eprintln!(
            "warning: {:#}; starting with an empty cache",
            anyhow::Error::new(error)
        );
    }

    let prepared = prepare(&options.settings(&config), &mut cache)?;
    if let Err(error) = cache.save(&paths.palette_cache) {
        eprintln!("warning: {:#}", anyhow::Error::new(error));
    }
    print_prepared(&prepared);

    if prepared.pending.is_empty() {
        println!("nothing to change");
        return Ok(());
    }
    if options.dry_run {
        println!("dry run: nothing was changed");
        return Ok(());
    }
    commit(&prepared)?;
    println!("applied");
    Ok(())
}

fn print_prepared(prepared: &Prepared) {
    if let Some(error) = prepared.wallpaper_engine_error() {
        eprintln!(
            "warning: Wallpaper Engine could not be read, using the Windows wallpaper: {error}"
        );
    }
    match &prepared.origin {
        Origin::Locked => println!("wallpaper  ignored: the palette is locked"),
        Origin::Wallpaper(resolved) => match &resolved.wallpaper {
            Wallpaper::Image(path) => {
                println!("wallpaper  {} ({})", path.display(), resolved.kind);
            }
            Wallpaper::Color(color) => {
                println!("wallpaper  solid colour {color} ({})", resolved.kind);
            }
        },
    }
    println!("source     {}", prepared.palette.source);
    match &prepared.accent {
        Some(accent) => println!("accent     {}", accent.accent()),
        None => println!("accent     turned off in the settings"),
    }
    for write in &prepared.pending {
        println!(
            "set        HKCU\\{}\\{} = {}",
            write.key, write.name, write.data
        );
    }
}
