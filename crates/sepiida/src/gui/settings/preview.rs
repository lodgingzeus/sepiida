//! Reads the wallpaper for the settings window, off the UI thread: its
//! palette, from the agent's cache (read only: the agent owns that file),
//! and a small copy of the picture.

use std::path::Path;

use anyhow::{Context, Result};
use sepiida_core::{
    Palette,
    cache::PaletteCache,
    source::{ResolveError, SourceChoice, SourceKind, Wallpaper, resolve},
    thumbnail::{Thumbnail, thumbnail},
};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use tracing::warn;

use crate::win::{desktop::WindowsWallpaper, wallpaper_engine::WallpaperEngine};

/// Pixels of the preview's copy of the wallpaper: the preview's screen at
/// 150 % scaling, 16:10.
const THUMBNAIL_SIZE: (u32, u32) = (640, 400);

pub struct WallpaperRead {
    pub wallpaper: Wallpaper,
    /// The wallpaper's own palette, before any colour choice.
    pub palette: Palette,
    /// "Windows" or "Wallpaper Engine".
    pub kind: String,
    /// `None` for a solid colour, or a picture that could not be read
    /// again (the preview then shows the colour).
    pub thumbnail: Option<Thumbnail>,
}

pub fn read(source: SourceChoice, cache_path: &Path) -> Result<WallpaperRead> {
    let resolved = resolve(source, &WindowsWallpaper, &WallpaperEngine)?;
    let (mut cache, _) = PaletteCache::load_or_empty(cache_path);
    let palette = cache
        .palette(&resolved.wallpaper)
        .context("could not read the wallpaper")?;
    let thumbnail = match &resolved.wallpaper {
        Wallpaper::Image(path) => {
            let (width, height) = THUMBNAIL_SIZE;
            thumbnail(path, width, height)
                .inspect_err(|error| warn!("no wallpaper picture for the preview: {error}"))
                .ok()
        }
        Wallpaper::Color(_) => None,
    };
    Ok(WallpaperRead {
        wallpaper: resolved.wallpaper,
        palette,
        kind: resolved.kind.to_string(),
        thumbnail,
    })
}

/// Built on the UI thread: Slint images are not `Send`.
pub fn to_image(thumbnail: &Thumbnail) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &thumbnail.rgba,
        thumbnail.width,
        thumbnail.height,
    ))
}

/// The error, plus what it means for the theme when the chosen source has
/// nothing to show: an explicit choice never falls back to the other one.
pub fn error_text(error: &anyhow::Error) -> String {
    let nothing_shown = matches!(
        error.downcast_ref::<ResolveError>(),
        Some(ResolveError::NothingShown {
            kind: SourceKind::WallpaperEngine,
            ..
        })
    );
    if nothing_shown {
        format!(
            "{error:#}. The colours stay as they are; choose Auto to use the \
             Windows wallpaper meanwhile."
        )
    } else {
        format!("{error:#}")
    }
}
