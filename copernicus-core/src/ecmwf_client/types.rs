use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ProcessingLink {
    #[serde(default)]
    rel: Option<String>,
    href: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProcessingJob {
    #[serde(default, alias = "jobID")]
    pub(crate) job_id: Option<String>,
    #[serde(default)]
    links: Vec<ProcessingLink>,
}

impl ProcessingJob {
    pub(crate) fn monitor_url(&self) -> Option<String> {
        self.links
            .iter()
            .find(|l| l.rel.as_deref() == Some("monitor"))
            .map(|l| l.href.clone())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProcessingJobStatus {
    pub(crate) status: JobStatus,
    #[serde(default)]
    links: Vec<ProcessingLink>,
}

#[derive(Clone, Copy, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum JobStatus {
    SuccessFul,
    Accepted,
    Running,
    Failed,
    Rejected,
    Dismissed,
    Deleted,
    #[serde(other)]
    Unknown,
}

impl ProcessingJobStatus {
    pub(crate) fn results_url(&self) -> Option<String> {
        self.links
            .iter()
            .find(|l| l.rel.as_deref() == Some("results"))
            .map(|l| l.href.clone())
    }
}

#[derive(Debug, Deserialize)]
pub struct ECMWFFile {
    asset: ProcessingAsset,
}

impl ECMWFFile {
    pub fn file_size(&self) -> u64 {
        self.asset.value.file_size
    }

    pub fn location(&self) -> String {
        self.asset.value.href.clone()
    }

    pub fn content_type(&self) -> String {
        self.asset.value.content_type.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct ProcessingAsset {
    value: ProcessingAssetValue,
}

#[derive(Debug, Deserialize)]
pub struct ProcessingAssetValue {
    href: String,
    #[serde(rename = "file:size")]
    file_size: u64,
    #[serde(rename = "type")]
    content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ERA5Month {
    #[serde(rename = "01")]
    Jan,
    #[serde(rename = "02")]
    Feb,
    #[serde(rename = "03")]
    Mar,
    #[serde(rename = "04")]
    Apr,
    #[serde(rename = "05")]
    May,
    #[serde(rename = "06")]
    Jun,
    #[serde(rename = "07")]
    Jul,
    #[serde(rename = "08")]
    Aug,
    #[serde(rename = "09")]
    Sep,
    #[serde(rename = "10")]
    Oct,
    #[serde(rename = "11")]
    Nov,
    #[serde(rename = "12")]
    Dec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ERA5Day {
    #[serde(rename = "01")]
    D01,
    #[serde(rename = "02")]
    D02,
    #[serde(rename = "03")]
    D03,
    #[serde(rename = "04")]
    D04,
    #[serde(rename = "05")]
    D05,
    #[serde(rename = "06")]
    D06,
    #[serde(rename = "07")]
    D07,
    #[serde(rename = "08")]
    D08,
    #[serde(rename = "09")]
    D09,
    #[serde(rename = "10")]
    D10,
    #[serde(rename = "11")]
    D11,
    #[serde(rename = "12")]
    D12,
    #[serde(rename = "13")]
    D13,
    #[serde(rename = "14")]
    D14,
    #[serde(rename = "15")]
    D15,
    #[serde(rename = "16")]
    D16,
    #[serde(rename = "17")]
    D17,
    #[serde(rename = "18")]
    D18,
    #[serde(rename = "19")]
    D19,
    #[serde(rename = "20")]
    D20,
    #[serde(rename = "21")]
    D21,
    #[serde(rename = "22")]
    D22,
    #[serde(rename = "23")]
    D23,
    #[serde(rename = "24")]
    D24,
    #[serde(rename = "25")]
    D25,
    #[serde(rename = "26")]
    D26,
    #[serde(rename = "27")]
    D27,
    #[serde(rename = "28")]
    D28,
    #[serde(rename = "29")]
    D29,
    #[serde(rename = "30")]
    D30,
    #[serde(rename = "31")]
    D31,
}

/// Hourly time slots
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ERA5Time {
    #[serde(rename = "00:00")]
    T00,
    #[serde(rename = "01:00")]
    T01,
    #[serde(rename = "02:00")]
    T02,
    #[serde(rename = "03:00")]
    T03,
    #[serde(rename = "04:00")]
    T04,
    #[serde(rename = "05:00")]
    T05,
    #[serde(rename = "06:00")]
    T06,
    #[serde(rename = "07:00")]
    T07,
    #[serde(rename = "08:00")]
    T08,
    #[serde(rename = "09:00")]
    T09,
    #[serde(rename = "10:00")]
    T10,
    #[serde(rename = "11:00")]
    T11,
    #[serde(rename = "12:00")]
    T12,
    #[serde(rename = "13:00")]
    T13,
    #[serde(rename = "14:00")]
    T14,
    #[serde(rename = "15:00")]
    T15,
    #[serde(rename = "16:00")]
    T16,
    #[serde(rename = "17:00")]
    T17,
    #[serde(rename = "18:00")]
    T18,
    #[serde(rename = "19:00")]
    T19,
    #[serde(rename = "20:00")]
    T20,
    #[serde(rename = "21:00")]
    T21,
    #[serde(rename = "22:00")]
    T22,
    #[serde(rename = "23:00")]
    T23,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERA5Year(u16);

impl ERA5Year {
    /// ERA5 data goes back until 1940
    pub const EARLIEST: u16 = 1940;

    pub fn new(year: u16) -> Result<Self, InvalidERA5Year> {
        if year < Self::EARLIEST {
            Err(InvalidERA5Year(year))
        } else {
            Ok(ERA5Year(year))
        }
    }

    /// Safely construct a const ERA5Year with compile time boundary check
    pub const fn new_const(year: u16) -> Self {
        assert!(year >= Self::EARLIEST, "Year must be >= 1940");
        Self(year)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidERA5Year(pub u16);

impl std::fmt::Display for InvalidERA5Year {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "year {} predates ERA5 coverage (starts {})",
            self.0,
            ERA5Year::EARLIEST
        )
    }
}

impl std::error::Error for InvalidERA5Year {}

impl Serialize for ERA5Year {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ERA5Year {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let year: u16 = s.parse().map_err(serde::de::Error::custom)?;
        ERA5Year::new(year).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataFormat {
    Grib,
    NetCDF,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadFormat {
    Unarchived,
    Zip,
}

/// A flexible query builder that serializes to a flat JSON object
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    params: HashMap<String, serde_json::Value>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add<T: Serialize>(mut self, key: impl Into<String>, value: T) -> Self {
        if let Ok(json_value) = serde_json::to_value(&value) {
            self.params.insert(key.into(), json_value);
        }
        self
    }

    pub fn add_array<T: Serialize>(
        mut self,
        key: impl Into<String>,
        values: impl IntoIterator<Item = T>,
    ) -> Self {
        let array: Vec<_> = values
            .into_iter()
            .filter_map(|v| serde_json::to_value(&v).ok())
            .collect();
        self.params
            .insert(key.into(), serde_json::Value::Array(array));
        self
    }

    pub fn build(self) -> Query {
        Query {
            params: self.params,
        }
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    params: HashMap<String, serde_json::Value>,
}

impl Query {
    pub fn builder() -> QueryBuilder {
        QueryBuilder::new()
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.params.get(key)
    }
}

impl Serialize for Query {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.params.serialize(serializer)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_era5_year() {
        let y1 = ERA5Year::new(1931);
        assert_eq!(y1, Err(InvalidERA5Year(1931)));

        let y2 = ERA5Year::new(2003);
        assert!(y2.is_ok());
    }

    #[test]
    fn test_builder() {
        let query = QueryBuilder::new()
            .add_array("product_type", vec!["reanalysis"])
            .add_array("variable", vec!["temperature"])
            .add("data_format", "Grib")
            .add("download_format", "Unarchived")
            .build();

        let json = serde_json::to_value(&query).unwrap();
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
    }
}
