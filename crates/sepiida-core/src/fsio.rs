//! Small file helpers for sepiida's own state files.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};

/// Reads at most `max` bytes. A larger file is an error rather than being
/// truncated, because half a settings file is worse than none.
pub fn read_capped(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file is larger than {max} bytes"),
        ));
    }
    Ok(bytes)
}

/// Replaces `path` with `bytes` so that a crash or power cut leaves either
/// the old file or the new one, never a partial write. Creates the parent
/// directory if needed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_owned();
    temp_name.push(format!(".tmp-{}", std::process::id()));
    let temp = parent.join(temp_name);

    let result = write_and_sync(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        // Best effort: a leftover temporary file is harmless.
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_capped_rejects_oversized_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("f");
        fs::write(&path, b"12345").expect("write");
        assert_eq!(read_capped(&path, 5).expect("read"), b"12345");
        let error = read_capped(&path, 4).expect_err("too large");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let missing = read_capped(&dir.path().join("missing"), 4).expect_err("missing");
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn write_atomic_creates_directories_and_replaces_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("a").join("b").join("state.json");
        write_atomic(&path, b"first").expect("first write");
        write_atomic(&path, b"second").expect("second write");
        assert_eq!(fs::read(&path).expect("read"), b"second");
        let leftovers: Vec<_> = fs::read_dir(path.parent().expect("parent"))
            .expect("list")
            .collect();
        assert_eq!(leftovers.len(), 1, "temporary file was left behind");
    }

    #[test]
    fn write_atomic_rejects_a_path_without_a_file_name() {
        let error = write_atomic(Path::new(".."), b"x").expect_err("no file name");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
