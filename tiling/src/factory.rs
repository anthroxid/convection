use std::hash::{DefaultHasher, Hash, Hasher};

use image::{Rgba, RgbaImage};

use crate::tile::{Tile, TileImage, TilingScheme, WebMercatorScheme};

pub trait TileFactory {
    type Scheme: TilingScheme + Default;

    /// constructs self with the default `TilingScheme`
    fn new() -> Self;
    fn with_scheme(scheme: Self::Scheme) -> Self;
    fn scheme(&self) -> &Self::Scheme;
    /// render a tile to a [`TileImage`] using [`Tile`] as a key
    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage>;

    /// optional impl if you want to segregate the caches into separate namespaces
    /// see [`TileCache`] for more info
    fn cache_namespace(&self) -> Option<String> {
        None
    }

    /// construct a tile at the coordinates with the factory's scheme
    fn tile_at(&self, lon: f64, lat: f64, zoom: u32) -> Tile {
        self.scheme().tile_for_lonlat(lon, lat, zoom)
    }

    fn rendered_tile_at(&self, lon: f64, lat: f64, zoom: u32) -> anyhow::Result<TileImage> {
        self.rendered_tile(self.tile_at(lon, lat, zoom))
    }
}

#[derive(Default)]
pub struct DummyTileFactory {
    scheme: WebMercatorScheme,
}

impl DummyTileFactory {
    /// get a tile color from the tile's hash
    fn tile_color(tile: Tile) -> Rgba<u8> {
        let mut hasher = DefaultHasher::new();
        tile.hash(&mut hasher);
        let hash = hasher.finish();
        Rgba([
            (hash & 0xFF) as u8,
            ((hash >> 8) & 0xFF) as u8,
            ((hash >> 16) & 0xFF) as u8,
            255,
        ])
    }
}

impl TileFactory for DummyTileFactory {
    type Scheme = WebMercatorScheme;

    fn new() -> Self {
        Self::default()
    }

    fn with_scheme(scheme: Self::Scheme) -> Self {
        Self { scheme }
    }

    fn scheme(&self) -> &Self::Scheme {
        &self.scheme
    }

    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
        let size = self.scheme.tile_size();
        let image = RgbaImage::from_pixel(size, size, Self::tile_color(tile));
        Ok(TileImage::new(tile, image))
    }
}
