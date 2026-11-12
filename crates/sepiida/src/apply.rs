//! `sepiida apply`: wallpaper → palette → Windows accent colour, once.

use anyhow::{Context, Result};
use sepiida_core::{
    Palette,
    accent::AccentValues,
    source::{SourceError, Wallpaper, WallpaperSource, resolve},
};

use crate::{
    cli::ApplyOptions,
    win::{desktop::WindowsWallpaper, theme},
};

/// Placeholder until the Wallpaper Engine source lands: it never shows a
/// wallpaper, so Auto always uses the Windows wallpaper.
struct NoWallpaperEngine;

impl WallpaperSource for NoWallpaperEngine {
    fn current(&self) -> Result<Option<Wallpaper>, SourceError> {
        Ok(None)
    }
}

pub fn run(options: ApplyOptions) -> Result<()> {
    let resolved = resolve(options.source, &WindowsWallpaper, &NoWallpaperEngine)?;
    if let Some(error) = &resolved.wallpaper_engine_error {
        eprintln!(
            "warning: Wallpaper Engine could not be read, using the Windows wallpaper: {error}"
        );
    }
    match &resolved.wallpaper {
        Wallpaper::Image(path) => println!("wallpaper  {} ({})", path.display(), resolved.kind),
        Wallpaper::Color(color) => println!("wallpaper  solid colour {color} ({})", resolved.kind),
    }

    let palette = Palette::from_wallpaper(&resolved.wallpaper)
        .context("could not derive a palette from the wallpaper")?;
    let accent = AccentValues::from_palette(&palette);
    println!("source     {}", palette.source);
    println!("accent     {}", accent.accent());

    let mut writes = accent.registry_writes();
    if let Some(mode) = options.mode {
        writes.extend(mode.registry_writes());
    }
    let pending = theme::pending(writes)?;

    if pending.is_empty() {
        println!("nothing to change");
        return Ok(());
    }
    for write in &pending {
        println!(
            "set        HKCU\\{}\\{} = {}",
            write.key, write.name, write.data
        );
    }
    if options.dry_run {
        println!("dry run: nothing was changed");
        return Ok(());
    }
    theme::apply(&pending)?;
    println!("applied");
    Ok(())
}
