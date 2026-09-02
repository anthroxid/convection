//! perspective camera and view-frustum geometry, in globe-centered meters

use glam::{DMat4, DVec3, DVec4};
use typed_builder::TypedBuilder;

use crate::{Distance, Globe};

/// perspective camera looking at a point in globe-centered coordinates
#[derive(Clone, Copy, Debug, TypedBuilder)]
pub struct Camera {
    pub position: DVec3,
    /// the point being looked at, the globe's center by default
    #[builder(default = DVec3::ZERO)]
    pub target: DVec3,
    /// world up hint, orthogonal against the view direction
    #[builder(default = DVec3::Y)]
    pub up: DVec3,
    pub fov_y_rad: f64,
    pub viewport_width_px: u32,
    pub viewport_height_px: u32,
}

impl Camera {
    pub fn forward(&self) -> DVec3 {
        let d = self.target - self.position;
        if d.length_squared() > 0.0 {
            d.normalize()
        } else {
            -DVec3::Z
        }
    }

    pub fn aspect(&self) -> f64 {
        self.viewport_width_px.max(1) as f64 / self.viewport_height_px.max(1) as f64
    }

    pub fn altitude(&self, globe: &Globe) -> Distance {
        Distance::meters(self.position.length()) - globe.radius
    }

    pub fn focal_length_px(&self) -> f64 {
        self.viewport_height_px as f64 / (2.0 * (self.fov_y_rad / 2.0).tan())
    }

    pub fn projected_px(&self, size: f64, distance: f64) -> f64 {
        size * self.focal_length_px() / distance.max(f64::MIN_POSITIVE)
    }

    pub fn depth_range(&self, globe: &Globe) -> (f64, f64) {
        let center_dist = self.position.length();
        let radius = globe.radius.as_meters();
        let altitude = (center_dist - radius).max(1.0);
        (altitude * 0.5, center_dist + radius)
    }

    pub fn view_rotation(&self) -> DMat4 {
        glam::dcamera::rh::view::look_to_mat4(DVec3::ZERO, self.forward(), self.up)
    }

    /// projection for wgpu's NDC (z in [0, 1], y up)
    pub fn projection(&self, near: f64, far: f64) -> DMat4 {
        glam::dcamera::rh::proj::directx::perspective(self.fov_y_rad, self.aspect(), near, far)
    }

    /// combined transform for eye-relative geometry, see [`Camera::view_rotation`]
    pub fn eye_relative_view_projection(&self, near: f64, far: f64) -> DMat4 {
        self.projection(near, far) * self.view_rotation()
    }

    pub fn frustum(&self, near: f64, far: f64) -> Frustum {
        Frustum::new(self, near, far)
    }
}

/// the six bounding planes of a camera's view volume, in world space
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    /// `(normal, distance)` as `xyz` and `w`
    planes: [DVec4; 6],
}

impl Frustum {
    pub fn new(camera: &Camera, near: f64, far: f64) -> Self {
        let forward = camera.forward();
        let right = forward.cross(camera.up).normalize();
        let up = right.cross(forward);

        let tan_half_v = (camera.fov_y_rad / 2.0).tan();
        let tan_half_h = tan_half_v * camera.aspect();

        let plane = |normal: DVec3, point: DVec3| {
            let n = normal.normalize();
            DVec4::new(n.x, n.y, n.z, -n.dot(point))
        };

        let eye = camera.position;
        Self {
            planes: [
                plane(forward, eye + forward * near),
                plane(-forward, eye + forward * far),
                plane(up.cross(forward + right * tan_half_h), eye),
                plane((forward - right * tan_half_h).cross(up), eye),
                plane((forward + up * tan_half_v).cross(right), eye),
                plane(right.cross(forward - up * tan_half_v), eye),
            ],
        }
    }

    /// whether any part of the sphere is inside the view volume
    pub fn intersects_sphere(&self, center: DVec3, radius: f64) -> bool {
        self.planes
            .iter()
            .all(|p| p.x * center.x + p.y * center.y + p.z * center.z + p.w >= -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera::builder()
            .position(DVec3::new(0.0, 0.0, 100.0))
            .target(DVec3::ZERO)
            .fov_y_rad(90.0_f64.to_radians())
            .viewport_width_px(100)
            .viewport_height_px(100)
            .build()
    }

    #[test]
    fn frustum_keeps_what_is_in_front_and_drops_what_is_behind() {
        let f = camera().frustum(1.0, 1000.0);
        assert!(f.intersects_sphere(DVec3::ZERO, 1.0));
        assert!(f.intersects_sphere(DVec3::new(0.0, 0.0, 90.0), 1.0));
        assert!(!f.intersects_sphere(DVec3::new(0.0, 0.0, 200.0), 1.0));
        assert!(!f.intersects_sphere(DVec3::new(0.0, 0.0, -1000.0), 1.0));
    }

    #[test]
    fn frustum_widens_with_distance() {
        let f = camera().frustum(1.0, 1000.0);
        assert!(f.intersects_sphere(DVec3::new(45.0, 0.0, 50.0), 1.0));
        assert!(!f.intersects_sphere(DVec3::new(60.0, 0.0, 50.0), 1.0));
        assert!(!f.intersects_sphere(DVec3::new(45.0, 0.0, 95.0), 1.0));
    }

    #[test]
    fn frustum_respects_aspect_ratio() {
        let wide = Camera::builder()
            .position(DVec3::new(0.0, 0.0, 100.0))
            .fov_y_rad(90.0_f64.to_radians())
            .viewport_width_px(200)
            .viewport_height_px(100)
            .build()
            .frustum(1.0, 1000.0);
        assert!(wide.intersects_sphere(DVec3::new(90.0, 0.0, 50.0), 1.0));
        assert!(!wide.intersects_sphere(DVec3::new(0.0, 90.0, 50.0), 1.0));
    }

    #[test]
    fn projected_size_halves_with_doubled_distance() {
        let c = camera();
        let near = c.projected_px(1.0, 10.0);
        let far = c.projected_px(1.0, 20.0);
        assert!((near / far - 2.0).abs() < 1e-9);
        assert!((c.focal_length_px() - 50.0).abs() < 1e-9);
    }
}
