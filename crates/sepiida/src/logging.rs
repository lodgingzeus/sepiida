//! File logging for the background agent, which has no console to print
//! to.

use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
    sync::Mutex,
};

use tracing::Level;

const LOG_FILE: &str = "sepiida.log";
const OLD_LOG_FILE: &str = "sepiida.old.log";
/// The log is moved aside at startup once it passes this size, so at most
/// about twice this is kept on disk.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("could not open the log file in {0}")]
    Open(String, #[source] io::Error),
    #[error("logging was already set up")]
    AlreadySet,
}

/// Sends `tracing` output to `log_dir/sepiida.log`.
pub fn init(log_dir: &Path) -> Result<(), LoggingError> {
    let open_error = |error| LoggingError::Open(log_dir.display().to_string(), error);
    fs::create_dir_all(log_dir).map_err(open_error)?;
    rotate(log_dir, MAX_LOG_BYTES).map_err(open_error)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join(LOG_FILE))
        .map_err(open_error)?;
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .try_init()
        .map_err(|_| LoggingError::AlreadySet)
}

/// Moves a log larger than `max` bytes to the old-log name, replacing the
/// previous old log.
fn rotate(log_dir: &Path, max: u64) -> io::Result<()> {
    let current = log_dir.join(LOG_FILE);
    match current.metadata() {
        Ok(metadata) if metadata.len() > max => fs::rename(current, log_dir.join(OLD_LOG_FILE)),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_to_the_log_file_once() {
        // The only test that installs the global subscriber.
        let dir = tempfile::tempdir().expect("temp dir");
        let log_dir = dir.path().join("logs");
        init(&log_dir).expect("init");
        tracing::info!("hello from the test");
        let text = fs::read_to_string(log_dir.join(LOG_FILE)).expect("read log");
        assert!(text.contains("hello from the test"), "{text}");
        assert!(
            !text.contains('\u{1b}'),
            "log must not contain colour codes"
        );
        assert!(matches!(init(&log_dir), Err(LoggingError::AlreadySet)));
    }

    #[test]
    fn rotate_moves_only_large_logs() {
        let dir = tempfile::tempdir().expect("temp dir");
        rotate(dir.path(), 4).expect("no log yet is fine");

        fs::write(dir.path().join(LOG_FILE), b"1234").expect("write");
        rotate(dir.path(), 4).expect("rotate");
        assert!(dir.path().join(LOG_FILE).exists(), "small log stays");

        fs::write(dir.path().join(OLD_LOG_FILE), b"older").expect("write");
        fs::write(dir.path().join(LOG_FILE), b"12345").expect("write");
        rotate(dir.path(), 4).expect("rotate");
        assert!(!dir.path().join(LOG_FILE).exists());
        assert_eq!(
            fs::read(dir.path().join(OLD_LOG_FILE)).expect("read"),
            b"12345"
        );
    }
}
