use std::f64::consts::PI;

pub const EARTH_CIRCUMFERENCE: f64 = 40_075_017.0;
pub const EARTH_MEAN_RADIUS: f64 = EARTH_CIRCUMFERENCE / (2. * PI);

/// `bbox` in WGS84 (lon/lat) degrees: `[min_lon, min_lat, max_lon, max_lat]`.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BBox {
    inner: [f64; 4],
}

impl BBox {
    /// also as: west, south, east, north
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
