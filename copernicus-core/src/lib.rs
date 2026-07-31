pub mod auth;
pub mod cdse_client;
pub mod ecmwf_client;
pub mod grib;
pub mod http_engine;
pub mod sentinel_client;

pub use auth::{ApiKeyAuth, AuthStrategy, OAuth2ClientCredentials};
pub use cdse_client::CdseClient;
pub use http_engine::{EngineConfig, HttpEngine};
pub use sentinel_client::SentinelHubClient;

pub mod types {

    // public re-exports
    pub use super::cdse_client::types::*;
    pub use super::sentinel_client::types::*;

    /// `bbox` in WGS84 (lon/lat) degrees: `[min_lon, min_lat, max_lon, max_lat]`.
    #[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
    #[serde(transparent)]
    pub struct BBox {
        inner: [f64; 4],
    }

    impl BBox {
        pub fn new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Self {
            Self {
                inner: [min_lon, min_lat, max_lon, max_lat],
            }
        }
    }

    impl From<(f64, f64, f64, f64)> for BBox {
        fn from((min_lon, min_lat, max_lon, max_lat): (f64, f64, f64, f64)) -> Self {
            Self::new(min_lon, min_lat, max_lon, max_lat)
        }
    }

    impl From<[f64; 4]> for BBox {
        fn from([min_lon, min_lat, max_lon, max_lat]: [f64; 4]) -> Self {
            Self::new(min_lon, min_lat, max_lon, max_lat)
        }
    }

    impl From<ndarray::Array2<f64>> for BBox {
        fn from(value: ndarray::Array2<f64>) -> Self {
            Self::new(value[[0, 0]], value[[0, 1]], value[[1, 0]], value[[1, 1]])
        }
    }
}
