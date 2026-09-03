use std::{ops::RangeInclusive, time::Instant};

use convection_types::{BBox, Globe};
use copernicus_core::{SentinelHubClient, types::ProcessRequest};
use log::{debug, trace, warn};

use crate::{
    factory::TileFactory,
    geometry::{TilePatch, zoom_for_resolution},
    tile::{Tile, TileImage, TilingScheme, Wgs84Scheme},
};

/// the Process API refuses requests for Sentinel-2 L2A that are coarser than
/// this, which is what puts a floor under the zoom levels this source can
/// serve for a given tile size
const S2L2A_MAX_METERS_PER_PIXEL: f64 = 1500.0;
/// Sentinel-2's own ground resolution. asking for more detail than this only
/// resamples the same pixels, so there is nothing to gain past it
const S2L2A_NATIVE_METERS_PER_PIXEL: f64 = 10.0;
/// the Process API's per-request pixel limit, on either axis
const MAX_REQUEST_PX: u32 = 2500;
/// tile size to request by default. bigger tiles reach coarser zoom levels,
/// at the cost of a slower request each
const DEFAULT_TILE_PX: u32 = 512;

pub struct SentinelHubTileFactory {
    /// the scheme's tile size doubles as the pixel size to request, so that
    /// the level of detail selection sizes texels by what actually comes back
    scheme: Wgs84Scheme,
    client: SentinelHubClient,
    time_from: String,
    time_to: String,
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
        }
        .with_tile_px(DEFAULT_TILE_PX)
    }

    /// set the pixel size of a requested tile, which also decides how coarse
    /// a zoom level this source can still serve
    pub fn with_tile_px(mut self, px: u32) -> Self {
        let tile_size = px.clamp(1, MAX_REQUEST_PX);
        if tile_size != px {
            warn!("{px} px exceeds the process api's limit of {MAX_REQUEST_PX} px, using that");
        }
        self.scheme = Wgs84Scheme { tile_size };

        let serves = self.zoom_range();
        debug!(
            "{} at {tile_size} px per tile serves zoom {}..={} ({:.0} to {:.0} m/px)",
            self.layer_label(),
            serves.start(),
            serves.end(),
            self.meters_per_pixel(*serves.start()),
            self.meters_per_pixel(*serves.end()),
        );
        self
    }

    /// ground meters per pixel of a tile at `zoom`, as the Process API
    /// measures the resolution it is being asked for
    fn meters_per_pixel(&self, zoom: u32) -> f64 {
        let tile = self.scheme.tile_for_lonlat(0.0, 0.0, zoom);
        TilePatch::new(&self.scheme, tile, &Globe::earth())
            .extent()
            .as_meters()
            / self.scheme.tile_size().max(1) as f64
    }

    fn build_request(&self, bbox: BBox) -> ProcessRequest {
        let px = self.scheme.tile_size();
        ProcessRequest::true_color_s2(bbox, &self.time_from, &self.time_to, px, px)
    }

    fn layer_label(&self) -> &str {
        "sentinel-2-l2a-true-color"
    }
}

impl TileFactory for SentinelHubTileFactory {
    type Scheme = Wgs84Scheme;

    /// cannot be constructed this way, use `true_color_s2` constructor instead
    fn new() -> Self {
        unimplemented!(
            "SentinelHubTileFactory cannot be constructed this way, use `true_color_s2` instead"
        );
    }

    /// cannot be constructed this way, use `true_color_s2` constructor instead
    fn with_scheme(_scheme: Self::Scheme) -> Self {
        unimplemented!(
            "SentinelHubTileFactory cannot be constructed this way, use `true_color_s2` instead"
        );
    }

    fn scheme(&self) -> &Self::Scheme {
        &self.scheme
    }

    fn zoom_range(&self) -> RangeInclusive<u32> {
        let globe = Globe::earth();
        let coarsest = zoom_for_resolution(&self.scheme, &globe, S2L2A_MAX_METERS_PER_PIXEL);
        let finest = zoom_for_resolution(&self.scheme, &globe, S2L2A_NATIVE_METERS_PER_PIXEL);
        coarsest..=finest.max(coarsest)
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
        // the api rejects a request coarser than the collection's limit, so
        // say so here rather than spending a round trip on finding out
        let coarsest = *self.zoom_range().start();
        anyhow::ensure!(
            tile.zoom() >= coarsest,
            "tile {tile} asks for {:.0} m/px, more than the {:.0} m/px {} can render at \
             {} px per tile: zoom {coarsest} or deeper is needed",
            self.meters_per_pixel(tile.zoom()),
            S2L2A_MAX_METERS_PER_PIXEL,
            self.layer_label(),
            self.scheme.tile_size(),
        );

        let bbox = self.scheme.bounds(tile);
        debug!(
            "requesting {} for tile {tile} at {:.4},{:.4} to {:.4},{:.4}",
            self.layer_label(),
            bbox.west(),
            bbox.south(),
            bbox.east(),
            bbox.north(),
        );

        let started = Instant::now();
        let req = self.build_request(bbox);
        let bytes = self.client.process_image(&req)?;
        let fetched = started.elapsed();

        let image = image::load_from_memory(&bytes)?.into_rgba8();
        trace!(
            "tile {tile}: {} bytes fetched in {fetched:?}, decoded to {}x{} px in {:?}",
            bytes.len(),
            image.width(),
            image.height(),
            started.elapsed() - fetched,
        );

        Ok(TileImage::new(tile, image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> SentinelHubTileFactory {
        // the client is never used: none of these assertions send a request
        SentinelHubTileFactory::true_color_s2(
            SentinelHubClient::new("id", "secret").expect("client"),
            "2026-06-01T00:00:00Z",
            "2026-06-10T00:00:00Z",
        )
    }

    #[test]
    fn the_served_zoom_range_stays_inside_the_collection_limits() {
        let factory = factory();
        let serves = factory.zoom_range();

        // 512 px tiles put the coarsest servable level at zoom 5
        assert_eq!(*serves.start(), 5);
        assert!(factory.meters_per_pixel(*serves.start()) <= S2L2A_MAX_METERS_PER_PIXEL);
        // and one level coarser is exactly what the api refuses
        assert!(factory.meters_per_pixel(serves.start() - 1) > S2L2A_MAX_METERS_PER_PIXEL);

        // the deep end stops where sentinel-2 stops resolving
        assert!(factory.meters_per_pixel(*serves.end()) <= S2L2A_NATIVE_METERS_PER_PIXEL);
    }

    #[test]
    fn a_tile_too_coarse_to_render_fails_without_a_request() {
        // zoom 3 at 512 px is the 4892 m/px request the api rejected
        let err = factory()
            .rendered_tile(Tile::new(3, 4, 2))
            .expect_err("zoom 3 is too coarse");
        let message = format!("{err}");
        assert!(message.contains("4892 m/px"), "{message}");
        assert!(message.contains("zoom 5 or deeper"), "{message}");
    }

    #[test]
    fn the_range_survives_the_caching_wrapper_and_reaches_the_manager() {
        use crate::{
            cache::{CachingTileFactory, MemoryCache},
            lod::{LodConfig, LodTileManager},
        };

        // the same stack the viewer builds: a cached sentinel hub source
        // driven by an otherwise unconstrained level of detail config
        let manager = LodTileManager::new(
            CachingTileFactory::wrap(factory(), MemoryCache::default()),
            LodConfig::default(),
        );
        assert_eq!(manager.config().min_zoom, 5);
        assert_eq!(manager.config().max_zoom, 12);
    }

    #[test]
    fn bigger_tiles_reach_coarser_levels() {
        let factory = factory().with_tile_px(1024);
        assert_eq!(*factory.zoom_range().start(), 4);
        assert_eq!(factory.scheme().tile_size(), 1024);
    }

    #[test]
    fn a_tile_size_beyond_the_api_limit_is_clamped() {
        let factory = factory().with_tile_px(4096);
        assert_eq!(factory.scheme().tile_size(), MAX_REQUEST_PX);
    }
}
