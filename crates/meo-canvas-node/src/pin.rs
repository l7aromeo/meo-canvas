//! Keeps the addon loaded until the process exits, on Windows.
//!
//! Node closes an addon with the environment that loaded it. When the last
//! worker thread holding `meo-canvas.node` ended, Windows unmapped the DLL
//! while rayon's pool and the GIF palette's pool, which the addon started and
//! which live as long as the process, were parked inside its code; the first
//! to wake executed freed memory, `0xC0000005` at `<Unloaded_meo-canvas.node>`
//! (`l7aromeo/meo-canvas#254`).
//!
//! A pin rather than stopping the pools when an environment ends: rayon's
//! global pool cannot be stopped, and another environment may still be using
//! it. On Linux and macOS a closed module with live threads stays mapped, so
//! nothing is needed there; `worker-unload.test.ts` checks every platform.

/// `GET_MODULE_HANDLE_EX_FLAG_PIN`: the module stays loaded until the process
/// ends, however often `FreeLibrary` is called.
const PIN: u32 = 0x0000_0001;

/// `GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS`: the module is the one holding the
/// address passed, so the DLL is found by where this code is rather than by a
/// file name a platform package may change.
const FROM_ADDRESS: u32 = 0x0000_0004;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleExW(
        flags: u32,
        module_name: *const u16,
        module: *mut *mut core::ffi::c_void,
    ) -> i32;
    fn GetLastError() -> u32;
}

/// Pins the module holding this code, or says why it could not.
pub(crate) fn keep_loaded() -> Result<(), String> {
    let address: fn() -> Result<(), String> = keep_loaded;
    let mut module = core::ptr::null_mut();
    // SAFETY: with FROM_ADDRESS the second argument is read as an address
    // inside a loaded module, which this function is, not as a string; `module`
    // is a valid place for the handle, which a pinned module never releases.
    let pinned = unsafe {
        GetModuleHandleExW(
            PIN | FROM_ADDRESS,
            address as *const u16,
            &raw mut module,
        )
    };
    if pinned != 0 {
        return Ok(());
    }
    // SAFETY: takes nothing and reads this thread's last error, which the
    // failed call above set.
    let code = unsafe { GetLastError() };
    Err(format!(
        "meo-canvas could not keep its addon loaded: GetModuleHandleExW failed \
         with Win32 error {code}, so a worker thread ending could unload it \
         under its own threads"
    ))
}
