use convection_types::BBox;

use crate::{
    factory::TileFactory,
    tile::{Tile, TileImage, TilingScheme, WebMercatorScheme},
};

use reqwest::blocking::{Client as HttpClient, Response};

pub struct OsmTileFactory {
    scheme: WebMercatorScheme,
    client: HttpClient,
}

impl TileFactory for OsmTileFactory {
    type Scheme = WebMercatorScheme;

    fn new() -> Self {
        Self::with_scheme(WebMercatorScheme::default())
    }

    fn with_scheme(scheme: Self::Scheme) -> Self {
        Self {
            scheme,
            client: HttpClient::new(),
        }
    }

    fn scheme(&self) -> &Self::Scheme {
        &self.scheme
    }

    fn cache_namespace(&self) -> Option<String> {
        Some("open-street-map".to_owned())
    }

    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
        let bbox = self.scheme.bounds(tile);
        todo!()
        // let req = self.build_request(bbox);
        // let bytes = self.client.process_image(&req)?;
        // let image = image::load_from_memory(&bytes)?.into_rgba8();

        // Ok(TileImage::new(tile, image))
    }
}
