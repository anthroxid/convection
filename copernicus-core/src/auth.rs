use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client as HttpClient, RequestBuilder};
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// strategy for attaching credentials to an outgoing request.
pub trait AuthStrategy: Send + Sync {
    fn apply(&self, req: RequestBuilder) -> Result<RequestBuilder>;
}

/// static personal API key / token auth, e.g. CDS's `PRIVATE-TOKEN` header
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    key: String,
    header_name: &'static str,
}

impl ApiKeyAuth {
    /// defaults to the `PRIVATE-TOKEN` header used by the CDS/ADS "Processes"
    /// API. see [`ApiKeyAuth::with_header_name`] for services that expect the key
    /// under a different header name
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            header_name: "PRIVATE-TOKEN",
        }
    }

    pub fn with_header_name(mut self, header_name: &'static str) -> Self {
        self.header_name = header_name;
        self
    }
}

impl AuthStrategy for ApiKeyAuth {
    fn apply(&self, req: RequestBuilder) -> Result<RequestBuilder> {
        Ok(req.header(self.header_name, self.key.trim()))
    }
}

/// OAuth2 client credentials grant, used by the Copernicus Data Space
/// Ecosystem (Keycloak realm `CDSE`) and by Sentinel Hub
///
/// tokens are cached and refreshed automatically a little before they
/// expire
pub struct OAuth2ClientCredentials {
    token_url: String,
    client_id: String,
    client_secret: String,
    http: HttpClient,
    /// (access_token, expires_at)
    cached: Mutex<Option<(String, Instant)>>,
    /// refresh token safety margin
    refresh_margin: Duration,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

impl OAuth2ClientCredentials {
    pub fn new(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            http: HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build token-exchange HTTP client"),
            cached: Mutex::new(None),
            refresh_margin: Duration::from_secs(30),
        }
    }

    /// convenience constructor for the Copernicus Data Space Ecosystem's
    /// Keycloak realm
    pub fn cdse(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self::new(
            "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token",
            client_id,
            client_secret,
        )
    }

    /// Builder-style method to change the refresh safety margin (default 30s)
    pub fn with_refresh_margin(mut self, margin: Duration) -> Self {
        self.refresh_margin = margin;
        self
    }

    fn fetch_token(&self) -> Result<(String, Instant)> {
        let resp = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ])
            .send()
            .context("OAuth2 token request failed to send")?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!("OAuth2 token request failed: HTTP {status}: {text}");
        }

        let token: TokenResponse = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse OAuth2 token response: {text}"))?;

        let lifetime = Duration::from_secs(token.expires_in).saturating_sub(self.refresh_margin);
        let expiry = Instant::now() + lifetime;
        Ok((token.access_token, expiry))
    }

    fn get_or_refresh_token(&self) -> Result<String> {
        let mut guard = self.cached.lock().expect("token cache mutex poisoned");
        if let Some((token, expiry)) = guard.as_ref()
            && Instant::now() < *expiry
        {
            return Ok(token.clone());
        }
        let (token, expiry) = self.fetch_token()?;
        *guard = Some((token.clone(), expiry));
        Ok(token)
    }
}

impl AuthStrategy for OAuth2ClientCredentials {
    fn apply(&self, req: RequestBuilder) -> Result<RequestBuilder> {
        let token = self.get_or_refresh_token()?;
        Ok(req.bearer_auth(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_auth_defaults_to_private_token_header() {
        let auth = ApiKeyAuth::new("abc123");
        assert_eq!(auth.header_name, "PRIVATE-TOKEN");
        assert_eq!(auth.key, "abc123");
    }

    #[test]
    fn api_key_auth_header_override() {
        let auth = ApiKeyAuth::new("abc123").with_header_name("Authorization");
        assert_eq!(auth.header_name, "Authorization");
    }
}
