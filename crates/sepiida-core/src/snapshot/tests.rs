use super::*;

fn accent_writes() -> Vec<RegistryWrite> {
    AccentValues::from_color(Color::new(200, 111, 201)).registry_writes()
}

fn named<'a>(writes: &'a [RegistryWrite], name: &str) -> &'a RegistryWrite {
    writes
        .iter()
        .find(|w| w.name == name)
        .unwrap_or_else(|| panic!("no write named {name}"))
}

#[test]
fn every_value_sepiida_writes_is_a_target() {
    let mut all = accent_writes();
    all.extend(ThemeMode::Dark.registry_writes());
    for write in &all {
        assert!(
            find_target(write.key, write.name).is_some(),
            "{} is missing",
            write.name
        );
    }
    assert_eq!(targets().len(), all.len());
}

#[test]
fn only_the_first_original_is_kept() {
    let writes = accent_writes();
    let menu = named(&writes, "AccentColorMenu");
    let mut snapshot = Snapshot::default();
    assert_eq!(snapshot.unrecorded(&writes).len(), writes.len());

    snapshot.record(menu, Original::Value(RegistryData::Dword(1)));
    snapshot.record(menu, Original::Value(RegistryData::Dword(2)));
    assert_eq!(snapshot.unrecorded(&writes).len(), writes.len() - 1);
    assert_eq!(
        snapshot.restore_actions(),
        vec![RestoreAction::Set(RegistryWrite {
            data: RegistryData::Dword(1),
            ..menu.clone()
        })]
    );
}

#[test]
fn values_sepiida_does_not_write_are_never_recorded() {
    let mut snapshot = Snapshot::default();
    let foreign = RegistryWrite {
        key: r"Software\Microsoft\Windows\CurrentVersion\Run",
        name: "Evil",
        data: RegistryData::Dword(1),
    };
    snapshot.record(&foreign, Original::Absent);
    assert!(snapshot.is_empty());
}

#[test]
fn values_that_cannot_be_put_back_faithfully_are_left_alone() {
    let writes = accent_writes();
    let mut snapshot = Snapshot::default();
    // A binary value where a DWORD belongs, and an oversized palette.
    snapshot.record(
        named(&writes, "AccentColorMenu"),
        Original::Value(RegistryData::Binary(vec![1, 2])),
    );
    snapshot.record(
        named(&writes, "AccentPalette"),
        Original::Value(RegistryData::Binary(vec![0; MAX_BINARY_LEN + 1])),
    );
    snapshot.record(named(&writes, "AccentColor"), Original::Unknown);
    assert_eq!(snapshot.unrecorded(&writes).len(), writes.len() - 3);
    assert!(snapshot.restore_actions().is_empty());
}

#[test]
fn absent_values_are_deleted_on_restore() {
    let writes = accent_writes();
    let auto = named(&writes, "AutoColorization");
    let mut snapshot = Snapshot::default();
    snapshot.record(auto, Original::Absent);
    assert_eq!(
        snapshot.restore_actions(),
        vec![RestoreAction::Delete {
            key: auto.key,
            name: auto.name,
        }]
    );
}

#[test]
fn round_trips_through_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("state").join("saved-theme.json");
    let writes = accent_writes();
    let mut snapshot = Snapshot::default();
    snapshot.record(
        named(&writes, "AccentPalette"),
        Original::Value(RegistryData::Binary(vec![0x00, 0xab, 0xff])),
    );
    snapshot.record(
        named(&writes, "AccentColorMenu"),
        Original::Value(RegistryData::Dword(0xff00_11ee)),
    );
    snapshot.record(named(&writes, "AutoColorization"), Original::Absent);
    snapshot.record(named(&writes, "AccentColor"), Original::Unknown);

    snapshot.save(&path).expect("save");
    assert_eq!(Snapshot::load(&path).expect("load"), snapshot);

    Snapshot::discard(&path).expect("discard");
    assert!(!path.exists());
    Snapshot::discard(&path).expect("discarding twice is fine");
    assert!(Snapshot::load(&path).expect("missing").is_empty());
}

fn load_text(text: &str) -> Result<Snapshot, SnapshotError> {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("saved-theme.json");
    std::fs::write(&path, text).expect("write");
    Snapshot::load(&path)
}

#[test]
fn the_file_cannot_name_other_registry_values() {
    let snapshot = load_text(
        r#"{"version":1,"values":[
            {"key":"Software\\Microsoft\\Windows\\CurrentVersion\\Run","name":"AccentColorMenu","original":{"dword":1}},
            {"key":"Software\\Microsoft\\Windows\\DWM","name":"Shell","original":{"dword":1}},
            {"key":"software\\microsoft\\windows\\dwm","name":"accentcolor","original":{"dword":7}},
            {"key":"Software\\Microsoft\\Windows\\DWM","name":"AccentColor","original":{"dword":8}}
        ]}"#,
    )
    .expect("load");
    // Only the matching value survives, under sepiida's own spelling, and
    // the duplicate after it is ignored.
    assert_eq!(
        snapshot.restore_actions(),
        vec![RestoreAction::Set(RegistryWrite {
            key: r"Software\Microsoft\Windows\DWM",
            name: "AccentColor",
            data: RegistryData::Dword(7),
        })]
    );
}

#[test]
fn bad_entries_in_the_file_become_unknown() {
    let snapshot = load_text(
        r#"{"version":1,"values":[
            {"key":"Software\\Microsoft\\Windows\\DWM","name":"AccentColor","original":{"binary":"0102"}},
            {"key":"Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent","name":"AccentPalette","original":{"binary":"0g"}},
            {"key":"Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent","name":"AccentColorMenu","original":{"binary":"abc"}}
        ]}"#,
    )
    .expect("load");
    assert!(!snapshot.is_empty());
    assert!(snapshot.restore_actions().is_empty());
}

#[test]
fn damaged_files_are_errors_and_other_versions_are_ignored() {
    assert!(matches!(load_text("{oops"), Err(SnapshotError::Parse(_))));
    assert!(matches!(
        load_text(r#"{"version":1,"values":[{"key":"k","name":"n","original":{"qword":1}}]}"#),
        Err(SnapshotError::Parse(_))
    ));
    assert!(
        load_text(r#"{"version":2,"values":[]}"#)
            .expect("load")
            .is_empty()
    );
}

#[test]
fn decode_hex_accepts_only_whole_bytes() {
    assert_eq!(decode_hex(""), Some(vec![]));
    assert_eq!(decode_hex("00aBff"), Some(vec![0x00, 0xab, 0xff]));
    for bad in ["0", "0g", "+1", "\u{e9}0", "zz"] {
        assert_eq!(decode_hex(bad), None, "{bad:?}");
    }
    assert_eq!(decode_hex(&"00".repeat(MAX_BINARY_LEN + 1)), None);
}
