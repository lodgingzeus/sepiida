use super::*;

const NT: usize = 0x40;
const OPTIONAL: usize = NT + 4 + FILE_HEADER_SIZE;
const SIZE: usize = 0x2000;

/// A minimal PE32+ image built in memory: headers, then tables written
/// from 0x400 on.
struct Image {
    bytes: Vec<u8>,
    next: usize,
}

impl Image {
    fn new() -> Self {
        let mut image = Self {
            bytes: vec![0; SIZE],
            next: 0x400,
        };
        image.put_u16(0, DOS_MAGIC);
        image.put_u32(0x3c, NT as u32);
        image.put_u32(NT, NT_SIGNATURE);
        image.put_u16(OPTIONAL, PE32_PLUS_MAGIC);
        image.put_u32(OPTIONAL + SIZE_OF_IMAGE, SIZE as u32);
        image.put_u32(OPTIONAL + NUMBER_OF_RVA_AND_SIZES, 16);
        image
    }

    fn put_u16(&mut self, at: usize, value: u16) {
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(&mut self, at: usize, value: u32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(&mut self, at: usize, value: u64) {
        self.bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// Reserves `len` bytes, 8-aligned, and returns their RVA.
    fn alloc(&mut self, len: usize) -> usize {
        let at = self.next;
        self.next = (at + len + 7) & !7;
        at
    }

    fn name(&mut self, text: &str) -> u32 {
        let at = self.alloc(text.len() + 1);
        self.bytes[at..at + text.len()].copy_from_slice(text.as_bytes());
        at as u32
    }

    /// A name table and an address table for `functions` (`None` is an
    /// ordinal import); returns (names, addresses).
    fn thunks(&mut self, functions: &[Option<&str>]) -> (u32, u32) {
        let names = self.alloc((functions.len() + 1) * 8);
        let addresses = self.alloc((functions.len() + 1) * 8);
        for (index, function) in functions.iter().enumerate() {
            let thunk = match function {
                Some(function) => {
                    let by_name = self.alloc(function.len() + 3);
                    self.bytes[by_name + 2..by_name + 2 + function.len()]
                        .copy_from_slice(function.as_bytes());
                    by_name as u64
                }
                None => ORDINAL_FLAG | 7,
            };
            self.put_u64(names + index * 8, thunk);
            self.put_u64(addresses + index * 8, 0xdead_0000 + index as u64);
        }
        (names as u32, addresses as u32)
    }

    fn directory(&mut self, index: usize, rva: usize, size: usize) {
        let at = OPTIONAL + DATA_DIRECTORIES + index * 8;
        self.put_u32(at, rva as u32);
        self.put_u32(at + 4, size as u32);
    }

    /// Normal imports: one descriptor per (dll, functions), zero-ended.
    fn imports(&mut self, dlls: &[(&str, &[Option<&str>])]) -> Vec<u32> {
        let table = self.alloc((dlls.len() + 1) * IMPORT_DESCRIPTOR_SIZE);
        let mut address_tables = Vec::new();
        for (index, (dll, functions)) in dlls.iter().enumerate() {
            let name = self.name(dll);
            let (names, addresses) = self.thunks(functions);
            let at = table + index * IMPORT_DESCRIPTOR_SIZE;
            self.put_u32(at, names);
            self.put_u32(at + 12, name);
            self.put_u32(at + 16, addresses);
            address_tables.push(addresses);
        }
        self.directory(
            IMPORT_DIRECTORY,
            table,
            (dlls.len() + 1) * IMPORT_DESCRIPTOR_SIZE,
        );
        address_tables
    }

    /// Delay imports with the given attributes; returns the address table.
    fn delay_imports(&mut self, dll: &str, functions: &[Option<&str>], attributes: u32) -> u32 {
        let table = self.alloc(2 * DELAY_DESCRIPTOR_SIZE);
        let name = self.name(dll);
        let (names, addresses) = self.thunks(functions);
        self.put_u32(table, attributes);
        self.put_u32(table + 4, name);
        self.put_u32(table + 12, addresses);
        self.put_u32(table + 16, names);
        self.directory(DELAY_IMPORT_DIRECTORY, table, 2 * DELAY_DESCRIPTOR_SIZE);
        addresses
    }
}

const WANTED: [Import; 2] = [
    Import::Name("DwmSetWindowAttribute"),
    Import::Name("DwmExtendFrameIntoClientArea"),
];

fn slot(rva: u32, function: usize, delay_loaded: bool, dll: &str) -> ImportSlot {
    ImportSlot {
        rva,
        function,
        delay_loaded,
        dll: dll.into(),
    }
}

#[test]
fn the_size_comes_from_the_headers() {
    let image = Image::new();
    assert_eq!(image_size(&image.bytes[..0x200]), Ok(SIZE as u32));
}

#[test]
fn wanted_functions_of_the_named_dll_are_found_in_order() {
    let mut image = Image::new();
    let tables = image.imports(&[
        ("USER32.dll", &[Some("DwmSetWindowAttribute")]),
        (
            "DWMAPI.DLL",
            &[
                Some("DwmFlush"),
                Some("DwmExtendFrameIntoClientArea"),
                None,
                Some("DwmSetWindowAttribute"),
            ],
        ),
    ]);
    let dwm = tables[1];
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![
            slot(dwm + 8, 1, false, "dwmapi.dll"),
            slot(dwm + 24, 0, false, "dwmapi.dll")
        ])
    );
}

#[test]
fn delay_loaded_imports_are_found_too() {
    let mut image = Image::new();
    let addresses = image.delay_imports(
        "dwmapi.dll",
        &[Some("DwmSetWindowAttribute")],
        DELAY_ATTRIBUTE_RVA,
    );
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![slot(addresses, 0, true, "dwmapi.dll")])
    );
}

#[test]
fn api_set_names_containing_the_dll_match() {
    let mut image = Image::new();
    let tables = image.imports(&[
        (
            "ext-ms-win-dwmapi-ext-l1-1-0.dll",
            &[Some("DwmSetWindowAttribute")],
        ),
        ("uxtheme.dll", &[Some("DwmSetWindowAttribute")]),
    ]);
    assert_eq!(
        import_slots(&image.bytes, &["DWMAPI"], &WANTED),
        Ok(vec![slot(
            tables[0],
            0,
            false,
            "ext-ms-win-dwmapi-ext-l1-1-0.dll"
        )])
    );
}

#[test]
fn old_style_delay_descriptors_are_skipped() {
    let mut image = Image::new();
    image.delay_imports("dwmapi.dll", &[Some("DwmSetWindowAttribute")], 0);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
}

#[test]
fn an_image_without_imports_has_no_slots() {
    let image = Image::new();
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
}

#[test]
fn function_names_must_match_exactly() {
    let mut image = Image::new();
    image.imports(&[(
        "dwmapi.dll",
        &[
            Some("dwmsetwindowattribute"),
            Some("DwmSetWindowAttributeX"),
        ],
    )]);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
}

#[test]
fn foreign_and_32_bit_images_are_refused() {
    assert_eq!(
        import_slots(&[], &["dwmapi.dll"], &WANTED),
        Err(PeError::NotPe)
    );
    assert_eq!(
        import_slots(b"MZ", &["dwmapi.dll"], &WANTED),
        Err(PeError::Truncated)
    );
    let mut image = Image::new();
    image.put_u32(NT, 0x1234);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Err(PeError::NotPe)
    );
    let mut image = Image::new();
    image.put_u16(OPTIONAL, 0x10b);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Err(PeError::Not64Bit)
    );
}

#[test]
fn tables_pointing_outside_the_image_yield_nothing() {
    let mut image = Image::new();
    image.imports(&[("dwmapi.dll", &[Some("DwmSetWindowAttribute")])]);
    // The descriptor's name table now points past the end.
    let descriptor = read_u32(&image.bytes, OPTIONAL + DATA_DIRECTORIES + 8).unwrap() as usize;
    image.put_u32(descriptor, (SIZE + 0x100) as u32);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
    // A directory past the end, and a truncated image.
    let mut image = Image::new();
    image.directory(IMPORT_DIRECTORY, SIZE - 4, 40);
    assert_eq!(
        import_slots(&image.bytes, &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
    let image = Image::new();
    assert_eq!(
        import_slots(&image.bytes[..OPTIONAL + 4], &["dwmapi.dll"], &WANTED),
        Ok(vec![])
    );
}

#[test]
fn an_unterminated_name_never_matches() {
    let mut image = Image::new();
    image.imports(&[("dwmapi.dll", &[Some("DwmSetWindowAttribute")])]);
    let mut bytes = image.bytes.clone();
    // Fill the end of the image with a name that never ends.
    let tail = bytes.len() - 8;
    bytes[tail..].fill(b'a');
    assert!(import_slots(&bytes, &["dwmapi.dll"], &WANTED).is_ok());
}

#[test]
fn ordinal_imports_match_only_an_ordinal_wanted() {
    let mut image = Image::new();
    // The test image writes ordinal 7 for `None`.
    let tables = image.imports(&[("UxTheme.dll", &[Some("DrawThemeTextEx"), None])]);
    let wanted = [Import::Name("DrawThemeTextEx"), Import::Ordinal(7)];
    assert_eq!(
        import_slots(&image.bytes, &["uxtheme"], &wanted),
        Ok(vec![
            slot(tables[0], 0, false, "uxtheme.dll"),
            slot(tables[0] + 8, 1, false, "uxtheme.dll")
        ])
    );
    assert_eq!(
        import_slots(&image.bytes, &["uxtheme"], &[Import::Ordinal(8)]),
        Ok(vec![])
    );
    // A name never matches an ordinal import.
    assert_eq!(
        import_slots(&image.bytes, &["uxtheme"], &[Import::Name("#7")]),
        Ok(vec![])
    );
}

#[test]
fn any_of_several_dll_patterns_matches_and_empty_ones_never() {
    let mut image = Image::new();
    let tables = image.imports(&[
        ("GDI32.dll", &[Some("FillRect")]),
        ("ext-ms-win-gdi-draw-l1-1-0.dll", &[Some("FillRect")]),
        ("USER32.dll", &[Some("FillRect")]),
    ]);
    let wanted = [Import::Name("FillRect")];
    assert_eq!(
        import_slots(&image.bytes, &["gdi32", "-gdi-"], &wanted),
        Ok(vec![
            slot(tables[0], 0, false, "gdi32.dll"),
            slot(tables[1], 0, false, "ext-ms-win-gdi-draw-l1-1-0.dll")
        ])
    );
    assert_eq!(import_slots(&image.bytes, &[""], &wanted), Ok(vec![]));
}
