use convection_types::{BBox, Distance, Globe};
use glam::DVec3;

use crate::tile::{Tile, TilingScheme};

/// a tile projected onto the globe
#[derive(Clone, Copy, Debug)]
pub struct TilePatch {
    bounds: BBox,
    center: DVec3,
    radius: f64,
    extent: Distance,
}

impl TilePatch {
    pub fn new(scheme: &impl TilingScheme, tile: Tile, globe: &Globe) -> Self {
        let bounds = scheme.bounds(tile);
        let (center, radius) = bounding_sphere(globe, bounds);
        Self {
            bounds,
            center,
            radius,
            extent: ground_extent(globe, bounds),
        }
    }

    pub fn bounds(&self) -> BBox {
        self.bounds
    }

    /// center of the bounding sphere
    pub fn center(&self) -> DVec3 {
        self.center
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }

    /// length of the longest edge
    pub fn extent(&self) -> Distance {
        self.extent
    }

    /// distance from `eye` to the closest point of the tile.
    ///
    /// useful for calculating the required LOD
    pub fn distance_from(&self, globe: &Globe, eye: DVec3) -> f64 {
        (self.nearest_point_to(globe, eye) - eye).length()
    }

    /// whether any part of the tile faces `eye`.
    pub fn is_visible_from(&self, globe: &Globe, eye: DVec3) -> bool {
        if eye.length() <= globe.radius.as_meters() {
            return true;
        }
        // the nearest point is the one with the smallest angle to the eye, so
        // if it is beyond the horizon the whole tile is
        globe.is_above_horizon(self.nearest_point_to(globe, eye), eye)
    }

    /// the point of the tile closest to `eye`, which for an eye outside
    /// the globe is the point at the smallest angle to it
    pub fn nearest_point_to(&self, globe: &Globe, eye: DVec3) -> DVec3 {
        let (eye_lon, eye_lat) = to_lonlat(eye);
        // the angle to the eye shrinks as the longitude gap does
        let delta_lon = lon_offset(self.bounds, eye_lon);
        let cos_delta = delta_lon.to_radians().cos();
        let eye_lat = eye_lat.to_radians();

        let cos_angle = |lat_deg: &f64| {
            let lat = lat_deg.to_radians();
            eye_lat.sin() * lat.sin() + eye_lat.cos() * lat.cos() * cos_delta
        };

        // a bunch of trigonometry I pretend to understand
        let peak = eye_lat.sin().atan2(eye_lat.cos() * cos_delta).to_degrees();
        let lat = [
            self.bounds.south(),
            self.bounds.north(),
            peak.clamp(self.bounds.south(), self.bounds.north()),
        ]
        .into_iter()
        .max_by(|a, b| cos_angle(a).total_cmp(&cos_angle(b)))
        .unwrap_or(self.bounds.south());

        globe.lonlat_to_point(eye_lon + delta_lon, lat)
    }
}

/// the deepest zoom a scheme can express before its matrix dimensions
/// overflow
const MAX_REPRESENTABLE_ZOOM: u32 = 30;

/// the shallowest zoom whose tiles hold at most `max_m_per_px` ground meters
/// per texel.
///
/// sources are usually limited in how coarse a request they will serve, and a
/// tile's resolution only depends on its zoom and the scheme's tile size, so
/// that limit translates directly into a shallowest usable zoom. measured on
/// an equatorial tile, the largest of its level on either scheme
pub fn zoom_for_resolution(scheme: &impl TilingScheme, globe: &Globe, max_m_per_px: f64) -> u32 {
    let tile_px = scheme.tile_size().max(1) as f64;
    (0..=MAX_REPRESENTABLE_ZOOM)
        .find(|zoom| {
            let tile = scheme.tile_for_lonlat(0.0, 0.0, *zoom);
            let extent = TilePatch::new(scheme, tile, globe).extent().as_meters();
            extent / tile_px <= max_m_per_px
        })
        .unwrap_or(MAX_REPRESENTABLE_ZOOM)
}

/// (lon, lat) in degrees of a direction, the inverse of
/// [`Globe::lonlat_to_point`], whose longitude grows towards -Z
fn to_lonlat(point: DVec3) -> (f64, f64) {
    let length = point.length();
    if length == 0.0 {
        return (0.0, 0.0);
    }
    (
        (-point.z).atan2(point.x).to_degrees(),
        (point.y / length).clamp(-1.0, 1.0).asin().to_degrees(),
    )
}

/// how far (in degrees) `lon` has to move to reach the box's longitude span.
///
/// zero when `lon` already is inside the `bounds`
fn lon_offset(bounds: BBox, lon: f64) -> f64 {
    let span = bounds.east() - bounds.west();
    if (lon - bounds.west()).rem_euclid(360.0) <= span {
        return 0.0;
    }
    let to_west = wrap_lon(bounds.west() - lon);
    let to_east = wrap_lon(bounds.east() - lon);
    if to_west.abs() <= to_east.abs() {
        to_west
    } else {
        to_east
    }
}

/// normalize a longitude difference into [-180, 180)
fn wrap_lon(delta: f64) -> f64 {
    (delta + 180.0).rem_euclid(360.0) - 180.0
}

fn bounding_sphere(globe: &Globe, bounds: BBox) -> (DVec3, f64) {
    let radius = globe.radius.as_meters();
    let (center_lon, center_lat) = bounds.center();
    let axis = globe.normal_at(center_lon, center_lat);

    // no point of a lon/lat box is further from its center than its corners
    let cos_half_angle = [
        (bounds.west(), bounds.south()),
        (bounds.west(), bounds.north()),
        (bounds.east(), bounds.south()),
        (bounds.east(), bounds.north()),
    ]
    .into_iter()
    .map(|(lon, lat)| axis.dot(globe.normal_at(lon, lat)))
    .fold(f64::INFINITY, f64::min);

    if cos_half_angle <= 0.0 {
        return (DVec3::ZERO, radius);
    }

    let half_angle = cos_half_angle.clamp(-1.0, 1.0).acos();
    (axis * radius * cos_half_angle, radius * half_angle.sin())
}

/// the latitude closest to the equator (i.e. widest)
fn widest_lat(bounds: BBox) -> f64 {
    if bounds.south() > 0.0 {
        bounds.south()
    } else if bounds.north() < 0.0 {
        bounds.north()
    } else {
        0.0
    }
}

fn ground_extent(globe: &Globe, bounds: BBox) -> Distance {
    let radius = globe.radius.as_meters();
    let horizontal = radius
        * (bounds.east() - bounds.west()).to_radians().abs()
        * widest_lat(bounds).to_radians().cos();
    let vertical = radius * (bounds.north() - bounds.south()).to_radians().abs();

    Distance::meters(horizontal.max(vertical))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{WebMercatorScheme, Wgs84Scheme};
    use convection_types::EARTH_CIRCUMFERENCE;

    fn sample_tiles(scheme: &impl TilingScheme, max_zoom: u32) -> Vec<Tile> {
        let mut tiles = Vec::new();
        for zoom in 0..=max_zoom {
            let (cols, rows) = scheme.matrix_dims(zoom);
            let step = (cols / 8).max(1);
            for y in (0..rows).step_by(step as usize) {
                for x in (0..cols).step_by(step as usize) {
                    tiles.push(Tile::new(zoom, x, y));
                }
            }
        }
        tiles
    }

    #[test]
    fn root_tile_spans_the_globe() {
        let scheme = WebMercatorScheme::default();
        let patch = TilePatch::new(&scheme, Tile::new(0, 0, 0), &Globe::earth());

        let extent = patch.extent().as_meters();
        assert!(
            (extent - EARTH_CIRCUMFERENCE.as_meters()).abs() < 1.0,
            "extent {extent}"
        );
    }

    #[test]
    fn extent_halves_per_zoom_level() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let z1 = TilePatch::new(&scheme, Tile::new(1, 0, 0), &globe)
            .extent()
            .as_meters();
        let z2 = TilePatch::new(&scheme, Tile::new(2, 0, 1), &globe)
            .extent()
            .as_meters();
        assert!((z1 / z2 - 2.0).abs() < 0.01, "{z1} vs {z2}");
    }

    #[test]
    fn bounding_sphere_contains_the_tile() {
        let globe = Globe::earth();
        let mercator = WebMercatorScheme::default();
        let wgs84 = Wgs84Scheme::default();
        let tiles = sample_tiles(&mercator, 6)
            .into_iter()
            .map(|tile| {
                (
                    mercator.bounds(tile),
                    TilePatch::new(&mercator, tile, &globe),
                )
            })
            .chain(
                sample_tiles(&wgs84, 6)
                    .into_iter()
                    .map(|tile| (wgs84.bounds(tile), TilePatch::new(&wgs84, tile, &globe))),
            );

        for (bounds, patch) in tiles {
            for j in 0..=8 {
                let lat = bounds.south() + (bounds.north() - bounds.south()) * (j as f64 / 8.0);
                for i in 0..=8 {
                    let lon = bounds.west() + (bounds.east() - bounds.west()) * (i as f64 / 8.0);
                    let point = globe.lonlat_to_point(lon, lat);
                    assert!(
                        (point - patch.center()).length() <= patch.radius() + 1.0,
                        "sphere around {bounds:?} misses {lon},{lat}"
                    );
                }
            }
        }
    }

    #[test]
    fn bounding_sphere_tightens_with_zoom() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let mut previous = globe.radius.as_meters() + 1.0;
        for zoom in 0..12 {
            let tile = scheme.tile_for_lonlat(11.42, 47.27, zoom);
            let radius = TilePatch::new(&scheme, tile, &globe).radius();
            assert!(radius <= previous, "zoom {zoom} radius {radius}");
            if zoom >= 2 {
                assert!(radius < previous, "zoom {zoom} radius {radius}");
            }
            previous = radius;
        }
        assert!(previous < 20_000.0, "zoom 11 radius {previous}");
    }

    #[test]
    fn tiles_behind_the_globe_are_culled() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let eye = globe.lonlat_to_point(0.0, 0.0) * 3.0;

        let facing = TilePatch::new(&scheme, scheme.tile_for_lonlat(0.0, 0.0, 4), &globe);
        assert!(facing.is_visible_from(&globe, eye));

        let opposite = TilePatch::new(&scheme, scheme.tile_for_lonlat(180.0, 0.0, 4), &globe);
        assert!(!opposite.is_visible_from(&globe, eye));

        let inside = TilePatch::new(&scheme, scheme.tile_for_lonlat(65.0, 0.0, 6), &globe);
        assert!(inside.is_visible_from(&globe, eye));
        let outside = TilePatch::new(&scheme, scheme.tile_for_lonlat(80.0, 0.0, 6), &globe);
        assert!(!outside.is_visible_from(&globe, eye));
    }

    #[test]
    fn horizon_test_never_culls_a_tile_with_a_visible_point() {
        let globe = Globe::earth();
        let radius = globe.radius.as_meters();

        for scheme in [WebMercatorScheme::default()] {
            for tile in sample_tiles(&scheme, 5) {
                let patch = TilePatch::new(&scheme, tile, &globe);
                let bounds = scheme.bounds(tile);
                for altitude in [1_000.0, 500_000.0, 20_000_000.0] {
                    for (eye_lon, eye_lat) in [(0.0, 0.0), (11.42, 47.27), (-150.0, -70.0)] {
                        let eye = globe.normal_at(eye_lon, eye_lat) * (radius + altitude);
                        if patch.is_visible_from(&globe, eye) {
                            continue;
                        }
                        for j in 0..=8 {
                            let lat = bounds.south()
                                + (bounds.north() - bounds.south()) * (j as f64 / 8.0);
                            for i in 0..=8 {
                                let lon = bounds.west()
                                    + (bounds.east() - bounds.west()) * (i as f64 / 8.0);
                                let point = globe.lonlat_to_point(lon, lat);
                                assert!(
                                    !globe.is_above_horizon(point, eye),
                                    "{tile:?} culled at {altitude} m over {eye_lon},{eye_lat} \
                                     but {lon},{lat} is visible"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn everything_is_visible_from_inside_the_globe() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let patch = TilePatch::new(&scheme, scheme.tile_for_lonlat(180.0, 0.0, 4), &globe);
        assert!(patch.is_visible_from(&globe, DVec3::ZERO));
    }

    #[test]
    fn wgs84_rows_run_north_to_south() {
        let scheme = Wgs84Scheme::default();
        let globe = Globe::earth();
        let (_, rows) = scheme.matrix_dims(4);
        let north = TilePatch::new(&scheme, Tile::new(4, 0, 0), &globe);
        let south = TilePatch::new(&scheme, Tile::new(4, 0, rows - 1), &globe);
        assert!(north.center().y > 0.0);
        assert!(south.center().y < 0.0);
    }

    #[test]
    fn an_eye_over_a_tile_is_its_altitude_away() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let radius = globe.radius.as_meters();

        for zoom in [2, 6, 14] {
            for (lon, lat) in [(0.0, 0.0), (11.42, 47.27), (-150.0, -70.0)] {
                let tile = scheme.tile_for_lonlat(lon, lat, zoom);
                let patch = TilePatch::new(&scheme, tile, &globe);
                let eye = globe.normal_at(lon, lat) * (radius + 4_000.0);
                let distance = patch.distance_from(&globe, eye);
                assert!(
                    (distance - 4_000.0).abs() < 1.0,
                    "{tile:?} from {lon},{lat}: {distance}"
                );
            }
        }
    }

    #[test]
    fn distance_grows_with_the_gap_to_the_tile() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let radius = globe.radius.as_meters();
        let patch = TilePatch::new(&scheme, scheme.tile_for_lonlat(0.0, 0.0, 6), &globe);

        let mut previous = 0.0;
        for lon in [0.0, 10.0, 45.0, 90.0] {
            let eye = globe.normal_at(lon, 0.0) * (radius + 100_000.0);
            let distance = patch.distance_from(&globe, eye);
            assert!(distance > previous, "at {lon} degrees: {distance}");
            previous = distance;
        }
    }

    #[test]
    fn longitude_offset_wraps_across_the_antimeridian() {
        let bounds = BBox::new(90.0, 0.0, 135.0, 45.0);
        assert_eq!(lon_offset(bounds, 100.0), 0.0);
        assert_eq!(lon_offset(bounds, 90.0), 0.0);
        assert_eq!(lon_offset(bounds, 145.0), -10.0);
        assert_eq!(lon_offset(bounds, 80.0), 10.0);
        assert_eq!(lon_offset(bounds, -170.0), -55.0);
        assert_eq!(lon_offset(BBox::new(-180.0, -85.0, 180.0, 85.0), 42.0), 0.0);
    }

    #[test]
    fn the_nearest_point_stays_inside_the_tile() {
        let scheme = WebMercatorScheme::default();
        let globe = Globe::earth();
        let radius = globe.radius.as_meters();

        for tile in sample_tiles(&scheme, 4) {
            let patch = TilePatch::new(&scheme, tile, &globe);
            let bounds = scheme.bounds(tile);
            for (eye_lon, eye_lat) in [(0.0, 0.0), (11.42, 47.27), (-150.0, -70.0), (180.0, 10.0)] {
                let eye = globe.normal_at(eye_lon, eye_lat) * (radius + 500_000.0);
                let nearest = patch.nearest_point_to(&globe, eye);
                let (lon, lat) = to_lonlat(nearest);
                assert!(
                    lat >= bounds.south() - 1e-6 && lat <= bounds.north() + 1e-6,
                    "{tile:?}: latitude {lat} outside {bounds:?}"
                );
                let span = bounds.east() - bounds.west();
                let offset = (lon - bounds.west()).rem_euclid(360.0);
                assert!(
                    offset <= span + 1e-6 || offset >= 360.0 - 1e-6,
                    "{tile:?}: longitude {lon} outside {bounds:?}"
                );

                for j in 0..=8 {
                    let sample_lat =
                        bounds.south() + (bounds.north() - bounds.south()) * (j as f64 / 8.0);
                    for i in 0..=8 {
                        let sample_lon =
                            bounds.west() + (bounds.east() - bounds.west()) * (i as f64 / 8.0);
                        let sample = globe.lonlat_to_point(sample_lon, sample_lat);
                        assert!(
                            (sample - eye).length() >= (nearest - eye).length() - 1.0,
                            "{tile:?}: {sample_lon},{sample_lat} beats the nearest point"
                        );
                    }
                }
            }
        }
    }
}
