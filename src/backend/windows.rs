use super::{FocusBackend, FocusEvent};
use std::sync::mpsc::Sender;

/// Windows gives you a real push event for this: SetWinEventHook with
/// EVENT_SYSTEM_FOREGROUND fires whenever the foreground window changes.
/// No polling needed. Use the `windows` crate (official, from Microsoft)
/// rather than `winapi` for new code.
pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> std::io::Result<Self> {
        Ok(WindowsBackend)
    }
}

impl FocusBackend for WindowsBackend {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn run(self: Box<Self>, _tx: Sender<FocusEvent>) -> std::io::Result<()> {
        // TODO:
        // 1. SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, ...)
        //    with a callback that fires on foreground-window changes.
        // 2. In the callback, call GetWindowThreadProcessId on the HWND,
        //    then look up the process name (QueryFullProcessImageNameW)
        //    as the `window` identifier.
        // 3. This requires a Win32 message loop running on the same
        //    thread as the hook (GetMessage/DispatchMessage) — the hook
        //    callback won't fire without one.
        unimplemented!("wire up windows-rs SetWinEventHook here")
    }
}
