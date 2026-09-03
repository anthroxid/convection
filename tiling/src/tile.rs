use std::f64::consts::PI;

use convection_types::BBox;
use image::RgbaImage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    zoom: u32,
    x: u32,
    y: u32,
}

impl Tile {
    pub fn new(zoom: u32, x: u32, y: u32) -> Self {
        Self { zoom, x, y }
    }

    pub fn zoom(&self) -> u32 {
        self.zoom
    }

    pub fn x(&self) -> u32 {
        self.x
    }

    pub fn y(&self) -> u32 {
        self.y
    }

    pub fn xy(&self) -> (u32, u32) {
        (self.x, self.y)
    }

    /// validate against a scheme's matrix dimensions at this tile's zoom
    pub fn is_valid_for(&self, scheme: &impl TilingScheme) -> bool {
        let (cols, rows) = scheme.matrix_dims(self.zoom);
        self.x < cols && self.y < rows
    }

    /// flip the y-axis (XYZ <-> TMS conversion) given a scheme's row count at this zoom
    pub fn flip_y(&self, scheme: &impl TilingScheme) -> Tile {
        let (_, rows) = scheme.matrix_dims(self.zoom);
        Tile::new(self.zoom, self.x, rows - 1 - self.y)
    }

    /// the four child tiles at zoom+1. valid for any scheme whose matrix
    /// dimensions double per zoom level
    pub fn children(&self) -> [Tile; 4] {
        let z = self.zoom + 1;
        let x = self.x * 2;
        let y = self.y * 2;
        [
            Tile::new(z, x, y),
            Tile::new(z, x + 1, y),
            Tile::new(z, x, y + 1),
            Tile::new(z, x + 1, y + 1),
        ]
    }

    pub fn parent(&self) -> Option<Tile> {
        if self.zoom == 0 {
            None
        } else {
            Some(Tile::new(self.zoom - 1, self.x / 2, self.y / 2))
        }
    }

    /// offset in the ancestor's tile
    pub fn offset_in(&self, ancestor: Tile) -> Option<(u32, u32, u32)> {
        let levels = self.zoom.checked_sub(ancestor.zoom)?;
        if levels >= u32::BITS {
            return None;
        }
        let span = 1u32 << levels;
        let (x, y) = (
            self.x.checked_sub(ancestor.x.checked_mul(span)?)?,
            self.y.checked_sub(ancestor.y.checked_mul(span)?)?,
        );
        (x < span && y < span).then_some((x, y, span))
    }
}

/// the conventional zoom/x/y notation, as used by tile URLs
impl std::fmt::Display for Tile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.zoom, self.x, self.y)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TileImage {
    tile: Tile,
    image: RgbaImage,
}

impl TileImage {
    pub fn new(tile: Tile, image: RgbaImage) -> Self {
        Self { tile, image }
    }

    pub fn tile(&self) -> Tile {
        self.tile
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn pixel_dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }
}

pub trait TilingScheme {
    fn id(&self) -> &'static str;

    /// tile edge length in pixels (e.g. 256, 512)
    fn tile_size(&self) -> u32;

    /// (columns, rows) of the tile matrix at a given zoom.
    /// not promised to be square (see WGS84 level 0 zoom)
    fn matrix_dims(&self, zoom: u32) -> (u32, u32);

    fn tile_for_lonlat(&self, lon: f64, lat: f64, zoom: u32) -> Tile;

    fn bounds(&self, tile: Tile) -> BBox;

    fn root_tiles(&self) -> Vec<Tile> {
        let (cols, rows) = self.matrix_dims(0);
        (0..rows)
            .flat_map(|y| (0..cols).map(move |x| Tile::new(0, x, y)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WebMercatorScheme {
    pub tile_size: u32,
}

impl Default for WebMercatorScheme {
    fn default() -> Self {
        Self { tile_size: 256 }
    }
}

impl TilingScheme for WebMercatorScheme {
    fn id(&self) -> &'static str {
        "WebMercator"
    }

    fn tile_size(&self) -> u32 {
        self.tile_size
    }

    fn matrix_dims(&self, zoom: u32) -> (u32, u32) {
        let n = 1u32 << zoom;
        (n, n)
    }

    fn tile_for_lonlat(&self, lon: f64, lat: f64, zoom: u32) -> Tile {
        let n = (1u32 << zoom) as f64;
        let lat = lat.clamp(-85.051_128_78, 85.051_128_78); // Web Mercator's valid range
        let lon = ((lon + 180.0).rem_euclid(360.0)) - 180.0; // normalize antimeridian wraparound

        let x = ((lon + 180.0) / 360.0 * n).floor() as u32;
        let lat_rad = lat.to_radians();
        let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * n).floor() as u32;

        Tile::new(zoom, x.min(n as u32 - 1), y.min(n as u32 - 1))
    }

    fn bounds(&self, tile: Tile) -> BBox {
        let n = (1u32 << tile.zoom()) as f64;
        let west = tile.x() as f64 / n * 360.0 - 180.0;
        let east = (tile.x() as f64 + 1.0) / n * 360.0 - 180.0;

        let lat_of = |y: f64| {
            let val = std::f64::consts::PI * (1.0 - 2.0 * y / n);
            val.sinh().atan().to_degrees()
        };

        BBox::new(
            west,
            lat_of(tile.y() as f64 + 1.0),
            east,
            lat_of(tile.y() as f64),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Wgs84Scheme {
    pub tile_size: u32,
}

impl Default for Wgs84Scheme {
    fn default() -> Self {
        Self { tile_size: 256 }
    }
}

impl TilingScheme for Wgs84Scheme {
    fn id(&self) -> &'static str {
        "WGS84"
    }

    fn tile_size(&self) -> u32 {
        self.tile_size
    }

    fn matrix_dims(&self, zoom: u32) -> (u32, u32) {
        // 360deg wide / 180deg tall, twice as many columns as rows
        let rows = 1u32 << zoom;
        (rows * 2, rows)
    }

    fn tile_for_lonlat(&self, lon: f64, lat: f64, zoom: u32) -> Tile {
        let (cols, rows) = self.matrix_dims(zoom);
        let lon = ((lon + 180.0).rem_euclid(360.0)) - 180.0;
        let lat = lat.clamp(-90.0, 90.0);

        let x = ((lon + 180.0) / 360.0 * cols as f64).floor() as u32;
        let y = ((90.0 - lat) / 180.0 * rows as f64).floor() as u32;

        Tile::new(zoom, x.min(cols - 1), y.min(rows - 1))
    }

    fn bounds(&self, tile: Tile) -> BBox {
        let (cols, rows) = self.matrix_dims(tile.zoom());
        let west = tile.x() as f64 / cols as f64 * 360.0 - 180.0;
        let east = (tile.x() as f64 + 1.0) / cols as f64 * 360.0 - 180.0;
        let north = 90.0 - tile.y() as f64 / rows as f64 * 180.0;
        let south = 90.0 - (tile.y() as f64 + 1.0) / rows as f64 * 180.0;
        BBox::new(west, south, east, north)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_display_as_zoom_x_y() {
        assert_eq!(Tile::new(12, 2200, 1428).to_string(), "12/2200/1428");
    }

    #[test]
    fn children_and_parent_round_trip() {
        let tile = Tile::new(3, 5, 2);
        for child in tile.children() {
            assert_eq!(child.zoom(), 4);
            assert_eq!(child.parent(), Some(tile));
        }
        assert_eq!(Tile::new(0, 0, 0).parent(), None);
    }

    #[test]
    fn offset_locates_a_tile_within_its_ancestor() {
        let ancestor = Tile::new(2, 1, 1);
        assert_eq!(ancestor.offset_in(ancestor), Some((0, 0, 1)));

        // the ancestor's own quadrant, two levels down: 4x4 tiles starting at (4, 4)
        assert_eq!(Tile::new(4, 4, 4).offset_in(ancestor), Some((0, 0, 4)));
        assert_eq!(Tile::new(4, 7, 5).offset_in(ancestor), Some((3, 1, 4)));

        // outside the ancestor's footprint, or above it
        assert_eq!(Tile::new(4, 8, 4).offset_in(ancestor), None);
        assert_eq!(Tile::new(1, 0, 0).offset_in(ancestor), None);
    }

    #[test]
    fn web_mercator_lonlat_round_trips_through_bounds() {
        let scheme = WebMercatorScheme::default();
        for (lon, lat) in [(0.0, 0.0), (11.42, 47.27), (-122.4, 37.8), (179.9, -60.0)] {
            for zoom in [0, 4, 12] {
                let tile = scheme.tile_for_lonlat(lon, lat, zoom);
                assert!(tile.is_valid_for(&scheme));
                let b = scheme.bounds(tile);
                assert!(b.west() <= lon && lon <= b.east(), "lon {lon} at z{zoom}");
                assert!(b.south() <= lat && lat <= b.north(), "lat {lat} at z{zoom}");
            }
        }
    }

    #[test]
    fn web_mercator_root_covers_the_world() {
        let b = WebMercatorScheme::default().bounds(Tile::new(0, 0, 0));
        assert!((b.west() - -180.0).abs() < 1e-9);
        assert!((b.east() - 180.0).abs() < 1e-9);
        assert!((b.north() - 85.051_128_78).abs() < 1e-6);
        assert!((b.south() - -85.051_128_78).abs() < 1e-6);
    }

    #[test]
    fn wgs84_lonlat_round_trips_through_bounds() {
        let scheme = Wgs84Scheme::default();
        for (lon, lat) in [(0.0, 0.0), (11.42, 47.27), (-30.0, -89.0)] {
            for zoom in [0, 3, 9] {
                let tile = scheme.tile_for_lonlat(lon, lat, zoom);
                assert!(tile.is_valid_for(&scheme));
                let b = scheme.bounds(tile);
                assert!(b.west() <= lon && lon <= b.east(), "lon {lon} at z{zoom}");
                assert!(b.south() <= lat && lat <= b.north(), "lat {lat} at z{zoom}");
            }
        }
    }

    #[test]
    fn flip_y_is_an_involution() {
        let scheme = WebMercatorScheme::default();
        let tile = Tile::new(4, 3, 11);
        assert_eq!(tile.flip_y(&scheme).flip_y(&scheme), tile);
        assert_eq!(tile.flip_y(&scheme), Tile::new(4, 3, 4));
    }
}
