use super::{FocusBackend, FocusEvent};
use std::sync::mpsc::Sender;

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

    fn run(self: Box<Self>, _tx: Sender<FocusEvent>) -> std::io::Result<()> {
        // TODO:
        // 1. Register for NSWorkspace.shared.notificationCenter
        //    didActivateApplicationNotification.
        // 2. On each notification, read the activated app's bundle
        //    identifier (cheap, no permission needed) as the `window`
        //    field's minimum viable value.
        // 3. Optionally use the Accessibility API to also grab the
        //    frontmost window's title for finer-grained tracking.
        unimplemented!("wire up objc2-app-kit NSWorkspace notifications here")
    }
}
