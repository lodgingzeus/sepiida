//! Where sepiida keeps its files.
//!
//! Settings roam with the user profile (`%APPDATA%\sepiida`); the palette
//! cache, saved state and logs are per machine (`%LOCALAPPDATA%\sepiida`).

use std::path::{Path, PathBuf};

use directories::BaseDirs;

const APP_DIR: &str = "sepiida";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config: PathBuf,
    pub palette_cache: PathBuf,
    /// The accent values sepiida replaced, for `sepiida restore`.
    pub saved_theme: PathBuf,
    /// Facts the agent remembers between runs, such as where Wallpaper
    /// Engine is installed.
    pub agent_state: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
#[error("could not find the user profile folders")]
pub struct NoProfileDirs;

impl Paths {
    pub fn locate() -> Result<Self, NoProfileDirs> {
        let dirs = BaseDirs::new().ok_or(NoProfileDirs)?;
        Ok(Self::under(dirs.config_dir(), dirs.data_local_dir()))
    }

    fn under(roaming: &Path, local: &Path) -> Self {
        let roaming = roaming.join(APP_DIR);
        let local = local.join(APP_DIR);
        Self {
            config: roaming.join("config.toml"),
            palette_cache: local.join("palettes.json"),
            saved_theme: local.join("saved-theme.json"),
            agent_state: local.join("agent-state.json"),
            log_dir: local.join("logs"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roam_and_everything_else_stays_local() {
        let paths = Paths::under(Path::new("roaming"), Path::new("local"));
        assert_eq!(paths.config, Path::new("roaming/sepiida/config.toml"));
        assert_eq!(
            paths.palette_cache,
            Path::new("local/sepiida/palettes.json")
        );
        assert_eq!(
            paths.saved_theme,
            Path::new("local/sepiida/saved-theme.json")
        );
        assert_eq!(paths.log_dir, Path::new("local/sepiida/logs"));
    }

    #[test]
    fn locate_finds_absolute_profile_folders() {
        // Only reads environment and profile lookups; creates nothing.
        let paths = Paths::locate().expect("profile folders");
        assert!(paths.config.is_absolute());
        assert!(paths.config.ends_with("sepiida/config.toml"));
    }
}
