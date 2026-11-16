//! The plain-data side of the settings window: the order of the options
//! in its button rows and the "start with Windows" command line. Kept free
//! of UI and Windows code so it can be tested anywhere.

use std::path::Path;

use sepiida_core::{
    Style,
    config::{Config, ModeSetting},
    shell::rules::Material,
    source::SourceChoice,
};

/// Order of the wallpaper source buttons; must match `settings.slint`.
pub const SOURCES: [SourceChoice; 3] = [
    SourceChoice::Auto,
    SourceChoice::Windows,
    SourceChoice::WallpaperEngine,
];

/// Order of the light/dark buttons; must match `settings.slint`. Leaving
/// the mode alone (`Unchanged`) is the switch beside them, not a button.
pub const MODES: [ModeSetting; 3] = [
    ModeSetting::Light,
    ModeSetting::Dark,
    ModeSetting::FollowWallpaper,
];

/// The button shown while sepiida leaves the mode alone, and selected when
/// the switch is turned on.
const DEFAULT_MODE: ModeSetting = ModeSetting::FollowWallpaper;

/// Order of the style chips; must match `settings.slint`.
pub const STYLES: [Style; 4] = Style::ALL;

/// The shell look buttons: glass, solid, or Windows' own look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Glass,
    Solid,
    Off,
}

/// Order of the shell look buttons; must match `settings.slint`.
pub const LOOKS: [Look; 3] = [Look::Glass, Look::Solid, Look::Off];

impl Look {
    pub fn of(config: &Config) -> Self {
        match (config.shell_styling, config.material) {
            (false, _) => Self::Off,
            (true, Material::Glass) => Self::Glass,
            (true, Material::Solid) => Self::Solid,
        }
    }

    /// Off keeps the material, so turning styling back on restores it.
    pub fn apply(self, config: &mut Config) {
        match self {
            Self::Glass => (config.shell_styling, config.material) = (true, Material::Glass),
            Self::Solid => (config.shell_styling, config.material) = (true, Material::Solid),
            Self::Off => config.shell_styling = false,
        }
    }
}

pub fn style_index(style: Style) -> i32 {
    index_of(&STYLES, style)
}

pub fn style_at(index: i32) -> Option<Style> {
    at(&STYLES, index)
}

pub fn look_index(look: Look) -> i32 {
    index_of(&LOOKS, look)
}

pub fn look_at(index: i32) -> Option<Look> {
    at(&LOOKS, index)
}

pub fn source_index(choice: SourceChoice) -> i32 {
    index_of(&SOURCES, choice)
}

/// `None` for an index the drop-down cannot produce.
pub fn source_at(index: i32) -> Option<SourceChoice> {
    at(&SOURCES, index)
}

/// Whether sepiida sets light/dark mode, and which button is selected.
pub fn mode_controls(mode: ModeSetting) -> (bool, i32) {
    match mode {
        ModeSetting::Unchanged => (false, index_of(&MODES, DEFAULT_MODE)),
        mode => (true, index_of(&MODES, mode)),
    }
}

/// The setting for the switch and the selected button. `None` for an
/// index the buttons cannot produce.
pub fn mode_from(enabled: bool, index: i32) -> Option<ModeSetting> {
    if enabled {
        at(&MODES, index)
    } else {
        Some(ModeSetting::Unchanged)
    }
}

fn index_of<T: PartialEq>(items: &[T], item: T) -> i32 {
    items
        .iter()
        .position(|i| *i == item)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

fn at<T: Copy>(items: &[T], index: i32) -> Option<T> {
    usize::try_from(index)
        .ok()
        .and_then(|i| items.get(i))
        .copied()
}

/// The command Windows runs at sign-in: the quoted path of the windowless
/// program. `None` if the path cannot be quoted safely (it contains a quote
/// or is not valid Unicode); a path is never written half-escaped.
pub fn startup_command(windowless_exe: &Path) -> Option<String> {
    let path = windowless_exe.to_str()?;
    if path.is_empty() || path.contains('"') || path.chars().any(char::is_control) {
        return None;
    }
    Some(format!("\"{path}\""))
}

/// Whether a `Run` entry starts this copy of sepiida. Windows paths are
/// compared without regard to case.
pub fn is_our_startup_entry(value: &str, expected: &str) -> bool {
    value.trim().eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_round_trip() {
        for (i, choice) in SOURCES.iter().enumerate() {
            let index = i32::try_from(i).expect("small");
            assert_eq!(source_index(*choice), index);
            assert_eq!(source_at(index), Some(*choice));
        }
        for (i, mode) in MODES.iter().enumerate() {
            let index = i32::try_from(i).expect("small");
            assert_eq!(mode_controls(*mode), (true, index));
            assert_eq!(mode_from(true, index), Some(*mode));
        }
    }

    #[test]
    fn leaving_the_mode_alone_is_the_switch_not_a_button() {
        let (enabled, index) = mode_controls(ModeSetting::Unchanged);
        assert!(!enabled);
        assert_eq!(mode_from(true, index), Some(ModeSetting::FollowWallpaper));
        for index in [0, 2, 99] {
            assert_eq!(mode_from(false, index), Some(ModeSetting::Unchanged));
        }
        assert!(!MODES.contains(&ModeSetting::Unchanged));
    }

    #[test]
    fn style_and_look_indexes_round_trip() {
        for (i, style) in STYLES.iter().enumerate() {
            let index = i32::try_from(i).expect("small");
            assert_eq!(style_index(*style), index);
            assert_eq!(style_at(index), Some(*style));
        }
        for (i, look) in LOOKS.iter().enumerate() {
            let index = i32::try_from(i).expect("small");
            assert_eq!(look_index(*look), index);
            assert_eq!(look_at(index), Some(*look));
        }
        assert_eq!(style_at(4), None);
        assert_eq!(look_at(-1), None);
    }

    #[test]
    fn looks_map_to_styling_and_material() {
        let mut config = Config::default();
        assert_eq!(Look::of(&config), Look::Off, "styling is off by default");
        Look::Solid.apply(&mut config);
        assert_eq!(Look::of(&config), Look::Solid);
        Look::Off.apply(&mut config);
        assert_eq!(Look::of(&config), Look::Off);
        assert_eq!(config.material, Material::Solid, "off keeps the material");
        Look::Glass.apply(&mut config);
        assert_eq!(
            (config.shell_styling, config.material),
            (true, Material::Glass)
        );
    }

    #[test]
    fn out_of_range_indexes_are_rejected() {
        for index in [-1, 3, i32::MAX, i32::MIN] {
            assert_eq!(mode_from(true, index), None);
        }
        assert_eq!(source_at(3), None);
    }

    #[test]
    fn startup_command_quotes_the_path() {
        assert_eq!(
            startup_command(Path::new(r"C:\Program Files\sepiida\sepiidaw.exe")).as_deref(),
            Some(r#""C:\Program Files\sepiida\sepiidaw.exe""#)
        );
        assert_eq!(startup_command(Path::new(r#"C:\a"b\sepiidaw.exe"#)), None);
        assert_eq!(startup_command(Path::new("C:\\a\nb.exe")), None);
        assert_eq!(startup_command(Path::new("")), None);
    }

    #[test]
    fn startup_entries_match_without_case() {
        let expected = r#""C:\Apps\sepiidaw.exe""#;
        assert!(is_our_startup_entry(r#""c:\apps\SEPIIDAW.EXE""#, expected));
        assert!(is_our_startup_entry(&format!(" {expected} "), expected));
        assert!(!is_our_startup_entry(
            r#""C:\Other\sepiidaw.exe""#,
            expected
        ));
    }
}
