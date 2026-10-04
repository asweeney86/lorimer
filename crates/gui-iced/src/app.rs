use crate::{
    chart::{self, short_label, ChartProps},
    format::{format_bytes, format_count, format_duration},
    icons::{icon, Icon},
    layout::compute_layout,
    platform, sidebar,
    styles::{self, DANGER, TEXT, TEXT_SECONDARY, TEXT_TERTIARY},
    welcome,
};
use directories::UserDirs;
use iced::widget::{button, column, container, mouse_area, responsive, row, space, stack, text};
use iced::{keyboard, window, Alignment, Color, Element, Fill, Subscription, Task, Theme};
use lorimer_core::{
    LargestFileEntry, MetadataRequest, MetadataService, NodeId, NodeKind, NodeMetadata, ScanCancel,
    ScanEvent, ScanProgress, ScanService, ScanSnapshot, SystemActions, VolumeService, VolumeUsage,
};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const TOOLBAR_HEIGHT: f32 = 38.0;
/// Nudges toolbar content up so it lines up with the macOS window controls.
const TOOLBAR_BOTTOM_INSET: f32 = 5.0;
/// Room for the window controls that overlay the toolbar on macOS.
#[cfg(target_os = "macos")]
const TOOLBAR_LEADING_INSET: f32 = 84.0;
#[cfg(not(target_os = "macos"))]
const TOOLBAR_LEADING_INSET: f32 = 14.0;
const MAX_VISIBLE_CRUMBS: usize = 4;
const REVEAL_DURATION: Duration = Duration::from_millis(650);
/// Progress events arrive per directory; the interface only needs a few per second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(90);

pub struct Lorimer {
    pub(crate) focus_id: Option<NodeId>,
    pub(crate) hovered: Option<NodeId>,
    pub(crate) locations: Vec<Location>,
    pub(crate) metadata_cache: HashMap<PathBuf, Result<NodeMetadata, String>>,
    pub(crate) notice: Option<String>,
    pub(crate) now: Instant,
    pub(crate) path_input: String,
    pub(crate) result: Option<ScanSnapshot>,
    reveal_started_at: Option<Instant>,
    pub(crate) scan: Option<ActiveScan>,
    pub(crate) selected: Option<SelectedEntry>,
    pub(crate) startup_volume: Option<Volume>,
    pub(crate) tab: SidebarTab,
}

#[derive(Debug, Clone)]
pub enum Message {
    Launched,
    PathChanged(String),
    ChooseFolder,
    FolderPicked(Option<PathBuf>),
    StartScan,
    ScanPath(PathBuf),
    Rescan,
    CancelScan,
    ShowWelcome,
    Scan(ScanEvent),
    Tick(Instant),
    HoverChanged(Option<NodeId>),
    RowExited(NodeId),
    NodeSelected(NodeId),
    LargestSelected(usize),
    FocusDirectory(NodeId),
    FocusParent,
    TabSelected(SidebarTab),
    OpenSelected,
    RevealSelected,
    MetadataLoaded {
        path: PathBuf,
        result: Result<NodeMetadata, String>,
    },
    ActionFinished(Result<(), String>),
    DismissNotice,
    DragWindow,
    ToggleMaximize,
    Key(keyboard::Event),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectedEntry {
    Tree(NodeId),
    Largest(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Contents,
    Largest,
    Info,
}

/// A scan that is still running.
#[derive(Debug, Clone)]
pub(crate) struct ActiveScan {
    /// Stops the scan when triggered. The scan then reports `ScanEvent::Cancelled`.
    pub cancel: ScanCancel,
    pub path: PathBuf,
    pub started_at: Instant,
    pub progress: Option<ScanProgress>,
}

/// A folder offered as a one-click scan target on the welcome screen.
#[derive(Debug, Clone)]
pub(crate) struct Location {
    pub label: &'static str,
    pub icon: Icon,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct Volume {
    pub name: String,
    pub path: PathBuf,
    pub usage: VolumeUsage,
}

#[derive(Debug, Clone)]
pub(crate) struct InspectorTarget {
    pub title: String,
    pub path: PathBuf,
    pub kind: NodeKind,
    pub size: u64,
    pub item_count: Option<u64>,
    pub error_count: u64,
    pub skipped_count: u64,
    pub is_virtual: bool,
    pub node_id: Option<NodeId>,
    pub nearest_directory_id: Option<NodeId>,
    pub nearest_directory_label: Option<String>,
}

impl InspectorTarget {
    fn metadata_request(&self) -> MetadataRequest {
        MetadataRequest {
            path: self.path.clone(),
            name: self.title.clone(),
            kind: self.kind,
            apparent_size: self.size,
            item_count: self.item_count,
            error_count: self.error_count,
            skipped_count: self.skipped_count,
            is_virtual: self.is_virtual,
        }
    }
}

impl Default for Lorimer {
    fn default() -> Self {
        Self {
            focus_id: None,
            hovered: None,
            locations: default_locations(),
            metadata_cache: HashMap::new(),
            notice: None,
            now: Instant::now(),
            path_input: default_scan_path(),
            result: None,
            reveal_started_at: None,
            scan: None,
            selected: None,
            startup_volume: startup_volume(),
            tab: SidebarTab::default(),
        }
    }
}

impl Lorimer {
    pub fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();

        // The Dock icon can only be set once the application is up, so it rides on a message.
        let launched = Task::done(Message::Launched);

        if let Ok(scan_path) = env::var("LORIMER_AUTOSCAN") {
            let task = app.begin_scan(PathBuf::from(scan_path));
            return (app, Task::batch([launched, task]));
        }

        (app, launched)
    }

    pub fn title(&self) -> String {
        match (&self.scan, &self.result) {
            (Some(_), _) => "Lorimer · Scanning".to_string(),
            (None, Some(snapshot)) => format!("Lorimer · {}", short_label(snapshot.root())),
            (None, None) => "Lorimer".to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        styles::theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let animating = self.scan.is_some() || self.reveal_started_at.is_some();

        Subscription::batch([
            keyboard::listen().map(Message::Key),
            if animating {
                window::frames().map(Message::Tick)
            } else {
                Subscription::none()
            },
        ])
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Launched => {
                platform::set_dock_icon();
                Task::none()
            }
            Message::PathChanged(path) => {
                self.path_input = path;
                Task::none()
            }
            Message::ChooseFolder => {
                if self.scan.is_some() {
                    return Task::none();
                }

                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose a folder to scan")
                            .pick_folder()
                            .await
                            .map(|handle| handle.path().to_path_buf())
                    },
                    Message::FolderPicked,
                )
            }
            Message::FolderPicked(path) => match path {
                Some(path) => self.begin_scan(path),
                None => Task::none(),
            },
            Message::StartScan => {
                let input = self.path_input.trim();
                if input.is_empty() {
                    self.notice = Some("Enter a folder path before scanning.".to_string());
                    return Task::none();
                }

                self.begin_scan(PathBuf::from(input))
            }
            Message::ScanPath(path) => self.begin_scan(path),
            Message::Rescan => match &self.result {
                Some(snapshot) => self.begin_scan(snapshot.stats().scanned_path.clone()),
                None => Task::none(),
            },
            Message::CancelScan => {
                if let Some(scan) = &self.scan {
                    scan.cancel.cancel();
                }
                Task::none()
            }
            Message::ShowWelcome => {
                if self.scan.is_none() {
                    self.clear_result();
                    self.startup_volume = startup_volume();
                }
                Task::none()
            }
            Message::Scan(event) => match event {
                ScanEvent::Started { .. } => Task::none(),
                ScanEvent::Progress(progress) => {
                    if let Some(scan) = &mut self.scan {
                        scan.progress = Some(progress);
                    }
                    Task::none()
                }
                ScanEvent::Finished(snapshot) => {
                    let root_id = snapshot.root_id();
                    self.scan = None;
                    self.focus_id = Some(root_id);
                    self.hovered = None;
                    self.selected = Some(SelectedEntry::Tree(root_id));
                    self.metadata_cache.clear();
                    self.result = Some(snapshot);
                    self.notice = None;
                    self.tab = SidebarTab::Contents;
                    self.start_reveal();

                    self.load_selected_metadata()
                }
                ScanEvent::Failed(error) => {
                    self.scan = None;
                    self.notice = Some(error);
                    Task::none()
                }
                ScanEvent::Cancelled => {
                    self.scan = None;
                    Task::none()
                }
            },
            Message::Tick(now) => {
                self.now = now;
                if self.reveal_progress() >= 1.0 {
                    self.reveal_started_at = None;
                }
                Task::none()
            }
            Message::HoverChanged(node_id) => {
                self.hovered = node_id;
                Task::none()
            }
            Message::RowExited(node_id) => {
                // A neighbouring row may already have claimed the hover.
                if self.hovered == Some(node_id) {
                    self.hovered = None;
                }
                Task::none()
            }
            Message::NodeSelected(node_id) => {
                if self.selected == Some(SelectedEntry::Tree(node_id)) && self.can_focus(node_id) {
                    return self.focus_directory(node_id);
                }

                self.selected = Some(SelectedEntry::Tree(node_id));
                self.load_selected_metadata()
            }
            Message::LargestSelected(index) => {
                self.selected = Some(SelectedEntry::Largest(index));
                self.load_selected_metadata()
            }
            Message::FocusDirectory(node_id) => self.focus_directory(node_id),
            Message::FocusParent => match self.focus_parent_id() {
                Some(parent_id) => self.focus_directory(parent_id),
                None => Task::none(),
            },
            Message::TabSelected(tab) => {
                self.tab = tab;
                Task::none()
            }
            Message::OpenSelected => self.perform_selected_action(SystemActions::open),
            Message::RevealSelected => self.perform_selected_action(SystemActions::reveal),
            Message::MetadataLoaded { path, result } => {
                self.metadata_cache.insert(path, result);
                Task::none()
            }
            Message::ActionFinished(result) => {
                if let Err(error) = result {
                    self.notice = Some(error);
                }
                Task::none()
            }
            Message::DismissNotice => {
                self.notice = None;
                Task::none()
            }
            Message::DragWindow => window::latest().and_then(window::drag),
            Message::ToggleMaximize => window::latest().and_then(window::toggle_maximize),
            Message::Key(event) => match shortcut(&event, self.scan.is_some()) {
                Some(message) => self.update(message),
                None => Task::none(),
            },
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut content = column![self.toolbar(), divider()];

        content = if let Some(scan) = &self.scan {
            content.push(welcome::scanning(self, scan))
        } else if let Some(snapshot) = &self.result {
            content
                .push(responsive(move |size| self.scan_view(snapshot, size)))
                .push(divider())
                .push(self.status_bar(snapshot))
        } else {
            content.push(welcome::view(self))
        };

        let mut layers = stack![content.width(Fill).height(Fill)];
        if let Some(notice) = &self.notice {
            layers = layers.push(notice_overlay(notice));
        }

        container(layers)
            .width(Fill)
            .height(Fill)
            .style(styles::app_background)
            .into()
    }

    fn begin_scan(&mut self, path: PathBuf) -> Task<Message> {
        if self.scan.is_some() {
            return Task::none();
        }

        self.clear_result();
        self.notice = None;
        self.path_input = path.display().to_string();
        self.now = Instant::now();
        let cancel = ScanCancel::new();
        self.scan = Some(ActiveScan {
            cancel: cancel.clone(),
            path: path.clone(),
            started_at: self.now,
            progress: None,
        });

        scan_task(path, cancel)
    }

    fn clear_result(&mut self) {
        self.focus_id = None;
        self.hovered = None;
        self.metadata_cache.clear();
        self.result = None;
        self.reveal_started_at = None;
        self.selected = None;
    }

    fn start_reveal(&mut self) {
        self.now = Instant::now();
        self.reveal_started_at = Some(self.now);
    }

    /// Progress of the chart reveal animation, `1.0` when idle.
    pub(crate) fn reveal_progress(&self) -> f32 {
        match self.reveal_started_at {
            Some(started_at) => (self.now.saturating_duration_since(started_at).as_secs_f32()
                / REVEAL_DURATION.as_secs_f32())
            .clamp(0.0, 1.0),
            None => 1.0,
        }
    }

    fn focus_directory(&mut self, node_id: NodeId) -> Task<Message> {
        if self.focus_id != Some(node_id) {
            self.start_reveal();
        }
        self.focus_id = Some(node_id);
        self.selected = Some(SelectedEntry::Tree(node_id));
        self.hovered = None;
        self.tab = SidebarTab::Contents;
        self.load_selected_metadata()
    }

    /// Whether the chart can be re-centred on `node_id`.
    fn can_focus(&self, node_id: NodeId) -> bool {
        self.result.as_ref().is_some_and(|snapshot| {
            self.current_directory_id(snapshot) != node_id
                && snapshot
                    .node(node_id)
                    .is_some_and(|node| node.is_directory() && !node.children.is_empty())
        })
    }

    fn focus_parent_id(&self) -> Option<NodeId> {
        let snapshot = self.result.as_ref()?;
        snapshot
            .node(self.current_directory_id(snapshot))
            .and_then(|node| node.parent_id)
    }

    pub(crate) fn current_directory_id(&self, snapshot: &ScanSnapshot) -> NodeId {
        self.focus_id
            .and_then(|id| {
                snapshot
                    .node(id)
                    .filter(|node| node.is_directory())
                    .map(|_| id)
            })
            .unwrap_or_else(|| snapshot.root_id())
    }

    pub(crate) fn selected_tree_id(&self) -> Option<NodeId> {
        match self.selected {
            Some(SelectedEntry::Tree(id)) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn selected_target(&self) -> Option<InspectorTarget> {
        let snapshot = self.result.as_ref()?;

        if let Some(selected) = self.selected {
            match selected {
                SelectedEntry::Tree(id) => {
                    if let Some(node) = snapshot.node(id) {
                        return Some(target_from_tree(snapshot, node));
                    }
                }
                SelectedEntry::Largest(index) => {
                    if let Some(entry) = snapshot.largest_files().get(index) {
                        return Some(target_from_largest(snapshot, entry));
                    }
                }
            }
        }

        let focus_id = self.current_directory_id(snapshot);
        snapshot
            .node(focus_id)
            .map(|node| target_from_tree(snapshot, node))
    }

    /// Chart color of the first-ring sector that contains `node_id`, so the sidebar can echo it.
    pub(crate) fn chart_color(&self, snapshot: &ScanSnapshot, node_id: Option<NodeId>) -> Color {
        let focus_id = self.current_directory_id(snapshot);
        let mut current = node_id.and_then(|id| snapshot.node(id));

        while let Some(node) = current {
            if node.parent_id == Some(focus_id) {
                let index = snapshot
                    .node(focus_id)
                    .and_then(|focus| focus.children.iter().position(|id| *id == node.id));
                return index.map_or(styles::ACCENT, |index| chart::child_color(node, index));
            }
            current = node.parent_id.and_then(|id| snapshot.node(id));
        }

        styles::ACCENT
    }

    fn load_selected_metadata(&mut self) -> Task<Message> {
        let Some(target) = self.selected_target() else {
            return Task::none();
        };

        if self.metadata_cache.contains_key(&target.path) {
            return Task::none();
        }

        let request = target.metadata_request();
        let path = target.path.clone();

        Task::perform(
            async move {
                let result = tokio::task::spawn_blocking(move || MetadataService::read(&request))
                    .await
                    .map_err(|error| format!("Metadata worker failed: {error}"))
                    .and_then(|result| result);

                (path, result)
            },
            |(path, result)| Message::MetadataLoaded { path, result },
        )
    }

    fn perform_selected_action(&self, action: fn(&Path) -> Result<(), String>) -> Task<Message> {
        let Some(target) = self.selected_target() else {
            return Task::none();
        };

        if target.is_virtual {
            return Task::none();
        }

        let path = target.path.clone();

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || action(&path))
                    .await
                    .map_err(|error| format!("Action failed to start: {error}"))
                    .and_then(|result| result)
            },
            Message::ActionFinished,
        )
    }

    fn toolbar(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match (&self.scan, &self.result) {
            (None, Some(snapshot)) => self.navigation_bar(snapshot),
            _ => container(
                text("Lorimer")
                    .size(13)
                    .font(styles::FONT_SEMIBOLD)
                    .color(TEXT_SECONDARY),
            )
            .center_x(Fill)
            .padding(iced::Padding::ZERO.right(TOOLBAR_LEADING_INSET))
            .into(),
        };

        // Empty toolbar space behaves like a native title bar: drag to move, double-click to zoom.
        mouse_area(
            container(content)
                .width(Fill)
                .height(TOOLBAR_HEIGHT)
                .align_y(Alignment::Center)
                .padding(
                    iced::Padding::ZERO
                        .left(TOOLBAR_LEADING_INSET)
                        .right(10.0)
                        .bottom(TOOLBAR_BOTTOM_INSET),
                ),
        )
        .on_press(Message::DragWindow)
        .on_double_click(Message::ToggleMaximize)
        .into()
    }

    fn navigation_bar<'a>(&'a self, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
        let focus_id = self.current_directory_id(snapshot);
        let can_go_up = snapshot
            .node(focus_id)
            .is_some_and(|node| node.parent_id.is_some());

        let back = button(icon(
            Icon::ChevronLeft,
            16.0,
            if can_go_up { TEXT } else { TEXT_TERTIARY },
        ))
        .padding([5, 6])
        .style(styles::ghost_button)
        .on_press_maybe(can_go_up.then_some(Message::FocusParent));

        let trail = snapshot.breadcrumbs(focus_id);
        let hidden = trail.len().saturating_sub(MAX_VISIBLE_CRUMBS);
        let mut crumbs = row![].spacing(2).align_y(Alignment::Center);

        if hidden > 0 {
            crumbs = crumbs
                .push(text("…").size(13).color(TEXT_TERTIARY))
                .push(icon(Icon::ChevronRight, 11.0, TEXT_TERTIARY));
        }

        for (index, node) in trail.iter().enumerate().skip(hidden) {
            let is_current = index + 1 == trail.len();
            let label = text(crate::format::truncate_middle(&short_label(node), 28)).size(13);

            crumbs = crumbs.push(if is_current {
                button(label.font(styles::FONT_SEMIBOLD))
                    .padding([5, 7])
                    .style(styles::crumb_current)
            } else {
                button(label)
                    .padding([5, 7])
                    .style(styles::ghost_button)
                    .on_press(Message::FocusDirectory(node.id))
            });

            if !is_current {
                crumbs = crumbs.push(icon(Icon::ChevronRight, 11.0, TEXT_TERTIARY));
            }
        }

        row![
            back,
            crumbs,
            space().width(Fill),
            button(icon(Icon::Refresh, 16.0, TEXT_SECONDARY))
                .padding([5, 6])
                .style(styles::ghost_button)
                .on_press(Message::Rescan),
            button(text("New Scan").size(13).font(styles::FONT_MEDIUM))
                .padding([5, 11])
                .style(styles::secondary_button)
                .on_press(Message::ShowWelcome),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn scan_view<'a>(
        &'a self,
        snapshot: &'a ScanSnapshot,
        size: iced::Size,
    ) -> Element<'a, Message> {
        let layout = compute_layout(size.width, size.height);
        let chart = self.chart_pane(snapshot);
        let sidebar = sidebar::view(self, snapshot);

        if layout.compact {
            column![
                container(chart).height(layout.chart_height),
                divider(),
                container(sidebar).height(Fill)
            ]
            .into()
        } else {
            row![
                container(chart).width(Fill).height(Fill),
                container(space().width(1).height(Fill)).style(styles::hairline),
                container(sidebar).width(layout.sidebar_width).height(Fill)
            ]
            .into()
        }
    }

    fn chart_pane<'a>(&'a self, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
        let focus_id = self.current_directory_id(snapshot);
        let can_go_up = snapshot
            .node(focus_id)
            .is_some_and(|node| node.parent_id.is_some());

        let chart = chart::sunburst(ChartProps {
            snapshot,
            focus_id,
            selected: self.selected_tree_id(),
            hovered: self.hovered,
            reveal: self.reveal_progress(),
            on_hover: Message::HoverChanged,
            on_select: Message::NodeSelected,
            on_up: can_go_up.then_some(Message::FocusParent),
        });

        let hint = text("Click to select  ·  Click a selected folder to open it  ·  Click the centre to go back")
            .size(11)
            .color(TEXT_TERTIARY);

        column![
            container(chart).width(Fill).height(Fill),
            container(hint).center_x(Fill).padding([0, 16])
        ]
        .padding(iced::Padding::ZERO.bottom(12.0))
        .into()
    }

    fn status_bar<'a>(&'a self, snapshot: &'a ScanSnapshot) -> Element<'a, Message> {
        let stats = snapshot.stats();
        let label = |content: String, color: Color| text(content).size(11).color(color);

        let mut summary = row![label(
            format!(
                "{} files  ·  {} folders",
                format_count(stats.file_count),
                format_count(stats.directory_count)
            ),
            TEXT_SECONDARY,
        ),]
        .spacing(14)
        .align_y(Alignment::Center);

        if stats.skipped_count > 0 {
            summary = summary.push(label(
                format!("{} skipped", format_count(stats.skipped_count)),
                TEXT_TERTIARY,
            ));
        }
        if stats.error_count > 0 {
            summary = summary.push(label(
                format!("{} unreadable", format_count(stats.error_count)),
                styles::WARNING,
            ));
        }

        container(
            row![
                summary,
                space().width(Fill),
                label(
                    format!(
                        "{} scanned in {}",
                        format_bytes(stats.total_size),
                        format_duration(stats.duration)
                    ),
                    TEXT_TERTIARY,
                ),
            ]
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .padding([6, 14])
        .style(styles::status_bar)
        .into()
    }
}

/// Runs a scan on a worker thread and streams its events, with progress throttled so a scan of
/// millions of directories does not flood the interface.
fn scan_task(path: PathBuf, cancel: ScanCancel) -> Task<Message> {
    let (sender, receiver) = iced::futures::channel::mpsc::unbounded();

    std::thread::spawn(move || {
        let (events, incoming) = crossbeam_channel::unbounded();
        let worker = std::thread::spawn(move || {
            let _ = ScanService::stream_with_cancel(path, events, cancel);
        });

        let mut last_progress: Option<Instant> = None;
        let mut concluded = false;

        for event in incoming {
            match &event {
                ScanEvent::Progress(_) => {
                    if last_progress.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                        continue;
                    }
                    last_progress = Some(Instant::now());
                }
                ScanEvent::Finished(_) | ScanEvent::Failed(_) | ScanEvent::Cancelled => {
                    concluded = true;
                }
                ScanEvent::Started { .. } => {}
            }

            if sender.unbounded_send(event).is_err() {
                return;
            }
        }

        if worker.join().is_err() || !concluded {
            let _ = sender.unbounded_send(ScanEvent::Failed(
                "The scan stopped unexpectedly.".to_string(),
            ));
        }
    });

    Task::stream(receiver).map(Message::Scan)
}

/// The action for a key press. While a scan is running, Escape cancels it and the navigation
/// shortcuts are inactive.
fn shortcut(event: &keyboard::Event, scanning: bool) -> Option<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };

    if scanning {
        return matches!(
            key.as_ref(),
            keyboard::Key::Named(keyboard::key::Named::Escape)
        )
        .then_some(Message::CancelScan);
    }

    match key.as_ref() {
        keyboard::Key::Character("o") if modifiers.command() => Some(Message::ChooseFolder),
        keyboard::Key::Character("r") if modifiers.command() => Some(Message::Rescan),
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) if modifiers.command() => {
            Some(Message::FocusParent)
        }
        keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::FocusParent),
        _ => None,
    }
}

fn notice_overlay(notice: &str) -> Element<'_, Message> {
    let banner = container(
        row![
            text(notice).size(13).color(TEXT),
            button(icon(Icon::Close, 12.0, TEXT_SECONDARY))
                .padding(5)
                .style(styles::ghost_button)
                .on_press(Message::DismissNotice)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(iced::Padding::from([8, 10]).left(14.0))
    .max_width(560)
    .style(styles::notice(DANGER));

    container(banner)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(44)
        .into()
}

pub(crate) fn divider<'a>() -> Element<'a, Message> {
    container(space().width(Fill).height(1))
        .style(styles::hairline)
        .into()
}

fn target_from_tree(snapshot: &ScanSnapshot, node: &lorimer_core::ScanNode) -> InspectorTarget {
    let nearest_directory_id = if node.is_directory() {
        Some(node.id)
    } else {
        node.parent_id
    };
    let nearest_directory_label = nearest_directory_id
        .and_then(|id| snapshot.node(id))
        .map(short_label);

    InspectorTarget {
        title: short_label(node),
        path: node.path.clone(),
        kind: node.kind,
        size: node.size,
        item_count: Some(node.item_count),
        error_count: node.error_count,
        skipped_count: node.skipped_count,
        is_virtual: node.is_virtual,
        node_id: Some(node.id),
        nearest_directory_id,
        nearest_directory_label,
    }
}

fn target_from_largest(snapshot: &ScanSnapshot, entry: &LargestFileEntry) -> InspectorTarget {
    let nearest_directory_id = snapshot.nearest_node_for_path(&entry.parent_path);
    let nearest_directory_label = nearest_directory_id
        .and_then(|id| snapshot.node(id))
        .map(short_label);

    InspectorTarget {
        title: entry.name.clone(),
        path: entry.path.clone(),
        kind: NodeKind::File,
        size: entry.size,
        item_count: None,
        error_count: 0,
        skipped_count: 0,
        is_virtual: false,
        node_id: snapshot.find_node(&entry.path),
        nearest_directory_id,
        nearest_directory_label,
    }
}

fn default_scan_path() -> String {
    if let Some(user_dirs) = UserDirs::new() {
        return user_dirs.home_dir().display().to_string();
    }

    startup_volume_path().display().to_string()
}

/// Root of the volume the operating system runs from: `/`, or the system drive on Windows.
fn startup_volume_path() -> PathBuf {
    #[cfg(windows)]
    {
        let drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        return PathBuf::from(format!("{drive}\\"));
    }

    #[allow(unreachable_code)]
    PathBuf::from("/")
}

fn default_locations() -> Vec<Location> {
    let mut locations = Vec::new();
    let mut push = |label, icon, path: Option<&Path>| {
        if let Some(path) = path.filter(|path| path.is_dir()) {
            locations.push(Location {
                label,
                icon,
                path: path.to_path_buf(),
            });
        }
    };

    if let Some(user_dirs) = UserDirs::new() {
        push("Home", Icon::Home, Some(user_dirs.home_dir()));
        push("Desktop", Icon::Desktop, user_dirs.desktop_dir());
        push("Documents", Icon::File, user_dirs.document_dir());
        push("Downloads", Icon::Download, user_dirs.download_dir());
    }
    push("Applications", Icon::Apps, Some(Path::new("/Applications")));

    locations
}

fn startup_volume() -> Option<Volume> {
    let path = startup_volume_path();
    let usage = VolumeService::usage(&path).ok()?;

    Some(Volume {
        name: startup_volume_name(&path),
        path,
        usage,
    })
}

/// A name for the startup volume as the platform's file manager would show it.
fn startup_volume_name(path: &Path) -> String {
    // On macOS the startup volume is listed in `/Volumes` as a link back to `/`.
    #[cfg(target_os = "macos")]
    if let Some(name) = std::fs::read_dir("/Volumes").ok().and_then(|entries| {
        entries
            .filter_map(Result::ok)
            .find(|entry| std::fs::read_link(entry.path()).is_ok_and(|target| target == path))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
    }) {
        return name;
    }

    #[cfg(windows)]
    {
        let drive = path.display().to_string();
        return format!("Local Disk ({})", drive.trim_end_matches('\\'));
    }

    #[allow(unreachable_code)]
    {
        let _ = path;
        "Startup Disk".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scanned_app() -> (tempfile::TempDir, Lorimer) {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("photos/raw")).expect("mkdir");
        fs::write(temp.path().join("photos/raw/a.raw"), vec![0_u8; 4096]).expect("write");
        fs::write(temp.path().join("photos/b.jpg"), vec![0_u8; 1024]).expect("write");
        fs::write(temp.path().join("notes.txt"), vec![0_u8; 512]).expect("write");

        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan");
        let mut app = Lorimer::default();
        let _ = app.update(Message::Scan(ScanEvent::Finished(snapshot)));

        (temp, app)
    }

    fn node_id(app: &Lorimer, relative: &str) -> NodeId {
        let snapshot = app.result.as_ref().expect("snapshot");
        snapshot
            .find_node(&snapshot.root().path.join(relative))
            .expect("node")
    }

    #[test]
    fn finished_scan_focuses_and_selects_the_root() {
        let (_temp, app) = scanned_app();
        let root_id = app.result.as_ref().expect("snapshot").root_id();

        assert!(app.scan.is_none());
        assert_eq!(app.focus_id, Some(root_id));
        assert_eq!(app.selected, Some(SelectedEntry::Tree(root_id)));
        assert!(app.reveal_progress() < 1.0);
    }

    #[test]
    fn failed_scan_surfaces_a_notice() {
        let mut app = Lorimer::default();
        let _ = app.begin_scan(PathBuf::from("/definitely/missing"));

        let _ = app.update(Message::Scan(ScanEvent::Failed(
            "No such folder.".to_string(),
        )));

        assert!(app.scan.is_none());
        assert_eq!(app.notice.as_deref(), Some("No such folder."));
    }

    #[test]
    fn selecting_a_selected_folder_opens_it() {
        let (_temp, mut app) = scanned_app();
        let photos = node_id(&app, "photos");

        let _ = app.update(Message::NodeSelected(photos));
        assert_eq!(app.selected, Some(SelectedEntry::Tree(photos)));
        assert_ne!(app.focus_id, Some(photos));

        let _ = app.update(Message::NodeSelected(photos));
        assert_eq!(app.focus_id, Some(photos));
    }

    #[test]
    fn selecting_a_file_twice_keeps_the_focus() {
        let (_temp, mut app) = scanned_app();
        let notes = node_id(&app, "notes.txt");
        let focus = app.focus_id;

        let _ = app.update(Message::NodeSelected(notes));
        let _ = app.update(Message::NodeSelected(notes));

        assert_eq!(app.focus_id, focus);
        assert_eq!(app.selected, Some(SelectedEntry::Tree(notes)));
    }

    #[test]
    fn focus_parent_walks_up_and_stops_at_the_root() {
        let (_temp, mut app) = scanned_app();
        let root_id = app.result.as_ref().expect("snapshot").root_id();
        let raw = node_id(&app, "photos/raw");

        let _ = app.update(Message::FocusDirectory(raw));
        let _ = app.update(Message::FocusParent);
        assert_eq!(app.focus_id, Some(node_id(&app, "photos")));

        let _ = app.update(Message::FocusParent);
        let _ = app.update(Message::FocusParent);
        assert_eq!(app.focus_id, Some(root_id));
    }

    #[test]
    fn row_exit_only_clears_its_own_hover() {
        let (_temp, mut app) = scanned_app();
        let photos = node_id(&app, "photos");
        let notes = node_id(&app, "notes.txt");

        let _ = app.update(Message::HoverChanged(Some(notes)));
        let _ = app.update(Message::RowExited(photos));
        assert_eq!(app.hovered, Some(notes));

        let _ = app.update(Message::RowExited(notes));
        assert_eq!(app.hovered, None);
    }

    #[test]
    fn chart_color_follows_the_first_ring_ancestor() {
        let (_temp, app) = scanned_app();
        let snapshot = app.result.as_ref().expect("snapshot");
        let photos = node_id(&app, "photos");
        let nested = node_id(&app, "photos/raw");

        assert_eq!(
            app.chart_color(snapshot, Some(nested)),
            app.chart_color(snapshot, Some(photos))
        );
        assert_eq!(
            app.chart_color(snapshot, Some(snapshot.root_id())),
            styles::ACCENT
        );
    }

    #[test]
    fn show_welcome_discards_the_result() {
        let (_temp, mut app) = scanned_app();

        let _ = app.update(Message::ShowWelcome);

        assert!(app.result.is_none());
        assert!(app.selected.is_none());
    }

    #[test]
    fn reveal_animation_finishes_after_its_duration() {
        let (_temp, mut app) = scanned_app();
        let later = app.now + REVEAL_DURATION + Duration::from_millis(1);

        let _ = app.update(Message::Tick(later));

        assert_eq!(app.reveal_progress(), 1.0);
        assert!(app.reveal_started_at.is_none());
    }

    #[test]
    fn shortcuts_map_to_navigation() {
        let press = |key, modifiers| keyboard::Event::KeyPressed {
            key,
            modified_key: keyboard::Key::Unidentified,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        };

        let escape = || {
            press(
                keyboard::Key::Named(keyboard::key::Named::Escape),
                keyboard::Modifiers::empty(),
            )
        };
        let rescan = press(
            keyboard::Key::Character("r".into()),
            keyboard::Modifiers::COMMAND,
        );
        let plain_r = press(
            keyboard::Key::Character("r".into()),
            keyboard::Modifiers::empty(),
        );

        assert!(matches!(
            shortcut(&escape(), false),
            Some(Message::FocusParent)
        ));
        assert!(matches!(shortcut(&rescan, false), Some(Message::Rescan)));
        assert!(shortcut(&plain_r, false).is_none());

        // During a scan, Escape cancels and the other shortcuts are inactive.
        assert!(matches!(
            shortcut(&escape(), true),
            Some(Message::CancelScan)
        ));
        assert!(shortcut(&rescan, true).is_none());
    }

    #[test]
    fn cancelling_triggers_the_signal_and_keeps_showing_the_scan_until_it_stops() {
        let mut app = Lorimer::default();
        let _ = app.begin_scan(PathBuf::from("/definitely/missing"));
        let cancel = app.scan.as_ref().expect("scan").cancel.clone();

        let _ = app.update(Message::CancelScan);

        assert!(cancel.is_cancelled());
        assert!(app.scan.is_some());
    }

    #[test]
    fn cancelled_scan_returns_to_the_welcome_screen_without_a_notice() {
        let mut app = Lorimer::default();
        let _ = app.begin_scan(PathBuf::from("/definitely/missing"));

        let _ = app.update(Message::Scan(ScanEvent::Cancelled));

        assert!(app.scan.is_none());
        assert!(app.result.is_none());
        assert!(app.notice.is_none());
    }

    #[test]
    fn cancel_without_a_running_scan_does_nothing() {
        let (_temp, mut app) = scanned_app();

        let _ = app.update(Message::CancelScan);

        assert!(app.result.is_some());
    }
}
