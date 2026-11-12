//! On-disk palette cache, keyed by wallpaper file, modification time and
//! size.
//!
//! Only the source colour is stored: the rest of the palette is rebuilt
//! from it, which is cheap. The file is sepiida's own, but it is still read
//! defensively; anything wrong with it just means an empty cache.

use std::{io, path::Path, time::UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{Color, Palette, fsio, source::Wallpaper};

/// Enough for a large slideshow folder. Oldest entries go first.
const MAX_ENTRIES: usize = 256;
/// Comfortably above `MAX_ENTRIES` entries with long paths.
const MAX_CACHE_BYTES: u64 = 1024 * 1024;
/// Bumped whenever the colour extraction changes, so old results are
/// dropped instead of reused.
const CACHE_VERSION: u32 = 1;

/// Identifies one version of one wallpaper file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    path: String,
    modified_secs: u64,
    modified_nanos: u32,
    len: u64,
}

impl CacheKey {
    /// `None` when the file cannot be inspected or its path is not valid
    /// Unicode. Such a file is analysed every time instead of cached.
    pub fn for_file(path: &Path) -> Option<Self> {
        let metadata = path.metadata().ok()?;
        let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            path: path.to_str()?.to_owned(),
            modified_secs: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
            len: metadata.len(),
        })
    }

    #[cfg(test)]
    fn new(path: &str, modified: std::time::SystemTime, len: u64) -> Self {
        let modified = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
        Self {
            path: path.to_owned(),
            modified_secs: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
            len,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    #[serde(flatten)]
    key: CacheKey,
    source: Color,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("could not read the palette cache")]
    Read(#[source] io::Error),
    #[error("the palette cache is not valid")]
    Parse(#[source] serde_json::Error),
    #[error("could not write the palette cache")]
    Write(#[source] io::Error),
}

#[derive(Debug, Default)]
pub struct PaletteCache {
    entries: Vec<Entry>,
    dirty: bool,
}

impl PaletteCache {
    /// A missing file is an empty cache. A file from another cache version
    /// is also treated as empty, not as an error.
    pub fn load(path: &Path) -> Result<Self, CacheError> {
        let bytes = match fsio::read_capped(path, MAX_CACHE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(CacheError::Read(error)),
        };
        let file: CacheFile = serde_json::from_slice(&bytes).map_err(CacheError::Parse)?;
        if file.version != CACHE_VERSION {
            return Ok(Self::default());
        }
        let mut entries = file.entries;
        let excess = entries.len().saturating_sub(MAX_ENTRIES);
        entries.drain(..excess);
        Ok(Self {
            entries,
            dirty: false,
        })
    }

    /// Never fails: a broken cache is replaced by an empty one, with the
    /// error returned for the caller to log.
    pub fn load_or_empty(path: &Path) -> (Self, Option<CacheError>) {
        match Self::load(path) {
            Ok(cache) => (cache, None),
            Err(error) => (Self::default(), Some(error)),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Color> {
        self.entries
            .iter()
            .rev()
            .find(|entry| &entry.key == key)
            .map(|entry| entry.source)
    }

    pub fn insert(&mut self, key: CacheKey, source: Color) {
        if self.get(&key) == Some(source) {
            return;
        }
        // A file that changed keeps its path, so the stale entry goes too.
        self.entries.retain(|entry| entry.key.path != key.path);
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(Entry { key, source });
        self.dirty = true;
    }

    /// True when there are changes that `save` would write.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Writes the cache if it changed since it was loaded or last saved.
    pub fn save(&mut self, path: &Path) -> Result<(), CacheError> {
        if !self.dirty {
            return Ok(());
        }
        let file = CacheFile {
            version: CACHE_VERSION,
            entries: self.entries.clone(),
        };
        let bytes = serde_json::to_vec(&file).map_err(CacheError::Parse)?;
        fsio::write_atomic(path, &bytes).map_err(CacheError::Write)?;
        self.dirty = false;
        Ok(())
    }

    /// The palette for a wallpaper, from the cache when the image has been
    /// analysed before.
    pub fn palette(&mut self, wallpaper: &Wallpaper) -> Result<Palette, crate::Error> {
        let Wallpaper::Image(path) = wallpaper else {
            return Palette::from_wallpaper(wallpaper);
        };
        let key = CacheKey::for_file(path);
        if let Some(source) = key.as_ref().and_then(|key| self.get(key)) {
            return Ok(Palette::from_color(source));
        }
        let palette = Palette::from_path(path)?;
        if let Some(key) = key {
            self.insert(key, palette.source);
        }
        Ok(palette)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

    use super::*;

    const GREEN: Color = Color::new(40, 180, 60);

    fn key(path: &str, secs: u64) -> CacheKey {
        CacheKey::new(path, UNIX_EPOCH + Duration::from_secs(secs), 10)
    }

    fn write_image(path: &Path, r: u8, g: u8, b: u8) {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 32, Rgb([r, g, b])))
            .save_with_format(path, ImageFormat::Png)
            .expect("write image");
    }

    #[test]
    fn insert_replaces_stale_entries_for_the_same_file() {
        let mut cache = PaletteCache::default();
        cache.insert(key("a.jpg", 1), GREEN);
        cache.insert(key("a.jpg", 2), Color::new(1, 2, 3));
        assert_eq!(cache.get(&key("a.jpg", 1)), None);
        assert_eq!(cache.get(&key("a.jpg", 2)), Some(Color::new(1, 2, 3)));
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn oldest_entries_are_evicted_first() {
        let mut cache = PaletteCache::default();
        for i in 0..=MAX_ENTRIES {
            cache.insert(key(&format!("{i}.jpg"), 1), GREEN);
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert_eq!(cache.get(&key("0.jpg", 1)), None);
        assert_eq!(
            cache.get(&key(&format!("{MAX_ENTRIES}.jpg"), 1)),
            Some(GREEN)
        );
    }

    #[test]
    fn saves_only_when_changed_and_loads_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cache").join("palettes.json");
        let mut cache = PaletteCache::load(&path).expect("missing file is empty");
        assert!(!cache.is_dirty());
        cache.save(&path).expect("nothing to save");
        assert!(!path.exists());

        cache.insert(key("a.jpg", 1), GREEN);
        assert!(cache.is_dirty());
        cache.save(&path).expect("save");
        assert!(!cache.is_dirty());
        cache.insert(key("a.jpg", 1), GREEN);
        assert!(
            !cache.is_dirty(),
            "re-inserting the same result is no change"
        );

        let loaded = PaletteCache::load(&path).expect("load");
        assert_eq!(loaded.get(&key("a.jpg", 1)), Some(GREEN));
    }

    #[test]
    fn broken_or_foreign_files_give_an_empty_cache() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("palettes.json");

        std::fs::write(&path, b"{not json").expect("write");
        let (cache, error) = PaletteCache::load_or_empty(&path);
        assert!(cache.entries.is_empty());
        assert!(matches!(error, Some(CacheError::Parse(_))));

        std::fs::write(&path, br##"{"version":1,"entries":[{"path":"a","modified_secs":1,"modified_nanos":0,"len":1,"source":"green"}]}"##).expect("write");
        assert!(matches!(
            PaletteCache::load(&path),
            Err(CacheError::Parse(_))
        ));

        std::fs::write(&path, br#"{"version":999,"entries":[]}"#).expect("write");
        assert!(
            PaletteCache::load(&path)
                .expect("other version")
                .entries
                .is_empty()
        );

        std::fs::write(&path, vec![b' '; 1024 * 1024 + 1]).expect("write");
        assert!(matches!(
            PaletteCache::load(&path),
            Err(CacheError::Read(_))
        ));
    }

    #[test]
    fn palette_uses_the_cache_and_notices_changed_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image = dir.path().join("wallpaper.png");
        write_image(&image, 40, 180, 60);
        let wallpaper = Wallpaper::Image(image.clone());

        let mut cache = PaletteCache::default();
        let first = cache.palette(&wallpaper).expect("palette");
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.palette(&wallpaper).expect("cached"), first);

        // Swap the cached colour to prove the second call used the cache.
        let fake = Color::new(1, 2, 3);
        cache.entries[0].source = fake;
        assert_eq!(cache.palette(&wallpaper).expect("cached").source, fake);

        // A different file at the same path gets a new key (its size
        // differs) and is analysed again.
        DynamicImage::ImageRgb8(RgbImage::from_pixel(48, 48, Rgb([200, 30, 30])))
            .save_with_format(&image, ImageFormat::Png)
            .expect("rewrite image");
        let red = cache.palette(&wallpaper).expect("palette");
        assert!(
            red.source.r > red.source.g,
            "expected red, got {}",
            red.source
        );
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn colours_and_missing_files_bypass_the_cache() {
        let mut cache = PaletteCache::default();
        let palette = cache.palette(&Wallpaper::Color(GREEN)).expect("palette");
        assert_eq!(palette.source, GREEN);
        assert!(
            cache
                .palette(&Wallpaper::Image("/no/such/file.png".into()))
                .is_err()
        );
        assert!(!cache.is_dirty());
    }
}
