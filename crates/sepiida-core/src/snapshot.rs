//! The registry values sepiida replaced, so they can be put back.
//!
//! The first time sepiida changes a value, the value it had before is
//! recorded. Later changes leave the record alone, so it always holds what
//! the user had before sepiida, not sepiida's previous colour.
//!
//! The saved file only names values; it can never name a registry key of
//! its own. Every entry is matched against the fixed list of values sepiida
//! writes and type-checked against it, and anything else is ignored.

use std::{io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    Color,
    accent::{AccentValues, RegistryData, RegistryWrite, ThemeMode},
    fsio,
};

const SNAPSHOT_VERSION: u32 = 1;
/// A handful of values; a few kilobytes at most.
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024;
/// Larger than any value sepiida writes. A longer saved value is not one of
/// ours to restore.
const MAX_BINARY_LEN: usize = 256;

/// What a value was before sepiida first changed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Original {
    /// The value did not exist; restoring deletes it.
    Absent,
    Value(RegistryData),
    /// The value existed but could not be read back faithfully (another
    /// type, or too large). Restoring leaves it as sepiida set it rather
    /// than guess.
    Unknown,
}

/// One step of putting the original values back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreAction {
    Set(RegistryWrite),
    Delete {
        key: &'static str,
        name: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dword,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Target {
    key: &'static str,
    name: &'static str,
    kind: Kind,
}

/// Every value sepiida may write. Built from the write lists themselves so
/// the two cannot drift apart.
fn targets() -> Vec<Target> {
    let accent = AccentValues::from_color(Color::new(0, 0, 0)).registry_writes();
    let mode = ThemeMode::Light.registry_writes();
    accent
        .iter()
        .chain(&mode)
        .map(|write| Target {
            key: write.key,
            name: write.name,
            kind: kind_of(&write.data),
        })
        .collect()
}

fn kind_of(data: &RegistryData) -> Kind {
    match data {
        RegistryData::Dword(_) => Kind::Dword,
        RegistryData::Binary(_) => Kind::Binary,
    }
}

fn find_target(key: &str, name: &str) -> Option<Target> {
    targets()
        .into_iter()
        .find(|t| t.key.eq_ignore_ascii_case(key) && t.name.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Saved {
    target: Target,
    original: Original,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    saved: Vec<Saved>,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("could not read the saved original colours")]
    Read(#[source] io::Error),
    #[error("the saved original colours are damaged")]
    Parse(#[source] serde_json::Error),
    #[error("could not write the saved original colours")]
    Write(#[source] io::Error),
}

impl Snapshot {
    /// A missing file is an empty snapshot. A damaged one is an error: the
    /// caller must not start a new snapshot over it, or the user's real
    /// originals would be replaced by sepiida's colours.
    pub fn load(path: &Path) -> Result<Self, SnapshotError> {
        let bytes = match fsio::read_capped(path, MAX_SNAPSHOT_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(SnapshotError::Read(error)),
        };
        let file: SnapshotFile = serde_json::from_slice(&bytes).map_err(SnapshotError::Parse)?;
        Ok(Self::from_file(file))
    }

    pub fn save(&self, path: &Path) -> Result<(), SnapshotError> {
        let bytes = serde_json::to_vec_pretty(&self.to_file()).map_err(SnapshotError::Parse)?;
        fsio::write_atomic(path, &bytes).map_err(SnapshotError::Write)
    }

    /// Deletes the saved file after a restore. A missing file is fine.
    pub fn discard(path: &Path) -> Result<(), SnapshotError> {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(SnapshotError::Write(error))
            }
            _ => Ok(()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.saved.is_empty()
    }

    /// The writes whose original value is not recorded yet.
    pub fn unrecorded<'a>(&self, writes: &'a [RegistryWrite]) -> Vec<&'a RegistryWrite> {
        writes
            .iter()
            .filter(|write| self.position(write.key, write.name).is_none())
            .collect()
    }

    /// Records what `write`'s value was before sepiida changed it. Does
    /// nothing if it is already recorded or is not a value sepiida writes.
    pub fn record(&mut self, write: &RegistryWrite, original: Original) {
        if self.position(write.key, write.name).is_some() {
            return;
        }
        let Some(target) = find_target(write.key, write.name) else {
            return;
        };
        self.saved.push(Saved {
            target,
            original: checked(target, original),
        });
    }

    /// The steps that put every recorded value back.
    pub fn restore_actions(&self) -> Vec<RestoreAction> {
        self.saved
            .iter()
            .filter_map(|saved| {
                let Target { key, name, .. } = saved.target;
                match &saved.original {
                    Original::Absent => Some(RestoreAction::Delete { key, name }),
                    Original::Value(data) => Some(RestoreAction::Set(RegistryWrite {
                        key,
                        name,
                        data: data.clone(),
                    })),
                    Original::Unknown => None,
                }
            })
            .collect()
    }

    fn position(&self, key: &str, name: &str) -> Option<usize> {
        self.saved.iter().position(|saved| {
            saved.target.key.eq_ignore_ascii_case(key)
                && saved.target.name.eq_ignore_ascii_case(name)
        })
    }

    fn from_file(file: SnapshotFile) -> Self {
        let mut snapshot = Self::default();
        if file.version != SNAPSHOT_VERSION {
            return snapshot;
        }
        for entry in file.values {
            let Some(target) = find_target(&entry.key, &entry.name) else {
                continue;
            };
            if snapshot.position(target.key, target.name).is_some() {
                continue;
            }
            snapshot.saved.push(Saved {
                target,
                original: checked(target, entry.original.into()),
            });
        }
        snapshot
    }

    fn to_file(&self) -> SnapshotFile {
        SnapshotFile {
            version: SNAPSHOT_VERSION,
            values: self
                .saved
                .iter()
                .map(|saved| StoredEntry {
                    key: saved.target.key.to_owned(),
                    name: saved.target.name.to_owned(),
                    original: (&saved.original).into(),
                })
                .collect(),
        }
    }
}

/// A recorded value must have the type sepiida writes and fit its limits;
/// otherwise it is not something sepiida can faithfully put back.
fn checked(target: Target, original: Original) -> Original {
    match &original {
        Original::Value(data)
            if kind_of(data) != target.kind
                || matches!(data, RegistryData::Binary(b) if b.len() > MAX_BINARY_LEN) =>
        {
            Original::Unknown
        }
        _ => original,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotFile {
    version: u32,
    values: Vec<StoredEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    key: String,
    name: String,
    original: StoredOriginal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum StoredOriginal {
    Absent,
    Unknown,
    Dword(u32),
    /// Hex, two digits per byte.
    Binary(String),
}

impl From<StoredOriginal> for Original {
    fn from(stored: StoredOriginal) -> Self {
        match stored {
            StoredOriginal::Absent => Self::Absent,
            StoredOriginal::Unknown => Self::Unknown,
            StoredOriginal::Dword(value) => Self::Value(RegistryData::Dword(value)),
            StoredOriginal::Binary(hex) => decode_hex(&hex)
                .map(|bytes| Self::Value(RegistryData::Binary(bytes)))
                .unwrap_or(Self::Unknown),
        }
    }
}

impl From<&Original> for StoredOriginal {
    fn from(original: &Original) -> Self {
        match original {
            Original::Absent => Self::Absent,
            Original::Unknown => Self::Unknown,
            Original::Value(RegistryData::Dword(value)) => Self::Dword(*value),
            Original::Value(data @ RegistryData::Binary(_)) => Self::Binary(data.to_string()),
        }
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || hex.len() > MAX_BINARY_LEN * 2 || !hex.is_ascii() {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            pair.bytes()
                .all(|b| b.is_ascii_hexdigit())
                .then(|| u8::from_str_radix(pair, 16).ok())?
        })
        .collect()
}

#[cfg(test)]
mod tests;
