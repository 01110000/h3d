//! Procedural 3D Gaussian Splatting scene: a white torus.

use glam::{Mat3, Vec3};

/// GPU instance layout of a single gaussian.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Splat {
    pub center: [f32; 3],
    /// Upper triangle of the 3D covariance: xx, xy, xz.
    pub cov_a: [f32; 3],
    /// Upper triangle of the 3D covariance: yy, yz, zz.
    pub cov_b: [f32; 3],
    /// Linear RGB + opacity.
    pub color: [f32; 4],
}

pub const MAJOR_RADIUS: f32 = 1.0;
pub const MINOR_RADIUS: f32 = 0.4;
pub const SPLAT_COUNT: usize = 40_000;

/// Small deterministic PRNG so the scene is identical on every platform.
struct XorShift(u64);

impl XorShift {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Samples gaussians uniformly over the torus surface. Each gaussian is a flat
/// disc lying in the tangent plane; lighting is baked into its colour, as in a
/// captured 3DGS scene.
pub fn white_torus() -> Vec<Splat> {
    let (big_r, small_r) = (MAJOR_RADIUS, MINOR_RADIUS);
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let area = 4.0 * std::f32::consts::PI.powi(2) * big_r * small_r;
    let spacing = (area / SPLAT_COUNT as f32).sqrt();
    let light = Vec3::new(0.45, 0.8, 0.4).normalize();
    let fill = Vec3::new(-0.5, -0.3, -0.6).normalize();
    let tau = std::f32::consts::TAU;

    let mut splats = Vec::with_capacity(SPLAT_COUNT);
    while splats.len() < SPLAT_COUNT {
        let u = rng.next_f32() * tau;
        let v = rng.next_f32() * tau;
        // Rejection sampling so the density is uniform in surface area.
        if rng.next_f32() * (big_r + small_r) > big_r + small_r * v.cos() {
            continue;
        }

        let (su, cu) = u.sin_cos();
        let (sv, cv) = v.sin_cos();
        let ring = big_r + small_r * cv;
        let center = Vec3::new(ring * cu, small_r * sv, ring * su);
        let normal = Vec3::new(cv * cu, sv, cv * su);
        let tangent_u = Vec3::new(-su, 0.0, cu);
        let tangent_v = normal.cross(tangent_u);

        let rot = Mat3::from_cols(tangent_u, tangent_v, normal);
        let jitter = 0.8 + 0.4 * rng.next_f32();
        let s_t = spacing * jitter;
        let scale = Mat3::from_diagonal(Vec3::new(s_t, s_t, spacing * 0.08));
        let m = rot * scale;
        let cov = m * m.transpose();

        let diffuse = normal.dot(light).max(0.0);
        let back = normal.dot(fill).max(0.0);
        let shade = (0.42 + 0.58 * diffuse + 0.15 * back).min(1.0);

        splats.push(Splat {
            center: center.into(),
            cov_a: [cov.x_axis.x, cov.y_axis.x, cov.z_axis.x],
            cov_b: [cov.y_axis.y, cov.z_axis.y, cov.z_axis.z],
            color: [shade, shade, shade, 0.95],
        });
    }
    splats
}
