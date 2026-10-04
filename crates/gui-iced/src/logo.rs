//! The Lorimer mark, drawn on a canvas so it stays crisp at any size. The geometry mirrors
//! `assets/logo/mark.svg`; keep the two in sync.

use crate::styles;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Theme};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Square region of the logo artwork that the mark occupies, as (x, y, side).
const ARTBOARD: (f32, f32, f32) = (225.0, 178.0, 630.0);
const HUB: Point = Point::new(548.0, 492.0);
const HUB_RADIUS: f32 = 40.0;
const STRIPE_WIDTH: f32 = 66.0;
const BEND_SEGMENTS_PER_TURN: f32 = 96.0;

/// One stripe of the mark, and how it behaves once the mark starts to spin.
struct Stripe {
    /// Radius of the bend around the hub.
    radius: f32,
    /// Top of the upright and right end of the foot, in artwork coordinates.
    top: f32,
    right: f32,
    /// Index into the chart palette.
    color: usize,
    /// Turns per second while spinning. Neighbours turn in opposite directions.
    turns_per_second: f32,
    /// How much of the circle the stripe covers while spinning, in radians.
    spin_sweep: f32,
}

const STRIPES: [Stripe; 3] = [
    Stripe {
        radius: 268.0,
        top: 262.0,
        right: 742.0,
        color: 0,
        turns_per_second: 0.22,
        spin_sweep: 4.4,
    },
    Stripe {
        radius: 188.0,
        top: 226.0,
        right: 800.0,
        color: 2,
        turns_per_second: -0.34,
        spin_sweep: 3.5,
    },
    Stripe {
        radius: 108.0,
        top: 300.0,
        right: 690.0,
        color: 3,
        turns_per_second: 0.5,
        spin_sweep: 2.7,
    },
];

// The scanning animation: the stripes draw themselves into the mark, pause, then the straight
// ends pull into the bends and the bends open into rings that keep turning around the hub.
const STRIPE_STAGGER: f32 = 0.16;
const DRAW_SECONDS: f32 = 1.0;
const SPIN_STARTS_AT: f32 = 1.6;
const MORPH_SECONDS: f32 = 0.9;
/// The straight ends pull in during the first part of the morph, before the bends open far
/// enough to swing them outside the mark's bounds.
const RETRACT_SECONDS: f32 = 0.4;
/// Each ring slowly grows and shrinks a little so the rings never look frozen relative to
/// each other.
const BREATHING: f32 = 0.35;

/// How a stripe is drawn at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    /// Fraction of the stripe's length that is drawn, from the top of the upright.
    drawn: f32,
    /// Length of the straight ends, from `1.0` (the mark) to `0.0` (a plain arc).
    straight: f32,
    /// Rotation of the whole stripe around the hub, in radians.
    rotation: f32,
    /// Angle the bend covers, in radians. A quarter turn in the mark.
    sweep: f32,
}

impl Pose {
    /// The stripe as it appears in the logo.
    const MARK: Self = Self {
        drawn: 1.0,
        straight: 1.0,
        rotation: 0.0,
        sweep: FRAC_PI_2,
    };
}

/// The mark at `size` pixels. With `time` (seconds since the animation began) the stripes draw
/// themselves and then spin as rings, which serves as the activity indicator; without it the
/// mark is static.
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
            let pose = match self.time {
                Some(time) => pose_at(time, index),
                None => Pose::MARK,
            };
            let points = partial(&stripe_points(stripe, pose), pose.drawn);
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
                    .with_color(styles::root_color(stripe.color))
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

/// Centre line of a stripe in artwork coordinates: along the upright, around the bend, and out
/// along the foot. The straight ends stay tangent to the bend however the stripe is rotated.
fn stripe_points(stripe: &Stripe, pose: Pose) -> Vec<Point> {
    let on_bend = |angle: f32| {
        Point::new(
            HUB.x + stripe.radius * angle.cos(),
            HUB.y + stripe.radius * angle.sin(),
        )
    };
    // Direction of travel along the bend at `angle`. The bend runs from the hub's left
    // (angle π) towards its bottom (angle π/2), so angles decrease along the stripe.
    let heading = |angle: f32| (angle.sin(), -angle.cos());

    let start_angle = PI + pose.rotation;
    let end_angle = start_angle - pose.sweep;
    let upright = (HUB.y - stripe.top) * pose.straight;
    let foot = (stripe.right - HUB.x) * pose.straight;

    let start = on_bend(start_angle);
    let (dx, dy) = heading(start_angle);
    let mut points = vec![Point::new(start.x - dx * upright, start.y - dy * upright)];

    let segments = ((pose.sweep / TAU) * BEND_SEGMENTS_PER_TURN)
        .ceil()
        .max(1.0) as usize;
    for step in 0..=segments {
        points.push(on_bend(
            start_angle - pose.sweep * step as f32 / segments as f32,
        ));
    }

    let end = on_bend(end_angle);
    let (dx, dy) = heading(end_angle);
    points.push(Point::new(end.x + dx * foot, end.y + dy * foot));
    points
}

/// The first `fraction` (by length) of the polyline `points`.
fn partial(points: &[Point], fraction: f32) -> Vec<Point> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction >= 1.0 {
        return points.to_vec();
    }

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

/// The pose of stripe `index` at `time` seconds into the scanning animation.
fn pose_at(time: f32, index: usize) -> Pose {
    let stripe = &STRIPES[index];
    let time = time.max(0.0);

    if time < SPIN_STARTS_AT {
        let progress = ((time - index as f32 * STRIPE_STAGGER) / DRAW_SECONDS).clamp(0.0, 1.0);
        return Pose {
            drawn: 1.0 - (1.0 - progress).powi(3),
            ..Pose::MARK
        };
    }

    let spinning = time - SPIN_STARTS_AT;
    let retracted = smoothstep(spinning / RETRACT_SECONDS);
    let opened =
        smoothstep((spinning - RETRACT_SECONDS * 0.5) / (MORPH_SECONDS - RETRACT_SECONDS * 0.5));
    // The rings speed up from rest over the morph, then turn at a constant rate. This is the
    // integral of a speed that ramps linearly to one, so the angle never jumps.
    let turned = if spinning < MORPH_SECONDS {
        spinning * spinning / (2.0 * MORPH_SECONDS)
    } else {
        spinning - MORPH_SECONDS / 2.0
    };
    let breathing = BREATHING * (spinning * 0.9 + index as f32 * 2.1).sin();

    Pose {
        drawn: 1.0,
        straight: 1.0 - retracted,
        rotation: stripe.turns_per_second * TAU * turned,
        sweep: FRAC_PI_2 + (stripe.spin_sweep + breathing - FRAC_PI_2) * opened,
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(left: Point, right: Point) {
        assert!(left.distance(right) < 0.01, "{left:?} != {right:?}");
    }

    fn assert_inside_artboard(points: &[Point], context: &str) {
        let (x, y, side) = ARTBOARD;
        let half = STRIPE_WIDTH / 2.0;

        for point in points {
            assert!(
                point.x - half >= x
                    && point.x + half <= x + side
                    && point.y - half >= y
                    && point.y + half <= y + side,
                "{context}: {point:?} leaves the artboard"
            );
        }
    }

    #[test]
    fn mark_stripes_run_from_the_upright_to_the_foot() {
        for stripe in &STRIPES {
            let points = stripe_points(stripe, Pose::MARK);

            assert_close(points[0], Point::new(HUB.x - stripe.radius, stripe.top));
            assert_close(
                points[points.len() - 1],
                Point::new(stripe.right, HUB.y + stripe.radius),
            );
            // The bend ends directly below the hub, where the foot begins.
            assert_close(
                points[points.len() - 2],
                Point::new(HUB.x, HUB.y + stripe.radius),
            );
        }
    }

    #[test]
    fn stripes_stay_inside_the_artboard_throughout_the_animation() {
        for stripe in &STRIPES {
            assert_inside_artboard(&stripe_points(stripe, Pose::MARK), "mark");
        }

        for step in 0..600 {
            let time = step as f32 * 0.05;
            for (index, stripe) in STRIPES.iter().enumerate() {
                let points = stripe_points(stripe, pose_at(time, index));
                assert_inside_artboard(&points, &format!("stripe {index} at {time} s"));
            }
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
    fn animation_first_draws_the_mark_stripe_by_stripe() {
        assert_eq!(pose_at(0.0, 0).drawn, 0.0);
        // Later stripes start later.
        assert!(pose_at(0.3, 0).drawn > pose_at(0.3, 2).drawn);

        // Just before the spin starts, every stripe is the finished mark.
        for index in 0..STRIPES.len() {
            assert_eq!(pose_at(SPIN_STARTS_AT - 0.01, index), Pose::MARK);
        }
    }

    #[test]
    fn spin_begins_from_the_mark_without_a_jump() {
        for index in 0..STRIPES.len() {
            let pose = pose_at(SPIN_STARTS_AT, index);

            assert_eq!(pose.drawn, 1.0);
            assert!((pose.straight - 1.0).abs() < 1e-6);
            assert!(pose.rotation.abs() < 1e-6);
            assert!((pose.sweep - FRAC_PI_2).abs() < 1e-6);
        }
    }

    #[test]
    fn stripes_become_rings_that_keep_turning() {
        let settled = SPIN_STARTS_AT + MORPH_SECONDS + 0.5;

        for (index, stripe) in STRIPES.iter().enumerate() {
            let earlier = pose_at(settled, index);
            let later = pose_at(settled + 1.0, index);

            assert_eq!(earlier.straight, 0.0);
            assert!(earlier.sweep > FRAC_PI_2 && earlier.sweep < TAU);
            // One second later the ring has turned by exactly its rate, in its own direction.
            let turned = later.rotation - earlier.rotation;
            assert!((turned - stripe.turns_per_second * TAU).abs() < 1e-3);
        }

        // Neighbouring rings turn in opposite directions.
        assert!(STRIPES[0].turns_per_second * STRIPES[1].turns_per_second < 0.0);
        assert!(STRIPES[1].turns_per_second * STRIPES[2].turns_per_second < 0.0);
    }

    #[test]
    fn the_animation_never_resets() {
        // Long after the start, the rings are still rings: nothing loops back to the drawing
        // phase.
        for index in 0..STRIPES.len() {
            let pose = pose_at(600.0, index);
            assert_eq!(pose.drawn, 1.0);
            assert_eq!(pose.straight, 0.0);
        }
    }
}
