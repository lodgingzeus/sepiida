//! The plain-data side of the settings window: the order of the choices in
//! its drop-downs and the "start with Windows" command line. Kept free of
//! UI and Windows code so it can be tested anywhere.

use std::path::Path;

use sepiida_core::{config::ModeSetting, source::SourceChoice};

/// Drop-down order of the wallpaper source; must match `settings.slint`.
pub const SOURCES: [SourceChoice; 3] = [
    SourceChoice::Auto,
    SourceChoice::Windows,
    SourceChoice::WallpaperEngine,
];

/// Drop-down order of the light/dark setting; must match `settings.slint`.
pub const MODES: [ModeSetting; 4] = [
    ModeSetting::Unchanged,
    ModeSetting::Light,
    ModeSetting::Dark,
    ModeSetting::FollowWallpaper,
];

pub fn source_index(choice: SourceChoice) -> i32 {
    index_of(&SOURCES, choice)
}

/// `None` for an index the drop-down cannot produce.
pub fn source_at(index: i32) -> Option<SourceChoice> {
    at(&SOURCES, index)
}

pub fn mode_index(mode: ModeSetting) -> i32 {
    index_of(&MODES, mode)
}

pub fn mode_at(index: i32) -> Option<ModeSetting> {
    at(&MODES, index)
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
            assert_eq!(mode_index(*mode), index);
            assert_eq!(mode_at(index), Some(*mode));
        }
    }

    #[test]
    fn out_of_range_indexes_are_rejected() {
        for index in [-1, 4, i32::MAX, i32::MIN] {
            assert_eq!(mode_at(index), None);
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
