//! generic, auth-agnostic HTTP engine: request construction, retry/backoff
//! on transient failures, JSON (de)serialization, and resumable streaming
//! downloads.

use crate::auth::AuthStrategy;
use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::header::{HeaderMap, HeaderValue, RANGE, USER_AGENT};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub user_agent: String,
    pub timeout: Duration,
    /// maximum number of attempts for a retriable failure (transient network
    /// errors, 429/5xx responses, or an interrupted download chunk).
    pub retry_max: usize,
    /// delay between retries. Kept as a flat delay (matching your existing
    /// CDS client's `sleep_max`) rather than exponential backoff, since CDSE
    /// and Sentinel Hub rate limits are typically better handled by simply
    /// waiting a fixed, modest interval than by backing off aggressively
    pub retry_delay: Duration,
    pub verify_tls: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            user_agent: "anthrxd_copernicus_client".to_string(),
            timeout: Duration::from_secs(60),
            retry_max: 10,
            retry_delay: Duration::from_secs(5),
            verify_tls: true,
        }
    }
}

#[derive(Clone)]
pub struct HttpEngine {
    http: HttpClient,
    auth: Arc<dyn AuthStrategy>,
    retry_max: usize,
    retry_delay: Duration,
}

impl std::fmt::Debug for HttpEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpEngine")
            .field("retry_max", &self.retry_max)
            .field("retry_delay", &self.retry_delay)
            .finish_non_exhaustive()
    }
}

impl HttpEngine {
    pub fn new(auth: Arc<dyn AuthStrategy>, config: EngineConfig) -> Result<Self> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&config.user_agent).context("invalid user agent string")?,
        );
        let http = HttpClient::builder()
            .default_headers(default_headers)
            .timeout(config.timeout)
            .danger_accept_invalid_certs(!config.verify_tls)
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            auth,
            retry_max: config.retry_max.max(1),
            retry_delay: config.retry_delay,
        })
    }

    /// escape hatch for callers that need the raw client
    pub fn http_client(&self) -> &HttpClient {
        &self.http
    }

    pub fn auth(&self) -> &Arc<dyn AuthStrategy> {
        &self.auth
    }

    /// send a JSON request and deserialize the response, retrying transient
    /// failures. `body` is ignored for `GET` requests.
    pub fn json_request<Req, Res>(
        &self,
        method: Method,
        url: &str,
        body: Option<&Req>,
    ) -> Result<Res>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let resp = self.retried(|| {
            let mut req = self.http.request(method.clone(), url);
            req = self.auth.apply(req)?;
            if method != Method::GET
                && let Some(b) = body
            {
                req = req.json(b);
            }
            req.send().map_err(Into::into)
        })?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!("API request failed: HTTP {status} for url ({url}): {text}");
        }
        serde_json::from_str(&text).with_context(|| {
            format!("failed to parse API JSON (url={url}, status={status}): {text}")
        })
    }

    /// GET raw bytes without assuming a JSON body (e.g. WMTS/WMS tiles)
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.retried(|| {
            let mut req = self.http.get(url);
            req = self.auth.apply(req)?;
            req.send().map_err(Into::into)
        })?;
        let resp = resp.error_for_status().context("GET request failed")?;
        Ok(resp
            .bytes()
            .context("failed to read response body")?
            .to_vec())
    }

    /// POST a JSON body and return the raw response bytes (e.g. the
    /// Sentinel Hub Process API, which takes a JSON request and returns an
    /// image in the format requested by the `output.responses` field).
    pub fn post_bytes<Req: Serialize>(&self, url: &str, body: &Req) -> Result<Vec<u8>> {
        let resp = self.retried(|| {
            let mut req = self.http.post(url).json(body);
            req = self.auth.apply(req)?;
            req.send().map_err(Into::into)
        })?;
        let resp = resp.error_for_status().context("POST request failed")?;
        Ok(resp
            .bytes()
            .context("failed to read response body")?
            .to_vec())
    }

    /// Stream a download to `writer`, resuming via `Range` on transient
    /// interruption, up to `retry_max` attempts total.
    ///
    /// `expected_size`, if known, is used to detect a short read and retry it as well,
    /// passing `None` results in this detections being skipped
    pub fn download<W: Write>(
        &self,
        url: &str,
        writer: &mut W,
        expected_size: Option<u64>,
    ) -> Result<u64> {
        let mut downloaded: u64 = 0;
        let mut tries = 0usize;

        'download_attempt: while tries < self.retry_max {
            let mut headers = HeaderMap::new();
            if downloaded > 0 {
                headers.insert(
                    RANGE,
                    HeaderValue::from_str(&format!("bytes={downloaded}-"))?,
                );
            }
            let resp = self.retried(|| {
                let mut req = self.http.get(url).headers(headers.clone());
                req = self.auth.apply(req)?;
                req.send().map_err(Into::into)
            })?;
            let mut resp = resp.error_for_status().context("download request failed")?;

            let mut buf = [0u8; 64 * 1024];
            loop {
                match resp.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        writer.write_all(&buf[..n])?;
                        downloaded += n as u64;
                    }
                    Err(e) => {
                        tries += 1;
                        if tries >= self.retry_max {
                            return Err(e).context("download interrupted");
                        }
                        writer.flush().ok();
                        thread::sleep(self.retry_delay);
                        continue 'download_attempt;
                    }
                }
            }
            writer.flush()?;

            match expected_size {
                Some(size) if downloaded < size => {
                    tries += 1;
                    thread::sleep(self.retry_delay);
                    continue;
                }
                _ => return Ok(downloaded),
            }
        }
        bail!("download failed: downloaded {downloaded} byte(s), retries exhausted");
    }

    /// runs closure `f`, retrying on transient network errors or retriable HTTP
    /// status codes, up to `retry_max` attempts.
    fn retried<F>(&self, mut f: F) -> Result<Response>
    where
        F: FnMut() -> Result<Response>,
    {
        let mut tries = 0usize;
        loop {
            match f() {
                Ok(resp) => {
                    if is_status_retriable(resp.status()) {
                        tries += 1;
                        if tries >= self.retry_max {
                            return Ok(resp);
                        }
                        thread::sleep(self.retry_delay);
                        continue;
                    }
                    return Ok(resp);
                }
                Err(err) => {
                    tries += 1;
                    if tries >= self.retry_max {
                        return Err(err).context("request failed after retries");
                    }
                    thread::sleep(self.retry_delay);
                }
            }
        }
    }
}

/// checks whether a status code represents a possibly-temporary failure
/// worth retrying
fn is_status_retriable(code: StatusCode) -> bool {
    matches!(
        code,
        StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::REQUEST_TIMEOUT
    )
}
