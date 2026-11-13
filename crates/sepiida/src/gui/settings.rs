//! The settings window: shows the current palette and edits `config.toml`.
//!
//! Each control saves the file straight away. A running agent watches the
//! file and applies the change, so the window never talks to the agent
//! directly and works the same whether it was opened from the tray or on
//! its own. Runs on the Slint event-loop thread; there is at most one
//! settings window per process.

use std::{cell::RefCell, path::PathBuf, thread};

use anyhow::{Context, Result};
use sepiida_core::{
    Color, Palette,
    accent::AccentValues,
    cache::PaletteCache,
    config::Config,
    source::{ResolveError, SourceChoice, SourceKind, resolve},
};
use slint::ComponentHandle;
use tracing::warn;

use crate::{
    SettingsWindow,
    gui::settings_model::{
        is_our_startup_entry, mode_at, mode_index, source_at, source_index, startup_command,
    },
    paths::Paths,
    win::{desktop::WindowsWallpaper, startup, wallpaper_engine::WallpaperEngine},
};

const WINDOWLESS_EXE: &str = "sepiidaw.exe";

struct State {
    config: Config,
    config_path: PathBuf,
    palette_cache_path: PathBuf,
    /// The wallpaper's own palette, which "lock" freezes. `None` until the
    /// wallpaper has been read.
    wallpaper: Option<Palette>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Creates and shows the window.
pub fn open(paths: &Paths) -> Result<SettingsWindow> {
    let window = SettingsWindow::new()?;
    let (config, error) = Config::load_or_default(&paths.config);
    if let Some(error) = error {
        window.set_status(
            format!(
                "{:#}. Changing a setting here replaces the file.",
                anyhow::Error::new(error)
            )
            .into(),
        );
    }
    show_config(&window, &config);
    show_startup(&window);
    STATE.with_borrow_mut(|state| {
        *state = Some(State {
            config,
            config_path: paths.config.clone(),
            palette_cache_path: paths.palette_cache.clone(),
            wallpaper: None,
        });
    });
    connect(&window);
    reload_preview(&window);
    window.show()?;
    Ok(window)
}

/// Re-reads the wallpaper and updates the preview, for example after the
/// agent applied a new theme.
pub fn reload_preview(window: &SettingsWindow) {
    let Some((source, cache_path)) = STATE.with_borrow(|state| {
        state
            .as_ref()
            .map(|s| (s.config.source, s.palette_cache_path.clone()))
    }) else {
        return;
    };
    let weak = window.as_weak();
    // Reading Wallpaper Engine can take a moment; keep the window
    // responsive meanwhile.
    let spawned = thread::Builder::new()
        .name("settings-preview".to_owned())
        .spawn(move || {
            let result = wallpaper_palette(source, &cache_path);
            let _ = weak.upgrade_in_event_loop(move |window| match result {
                Ok((palette, from)) => {
                    STATE.with_borrow_mut(|state| {
                        if let Some(state) = state {
                            state.wallpaper = Some(palette);
                        }
                    });
                    show_preview(&window, Some((palette, from)));
                }
                Err(error) => {
                    window.set_preview_text(error_text(&error).into());
                    show_preview(&window, None);
                }
            });
        });
    if let Err(error) = spawned {
        warn!("could not read the wallpaper for the preview: {error}");
    }
}

/// The error, plus what it means for the theme when the chosen source has
/// nothing to show: an explicit choice never falls back to the other one.
fn error_text(error: &anyhow::Error) -> String {
    let nothing_shown = matches!(
        error.downcast_ref::<ResolveError>(),
        Some(ResolveError::NothingShown {
            kind: SourceKind::WallpaperEngine,
            ..
        })
    );
    if nothing_shown {
        format!(
            "{error:#}. The colours stay as they are; choose Automatic to use the \
             Windows wallpaper meanwhile."
        )
    } else {
        format!("{error:#}")
    }
}

/// Reads the palette only from the cache file, never writes it: the agent
/// owns that file.
fn wallpaper_palette(
    source: SourceChoice,
    cache_path: &std::path::Path,
) -> Result<(Palette, String)> {
    let resolved = resolve(source, &WindowsWallpaper, &WallpaperEngine)?;
    let (mut cache, _) = PaletteCache::load_or_empty(cache_path);
    let palette = cache
        .palette(&resolved.wallpaper)
        .context("could not read the wallpaper")?;
    Ok((palette, resolved.kind.to_string()))
}

/// Shows the palette in effect: the locked colour if there is one,
/// otherwise the wallpaper's (when known).
fn show_preview(window: &SettingsWindow, wallpaper: Option<(Palette, String)>) {
    let locked = STATE.with_borrow(|state| state.as_ref().and_then(|s| s.config.locked_color));
    let (palette, text) = match (locked, wallpaper) {
        (Some(color), _) => (
            Palette::from_color(color),
            format!("Locked to {color}; the wallpaper is ignored."),
        ),
        (None, Some((palette, from))) => (
            palette,
            format!(
                "From the {from} wallpaper: colour {}, lightness {} of 100.",
                palette.source, palette.lightness
            ),
        ),
        (None, None) => return,
    };
    // The accent Windows actually gets, not the Material one.
    let accent = AccentValues::from_palette(&palette).accent();
    let (light, dark) = (palette.light.surface, palette.dark.surface);
    window.set_preview_source(to_slint(palette.source));
    window.set_preview_source_label(to_slint(palette.source.readable_text()));
    window.set_preview_accent(to_slint(accent));
    window.set_preview_accent_label(to_slint(accent.readable_text()));
    window.set_preview_light_surface(to_slint(light));
    window.set_preview_light_surface_label(to_slint(light.readable_text()));
    window.set_preview_dark_surface(to_slint(dark));
    window.set_preview_dark_surface_label(to_slint(dark.readable_text()));
    window.set_preview_text(text.into());
}

fn show_config(window: &SettingsWindow, config: &Config) {
    window.set_source_index(source_index(config.source));
    window.set_mode_index(mode_index(config.mode));
    window.set_accent_enabled(config.accent);
    window.set_shell_styling(config.shell_styling);
    window.set_locked(config.locked_color.is_some());
    window.set_locked_text(
        config
            .locked_color
            .map(|c| c.to_string())
            .unwrap_or_default()
            .into(),
    );
}

fn connect(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_source_changed(move |index| {
        let Some(source) = source_at(index) else {
            return;
        };
        update(&weak, |config| config.source = source);
        if let Some(window) = weak.upgrade() {
            reload_preview(&window);
        }
    });

    let weak = window.as_weak();
    window.on_mode_changed(move |index| {
        let Some(mode) = mode_at(index) else { return };
        update(&weak, |config| config.mode = mode);
    });

    let weak = window.as_weak();
    window.on_accent_toggled(move |on| update(&weak, |config| config.accent = on));

    let weak = window.as_weak();
    window.on_shell_styling_toggled(move |on| update(&weak, |config| config.shell_styling = on));

    let weak = window.as_weak();
    window.on_lock_toggled(move |on| {
        let Some(window) = weak.upgrade() else { return };
        let color = if on {
            match current_wallpaper_color() {
                Some(color) => Some(color),
                None => {
                    window.set_locked(false);
                    window.set_status("The wallpaper has not been read yet; try again.".into());
                    return;
                }
            }
        } else {
            None
        };
        update(&weak, |config| config.locked_color = color);
        window.set_locked_text(color.map(|c| c.to_string()).unwrap_or_default().into());
        let wallpaper = STATE.with_borrow(|state| state.as_ref().and_then(|s| s.wallpaper));
        show_preview(&window, wallpaper.map(|p| (p, "current".to_owned())));
    });

    let weak = window.as_weak();
    window.on_start_with_windows_toggled(move |on| {
        let Some(window) = weak.upgrade() else { return };
        if let Err(error) = set_start_with_windows(on) {
            window.set_start_with_windows(!on);
            window.set_status(format!("{error:#}").into());
        }
    });
}

fn current_wallpaper_color() -> Option<Color> {
    STATE.with_borrow(|state| state.as_ref().and_then(|s| s.wallpaper).map(|p| p.source))
}

/// Changes one setting and saves the file, reporting the result in the
/// window.
fn update(weak: &slint::Weak<SettingsWindow>, change: impl FnOnce(&mut Config)) {
    let result = STATE.with_borrow_mut(|state| {
        let Some(state) = state else {
            return Ok(());
        };
        change(&mut state.config);
        state.config.save(&state.config_path)
    });
    let Some(window) = weak.upgrade() else { return };
    match result {
        Ok(()) => window.set_status("".into()),
        Err(error) => window.set_status(format!("{:#}", anyhow::Error::new(error)).into()),
    }
}

fn show_startup(window: &SettingsWindow) {
    let enabled = expected_startup_command()
        .ok()
        .zip(startup::read().ok().flatten())
        .is_some_and(|(expected, value)| is_our_startup_entry(&value, &expected));
    window.set_start_with_windows(enabled);
}

fn set_start_with_windows(on: bool) -> Result<()> {
    if on {
        startup::enable(&expected_startup_command()?)?;
    } else {
        startup::disable()?;
    }
    Ok(())
}

/// `sepiidaw.exe` next to the running program, quoted.
fn expected_startup_command() -> Result<String> {
    let exe = std::env::current_exe()
        .context("could not find sepiida's own location")?
        .with_file_name(WINDOWLESS_EXE);
    anyhow::ensure!(exe.is_file(), "{} was not found", exe.display());
    startup_command(&exe).with_context(|| format!("{} cannot be started safely", exe.display()))
}

fn to_slint(color: Color) -> slint::Color {
    slint::Color::from_rgb_u8(color.r, color.g, color.b)
}
