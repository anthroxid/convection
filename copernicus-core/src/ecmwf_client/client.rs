use crate::ecmwf_client::types::{ECMWFFile, JobStatus, ProcessingJob, ProcessingJobStatus};
use crate::{ApiKeyAuth, EngineConfig, HttpEngine};
use anyhow::{Result, anyhow, bail};
use log::{debug, error, info};
use reqwest::Method;
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const DEBUG_COPERNICUS_API_KEY_ENV_VAR: &str = "LOCAL_DEBUG_COPERNICUS_API_KEY";

#[derive(Clone)]
pub struct EcmwfClient {
    engine: HttpEngine,
    url: String,
    poll_sleep_max: Duration,
}

const DEFAULT_BASE_URL: &str = "https://cds.climate.copernicus.eu/api";

impl EcmwfClient {
    pub fn key(key: impl Into<String>) -> Result<Self> {
        Self::with_config(key, EngineConfig::default())
    }

    pub fn with_config(key: impl Into<String>, config: EngineConfig) -> Result<Self> {
        let auth = Arc::new(ApiKeyAuth::new(key));
        let engine = HttpEngine::new(auth, config)?;
        Ok(Self {
            engine,
            url: DEFAULT_BASE_URL.to_string(),
            poll_sleep_max: Duration::from_secs(120),
        })
    }

    /// Builder-style method to point at a non-default CDS/ADS base URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Builder-style method to change the job-status polling backoff
    /// ceiling (default 120s)
    pub fn with_poll_sleep_max(mut self, poll_sleep_max: Duration) -> Self {
        self.poll_sleep_max = poll_sleep_max;
        self
    }

    /// submits a request and returns the resulting ECMWFFile.
    ///
    /// notes on blocking: this function retries the processing job until a
    /// timeout is reached or the max retries.
    pub fn retrieve<T>(&self, dataset: &str, request: &T) -> Result<ECMWFFile>
    where
        T: Serialize,
    {
        let base = self.url.trim_end_matches('/');
        let retrieve_base = format!("{base}/retrieve/v1");
        let exec_url = format!("{retrieve_base}/processes/{dataset}/execution");
        let submit_body = serde_json::json!({ "inputs": request });
        info!("submitting a processing job for dataset {dataset}");
        let job: ProcessingJob =
            self.engine
                .json_request(Method::POST, &exec_url, Some(&submit_body))?;
        let monitor_url = job
            .monitor_url()
            .or_else(|| {
                job.job_id
                    .as_deref()
                    .map(|id| format!("{retrieve_base}/jobs/{id}"))
            })
            .ok_or_else(|| anyhow!("missing monitor link in job submission response"))?;
        info!(
            "job {} accepted, monitoring at {monitor_url}",
            job.job_id.as_deref().unwrap_or("with no id"),
        );

        let submitted = Instant::now();
        let mut sleep = Duration::from_secs(1);
        let mut last_status: Option<JobStatus> = None;
        loop {
            let status_url = append_query(&monitor_url, &[("log", "true"), ("request", "true")]);
            let job_status: ProcessingJobStatus =
                self.engine
                    .json_request::<(), _>(Method::GET, &status_url, None)?;
            if last_status != Some(job_status.status) {
                info!(
                    "job is {:?} after {:?}",
                    job_status.status,
                    submitted.elapsed()
                );
                last_status = Some(job_status.status);
            }
            match job_status.status {
                JobStatus::SuccessFul => {
                    let results_url = job_status.results_url().unwrap_or_else(|| {
                        format!("{}/results", monitor_url.trim_end_matches('/'))
                    });
                    info!(
                        "job finished after {:?}, fetching its result",
                        submitted.elapsed()
                    );
                    return self
                        .engine
                        .json_request::<(), _>(Method::GET, &results_url, None);
                }
                JobStatus::Accepted | JobStatus::Running => {
                    debug!(
                        "job still {:?}, polling again in {sleep:?}",
                        job_status.status
                    );
                    thread::sleep(sleep);
                    let next = Duration::from_secs_f64((sleep.as_secs_f64() * 1.5).max(1.0));
                    sleep = next.min(self.poll_sleep_max);
                }
                JobStatus::Failed
                | JobStatus::Rejected
                | JobStatus::Dismissed
                | JobStatus::Deleted => {
                    error!(
                        "job ended as {:?} after {:?}",
                        job_status.status,
                        submitted.elapsed()
                    );
                    bail!("processing failed with status {:?}", job_status.status);
                }
                other => {
                    error!("job reported the unknown status {other:?}");
                    bail!("unknown processing status [{other:?}]")
                }
            }
        }
    }

    /// downloads the given ECMWFFile, returning the amount of written bytes
    /// while writing them to the buffer in the writer impl [`W`]
    ///
    /// note that depending on the input `data_format`, it will result in data output
    /// for one of the variants found in [`ecmwf_client::types::DataFormat`], it is up
    /// to the user to correclty interpret the resulting bytes
    pub fn download<W>(&self, file: &ECMWFFile, writer: &mut W) -> Result<u64>
    where
        W: Write,
    {
        info!(
            "downloading {} ({} bytes)",
            file.location(),
            file.file_size()
        );
        self.engine
            .download(&file.location(), writer, Some(file.file_size()))
    }
}

fn append_query(url: &str, params: &[(&str, &str)]) -> String {
    let mut out = url.to_string();
    let sep = if url.contains('?') { '&' } else { '?' };
    out.push(sep);
    let mut first = true;
    for (k, v) in params {
        if !first {
            out.push('&');
        }
        first = false;
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{
        ecmwf_client::{
            client::{DEBUG_COPERNICUS_API_KEY_ENV_VAR, EcmwfClient},
            types::{DataFormat, DownloadFormat, ERA5Day, ERA5Month, ERA5Time, Query},
        },
        era5_year,
    };
    use std::{fs::File, io::BufWriter};
    #[test]
    #[ignore = "requires a local API key to be set"]
    fn try_download_file() -> anyhow::Result<()> {
        let client = EcmwfClient::key(std::env::var(DEBUG_COPERNICUS_API_KEY_ENV_VAR)?)?;
        // can also be done using serde_json::json macro, but this looks nicer
        let query = Query::builder()
            .add_array("product_type", ["reanalysis"])
            .add_array("variable", ["temperature"])
            .add_array("year", [era5_year!(2022)])
            .add_array("month", [ERA5Month::Jan])
            .add_array("day", [ERA5Day::D01])
            .add_array("time", [ERA5Time::T00])
            .add_array("pressure_level", vec!["1000"])
            .add("data_format", DataFormat::Grib)
            .add("download_format", DownloadFormat::Unarchived)
            .build();
        let processed_file = client.retrieve("reanalysis-era5-pressure-levels", &query)?;
        println!("{processed_file:#?}");
        // download to file, but can also write to in-memory buffer
        let file = File::create("../test_data/try_load_file_test.grib")?;
        let mut writer = BufWriter::new(file);
        let len = client.download(&processed_file, &mut writer)?;
        println!("downloaded {len} bytes");
        Ok(())
    }
}
