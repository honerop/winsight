use super::{FocusBackend, FocusEvent};
use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

/// Look at env vars to figure out which compositor/WM is actually running,
/// and return the matching backend. This is the runtime half of the split:
/// one Linux binary, many possible compositors.
pub fn detect() -> std::io::Result<Box<dyn FocusBackend>> {
    if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return Ok(Box::new(HyprlandBackend));
    }
    if env::var("SWAYSOCK").is_ok() {
        return Ok(Box::new(SwayBackend));
    }
    if env::var("WAYLAND_DISPLAY").is_ok() {
        // GNOME/KDE/other Wayland compositors don't expose a generic
        // global-focus IPC for security reasons. GNOME needs a Shell
        // extension publishing over D-Bus; KDE needs a KWin script doing
        // the same. Both are real options, just not "free" like Hyprland/
        // Sway's built-in IPC — implement as their own backends if needed.
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Wayland compositor detected but not Hyprland or Sway — \
             needs a compositor-specific backend (e.g. a GNOME Shell \
             extension or KWin script exposing focus over D-Bus)",
        ));
    }
    if env::var("DISPLAY").is_ok() {
        return Ok(Box::new(X11Backend));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no known display server detected",
    ))
}

// --- Hyprland: reads .socket2.sock, exactly as in the first version ---
struct HyprlandBackend;

impl FocusBackend for HyprlandBackend {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()> {
        let runtime_dir = env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
        let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap();
        let sock_path = std::path::PathBuf::from(runtime_dir)
            .join("hypr")
            .join(sig)
            .join(".socket2.sock");

        let stream = UnixStream::connect(sock_path)?;
        for line in BufReader::new(stream).lines() {
            let line = line?;
            if let Some(rest) = line.strip_prefix("activewindow>>") {
                let class = rest.splitn(2, ',').next().unwrap_or("").to_string();
                let window = if class.is_empty() { None } else { Some(class) };
                let _ = tx.send(FocusEvent { window, at: SystemTime::now() });
            }
        }
        Ok(())
    }
}

// --- Sway: same idea as Hyprland, different socket + JSON event framing.
// Reach for the `swayipc` crate (subscribe to the "window" event) rather
// than hand-rolling the length-prefixed binary protocol yourself.
struct SwayBackend;

impl FocusBackend for SwayBackend {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn run(self: Box<Self>, _tx: Sender<FocusEvent>) -> std::io::Result<()> {
        // TODO: swayipc::Connection::new()?.subscribe([EventType::Window])?
        // and match on WindowChange::Focus, sending FocusEvent on each.
        unimplemented!("wire up the swayipc crate here")
    }
}

// --- X11 (any X11 WM: i3, bspwm, XFCE, etc.): watch _NET_ACTIVE_WINDOW on
// the root window via x11rb, then track PropertyNotify for window title/
// class changes. This one has no push "focus changed" event by default,
// so you subscribe to PropertyChangeMask on the root window instead.
struct X11Backend;

impl FocusBackend for X11Backend {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn run(self: Box<Self>, _tx: Sender<FocusEvent>) -> std::io::Result<()> {
        // TODO: x11rb — select PropertyChangeMask on the root window,
        // read _NET_ACTIVE_WINDOW on each PropertyNotify, then fetch
        // WM_CLASS for that window id and send a FocusEvent.
        unimplemented!("wire up x11rb here")
    }
}
