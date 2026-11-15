//! explorer.exe holds two attachments, the taskbar's (system XAML) and File
//! Explorer's (WinUI 3): attaching or detaching one leaves the other alone.
//! File Explorer opens a XAML thread per window, so threads that appear
//! later start styled like the others.

use sepiida_core::{
    Color,
    shell::{protocol::Command, rules::Role},
};
use windows::Win32::System::Ole::IObjectWithSite;

use crate::xamlom::IVisualTreeServiceCallback2;

use super::{add_taskbar, fake::*, new_tap, send, serial, wait_for_callback, wait_until};

#[test]
fn the_taskbar_and_file_explorer_attachments_never_end_each_other() {
    let _serial = serial();
    let (taskbar, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&taskbar);
    add_taskbar(&callback);

    // A style attach to File Explorer does not replace the taskbar's.
    let (explorer, explorer_site) = new_fake();
    *explorer.init_data.lock().unwrap() = Some("sepiida:style@winui".into());
    let explorer_tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { explorer_tap.SetSite(&explorer_site) }.unwrap();
    wait_for_callback(&explorer);
    assert!(!taskbar.has("Unadvise"));

    // Detaching the taskbar (its control window) leaves File Explorer's.
    send(Command::Detach);
    wait_until("taskbar unadvised", || taskbar.has("Unadvise"));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!explorer.has("Unadvise"));

    // Clearing File Explorer's site ends only its own attachment.
    clear_site(&explorer_tap);
    wait_until("File Explorer unadvised", || explorer.has("Unadvise"));
    assert_eq!(taskbar.count("Unadvise"), 1);
}

/// Moves the callback to another thread, as XAML calls it from each UI
/// thread. SAFETY: test-only; the DLL's callback object is agile.
struct SendCallback2(IVisualTreeServiceCallback2);
unsafe impl Send for SendCallback2 {}

fn clear_site(tap: &IObjectWithSite) {
    // SAFETY: plain COM call; a null site is how XAML detaches a TAP.
    unsafe { tap.SetSite(None) }.unwrap();
}

/// A XAML thread created after the colours were sent (a File Explorer
/// window opened later) starts with them, without another command.
#[test]
fn a_thread_that_appears_later_starts_with_the_colours_sent() {
    let _serial = serial();
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    send(Command::SetColor(Role::Surface, Color::new(1, 2, 3)));
    let painted = |shared: &Shared| {
        shared
            .calls()
            .iter()
            .filter(|c| c.starts_with(&format!("SetProperty 0x5 {FILL_INDEX} 0x")))
            .count()
    };
    let before = painted(&shared);
    assert!(before >= 1);

    let later = SendCallback2(callback.clone());
    std::thread::spawn(move || add_taskbar(&{ later }.0))
        .join()
        .unwrap();
    assert!(painted(&shared) > before, "the new thread painted nothing");

    send(Command::Detach);
    wait_until("unadvised", || shared.has("Unadvise"));
}
