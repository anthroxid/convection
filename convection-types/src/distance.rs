//! internally every operation normalizes through meters

use std::cmp::Ordering;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

const METERS_PER_KM: f64 = 1000.0;
const METERS_PER_FOOT: f64 = 0.3048;
const METERS_PER_MILE: f64 = 1609.344;

#[derive(Debug, Clone, Copy)]
pub enum Distance {
    Meters(f64),
    Kilometers(f64),
    Feet(f64),
    Miles(f64),
}

impl Distance {
    pub const fn meters(v: f64) -> Self {
        Distance::Meters(v)
    }

    pub const fn kilometers(v: f64) -> Self {
        Distance::Kilometers(v)
    }

    pub const fn feet(v: f64) -> Self {
        Distance::Feet(v)
    }

    pub const fn miles(v: f64) -> Self {
        Distance::Miles(v)
    }

    pub const ZERO: Distance = Distance::Meters(0.0);

    /// the numeric value without the unit
    pub const fn value(&self) -> f64 {
        match *self {
            Distance::Meters(v)
            | Distance::Kilometers(v)
            | Distance::Feet(v)
            | Distance::Miles(v) => v,
        }
    }

    pub const fn unit_suffix(&self) -> &'static str {
        match self {
            Distance::Meters(_) => "m",
            Distance::Kilometers(_) => "km",
            Distance::Feet(_) => "ft",
            Distance::Miles(_) => "mi",
        }
    }

    pub const fn as_meters(&self) -> f64 {
        match *self {
            Distance::Meters(v) => v,
            Distance::Kilometers(v) => v * METERS_PER_KM,
            Distance::Feet(v) => v * METERS_PER_FOOT,
            Distance::Miles(v) => v * METERS_PER_MILE,
        }
    }

    pub const fn to_meters(&self) -> Distance {
        Distance::Meters(self.as_meters())
    }

    pub const fn to_kilometers(&self) -> Distance {
        Distance::Kilometers(self.as_meters() / METERS_PER_KM)
    }

    pub const fn to_feet(&self) -> Distance {
        Distance::Feet(self.as_meters() / METERS_PER_FOOT)
    }

    pub const fn to_miles(&self) -> Distance {
        Distance::Miles(self.as_meters() / METERS_PER_MILE)
    }

    pub const fn convert_to_unit_of(&self, other: &Distance) -> Distance {
        match other {
            Distance::Meters(_) => self.to_meters(),
            Distance::Kilometers(_) => self.to_kilometers(),
            Distance::Feet(_) => self.to_feet(),
            Distance::Miles(_) => self.to_miles(),
        }
    }

    const fn with_value(&self, v: f64) -> Distance {
        match self {
            Distance::Meters(_) => Distance::Meters(v),
            Distance::Kilometers(_) => Distance::Kilometers(v),
            Distance::Feet(_) => Distance::Feet(v),
            Distance::Miles(_) => Distance::Miles(v),
        }
    }

    pub const fn abs(&self) -> Distance {
        self.with_value(self.value().abs())
    }

    pub const fn is_zero(&self) -> bool {
        self.as_meters() == 0.0
    }

    pub const fn min(self, other: Distance) -> Distance {
        if self.as_meters() <= other.as_meters() {
            self
        } else {
            other
        }
    }

    pub const fn max(self, other: Distance) -> Distance {
        if self.as_meters() >= other.as_meters() {
            self
        } else {
            other
        }
    }
}

impl Default for Distance {
    fn default() -> Self {
        Distance::ZERO
    }
}

// impl eq/ord over meter as common denominator

impl PartialEq for Distance {
    fn eq(&self, other: &Self) -> bool {
        self.as_meters() == other.as_meters()
    }
}

impl PartialOrd for Distance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_meters().partial_cmp(&other.as_meters())
    }
}

impl fmt::Display for Distance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(precision) = f.precision() {
            write!(f, "{:.*} {}", precision, self.value(), self.unit_suffix())
        } else {
            write!(f, "{} {}", self.value(), self.unit_suffix())
        }
    }
}

impl Add for Distance {
    type Output = Distance;
    fn add(self, rhs: Distance) -> Distance {
        self.with_value(self.value() + rhs.convert_to_unit_of(&self).value())
    }
}

impl Sub for Distance {
    type Output = Distance;
    fn sub(self, rhs: Distance) -> Distance {
        self.with_value(self.value() - rhs.convert_to_unit_of(&self).value())
    }
}

impl Neg for Distance {
    type Output = Distance;
    fn neg(self) -> Distance {
        self.with_value(-self.value())
    }
}

impl AddAssign for Distance {
    fn add_assign(&mut self, rhs: Distance) {
        *self = *self + rhs;
    }
}

impl SubAssign for Distance {
    fn sub_assign(&mut self, rhs: Distance) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Distance {
    type Output = Distance;
    fn mul(self, rhs: f64) -> Distance {
        self.with_value(self.value() * rhs)
    }
}

impl Mul<Distance> for f64 {
    type Output = Distance;
    fn mul(self, rhs: Distance) -> Distance {
        rhs * self
    }
}

impl Div<f64> for Distance {
    type Output = Distance;
    fn div(self, rhs: f64) -> Distance {
        self.with_value(self.value() / rhs)
    }
}

impl MulAssign<f64> for Distance {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

impl Div<Distance> for Distance {
    type Output = f64;
    fn div(self, rhs: Distance) -> f64 {
        self.as_meters() / rhs.as_meters()
    }
}

impl Sum for Distance {
    fn sum<I: Iterator<Item = Distance>>(iter: I) -> Self {
        iter.fold(Distance::ZERO, |acc, d| acc + d)
    }
}

impl From<f64> for Distance {
    /// use meters as base as it's the SI unit
    fn from(v: f64) -> Self {
        Distance::Meters(v)
    }
}

impl From<Distance> for f64 {
    /// converting to f64 yields the distance in meters, as it's the SI unit
    fn from(d: Distance) -> Self {
        d.as_meters()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip() {
        let d = Distance::Miles(1.0);
        assert!((d.as_meters() - 1609.344).abs() < 1e-9);
        assert!((d.to_kilometers().value() - 1.609344).abs() < 1e-9);
    }

    #[test]
    fn addition_mixes_units_and_keeps_lhs_unit() {
        let a = Distance::Kilometers(1.0);
        let b = Distance::Meters(500.0);
        let sum = a + b;
        match sum {
            Distance::Kilometers(v) => assert!((v - 1.5).abs() < 1e-9),
            _ => panic!("expected Kilometers"),
        }
    }

    #[test]
    fn subtraction_and_negation() {
        let a = Distance::Meters(10.0);
        let b = Distance::Feet(3.28084);
        let diff = a - b;
        assert!((diff.as_meters() - 9.0).abs() < 1e-3);
        assert!((-a).value() == -10.0);
    }

    #[test]
    fn scaling_by_scalar() {
        let a = Distance::Meters(10.0);
        assert_eq!((a * 2.0).value(), 20.0);
        assert_eq!((2.0 * a).value(), 20.0);
        assert_eq!((a / 2.0).value(), 5.0);
    }

    #[test]
    fn ratio_of_two_distances_is_dimensionless() {
        let a = Distance::Kilometers(2.0);
        let b = Distance::Meters(500.0);
        assert!((a / b - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ordering_and_equality_cross_unit() {
        let a = Distance::Miles(1.0);
        let b = Distance::Meters(1609.344);
        assert_eq!(a, b);
        assert!(Distance::Kilometers(2.0) > Distance::Miles(1.0));
    }

    #[test]
    fn sum_over_iterator() {
        let total: Distance = vec![
            Distance::Meters(100.0),
            Distance::Kilometers(1.0),
            Distance::Feet(0.0),
        ]
        .into_iter()
        .sum();
        assert!((total.as_meters() - 1100.0).abs() < 1e-9);
    }

    #[test]
    fn display_formatting() {
        let d = Distance::Kilometers(5.0);
        assert_eq!(format!("{}", d), "5 km");
        let d2 = Distance::Meters(std::f64::consts::PI);
        assert_eq!(format!("{:.2}", d2), "3.14 m");
    }

    #[test]
    fn min_max_abs() {
        let a = Distance::Meters(-5.0);
        let b = Distance::Meters(3.0);
        assert_eq!(a.abs().value(), 5.0);
        assert_eq!(a.min(b), a);
        assert_eq!(a.max(b), b);
    }
}
