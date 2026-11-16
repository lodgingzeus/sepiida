//! Drives the settings window: shows the palette in effect and edits
//! `config.toml`.
//!
//! Each control saves the file straight away; the custom colour only when
//! "Use this colour" is pressed, so dragging the picker never rewrites the
//! Windows accent. A running agent watches the file and applies the
//! change, so the window never talks to the agent directly and works the
//! same whether it was opened from the tray or on its own. Runs on the
//! Slint event-loop thread; there is at most one settings window per
//! process.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    thread,
};

use anyhow::{Context, Result};
use sepiida_core::{
    Color, Palette,
    accent::ThemeMode,
    choice::WallpaperPick,
    config::Config,
    settings_look::{
        self, ChipColors as Chip, MAX_PICKER_CHROMA, ShellPreview, WindowColors as Look,
        picker_color, picker_position,
    },
    source::Wallpaper,
};
use slint::{
    ComponentHandle, ModelRc, VecModel,
    winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent},
};
use tracing::warn;

use super::{
    model::{
        Look as ShellLook, is_our_startup_entry, look_at, look_index, mode_controls, mode_from,
        source_at, source_index, startup_command, style_at, style_index,
    },
    preview::{self, WallpaperRead},
};
use crate::{
    ChipColors, SettingsWindow, ShellColors, Theme, WindowColors,
    paths::Paths,
    win::{startup, theme},
};

const WINDOWLESS_EXE: &str = "sepiidaw.exe";

struct State {
    config: Config,
    config_path: PathBuf,
    palette_cache_path: PathBuf,
    /// `None` until the wallpaper has been read, or when it cannot be.
    wallpaper: Option<(Wallpaper, Palette, String)>,
    /// The picker's colour while it is open: previewed, not saved.
    picker: Option<Color>,
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
    let stops: Vec<slint::Color> = settings_look::hue_stops().map(to_slint).to_vec();
    window.set_picker_hue_stops(ModelRc::new(VecModel::from(stops)));
    STATE.with_borrow_mut(|state| {
        *state = Some(State {
            config,
            config_path: paths.config.clone(),
            palette_cache_path: paths.palette_cache.clone(),
            wallpaper: None,
            picker: None,
        });
    });
    connect(&window);
    repaint_after_restore(&window);
    refresh(&window);
    reload_preview(&window);
    window.show()?;
    Ok(window)
}

/// Repaints the whole window when it comes back from minimised or hidden.
/// Windows drops the window's pixels meanwhile, and the software renderer
/// would otherwise repaint only what changed (Slint 1.18). Minimising shows
/// up as a zero size, restoring as the real size again.
fn repaint_after_restore(window: &SettingsWindow) {
    let weak = window.as_weak();
    let minimized = Cell::new(false);
    window.window().on_winit_window_event(move |_, event| {
        let restored = match event {
            WindowEvent::Resized(size) if size.width == 0 || size.height == 0 => {
                minimized.set(true);
                false
            }
            WindowEvent::Resized(_) => minimized.replace(false),
            WindowEvent::Occluded(false) => true,
            _ => false,
        };
        if restored && let Some(window) = weak.upgrade() {
            window.set_repaint_flip(!window.get_repaint_flip());
        }
        EventResult::Propagate
    });
}

/// Re-reads the wallpaper and updates the preview, for example after the
/// agent applied a new theme.
pub fn reload_preview(window: &SettingsWindow) {
    let Some((source, cache_path)) =
        with_state(|s| (s.config.source, s.palette_cache_path.clone()))
    else {
        return;
    };
    let weak = window.as_weak();
    // Reading Wallpaper Engine and decoding the picture take a moment;
    // keep the window responsive meanwhile.
    let spawned = thread::Builder::new()
        .name("settings-preview".to_owned())
        .spawn(move || {
            let result = preview::read(source, &cache_path);
            let _ = weak.upgrade_in_event_loop(move |window| show_wallpaper(&window, result));
        });
    if let Err(error) = spawned {
        warn!("could not read the wallpaper for the preview: {error}");
    }
}

fn show_wallpaper(window: &SettingsWindow, result: Result<WallpaperRead>) {
    let wallpaper = match result {
        Ok(read) => {
            match &read.thumbnail {
                Some(thumbnail) => window.set_wallpaper(preview::to_image(thumbnail)),
                None => window.set_wallpaper(slint::Image::default()),
            }
            window.set_has_wallpaper(read.thumbnail.is_some());
            if let Wallpaper::Color(color) = read.wallpaper {
                window.set_wallpaper_color(to_slint(color));
            }
            Some((read.wallpaper, read.palette, read.kind))
        }
        Err(error) => {
            window.set_has_wallpaper(false);
            let text = preview::error_text(&error);
            window.set_preview_text(text.clone().into());
            // The preview text is replaced when a custom colour is shown.
            window.set_status(text.into());
            None
        }
    };
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state {
            state.wallpaper = wallpaper;
        }
    });
    refresh(window);
}

/// Shows the palette in effect: the window's own colours, the preview and
/// the chips.
fn refresh(window: &SettingsWindow) {
    let Some((config, wallpaper, picker)) =
        with_state(|s| (s.config.clone(), s.wallpaper.clone(), s.picker))
    else {
        return;
    };
    let mut choice = config.color_choice();
    if picker.is_some() {
        choice.custom = picker;
    }

    let (chips, index) = match &wallpaper {
        Some((wallpaper, palette, _)) => (
            settings_look::chip_colors(&palette.with_choice(palette.source, config.style)),
            choice.candidate_index(wallpaper, palette),
        ),
        None => (Vec::new(), None),
    };
    let chips: Vec<ChipColors> = chips.into_iter().map(chip).collect();
    window.set_candidates(ModelRc::new(VecModel::from(chips)));
    window.set_candidate_index(index.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
    window.set_has_custom(config.locked_color.is_some());
    if let Some(color) = config.locked_color {
        window.set_custom_color(to_slint(color));
    }

    let Some(palette) = choice.palette(
        wallpaper
            .as_ref()
            .map(|(wallpaper, palette, _)| (wallpaper, palette)),
    ) else {
        return;
    };
    let theme = window.global::<Theme>();
    theme.set_light(look(Look::new(&palette, false)));
    theme.set_dark(look(Look::new(&palette, true)));
    // The mode the shell will be in: what the light/dark setting gives
    // for this wallpaper, else what Windows uses now.
    let shell_dark = config
        .mode
        .theme_mode(palette.lightness)
        .unwrap_or_else(theme::system_mode)
        == ThemeMode::Dark;
    window.set_shell(shell(ShellPreview::new(&palette, shell_dark)));
    window.set_shell_dark(shell_dark);

    let kind = wallpaper.as_ref().map(|(_, _, kind)| kind.as_str());
    let text = match (picker, config.locked_color, kind) {
        (Some(color), _, _) => format!("Previewing {color}. Nothing changes until you use it."),
        (None, Some(color), Some(kind)) => {
            format!("Your own colour {color}; the {kind} wallpaper still sets light and dark.")
        }
        (None, Some(color), None) => format!("Your own colour {color}."),
        (None, None, Some(kind)) => format!("From the {kind} wallpaper."),
        // The wallpaper could not be read: the error stays on screen.
        (None, None, None) => return,
    };
    window.set_preview_text(text.into());
}

fn show_config(window: &SettingsWindow, config: &Config) {
    window.set_source_index(source_index(config.source));
    let (mode_enabled, mode_index) = mode_controls(config.mode);
    window.set_mode_enabled(mode_enabled);
    window.set_mode_index(mode_index);
    window.set_style_index(style_index(config.style));
    window.set_look_index(look_index(ShellLook::of(config)));
    window.set_accent_enabled(config.accent);
}

fn connect(window: &SettingsWindow) {
    connect_colours(window);
    connect_picker(window);

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
    window.on_mode_changed(move || {
        let Some(window) = weak.upgrade() else { return };
        let Some(mode) = mode_from(window.get_mode_enabled(), window.get_mode_index()) else {
            return;
        };
        update(&weak, |config| config.mode = mode);
    });

    let weak = window.as_weak();
    window.on_look_changed(move |index| {
        let Some(look) = look_at(index) else { return };
        update(&weak, |config| look.apply(config));
    });

    let weak = window.as_weak();
    window.on_accent_toggled(move |on| update(&weak, |config| config.accent = on));

    let weak = window.as_weak();
    window.on_start_with_windows_toggled(move |on| {
        let Some(window) = weak.upgrade() else { return };
        if let Err(error) = set_start_with_windows(on) {
            window.set_start_with_windows(!on);
            window.set_status(format!("{error:#}").into());
        }
    });
}

/// The wallpaper's colour chips and the style chips.
fn connect_colours(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_candidate_chosen(move |index| {
        let Some((wallpaper, color)) = with_state(|s| {
            let (wallpaper, palette, _) = s.wallpaper.as_ref()?;
            let color = *palette
                .candidates
                .as_slice()
                .get(usize::try_from(index).ok()?)?;
            Some((wallpaper.clone(), color))
        })
        .flatten() else {
            return;
        };
        if let Some(window) = weak.upgrade() {
            window.set_picker_open(false);
        }
        set_picker(&weak, None);
        update(&weak, |config| {
            config.locked_color = None;
            // The best colour is the default; only another one is kept.
            config.wallpaper_pick = (index != 0)
                .then(|| WallpaperPick::new(&wallpaper, color))
                .flatten();
        });
    });

    let weak = window.as_weak();
    window.on_style_changed(move |index| {
        let Some(style) = style_at(index) else { return };
        update(&weak, |config| config.style = style);
    });
}

/// The custom colour picker. Its colour is only previewed until used.
fn connect_picker(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_open_picker(move || {
        let Some(window) = weak.upgrade() else { return };
        let start = with_state(|s| {
            s.config.locked_color.or_else(|| {
                let (wallpaper, palette, _) = s.wallpaper.as_ref()?;
                s.config
                    .color_choice()
                    .palette(Some((wallpaper, palette)))
                    .map(|p| p.source)
            })
        })
        .flatten()
        .unwrap_or(Color::new(0x3f, 0x5a, 0x94));
        show_picker(&window, start, true);
        window.set_picker_open(true);
        set_picker(&weak, Some(start));
    });

    let weak = window.as_weak();
    window.on_picker_moved(move || {
        let Some(window) = weak.upgrade() else { return };
        let hue = f64::from(window.get_picker_hue()) * 360.0;
        let chroma = f64::from(window.get_picker_colourfulness()) * MAX_PICKER_CHROMA;
        let color = picker_color(hue, chroma);
        show_picker(&window, color, false);
        set_picker(&weak, Some(color));
    });

    let weak = window.as_weak();
    window.on_picker_hex_entered(move |text| {
        let Some(window) = weak.upgrade() else { return };
        match text.trim().parse::<Color>() {
            Ok(color) => {
                show_picker(&window, color, true);
                set_picker(&weak, Some(color));
            }
            Err(_) => window.set_picker_error("Type a colour as #rrggbb.".into()),
        }
    });

    let weak = window.as_weak();
    window.on_picker_use(move || {
        let Some(color) = with_state(|s| s.picker).flatten() else {
            return;
        };
        if let Some(window) = weak.upgrade() {
            window.set_picker_open(false);
        }
        set_picker(&weak, None);
        update(&weak, |config| config.locked_color = Some(color));
    });

    let weak = window.as_weak();
    window.on_picker_cancel(move || {
        if let Some(window) = weak.upgrade() {
            window.set_picker_open(false);
        }
        set_picker(&weak, None);
    });
}

/// Shows `color` in the picker; `move_sliders` when it came from elsewhere
/// (the hex field, or opening the picker) rather than from the sliders.
fn show_picker(window: &SettingsWindow, color: Color, move_sliders: bool) {
    let (hue, chroma) = picker_position(color);
    if move_sliders {
        window.set_picker_hue((hue / 360.0) as f32);
        window.set_picker_colourfulness((chroma / MAX_PICKER_CHROMA) as f32);
    }
    let slider_hue = f64::from(window.get_picker_hue()) * 360.0;
    window.set_picker_color(to_slint(color));
    window.set_picker_grey(to_slint(picker_color(slider_hue, 0.0)));
    window.set_picker_vivid(to_slint(picker_color(slider_hue, MAX_PICKER_CHROMA)));
    window.set_picker_hex(color.to_string().into());
    window.set_picker_error("".into());
}

fn set_picker(weak: &slint::Weak<SettingsWindow>, color: Option<Color>) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state {
            state.picker = color;
        }
    });
    if let Some(window) = weak.upgrade() {
        refresh(&window);
    }
}

fn with_state<T>(read: impl FnOnce(&State) -> T) -> Option<T> {
    STATE.with_borrow(|state| state.as_ref().map(read))
}

/// Changes one setting, saves the file and shows the result.
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
    refresh(&window);
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

fn look(c: Look) -> WindowColors {
    WindowColors {
        background: to_slint(c.background),
        card: to_slint(c.card),
        chip: to_slint(c.chip),
        outline: to_slint(c.outline),
        text: to_slint(c.text),
        muted_text: to_slint(c.muted_text),
        accent: to_slint(c.accent),
        on_accent: to_slint(c.on_accent),
    }
}

fn shell(c: ShellPreview) -> ShellColors {
    ShellColors {
        surface: to_slint(c.surface),
        card: to_slint(c.card),
        accent: to_slint(c.accent),
    }
}

fn chip(c: Chip) -> ChipColors {
    ChipColors {
        accent: to_slint(c.accent),
        light: to_slint(c.light),
        dark: to_slint(c.dark),
    }
}

fn to_slint(color: Color) -> slint::Color {
    slint::Color::from_rgb_u8(color.r, color.g, color.b)
}
