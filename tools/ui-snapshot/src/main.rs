//! Renders `SettingsWindow` off-screen with sample data and saves a PNG,
//! to check the layout on Linux:
//! `cargo run -p ui-snapshot -- out.png [--dark] [--picker] [--size WxH] [--scale S]`.
//! The colours come from the real colour engine and a generated wallpaper.

use std::rc::Rc;

use anyhow::{Context, Result, anyhow, bail};
use image::{DynamicImage, Rgb, RgbImage};
use sepiida_core::{
    Color, Palette, Style,
    settings_look::{self, ShellPreview, WindowColors as Look, picker_color},
};
use slint::{
    ComponentHandle, Image, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, VecModel,
    platform::{
        Platform, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

slint::include_modules!();

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

struct Options {
    out: String,
    dark: bool,
    picker: bool,
    width: u32,
    height: u32,
    scale: f32,
    style: Style,
}

fn options() -> Result<Options> {
    let mut o = Options {
        out: "settings.png".to_owned(),
        dark: false,
        picker: false,
        width: 880,
        height: 640,
        scale: 1.0,
        style: Style::Soft,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dark" => o.dark = true,
            "--picker" => o.picker = true,
            "--vivid" => o.style = Style::Vivid,
            "--neutral" => o.style = Style::Neutral,
            "--faithful" => o.style = Style::Faithful,
            "--size" => {
                let size = args.next().context("--size needs WxH")?;
                let (w, h) = size.split_once('x').context("--size needs WxH")?;
                (o.width, o.height) = (w.parse()?, h.parse()?);
            }
            "--scale" => o.scale = args.next().context("--scale needs a number")?.parse()?,
            other if other.starts_with("--") => bail!("unknown option {other}"),
            other => o.out = other.to_owned(),
        }
    }
    Ok(o)
}

/// A dusk landscape: warm sky, rose haze, dark ridge.
fn sample_wallpaper() -> RgbImage {
    let (w, h) = (640u32, 400u32);
    RgbImage::from_fn(w, h, |x, y| {
        let t = y as f32 / h as f32;
        let ridge = 0.62 + 0.08 * ((x as f32 / 90.0).sin() + (x as f32 / 37.0).cos() * 0.4);
        if t > ridge {
            return Rgb([13, 19, 34]);
        }
        let sun = (1.0 - ((x as f32 - 470.0).powi(2) + (y as f32 - 60.0).powi(2)).sqrt() / 420.0)
            .max(0.0);
        let mix = |a: f32, b: f32, k: f32| (a + (b - a) * k).clamp(0.0, 255.0) as u8;
        Rgb([
            mix(44.0 + 150.0 * (1.0 - t), 242.0, sun * 0.8),
            mix(58.0 + 40.0 * (1.0 - t), 178.0, sun * 0.8),
            mix(99.0 + 20.0 * (1.0 - t), 124.0, sun * 0.8),
        ])
    })
}

fn rgb(c: Color) -> slint::Color {
    slint::Color::from_rgb_u8(c.r, c.g, c.b)
}

fn look(c: Look) -> WindowColors {
    WindowColors {
        background: rgb(c.background),
        card: rgb(c.card),
        chip: rgb(c.chip),
        outline: rgb(c.outline),
        text: rgb(c.text),
        muted_text: rgb(c.muted_text),
        accent: rgb(c.accent),
        on_accent: rgb(c.on_accent),
    }
}

fn shell(p: &Palette, dark: bool) -> ShellColors {
    let c = ShellPreview::new(p, dark);
    ShellColors {
        surface: rgb(c.surface),
        card: rgb(c.card),
        accent: rgb(c.accent),
    }
}

fn fill(ui: &SettingsWindow, o: &Options) {
    let wallpaper = sample_wallpaper();
    let analysed = Palette::from_image(&DynamicImage::ImageRgb8(wallpaper.clone()));
    let palette = analysed.with_choice(analysed.source, o.style);

    let theme = ui.global::<Theme>();
    theme.set_light(look(Look::new(&palette, false)));
    theme.set_dark(look(Look::new(&palette, true)));
    theme.set_is_dark(o.dark);

    let rgba = DynamicImage::ImageRgb8(wallpaper).into_rgba8();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    );
    ui.set_wallpaper(Image::from_rgba8(buffer));
    ui.set_has_wallpaper(true);
    ui.set_shell(shell(&palette, o.dark));
    ui.set_shell_dark(o.dark);
    ui.set_preview_text("From the Wallpaper Engine wallpaper".into());

    let chips: Vec<ChipColors> = settings_look::chip_colors(&palette)
        .into_iter()
        .map(|c| ChipColors {
            accent: rgb(c.accent),
            light: rgb(c.light),
            dark: rgb(c.dark),
        })
        .collect();
    ui.set_candidates(ModelRc::new(VecModel::from(chips)));
    ui.set_candidate_index(0);
    ui.set_style_index(Style::ALL.iter().position(|&s| s == o.style).unwrap_or(1) as i32);
    ui.set_mode_enabled(!o.picker);
    ui.set_mode_index(2);
    ui.set_look_index(0);
    ui.set_source_index(0);
    ui.set_start_with_windows(true);

    let stops: Vec<slint::Color> = settings_look::hue_stops().map(rgb).to_vec();
    ui.set_picker_hue_stops(ModelRc::new(VecModel::from(stops)));
    ui.set_picker_open(o.picker);
    ui.set_picker_hue(0.55);
    ui.set_picker_colourfulness(0.6);
    let picked = picker_color(0.55 * 360.0, 0.6 * 80.0);
    ui.set_picker_color(rgb(picked));
    ui.set_picker_grey(rgb(picker_color(198.0, 0.0)));
    ui.set_picker_vivid(rgb(picker_color(198.0, 80.0)));
    ui.set_picker_hex(picked.to_string().into());
}

fn main() -> Result<()> {
    let o = options()?;
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone())))
        .map_err(|e| anyhow!("{e:?}"))?;
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: o.scale,
    });
    let ui = SettingsWindow::new()?;
    fill(&ui, &o);
    window.set_size(PhysicalSize::new(o.width, o.height));
    ui.show()?;

    let shot = window.take_snapshot().map_err(|e| anyhow!("{e:?}"))?;
    image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_bytes().to_vec())
        .context("snapshot size mismatch")?
        .save(&o.out)
        .with_context(|| format!("could not save {}", o.out))?;
    println!("wrote {} ({}x{})", o.out, shot.width(), shot.height());
    Ok(())
}
