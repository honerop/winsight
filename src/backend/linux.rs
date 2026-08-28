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
                let _ = tx.send(FocusEvent {
                    window,
                    at: SystemTime::now(),
                });
            }
        }
        Ok(())
    }
}

// --- Sway: uses swayipc to subscribe to window focus events.
struct SwayBackend;

impl FocusBackend for SwayBackend {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()> {
        use swayipc::{Connection, EventType, WindowChange};

        let mut conn = Connection::new()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let mut events = conn
            .subscribe(&[EventType::Window])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        while let Some(event) = events.next() {
            match event {
                Ok(swayipc::Event::Window(window_event)) => {
                    if window_event.change == WindowChange::Focus {
                        let container = &window_event.container;
                        let window_name = container.name.clone();
                        let window_class = container.app_id.clone();
                        let window_identifier =
                            window_name.or(window_class).or(Some("unknown".to_string()));

                        let _ = tx.send(FocusEvent {
                            window: window_identifier,
                            at: SystemTime::now(),
                        });
                    }
                }
                Ok(_) => {} // other event types we didn't subscribe to; ignore
                Err(e) => {
                    eprintln!("Sway IPC error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        WM_NAME,
        WM_CLASS,
        UTF8_STRING,
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
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

    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{
            ChangeWindowAttributesAux, ConnectionExt, EventMask, Window,
        };
        use x11rb::protocol::Event;

        let (conn, screen_num) = x11rb::connect(None).map_err(io_err)?;
        let root = conn.setup().roots[screen_num].root;
        let atoms = Atoms::new(&conn).map_err(io_err)?.reply().map_err(io_err)?;

        // Root window announces focus changes via PropertyNotify on
        // _NET_ACTIVE_WINDOW — this is the "subscribe" step.
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(io_err)?
        .check()
        .map_err(io_err)?;

        let mut watched: Option<Window> = None;

        // Prime with whatever's focused right now.
        if let Some(win) =
            get_active_window(&conn, root, atoms._NET_ACTIVE_WINDOW).map_err(io_err)?
        {
            watch_title(&conn, win).map_err(io_err)?;
            watched = Some(win);
            let title = get_title(&conn, &atoms, win).map_err(io_err)?;
            let _ = tx.send(FocusEvent {
                window: title,
                at: SystemTime::now(),
            });
        }

        loop {
            let event = conn.wait_for_event().map_err(io_err)?;
            match event {
                Event::PropertyNotify(ev)
                    if ev.window == root && ev.atom == atoms._NET_ACTIVE_WINDOW =>
                {
                    match get_active_window(&conn, root, atoms._NET_ACTIVE_WINDOW)
                        .map_err(io_err)?
                    {
                        Some(win) => {
                            if watched != Some(win) {
                                watch_title(&conn, win).map_err(io_err)?;
                                watched = Some(win);
                            }
                            let title = get_title(&conn, &atoms, win).map_err(io_err)?;
                            let _ = tx.send(FocusEvent {
                                window: title,
                                at: SystemTime::now(),
                            });
                        }
                        None => {
                            watched = None;
                            let _ = tx.send(FocusEvent {
                                window: None,
                                at: SystemTime::now(),
                            });
                        }
                    }
                }
                // Title changed on the window we're currently watching,
                // without a focus change (e.g. tab switch inside a browser).
                Event::PropertyNotify(ev)
                    if Some(ev.window) == watched
                        && (ev.atom == atoms._NET_WM_NAME || ev.atom == atoms.WM_NAME) =>
                {
                    let title = get_title(&conn, &atoms, ev.window).map_err(io_err)?;
                    let _ = tx.send(FocusEvent {
                        window: title,
                        at: SystemTime::now(),
                    });
                }
                _ => {}
            }
        }
    }
}

fn get_active_window<C: x11rb::connection::Connection>(
    conn: &C,
    root: x11rb::protocol::xproto::Window,
    net_active_window: x11rb::protocol::xproto::Atom,
) -> Result<Option<x11rb::protocol::xproto::Window>, x11rb::errors::ReplyError> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    let reply = conn
        .get_property(false, root, net_active_window, AtomEnum::WINDOW, 0, 1)?
        .reply()?;
    Ok(reply
        .value32()
        .and_then(|mut v| v.next())
        .filter(|&w| w != 0))
}

fn watch_title<C: x11rb::connection::Connection>(
    conn: &C,
    win: x11rb::protocol::xproto::Window,
) -> Result<(), x11rb::errors::ReplyError> {
    use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt, EventMask};
    conn.change_window_attributes(
        win,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    Ok(())
}

fn get_title<C: x11rb::connection::Connection>(
    conn: &C,
    atoms: &Atoms,
    win: x11rb::protocol::xproto::Window,
) -> Result<Option<String>, x11rb::errors::ReplyError> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let reply = conn
        .get_property(false, win, atoms._NET_WM_NAME, atoms.UTF8_STRING, 0, 1024)?
        .reply()?;
    if !reply.value.is_empty() {
        if let Ok(s) = String::from_utf8(reply.value) {
            return Ok(Some(s));
        }
    }

    let reply = conn
        .get_property(false, win, atoms.WM_CLASS, AtomEnum::STRING, 0, 1024)?
        .reply()?;
    if !reply.value.is_empty() {
        if let Some(class) = reply
            .value
            .split(|&b| b == 0)
            .filter_map(|s| std::str::from_utf8(s).ok())
            .filter(|s| !s.is_empty())
            .last()
        {
            return Ok(Some(class.to_string()));
        }
    }

    Ok(None)
}
