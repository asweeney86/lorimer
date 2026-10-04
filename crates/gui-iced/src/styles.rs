use iced::{
    font::{Family, Weight},
    gradient,
    theme::Palette,
    widget::{button, container, scrollable as scrollable_widget, text_input},
    Background, Border, Color, Font, Gradient, Radians, Shadow, Theme, Vector,
};

// Surfaces, from the window background up to raised controls.
pub const BACKGROUND: Color = color(20, 21, 25);
pub const BACKGROUND_TOP: Color = color(27, 28, 34);
pub const SIDEBAR: Color = color(26, 27, 32);
pub const ELEVATED: Color = color(36, 37, 44);
pub const HAIRLINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.07);
pub const FILL_SUBTLE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.05);
pub const FILL: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.08);
pub const FILL_HOVER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
pub const FILL_PRESSED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.16);

// Text hierarchy.
pub const TEXT: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.92);
pub const TEXT_SECONDARY: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.58);
pub const TEXT_TERTIARY: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.36);

pub const ACCENT: Color = color(10, 132, 255);
pub const WARNING: Color = color(255, 214, 10);
pub const DANGER: Color = color(255, 105, 97);

/// Color for aggregated or otherwise anonymous chart content.
pub const NEUTRAL_SECTOR: Color = color(92, 94, 108);

/// Hues for the first ring of the chart, ordered so neighbours blend into each other.
pub const ROOT_COLORS: [Color; 10] = [
    color(64, 156, 255),
    color(125, 122, 255),
    color(191, 110, 250),
    color(255, 100, 150),
    color(255, 122, 89),
    color(255, 169, 48),
    color(255, 214, 70),
    color(104, 214, 110),
    color(92, 220, 196),
    color(90, 200, 245),
];

// Each platform's own interface font. Linux has no single one, so it gets the default sans.
#[cfg(target_os = "macos")]
const UI_FAMILY: Family = Family::Name(".SF NS");
#[cfg(windows)]
const UI_FAMILY: Family = Family::Name("Segoe UI");
#[cfg(not(any(target_os = "macos", windows)))]
const UI_FAMILY: Family = Family::SansSerif;

#[cfg(target_os = "macos")]
const DISPLAY_FAMILY: Family = Family::Name(".SF NS Rounded");
#[cfg(not(target_os = "macos"))]
const DISPLAY_FAMILY: Family = UI_FAMILY;

pub const FONT: Font = font(UI_FAMILY, Weight::Normal);
pub const FONT_MEDIUM: Font = font(UI_FAMILY, Weight::Medium);
pub const FONT_SEMIBOLD: Font = font(UI_FAMILY, Weight::Semibold);
/// Rounded face used for large figures such as sizes.
pub const FONT_DISPLAY: Font = font(DISPLAY_FAMILY, Weight::Semibold);

pub fn theme() -> Theme {
    Theme::custom(
        "Lorimer",
        Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT,
            success: ROOT_COLORS[7],
            warning: WARNING,
            danger: DANGER,
        },
    )
}

pub fn app_background(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Gradient::Linear(
            gradient::Linear::new(Radians(std::f32::consts::PI))
                .add_stop(0.0, BACKGROUND_TOP)
                .add_stop(1.0, BACKGROUND),
        ))
        .color(TEXT)
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style::default().background(SIDEBAR).color(TEXT)
}

pub fn status_bar(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgba(0.0, 0.0, 0.0, 0.18))
        .color(TEXT_SECONDARY)
}

pub fn hairline(_: &Theme) -> container::Style {
    container::Style::default().background(HAIRLINE)
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(FILL_SUBTLE)),
        border: border(HAIRLINE, 1.0, 14.0),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..container::Style::default()
    }
}

/// Recessed track behind a segmented control.
pub fn segmented_track(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgba(0.0, 0.0, 0.0, 0.28))
        .border(border(HAIRLINE, 1.0, 9.0))
}

/// Solid rounded block, used for swatches, icon chips, and bar fills.
pub fn swatch(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        container::Style::default().background(color).border(border(
            Color::TRANSPARENT,
            0.0,
            radius,
        ))
    }
}

pub fn notice(tone: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(mix(ELEVATED, tone, 0.14))),
        border: border(with_alpha(tone, 0.45), 1.0, 10.0),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => mix(ACCENT, Color::WHITE, 0.10),
        button::Status::Pressed => mix(ACCENT, Color::BLACK, 0.12),
        button::Status::Disabled => with_alpha(ACCENT, 0.35),
        button::Status::Active => ACCENT,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: if matches!(status, button::Status::Disabled) {
            with_alpha(Color::WHITE, 0.5)
        } else {
            Color::WHITE
        },
        border: border(Color::TRANSPARENT, 0.0, 8.0),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        snap: true,
    }
}

pub fn secondary_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => FILL_HOVER,
            button::Status::Pressed => FILL_PRESSED,
            button::Status::Disabled => FILL_SUBTLE,
            button::Status::Active => FILL,
        })),
        text_color: if matches!(status, button::Status::Disabled) {
            TEXT_TERTIARY
        } else {
            TEXT
        },
        border: border(Color::TRANSPARENT, 0.0, 8.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// Borderless control that only shows a fill while hovered, as in a macOS toolbar.
pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(Background::Color(FILL)),
            button::Status::Pressed => Some(Background::Color(FILL_PRESSED)),
            _ => None,
        },
        text_color: if matches!(status, button::Status::Disabled) {
            TEXT_TERTIARY
        } else {
            TEXT_SECONDARY
        },
        border: border(Color::TRANSPARENT, 0.0, 7.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The current location in the breadcrumb trail.
pub fn crumb_current(_: &Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: TEXT,
        border: border(Color::TRANSPARENT, 0.0, 7.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn list_row(selected: bool, hovered: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = if selected {
            Some(with_alpha(ACCENT, 0.24))
        } else if hovered || matches!(status, button::Status::Hovered) {
            Some(FILL)
        } else if matches!(status, button::Status::Pressed) {
            Some(FILL_HOVER)
        } else {
            None
        };

        button::Style {
            background: background.map(Background::Color),
            text_color: TEXT,
            border: border(Color::TRANSPARENT, 0.0, 8.0),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if active {
            Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.14)))
        } else if matches!(status, button::Status::Hovered) {
            Some(Background::Color(FILL_SUBTLE))
        } else {
            None
        },
        text_color: if active { TEXT } else { TEXT_SECONDARY },
        border: border(Color::TRANSPARENT, 0.0, 7.0),
        shadow: if active {
            Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                offset: Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            }
        } else {
            Shadow::default()
        },
        snap: true,
    }
}

/// Large tappable tile on the welcome screen.
pub fn tile(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered => FILL_HOVER,
            button::Status::Pressed => FILL_PRESSED,
            _ => FILL_SUBTLE,
        })),
        text_color: TEXT,
        border: border(
            if matches!(status, button::Status::Hovered) {
                Color::from_rgba(1.0, 1.0, 1.0, 0.14)
            } else {
                HAIRLINE
            },
            1.0,
            12.0,
        ),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => ACCENT,
        text_input::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.16),
        _ => HAIRLINE,
    };

    text_input::Style {
        background: Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.22)),
        border: border(border_color, 1.0, 8.0),
        icon: TEXT_SECONDARY,
        placeholder: TEXT_TERTIARY,
        value: TEXT,
        selection: with_alpha(ACCENT, 0.45),
    }
}

/// Overlay-style scrollbar: invisible rail, thin thumb that brightens on hover.
pub fn scrollable(theme: &Theme, status: scrollable_widget::Status) -> scrollable_widget::Style {
    let mut style = scrollable_widget::default(theme, status);
    let thumb = match status {
        scrollable_widget::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        }
        | scrollable_widget::Status::Dragged { .. } => Color::from_rgba(1.0, 1.0, 1.0, 0.38),
        scrollable_widget::Status::Hovered { .. } => Color::from_rgba(1.0, 1.0, 1.0, 0.22),
        _ => Color::from_rgba(1.0, 1.0, 1.0, 0.10),
    };

    style.container = container::Style::default();
    style.vertical_rail.background = None;
    style.vertical_rail.border = border(Color::TRANSPARENT, 0.0, 3.0);
    style.vertical_rail.scroller.background = Background::Color(thumb);
    style.vertical_rail.scroller.border = border(Color::TRANSPARENT, 0.0, 3.0);
    style.horizontal_rail = style.vertical_rail;
    style
}

pub fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}

pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

pub fn mix(left: Color, right: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let keep = 1.0 - amount;

    Color::from_rgba(
        left.r * keep + right.r * amount,
        left.g * keep + right.g * amount,
        left.b * keep + right.b * amount,
        left.a * keep + right.a * amount,
    )
}

/// Color of the `index`-th child in the first ring of the chart.
pub fn root_color(index: usize) -> Color {
    ROOT_COLORS[index % ROOT_COLORS.len()]
}

/// Color for a sector nested under a sector colored `parent`.
///
/// `position` is where the child sits within its parent (`0.0..=1.0`) and `parent_span` is the
/// share of the full circle the parent covers. The hue drifts with the position so siblings stay
/// distinguishable, and it drifts further under wide parents so a chart dominated by one folder
/// still uses the whole palette. Colors are always taken from the curated palette, so no hue
/// ever turns out murky or glaring, and every ring further out sinks slightly into the
/// background, which leads the eye from the centre outwards.
pub fn nested_color(parent: Color, position: f32, parent_span: f32, depth: usize) -> Color {
    let hue = hue_of(parent);
    let hue_range = 22.0 + 150.0 * parent_span.clamp(0.0, 1.0);
    let drift = (position.clamp(0.0, 1.0) - 0.5) * hue_range;
    let step = depth.saturating_sub(1).min(6) as f32;

    mix(palette_at(hue + drift), BACKGROUND, 0.055 * step)
}

/// The palette color for an arbitrary hue, blended from the two nearest [`ROOT_COLORS`].
pub fn palette_at(hue: f32) -> Color {
    let hue = hue.rem_euclid(360.0);
    let mut stops = ROOT_COLORS.map(|color| (hue_of(color), color));
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));

    let after = stops.iter().position(|(stop, _)| *stop > hue).unwrap_or(0);
    let (end_hue, end) = stops[after];
    let (start_hue, start) = stops[(after + stops.len() - 1) % stops.len()];

    let gap = (end_hue - start_hue).rem_euclid(360.0);
    if gap < f32::EPSILON {
        return start;
    }

    mix(start, end, (hue - start_hue).rem_euclid(360.0) / gap)
}

/// Hue of `color` in degrees, `0.0` for greys.
fn hue_of(color: Color) -> f32 {
    let max = color.r.max(color.g).max(color.b);
    let min = color.r.min(color.g).min(color.b);
    let delta = max - min;

    if delta < f32::EPSILON {
        0.0
    } else if max == color.r {
        60.0 * ((color.g - color.b) / delta).rem_euclid(6.0)
    } else if max == color.g {
        60.0 * ((color.b - color.r) / delta + 2.0)
    } else {
        60.0 * ((color.r - color.g) / delta + 4.0)
    }
}

const fn font(family: Family, weight: Weight) -> Font {
    Font {
        family,
        weight,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    }
}

const fn color(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(left: Color, right: Color) {
        for (a, b) in [(left.r, right.r), (left.g, right.g), (left.b, right.b)] {
            assert!((a - b).abs() < 0.01, "{left:?} != {right:?}");
        }
    }

    #[test]
    fn hue_matches_primary_colors() {
        assert_eq!(hue_of(Color::from_rgb(1.0, 0.0, 0.0)), 0.0);
        assert_eq!(hue_of(Color::from_rgb(0.0, 1.0, 0.0)), 120.0);
        assert_eq!(hue_of(Color::from_rgb(0.0, 0.0, 1.0)), 240.0);
        assert_eq!(hue_of(Color::from_rgb(0.4, 0.4, 0.4)), 0.0);
    }

    #[test]
    fn root_colors_cycle() {
        assert_eq!(root_color(0), root_color(ROOT_COLORS.len()));
    }

    #[test]
    fn nested_colors_stay_valid_and_distinguish_siblings() {
        for base in ROOT_COLORS {
            for depth in 1..=8 {
                let first = nested_color(base, 0.0, 0.1, depth);
                let last = nested_color(base, 1.0, 0.1, depth);

                for channel in [first.r, first.g, first.b, last.r, last.g, last.b] {
                    assert!((0.0..=1.0).contains(&channel));
                }
                assert_ne!(first, last);
            }
        }
    }

    #[test]
    fn nested_colors_sink_into_the_background_with_depth() {
        let base = ROOT_COLORS[0];
        let distance = |color: Color| {
            (color.r - BACKGROUND.r).abs()
                + (color.g - BACKGROUND.g).abs()
                + (color.b - BACKGROUND.b).abs()
        };

        assert!(
            distance(nested_color(base, 0.5, 0.1, 6)) < distance(nested_color(base, 0.5, 0.1, 2))
        );
    }

    #[test]
    fn palette_lookup_returns_palette_colors_at_their_own_hue() {
        for base in ROOT_COLORS {
            assert_close(palette_at(hue_of(base)), base);
        }
    }

    #[test]
    fn palette_lookup_blends_between_neighbours_and_wraps() {
        for hue in [-30.0, 0.0, 5.0, 90.0, 180.0, 300.0, 359.9, 720.0] {
            let color = palette_at(hue);
            for channel in [color.r, color.g, color.b] {
                assert!((0.0..=1.0).contains(&channel));
            }
            // Every palette color is bright, and a blend of two of them stays bright.
            assert!(color.r.max(color.g).max(color.b) > 0.8, "{hue}: {color:?}");
        }
    }

    #[test]
    fn wide_parents_spread_their_children_across_more_hues() {
        let base = ROOT_COLORS[0];
        let hue = hue_of;
        let spread = |span| {
            (hue(nested_color(base, 1.0, span, 2)) - hue(nested_color(base, 0.0, span, 2)))
                .rem_euclid(360.0)
        };

        assert!(spread(1.0) > spread(0.05) * 3.0);
    }

    #[test]
    fn mix_clamps_amount() {
        assert_eq!(mix(Color::BLACK, Color::WHITE, 2.0), Color::WHITE);
        assert_eq!(mix(Color::BLACK, Color::WHITE, -1.0), Color::BLACK);
    }
}
