use super::{FocusBackend, FocusEvent};
use std::cell::RefCell;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

use windows::Win32::Foundation::{HWND, MAX_PATH};
use windows::Win32::System::ProcessStatus::K32GetModuleBaseNameW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, TranslateMessage,
    EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, MSG, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};

// The Win32 hook callback is a bare `extern "system" fn` — no closures,
// no capturing state directly. We stash the Sender in thread-local
// storage so the callback can reach it.
thread_local! {
    static TX: RefCell<Option<Sender<FocusEvent>>> = RefCell::new(None);
}

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

    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()> {
        TX.with(|cell| *cell.borrow_mut() = Some(tx));

        unsafe {
            // EVENT_SYSTEM_FOREGROUND: window focus changed.
            // EVENT_OBJECT_NAMECHANGE: title changed on current window
            // (e.g. browser tab switch) — same idea as the X11 title watch.
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_OBJECT_NAMECHANGE,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );

            if hook == HWINEVENTHOOK::default() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "SetWinEventHook failed",
                ));
            }

            // Hooks deliver via the thread's message queue, so we need a
            // standard message pump. This call blocks for the life of
            // the process, matching the other backends' `run`.
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        Ok(())
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    // Only care about the window itself, not its child controls, and
    // ignore null hwnds (some events fire for non-window objects).
    if id_object != OBJID_WINDOW.0 || hwnd.is_invalid() {
        return;
    }
    if event != EVENT_SYSTEM_FOREGROUND && event != EVENT_OBJECT_NAMECHANGE {
        return;
    }

    let title = window_identifier(hwnd);

    TX.with(|cell| {
        if let Some(tx) = cell.borrow().as_ref() {
            let _ = tx.send(FocusEvent {
                window: title.clone(),
                at: SystemTime::now(),
            });
        }
    });
}

/// Prefer the window title; fall back to the owning process's exe name
/// if the title is empty (matches WM_CLASS fallback on the X11 side).
unsafe fn window_identifier(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len > 0 {
        return Some(String::from_utf16_lossy(&buf[..len as usize]));
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }

    let process = OpenProcess(
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
        false,
        pid,
    )
    .ok()?;
    let mut name_buf = [0u16; MAX_PATH as usize];
    let len = K32GetModuleBaseNameW(process, None, &mut name_buf);
    if len > 0 {
        Some(String::from_utf16_lossy(&name_buf[..len as usize]))
    } else {
        None
    }
}
