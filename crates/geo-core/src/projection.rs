//! Projection « Natural Earth » (Šavrič et al., 2015), compromis esthétique pour les cartes du monde.

/// Ramène une longitude dans [-180, 180[ autour de `center`.
fn wrap(lon: f32, center: f32) -> f32 {
    (lon - center + 180.0).rem_euclid(360.0) - 180.0
}

/// Projette (lon, lat) en degrés vers le plan, l'axe y pointant vers le nord.
/// Le monde entier tient à peu près dans [-2.73, 2.73] × [-1.42, 1.42].
pub fn project(lon: f32, lat: f32, center: f32) -> [f32; 2] {
    project_raw(wrap(lon, center), lat)
}

fn project_raw(lon: f32, lat: f32) -> [f32; 2] {
    let l = lon.to_radians();
    let p = lat.to_radians();
    let p2 = p * p;
    let p4 = p2 * p2;
    let p6 = p4 * p2;
    let p8 = p4 * p4;
    let p10 = p8 * p2;
    let p12 = p10 * p2;
    let x = l * (0.870_700 - 0.131_979 * p2 - 0.013_791 * p4 + 0.003_971 * p10 - 0.001_529 * p12);
    let y = p * (1.007_226 + 0.015_085 * p2 - 0.044_475 * p6 + 0.028_874 * p8 - 0.005_916 * p10);
    [x, y]
}

/// Projette un anneau en évitant qu'il ne soit coupé en deux par l'antiméridien de la projection :
/// les longitudes sont « déroulées » pour rester continues, quitte à sortir de [-180, 180].
///
/// `anchor` (longitude du pays) place l'anneau du même côté que le reste du pays : les îles
/// de Kiribati restent groupées, même de part et d'autre de la coupure.
pub fn project_ring(ring: &[[f32; 2]], center: f32, anchor: f32, out: &mut Vec<[f32; 2]>) {
    out.clear();
    let mut prev = wrap(anchor, center);
    for &[lon, lat] in ring {
        let mut l = wrap(lon, center);
        while l - prev > 180.0 {
            l -= 360.0;
        }
        while prev - l > 180.0 {
            l += 360.0;
        }
        prev = l;
        out.push(project_raw(l, lat));
    }
}

/// Boîte englobante projetée d'un cadrage [lon_min, lat_min, lon_max, lat_max].
pub fn project_view(view: [f32; 4], center: f32) -> [f32; 4] {
    let [lon0, lat0, lon1, lat1] = view;
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let steps = 16;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let lon = lon0 + (lon1 - lon0) * t;
        let lat = lat0 + (lat1 - lat0) * t;
        // Le cadrage est exprimé autour de son propre milieu : pas de repliement.
        for [x, y] in [
            project_raw(lon - center, lat0),
            project_raw(lon - center, lat1),
            project_raw(lon0 - center, lat),
            project_raw(lon1 - center, lat),
        ] {
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_symmetry() {
        assert_eq!(project(0.0, 0.0, 0.0), [0.0, 0.0]);
        let [x1, y1] = project(30.0, 45.0, 0.0);
        let [x2, y2] = project(-30.0, -45.0, 0.0);
        assert!((x1 + x2).abs() < 1e-6 && (y1 + y2).abs() < 1e-6);
    }

    #[test]
    fn center_shifts_meridian() {
        let a = project(170.0, 0.0, 160.0);
        let b = project(10.0, 0.0, 0.0);
        assert!((a[0] - b[0]).abs() < 1e-5);
    }

    #[test]
    fn ring_across_antimeridian_stays_continuous() {
        // Un îlot à cheval sur 180° (Fidji), projeté avec un méridien central à 0.
        let ring = [
            [179.5, -17.0],
            [-179.5, -17.0],
            [-179.5, -16.0],
            [179.5, -16.0],
        ];
        let mut out = Vec::new();
        project_ring(&ring, 0.0, 179.0, &mut out);
        let width = out.iter().map(|p| p[0]).fold(f32::MIN, f32::max)
            - out.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(width < 0.1, "l'anneau a été déchiré : largeur {width}");
    }

    #[test]
    fn anchor_keeps_islands_with_their_country() {
        // Île à 157° O d'un pays ancré à 173° E (Kiribati), méridien central à 11° E :
        // elle passe à l'est de la coupure, à côté du reste du pays.
        let mut out = Vec::new();
        project_ring(&[[-157.0, 1.8]], 11.0, 173.0, &mut out);
        let east = project(173.0, 1.8, 11.0)[0];
        assert!(
            out[0][0] > east,
            "{} devrait être à l'est de {east}",
            out[0][0]
        );
        // Sans pays à l'est, Samoa (172° O) tombe aussi à l'est avec ce méridien central.
        assert!(project(-172.4, -13.6, 11.0)[0] > 2.5);
    }
}
