use convection_types::BBox;
use copernicus_core::{SentinelHubClient, types::ProcessRequest};

use crate::{
    factory::TileFactory,
    tile::{Tile, TileImage, TilingScheme, Wgs84Scheme},
};

pub struct SentinelHubTileFactory {
    scheme: Wgs84Scheme,
    client: SentinelHubClient,
    time_from: String,
    time_to: String,
    tile_px: u32,
}

impl SentinelHubTileFactory {
    /// true-color Sentinel-2 L2A imagery over the given ISO8601 time range
    pub fn true_color_s2(
        client: SentinelHubClient,
        time_from: impl Into<String>,
        time_to: impl Into<String>,
    ) -> Self {
        Self {
            scheme: Wgs84Scheme::default(),
            client,
            time_from: time_from.into(),
            time_to: time_to.into(),
            tile_px: 512,
        }
    }

    pub fn with_tile_px(mut self, px: u32) -> Self {
        self.tile_px = px;
        self
    }

    fn build_request(&self, bbox: BBox) -> ProcessRequest {
        ProcessRequest::true_color_s2(
            bbox,
            &self.time_from,
            &self.time_to,
            self.tile_px,
            self.tile_px,
        )
    }

    fn layer_label(&self) -> &str {
        "sentinel-2-l2a-true-color"
    }
}

impl TileFactory for SentinelHubTileFactory {
    type Scheme = Wgs84Scheme;

    // TODO: stabilize API in trait so this isn't necessary
    fn new() -> Self {
        panic!(
            "SentinelHubTileFactory cannot be constructed this way, use `true_color_s2` instead"
        );
    }

    // TODO: stabilize API in trait so this isn't necessary
    fn with_scheme(_scheme: Self::Scheme) -> Self {
        panic!(
            "SentinelHubTileFactory cannot be constructed this way, use `true_color_s2` instead"
        );
    }

    fn scheme(&self) -> &Self::Scheme {
        &self.scheme
    }

    fn cache_namespace(&self) -> Option<String> {
        Some(format!(
            "{}/{}_{}",
            self.layer_label(),
            self.time_from,
            self.time_to
        ))
    }

    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
        let bbox = self.scheme.bounds(tile);
        let req = self.build_request(bbox);
        let bytes = self.client.process_image(&req)?;
        let image = image::load_from_memory(&bytes)?.into_rgba8();

        Ok(TileImage::new(tile, image))
    }
}
