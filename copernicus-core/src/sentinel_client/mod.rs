//! client for Sentinel Hub, as offered through the Copernicus Data Space
//! Ecosystem.

pub mod types;

use anyhow::Result;
use reqwest::Method;
use serde_json::Value;
use std::sync::Arc;
use types::{CatalogSearchQuery, ProcessRequest};

use crate::{
    auth::OAuth2ClientCredentials,
    http_engine::{EngineConfig, HttpEngine},
};

const DEFAULT_PROCESS_BASE: &str = "https://sh.dataspace.copernicus.eu/process/v1";
const DEFAULT_CATALOG_BASE: &str = "https://sh.dataspace.copernicus.eu/catalog/v1";

#[derive(Clone)]
pub struct SentinelHubClient {
    engine: HttpEngine,
    process_base: String,
    catalog_base: String,
}

impl SentinelHubClient {
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
        Ok(Self::from_engine(engine))
    }

    /// reuse an existing engine, e.g. the one backing a
    /// `[CdseClient]`, so both clients share a single
    /// cached OAuth2 token instead of each fetching their own.
    pub fn from_engine(engine: HttpEngine) -> Self {
        Self {
            engine,
            process_base: DEFAULT_PROCESS_BASE.to_string(),
            catalog_base: DEFAULT_CATALOG_BASE.to_string(),
        }
    }

    pub fn with_process_base(mut self, url: impl Into<String>) -> Self {
        self.process_base = url.into();
        self
    }

    pub fn with_catalog_base(mut self, url: impl Into<String>) -> Self {
        self.catalog_base = url.into();
        self
    }

    pub fn engine(&self) -> &HttpEngine {
        &self.engine
    }

    /// render an image for the given request and return the raw encoded
    /// bytes (PNG by default, see [`ProcessRequest`]/`ProcessFormat` to
    /// request TIFF, etc.).
    pub fn process_image(&self, req: &ProcessRequest) -> Result<Vec<u8>> {
        self.engine.post_bytes(&self.process_base, req)
    }

    /// Search Sentinel Hub's STAC-compliant catalog
    pub fn search_catalog(&self, query: &CatalogSearchQuery) -> Result<Value> {
        let url = format!("{}/search", self.catalog_base.trim_end_matches('/'));
        self.engine.json_request(Method::POST, &url, Some(query))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{ProcessBounds, ProcessDataSource};

    #[test]
    fn true_color_request_serializes_expected_shape() {
        let req = ProcessRequest::true_color_s2(
            [16.3, 48.1, 16.5, 48.3],
            "2026-06-01T00:00:00Z",
            "2026-06-10T00:00:00Z",
            512,
            512,
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json["input"]["bounds"]["bbox"],
            serde_json::json!([16.3, 48.1, 16.5, 48.3])
        );
        assert_eq!(
            json["input"]["data"][0]["type"],
            serde_json::json!("sentinel-2-l2a")
        );
        assert_eq!(
            json["input"]["data"][0]["dataFilter"]["timeRange"]["from"],
            serde_json::json!("2026-06-01T00:00:00Z")
        );
        assert_eq!(json["output"]["width"], serde_json::json!(512));
        assert!(req.evalscript.contains("evaluatePixel"));
    }

    #[test]
    fn manual_process_request_with_custom_evalscript() {
        let req = ProcessRequest::new(
            ProcessBounds::wgs84([0.0, 0.0, 1.0, 1.0]),
            vec![ProcessDataSource::new("sentinel-1-grd")],
            256,
            256,
            "//VERSION=3\nfunction setup(){}\n",
        );
        assert_eq!(req.output.width, 256);
    }
}
