//! Desktop integration that the windowing toolkit does not cover.

/// Shows the Lorimer icon in the macOS Dock even when the app runs as a bare binary rather than
/// from an app bundle. Does nothing on other platforms, where the window icon is used instead.
pub fn set_dock_icon() {
    #[cfg(target_os = "macos")]
    {
        use objc2::{AnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;

        const DOCK_ICON: &[u8] = include_bytes!("../../../assets/icons/png/icon-512.png");

        // AppKit may only be used from the main thread.
        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        let data = NSData::with_bytes(DOCK_ICON);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };

        // SAFETY: called on the main thread with a valid, non-nil image, which the
        // application retains.
        unsafe {
            NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
        }
    }
}
