use std::sync::mpsc::Sender;
use std::time::SystemTime;

/// One focus-change, pushed by whichever backend is active.
/// `None` window = nothing focused (locked screen, empty workspace, desktop).
#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub window: Option<String>, // app/class identifier, e.g. "firefox"
    pub at: SystemTime,
}

/// Every platform backend implements this. It owns its own event loop
/// (thread, socket, hook, whatever) and just pushes events into `tx`.
/// This is the ONLY thing that differs per platform — everything else
/// (accumulation, storage, CLI, reporting) is written once and never
/// touches `cfg(target_os = ...)`.
pub trait FocusBackend: Send {
    fn name(&self) -> &'static str;
    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()>;
}

/// Compile-time OS split. Each module only compiles on its target OS,
/// so a Linux build never pulls in windows-rs, and vice versa.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Pick a backend. On Linux this is a *runtime* decision (the compositor
/// isn't known until the program actually starts); on Windows/macOS
/// there's only ever one implementation, so it's a straight return.
pub fn detect_backend() -> std::io::Result<Box<dyn FocusBackend>> {
    #[cfg(target_os = "linux")]
    {
        linux::detect()
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOsBackend::new()?))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsBackend::new()?))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no focus-tracking backend for this OS",
        ))
    }
}
