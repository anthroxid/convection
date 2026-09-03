//! note that use of VecN or DVecN is meant to be taken in meters, so converting
//! it to `Distance::Meters` is the correct way to handle it. when in doubt, a
//! standalone f64 or f32 (or in the context of being in a vector) should be
//! taken as the SI unit "meters". if the code does not reflect this, make an
//! effort to correct it

use std::f64::consts::PI;

pub const EARTH_CIRCUMFERENCE: Distance = Distance::meters(40_075_017.0);
pub const EARTH_MEAN_RADIUS: Distance =
    Distance::meters(EARTH_CIRCUMFERENCE.as_meters() / (2. * PI));

mod bbox;
mod camera;
mod distance;

pub use bbox::*;
pub use camera::*;
pub use distance::*;
use glam::DVec3;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Globe {
    pub radius: Distance,
}

impl Globe {
    pub fn earth() -> Self {
        Self {
            radius: EARTH_MEAN_RADIUS,
        }
    }
}

impl Default for Globe {
    fn default() -> Self {
        Self::earth()
    }
}

impl Globe {
    /// point on the sphere for a given lon/lat, in degrees.
    pub fn lonlat_to_point(&self, lon_deg: f64, lat_deg: f64) -> DVec3 {
        let lon = lon_deg.to_radians();
        let lat = lat_deg.to_radians();
        let r = self.radius.as_meters();
        DVec3::new(
            r * lat.cos() * lon.cos(),
            r * lat.sin(),
            -r * lat.cos() * lon.sin(),
        )
    }

    /// surface normal at a lon/lat
    pub fn normal_at(&self, lon_deg: f64, lat_deg: f64) -> DVec3 {
        self.lonlat_to_point(lon_deg, lat_deg).normalize()
    }

    /// whether a point on the surface faces an eye above the surface, i.e. is
    /// on the near side of the horizon circle. points on the sphere satisfy
    /// `dot(point, eye) == radius^2` exactly on the horizon
    pub fn is_above_horizon(&self, point: DVec3, eye: DVec3) -> bool {
        let r = self.radius.as_meters();
        point.dot(eye) >= r * r
    }
}
