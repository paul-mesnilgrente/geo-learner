//! Drapeaux des pays de l'atlas du monde, peints à partir de leurs polygones (`flags.bin`, généré
//! par `mapgen flags`) : nets à toutes les tailles, sans moteur SVG.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use geo_core::Flag;
use iced::widget::canvas::{self, Cache, Fill, Geometry, Path, Stroke, fill::Rule};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::style;

/// Drapeau prêt à peindre : couleurs et contours décodés, en unités du drapeau.
pub struct Painted {
    pub code: String,
    size: Size,
    layers: Vec<(Color, Vec<Vec<Point>>)>,
}

/// Drapeaux, dans l'ordre des pays de l'atlas du monde.
pub static FLAGS: LazyLock<Vec<Painted>> = LazyLock::new(|| {
    let flags = Flag::list_from_bytes(include_bytes!("../../../assets/flags.bin"))
        .expect("drapeaux invalides");
    flags
        .iter()
        .map(|flag| Painted {
            code: flag.code.clone(),
            size: Size::new(f32::from(flag.width), f32::from(flag.height)),
            layers: flag
                .layers
                .iter()
                .map(|layer| {
                    let [r, g, b, a] = layer.color;
                    let contours = layer
                        .points()
                        .map(|c| c.into_iter().map(|[x, y]| Point::new(x, y)).collect())
                        .collect();
                    (Color::from_rgba8(r, g, b, f32::from(a) / 255.0), contours)
                })
                .collect(),
        })
        .collect()
});

/// Drapeau d'un pays, d'après son code (ADM0_A3).
pub fn find(code: &str) -> Option<usize> {
    FLAGS.iter().position(|f| f.code == code)
}

thread_local! {
    /// Rendu de chaque drapeau, par taille : les drapeaux ne sont pas recalculés à chaque image
    /// des animations.
    static CACHES: RefCell<HashMap<(usize, u32, u32), Rc<Cache>>> = RefCell::new(HashMap::new());
}

/// Taille d'affichage du drapeau `index` dans un cadre `max` (pixels), à ses proportions.
pub fn fit(index: usize, max: Size) -> Size {
    let flag = &FLAGS[index];
    let ratio = flag.size.width / flag.size.height;
    let width = max.width.min(max.height * ratio);
    Size::new(width.round(), (width / ratio).round())
}

/// Drapeau `index` aussi grand que possible dans `max_width` × `max_height` pixels.
pub fn view<'a, M: 'a>(index: usize, max_width: f32, max_height: f32) -> Element<'a, M> {
    let size = fit(index, Size::new(max_width, max_height));
    let key = (index, size.width as u32, size.height as u32);
    let cache = CACHES.with(|caches| {
        caches
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| Rc::new(Cache::new()))
            .clone()
    });
    canvas::Canvas::new(FlagView {
        flag: &FLAGS[index],
        cache,
    })
    .width(size.width)
    .height(size.height)
    .into()
}

struct FlagView {
    flag: &'static Painted,
    cache: Rc<Cache>,
}

impl<M> canvas::Program<M> for FlagView {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let s =
                (bounds.width / self.flag.size.width).min(bounds.height / self.flag.size.height);
            for (color, contours) in &self.flag.layers {
                let path = Path::new(|b| {
                    for contour in contours {
                        let mut points = contour.iter().map(|p| Point::new(p.x * s, p.y * s));
                        if let Some(first) = points.next() {
                            b.move_to(first);
                            for p in points {
                                b.line_to(p);
                            }
                            b.close();
                        }
                    }
                });
                frame.fill(
                    &path,
                    Fill {
                        style: (*color).into(),
                        rule: Rule::EvenOdd,
                    },
                );
            }
            // Liseré : un drapeau clair (Japon) se détache du fond.
            frame.stroke(
                &Path::rectangle(Point::ORIGIN, bounds.size()),
                Stroke::default()
                    .with_color(style::with_alpha(Color::BLACK, 0.3))
                    .with_width(1.0),
            );
        });
        vec![geometry]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ATLASES;

    /// Chaque pays du monde a son drapeau, dans le même ordre que l'atlas.
    #[test]
    fn every_country_has_its_flag() {
        let world = &ATLASES[0];
        let codes: Vec<&str> = FLAGS.iter().map(|f| f.code.as_str()).collect();
        let expected: Vec<&str> = world.countries.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, expected);
        // Proportions officielles : 2:3 pour la France, 1:1 pour la Suisse, 28:11 pour le Qatar.
        let ratio = |code: &str| {
            let f = &FLAGS[find(code).unwrap()];
            f.size.width / f.size.height
        };
        assert!((ratio("FRA") - 1.5).abs() < 0.01);
        assert!((ratio("CHE") - 1.0).abs() < 0.01);
        assert!((ratio("QAT") - 28.0 / 11.0).abs() < 0.01);
    }
}
