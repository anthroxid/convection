use convection_types::BBox;
use serde::Serialize;
use serde_json::Value;

// https://documentation.dataspace.copernicus.eu/APIs/SentinelHub/Evalscript/V3.html
// https://sh.dataspace.copernicus.eu/process/v1

/// A `POST /process/v1` request: a bbox + time filter, a rendering script
/// (i.e. "evalscript"), and the desired output image format
#[derive(Debug, Clone, Serialize)]
pub struct ProcessRequest {
    pub input: ProcessInput,
    pub output: ProcessOutput,
    pub evalscript: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInput {
    pub bounds: ProcessBounds,
    pub data: Vec<ProcessDataSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessBounds {
    pub bbox: BBox,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
}

impl ProcessBounds {
    pub fn wgs84<B>(bbox: B) -> Self
    where
        B: Into<BBox>,
    {
        Self {
            bbox: bbox.into(),
            properties: Some(serde_json::json!({
                "crs": "http://www.opengis.net/def/crs/EPSG/0/4326"
            })),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessDataSource {
    /// data collection identifier, e.g. `"sentinel-2-l2a"`, `"sentinel-1-grd"`,
    /// `"sentinel-3-olci"`. see
    /// <https://documentation.dataspace.copernicus.eu/APIs/SentinelHub/Data/S2L2A.html>
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "dataFilter")]
    pub data_filter: Option<Value>,
}

impl ProcessDataSource {
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            ty: collection.into(),
            data_filter: None,
        }
    }

    /// restrict to acquisitions within time range `[from_rfc3339, to_rfc3339)`
    pub fn time_range(mut self, from_rfc3339: &str, to_rfc3339: &str) -> Self {
        let filter = self
            .data_filter
            .get_or_insert_with(|| serde_json::json!({}));
        filter["timeRange"] = serde_json::json!({ "from": from_rfc3339, "to": to_rfc3339 });
        self
    }

    /// set max cloud cover by percentage
    pub fn max_cloud_cover(mut self, max_pct: f64) -> Self {
        let filter = self
            .data_filter
            .get_or_insert_with(|| serde_json::json!({}));
        filter["maxCloudCoverage"] = serde_json::json!(max_pct);
        self
    }

    /// set mosaicking order, see [`MosaichingOrder`] for more info
    pub fn mosaicking_order(mut self, order: MosaickingOrder) -> Self {
        let filter = self
            .data_filter
            .get_or_insert_with(|| serde_json::json!({}));
        filter["mosaickingOrder"] = serde_json::json!(order);
        self
    }
}

/// the mosacking order defined in <https://documentation.dataspace.copernicus.eu/APIs/SentinelHub/Data/S2L1C.html#mosaickingorder>
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MosaickingOrder {
    MostRecent,
    LeastRecent,
    LeastCC,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessOutput {
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses: Option<Vec<ProcessResponseFormat>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessResponseFormat {
    pub identifier: String,
    pub format: ProcessFormat,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ProcessFormat {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/tiff")]
    Tiff,
}

impl ProcessRequest {
    pub fn new(
        bounds: ProcessBounds,
        data: Vec<ProcessDataSource>,
        width: u32,
        height: u32,
        evalscript: impl Into<String>,
    ) -> Self {
        Self {
            input: ProcessInput { bounds, data },
            output: ProcessOutput {
                width,
                height,
                responses: Some(vec![ProcessResponseFormat {
                    identifier: "default".to_string(),
                    format: ProcessFormat::Png,
                }]),
            },
            evalscript: evalscript.into(),
        }
    }

    /// constrtuct a Sentinel-2 L2A true-color render over `bbox`
    /// for the given time window, at `width` x `height` pixels
    /// see <https://documentation.dataspace.copernicus.eu/notebook-samples/sentinelhub/data_download_process_request.html#example-2-true-color-mosaic-of-least-cloudy-acquisitions>
    pub fn true_color_s2<B>(
        bbox: B,
        from_rfc3339: &str,
        to_rfc3339: &str,
        width: u32,
        height: u32,
    ) -> Self
    where
        B: Into<BBox>,
    {
        Self::new(
            ProcessBounds::wgs84(bbox),
            vec![
                ProcessDataSource::new("sentinel-2-l2a")
                    .time_range(from_rfc3339, to_rfc3339)
                    .mosaicking_order(MosaickingOrder::LeastRecent),
            ],
            width,
            height,
            TRUE_COLOR_EVALSCRIPT,
        )
    }
}

/// minimal, standard true-color evalscript
pub const TRUE_COLOR_EVALSCRIPT: &str = include_str!("true_color.js");

#[derive(Debug, Clone, Default, Serialize)]
pub struct CatalogSearchQuery {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl CatalogSearchQuery {
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
}
