//! Wallpaper Engine's `config.json`: which wallpaper it shows.
//!
//! `wallpaper64.exe -control getWallpaper` does not write its answer to
//! the stdout pipe it is given (it prints to the parent's console, if
//! any), so the current wallpaper is read from the settings file instead.
//! Seen on Windows 11 with config version 5:
//!
//! ```json
//! { "<steam user>": { "general": { "wallpaperconfig": {
//!     "selectedwallpapers": { "Monitor0": { "file": "C:/…/scene.pkg" } }
//! } } } }
//! ```
//!
//! The layout is undocumented, so the `selectedwallpapers` object is
//! searched for rather than reached by a fixed path. Monitors can also be
//! keyed by a device path (`\\?\DISPLAY#…`).

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

use crate::engine_tracker::CONFIG_FILE;

/// Real files are a few kilobytes; playlists add a little.
const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
/// `selectedwallpapers` sits four levels down; this leaves room for
/// layout changes without walking arbitrarily deep input.
const MAX_SEARCH_DEPTH: usize = 8;
/// Longest path Windows accepts, in UTF-16 units.
const MAX_PATH_CHARS: usize = 32_767;
/// Preferred when several monitors have a wallpaper. sepiida themes from
/// one monitor (see the plan); Wallpaper Engine numbers from 0.
const FIRST_MONITOR: &str = "Monitor0";

#[derive(Debug, thiserror::Error)]
pub enum EngineConfigError {
    #[error("could not read `{path}`")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{0}` is larger than Wallpaper Engine's settings should be")]
    TooLarge(PathBuf),
    #[error("`{path}` is not valid JSON")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// The wallpaper file Wallpaper Engine has selected, from the
/// `config.json` in its install folder. `Ok(None)` when none is selected.
pub fn read_selected_wallpaper(install_folder: &Path) -> Result<Option<String>, EngineConfigError> {
    let path = install_folder.join(CONFIG_FILE);
    let read_error = |source| EngineConfigError::Read {
        path: path.clone(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(&path)
        .map_err(read_error)?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CONFIG_BYTES {
        return Err(EngineConfigError::TooLarge(path));
    }
    selected_wallpaper(&bytes).map_err(|source| EngineConfigError::Json { path, source })
}

/// Picks the selected wallpaper out of `config.json`: `Monitor0` if it has
/// one, otherwise the monitor whose key sorts first. Entries that are not
/// plausible paths are skipped.
pub fn selected_wallpaper(json: &[u8]) -> Result<Option<String>, serde_json::Error> {
    let root: Value = serde_json::from_slice(json)?;
    let mut found = Vec::new();
    collect_selected(&root, 0, &mut found);
    found.sort_by(|(a, _), (b, _)| (*a != FIRST_MONITOR, a).cmp(&(*b != FIRST_MONITOR, b)));
    Ok(found.into_iter().next().map(|(_, file)| file.to_owned()))
}

fn collect_selected<'a>(value: &'a Value, depth: usize, found: &mut Vec<(&'a str, &'a str)>) {
    let Value::Object(map) = value else { return };
    if depth > MAX_SEARCH_DEPTH {
        return;
    }
    for (key, child) in map {
        if key == "selectedwallpapers" {
            if let Value::Object(monitors) = child {
                found.extend(monitor_files(monitors));
            }
        } else {
            collect_selected(child, depth + 1, found);
        }
    }
}

fn monitor_files(monitors: &Map<String, Value>) -> impl Iterator<Item = (&str, &str)> {
    monitors.iter().filter_map(|(monitor, entry)| {
        let file = entry.get("file")?.as_str()?;
        is_plausible_path(file).then_some((monitor.as_str(), file))
    })
}

fn is_plausible_path(text: &str) -> bool {
    !text.trim().is_empty()
        && text.encode_utf16().count() <= MAX_PATH_CHARS
        && !text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout seen on a real install, trimmed.
    const REAL: &str = r#"{
        "someuser" : {
            "general" : {
                "browser" : { "theme" : "dark" },
                "wallpaperconfig" : {
                    "profile" : null,
                    "selectedwallpapers" : {
                        "Monitor0" : {
                            "file" : "C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/1660044780/2.rain60.mp4"
                        }
                    }
                }
            },
            "version" : 5,
            "wproperties" : {}
        }
    }"#;

    #[test]
    fn reads_the_real_layout() {
        assert_eq!(
            selected_wallpaper(REAL.as_bytes())
                .expect("parse")
                .as_deref(),
            Some(
                "C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/1660044780/2.rain60.mp4"
            )
        );
    }

    #[test]
    fn prefers_monitor0_then_the_first_key() {
        let json = r#"{"u":{"selectedwallpapers":{
            "Monitor1":{"file":"C:/b/one.pkg"},
            "Monitor0":{"file":"C:/a/zero.pkg"}}}}"#;
        assert_eq!(
            selected_wallpaper(json.as_bytes())
                .expect("parse")
                .as_deref(),
            Some("C:/a/zero.pkg")
        );
        let json = r#"{"u":{"selectedwallpapers":{
            "\\\\?\\DISPLAY#B":{"file":"C:/b/b.pkg"},
            "\\\\?\\DISPLAY#A":{"file":"C:/a/a.pkg"}}}}"#;
        assert_eq!(
            selected_wallpaper(json.as_bytes())
                .expect("parse")
                .as_deref(),
            Some("C:/a/a.pkg")
        );
    }

    #[test]
    fn nothing_selected_is_none() {
        for json in [
            r#"{}"#,
            r#"{"u":{"general":{"wallpaperconfig":{"selectedwallpapers":{}}}}}"#,
            r#"{"u":{"selectedwallpapers":{"Monitor0":{}}}}"#,
            r#"{"u":{"selectedwallpapers":{"Monitor0":{"file":""}}}}"#,
            r#"{"u":{"selectedwallpapers":{"Monitor0":{"file":7}}}}"#,
            r#"{"u":{"selectedwallpapers":{"Monitor0":{"file":"C:/a\nb"}}}}"#,
            r#"{"u":{"selectedwallpapers":["C:/a.pkg"]}}"#,
            r#"[1,2,3]"#,
        ] {
            assert_eq!(
                selected_wallpaper(json.as_bytes()).expect("parse"),
                None,
                "{json}"
            );
        }
    }

    #[test]
    fn does_not_search_arbitrarily_deep() {
        let mut json = r#"{"selectedwallpapers":{"Monitor0":{"file":"C:/deep.pkg"}}}"#.to_owned();
        for _ in 0..=MAX_SEARCH_DEPTH {
            json = format!(r#"{{"x":{json}}}"#);
        }
        assert_eq!(selected_wallpaper(json.as_bytes()).expect("parse"), None);
    }

    #[test]
    fn reads_the_file_from_the_install_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            read_selected_wallpaper(dir.path()),
            Err(EngineConfigError::Read { .. })
        ));
        std::fs::write(dir.path().join(CONFIG_FILE), REAL).expect("write");
        assert!(read_selected_wallpaper(dir.path()).expect("read").is_some());
        std::fs::write(dir.path().join(CONFIG_FILE), "{broken").expect("write");
        assert!(matches!(
            read_selected_wallpaper(dir.path()),
            Err(EngineConfigError::Json { .. })
        ));
    }
}
