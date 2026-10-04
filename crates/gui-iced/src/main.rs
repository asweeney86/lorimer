// A release build on Windows is a GUI program and must not open a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chart;
mod format;
mod icons;
mod layout;
mod logo;
mod platform;
mod sidebar;
mod styles;
mod welcome;

use app::Lorimer;
use iced::{window, Size};

/// Window icon as raw RGBA, generated from the logo by `assets/build-icons.sh`. Used for the
/// window and taskbar on Linux and Windows; macOS takes its icon from the app bundle or Dock.
const WINDOW_ICON: &[u8] = include_bytes!("../../../assets/icons/window-icon-128.rgba");
const WINDOW_ICON_SIZE: u32 = 128;

fn main() -> iced::Result {
    iced::application(Lorimer::new, Lorimer::update, Lorimer::view)
        .title(Lorimer::title)
        .theme(Lorimer::theme)
        .subscription(Lorimer::subscription)
        .default_font(styles::FONT)
        .antialiasing(true)
        .window(window::Settings {
            size: Size::new(1240.0, 800.0),
            min_size: Some(Size::new(720.0, 560.0)),
            position: window::Position::Centered,
            icon: window::icon::from_rgba(WINDOW_ICON.to_vec(), WINDOW_ICON_SIZE, WINDOW_ICON_SIZE)
                .ok(),
            // The toolbar extends under the title bar, as in current macOS apps.
            #[cfg(target_os = "macos")]
            platform_specific: window::settings::PlatformSpecific {
                title_hidden: true,
                titlebar_transparent: true,
                fullsize_content_view: true,
            },
            ..window::Settings::default()
        })
        .run()
}
