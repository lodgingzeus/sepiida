//! Function hooks by import-table patching: the slot through which a
//! loaded module calls a DLL function is pointed at ours, and pointed back
//! when we are done. Pure Rust and architecture-neutral (x64 and ARM64),
//! unlike inline hooks. It only sees calls made through import tables
//! (normal or delay-load), not through `GetProcAddress`.
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
    // SAFETY: the module is pinned, so its first page (the headers, always
    // mapped readable) stays mapped while we read it.
    let headers = unsafe { std::slice::from_raw_parts(base as *const u8, 0x400) };
    let Ok(image_size) = pe::image_size(headers) else {
        return Vec::new();
    };
    let length = image_size.min(size) as usize;
    // SAFETY: the loader maps the whole image (SizeOfImage) readable and
    // the module is pinned while the view is used.
    let image = unsafe { std::slice::from_raw_parts(base as *const u8, length) };
    pe::import_slots(image, dlls, names).unwrap_or_default()
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
mod tests {
    use windows::{Win32::Graphics::Dwm::DwmFlush, core::HRESULT};

    use super::*;

    const FAKE_ERROR: HRESULT = HRESULT(0x8000_5e91_u32 as i32);

    unsafe extern "system" fn fake_flush() -> HRESULT {
        FAKE_ERROR
    }

    fn flush() -> windows::core::Result<()> {
        // SAFETY: no arguments; DwmFlush only waits for composition.
        unsafe { DwmFlush() }
    }

    #[test]
    fn a_set_skips_implementers_our_dll_and_modules_it_is_not_limited_to() {
        let gdi = HookSet {
            name: "test gdi",
            dlls: &["gdi32"],
            implementers: &["gdi32", "user32"],
            only_in: None,
        };
        assert!(gdi.covers("ExplorerFrame.dll"));
        assert!(gdi.covers("comctl32.dll"));
        assert!(!gdi.covers("GDI32.dll"));
        assert!(!gdi.covers("gdi32full.dll"));
        assert!(!gdi.covers("USER32.dll"));
        assert!(!gdi.covers("Sepiida_Hook.DLL"));
        let comctl = HookSet {
            only_in: Some("comctl32.dll"),
            ..gdi
        };
        assert!(comctl.covers("COMCTL32.DLL"));
        assert!(!comctl.covers("explorerframe.dll"));
        assert!(!comctl.covers("comctl32.dll.mui"));
    }

    /// The test executable is this crate's module, so `patch` skips it;
    /// `patch_module` on it patches a real, linker-made import table.
    #[test]
    fn our_own_import_of_dwm_flush_goes_through_the_replacement_and_back() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = module::this_module().unwrap().0 as usize;
        // SAFETY: our own headers.
        let headers = unsafe { std::slice::from_raw_parts(base as *const u8, 0x400) };
        let own = Module {
            name: "test".into(),
            base,
            size: pe::image_size(headers).unwrap(),
        };
        let real = flush();
        let replacement = Replacement {
            function: Import::Name("DwmFlush"),
            address: fake_flush as *const () as usize,
        };
        let set = |dlls| HookSet {
            name: "test flush",
            dlls,
            implementers: &[],
            only_in: None,
        };
        assert_eq!(
            patch_module(
                &own,
                &set(&["dwmapi_test_only.dll"]),
                std::slice::from_ref(&replacement)
            ),
            0
        );
        let patched = patch_module(
            &own,
            &set(&["dwmapi.dll"]),
            std::slice::from_ref(&replacement),
        );
        assert!(patched >= 1, "no DwmFlush import found");
        assert_eq!(flush().unwrap_err().code(), FAKE_ERROR);
        // Patching again finds the slot already ours.
        assert_eq!(
            patch_module(
                &own,
                &set(&["dwmapi.dll"]),
                std::slice::from_ref(&replacement)
            ),
            0
        );
        assert!(is_patched("test flush"));
        assert_eq!(unpatch("test flush"), patched);
        assert!(!is_patched("test flush"));
        assert_eq!(flush().map_err(|e| e.code()), real.map_err(|e| e.code()));
    }
}
