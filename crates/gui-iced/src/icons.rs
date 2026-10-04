//! Small line icons drawn on a canvas, so the app needs no icon font or image assets.

use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme};
use std::f32::consts::PI;

/// Icons are designed on a square grid of this many units.
const GRID: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    Refresh,
    Folder,
    File,
    Home,
    Download,
    Desktop,
    Apps,
    Disk,
    Close,
    Plus,
}

pub fn icon<'a, Message: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, Message> {
    Canvas::new(IconProgram { icon, color })
        .width(size)
        .height(size)
        .into()
}

struct IconProgram {
    icon: Icon,
    color: Color,
}

impl<Message> canvas::Program<Message> for IconProgram {
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
        let scale = bounds.width.min(bounds.height) / GRID;
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width(1.5 * scale.max(0.75))
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let at = |x: f32, y: f32| Point::new(x * scale, y * scale);
        let line = |points: &[(f32, f32)], closed: bool| {
            Path::new(|builder| {
                if let Some(((x, y), rest)) = points.split_first() {
                    builder.move_to(at(*x, *y));
                    for (x, y) in rest {
                        builder.line_to(at(*x, *y));
                    }
                    if closed {
                        builder.close();
                    }
                }
            })
        };

        match self.icon {
            Icon::ChevronLeft => {
                frame.stroke(
                    &line(&[(10.0, 3.5), (5.5, 8.0), (10.0, 12.5)], false),
                    stroke,
                );
            }
            Icon::ChevronRight => {
                frame.stroke(
                    &line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)], false),
                    stroke,
                );
            }
            Icon::Refresh => {
                let arc: Vec<(f32, f32)> = (0..=28)
                    .map(|step| {
                        let angle = -0.25 * PI + 1.6 * PI * step as f32 / 28.0;
                        (8.0 + 5.25 * angle.cos(), 8.0 + 5.25 * angle.sin())
                    })
                    .collect();
                frame.stroke(&line(&arc, false), stroke);
                frame.stroke(
                    &line(&[(8.6, 1.6), (12.2, 4.3), (12.6, 0.4)], false),
                    stroke,
                );
            }
            Icon::Folder => {
                frame.stroke(
                    &line(
                        &[
                            (2.0, 3.75),
                            (6.2, 3.75),
                            (7.7, 5.5),
                            (14.0, 5.5),
                            (14.0, 12.75),
                            (2.0, 12.75),
                        ],
                        true,
                    ),
                    stroke,
                );
            }
            Icon::File => {
                frame.stroke(
                    &line(
                        &[
                            (4.0, 2.0),
                            (9.25, 2.0),
                            (12.5, 5.25),
                            (12.5, 14.0),
                            (4.0, 14.0),
                        ],
                        true,
                    ),
                    stroke,
                );
                frame.stroke(
                    &line(&[(9.25, 2.0), (9.25, 5.25), (12.5, 5.25)], false),
                    stroke,
                );
            }
            Icon::Home => {
                frame.stroke(
                    &line(&[(1.75, 8.0), (8.0, 2.5), (14.25, 8.0)], false),
                    stroke,
                );
                frame.stroke(
                    &line(
                        &[(3.75, 7.0), (3.75, 13.5), (12.25, 13.5), (12.25, 7.0)],
                        false,
                    ),
                    stroke,
                );
            }
            Icon::Download => {
                frame.stroke(&line(&[(8.0, 2.25), (8.0, 10.5)], false), stroke);
                frame.stroke(
                    &line(&[(4.75, 7.5), (8.0, 10.75), (11.25, 7.5)], false),
                    stroke,
                );
                frame.stroke(&line(&[(3.0, 13.75), (13.0, 13.75)], false), stroke);
            }
            Icon::Desktop => {
                frame.stroke(
                    &Path::rounded_rectangle(
                        at(1.75, 2.75),
                        Size::new(12.5 * scale, 8.5 * scale),
                        (1.5 * scale).into(),
                    ),
                    stroke,
                );
                frame.stroke(&line(&[(8.0, 11.25), (8.0, 13.75)], false), stroke);
                frame.stroke(&line(&[(5.5, 13.75), (10.5, 13.75)], false), stroke);
            }
            Icon::Apps => {
                for (x, y) in [(2.5, 2.5), (9.25, 2.5), (2.5, 9.25), (9.25, 9.25)] {
                    frame.stroke(
                        &Path::rounded_rectangle(
                            at(x, y),
                            Size::new(4.25 * scale, 4.25 * scale),
                            (1.2 * scale).into(),
                        ),
                        stroke,
                    );
                }
            }
            Icon::Disk => {
                frame.stroke(
                    &Path::rounded_rectangle(
                        at(1.75, 3.5),
                        Size::new(12.5 * scale, 9.0 * scale),
                        (2.0 * scale).into(),
                    ),
                    stroke,
                );
                frame.stroke(&line(&[(1.75, 9.25), (14.25, 9.25)], false), stroke);
                frame.fill(&Path::circle(at(11.6, 10.9), 0.75 * scale), self.color);
            }
            Icon::Close => {
                frame.stroke(&line(&[(4.25, 4.25), (11.75, 11.75)], false), stroke);
                frame.stroke(&line(&[(11.75, 4.25), (4.25, 11.75)], false), stroke);
            }
            Icon::Plus => {
                frame.stroke(&line(&[(8.0, 3.25), (8.0, 12.75)], false), stroke);
                frame.stroke(&line(&[(3.25, 8.0), (12.75, 8.0)], false), stroke);
            }
        }

        vec![frame.into_geometry()]
    }
}
