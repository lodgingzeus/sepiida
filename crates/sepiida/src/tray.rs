//! The tray icon and the settings window. Everything here runs on the Slint
//! event-loop thread; other threads reach it through the `pub fn`s, which
//! hand their work to that thread.

use std::{cell::RefCell, sync::mpsc::Sender, time::Duration};

use anyhow::Result;
use sepiida_core::{Color, schedule::Event};
use slint::{CloseRequestResponse, ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, Timer};
use tracing::warn;

use crate::{
    SettingsWindow, Tray,
    paths::Paths,
    settings,
    tray_icon::{ICON_SIZE, NEUTRAL, dot_pixels},
};

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

struct Ui {
    tray: Tray,
    events: Sender<Event>,
    paths: Paths,
    accent: Color,
    /// Created when opened and dropped when closed, so it costs nothing
    /// the rest of the time.
    settings: Option<SettingsWindow>,
}

/// Shows the tray icon. Must be called on the event-loop thread before the
/// loop starts.
pub fn start(events: Sender<Event>, paths: Paths) -> Result<()> {
    let tray = build_tray(&events, false, NEUTRAL)?;
    UI.with_borrow_mut(|ui| {
        *ui = Some(Ui {
            tray,
            events,
            paths,
            accent: NEUTRAL,
            settings: None,
        });
    });
    Ok(())
}

/// Recolours the tray icon and refreshes an open settings window, after
/// an apply. Callable from any thread.
pub fn show_accent(accent: Color) {
    on_ui_thread(move |ui| {
        ui.accent = accent;
        ui.tray.set_accent_icon(dot_image(accent));
        if let Some(window) = &ui.settings {
            settings::reload_preview(window);
        }
    });
}

/// Adds the icon again after Explorer restarted, which forgets every tray
/// icon. Callable from any thread.
pub fn recreate_tray() {
    on_ui_thread(|ui| {
        let paused = ui.tray.get_paused();
        match build_tray(&ui.events, paused, ui.accent) {
            Ok(tray) => ui.tray = tray,
            Err(error) => warn!("could not add the tray icon again: {error:#}"),
        }
    });
}

fn on_ui_thread(work: impl FnOnce(&mut Ui) + Send + 'static) {
    let result = slint::invoke_from_event_loop(move || {
        UI.with_borrow_mut(|ui| {
            if let Some(ui) = ui {
                work(ui);
            }
        });
    });
    if result.is_err() {
        // Only during shutdown, when the loop has already stopped.
        warn!("the user interface is not running");
    }
}

fn build_tray(events: &Sender<Event>, paused: bool, accent: Color) -> Result<Tray> {
    let tray = Tray::new()?;
    tray.set_paused(paused);
    tray.set_accent_icon(dot_image(accent));

    let sender = events.clone();
    tray.on_apply_now(move || {
        let _ = sender.send(Event::ApplyNow);
    });

    let sender = events.clone();
    let weak = tray.as_weak();
    tray.on_toggle_pause(move || {
        let Some(tray) = weak.upgrade() else { return };
        let paused = !tray.get_paused();
        tray.set_paused(paused);
        let _ = sender.send(if paused { Event::Pause } else { Event::Resume });
    });

    tray.on_open_settings(open_settings);

    let sender = events.clone();
    tray.on_quit(move || {
        let _ = sender.send(Event::Quit);
        let _ = slint::quit_event_loop();
    });
    Ok(tray)
}

fn open_settings() {
    UI.with_borrow_mut(|ui| {
        let Some(ui) = ui else { return };
        if let Some(settings) = &ui.settings {
            if let Err(error) = settings.show() {
                warn!("could not show the settings window: {error}");
            }
            return;
        }
        match create_settings(&ui.paths) {
            Ok(settings) => ui.settings = Some(settings),
            Err(error) => warn!("could not open the settings window: {error:#}"),
        }
    });
}

fn create_settings(paths: &Paths) -> Result<SettingsWindow> {
    let settings = settings::open(paths)?;
    settings.window().on_close_requested(|| {
        // Dropped on the next turn of the loop, not inside its own
        // callback.
        Timer::single_shot(Duration::ZERO, || {
            UI.with_borrow_mut(|ui| {
                if let Some(ui) = ui {
                    ui.settings = None;
                }
            });
        });
        CloseRequestResponse::HideWindow
    });
    Ok(settings)
}

fn dot_image(color: Color) -> Image {
    let pixels = dot_pixels(ICON_SIZE, color);
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &pixels, ICON_SIZE, ICON_SIZE,
    ))
}
