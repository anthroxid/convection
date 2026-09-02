/// `bbox` in WGS84 (lon/lat) degrees: `[west, south, east, max_lat]`.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BBox {
    inner: [f64; 4],
}

impl BBox {
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Self {
        Self {
            inner: [west, south, east, north],
        }
    }

    /// west, south, east, north
    pub const fn wsen(&self) -> [f64; 4] {
        self.inner
    }

    pub const fn west(&self) -> f64 {
        self.inner[0]
    }

    pub const fn south(&self) -> f64 {
        self.inner[1]
    }

    pub const fn east(&self) -> f64 {
        self.inner[2]
    }

    pub const fn north(&self) -> f64 {
        self.inner[3]
    }

    /// (lon, lat) of the box's center
    pub fn center(&self) -> (f64, f64) {
        (
            (self.west() + self.east()) / 2.0,
            (self.south() + self.north()) / 2.0,
        )
    }
}

impl From<(f64, f64, f64, f64)> for BBox {
    fn from((west, south, east, north): (f64, f64, f64, f64)) -> Self {
        Self::new(west, south, east, north)
    }
}

impl From<[f64; 4]> for BBox {
    fn from([west, south, east, north]: [f64; 4]) -> Self {
        Self::new(west, south, east, north)
    }
}

impl From<ndarray::Array2<f64>> for BBox {
    fn from(value: ndarray::Array2<f64>) -> Self {
        Self::new(value[[0, 0]], value[[0, 1]], value[[1, 0]], value[[1, 1]])
    }
}
