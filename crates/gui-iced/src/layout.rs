/// Below this window width the sidebar moves under the chart.
const COMPACT_BREAKPOINT: f32 = 860.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutSpec {
    /// Stack the sidebar below the chart instead of beside it.
    pub compact: bool,
    /// Height of the chart pane in the compact layout. Wide layouts fill the window height.
    pub chart_height: f32,
    /// Width of the sidebar in the wide layout. Compact layouts fill the window width.
    pub sidebar_width: f32,
}

pub fn compute_layout(width: f32, height: f32) -> LayoutSpec {
    LayoutSpec {
        compact: width < COMPACT_BREAKPOINT,
        chart_height: (height * 0.52).clamp(260.0, 520.0),
        sidebar_width: (width * 0.30).clamp(300.0, 380.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_layout_is_selected_for_small_widths() {
        let layout = compute_layout(760.0, 700.0);

        assert!(layout.compact);
        assert!(layout.chart_height >= 260.0);
        assert!(layout.chart_height < 700.0);
    }

    #[test]
    fn wide_layout_keeps_sidebar_reasonable() {
        for width in [860.0, 1280.0, 2560.0] {
            let layout = compute_layout(width, 900.0);

            assert!(!layout.compact);
            assert!((300.0..=380.0).contains(&layout.sidebar_width));
            assert!(width - layout.sidebar_width >= 480.0);
        }
    }

    #[test]
    fn layout_values_remain_positive() {
        let layout = compute_layout(1.0, 1.0);

        assert!(layout.chart_height > 0.0);
        assert!(layout.sidebar_width > 0.0);
    }
}
