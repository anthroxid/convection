//! tile source for OpenStreetMap's standard raster tiles.
//!
//! the public tile servers are run on donated capacity, and their usage
//! policy asks callers to identify themselves, to cache what they fetch, and
//! to keep the request rate low. see <https://operations.osmfoundation.org/policies/tiles/>

use std::{time::Duration, time::Instant};

use log::{debug, trace, warn};
use reqwest::{
    blocking::Client as HttpClient,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};

use crate::{
    factory::TileFactory,
    tile::{Tile, TileImage, WebMercatorScheme},
};

/// the deepest zoom the standard style is rendered at
const MAX_ZOOM: u32 = 19;
/// the tiles the standard style serves are always this many pixels square
const TILE_PX: u32 = 256;
const DEFAULT_URL_TEMPLATE: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
const DEFAULT_USER_AGENT: &str = concat!("convection-tiling/", env!("CARGO_PKG_VERSION"));
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct OsmTileFactory {
    scheme: WebMercatorScheme,
    client: HttpClient,
    url_template: String,
}

impl OsmTileFactory {
    /// build a factory that identifies itself with `user_agent`
    pub fn with_user_agent(user_agent: impl AsRef<str>) -> anyhow::Result<Self> {
        let user_agent = user_agent.as_ref();
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(user_agent).map_err(|_| {
                anyhow::anyhow!("the user agent {user_agent:?} is not a valid header value")
            })?,
        );

        let client = HttpClient::builder()
            .default_headers(headers)
            .timeout(DEFAULT_TIMEOUT)
            .build()?;

        debug!("osm tile source as {user_agent}, serving zoom 0..={MAX_ZOOM}");
        Ok(Self {
            scheme: WebMercatorScheme { tile_size: TILE_PX },
            client,
            url_template: DEFAULT_URL_TEMPLATE.to_string(),
        })
    }

    /// point the factory at another (self hosted) server
    pub fn with_url_template(mut self, template: impl Into<String>) -> Self {
        self.url_template = template.into();
        debug!("osm tile source using {}", self.url_template);
        self
    }

    fn tile_url(&self, tile: Tile) -> String {
        self.url_template
            .replace("{z}", &tile.zoom().to_string())
            .replace("{x}", &tile.x().to_string())
            .replace("{y}", &tile.y().to_string())
    }
}

impl TileFactory for OsmTileFactory {
    type Scheme = WebMercatorScheme;

    /// identifies itself as this crate, see [`OsmTileFactory::with_user_agent`]
    fn new() -> Self {
        warn!(
            "the osm tile source is identifying itself as {DEFAULT_USER_AGENT}: the tile usage \
             policy asks for a user agent naming your application, see \
             OsmTileFactory::with_user_agent"
        );
        Self::with_user_agent(DEFAULT_USER_AGENT).expect("the default user agent is a valid header")
    }

    /// the tile size is fixed by what the servers render, so only a scheme
    /// carrying [`TILE_PX`] tiles is honoured
    fn with_scheme(scheme: Self::Scheme) -> Self {
        if scheme.tile_size != TILE_PX {
            warn!(
                "osm serves {TILE_PX} px tiles, ignoring requested {} px",
                scheme.tile_size
            );
        }
        Self::new()
    }

    fn scheme(&self) -> &Self::Scheme {
        &self.scheme
    }

    fn cache_namespace(&self) -> Option<String> {
        Some("open-street-map".to_owned())
    }

    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
        anyhow::ensure!(
            tile.zoom() <= MAX_ZOOM,
            "tile {tile} is deeper than the zoom {MAX_ZOOM} osm renders"
        );
        anyhow::ensure!(
            tile.is_valid_for(&self.scheme),
            "tile {tile} is outside the tile matrix at its zoom"
        );

        let url = self.tile_url(tile);
        trace!("fetching {url}");
        let started = Instant::now();

        let response = self
            .client
            .get(&url)
            .send()
            .map_err(|err| anyhow::anyhow!("fetching {url} failed: {err}"))?;

        let status = response.status();
        anyhow::ensure!(
            status.is_success(),
            "fetching {url} failed: HTTP {status}{}",
            if status.as_u16() == 429 {
                ", the request rate is above what the tile usage policy allows"
            } else {
                ""
            }
        );

        let bytes = response.bytes()?;
        let image = image::load_from_memory(&bytes)?.into_rgba8();
        if image.width() != TILE_PX || image.height() != TILE_PX {
            warn!(
                "{url} returned a {}x{} px tile, expected: {TILE_PX} px ",
                image.width(),
                image.height()
            );
        }
        debug!(
            "fetched tile {tile}: {} bytes in {:?}",
            bytes.len(),
            started.elapsed()
        );

        Ok(TileImage::new(tile, image))
    }
}
