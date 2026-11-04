//! Cœur de GeoLearner : données, reconnaissance des réponses et logique de quiz,
//! sans dépendance à l'interface graphique.

pub mod flag;
pub mod matcher;
pub mod normalize;
pub mod projection;
pub mod quiz;
pub mod world;

pub use flag::{Flag, Layer};
pub use matcher::Matcher;
pub use normalize::normalize;
pub use quiz::{Answer, Mark, Mode, Outcome, Quiz};
pub use world::{Country, Info, Inset, Lang, Region, RegionKind, Shape, Territory, Text, World};

#[cfg(test)]
mod testing {
    use crate::world::{Country, Shape, World};

    pub fn world_with(countries: &[(&str, &[&str])]) -> World {
        World {
            info: Default::default(),
            countries: countries
                .iter()
                .map(|(code, answers)| Country {
                    code: code.to_string(),
                    name: crate::Text::new(*code, *code),
                    answers: answers.iter().map(|a| a.to_string()).collect(),
                    spoken: vec![],
                    sub: 0,
                    label: [0.0, 0.0],
                    bubble: false,
                    one_bubble: false,
                    shape: Shape {
                        rings: vec![],
                        rings_lo: vec![],
                        main: vec![],
                    },
                })
                .collect(),
            territories: vec![],
            regions: vec![],
            lakes: vec![],
        }
    }
}
