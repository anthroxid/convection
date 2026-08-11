use convection_types::BBox;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// builder for a STAC API `POST /search` request body.
///
/// see <https://stac.dataspace.copernicus.eu/v1/> for the live catalogue and
/// <https://documentation.dataspace.copernicus.eu/APIs/STAC.html> for the
/// supported filters/extensions
#[derive(Debug, Clone, Default, Serialize)]
pub struct StacSearchQuery {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    /// RFC 3339 interval, e.g. `"2024-06-01T00:00:00Z/2024-06-10T00:00:00Z"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// STAC query extension, e.g. `{"eo:cloud_cover": {"lte": 20}}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<Value>,
}

impl StacSearchQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collection(mut self, id: impl Into<String>) -> Self {
        self.collections.push(id.into());
        self
    }

    pub fn bbox<B>(mut self, bbox: B) -> Self
    where
        B: Into<BBox>,
    {
        self.bbox = Some(bbox.into());
        self
    }

    pub fn datetime_range(mut self, start_rfc3339: &str, end_rfc3339: &str) -> Self {
        self.datetime = Some(format!("{start_rfc3339}/{end_rfc3339}"));
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// filters on the `eo:cloud_cover` property (percentage, 0-100),
    pub fn max_cloud_cover(mut self, max_pct: f64) -> Self {
        self.query = Some(serde_json::json!({ "eo:cloud_cover": { "lte": max_pct } }));
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StacItemCollection {
    #[serde(default)]
    pub features: Vec<StacItem>,
    #[serde(default)]
    pub links: Vec<StacLink>,
    #[serde(default)]
    pub context: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StacItem {
    pub id: String,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    #[serde(default)]
    pub geometry: Option<Value>,
    #[serde(default)]
    pub properties: serde_json::Map<String, Value>,
    #[serde(default)]
    pub assets: HashMap<String, StacAsset>,
}

impl StacItem {
    /// convenience accessor for the common `properties.datetime` field.
    pub fn datetime(&self) -> Option<&str> {
        self.properties.get("datetime").and_then(Value::as_str)
    }

    /// convenience accessor for `properties["eo:cloud_cover"]`, present on
    /// Sentinel-2 optical items.
    pub fn cloud_cover(&self) -> Option<f64> {
        self.properties
            .get("eo:cloud_cover")
            .and_then(Value::as_f64)
    }

    pub fn asset(&self, key: &str) -> Option<&StacAsset> {
        self.assets.get(key)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StacAsset {
    pub href: String,
    #[serde(default, rename = "type")]
    pub media_type: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StacLink {
    pub rel: String,
    pub href: String,
}
