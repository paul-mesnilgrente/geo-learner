//! Petites icônes vectorielles dessinées au trait (grille de 24 unités).

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Back,
    Mic,
    Plus,
    Minus,
    Target,
    Flag,
    Skip,
    Play,
    Download,
    Refresh,
    Timer,
}

struct Program {
    icon: Icon,
    color: Color,
}

pub fn icon<'a, Message: 'a>(icon: Icon, size: f32, color: Color) -> Element<'a, Message> {
    canvas::Canvas::new(Program { icon, color })
        .width(size)
        .height(size)
        .into()
}

impl<Message> canvas::Program<Message> for Program {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let s = bounds.width.min(bounds.height) / 24.0;
        let p = |x: f32, y: f32| Point::new(x * s, y * s);
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width((1.9 * s).max(1.2))
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);

        let path = Path::new(|b| match self.icon {
            Icon::Back => {
                b.move_to(p(19.0, 12.0));
                b.line_to(p(5.0, 12.0));
                b.move_to(p(11.0, 6.0));
                b.line_to(p(5.0, 12.0));
                b.line_to(p(11.0, 18.0));
            }
            Icon::Mic => {
                b.rounded_rectangle(p(9.0, 3.0), Size::new(6.0 * s, 11.0 * s), (3.0 * s).into());
                b.move_to(p(5.5, 11.0));
                b.bezier_curve_to(p(5.5, 15.0), p(8.5, 17.5), p(12.0, 17.5));
                b.bezier_curve_to(p(15.5, 17.5), p(18.5, 15.0), p(18.5, 11.0));
                b.move_to(p(12.0, 17.5));
                b.line_to(p(12.0, 21.0));
            }
            Icon::Plus => {
                b.move_to(p(12.0, 5.0));
                b.line_to(p(12.0, 19.0));
                b.move_to(p(5.0, 12.0));
                b.line_to(p(19.0, 12.0));
            }
            Icon::Minus => {
                b.move_to(p(5.0, 12.0));
                b.line_to(p(19.0, 12.0));
            }
            Icon::Target => {
                b.circle(p(12.0, 12.0), 7.0 * s);
                b.circle(p(12.0, 12.0), 2.5 * s);
                for (a, c) in [
                    ((12.0, 2.0), (12.0, 5.0)),
                    ((12.0, 19.0), (12.0, 22.0)),
                    ((2.0, 12.0), (5.0, 12.0)),
                    ((19.0, 12.0), (22.0, 12.0)),
                ] {
                    b.move_to(p(a.0, a.1));
                    b.line_to(p(c.0, c.1));
                }
            }
            Icon::Flag => {
                b.move_to(p(5.0, 21.0));
                b.line_to(p(5.0, 4.0));
                b.bezier_curve_to(p(9.0, 2.0), p(12.0, 6.0), p(19.0, 4.0));
                b.line_to(p(19.0, 13.0));
                b.bezier_curve_to(p(12.0, 15.0), p(9.0, 11.0), p(5.0, 13.0));
            }
            Icon::Skip => {
                b.move_to(p(5.0, 5.0));
                b.line_to(p(14.0, 12.0));
                b.line_to(p(5.0, 19.0));
                b.close();
                b.move_to(p(19.0, 5.0));
                b.line_to(p(19.0, 19.0));
            }
            Icon::Play => {
                b.move_to(p(7.0, 4.5));
                b.line_to(p(19.0, 12.0));
                b.line_to(p(7.0, 19.5));
                b.close();
            }
            Icon::Download => {
                b.move_to(p(12.0, 3.0));
                b.line_to(p(12.0, 15.0));
                b.move_to(p(7.0, 10.0));
                b.line_to(p(12.0, 15.0));
                b.line_to(p(17.0, 10.0));
                b.move_to(p(4.0, 20.0));
                b.line_to(p(20.0, 20.0));
            }
            Icon::Refresh => {
                b.arc(canvas::path::Arc {
                    center: p(12.0, 12.0),
                    radius: 7.5 * s,
                    start_angle: iced::Radians(-0.6),
                    end_angle: iced::Radians(4.4),
                });
                b.move_to(p(19.5, 4.0));
                b.line_to(p(19.0, 8.6));
                b.line_to(p(14.6, 8.0));
            }
            Icon::Timer => {
                b.circle(p(12.0, 13.5), 7.5 * s);
                b.move_to(p(12.0, 13.5));
                b.line_to(p(12.0, 9.5));
                b.move_to(p(10.0, 3.0));
                b.line_to(p(14.0, 3.0));
                b.move_to(p(18.0, 6.5));
                b.line_to(p(19.5, 5.0));
            }
        });
        frame.stroke(&path, stroke);
        vec![frame.into_geometry()]
    }
}
