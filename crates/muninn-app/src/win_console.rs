//! Attach a GUI-subsystem release binary to its launching terminal so the
//! headless CLI flags still print. Compiled only for non-debug Windows builds
//! (see the `cfg` on the `mod` declaration in `main.rs`).

// This module is the only permitted unsafe surface in muninn-app: the Win32
// console FFI below cannot be called from safe Rust.
#![allow(unsafe_code)]

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
};

/// Give `println!` / `eprintln!` a real destination when a headless command
/// (`--version`, `--help`, `--check`, `--list-links`) runs from a terminal.
///
/// A GUI-subsystem process starts with no standard handles, so its output is
/// otherwise discarded. We attach to the parent console and bind `CONOUT$` to
/// stdout/stderr — but only when *both* handles are absent. If either is valid
/// (the shell redirected a stream to a file or pipe), we leave it alone so
/// redirection is never clobbered. An Explorer double-click has no parent
/// console, so `AttachConsole` fails and the app runs with no console window.
pub(crate) fn attach_parent_console() {
    // SAFETY: `GetStdHandle` only reads the process's standard-handle slots.
    let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    if !is_absent(stdout) || !is_absent(stderr) {
        return;
    }

    // SAFETY: `AttachConsole` takes no pointers and only updates process state.
    // It fails for an Explorer launch, which has no parent console to attach to.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }

    // Order matters: attach first, then open `CONOUT$`, then publish the handle.
    // Opening it before the console exists would hand `println!` a dead handle.
    // SAFETY: `CONOUT$` is a fixed, NUL-terminated wide literal; a null security
    // descriptor and template mean "defaults", which Win32 accepts here.
    let conout = unsafe {
        CreateFileW(
            windows_sys::core::w!("CONOUT$"),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if conout == INVALID_HANDLE_VALUE {
        return;
    }

    // SAFETY: `conout` is a console handle owned by this process; `SetStdHandle`
    // installs it into both standard slots for the rest of the process lifetime.
    unsafe {
        let _ = SetStdHandle(STD_OUTPUT_HANDLE, conout);
        let _ = SetStdHandle(STD_ERROR_HANDLE, conout);
    }
}

/// A standard handle is unusable when it is `NULL` (no handle) or
/// `INVALID_HANDLE_VALUE` (the call failed).
fn is_absent(handle: HANDLE) -> bool {
    handle.is_null() || handle == INVALID_HANDLE_VALUE
}
