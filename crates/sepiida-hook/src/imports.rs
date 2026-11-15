//! Function hooks by import-table patching: the slot through which a
//! loaded module calls a DLL function is pointed at ours, and pointed back
//! when we are done. Pure Rust and architecture-neutral (x64 and ARM64),
//! unlike inline hooks. It only sees calls made through import tables
//! (normal or delay-load), not through `GetProcAddress`; for a module
//! that keeps what `GetProcAddress` gave it in its own writable data,
//! `patch_stored` swaps those stored pointers the same way.
//!
//! Every module is pinned (its reference count raised) while one of its
//! slots is ours, so a slot is never written after its module unloaded.
//! Our own module and the DLLs that implement the functions are never
//! patched: our calls, and their calls among themselves, must reach the
//! real functions.
//!
//! Because a slot belongs to one module, a set can be limited to the
//! modules of one name: the hook then knows who called it, with no stack
//! walk.

use std::{
    ffi::c_void,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
};

use sepiida_core::pe::{self, Import};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FreeLibrary, HANDLE, HMODULE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW,
                TH32CS_SNAPMODULE,
            },
            LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleHandleExW},
            Memory::{
                MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
                PAGE_EXECUTE_WRITECOPY, PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect,
                VirtualQuery,
            },
        },
    },
    core::PCWSTR,
};

use crate::{log::log, module};

/// A function to replace: its name or ordinal in the DLL and our
/// function's address.
pub struct Replacement {
    pub function: Import<'static>,
    pub address: usize,
}

/// Hooks patched and put back together.
pub struct HookSet {
    /// Keys the patches for `unpatch` and `is_patched`.
    pub name: &'static str,
    /// Lower-case parts of the imported DLL's name; any one matches, so
    /// API sets (`ext-ms-win-gdi-*`) can be listed beside the DLL.
    pub dlls: &'static [&'static str],
    /// Lower-case parts of the names of modules never patched: those
    /// that implement the functions.
    pub implementers: &'static [&'static str],
    /// Only the modules with this lower-case file name, or every module.
    pub only_in: Option<&'static str>,
}

impl HookSet {
    /// Whether the module with file name `name` is patched for this set:
    /// never an implementer, nor any copy of this DLL (older copies must
    /// reach the real functions too).
    fn covers(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower != OUR_FILE_NAME
            && !self.implementers.iter().any(|part| lower.contains(part))
            && self.only_in.is_none_or(|only| lower == only)
    }
}

fn shown(function: Import) -> String {
    match function {
        Import::Name(name) => name.to_owned(),
        Import::Ordinal(ordinal) => format!("#{ordinal}"),
    }
}

/// This DLL's file name, lower case.
const OUR_FILE_NAME: &str = "sepiida_hook.dll";

struct Patch {
    set: &'static str,
    /// Our reference on the patched module, released on unpatch.
    module: HMODULE,
    slot: usize,
    original: usize,
    replacement: usize,
}

// SAFETY: the module handle is only passed to FreeLibrary, from any thread.
unsafe impl Send for Patch {}

static PATCHES: Mutex<Vec<Patch>> = Mutex::new(Vec::new());

fn patches() -> MutexGuard<'static, Vec<Patch>> {
    PATCHES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Points every loaded module's import of `replacements` (as `set`
/// selects them) at ours. Slots already ours are left alone, so calling
/// it again only covers modules loaded since. Returns the number of slots
/// patched now.
pub fn patch(set: &HookSet, replacements: &[Replacement]) -> usize {
    let own = module::this_module();
    let mut count = 0;
    for entry in loaded_modules() {
        let name = String::from_utf16_lossy(until_nul(&entry.szModule));
        if Some(entry.hModule) == own || !set.covers(&name) {
            continue;
        }
        let module = Module {
            name,
            base: entry.modBaseAddr as usize,
            size: entry.modBaseSize,
        };
        count += patch_module(&module, set, replacements);
    }
    count
}

/// A loaded module, as the snapshot lists it.
struct Module {
    name: String,
    base: usize,
    size: u32,
}

/// Patches `module`'s imports of `replacements` from `set.dlls`.
fn patch_module(module: &Module, set: &HookSet, replacements: &[Replacement]) -> usize {
    let Some(pinned) = pin(module.base as *const c_void) else {
        return 0; // unloading
    };
    let names: Vec<Import> = replacements.iter().map(|r| r.function).collect();
    let mut patches = patches();
    let mut count = 0;
    for slot in import_slots(module.base, module.size, set.dlls, &names) {
        let Some(replacement) = replacements.get(slot.function) else {
            continue;
        };
        let address = module.base + slot.rva as usize;
        let original = read_slot(address);
        if original == replacement.address {
            continue;
        }
        let Some(reference) = pin(module.base as *const c_void) else {
            continue;
        };
        if !write_slot(address, replacement.address) {
            release(reference);
            continue;
        }
        let delay = if slot.delay_loaded {
            " (delay-load)"
        } else {
            ""
        };
        log!(
            "imports: {} calls {}!{} through us now{delay}",
            module.name,
            slot.dll,
            shown(replacement.function)
        );
        patches.push(Patch {
            set: set.name,
            module: reference,
            slot: address,
            original,
            replacement: replacement.address,
        });
        count += 1;
    }
    release(pinned);
    count
}

/// A function a module may have stored a pointer to: the real address
/// (as `GetProcAddress` gives it) and ours.
pub struct Stored {
    pub name: &'static str,
    pub real: usize,
    pub address: usize,
}

/// Points every pointer to one of `functions` that a module covered by
/// `set` stored in its writable data at ours, recorded like an import
/// slot (put back by `unpatch`). Pointers already ours are no longer
/// equal to the real address, so calling it again finds only new ones.
/// Returns the number patched now.
pub fn patch_stored(set: &HookSet, functions: &[Stored]) -> usize {
    let own = module::this_module();
    let targets: Vec<u64> = functions.iter().map(|f| f.real as u64).collect();
    let mut count = 0;
    for entry in loaded_modules() {
        let name = String::from_utf16_lossy(until_nul(&entry.szModule));
        if Some(entry.hModule) == own || !set.covers(&name) {
            continue;
        }
        let module = Module {
            name,
            base: entry.modBaseAddr as usize,
            size: entry.modBaseSize,
        };
        count += patch_stored_in(&module, set, functions, &targets);
    }
    count
}

fn patch_stored_in(module: &Module, set: &HookSet, functions: &[Stored], targets: &[u64]) -> usize {
    let Some(pinned) = pin(module.base as *const c_void) else {
        return 0; // unloading
    };
    let found = with_image(module.base, module.size, |image| {
        pe::stored_slots(image, targets).unwrap_or_default()
    });
    let mut patches = patches();
    let mut count = 0;
    for slot in found {
        let Some(function) = functions.get(slot.target) else {
            continue;
        };
        let address = module.base + slot.rva as usize;
        // Read again: the module may have changed it since the scan.
        let original = read_slot(address);
        if original != function.real {
            continue;
        }
        let Some(reference) = pin(module.base as *const c_void) else {
            continue;
        };
        if !write_slot(address, function.address) {
            release(reference);
            continue;
        }
        log!(
            "imports: {} calls {} through us now (stored pointer at +{:#x})",
            module.name,
            function.name,
            slot.rva
        );
        patches.push(Patch {
            set: set.name,
            module: reference,
            slot: address,
            original,
            replacement: function.address,
        });
        count += 1;
    }
    let none_yet = count == 0 && !patches.iter().any(|p| p.set == set.name);
    drop(patches);
    if none_yet {
        log_no_stored(&module.name, set.name);
    }
    release(pinned);
    count
}

/// Logs once per set that a module keeps no pointer we look for.
fn log_no_stored(module: &str, set: &'static str) {
    static NOTED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut noted = NOTED.lock().unwrap_or_else(|e| e.into_inner());
    if !noted.contains(&set) {
        noted.push(set);
        log!("imports: {module} keeps no stored pointer for {set}");
    }
}

/// Points every slot patched for the set named `set` back where it
/// pointed before, if it is still ours, and releases the modules. Returns
/// how many there were.
pub fn unpatch(set: &str) -> usize {
    let mut patches = patches();
    let (ours, others): (Vec<Patch>, Vec<Patch>) = patches.drain(..).partition(|p| p.set == set);
    *patches = others;
    for patch in &ours {
        if read_slot(patch.slot) == patch.replacement && !write_slot(patch.slot, patch.original) {
            // Left pointing at us; our functions pass every call through.
            log!("imports: could not put a {set} slot back");
        }
        release(patch.module);
    }
    ours.len()
}

/// Whether any slot of the set named `set` is ours.
pub fn is_patched(set: &str) -> bool {
    patches().iter().any(|p| p.set == set)
}

fn import_slots(base: usize, size: u32, dlls: &[&str], names: &[Import]) -> Vec<pe::ImportSlot> {
    with_image(base, size, |image| {
        pe::import_slots(image, dlls, names).unwrap_or_default()
    })
}

/// Runs `read` on a byte view of the pinned module at `base`; empty if
/// its headers cannot be read.
fn with_image<T: Default>(base: usize, size: u32, read: impl FnOnce(&[u8]) -> T) -> T {
    // SAFETY: the module is pinned, so its first page (the headers, always
    // mapped readable) stays mapped while we read it.
    let headers = unsafe { std::slice::from_raw_parts(base as *const u8, 0x400) };
    let Ok(image_size) = pe::image_size(headers) else {
        return T::default();
    };
    let length = image_size.min(size) as usize;
    // SAFETY: the loader maps the whole image (SizeOfImage) readable and
    // the module is pinned while the view is used.
    let image = unsafe { std::slice::from_raw_parts(base as *const u8, length) };
    read(image)
}

/// The modules loaded in this process now.
fn loaded_modules() -> Vec<MODULEENTRY32W> {
    // SAFETY: a snapshot of our own process.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, 0) }) else {
        log!("imports: no module snapshot");
        return Vec::new();
    };
    let snapshot = Snapshot(snapshot);
    let mut modules = Vec::new();
    let mut entry = MODULEENTRY32W {
        dwSize: size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry` has its size set; the snapshot is open.
    let mut more = unsafe { Module32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        modules.push(entry);
        // SAFETY: as above.
        more = unsafe { Module32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    modules
}

/// Closes the snapshot handle when dropped.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateToolhelp32Snapshot.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn until_nul(text: &[u16]) -> &[u16] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

/// Raises the reference count of the module containing `address`.
fn pin(address: *const c_void) -> Option<HMODULE> {
    let mut module = HMODULE::default();
    // SAFETY: looks up a module by an address inside it.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            PCWSTR(address.cast()),
            &mut module,
        )
    }
    .ok()?;
    Some(module)
}

fn release(module: HMODULE) {
    // SAFETY: a reference taken by `pin`.
    let _ = unsafe { FreeLibrary(module) };
}

fn read_slot(slot: usize) -> usize {
    // SAFETY: an aligned, pointer-sized import slot in a pinned module;
    // the loader and other patchers may write it, hence atomic.
    unsafe { AtomicUsize::from_ptr(slot as *mut usize) }.load(Ordering::SeqCst)
}

/// Read-write, keeping execute if the page has it: an import table may
/// share a page with code that other threads are running.
fn writable_protection(slot: usize) -> PAGE_PROTECTION_FLAGS {
    let mut info = MEMORY_BASIC_INFORMATION::default();
    // SAFETY: queries one address of a pinned module into `info`.
    let filled = unsafe {
        VirtualQuery(
            Some(slot as *const c_void),
            &mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    let executable =
        PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    if filled != 0 && info.Protect.0 & executable.0 != 0 {
        PAGE_EXECUTE_READWRITE
    } else {
        PAGE_READWRITE
    }
}

fn write_slot(slot: usize, value: usize) -> bool {
    let size = size_of::<usize>();
    let mut old = PAGE_PROTECTION_FLAGS::default();
    let writable = writable_protection(slot);
    // SAFETY: changes the protection of one slot of a pinned module.
    if let Err(error) = unsafe { VirtualProtect(slot as *const c_void, size, writable, &mut old) } {
        log!("imports: VirtualProtect failed: {error}");
        return false;
    }
    // SAFETY: as in `read_slot`; the page is writable now.
    unsafe { AtomicUsize::from_ptr(slot as *mut usize) }.store(value, Ordering::SeqCst);
    let mut ignored = PAGE_PROTECTION_FLAGS::default();
    // SAFETY: puts the protection back.
    let _ = unsafe { VirtualProtect(slot as *const c_void, size, old, &mut ignored) };
    true
}

/// Held by every test that patches imports: patches are process-wide.
#[cfg(all(test, windows))]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(test, windows))]
mod tests;
