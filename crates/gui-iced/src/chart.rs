use crate::format::{format_bytes, format_items, format_percent, fraction, truncate_middle};
use crate::styles;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Fill, Pixels, Point, Rectangle, Renderer, Size, Theme};
use lorimer_core::{NodeId, ScanNode, ScanSnapshot};
use std::f32::consts::{FRAC_PI_2, TAU};

const MAX_VISIBLE_DEPTH: usize = 6;
/// Sectors whose outer arc is shorter than this many pixels are folded into a remainder sector.
const MIN_ARC_LENGTH: f32 = 1.4;
/// Width of the gap between neighbouring sectors, in pixels.
const SECTOR_GAP: f32 = 1.6;
/// Each ring is this much thinner than the one inside it.
const RING_TAPER: f32 = 0.86;
const MAX_RING_WIDTH: f32 = 76.0;
const CHART_PADDING: f32 = 30.0;
const START_ANGLE: f32 = -FRAC_PI_2;

pub struct ChartProps<'a, Message> {
    pub snapshot: &'a ScanSnapshot,
    pub focus_id: NodeId,
    pub selected: Option<NodeId>,
    pub hovered: Option<NodeId>,
    /// Progress of the reveal animation, from `0.0` (hidden) to `1.0` (fully drawn).
    pub reveal: f32,
    pub on_hover: fn(Option<NodeId>) -> Message,
    pub on_select: fn(NodeId) -> Message,
    /// Published when the centre is clicked; `None` when the focus has no parent.
    pub on_up: Option<Message>,
}

pub fn sunburst<'a, Message: Clone + 'a>(props: ChartProps<'a, Message>) -> Element<'a, Message> {
    Canvas::new(Sunburst { props })
        .width(Fill)
        .height(Fill)
        .into()
}

struct Sunburst<'a, Message> {
    props: ChartProps<'a, Message>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartGeometry {
    pub center: Point,
    pub center_radius: f32,
    pub outer_radius: f32,
}

impl ChartGeometry {
    pub fn for_size(size: Size) -> Self {
        let outer_radius = (size.width.min(size.height) / 2.0 - CHART_PADDING).max(96.0);

        Self {
            center: Point::new(size.width / 2.0, size.height / 2.0),
            center_radius: (outer_radius * 0.27).clamp(56.0, 112.0),
            outer_radius,
        }
    }

    /// Inner and outer radius of every ring, innermost first.
    fn rings(&self, count: usize) -> Vec<(f32, f32)> {
        let count = count.max(1);
        let start = self.center_radius + 7.0;
        let available = (self.outer_radius - start).max(1.0);
        let taper_sum: f32 = (0..count).map(|ring| RING_TAPER.powi(ring as i32)).sum();
        let first_width = (available / taper_sum).min(MAX_RING_WIDTH);

        let mut rings = Vec::with_capacity(count);
        let mut inner = start;
        for ring in 0..count {
            let outer = inner + first_width * RING_TAPER.powi(ring as i32);
            rings.push((inner, outer));
            inner = outer;
        }

        rings
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sector {
    /// `None` for the remainder sector that stands in for children too small to draw.
    pub node_id: Option<NodeId>,
    pub start_angle: f32,
    pub end_angle: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Center,
    Sector(NodeId),
}

#[derive(Debug, Default)]
pub struct State {
    hit: Option<Hit>,
}

impl<Message: Clone> canvas::Program<Message> for Sunburst<'_, Message> {
    type State = State;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let props = &self.props;

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hit = self.hit(bounds, cursor);
                if hit == state.hit {
                    return None;
                }

                let previous = std::mem::replace(&mut state.hit, hit);
                let sector = |hit: Option<Hit>| match hit {
                    Some(Hit::Sector(id)) => Some(id),
                    _ => None,
                };

                if sector(previous) != sector(hit) {
                    Some(canvas::Action::publish((props.on_hover)(sector(hit))))
                } else {
                    Some(canvas::Action::request_redraw())
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                match self.hit(bounds, cursor)? {
                    Hit::Center => props
                        .on_up
                        .clone()
                        .map(|message| canvas::Action::publish(message).and_capture()),
                    Hit::Sector(id) => {
                        Some(canvas::Action::publish((props.on_select)(id)).and_capture())
                    }
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let props = &self.props;
        let geometry = ChartGeometry::for_size(bounds.size());
        let sectors = layout_sectors(props.snapshot, props.focus_id, geometry);
        let highlighted = highlight_path(props.snapshot, props.focus_id, props.hovered);
        let reveal = ease_out(props.reveal);
        let opacity = (props.reveal * 2.5).clamp(0.0, 1.0);
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        if sectors.is_empty() {
            let (inner, outer) = geometry.rings(1)[0];
            frame.stroke(
                &Path::circle(geometry.center, (inner + outer) / 2.0),
                Stroke::default()
                    .with_color(styles::FILL_SUBTLE)
                    .with_width(outer - inner),
            );
        }

        for sector in &sectors {
            let in_path = sector.node_id.is_some_and(|id| highlighted.contains(&id));
            let is_hovered = sector.node_id.is_some() && sector.node_id == props.hovered;
            let color = if is_hovered {
                styles::mix(sector.color, Color::WHITE, 0.16)
            } else if highlighted.is_empty() || in_path {
                sector.color
            } else {
                styles::mix(sector.color, styles::BACKGROUND, 0.56)
            };

            if let Some(path) = sector_path(geometry.center, sector, reveal, SECTOR_GAP) {
                frame.fill(&path, styles::with_alpha(color, color.a * opacity));
            }
        }

        if let Some(sector) = sectors
            .iter()
            .find(|sector| sector.node_id.is_some() && sector.node_id == props.selected)
        {
            if let Some(path) = sector_path(geometry.center, sector, reveal, SECTOR_GAP + 1.5) {
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(styles::with_alpha(Color::WHITE, 0.95 * opacity))
                        .with_width(2.0)
                        .with_line_join(canvas::LineJoin::Round),
                );
            }
        }

        self.draw_center(&mut frame, geometry, self.hit(bounds, cursor));

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match self.hit(bounds, cursor) {
            Some(Hit::Sector(_)) => mouse::Interaction::Pointer,
            Some(Hit::Center) if self.props.on_up.is_some() => mouse::Interaction::Pointer,
            _ => mouse::Interaction::default(),
        }
    }
}

impl<Message> Sunburst<'_, Message> {
    fn hit(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Hit> {
        let position = cursor.position_in(bounds)?;
        let geometry = ChartGeometry::for_size(bounds.size());
        let sectors = layout_sectors(self.props.snapshot, self.props.focus_id, geometry);

        hit_test(position, geometry, &sectors)
    }

    fn draw_center(&self, frame: &mut canvas::Frame, geometry: ChartGeometry, hit: Option<Hit>) {
        let props = &self.props;
        let Some(focus) = props.snapshot.node(props.focus_id) else {
            return;
        };
        let can_go_up = props.on_up.is_some();
        let center_hovered = hit == Some(Hit::Center) && can_go_up;
        let hovered = props.hovered.and_then(|id| props.snapshot.node(id));
        let node = hovered.unwrap_or(focus);

        let disc = Path::circle(geometry.center, geometry.center_radius);
        frame.fill(
            &disc,
            if center_hovered {
                styles::mix(styles::ELEVATED, Color::WHITE, 0.07)
            } else {
                styles::ELEVATED
            },
        );
        frame.stroke(
            &disc,
            Stroke::default()
                .with_color(styles::HAIRLINE)
                .with_width(1.0),
        );

        let caption = if center_hovered {
            "Click to go up".to_string()
        } else if hovered.is_some() {
            format!(
                "{} of {}",
                format_percent(fraction(node.size, focus.size)),
                truncate_middle(&short_label(focus), 14)
            )
        } else if node.is_directory() {
            format_items(node.item_count)
        } else {
            String::new()
        };

        let size_text = (geometry.center_radius * 0.34).clamp(19.0, 30.0);
        let name_chars = ((geometry.center_radius * 2.0 - 28.0) / 7.2) as usize;
        let mut line = |content: String, offset: f32, size: f32, color: Color, font| {
            frame.fill_text(Text {
                content,
                position: geometry.center + iced::Vector::new(0.0, offset),
                color,
                size: Pixels(size),
                font,
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        };

        line(
            truncate_middle(&short_label(node), name_chars.max(8)),
            -size_text * 0.95,
            13.0,
            styles::TEXT_SECONDARY,
            styles::FONT_MEDIUM,
        );
        line(
            format_bytes(node.size),
            2.0,
            size_text,
            styles::TEXT,
            styles::FONT_DISPLAY,
        );
        line(
            caption,
            size_text * 0.95 + 3.0,
            11.0,
            if center_hovered {
                styles::ACCENT
            } else {
                styles::TEXT_TERTIARY
            },
            styles::FONT,
        );
    }
}

/// Color the chart gives to the `index`-th direct child of the focused directory. The sidebar
/// uses this to keep its swatches in sync with the first ring.
pub fn child_color(child: &ScanNode, index: usize) -> Color {
    if child.is_virtual {
        styles::NEUTRAL_SECTOR
    } else {
        styles::root_color(index)
    }
}

pub fn layout_sectors(
    snapshot: &ScanSnapshot,
    focus_id: NodeId,
    geometry: ChartGeometry,
) -> Vec<Sector> {
    let depth = tree_depth(snapshot, focus_id, MAX_VISIBLE_DEPTH);
    if depth == 0 {
        return Vec::new();
    }

    let mut layout = SectorLayout {
        snapshot,
        rings: geometry.rings(depth),
        sectors: Vec::new(),
    };
    layout.fill(focus_id, 0, START_ANGLE, START_ANGLE + TAU, None);
    layout.sectors
}

struct SectorLayout<'a> {
    snapshot: &'a ScanSnapshot,
    rings: Vec<(f32, f32)>,
    sectors: Vec<Sector>,
}

impl SectorLayout<'_> {
    fn fill(
        &mut self,
        node_id: NodeId,
        ring: usize,
        start_angle: f32,
        end_angle: f32,
        parent_color: Option<Color>,
    ) {
        let Some(node) = self.snapshot.node(node_id) else {
            return;
        };
        let Some(&(inner_radius, outer_radius)) = self.rings.get(ring) else {
            return;
        };
        if node.children.is_empty() || node.size == 0 {
            return;
        }

        let total_span = end_angle - start_angle;
        let total_size = node.size as f64;
        let mut cursor = start_angle;
        let mut remainder_start = None;

        for (index, child_id) in node.children.iter().enumerate() {
            let Some(child) = self.snapshot.node(*child_id) else {
                continue;
            };
            if child.size == 0 {
                continue;
            }

            let span = (total_span as f64 * child.size as f64 / total_size) as f32;
            let next = (cursor + span).min(end_angle);

            // Children are sorted by size, so everything after the first sector that is too
            // small to draw is too small as well and ends up in one remainder sector.
            if remainder_start.is_some() || span * outer_radius < MIN_ARC_LENGTH {
                remainder_start.get_or_insert(cursor);
                cursor = next;
                continue;
            }

            let color = match parent_color {
                None => child_color(child, index),
                Some(_) if child.is_virtual => styles::mix(
                    styles::NEUTRAL_SECTOR,
                    styles::BACKGROUND,
                    0.07 * ring as f32,
                ),
                Some(parent) => {
                    let middle = (cursor + next) / 2.0;
                    styles::nested_color(
                        parent,
                        (middle - start_angle) / total_span,
                        total_span / TAU,
                        ring + 1,
                    )
                }
            };

            self.sectors.push(Sector {
                node_id: Some(*child_id),
                start_angle: cursor,
                end_angle: next,
                inner_radius,
                outer_radius,
                color,
            });

            if child.is_directory() {
                self.fill(*child_id, ring + 1, cursor, next, Some(color));
            }

            cursor = next;
        }

        if let Some(remainder_start) = remainder_start {
            if (cursor - remainder_start) * outer_radius >= MIN_ARC_LENGTH {
                self.sectors.push(Sector {
                    node_id: None,
                    start_angle: remainder_start,
                    end_angle: cursor,
                    inner_radius,
                    outer_radius,
                    color: styles::with_alpha(styles::NEUTRAL_SECTOR, 0.55),
                });
            }
        }
    }
}

/// Ids from the hovered node up to (but excluding) the focused directory.
fn highlight_path(
    snapshot: &ScanSnapshot,
    focus_id: NodeId,
    hovered: Option<NodeId>,
) -> Vec<NodeId> {
    let mut path = Vec::new();
    let mut current = hovered;

    while let Some(id) = current {
        if id == focus_id {
            return path;
        }
        path.push(id);
        current = snapshot.node(id).and_then(|node| node.parent_id);
    }

    // The hovered node is not inside the focused directory, so nothing is highlighted.
    Vec::new()
}

/// Outline of a sector with a gap to its neighbours. `reveal` scales every angle towards the
/// start of the chart, which sweeps the sectors in during the reveal animation.
fn sector_path(center: Point, sector: &Sector, reveal: f32, gap: f32) -> Option<Path> {
    annular_path(
        center,
        (sector.inner_radius, sector.outer_radius),
        START_ANGLE + (sector.start_angle - START_ANGLE) * reveal,
        START_ANGLE + (sector.end_angle - START_ANGLE) * reveal,
        gap,
    )
}

/// Ring segment between the `radii` (inner, outer) that leaves a gap of constant pixel width
/// on every side. Returns `None` when the gap swallows the whole segment.
fn annular_path(center: Point, radii: (f32, f32), start: f32, end: f32, gap: f32) -> Option<Path> {
    let half_gap = gap / 2.0;
    let inner_radius = radii.0 + half_gap;
    let outer_radius = radii.1 - half_gap;

    // A gap of constant pixel width needs a larger angular inset closer to the centre.
    let outer_inset = half_gap / outer_radius;
    let inner_inset = half_gap / inner_radius;
    if outer_radius <= inner_radius || end - start <= outer_inset * 2.0 {
        return None;
    }

    Some(Path::new(|builder| {
        let outer = arc_points(center, outer_radius, start + outer_inset, end - outer_inset);
        builder.move_to(outer[0]);
        for point in &outer[1..] {
            builder.line_to(*point);
        }

        if end - start > inner_inset * 2.0 {
            for point in arc_points(center, inner_radius, end - inner_inset, start + inner_inset) {
                builder.line_to(point);
            }
        } else {
            let middle = (start + end) / 2.0;
            builder.line_to(point_at(center, inner_radius, middle));
        }

        builder.close();
    }))
}

fn arc_points(center: Point, radius: f32, start_angle: f32, end_angle: f32) -> Vec<Point> {
    let arc_length = (end_angle - start_angle).abs() * radius;
    let segments = (arc_length / 3.0).ceil().clamp(1.0, 256.0) as usize;

    (0..=segments)
        .map(|step| {
            let t = step as f32 / segments as f32;
            point_at(center, radius, start_angle + (end_angle - start_angle) * t)
        })
        .collect()
}

fn point_at(center: Point, radius: f32, angle: f32) -> Point {
    Point::new(
        center.x + angle.cos() * radius,
        center.y + angle.sin() * radius,
    )
}

fn hit_test(position: Point, geometry: ChartGeometry, sectors: &[Sector]) -> Option<Hit> {
    let vector = position - geometry.center;
    let radius = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if radius <= geometry.center_radius {
        return Some(Hit::Center);
    }

    let mut angle = vector.y.atan2(vector.x);
    if angle < START_ANGLE {
        angle += TAU;
    }

    sectors
        .iter()
        .find(|sector| {
            radius >= sector.inner_radius
                && radius <= sector.outer_radius
                && angle >= sector.start_angle
                && angle <= sector.end_angle
        })
        .and_then(|sector| sector.node_id)
        .map(Hit::Sector)
}

fn tree_depth(snapshot: &ScanSnapshot, node_id: NodeId, limit: usize) -> usize {
    if limit == 0 {
        return 0;
    }

    let Some(node) = snapshot.node(node_id) else {
        return 0;
    };
    if node.children.is_empty() || node.size == 0 {
        return 0;
    }

    1 + node
        .children
        .iter()
        .map(|child| tree_depth(snapshot, *child, limit - 1))
        .max()
        .unwrap_or(0)
}

fn ease_out(progress: f32) -> f32 {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining * remaining * remaining
}

pub fn short_label(node: &ScanNode) -> String {
    if node.name.is_empty() {
        node.path.display().to_string()
    } else {
        node.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lorimer_core::ScanService;
    use std::fs;

    fn geometry() -> ChartGeometry {
        ChartGeometry::for_size(Size::new(800.0, 800.0))
    }

    /// `big/` holds 6000 bytes across two files, `small.bin` holds 2000 bytes, and `tiny.bin`
    /// is far too small to get its own sector.
    fn fixture() -> (tempfile::TempDir, ScanSnapshot) {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir(temp.path().join("big")).expect("mkdir");
        fs::write(temp.path().join("big/a.bin"), vec![0_u8; 4000]).expect("write");
        fs::write(temp.path().join("big/b.bin"), vec![0_u8; 2000]).expect("write");
        fs::write(temp.path().join("small.bin"), vec![0_u8; 2000]).expect("write");
        fs::write(temp.path().join("tiny.bin"), vec![0_u8; 1]).expect("write");

        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan");
        (temp, snapshot)
    }

    fn sector_for<'a>(snapshot: &ScanSnapshot, sectors: &'a [Sector], name: &str) -> &'a Sector {
        sectors
            .iter()
            .find(|sector| {
                sector
                    .node_id
                    .and_then(|id| snapshot.node(id))
                    .is_some_and(|node| node.name == name)
            })
            .unwrap_or_else(|| panic!("no sector for {name}"))
    }

    #[test]
    fn rings_taper_and_stay_inside_the_chart() {
        let geometry = geometry();
        let rings = geometry.rings(4);

        assert_eq!(rings.len(), 4);
        assert!(rings[0].0 > geometry.center_radius);
        assert!(rings[3].1 <= geometry.outer_radius + 0.01);
        for pair in rings.windows(2) {
            assert_eq!(pair[0].1, pair[1].0);
            assert!(pair[1].1 - pair[1].0 < pair[0].1 - pair[0].0);
        }
    }

    #[test]
    fn first_ring_is_proportional_to_size() {
        let (_temp, snapshot) = fixture();
        let sectors = layout_sectors(&snapshot, snapshot.root_id(), geometry());

        let big = sector_for(&snapshot, &sectors, "big");
        let small = sector_for(&snapshot, &sectors, "small.bin");
        let ratio = (big.end_angle - big.start_angle) / (small.end_angle - small.start_angle);

        assert!((ratio - 3.0).abs() < 0.01, "ratio was {ratio}");
        assert_eq!(big.start_angle, START_ANGLE);
    }

    #[test]
    fn nested_sectors_stay_within_their_parent() {
        let (_temp, snapshot) = fixture();
        let sectors = layout_sectors(&snapshot, snapshot.root_id(), geometry());

        let big = sector_for(&snapshot, &sectors, "big");
        for name in ["a.bin", "b.bin"] {
            let child = sector_for(&snapshot, &sectors, name);
            assert!(child.start_angle >= big.start_angle);
            assert!(child.end_angle <= big.end_angle + 0.0001);
            assert_eq!(child.inner_radius, big.outer_radius);
        }
    }

    #[test]
    fn children_too_small_to_draw_become_a_remainder_sector() {
        let (_temp, snapshot) = fixture();
        let sectors = layout_sectors(&snapshot, snapshot.root_id(), geometry());

        assert!(!sectors.iter().any(|sector| {
            sector
                .node_id
                .and_then(|id| snapshot.node(id))
                .is_some_and(|node| node.name == "tiny.bin")
        }));

        // 1 byte of 8001 is too thin even for a remainder, so the ring simply ends there.
        let covered: f32 = sectors
            .iter()
            .filter(|sector| sector.inner_radius == sectors[0].inner_radius)
            .map(|sector| sector.end_angle - sector.start_angle)
            .sum();
        assert!(covered <= TAU + 0.0001);
        assert!(covered > TAU * 0.99);
    }

    #[test]
    fn empty_directories_have_no_sectors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan");

        assert!(layout_sectors(&snapshot, snapshot.root_id(), geometry()).is_empty());
    }

    #[test]
    fn hit_test_finds_sectors_and_the_center() {
        let (_temp, snapshot) = fixture();
        let geometry = geometry();
        let sectors = layout_sectors(&snapshot, snapshot.root_id(), geometry);
        let small = sector_for(&snapshot, &sectors, "small.bin");

        let middle = point_at(
            geometry.center,
            (small.inner_radius + small.outer_radius) / 2.0,
            (small.start_angle + small.end_angle) / 2.0,
        );

        assert_eq!(
            hit_test(middle, geometry, &sectors),
            small.node_id.map(Hit::Sector)
        );
        assert_eq!(
            hit_test(geometry.center, geometry, &sectors),
            Some(Hit::Center)
        );
        assert_eq!(hit_test(Point::new(1.0, 1.0), geometry, &sectors), None);
    }

    #[test]
    fn highlight_path_walks_up_to_the_focus() {
        let (_temp, snapshot) = fixture();
        let root = snapshot.root_id();
        let big = snapshot
            .find_node(&snapshot.root().path.join("big"))
            .expect("big");
        let file = snapshot
            .find_node(&snapshot.root().path.join("big/a.bin"))
            .expect("a.bin");

        assert_eq!(highlight_path(&snapshot, root, Some(file)), vec![file, big]);
        assert!(highlight_path(&snapshot, root, None).is_empty());
        assert!(highlight_path(&snapshot, big, Some(root)).is_empty());
    }

    #[test]
    fn first_ring_colors_match_sidebar_swatches() {
        let (_temp, snapshot) = fixture();
        let sectors = layout_sectors(&snapshot, snapshot.root_id(), geometry());

        for (index, child_id) in snapshot.root().children.iter().enumerate() {
            let child = snapshot.node(*child_id).expect("child");
            if let Some(sector) = sectors.iter().find(|s| s.node_id == Some(*child_id)) {
                assert_eq!(sector.color, child_color(child, index));
            }
        }
    }

    #[test]
    fn collapsed_sectors_have_no_path() {
        let sector = Sector {
            node_id: None,
            start_angle: 0.0,
            end_angle: 0.0005,
            inner_radius: 100.0,
            outer_radius: 140.0,
            color: Color::WHITE,
        };

        assert!(sector_path(Point::ORIGIN, &sector, 1.0, SECTOR_GAP).is_none());
        assert!(sector_path(Point::ORIGIN, &sector, 0.0, SECTOR_GAP).is_none());
    }

    #[test]
    fn ease_out_is_clamped_and_monotonic() {
        assert_eq!(ease_out(-1.0), 0.0);
        assert_eq!(ease_out(2.0), 1.0);
        assert!(ease_out(0.25) < ease_out(0.5));
    }
}
