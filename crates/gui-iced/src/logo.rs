//! The Lorimer mark, drawn on a canvas so it stays crisp at any size. The geometry mirrors
//! `assets/logo/mark.svg`; keep the two in sync.

use crate::styles;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Theme};
use std::f32::consts::{FRAC_PI_2, PI};

/// Square region of the logo artwork that the mark occupies, as (x, y, side).
const ARTBOARD: (f32, f32, f32) = (225.0, 178.0, 630.0);
const HUB: Point = Point::new(548.0, 492.0);
const HUB_RADIUS: f32 = 40.0;
const STRIPE_WIDTH: f32 = 66.0;
/// Each stripe as (radius of its bend, top of the upright, end of the foot, palette index).
const STRIPES: [(f32, f32, f32, usize); 3] = [
    (268.0, 262.0, 742.0, 0),
    (188.0, 226.0, 800.0, 2),
    (108.0, 300.0, 690.0, 3),
];
const BEND_SEGMENTS: usize = 24;

/// Seconds for one loop of the drawing animation, and how much later each stripe starts.
const LOOP_SECONDS: f32 = 2.2;
const STRIPE_STAGGER: f32 = 0.16;
const DRAW_SECONDS: f32 = 1.0;
const FADE_SECONDS: f32 = 0.35;

/// The mark at `size` pixels. With `time` (seconds) the stripes draw themselves in a loop, which
/// serves as the activity indicator; without it the mark is static.
pub fn mark<'a, Message: 'a>(size: f32, time: Option<f32>) -> Element<'a, Message> {
    Canvas::new(Mark { time }).width(size).height(size).into()
}

struct Mark {
    time: Option<f32>,
}

impl<Message> canvas::Program<Message> for Mark {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let scale = bounds.width.min(bounds.height) / ARTBOARD.2;
        let place = |point: Point| {
            Point::new(
                (point.x - ARTBOARD.0) * scale,
                (point.y - ARTBOARD.1) * scale,
            )
        };

        for (index, stripe) in STRIPES.iter().enumerate() {
            let (drawn, opacity) = match self.time {
                Some(time) => animation(time, index),
                None => (1.0, 1.0),
            };
            let points = partial(&stripe_points(*stripe), drawn);
            let Some((first, rest)) = points.split_first() else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }

            let path = Path::new(|builder| {
                builder.move_to(place(*first));
                for point in rest {
                    builder.line_to(place(*point));
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(styles::with_alpha(styles::root_color(stripe.3), opacity))
                    .with_width(STRIPE_WIDTH * scale)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
        }

        frame.fill(
            &Path::circle(place(HUB), HUB_RADIUS * scale),
            Color::from_rgb8(0xF5, 0xF5, 0xF7),
        );

        vec![frame.into_geometry()]
    }
}

/// Centre line of a stripe in artwork coordinates: down the upright, around the bend, and out
/// along the foot.
fn stripe_points((radius, top, right, _): (f32, f32, f32, usize)) -> Vec<Point> {
    let mut points = vec![Point::new(HUB.x - radius, top)];

    for step in 0..=BEND_SEGMENTS {
        let angle = PI - FRAC_PI_2 * step as f32 / BEND_SEGMENTS as f32;
        points.push(Point::new(
            HUB.x + radius * angle.cos(),
            HUB.y + radius * angle.sin(),
        ));
    }

    points.push(Point::new(right, HUB.y + radius));
    points
}

/// The first `fraction` (by length) of the polyline `points`.
fn partial(points: &[Point], fraction: f32) -> Vec<Point> {
    let fraction = fraction.clamp(0.0, 1.0);
    let total: f32 = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    let mut remaining = total * fraction;
    let mut result = Vec::with_capacity(points.len());

    for pair in points.windows(2) {
        if result.is_empty() {
            result.push(pair[0]);
        }

        let length = pair[0].distance(pair[1]);
        if length >= remaining {
            if length > 0.0 && remaining > 0.0 {
                let t = remaining / length;
                result.push(Point::new(
                    pair[0].x + (pair[1].x - pair[0].x) * t,
                    pair[0].y + (pair[1].y - pair[0].y) * t,
                ));
            }
            break;
        }

        result.push(pair[1]);
        remaining -= length;
    }

    result
}

/// How much of stripe `index` is drawn and how opaque it is at `time` seconds into the loop.
fn animation(time: f32, index: usize) -> (f32, f32) {
    let phase = time.rem_euclid(LOOP_SECONDS);
    let progress = ((phase - index as f32 * STRIPE_STAGGER) / DRAW_SECONDS).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - progress).powi(3);
    let opacity = ((LOOP_SECONDS - phase) / FADE_SECONDS).clamp(0.0, 1.0);

    (eased, opacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripes_stay_inside_the_artboard() {
        let (x, y, side) = ARTBOARD;
        let half = STRIPE_WIDTH / 2.0;

        for stripe in STRIPES {
            for point in stripe_points(stripe) {
                assert!(
                    point.x - half >= x && point.x + half <= x + side,
                    "{point:?}"
                );
                assert!(
                    point.y - half >= y && point.y + half <= y + side,
                    "{point:?}"
                );
            }
        }
    }

    #[test]
    fn stripes_run_from_the_upright_to_the_foot() {
        for stripe in STRIPES {
            let points = stripe_points(stripe);
            let (first, last) = (points[0], points[points.len() - 1]);

            assert_eq!(first, Point::new(HUB.x - stripe.0, stripe.1));
            assert_eq!(last, Point::new(stripe.2, HUB.y + stripe.0));
            // The bend ends directly below the hub, where the foot begins.
            let bend_end = points[points.len() - 2];
            assert!((bend_end.x - HUB.x).abs() < 0.01);
        }
    }

    #[test]
    fn partial_cuts_a_polyline_by_length() {
        let line = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];

        assert_eq!(partial(&line, 1.0), line.to_vec());
        assert_eq!(
            partial(&line, 0.75),
            vec![line[0], line[1], Point::new(10.0, 5.0)]
        );
        assert_eq!(partial(&line, 0.25), vec![line[0], Point::new(5.0, 0.0)]);
        assert_eq!(partial(&line, 0.0), vec![line[0]]);
        assert_eq!(partial(&line, 7.0), line.to_vec());
    }

    #[test]
    fn animation_draws_stripes_in_turn_then_fades_and_loops() {
        assert_eq!(animation(0.0, 0), (0.0, 1.0));
        // Later stripes start later.
        assert!(animation(0.3, 0).0 > animation(0.3, 2).0);
        // Everything is fully drawn before the fade starts.
        let hold = LOOP_SECONDS - FADE_SECONDS - 0.05;
        for index in 0..STRIPES.len() {
            assert_eq!(animation(hold, index), (1.0, 1.0));
        }
        assert!(animation(LOOP_SECONDS - 0.1, 0).1 < 1.0);
        assert_eq!(animation(LOOP_SECONDS + 0.3, 1), animation(0.3, 1));
    }
}
