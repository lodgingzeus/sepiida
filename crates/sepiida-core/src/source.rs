//! Where the wallpaper comes from, and which source wins.

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::Color;

/// What a source is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wallpaper {
    Image(PathBuf),
    /// A solid-colour background, or a colour the wallpaper declares for
    /// itself. There is no image to analyse.
    Color(Color),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Windows,
    WallpaperEngine,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Windows => "Windows",
            Self::WallpaperEngine => "Wallpaper Engine",
        })
    }
}

/// The user's wallpaper source setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceChoice {
    /// Wallpaper Engine when it is showing a wallpaper, otherwise Windows.
    #[default]
    Auto,
    Windows,
    WallpaperEngine,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown wallpaper source `{0}`; expected auto, windows or wallpaper-engine")]
pub struct UnknownSourceChoice(String);

impl FromStr for SourceChoice {
    type Err = UnknownSourceChoice;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(Self::Auto),
            "windows" => Ok(Self::Windows),
            "wallpaper-engine" => Ok(Self::WallpaperEngine),
            other => Err(UnknownSourceChoice(other.to_owned())),
        }
    }
}

/// Turns what `IDesktopWallpaper` reports into a wallpaper.
///
/// `reported` is the picture path for the monitor; Windows returns an empty
/// string when the background is a solid colour. `transcoded` is the copy of
/// the current wallpaper that Windows keeps, used when the reported file has
/// gone (a deleted download, a disconnected drive).
pub fn windows_wallpaper(
    reported: &str,
    transcoded: Option<&Path>,
    background: Color,
) -> Option<Wallpaper> {
    if reported.is_empty() {
        return Some(Wallpaper::Color(background));
    }
    [Some(Path::new(reported)), transcoded]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .map(|path| Wallpaper::Image(path.to_owned()))
}

pub type SourceError = Box<dyn std::error::Error + Send + Sync>;

pub trait WallpaperSource {
    /// `Ok(None)` means the source is working but showing nothing, for
    /// example Wallpaper Engine not running.
    fn current(&self) -> Result<Option<Wallpaper>, SourceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("{0} is not showing a wallpaper")]
    NothingShown(SourceKind),
    #[error("could not read the {kind} wallpaper")]
    Source {
        kind: SourceKind,
        #[source]
        error: SourceError,
    },
}

#[derive(Debug)]
pub struct Resolved {
    pub kind: SourceKind,
    pub wallpaper: Wallpaper,
    /// Set when Auto wanted Wallpaper Engine, it failed, and Windows was used
    /// instead. Worth showing to the user; not fatal.
    pub wallpaper_engine_error: Option<SourceError>,
}

/// Picks the wallpaper to theme from. An explicit choice never falls back to
/// the other source: the user asked for that one.
pub fn resolve(
    choice: SourceChoice,
    windows: &dyn WallpaperSource,
    wallpaper_engine: &dyn WallpaperSource,
) -> Result<Resolved, ResolveError> {
    match choice {
        SourceChoice::Windows => read(SourceKind::Windows, windows, None),
        SourceChoice::WallpaperEngine => read(SourceKind::WallpaperEngine, wallpaper_engine, None),
        SourceChoice::Auto => match wallpaper_engine.current() {
            Ok(Some(wallpaper)) => Ok(Resolved {
                kind: SourceKind::WallpaperEngine,
                wallpaper,
                wallpaper_engine_error: None,
            }),
            Ok(None) => read(SourceKind::Windows, windows, None),
            Err(error) => read(SourceKind::Windows, windows, Some(error)),
        },
    }
}

fn read(
    kind: SourceKind,
    source: &dyn WallpaperSource,
    wallpaper_engine_error: Option<SourceError>,
) -> Result<Resolved, ResolveError> {
    match source.current() {
        Ok(Some(wallpaper)) => Ok(Resolved {
            kind,
            wallpaper,
            wallpaper_engine_error,
        }),
        Ok(None) => Err(ResolveError::NothingShown(kind)),
        Err(error) => Err(ResolveError::Source { kind, error }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Fake {
        Shows(Wallpaper),
        Nothing,
        Fails,
    }

    impl WallpaperSource for Fake {
        fn current(&self) -> Result<Option<Wallpaper>, SourceError> {
            match self {
                Self::Shows(w) => Ok(Some(w.clone())),
                Self::Nothing => Ok(None),
                Self::Fails => Err("broken".into()),
            }
        }
    }

    fn picture() -> Wallpaper {
        Wallpaper::Image(PathBuf::from("picture.jpg"))
    }

    fn scene() -> Wallpaper {
        Wallpaper::Color(Color::new(1, 2, 3))
    }

    #[test]
    fn windows_wallpaper_handles_solid_colours_and_missing_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let picture = dir.path().join("picture.jpg");
        let transcoded = dir.path().join("TranscodedWallpaper");
        let missing = dir.path().join("missing.jpg");
        std::fs::write(&picture, b"x").expect("write file");
        std::fs::write(&transcoded, b"x").expect("write file");
        let background = Color::new(9, 8, 7);
        let reported = |path: &Path| path.to_str().expect("utf-8 path").to_owned();

        assert_eq!(
            windows_wallpaper("", Some(&transcoded), background),
            Some(Wallpaper::Color(background))
        );
        assert_eq!(
            windows_wallpaper(&reported(&picture), Some(&transcoded), background),
            Some(Wallpaper::Image(picture))
        );
        assert_eq!(
            windows_wallpaper(&reported(&missing), Some(&transcoded), background),
            Some(Wallpaper::Image(transcoded))
        );
        assert_eq!(
            windows_wallpaper(&reported(&missing), Some(&missing), background),
            None
        );
        assert_eq!(
            windows_wallpaper(&reported(&missing), None, background),
            None
        );
        // A directory is not a picture.
        assert_eq!(
            windows_wallpaper(&reported(dir.path()), None, background),
            None
        );
    }

    #[test]
    fn choice_parses_the_three_names_only() {
        assert_eq!("auto".parse(), Ok(SourceChoice::Auto));
        assert_eq!("windows".parse(), Ok(SourceChoice::Windows));
        assert_eq!(
            "wallpaper-engine".parse(),
            Ok(SourceChoice::WallpaperEngine)
        );
        assert!("Windows".parse::<SourceChoice>().is_err());
        assert!("".parse::<SourceChoice>().is_err());
    }

    #[test]
    fn auto_prefers_wallpaper_engine_when_it_shows_something() {
        let r = resolve(
            SourceChoice::Auto,
            &Fake::Shows(picture()),
            &Fake::Shows(scene()),
        )
        .expect("resolved");
        assert_eq!(r.kind, SourceKind::WallpaperEngine);
        assert_eq!(r.wallpaper, scene());
        assert!(r.wallpaper_engine_error.is_none());
    }

    #[test]
    fn auto_uses_windows_when_wallpaper_engine_shows_nothing() {
        let r =
            resolve(SourceChoice::Auto, &Fake::Shows(picture()), &Fake::Nothing).expect("resolved");
        assert_eq!(r.kind, SourceKind::Windows);
        assert_eq!(r.wallpaper, picture());
        assert!(r.wallpaper_engine_error.is_none());
    }

    #[test]
    fn auto_falls_back_to_windows_and_keeps_the_wallpaper_engine_error() {
        let r =
            resolve(SourceChoice::Auto, &Fake::Shows(picture()), &Fake::Fails).expect("resolved");
        assert_eq!(r.kind, SourceKind::Windows);
        assert!(r.wallpaper_engine_error.is_some());
    }

    #[test]
    fn auto_fails_when_neither_source_has_a_wallpaper() {
        assert!(matches!(
            resolve(SourceChoice::Auto, &Fake::Nothing, &Fake::Nothing),
            Err(ResolveError::NothingShown(SourceKind::Windows))
        ));
        assert!(matches!(
            resolve(SourceChoice::Auto, &Fake::Fails, &Fake::Fails),
            Err(ResolveError::Source {
                kind: SourceKind::Windows,
                ..
            })
        ));
    }

    #[test]
    fn explicit_choice_never_falls_back() {
        assert!(matches!(
            resolve(
                SourceChoice::WallpaperEngine,
                &Fake::Shows(picture()),
                &Fake::Nothing
            ),
            Err(ResolveError::NothingShown(SourceKind::WallpaperEngine))
        ));
        assert!(matches!(
            resolve(
                SourceChoice::WallpaperEngine,
                &Fake::Shows(picture()),
                &Fake::Fails
            ),
            Err(ResolveError::Source {
                kind: SourceKind::WallpaperEngine,
                ..
            })
        ));
        let r = resolve(
            SourceChoice::Windows,
            &Fake::Shows(picture()),
            &Fake::Shows(scene()),
        )
        .expect("resolved");
        assert_eq!(r.kind, SourceKind::Windows);
    }
}
