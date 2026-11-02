//! Courbe d'évolution d'une série de parties (score ou durée), avec info-bulle au survol.

use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::widget::text::Alignment;
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme};

use crate::style;

/// Unité de l'axe vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Valeurs entre 0 et 1, affichées en pourcentage.
    Percent,
    /// Durées en secondes.
    Seconds,
}

pub struct Chart {
    pub unit: Unit,
    /// Une valeur par partie, de la plus ancienne à la plus récente.
    pub values: Vec<f32>,
    /// Étiquette courte de chaque partie sous l'axe (« 2 nov. »).
    pub ticks: Vec<String>,
    /// Date complète de chaque partie, pour l'info-bulle.
    pub details: Vec<String>,
}

#[derive(Default)]
pub struct Hover {
    index: Option<usize>,
}

const LEFT: f32 = 52.0;
const RIGHT: f32 = 16.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 28.0;

impl Chart {
    /// Bornes et graduations de l'axe vertical.
    fn scale(&self) -> (f32, Vec<f32>) {
        match self.unit {
            Unit::Percent => (1.0, vec![0.0, 0.25, 0.5, 0.75, 1.0]),
            Unit::Seconds => {
                let max = self.values.iter().copied().fold(0.0, f32::max).max(1.0);
                let step = [
                    10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0,
                ]
                .into_iter()
                .find(|s| max / s <= 4.0)
                .unwrap_or(7200.0);
                let top = (max / step).ceil() * step;
                let ticks = (0..=(top / step) as usize)
                    .map(|i| i as f32 * step)
                    .collect();
                (top, ticks)
            }
        }
    }

    fn format(&self, value: f32) -> String {
        match self.unit {
            Unit::Percent => format!("{} %", (value * 100.0).round() as i32),
            Unit::Seconds => {
                let s = value.round() as u32;
                if s >= 3600 {
                    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
                } else {
                    format!("{:02}:{:02}", s / 60, s % 60)
                }
            }
        }
    }

    fn axis_label(&self, value: f32) -> String {
        match self.unit {
            Unit::Percent => self.format(value),
            Unit::Seconds => {
                let s = value.round() as u32;
                match (s / 3600, s / 60 % 60, s % 60) {
                    (0, 0, s) => format!("{s} s"),
                    (0, m, 0) => format!("{m} min"),
                    (0, m, s) => format!("{m}:{s:02}"),
                    (h, 0, _) => format!("{h} h"),
                    (h, m, _) => format!("{h} h {m:02}"),
                }
            }
        }
    }

    fn plot(bounds: Size) -> Rectangle {
        Rectangle::new(
            Point::new(LEFT, TOP),
            Size::new(
                (bounds.width - LEFT - RIGHT).max(1.0),
                (bounds.height - TOP - BOTTOM).max(1.0),
            ),
        )
    }

    fn x(&self, plot: &Rectangle, i: usize) -> f32 {
        match self.values.len() {
            0 | 1 => plot.center_x(),
            n => plot.x + plot.width * i as f32 / (n - 1) as f32,
        }
    }

    fn point(&self, plot: &Rectangle, top: f32, i: usize) -> Point {
        let v = (self.values[i] / top).clamp(0.0, 1.0);
        Point::new(self.x(plot, i), plot.y + plot.height * (1.0 - v))
    }

    /// Partie la plus proche du curseur, horizontalement.
    fn nearest(&self, plot: &Rectangle, cursor: Point) -> Option<usize> {
        if self.values.is_empty() || cursor.y < 0.0 {
            return None;
        }
        (0..self.values.len()).min_by(|&a, &b| {
            (self.x(plot, a) - cursor.x)
                .abs()
                .total_cmp(&(self.x(plot, b) - cursor.x).abs())
        })
    }
}

fn label(content: String, position: Point, color: Color, align_x: Alignment) -> canvas::Text {
    canvas::Text {
        content,
        position,
        color,
        size: Pixels(11.0),
        font: style::FONT,
        align_x,
        align_y: Vertical::Center,
        ..canvas::Text::default()
    }
}

impl<Message> canvas::Program<Message> for Chart {
    type State = Hover;

    fn update(
        &self,
        state: &mut Hover,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) = event
        else {
            return None;
        };
        let plot = Self::plot(bounds.size());
        let index = cursor
            .position_in(bounds)
            .and_then(|p| self.nearest(&plot, p));
        if index == state.index {
            return None;
        }
        state.index = index;
        Some(Action::request_redraw())
    }

    fn draw(
        &self,
        state: &Hover,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let plot = Self::plot(bounds.size());
        let (top, ticks) = self.scale();

        // Grille et graduations, discrètes.
        for &tick in &ticks {
            let y = plot.y + plot.height * (1.0 - tick / top);
            frame.stroke(
                &Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y)),
                Stroke::default()
                    .with_color(style::with_alpha(
                        style::BORDER,
                        if tick == 0.0 { 1.0 } else { 0.6 },
                    ))
                    .with_width(1.0),
            );
            frame.fill_text(label(
                self.axis_label(tick),
                Point::new(plot.x - 10.0, y),
                style::MUTED,
                Alignment::Right,
            ));
        }

        let n = self.values.len();
        if n == 0 {
            return vec![frame.into_geometry()];
        }

        // Dates sous l'axe : une étiquette toutes les ~90 px, sans chevauchement ni répétition
        // du même jour. On part de la plus récente, qui est toujours affichée.
        let slots = ((plot.width / 90.0) as usize).clamp(1, n.max(2) - 1);
        let align = |i: usize| match i {
            _ if n == 1 => Alignment::Center,
            0 => Alignment::Left,
            i if i == n - 1 => Alignment::Right,
            _ => Alignment::Center,
        };
        // Centre approximatif de l'étiquette, selon son alignement.
        let center = |i: usize| {
            self.x(&plot, i)
                + match align(i) {
                    Alignment::Left => 22.0,
                    Alignment::Right => -22.0,
                    _ => 0.0,
                }
        };
        let mut shown: Vec<usize> = Vec::new();
        for k in (0..=slots).rev() {
            let i = if n == 1 { 0 } else { k * (n - 1) / slots };
            let fits = shown
                .last()
                .is_none_or(|&j| center(j) - center(i) >= 64.0 && self.ticks[j] != self.ticks[i]);
            if fits {
                shown.push(i);
            }
        }
        for i in shown {
            frame.fill_text(label(
                self.ticks[i].clone(),
                Point::new(self.x(&plot, i), plot.y + plot.height + 16.0),
                style::MUTED,
                align(i),
            ));
        }

        // Aire légère sous la courbe, puis la courbe.
        let points: Vec<Point> = (0..n).map(|i| self.point(&plot, top, i)).collect();
        if n > 1 {
            let area = Path::new(|b| {
                b.move_to(Point::new(points[0].x, plot.y + plot.height));
                for &p in &points {
                    b.line_to(p);
                }
                b.line_to(Point::new(points[n - 1].x, plot.y + plot.height));
                b.close();
            });
            frame.fill(&area, style::with_alpha(style::ACCENT, 0.07));
            let line = Path::new(|b| {
                b.move_to(points[0]);
                for &p in &points[1..] {
                    b.line_to(p);
                }
            });
            frame.stroke(
                &line,
                Stroke::default()
                    .with_color(style::ACCENT)
                    .with_width(2.0)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
        }

        // Points : tous s'ils sont assez espacés, sinon seulement le dernier.
        let marker = |frame: &mut Frame, p: Point, radius: f32| {
            frame.fill(&Path::circle(p, radius + 2.0), style::SURFACE);
            frame.fill(&Path::circle(p, radius), style::ACCENT);
        };
        let spaced = n == 1 || plot.width / (n - 1) as f32 >= 12.0;
        for (i, &p) in points.iter().enumerate() {
            if spaced || i == n - 1 {
                marker(&mut frame, p, 4.0);
            }
        }

        // Survol : repère vertical, point agrandi et info-bulle.
        if let Some(i) = state.index.filter(|&i| i < n) {
            let p = points[i];
            frame.stroke(
                &Path::line(
                    Point::new(p.x, plot.y),
                    Point::new(p.x, plot.y + plot.height),
                ),
                Stroke::default()
                    .with_color(style::with_alpha(style::MUTED, 0.5))
                    .with_width(1.0),
            );
            marker(&mut frame, p, 5.5);

            let value = self.format(self.values[i]);
            let detail = &self.details[i];
            let width = 24.0 + 6.8 * (detail.chars().count().max(value.chars().count()) as f32);
            let height = 44.0;
            let mut x = p.x + 12.0;
            if x + width > bounds.width - 4.0 {
                x = p.x - 12.0 - width;
            }
            let y = (p.y - height / 2.0).clamp(2.0, bounds.height - height - 2.0);
            let tip =
                Path::rounded_rectangle(Point::new(x, y), Size::new(width, height), 8.0.into());
            frame.fill(&tip, style::SURFACE_3);
            frame.stroke(
                &tip,
                Stroke::default().with_color(style::BORDER).with_width(1.0),
            );
            frame.fill_text(canvas::Text {
                size: Pixels(14.0),
                font: style::FONT_SEMIBOLD,
                ..label(
                    value,
                    Point::new(x + 12.0, y + 15.0),
                    style::TEXT,
                    Alignment::Left,
                )
            });
            frame.fill_text(label(
                detail.clone(),
                Point::new(x + 12.0, y + 31.0),
                style::MUTED,
                Alignment::Left,
            ));
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Hover,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.index.is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(unit: Unit, values: &[f32]) -> Chart {
        Chart {
            unit,
            values: values.to_vec(),
            ticks: vec![String::new(); values.len()],
            details: vec![String::new(); values.len()],
        }
    }

    #[test]
    fn time_axis_uses_round_steps() {
        let (top, ticks) = chart(Unit::Seconds, &[400.0, 610.0]).scale();
        assert_eq!(top, 900.0);
        assert_eq!(ticks, [0.0, 300.0, 600.0, 900.0]);
        let c = chart(Unit::Seconds, &[]);
        assert_eq!(c.axis_label(300.0), "5 min");
        assert_eq!(c.axis_label(90.0), "1:30");
        assert_eq!(c.axis_label(5400.0), "1 h 30");
        assert_eq!(c.axis_label(45.0), "45 s");
    }
}
