use super::{FocusBackend, FocusEvent};
use std::sync::mpsc::Sender;
use std::time::SystemTime;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSNotification, NSOperationQueue};

/// macOS has a real push notification for app-level focus
/// (NSWorkspace.didActivateApplicationNotification), which is easy.
/// Getting the *window title* (not just the app) additionally requires
/// the Accessibility API (AXUIElement) and the user granting your app
/// Accessibility permissions in System Settings — there's no way around
/// that prompt, Apple gates it deliberately.
///
/// Practical crate options: `objc2` + `objc2-app-kit` for the
/// NSWorkspace notification, `accessibility` or raw `core-foundation`
/// bindings for the AX calls.
pub struct MacOsBackend;

impl MacOsBackend {
    pub fn new() -> std::io::Result<Self> {
        Ok(MacOsBackend)
    }
}

impl FocusBackend for MacOsBackend {
    fn name(&self) -> &'static str {
        "macos"
    }

    fn run(self: Box<Self>, tx: Sender<FocusEvent>) -> std::io::Result<()> {
        // AppKit notification/run-loop APIs require the main thread —
        // this backend must be `run` from the process's main thread.
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "macOS focus backend must run on the main thread",
            )
        })?;

        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let center = workspace.notificationCenter();

            // Prime with whatever's frontmost right now, same as the
            // Sway/X11 backends sending an initial state before
            // streaming changes.
            if let Some(app) = workspace.frontmostApplication() {
                let _ = tx.send(FocusEvent {
                    window: app_identifier(&app),
                    at: SystemTime::now(),
                });
            }

            // Step 1 + 2: register for didActivateApplicationNotification,
            // pull the bundle identifier off each notification's
            // userInfo as the minimum-viable `window` value.
            let tx_for_block = tx.clone();
            let block = block2::RcBlock::new(move |note: std::ptr::NonNull<NSNotification>| {
                let note = note.as_ref();
                let app: Option<Retained<NSRunningApplication>> =
                    note.userInfo().and_then(|info| {
                        let key = objc2_foundation::ns_string!("NSWorkspaceApplicationKey");
                        info.objectForKey(key)
                            .map(|obj| Retained::cast::<NSRunningApplication>(obj))
                    });

                let window = app.as_deref().and_then(|a| unsafe { app_identifier(a) });
                let _ = tx_for_block.send(FocusEvent {
                    window,
                    at: SystemTime::now(),
                });
            });

            let name =
                objc2_foundation::ns_string!("NSWorkspaceDidActivateApplicationNotification");
            let _: () = msg_send![
                &*center,
                addObserverForName: name,
                object: std::ptr::null::<objc2_foundation::NSObject>(),
                queue: NSOperationQueue::mainQueue(mtm).as_ref(),
                usingBlock: &*block,
            ];
        }

        // Notifications deliver through the run loop, so pump one here —
        // this call blocks for the life of the process, same role as the
        // X11 wait_for_event loop / Windows message pump.
        unsafe {
            use objc2_foundation::NSRunLoop;
            let run_loop = NSRunLoop::currentRunLoop();
            loop {
                run_loop.run();
            }
        }
    }
}

/// Bundle identifier as the cheap, no-permission-needed `window` value
/// (step 2 of the TODO). Falls back to it if the localized name is
/// unavailable for some reason.
unsafe fn app_identifier(app: &NSRunningApplication) -> Option<String> {
    if let Some(name) = app.localizedName() {
        return Some(name.to_string());
    }
    app.bundleIdentifier().map(|s| s.to_string())
}
