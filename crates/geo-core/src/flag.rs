//! Drapeaux vectoriels (générés par `mapgen flags` à partir de SVG) : des calques de polygones
//! colorés, peints dans l'ordre. Les coordonnées, entières, sont stockées par différences d'un
//! point au suivant : petites, elles tiennent sur un ou deux octets.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flag {
    /// Code du pays (ADM0_A3), comme dans l'atlas du monde.
    pub code: String,
    /// Dimensions, dans l'unité des coordonnées (proportions officielles du drapeau).
    pub width: u16,
    pub height: u16,
    pub layers: Vec<Layer>,
}

/// Polygones d'une même couleur, remplis selon la règle pair-impair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Rouge, vert, bleu, opacité.
    pub color: [u8; 4],
    /// Contours codés par différences : x0, y0, x1 - x0, y1 - y0…
    pub contours: Vec<Vec<i16>>,
}

impl Layer {
    /// Calque d'une couleur à partir de contours en coordonnées entières.
    pub fn new(color: [u8; 4], contours: &[Vec<[i32; 2]>]) -> Self {
        let contours = contours
            .iter()
            .map(|contour| {
                let mut out = Vec::with_capacity(contour.len() * 2);
                let mut prev = [0, 0];
                for &[x, y] in contour {
                    out.push((x - prev[0]) as i16);
                    out.push((y - prev[1]) as i16);
                    prev = [x, y];
                }
                out
            })
            .collect();
        Self { color, contours }
    }

    /// Contours décodés, en coordonnées du drapeau (y vers le bas).
    pub fn points(&self) -> impl Iterator<Item = Vec<[f32; 2]>> + '_ {
        self.contours.iter().map(|deltas| {
            let mut prev = [0i32, 0i32];
            (0..deltas.len() / 2)
                .map(|i| {
                    let (dx, dy) = (deltas[2 * i], deltas[2 * i + 1]);
                    prev = [prev[0] + i32::from(dx), prev[1] + i32::from(dy)];
                    [prev[0] as f32, prev[1] as f32]
                })
                .collect()
        })
    }
}

impl Flag {
    pub fn list_from_bytes(bytes: &[u8]) -> Result<Vec<Flag>, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    pub fn list_to_bytes(flags: &[Flag]) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contours_round_trip() {
        let square = vec![[0, 0], [300, 0], [300, 200], [0, 200]];
        let layer = Layer::new([206, 17, 38, 255], std::slice::from_ref(&square));
        let back: Vec<Vec<[f32; 2]>> = layer.points().collect();
        assert_eq!(back.len(), 1);
        let expected: Vec<[f32; 2]> = square.iter().map(|&[x, y]| [x as f32, y as f32]).collect();
        assert_eq!(back[0], expected);
        let flags = vec![Flag {
            code: "JPN".into(),
            width: 300,
            height: 200,
            layers: vec![layer],
        }];
        let bytes = Flag::list_to_bytes(&flags).unwrap();
        assert_eq!(Flag::list_from_bytes(&bytes).unwrap(), flags);
    }
}
