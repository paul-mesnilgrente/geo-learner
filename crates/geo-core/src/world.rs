//! Données géographiques embarquées dans l'application (générées par `mapgen`).

use serde::{Deserialize, Serialize};

/// Un anneau de polygone, en degrés (longitude, latitude).
pub type Ring = Vec<[f32; 2]>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    /// Anneaux détaillés (extérieurs et trous mélangés : remplissage pair-impair).
    pub rings: Vec<Ring>,
    /// Les mêmes anneaux simplifiés, pour les vues éloignées. Même ordre que `rings`.
    pub rings_lo: Vec<Ring>,
    /// Indique, pour chaque anneau, s'il fait partie du territoire principal
    /// (utilisé pour cadrer une zone sans tenir compte des territoires d'outre-mer).
    pub main: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    /// Code ADM0_A3 de Natural Earth.
    pub code: String,
    pub name: String,
    /// Noms acceptés, déjà normalisés (voir [`crate::normalize`]), nom principal compris.
    pub answers: Vec<String>,
    /// Formes orales (mots minuscules séparés par des espaces) proposées au module vocal.
    pub spoken: Vec<String>,
    /// Index de la sous-région dans [`World::regions`].
    pub sub: usize,
    /// Point d'ancrage de l'étiquette (lon, lat).
    pub label: [f32; 2],
    /// Archipel entouré d'une « bulle » sur la carte pour être repérable.
    pub bubble: bool,
    /// Une seule bulle pour toutes ses îles (Kiribati, Micronésie), même éloignées.
    pub one_bubble: bool,
    pub shape: Shape,
}

/// Territoire dépendant d'un pays du quiz (ou sans souveraineté reconnue).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Territory {
    pub name: String,
    /// Noms acceptés, normalisés : taper « Groenland » explique à qui il appartient.
    pub answers: Vec<String>,
    /// Pays souverain (index dans `countries`), s'il fait partie du quiz.
    pub owner: Option<usize>,
    pub label: [f32; 2],
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionKind {
    World,
    Continent,
    Sub,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub kind: RegionKind,
    /// Continent de rattachement (index dans `regions`) pour l'affichage du menu.
    pub parent: Option<usize>,
    /// Pays de la zone (index dans `countries`).
    pub members: Vec<usize>,
    /// Méridien central de la projection.
    pub center: f32,
    /// Cadrage [lon_min, lat_min, lon_max, lat_max].
    pub view: [f32; 4],
    /// Zone redondante avec son continent (non proposée dans le menu).
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    pub countries: Vec<Country>,
    /// Terres qui ne sont pas des pays du quiz (Groenland, Porto Rico, Sahara occidental…).
    pub territories: Vec<Territory>,
    pub regions: Vec<Region>,
}

impl World {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    pub fn region(&self, id: &str) -> Option<usize> {
        self.regions.iter().position(|r| r.id == id)
    }

    /// Continent auquel appartient un pays.
    pub fn continent_of(&self, country: usize) -> Option<usize> {
        self.regions[self.countries[country].sub].parent
    }
}
