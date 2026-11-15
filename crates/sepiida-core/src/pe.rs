//! Reads the import tables of a loaded 64-bit PE image (PE32+): where the
//! slots are that hold the addresses of functions imported from a DLL, so
//! the hook DLL can point some of them at its own functions. Covers normal
//! and delay-load imports, matched by name or by ordinal. Also finds
//! function addresses a module stored in its own writable data after
//! looking them up with `GetProcAddress` (`stored_slots`).
//!
//! The image is a byte view of the module as the loader mapped it, so
//! every offset is an RVA. Every read is bounds-checked: a malformed or
//! unusual image yields an error or fewer slots, never a panic.

/// A function imported by name, or by ordinal (uxtheme exports some
/// functions only by ordinal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Import<'a> {
    Name(&'a str),
    Ordinal(u16),
}

/// One import slot: the pointer-sized entry at `rva` that the module calls
/// `functions[function]` through, imported from the DLL named `dll`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSlot {
    pub rva: u32,
    pub function: usize,
    pub delay_loaded: bool,
    pub dll: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeError {
    #[error("not a PE image")]
    NotPe,
    #[error("not a 64-bit (PE32+) image")]
    Not64Bit,
    #[error("a header or table lies outside the image")]
    Truncated,
}

const DOS_MAGIC: u16 = 0x5a4d; // "MZ"
const NT_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"
const PE32_PLUS_MAGIC: u16 = 0x20b;
const FILE_HEADER_SIZE: usize = 20;
/// Offsets in the file header, from its start.
const NUMBER_OF_SECTIONS: usize = 2;
const SIZE_OF_OPTIONAL_HEADER: usize = 16;
const SECTION_HEADER_SIZE: usize = 40;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;
/// Offsets in the PE32+ optional header.
const SIZE_OF_IMAGE: usize = 56;
const NUMBER_OF_RVA_AND_SIZES: usize = 108;
const DATA_DIRECTORIES: usize = 112;
const IMPORT_DIRECTORY: usize = 1;
const DELAY_IMPORT_DIRECTORY: usize = 13;
const IMPORT_DESCRIPTOR_SIZE: usize = 20;
const DELAY_DESCRIPTOR_SIZE: usize = 32;
/// `dlattrRva`: the delay descriptor holds RVAs (all modern linkers).
const DELAY_ATTRIBUTE_RVA: u32 = 1;
const ORDINAL_FLAG: u64 = 1 << 63;
/// Longest DLL or function name read.
const MAX_NAME: usize = 256;
/// Most descriptors or thunks walked in one table: far above any real
/// image, low enough that a corrupt table cannot loop for long.
const MAX_ENTRIES: usize = 16_384;

/// The image's `SizeOfImage`, read from its headers alone (the first page
/// is enough), so the caller knows how large a view to make.
pub fn image_size(headers: &[u8]) -> Result<u32, PeError> {
    let optional = optional_header(headers)?;
    read_u32(headers, optional + SIZE_OF_IMAGE).ok_or(PeError::Truncated)
}

/// Every slot through which `image` calls one of `functions` from a DLL
/// whose name contains one of `dlls` ignoring ASCII case, so `dwmapi` also
/// matches API sets such as `ext-ms-win-dwmapi-ext-l1-1-0.dll`. Function
/// names must match exactly.
pub fn import_slots(
    image: &[u8],
    dlls: &[&str],
    functions: &[Import],
) -> Result<Vec<ImportSlot>, PeError> {
    let optional = optional_header(image)?;
    let mut slots = Vec::new();
    if let Some((table, _)) = directory(image, optional, IMPORT_DIRECTORY) {
        for index in 0..MAX_ENTRIES {
            let at = table + index * IMPORT_DESCRIPTOR_SIZE;
            let (Some(names), Some(name), Some(addresses)) = (
                read_u32(image, at),
                read_u32(image, at + 12),
                read_u32(image, at + 16),
            ) else {
                break;
            };
            if name == 0 && addresses == 0 {
                break;
            }
            if let Some(dll) = matching_dll(image, name, dlls) {
                // Without a name table (old bound images) nothing can be
                // matched by name.
                let table = Thunks {
                    names,
                    addresses,
                    delay_loaded: false,
                    dll: &dll,
                };
                walk_thunks(image, &table, functions, &mut slots);
            }
        }
    }
    if let Some((table, _)) = directory(image, optional, DELAY_IMPORT_DIRECTORY) {
        for index in 0..MAX_ENTRIES {
            let at = table + index * DELAY_DESCRIPTOR_SIZE;
            let (Some(attributes), Some(name), Some(addresses), Some(names)) = (
                read_u32(image, at),
                read_u32(image, at + 4),
                read_u32(image, at + 12),
                read_u32(image, at + 16),
            ) else {
                break;
            };
            if name == 0 {
                break;
            }
            if attributes & DELAY_ATTRIBUTE_RVA == 0 {
                continue;
            }
            if let Some(dll) = matching_dll(image, name, dlls) {
                let table = Thunks {
                    names,
                    addresses,
                    delay_loaded: true,
                    dll: &dll,
                };
                walk_thunks(image, &table, functions, &mut slots);
            }
        }
    }
    Ok(slots)
}

/// One stored pointer: the pointer-sized, aligned value at `rva`, in a
/// writable data section, equal to `targets[target]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredSlot {
    pub rva: u32,
    pub target: usize,
}

/// Every aligned 8-byte value in `image`'s writable, non-executable
/// sections that equals one of `targets` (absolute addresses, as the
/// loaded image holds them). Zero targets are never matched.
pub fn stored_slots(image: &[u8], targets: &[u64]) -> Result<Vec<StoredSlot>, PeError> {
    let mut slots = Vec::new();
    for (start, length) in writable_sections(image)? {
        let Some(end) = start.checked_add(length) else {
            continue;
        };
        let end = end.min(image.len());
        // Sections start page-aligned; round up anyway for odd images.
        let mut at = start.next_multiple_of(8);
        while at + 8 <= end {
            if let Some(value) = read_u64(image, at)
                && value != 0
                && let Some(target) = targets.iter().position(|&t| t == value)
                && let Ok(rva) = u32::try_from(at)
            {
                slots.push(StoredSlot { rva, target });
            }
            at += 8;
        }
    }
    Ok(slots)
}

/// RVA and in-memory size of each section that is writable and not
/// executable.
fn writable_sections(image: &[u8]) -> Result<Vec<(usize, usize)>, PeError> {
    let optional = optional_header(image)?;
    let file = optional - FILE_HEADER_SIZE;
    let count = read_u16(image, file + NUMBER_OF_SECTIONS).ok_or(PeError::Truncated)?;
    let optional_size =
        read_u16(image, file + SIZE_OF_OPTIONAL_HEADER).ok_or(PeError::Truncated)?;
    let table = optional + usize::from(optional_size);
    let mut sections = Vec::new();
    for index in 0..usize::from(count) {
        let at = table + index * SECTION_HEADER_SIZE;
        let (Some(size), Some(rva), Some(flags)) = (
            read_u32(image, at + 8),
            read_u32(image, at + 12),
            read_u32(image, at + 36),
        ) else {
            return Err(PeError::Truncated);
        };
        if flags & IMAGE_SCN_MEM_WRITE != 0 && flags & IMAGE_SCN_MEM_EXECUTE == 0 {
            sections.push((rva as usize, size as usize));
        }
    }
    Ok(sections)
}

/// Offset of the PE32+ optional header.
fn optional_header(image: &[u8]) -> Result<usize, PeError> {
    if read_u16(image, 0) != Some(DOS_MAGIC) {
        return Err(PeError::NotPe);
    }
    let nt = read_u32(image, 0x3c).ok_or(PeError::Truncated)? as usize;
    match read_u32(image, nt) {
        Some(NT_SIGNATURE) => {}
        Some(_) => return Err(PeError::NotPe),
        None => return Err(PeError::Truncated),
    }
    let optional = nt + 4 + FILE_HEADER_SIZE;
    match read_u16(image, optional) {
        Some(PE32_PLUS_MAGIC) => Ok(optional),
        Some(_) => Err(PeError::Not64Bit),
        None => Err(PeError::Truncated),
    }
}

/// RVA and size of data directory `index`, if present and non-empty.
fn directory(image: &[u8], optional: usize, index: usize) -> Option<(usize, usize)> {
    let count = read_u32(image, optional + NUMBER_OF_RVA_AND_SIZES)? as usize;
    if index >= count {
        return None;
    }
    let at = optional + DATA_DIRECTORIES + index * 8;
    let rva = read_u32(image, at)? as usize;
    let size = read_u32(image, at + 4)? as usize;
    (rva != 0 && size != 0).then_some((rva, size))
}

/// The DLL name at `name` if it contains one of `parts` (ASCII case
/// ignored).
fn matching_dll(image: &[u8], name: u32, parts: &[&str]) -> Option<String> {
    let name = read_name(image, name as usize)?.to_ascii_lowercase();
    parts
        .iter()
        .any(|part| {
            let part = part.to_ascii_lowercase();
            !part.is_empty() && name.windows(part.len()).any(|w| w == part.as_bytes())
        })
        .then(|| String::from_utf8_lossy(&name).into_owned())
}

/// One DLL's name table and address table.
struct Thunks<'a> {
    names: u32,
    addresses: u32,
    delay_loaded: bool,
    dll: &'a str,
}

/// Walks a name table (`names`) beside its address table (`addresses`),
/// adding the address slots of the wanted functions.
fn walk_thunks(image: &[u8], table: &Thunks, functions: &[Import], slots: &mut Vec<ImportSlot>) {
    let Thunks {
        names,
        addresses,
        delay_loaded,
        dll,
    } = *table;
    if names == 0 || addresses == 0 {
        return;
    }
    for index in 0..MAX_ENTRIES {
        let Some(thunk) = read_u64(image, names as usize + index * 8) else {
            return;
        };
        if thunk == 0 {
            return;
        }
        let Some(function) = wanted(image, thunk, functions) else {
            continue;
        };
        let Ok(rva) = u32::try_from(addresses as usize + index * 8) else {
            return;
        };
        if read_u64(image, rva as usize).is_some() {
            slots.push(ImportSlot {
                rva,
                function,
                delay_loaded,
                dll: dll.to_owned(),
            });
        }
    }
}

/// The index in `functions` of the function a name-table entry imports.
fn wanted(image: &[u8], thunk: u64, functions: &[Import]) -> Option<usize> {
    if thunk & ORDINAL_FLAG != 0 {
        let ordinal = (thunk & 0xffff) as u16;
        return functions
            .iter()
            .position(|f| *f == Import::Ordinal(ordinal));
    }
    // IMAGE_IMPORT_BY_NAME: a 16-bit hint, then the name.
    let by_name = usize::try_from(thunk).ok()?;
    let name = read_name(image, by_name.checked_add(2)?)?;
    functions
        .iter()
        .position(|f| matches!(f, Import::Name(n) if n.as_bytes() == name))
}

/// A NUL-terminated name at `at`, at most `MAX_NAME` bytes.
fn read_name(image: &[u8], at: usize) -> Option<&[u8]> {
    let rest = image.get(at..)?;
    let rest = &rest[..rest.len().min(MAX_NAME + 1)];
    let end = rest.iter().position(|&b| b == 0)?;
    Some(&rest[..end])
}

fn read_u16(image: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        image.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(image: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        image.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(image: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        image.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests;
