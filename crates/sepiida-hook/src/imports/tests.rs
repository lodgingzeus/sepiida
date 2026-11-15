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

/// A pointer looked up at run time and kept in writable data, as
/// DirectUI keeps uxtheme's functions.
static STORED: AtomicUsize = AtomicUsize::new(0);

fn call_stored() -> HRESULT {
    let address = STORED.load(Ordering::SeqCst);
    // SAFETY: DwmFlush's signature, or our fake with the same one.
    let function: unsafe extern "system" fn() -> HRESULT = unsafe { std::mem::transmute(address) };
    // SAFETY: no arguments.
    unsafe { function() }
}

#[test]
fn a_stored_pointer_goes_through_the_replacement_and_back() {
    use windows::{
        Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW},
        core::{s, w},
    };
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: loads a system DLL and looks up one export.
    let real = unsafe { GetProcAddress(LoadLibraryW(w!("dwmapi.dll")).unwrap(), s!("DwmFlush")) }
        .unwrap() as usize;
    STORED.store(real, Ordering::SeqCst);
    let base = module::this_module().unwrap().0 as usize;
    // SAFETY: our own headers.
    let headers = unsafe { std::slice::from_raw_parts(base as *const u8, 0x400) };
    let own = Module {
        name: "test".into(),
        base,
        size: pe::image_size(headers).unwrap(),
    };
    let set = HookSet {
        name: "test stored",
        dlls: &[],
        implementers: &[],
        only_in: None,
    };
    let functions = [Stored {
        name: "DwmFlush",
        real,
        address: fake_flush as *const () as usize,
    }];
    let targets = [real as u64];
    let patched = patch_stored_in(&own, &set, &functions, &targets);
    assert!(patched >= 1, "the stored pointer was not found");
    assert_eq!(call_stored(), FAKE_ERROR);
    assert_eq!(patch_stored_in(&own, &set, &functions, &targets), 0);
    assert_eq!(unpatch("test stored"), patched);
    assert_eq!(STORED.load(Ordering::SeqCst), real);
}
