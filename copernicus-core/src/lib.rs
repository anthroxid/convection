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
}
