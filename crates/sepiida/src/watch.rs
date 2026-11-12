//! File change notifications for the background agent.
//!
//! Each watched file's directory is watched rather than the file itself,
//! because editors and sepiida's own atomic writes replace files instead of
//! changing them in place, and a file may not exist yet.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("{0} has no parent folder to watch")]
    NoParent(String),
    #[error("could not create {0}")]
    CreateDir(String, #[source] std::io::Error),
    #[error("could not watch {0}")]
    Watch(String, #[source] notify::Error),
}

/// Stops watching when dropped.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Calls `on_change` from a background thread whenever one of `files`
    /// is created, written, removed or renamed. Missing parent folders are
    /// created.
    pub fn new(
        files: Vec<PathBuf>,
        on_change: impl Fn() + Send + 'static,
    ) -> Result<Self, WatchError> {
        let watched = files.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                // A watcher error means events may have been lost; treating it
                // as a change costs at most one unnecessary apply.
                let relevant = result.map_or(true, |event| {
                    !matches!(event.kind, EventKind::Access(_))
                        && event.paths.iter().any(|path| is_watched(&watched, path))
                });
                if relevant {
                    on_change();
                }
            })
            .map_err(|e| WatchError::Watch(display(&files), e))?;

        let mut dirs: Vec<&Path> = Vec::new();
        for file in &files {
            let dir = file
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(|| WatchError::NoParent(file.display().to_string()))?;
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        for dir in dirs {
            fs::create_dir_all(dir)
                .map_err(|e| WatchError::CreateDir(dir.display().to_string(), e))?;
            watcher
                .watch(dir, RecursiveMode::NonRecursive)
                .map_err(|e| WatchError::Watch(dir.display().to_string(), e))?;
        }
        Ok(Self { _watcher: watcher })
    }
}

fn display(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// File names compare without regard to ASCII case, as Windows does.
fn is_watched(watched: &[PathBuf], path: &Path) -> bool {
    let same_name = |a: Option<&OsStr>, b: Option<&OsStr>| match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    watched
        .iter()
        .any(|file| same_name(file.file_name(), path.file_name()) && file.parent() == path.parent())
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;

    #[test]
    fn matches_watched_files_by_folder_and_name() {
        let watched = vec![PathBuf::from("/a/config.toml")];
        assert!(is_watched(&watched, Path::new("/a/config.toml")));
        assert!(is_watched(&watched, Path::new("/a/CONFIG.TOML")));
        assert!(!is_watched(&watched, Path::new("/b/config.toml")));
        assert!(!is_watched(&watched, Path::new("/a/config.toml.tmp-1")));
        assert!(!is_watched(&watched, Path::new("/a")));
    }

    #[test]
    fn reports_changes_to_watched_files_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("created-by-watcher");
        let file = folder.join("config.toml");
        let (tx, rx) = mpsc::channel();
        let _watcher = FileWatcher::new(vec![file.clone()], move || {
            let _ = tx.send(());
        })
        .expect("watch");
        assert!(folder.is_dir());

        fs::write(folder.join("other.txt"), b"x").expect("write");
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "unrelated file reported"
        );

        fs::write(&file, b"glass = true").expect("write");
        rx.recv_timeout(Duration::from_secs(5))
            .expect("change to the watched file");
    }

    #[test]
    fn rejects_a_bare_file_name() {
        assert!(matches!(
            FileWatcher::new(vec![PathBuf::from("config.toml")], || {}),
            Err(WatchError::NoParent(_))
        ));
    }
}
