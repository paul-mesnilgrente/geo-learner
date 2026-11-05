//! Données géographiques embarquées dans l'application (générées par `mapgen`) : un « atlas »
//! par quiz. Les lieux à trouver s'appellent `countries` quel que soit le quiz.

use serde::{Deserialize, Serialize};

/// Un anneau de polygone, en degrés (longitude, latitude).
pub type Ring = Vec<[f32; 2]>;

/// Langue de l'interface, des noms affichés et de la reconnaissance vocale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    /// Rang dans [`Lang::ALL`], et donc dans un [`Text`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Code ISO 639-1 (« fr », « en »).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// Langue d'un code de locale (« fr_FR.UTF-8 », « en-US »…), si elle est prise en charge.
    pub fn from_locale(locale: &str) -> Option<Lang> {
        let code = locale.get(..2)?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }
}

/// Texte dans chaque langue, dans l'ordre de [`Lang::ALL`]. Une traduction manquante (vide)
/// retombe sur le français.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Text(pub Vec<String>);

impl Text {
    pub fn new(fr: impl Into<String>, en: impl Into<String>) -> Self {
        Self(vec![fr.into(), en.into()])
    }

    pub fn get(&self, lang: Lang) -> &str {
        self.0
            .get(lang.index())
            .filter(|s| !s.is_empty())
            .or(self.0.first())
            .map_or("", String::as_str)
    }
}

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
    pub name: Text,
    /// Noms acceptés dans toutes les langues, déjà normalisés (voir [`crate::normalize`]),
    /// noms principaux compris.
    pub answers: Vec<String>,
    /// Formes orales (mots minuscules séparés par des espaces) proposées au module vocal,
    /// par langue (dans l'ordre de [`Lang::ALL`]).
    pub spoken: Vec<Vec<String>>,
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
    pub name: Text,
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
    pub name: Text,
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

/// Description d'un atlas (un quiz) : le monde, les départements français…
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Info {
    pub id: String,
    pub title: Text,
    /// Catégorie du catalogue (« Monde », « France »…).
    pub category: Text,
    /// Nom des lieux à trouver, au singulier et au pluriel (« pays », « départements »).
    pub one: Text,
    pub many: Text,
    /// Accord au féminin en français (« 13 régions trouvées »).
    pub feminine: bool,
    /// Mode Nommer : un lieu trop petit est montré dans sa sous-région plutôt que dans son
    /// continent (départements : leur région).
    pub zoom_sub: bool,
    /// Noms des deux niveaux de zones : continents et sous-régions pour le monde.
    pub levels: [Text; 2],
    /// Encarts : terres rapprochées de la carte principale (outre-mer, Alaska…).
    pub insets: Vec<Inset>,
    /// Quiz sans carte, sur les numéros des lieux (départements) : « Numéro → nom » et
    /// « Nom → numéro ».
    pub numbers: bool,
    /// Lieux d'eau (océans) : de grands polygones dessinés sous les terres, qui les recouvrent
    /// le long des côtes.
    pub water: bool,
    /// Lieux linéaires (fleuves) : leurs « anneaux » sont des tracés ouverts, dessinés par-dessus
    /// les terres.
    pub lines: bool,
    /// Quiz des drapeaux (dérivé de l'atlas du monde) : le drapeau d'un pays remplace son nom
    /// dans les questions.
    pub flags: bool,
    /// Lieux ponctuels (capitales) : une pastille à leur point d'étiquette, quel que soit le
    /// zoom ; leur contour n'est qu'un minuscule carré.
    pub points: bool,
    /// Eaux intérieures (lacs) : des polygones peints par-dessus les terres, en couleur d'eau.
    pub inland_water: bool,
}

/// Encart, en (lon, lat) : `[lon_min, lat_min, lon_max, lat_max]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Inset {
    /// Cadre de l'encart sur la carte.
    pub frame: [f32; 4],
    /// Emprise réelle de ces terres (leur plus grande île ou masse), pour les situer sur le
    /// globe.
    pub origin: [f32; 4],
    /// Déplacement inverse, de l'encart vers la position réelle :
    /// `réel = from + (encart - to) * factor`.
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub factor: [f32; 2],
}

impl Inset {
    /// Position réelle d'un point de l'encart.
    pub fn real(&self, [lon, lat]: [f32; 2]) -> [f32; 2] {
        [
            self.from[0] + (lon - self.to[0]) * self.factor[0],
            self.from[1] + (lat - self.to[1]) * self.factor[1],
        ]
    }

    /// Le point (en coordonnées de la carte) est-il dans le cadre de l'encart ?
    pub fn contains(&self, [lon, lat]: [f32; 2]) -> bool {
        let [x0, y0, x1, y1] = self.frame;
        (x0..=x1).contains(&lon) && (y0..=y1).contains(&lat)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    pub info: Info,
    pub countries: Vec<Country>,
    /// Terres qui ne sont pas des pays du quiz (Groenland, Porto Rico, Sahara occidental…).
    pub territories: Vec<Territory>,
    pub regions: Vec<Region>,
    /// Grands lacs, peints par-dessus les terres (atlas des fleuves).
    pub lakes: Vec<Shape>,
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
