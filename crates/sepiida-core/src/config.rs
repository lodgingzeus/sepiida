//! User settings, stored as TOML.
//!
//! The file is external input: the user may edit it by hand, and an older
//! or newer sepiida may have written it. Missing fields take their default
//! and unknown fields are ignored, so the file survives upgrades in both
//! directions. A file that cannot be read at all is reported to the caller,
//! which falls back to the defaults and leaves the file alone so the user
//! can fix it.

use std::{io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{Color, accent::ThemeMode, fsio, source::SourceChoice};

/// A hand-edited settings file is a few hundred bytes.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// What sepiida does with light/dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModeSetting {
    /// Never touch the user's light/dark setting.
    #[default]
    Unchanged,
    Light,
    Dark,
}

impl ModeSetting {
    /// The mode to write, or `None` to leave it as it is.
    pub const fn theme_mode(self) -> Option<ThemeMode> {
        match self {
            Self::Unchanged => None,
            Self::Light => Some(ThemeMode::Light),
            Self::Dark => Some(ThemeMode::Dark),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub source: SourceChoice,
    pub mode: ModeSetting,
    /// Write the Windows accent colour.
    pub accent: bool,
    /// Restyle the taskbar, Start menu and notification centre (M4).
    pub shell_styling: bool,
    /// Translucent shell surfaces; experimental (M5).
    pub glass: bool,
    /// When set, this colour is used instead of the wallpaper's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_color: Option<Color>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: SourceChoice::default(),
            mode: ModeSetting::default(),
            accent: true,
            shell_styling: false,
            glass: false,
            locked_color: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read the settings file")]
    Read(#[source] io::Error),
    #[error("the settings file is not valid")]
    Parse(#[from] toml::de::Error),
    #[error("could not encode the settings")]
    Encode(#[from] toml::ser::Error),
    #[error("could not write the settings file")]
    Write(#[source] io::Error),
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string(self)?)
    }

    /// `Ok(None)` when there is no settings file yet.
    pub fn load(path: &Path) -> Result<Option<Self>, ConfigError> {
        let bytes = match fsio::read_capped(path, MAX_CONFIG_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ConfigError::Read(error)),
        };
        let text = String::from_utf8(bytes)
            .map_err(|e| ConfigError::Read(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        Self::from_toml(&text).map(Some)
    }

    /// Never fails: a missing file gives the defaults silently, a broken
    /// one gives the defaults and the error, for the caller to log.
    pub fn load_or_default(path: &Path) -> (Self, Option<ConfigError>) {
        match Self::load(path) {
            Ok(config) => (config.unwrap_or_default(), None),
            Err(error) => (Self::default(), Some(error)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        fsio::write_atomic(path, self.to_toml()?.as_bytes()).map_err(ConfigError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_only_write_the_accent_colour() {
        let config = Config::default();
        assert_eq!(config.source, SourceChoice::Auto);
        assert_eq!(config.mode, ModeSetting::Unchanged);
        assert_eq!(config.mode.theme_mode(), None);
        assert!(config.accent);
        assert!(!config.shell_styling && !config.glass);
        assert_eq!(config.locked_color, None);
    }

    #[test]
    fn an_empty_file_gives_the_defaults() {
        assert_eq!(Config::from_toml("").expect("parse"), Config::default());
    }

    #[test]
    fn round_trips_every_field() {
        let config = Config {
            source: SourceChoice::WallpaperEngine,
            mode: ModeSetting::Dark,
            accent: false,
            shell_styling: true,
            glass: true,
            locked_color: Some(Color::new(0x12, 0xab, 0xef)),
        };
        let text = config.to_toml().expect("encode");
        assert!(text.contains("source = \"wallpaper-engine\""), "{text}");
        assert!(text.contains("locked-color = \"#12abef\""), "{text}");
        assert_eq!(Config::from_toml(&text).expect("parse"), config);
    }

    #[test]
    fn missing_fields_default_and_unknown_fields_are_ignored() {
        let config = Config::from_toml("mode = \"light\"\nfuture-setting = 3\n").expect("parse");
        assert_eq!(
            config,
            Config {
                mode: ModeSetting::Light,
                ..Config::default()
            }
        );
    }

    #[test]
    fn invalid_values_are_rejected() {
        for text in [
            "source = \"kde\"",
            "mode = \"auto\"",
            "accent = \"yes\"",
            "locked-color = \"red\"",
            "source = ",
        ] {
            assert!(
                matches!(Config::from_toml(text), Err(ConfigError::Parse(_))),
                "{text:?} should be rejected"
            );
        }
    }

    #[test]
    fn load_handles_missing_broken_and_valid_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sepiida").join("config.toml");

        assert_eq!(Config::load(&path).expect("missing is fine"), None);
        let (config, error) = Config::load_or_default(&path);
        assert_eq!(config, Config::default());
        assert!(error.is_none());

        let saved = Config {
            glass: true,
            ..Config::default()
        };
        saved.save(&path).expect("save");
        assert_eq!(Config::load(&path).expect("load"), Some(saved));

        std::fs::write(&path, b"glass = maybe").expect("write");
        let (config, error) = Config::load_or_default(&path);
        assert_eq!(config, Config::default());
        assert!(matches!(error, Some(ConfigError::Parse(_))));
        // The broken file is left for the user to fix.
        assert_eq!(std::fs::read(&path).expect("read"), b"glass = maybe");

        std::fs::write(&path, [0xff, 0xfe]).expect("write");
        assert!(matches!(Config::load(&path), Err(ConfigError::Read(_))));

        std::fs::write(&path, vec![b'#'; 64 * 1024 + 1]).expect("write");
        assert!(matches!(Config::load(&path), Err(ConfigError::Read(_))));
    }
}
