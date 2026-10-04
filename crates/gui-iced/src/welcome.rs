//! The screens shown before a result exists: the welcome screen and the scan in progress.

use crate::{
    app::{ActiveScan, Location, Lorimer, Message, Volume},
    format::{format_bytes, format_count, format_duration, truncate_middle},
    icons::{icon, Icon},
    logo,
    styles::{self, TEXT, TEXT_SECONDARY, TEXT_TERTIARY},
};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Row};
use iced::{Alignment, Element, Fill, FillPortion};

const CONTENT_WIDTH: f32 = 560.0;
const TILES_PER_ROW: usize = 3;

pub fn view(app: &Lorimer) -> Element<'_, Message> {
    let hero = column![
        logo::mark(104.0, None),
        text("Lorimer")
            .size(30)
            .font(styles::FONT_SEMIBOLD)
            .color(TEXT),
        text("See what is taking up space, then take it back.")
            .size(14)
            .color(TEXT_SECONDARY),
    ]
    .spacing(8)
    .align_x(Alignment::Center);

    let mut content = column![hero].spacing(26).align_x(Alignment::Center);

    if let Some(volume) = &app.startup_volume {
        content = content.push(volume_card(volume));
    }

    content = content.push(location_tiles(&app.locations));
    content = content.push(path_entry(app));

    scrollable(
        container(content.width(CONTENT_WIDTH))
            .center_x(Fill)
            .padding([52, 24]),
    )
    .style(styles::scrollable)
    .width(Fill)
    .height(Fill)
    .into()
}

pub fn scanning<'a>(app: &'a Lorimer, scan: &'a ActiveScan) -> Element<'a, Message> {
    let elapsed = app.now.saturating_duration_since(scan.started_at);
    let name = scan
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| scan.path.display().to_string());

    let (folders, current) = match &scan.progress {
        Some(progress) => (
            format!(
                "{} folders scanned",
                format_count(progress.completed_directories)
            ),
            progress
                .current_path
                .as_ref()
                .map(|path| truncate_middle(&path.display().to_string(), 72))
                .unwrap_or_default(),
        ),
        None => ("Starting…".to_string(), String::new()),
    };

    container(
        column![
            logo::mark(132.0, Some(elapsed.as_secs_f32())),
            space().height(6),
            text(format!("Scanning {}", truncate_middle(&name, 36)))
                .size(20)
                .font(styles::FONT_SEMIBOLD)
                .color(TEXT),
            text(format!("{folders}  ·  {}", format_duration(elapsed)))
                .size(13)
                .color(TEXT_SECONDARY),
            text(current).size(11).color(TEXT_TERTIARY),
            space().height(10),
            cancel_button(scan.cancel.is_cancelled()),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center(Fill)
    .into()
}

/// The button stays on screen but goes inert once pressed: a large scan needs a moment to
/// wind down, and the label says so.
fn cancel_button<'a>(cancelling: bool) -> Element<'a, Message> {
    let label = if cancelling {
        "Cancelling…"
    } else {
        "Cancel"
    };

    button(text(label).size(13).font(styles::FONT_MEDIUM))
        .padding([6, 16])
        .style(styles::secondary_button)
        .on_press_maybe((!cancelling).then_some(Message::CancelScan))
        .into()
}

fn volume_card(volume: &Volume) -> Element<'_, Message> {
    let usage = volume.usage;
    let used = ((usage.used_fraction().clamp(0.0, 1.0) * 1000.0).round() as u16).clamp(8, 1000);
    // Nearly full disks deserve attention, so the bar shifts from the accent to a warm tone.
    let bar_color = if usage.used_fraction() > 0.9 {
        styles::DANGER
    } else {
        styles::ACCENT
    };

    let mut bar = row![container(space().height(6))
        .width(FillPortion(used))
        .style(styles::swatch(bar_color, 3.0))];
    if used < 1000 {
        bar = bar.push(space().width(FillPortion(1000 - used)));
    }

    container(
        row![
            container(icon(Icon::Disk, 22.0, styles::ACCENT))
                .center_x(44)
                .center_y(44)
                .style(styles::swatch(
                    styles::with_alpha(styles::ACCENT, 0.16),
                    11.0
                )),
            column![
                row![
                    text(volume.name.as_str())
                        .size(15)
                        .font(styles::FONT_SEMIBOLD)
                        .color(TEXT),
                    space().width(Fill),
                    text(format!(
                        "{} available of {}",
                        format_bytes(usage.available_bytes),
                        format_bytes(usage.total_bytes)
                    ))
                    .size(12)
                    .color(TEXT_SECONDARY),
                ]
                .align_y(Alignment::Center),
                container(bar)
                    .width(Fill)
                    .style(styles::swatch(styles::FILL, 3.0)),
            ]
            .spacing(10),
            button(text("Scan").size(13).font(styles::FONT_SEMIBOLD))
                .padding([7, 16])
                .style(styles::primary_button)
                .on_press(Message::ScanPath(volume.path.clone())),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(16)
    .width(Fill)
    .style(styles::card)
    .into()
}

fn location_tiles(locations: &[Location]) -> Element<'_, Message> {
    let mut tiles: Vec<Element<'_, Message>> = locations
        .iter()
        .map(|location| {
            tile(
                location.icon,
                location.label,
                Message::ScanPath(location.path.clone()),
            )
        })
        .collect();
    tiles.push(tile(Icon::Plus, "Choose Folder…", Message::ChooseFolder));

    let mut grid = column![].spacing(10);
    let mut tiles = tiles.into_iter().peekable();
    while tiles.peek().is_some() {
        let mut line = Row::new().spacing(10);
        for _ in 0..TILES_PER_ROW {
            line = line.push(match tiles.next() {
                Some(tile) => tile,
                None => space().width(Fill).into(),
            });
        }
        grid = grid.push(line);
    }

    grid.into()
}

fn tile<'a>(glyph: Icon, label: &'a str, message: Message) -> Element<'a, Message> {
    button(
        row![
            icon(glyph, 18.0, styles::ACCENT),
            text(label).size(13).font(styles::FONT_MEDIUM).color(TEXT)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([14, 14])
    .style(styles::tile)
    .on_press(message)
    .into()
}

fn path_entry(app: &Lorimer) -> Element<'_, Message> {
    row![
        text_input("Or type a path to scan", &app.path_input)
            .on_input(Message::PathChanged)
            .on_submit(Message::StartScan)
            .padding([7, 10])
            .size(13)
            .style(styles::input)
            .width(Fill),
        button(text("Scan").size(13).font(styles::FONT_MEDIUM))
            .padding([7, 14])
            .style(styles::secondary_button)
            .on_press(Message::StartScan),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
