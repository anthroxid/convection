//! client for the Copernicus Data Space Ecosystem (CDSE)
//!
//! Auth is OAuth2 client-credentials against CDSE's Keycloak realm. Register
//! an OAuth client in the Sentinel Hub dashboard to get a `client_id` /
//! `client_secret` pair. See
//! <https://documentation.dataspace.copernicus.eu/APIs/SentinelHub/Overview/Authentication.html>.
//!
//! for rendering images directly (rather than downloading whole granules
//! and processing them yourself), prefer the sentinel client, which
//! wraps the same account's Process/WMTS APIs.

pub mod types;

use anyhow::Result;
use reqwest::Method;
use std::sync::Arc;
use types::{StacItem, StacItemCollection, StacSearchQuery};

use crate::{
    auth::OAuth2ClientCredentials,
    http_engine::{EngineConfig, HttpEngine},
};

const DEFAULT_STAC_BASE: &str = "https://stac.dataspace.copernicus.eu/v1";

#[derive(Clone)]
pub struct CdseClient {
    engine: HttpEngine,
    stac_base: String,
}

impl CdseClient {
    /// creates a client using CDSE OAuth2 client-credentials and default
    /// endpoints/timeouts.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Result<Self> {
        Self::with_config(client_id, client_secret, EngineConfig::default())
    }

    pub fn with_config(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        config: EngineConfig,
    ) -> Result<Self> {
        let auth = Arc::new(OAuth2ClientCredentials::cdse(client_id, client_secret));
        let engine = HttpEngine::new(auth, config)?;
        Ok(Self {
            engine,
            stac_base: DEFAULT_STAC_BASE.to_string(),
        })
    }

    /// builds a client that shares an already-constructed [`HttpEngine`],
    /// e.g. one whose `OAuth2ClientCredentials` is also being reused by a
    /// sentinel client.
    pub fn from_engine(engine: HttpEngine) -> Self {
        Self {
            engine,
            stac_base: DEFAULT_STAC_BASE.to_string(),
        }
    }

    pub fn with_stac_base(mut self, url: impl Into<String>) -> Self {
        self.stac_base = url.into();
        self
    }

    /// read-only access to the underlying engine, e.g. to hand its auth to
    /// another client.
    pub fn engine(&self) -> &HttpEngine {
        &self.engine
    }

    /// search the STAC catalogue
    pub fn search_stac(&self, query: &StacSearchQuery) -> Result<StacItemCollection> {
        let url = format!("{}/search", self.stac_base.trim_end_matches('/'));
        self.engine.json_request(Method::POST, &url, Some(query))
    }

    /// Fetch a single STAC item by collection + id.
    pub fn get_stac_item(&self, collection: &str, item_id: &str) -> Result<StacItem> {
        let url = format!(
            "{}/collections/{}/items/{}",
            self.stac_base.trim_end_matches('/'),
            collection,
            item_id
        );
        self.engine
            .json_request::<(), StacItem>(Method::GET, &url, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stac_search_query_serializes_expected_shape() {
        let query = StacSearchQuery::new()
            .collection("sentinel-2-l2a")
            .bbox([16.3, 48.1, 16.5, 48.3])
            .datetime_range("2026-06-01T00:00:00Z", "2026-06-10T00:00:00Z")
            .max_cloud_cover(20.0)
            .limit(5);
        let json = serde_json::to_value(&query).unwrap();
        assert_eq!(json["collections"], serde_json::json!(["sentinel-2-l2a"]));
        assert_eq!(json["bbox"], serde_json::json!([16.3, 48.1, 16.5, 48.3]));
        assert_eq!(
            json["datetime"],
            serde_json::json!("2026-06-01T00:00:00Z/2026-06-10T00:00:00Z")
        );
        assert_eq!(json["limit"], serde_json::json!(5));
        assert_eq!(
            json["query"]["eo:cloud_cover"]["lte"],
            serde_json::json!(20.0)
        );
    }
}
