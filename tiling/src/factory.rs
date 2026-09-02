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

/// generates each tile with a color picked by its zoom and a checkerboard
/// to show its texel density, so that LOD and tile boundaries are visible
#[derive(Default)]
pub struct DummyTileFactory {
    scheme: WebMercatorScheme,
}

impl DummyTileFactory {
    const ZOOM_COLORS: [[u8; 3]; 8] = [
        [0x3b, 0x6e, 0xa5],
        [0x4c, 0x9a, 0x6e],
        [0xb8, 0x8b, 0x3a],
        [0xa5, 0x4b, 0x4b],
        [0x7a, 0x5a, 0xa5],
        [0x3f, 0x8f, 0x99],
        [0x99, 0x66, 0x4f],
        [0x6f, 0x7f, 0x3f],
    ];

    const CHECKER_CELLS: u32 = 8;
    const BORDER_PX: u32 = 2;

    /// generate the image using the constants above
    fn tile_image(&self, tile: Tile) -> RgbaImage {
        let size = self.scheme.tile_size().max(4);
        let base = Self::ZOOM_COLORS[tile.zoom() as usize % Self::ZOOM_COLORS.len()];
        let cell = (size / Self::CHECKER_CELLS).max(1);

        RgbaImage::from_fn(size, size, |x, y| {
            let on_border = x < Self::BORDER_PX
                || y < Self::BORDER_PX
                || x >= size - Self::BORDER_PX
                || y >= size - Self::BORDER_PX;
            let shade = if on_border {
                0.35
            } else if (x / cell + y / cell).is_multiple_of(2) {
                1.0
            } else {
                0.75
            };
            Rgba([
                (base[0] as f32 * shade) as u8,
                (base[1] as f32 * shade) as u8,
                (base[2] as f32 * shade) as u8,
                255,
            ])
        })
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
        Ok(TileImage::new(tile, self.tile_image(tile)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_tiles_differ_between_zoom_levels() {
        let factory = DummyTileFactory::new();
        let shallow = factory.rendered_tile(Tile::new(3, 4, 4)).unwrap();
        let deep = factory.rendered_tile(Tile::new(4, 8, 8)).unwrap();
        assert_ne!(shallow.image(), deep.image());

        let size = factory.scheme().tile_size();
        assert_eq!(shallow.pixel_dimensions(), (size, size));
        // the border is darker than the tile's interior
        let brightness = |pixel: &Rgba<u8>| pixel.0[..3].iter().map(|c| *c as u32).sum::<u32>();
        let border = brightness(shallow.image().get_pixel(0, 0));
        let interior = brightness(shallow.image().get_pixel(4, 4));
        assert!(border < interior, "border {border}, interior {interior}");
    }
}
