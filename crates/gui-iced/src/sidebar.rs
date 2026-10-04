use crate::{
    app::{divider, InspectorTarget, Lorimer, Message, SelectedEntry, SidebarTab},
    chart::{child_color, short_label},
    format::{
        format_bytes, format_count, format_duration, format_items, format_percent,
        format_timestamp, fraction, truncate_middle,
    },
    icons::{icon, Icon},
    styles::{self, DANGER, TEXT, TEXT_SECONDARY, TEXT_TERTIARY},
};
use iced::widget::{button, column, container, mouse_area, row, scrollable, space, text, Column};
use iced::{Alignment, Color, Element, Fill, FillPortion};
use lorimer_core::{NodeKind, NodeMetadata, ScanSnapshot, ScanStats, SystemActions};

/// Rows beyond this are summarized, which keeps very wide folders responsive.
const MAX_CONTENT_ROWS: usize = 120;
const HORIZONTAL_PADDING: f32 = 16.0;

pub fn view<'a>(app: &'a Lorimer, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
    let Some(selected) = app.selected_target() else {
        return container(space()).style(styles::sidebar).into();
    };

    let body = match app.tab {
        SidebarTab::Contents => contents(app, snapshot),
        SidebarTab::Largest => largest_files(app, snapshot),
        SidebarTab::Info => info(app, snapshot, &selected),
    };

    container(column![
        header(app, snapshot, &selected),
        divider(),
        container(tabs(app.tab)).padding([12.0, HORIZONTAL_PADDING]),
        scrollable(container(body).padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 14.0,
            left: 8.0,
        }))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(6)
                .scroller_width(6)
                .margin(2),
        ))
        .style(styles::scrollable)
        .width(Fill)
        .height(Fill),
    ])
    .width(Fill)
    .height(Fill)
    .style(styles::sidebar)
    .into()
}

fn header<'a>(
    app: &'a Lorimer,
    snapshot: &'a ScanSnapshot,
    selected: &InspectorTarget,
) -> Element<'a, Message> {
    let color = app.chart_color(snapshot, selected.node_id.or(selected.nearest_directory_id));
    let total = snapshot.root().size;
    let share = fraction(selected.size, total);

    let subtitle = match (selected.kind, selected.item_count) {
        (NodeKind::Directory, Some(count)) => format!("Folder  ·  {}", format_items(count)),
        (kind, _) => kind_label(kind).to_string(),
    };

    let identity = row![
        container(icon(kind_icon(selected.kind), 18.0, color))
            .center_x(36)
            .center_y(36)
            .style(styles::swatch(styles::with_alpha(color, 0.16), 9.0)),
        column![
            text(truncate_middle(&selected.title, 30))
                .size(15)
                .font(styles::FONT_SEMIBOLD)
                .color(TEXT),
            text(subtitle).size(12).color(TEXT_SECONDARY)
        ]
        .spacing(2)
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let figures = row![
        text(format_bytes(selected.size))
            .size(30)
            .font(styles::FONT_DISPLAY)
            .color(TEXT),
        space().width(Fill),
        text(format!("{} of scan", format_percent(share)))
            .size(12)
            .color(TEXT_SECONDARY)
    ]
    .align_y(Alignment::End);

    let mut actions = row![].spacing(8);
    if !selected.is_virtual {
        actions = actions
            .push(action_button("Open", Message::OpenSelected, false))
            .push(action_button(
                SystemActions::reveal_label(),
                Message::RevealSelected,
                false,
            ));
    }
    if let Some((label, message)) = focus_action(app, snapshot, selected) {
        actions = actions.push(action_button(label, message, true));
    }

    column![
        identity,
        figures,
        share_bar(share, color, 5.0),
        text(truncate_middle(&selected.path.display().to_string(), 52))
            .size(11)
            .color(TEXT_TERTIARY),
        actions
    ]
    .spacing(12)
    .padding(iced::Padding {
        top: 16.0,
        right: HORIZONTAL_PADDING,
        bottom: 16.0,
        left: HORIZONTAL_PADDING,
    })
    .into()
}

/// The navigation that makes sense for the selection: open a selected folder in the chart, or
/// jump to the folder that holds a selected file.
fn focus_action(
    app: &Lorimer,
    snapshot: &ScanSnapshot,
    selected: &InspectorTarget,
) -> Option<(String, Message)> {
    let focus_id = app.current_directory_id(snapshot);
    let directory_id = selected.nearest_directory_id?;
    if directory_id == focus_id {
        return None;
    }

    let directory = snapshot.node(directory_id)?;
    if directory.children.is_empty() {
        return None;
    }

    let label = if selected.kind == NodeKind::Directory && selected.node_id == Some(directory_id) {
        "Show Contents".to_string()
    } else {
        let name = selected
            .nearest_directory_label
            .clone()
            .unwrap_or_else(|| "Folder".to_string());
        format!("Go to {}", truncate_middle(&name, 14))
    };

    Some((label, Message::FocusDirectory(directory_id)))
}

fn tabs<'a>(active: SidebarTab) -> Element<'a, Message> {
    let segment = |label: &'static str, tab: SidebarTab| {
        button(
            text(label)
                .size(12)
                .font(styles::FONT_MEDIUM)
                .width(Fill)
                .center(),
        )
        .width(Fill)
        .padding([5, 8])
        .style(styles::segment(active == tab))
        .on_press(Message::TabSelected(tab))
    };

    container(
        row![
            segment("Contents", SidebarTab::Contents),
            segment("Largest Files", SidebarTab::Largest),
            segment("Info", SidebarTab::Info),
        ]
        .spacing(2),
    )
    .padding(2)
    .style(styles::segmented_track)
    .into()
}

fn contents<'a>(app: &'a Lorimer, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
    let focus_id = app.current_directory_id(snapshot);
    let Some(focus) = snapshot.node(focus_id) else {
        return empty_message("Nothing to show.");
    };

    let children: Vec<_> = focus
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, id)| snapshot.node(*id).map(|node| (index, node)))
        .collect();
    if children.is_empty() {
        return empty_message("This folder is empty.");
    }

    let mut list = Column::new().spacing(1);
    for (index, node) in children.iter().take(MAX_CONTENT_ROWS) {
        let color = child_color(node, *index);
        let is_selected = app.selected == Some(SelectedEntry::Tree(node.id));
        let is_hovered = app.hovered == Some(node.id);

        let title = row![
            text(truncate_middle(&short_label(node), 26))
                .size(13)
                .color(TEXT),
            space().width(Fill),
            text(format_bytes(node.size)).size(12).color(TEXT_SECONDARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let content = row![
            container(container(space().width(10).height(10)).style(styles::swatch(color, 3.0)))
                .padding(iced::Padding::ZERO.top(3.0)),
            column![
                title,
                share_bar(fraction(node.size, focus.size), color, 3.0)
            ]
            .spacing(6),
        ]
        .spacing(10)
        .align_y(Alignment::Start);

        list = list.push(
            mouse_area(
                button(content)
                    .width(Fill)
                    .padding([8, 10])
                    .style(styles::list_row(is_selected, is_hovered))
                    .on_press(Message::NodeSelected(node.id)),
            )
            .on_enter(Message::HoverChanged(Some(node.id)))
            .on_exit(Message::RowExited(node.id)),
        );
    }

    if children.len() > MAX_CONTENT_ROWS {
        let rest = &children[MAX_CONTENT_ROWS..];
        let size: u64 = rest.iter().map(|(_, node)| node.size).sum();
        list = list.push(
            container(
                text(format!(
                    "{} smaller items  ·  {}",
                    format_count(rest.len() as u64),
                    format_bytes(size)
                ))
                .size(12)
                .color(TEXT_TERTIARY),
            )
            .padding([10, 10]),
        );
    }

    list.into()
}

fn largest_files<'a>(app: &'a Lorimer, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
    let files = snapshot.largest_files();
    let Some(largest) = files.first() else {
        return empty_message("No files were found.");
    };

    let mut list = Column::new().spacing(1);
    for (index, entry) in files.iter().enumerate() {
        let is_selected = app.selected == Some(SelectedEntry::Largest(index));
        let color = app.chart_color(snapshot, snapshot.nearest_node_for_path(&entry.path));

        let title = row![
            text(truncate_middle(&entry.name, 26)).size(13).color(TEXT),
            space().width(Fill),
            text(format_bytes(entry.size))
                .size(12)
                .color(TEXT_SECONDARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let content = row![
            text(format!("{}", index + 1))
                .size(11)
                .font(styles::FONT_MEDIUM)
                .color(TEXT_TERTIARY)
                .width(16)
                .align_x(Alignment::End),
            column![
                title,
                text(truncate_middle(
                    &entry.parent_path.display().to_string(),
                    44
                ))
                .size(11)
                .color(TEXT_TERTIARY),
                share_bar(fraction(entry.size, largest.size), color, 3.0)
            ]
            .spacing(4),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        list = list.push(
            button(content)
                .width(Fill)
                .padding([8, 10])
                .style(styles::list_row(is_selected, false))
                .on_press(Message::LargestSelected(index)),
        );
    }

    list.into()
}

fn info<'a>(
    app: &'a Lorimer,
    snapshot: &'a ScanSnapshot,
    selected: &InspectorTarget,
) -> Element<'a, Message> {
    let details: Element<'a, Message> = match app.metadata_cache.get(&selected.path) {
        Some(Ok(metadata)) => metadata_rows(metadata),
        Some(Err(error)) => text(error.as_str()).size(12).color(DANGER).into(),
        None => text("Loading…").size(12).color(TEXT_TERTIARY).into(),
    };

    column![
        info_section("Selection", details),
        info_section("Scan", summary_rows(snapshot.stats())),
    ]
    .spacing(18)
    .padding([4, 8])
    .into()
}

fn info_section<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(title)
            .size(11)
            .font(styles::FONT_SEMIBOLD)
            .color(TEXT_TERTIARY),
        body
    ]
    .spacing(8)
    .into()
}

fn metadata_rows<'a>(metadata: &NodeMetadata) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(7);
    rows = rows.push(stat_row("Kind", kind_label(metadata.kind)));
    rows = rows.push(stat_row("Size", format_bytes(metadata.apparent_size)));

    if metadata.kind != NodeKind::Directory {
        if let Some(allocated) = metadata.allocated_size {
            rows = rows.push(stat_row("On disk", format_bytes(allocated)));
        }
    }
    if let Some(count) = metadata
        .item_count
        .filter(|_| metadata.kind == NodeKind::Directory)
    {
        rows = rows.push(stat_row("Items", format_count(count)));
    }
    if let Some(modified) = &metadata.modified_at {
        rows = rows.push(stat_row("Modified", format_timestamp(modified)));
    }
    if let Some(created) = &metadata.created_at {
        rows = rows.push(stat_row("Created", format_timestamp(created)));
    }
    if let Some(permissions) = &metadata.permissions {
        let access = if metadata.read_only {
            format!("{permissions}  ·  Read only")
        } else {
            permissions.clone()
        };
        rows = rows.push(stat_row("Permissions", access));
    }
    if metadata.skipped_count > 0 {
        rows = rows.push(stat_row("Skipped", format_count(metadata.skipped_count)));
    }
    if metadata.error_count > 0 {
        rows = rows.push(stat_row("Unreadable", format_count(metadata.error_count)));
    }
    rows = rows.push(stat_row("Where", metadata.path.display().to_string()));

    rows.into()
}

fn summary_rows<'a>(stats: &ScanStats) -> Element<'a, Message> {
    column![
        stat_row("Total size", format_bytes(stats.total_size)),
        stat_row("Files", format_count(stats.file_count)),
        stat_row("Folders", format_count(stats.directory_count)),
        stat_row("Skipped", format_count(stats.skipped_count)),
        stat_row("Unreadable", format_count(stats.error_count)),
        stat_row("Duration", format_duration(stats.duration)),
        stat_row("Where", stats.scanned_path.display().to_string()),
    ]
    .spacing(7)
    .into()
}

fn stat_row<'a>(label: &'a str, value: impl Into<String>) -> Element<'a, Message> {
    row![
        text(label).size(12).color(TEXT_SECONDARY).width(92),
        text(value.into()).size(12).color(TEXT).width(Fill)
    ]
    .spacing(10)
    .align_y(Alignment::Start)
    .into()
}

/// Thin track with a colored fill covering `share` of its width.
fn share_bar<'a>(share: f32, color: Color, height: f32) -> Element<'a, Message> {
    let filled = ((share.clamp(0.0, 1.0) * 1000.0).round() as u16).clamp(8, 1000);
    let radius = height / 2.0;

    let mut bar = row![container(space().height(height))
        .width(FillPortion(filled))
        .style(styles::swatch(color, radius))];
    if filled < 1000 {
        bar = bar.push(space().width(FillPortion(1000 - filled)));
    }

    container(bar)
        .width(Fill)
        .style(styles::swatch(styles::FILL_SUBTLE, radius))
        .into()
}

fn action_button<'a>(
    label: impl Into<String>,
    message: Message,
    primary: bool,
) -> Element<'a, Message> {
    button(text(label.into()).size(12).font(styles::FONT_MEDIUM))
        .padding([6, 11])
        .style(if primary {
            styles::primary_button
        } else {
            styles::secondary_button
        })
        .on_press(message)
        .into()
}

fn empty_message<'a>(message: &'a str) -> Element<'a, Message> {
    container(text(message).size(12).color(TEXT_TERTIARY))
        .center_x(Fill)
        .padding(28)
        .into()
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Directory => "Folder",
        NodeKind::File => "File",
        NodeKind::Symlink => "Symbolic link",
        NodeKind::Other => "Item",
    }
}

fn kind_icon(kind: NodeKind) -> Icon {
    match kind {
        NodeKind::Directory => Icon::Folder,
        _ => Icon::File,
    }
}
