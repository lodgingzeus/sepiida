//! Reading a Wallpaper Engine project from disk.
//!
//! Everything here is untrusted input: `project.json` comes from the Steam
//! Workshop and the path comes from another program's output.

use std::{
    fs::File,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use crate::{Color, source::Wallpaper};

const PROJECT_FILE: &str = "project.json";
/// Real project files are a few kilobytes. The cap keeps a broken or hostile
/// file from being read into memory whole.
const MAX_PROJECT_BYTES: u64 = 1024 * 1024;
/// Longest path Windows accepts, in UTF-16 units; anything longer is not a
/// path.
const MAX_PATH_CHARS: usize = 32_767;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("`{0}` is not inside a wallpaper project folder")]
    NoProjectFolder(PathBuf),
    #[error("could not read `{path}`")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{0}` is larger than a project file should be")]
    TooLarge(PathBuf),
    #[error("`{path}` is not valid JSON")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("the project in `{0}` has neither a scheme colour nor a usable preview image")]
    NothingUsable(PathBuf),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum OutputError {
    #[error("Wallpaper Engine returned text that is not valid UTF-8")]
    NotUtf8,
    #[error("Wallpaper Engine returned something that is not a file path")]
    NotAPath,
}

/// What sepiida needs from a `project.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Project {
    /// The colour the wallpaper's author (or the user) picked for it.
    pub scheme_color: Option<Color>,
    /// File name of the preview image, relative to the project folder.
    pub preview: Option<String>,
}

impl Project {
    /// Fields that are missing or have an unexpected type are treated as
    /// absent; only malformed JSON is an error.
    pub fn parse(json: &[u8]) -> Result<Self, serde_json::Error> {
        let root: Value = serde_json::from_slice(json)?;
        let scheme_color = root
            .pointer("/general/properties/schemecolor/value")
            .and_then(Value::as_str)
            .and_then(parse_scheme_color);
        let preview = root
            .get("preview")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        Ok(Self {
            scheme_color,
            preview,
        })
    }
}

/// Parses `"0.76 0.38 0.07"`: red, green and blue as fractions of one.
pub fn parse_scheme_color(text: &str) -> Option<Color> {
    let mut channels = text.split_whitespace().map(|part| {
        let value: f64 = part.parse().ok()?;
        // The range check also rejects NaN and the infinities.
        // In range by this check, so the cast below cannot truncate.
        (0.0..=1.0)
            .contains(&value)
            .then(|| (value * 255.0).round() as u8)
    });
    let color = Color::new(channels.next()??, channels.next()??, channels.next()??);
    channels.next().is_none().then_some(color)
}

/// Extracts the path from the output of `-control getWallpaper`. `Ok(None)`
/// means Wallpaper Engine printed nothing, so no wallpaper is active.
pub fn parse_get_wallpaper_output(stdout: &[u8]) -> Result<Option<String>, OutputError> {
    let text = std::str::from_utf8(stdout).map_err(|_| OutputError::NotUtf8)?;
    let Some(line) = text.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Ok(None);
    };
    if line.chars().any(char::is_control) || line.encode_utf16().count() > MAX_PATH_CHARS {
        return Err(OutputError::NotAPath);
    }
    Ok(Some(line.to_owned()))
}

/// Resolves the wallpaper Wallpaper Engine reports (a file inside a project
/// folder, or the folder itself) to something a palette can be made from.
///
/// The project's scheme colour wins when it is set; otherwise the preview
/// image is used.
pub fn project_wallpaper(reported: &Path) -> Result<Wallpaper, ProjectError> {
    let folder = if reported.is_dir() {
        reported
    } else {
        reported
            .parent()
            .filter(|parent| parent.is_dir())
            .ok_or_else(|| ProjectError::NoProjectFolder(reported.to_owned()))?
    };

    let project_path = folder.join(PROJECT_FILE);
    let json = read_capped(&project_path)?;
    let project = Project::parse(&json).map_err(|source| ProjectError::Json {
        path: project_path,
        source,
    })?;

    if let Some(color) = project.scheme_color {
        return Ok(Wallpaper::Color(color));
    }
    project
        .preview
        .as_deref()
        .and_then(|name| preview_path(folder, name))
        .map(Wallpaper::Image)
        .ok_or_else(|| ProjectError::NothingUsable(folder.to_owned()))
}

fn read_capped(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let read_error = |source| ProjectError::Read {
        path: path.to_owned(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(read_error)?
        .take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(ProjectError::TooLarge(path.to_owned()));
    }
    Ok(bytes)
}

/// The preview name comes from the project file, so it must not be able to
/// point outside the project folder: no absolute paths, no `..`.
fn preview_path(folder: &Path, name: &str) -> Option<PathBuf> {
    // A backslash is a separator on Windows but an ordinary character
    // elsewhere; reject it everywhere so the check does not depend on the
    // platform. Preview names are plain file names in practice.
    if name.contains(['\\', ':']) {
        return None;
    }
    let relative = Path::new(name);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let path = folder.join(relative);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const SAMPLE: &str = r#"{
        "description": "A wallpaper",
        "file": "scene.json",
        "general": { "properties": { "schemecolor": {
            "order": 0, "text": "ui_browse_properties_scheme_color",
            "type": "color", "value": "0.7647058823529411 0.3764705882352941 0.07450980392156863"
        } } },
        "preview": "preview.jpg",
        "tags": ["Abstract"]
    }"#;

    fn project(json: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join(PROJECT_FILE), json).expect("write project");
        dir
    }

    #[test]
    fn scheme_color_parses_fractions_of_one() {
        assert_eq!(
            parse_scheme_color("0.7647058823529411 0.3764705882352941 0.07450980392156863"),
            Some(Color::new(195, 96, 19))
        );
        assert_eq!(parse_scheme_color("0 0 0"), Some(Color::new(0, 0, 0)));
        assert_eq!(
            parse_scheme_color("  1   1\t1 "),
            Some(Color::new(255, 255, 255))
        );
    }

    #[test]
    fn scheme_color_rejects_malformed_values() {
        for bad in [
            "", "0 0", "0 0 0 0", "a b c", "1.5 0 0", "-0.1 0 0", "NaN 0 0", "inf 0 0", "0,0,0",
        ] {
            assert_eq!(parse_scheme_color(bad), None, "input {bad:?}");
        }
    }

    #[test]
    fn project_reads_the_documented_layout() {
        assert_eq!(
            Project::parse(SAMPLE.as_bytes()).expect("parsed"),
            Project {
                scheme_color: Some(Color::new(195, 96, 19)),
                preview: Some("preview.jpg".to_owned()),
            }
        );
    }

    #[test]
    fn project_ignores_missing_and_mistyped_fields() {
        assert_eq!(Project::parse(b"{}").expect("parsed"), Project::default());
        assert_eq!(
            Project::parse(br#"{"preview": 5, "general": {"properties": "none"}}"#)
                .expect("parsed"),
            Project::default()
        );
        assert_eq!(
            Project::parse(
                br#"{"preview": "", "general": {"properties": {"schemecolor": {"value": "red"}}}}"#
            )
            .expect("parsed"),
            Project::default()
        );
        assert_eq!(Project::parse(b"[]").expect("parsed"), Project::default());
        assert!(Project::parse(b"{").is_err());
        assert!(Project::parse(b"").is_err());
    }

    #[test]
    fn output_is_trimmed_to_the_first_line() {
        assert_eq!(
            parse_get_wallpaper_output(b"\r\n  C:\\Steam\\431960\\1\\scene.pkg \r\nextra\r\n"),
            Ok(Some(r"C:\Steam\431960\1\scene.pkg".to_owned()))
        );
        assert_eq!(parse_get_wallpaper_output(b""), Ok(None));
        assert_eq!(parse_get_wallpaper_output(b" \r\n\n"), Ok(None));
    }

    #[test]
    fn output_that_is_not_a_path_is_rejected() {
        assert_eq!(
            parse_get_wallpaper_output(&[0xff, 0xfe, 0x41]),
            Err(OutputError::NotUtf8)
        );
        assert_eq!(
            parse_get_wallpaper_output(b"C:\\a\0b"),
            Err(OutputError::NotAPath)
        );
        let long = "a".repeat(MAX_PATH_CHARS + 1);
        assert_eq!(
            parse_get_wallpaper_output(long.as_bytes()),
            Err(OutputError::NotAPath)
        );
    }

    #[test]
    fn scheme_color_is_preferred_over_the_preview() {
        let dir = project(SAMPLE);
        fs::write(dir.path().join("preview.jpg"), b"x").expect("write preview");
        assert_eq!(
            project_wallpaper(&dir.path().join("scene.pkg")).expect("wallpaper"),
            Wallpaper::Color(Color::new(195, 96, 19))
        );
    }

    #[test]
    fn preview_is_used_without_a_scheme_color() {
        let dir = project(r#"{"preview": "preview.gif"}"#);
        let preview = dir.path().join("preview.gif");
        fs::write(&preview, b"x").expect("write preview");
        let expected = Wallpaper::Image(preview);
        // The reported path may be a file in the folder (existing or not) or
        // the folder itself.
        assert_eq!(
            project_wallpaper(&dir.path().join("video.mp4")).expect("wallpaper"),
            expected
        );
        assert_eq!(project_wallpaper(dir.path()).expect("wallpaper"), expected);
    }

    #[test]
    fn preview_cannot_escape_the_project_folder() {
        let outer = tempfile::tempdir().expect("temp dir");
        fs::write(outer.path().join("secret.png"), b"x").expect("write file");
        let folder = outer.path().join("project");
        fs::create_dir(&folder).expect("create folder");
        let absolute = outer.path().join("secret.png");

        for name in [
            "../secret.png",
            "..\\secret.png",
            "sub/../../secret.png",
            "C:\\secret.png",
            absolute.to_str().expect("utf-8 path"),
        ] {
            assert_eq!(preview_path(&folder, name), None, "name {name:?}");
        }

        fs::write(folder.join(PROJECT_FILE), r#"{"preview": "../secret.png"}"#)
            .expect("write project");
        assert!(matches!(
            project_wallpaper(&folder.join("scene.pkg")),
            Err(ProjectError::NothingUsable(_))
        ));
    }

    #[test]
    fn preview_may_sit_in_a_subfolder_but_must_be_a_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir(dir.path().join("media")).expect("create folder");
        fs::write(dir.path().join("media/preview.png"), b"x").expect("write preview");
        assert_eq!(
            preview_path(dir.path(), "media/preview.png"),
            Some(dir.path().join("media/preview.png"))
        );
        assert_eq!(preview_path(dir.path(), "media"), None);
        assert_eq!(preview_path(dir.path(), "missing.png"), None);
    }

    #[test]
    fn broken_projects_are_reported() {
        let empty = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            project_wallpaper(&empty.path().join("scene.pkg")),
            Err(ProjectError::Read { .. })
        ));

        let invalid = project("{ not json");
        assert!(matches!(
            project_wallpaper(&invalid.path().join("scene.pkg")),
            Err(ProjectError::Json { .. })
        ));

        let nothing = project(r#"{"preview": "missing.jpg"}"#);
        assert!(matches!(
            project_wallpaper(&nothing.path().join("scene.pkg")),
            Err(ProjectError::NothingUsable(_))
        ));

        assert!(matches!(
            project_wallpaper(&empty.path().join("no-such-folder").join("scene.pkg")),
            Err(ProjectError::NoProjectFolder(_))
        ));
    }

    #[test]
    fn oversized_project_files_are_not_parsed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = File::create(dir.path().join(PROJECT_FILE)).expect("create project");
        file.set_len(MAX_PROJECT_BYTES + 1).expect("grow project");
        assert!(matches!(
            project_wallpaper(&dir.path().join("scene.pkg")),
            Err(ProjectError::TooLarge(_))
        ));
    }
}
