use std::f32::consts::PI;

use ultraviolet::{projection, Mat4, Vec3};
#[cfg(target_arch = "wasm32")]
use wgpu::util::DeviceExt;

#[cfg(target_arch = "wasm32")]
use crate::renderer::frame_data::UniformResource;

/// A camera matrix cannot produce a safe, meaningful frustum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrustumError {
    #[error("frustum plane {plane} contains a non-finite component")]
    NonFinite { plane: usize },
    #[error("frustum plane {plane} has a near-degenerate normal")]
    Degenerate { plane: usize },
}

const MIN_DISTANCE: f32 = 0.1;

/// SIMD-width shared camera row: eye, target, up, then projection parameters.
pub type SharedCameraState = [f32; 16];

#[repr(C)]
pub struct Camera {
    // Hot data - cached computed matrix (64 bytes, 1 cache line)
    pub view_proj: [[f32; 4]; 4],

    // Warm data - frequently accessed vectors (36 bytes)
    position: Vec3,
    target: Vec3,
    up: Vec3,

    // Cold data - projection parameters (16 bytes)
    fov: f32,
    aspect_ratio: f32,
    z_near: f32,
    z_far: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl Camera {
    pub fn frustum_planes(&self) -> Result<[[f32; 4]; 6], FrustumError> {
        extract_frustum_planes(self.view_proj)
    }
    pub fn new(aspect_ratio: f32) -> Self {
        let mut camera = Camera {
            view_proj: [[0.0; 4]; 4],
            position: Vec3::new(0.0, 0.5, 3.0),
            target: Vec3::new(0.0, 0.0, 0.0),
            up: Vec3::unit_y(),
            fov: PI / 3.0,
            aspect_ratio,
            z_near: 0.1,
            z_far: 100000.0,
        };

        camera.compute_view_proj_mat();

        camera
    }

    pub fn compute_view_proj_mat(&mut self) {
        let view = Mat4::look_at(self.position, self.target, self.up);
        let proj = projection::rh_yup::perspective_wgpu_dx(
            self.fov,
            self.aspect_ratio,
            self.z_near,
            self.z_far,
        );
        self.view_proj = (proj * view).into();
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn update_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = aspect_ratio;
        self.compute_view_proj_mat();
    }

    /// Snapshot the canonical 64-byte shared row.
    pub fn shared_state(&self) -> SharedCameraState {
        [
            self.position.x,
            self.position.y,
            self.position.z,
            1.0,
            self.target.x,
            self.target.y,
            self.target.z,
            1.0,
            self.up.x,
            self.up.y,
            self.up.z,
            0.0,
            self.fov,
            self.aspect_ratio,
            self.z_near,
            self.z_far,
        ]
    }

    /// Apply a complete shared row, rejecting malformed external writes.
    pub fn apply_shared_state(&mut self, state: SharedCameraState) -> bool {
        if !state.iter().all(|value| value.is_finite())
            || !(0.0..PI).contains(&state[12])
            || state[13] <= 0.0
            || state[14] <= 0.0
            || state[15] <= state[14]
        {
            return false;
        }
        let position = Vec3::new(state[0], state[1], state[2]);
        let target = Vec3::new(state[4], state[5], state[6]);
        let up = Vec3::new(state[8], state[9], state[10]);
        let forward = target - position;
        if forward.mag_sq() < MIN_DISTANCE * MIN_DISTANCE
            || up.mag_sq() <= f32::EPSILON
            || forward.cross(up).mag_sq() <= f32::EPSILON
        {
            return false;
        }
        self.position = position;
        self.target = target;
        self.up = up.normalized();
        self.fov = state[12];
        self.aspect_ratio = state[13];
        self.z_near = state[14];
        self.z_far = state[15];
        self.compute_view_proj_mat();
        true
    }

    #[cfg(target_arch = "wasm32")]
    pub fn create_uniform_resource(&self, device: &wgpu::Device) -> UniformResource {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: "camera uniform buffer".into(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[self.view_proj]),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        UniformResource {
            buffer,
            bind_group,
            bind_group_layout,
        }
    }
}

/// Extracts inward-facing normalized WebGPU clip-space planes (zero-to-one depth).
pub fn extract_frustum_planes(m: [[f32; 4]; 4]) -> Result<[[f32; 4]; 6], FrustumError> {
    let row = |r: usize| [m[0][r], m[1][r], m[2][r], m[3][r]];
    let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    let r0 = row(0);
    let r1 = row(1);
    let r2 = row(2);
    let r3 = row(3);
    let mut planes = [
        add(r3, r0),
        sub(r3, r0),
        add(r3, r1),
        sub(r3, r1),
        r2,
        sub(r3, r2),
    ];
    for (plane, p) in planes.iter_mut().enumerate() {
        if !p.iter().all(|component| component.is_finite()) {
            return Err(FrustumError::NonFinite { plane });
        }
        // Scale first: directly squaring very large/small coefficients can overflow or
        // underflow even though the plane itself is normalizable.
        let scale = p[0].abs().max(p[1].abs()).max(p[2].abs());
        if scale < f32::MIN_POSITIVE {
            return Err(FrustumError::Degenerate { plane });
        }
        let scaled = [p[0] / scale, p[1] / scale, p[2] / scale];
        let length = (scaled[0] * scaled[0] + scaled[1] * scaled[1] + scaled[2] * scaled[2]).sqrt();
        for v in p {
            *v = (*v / scale) / length;
            if !v.is_finite() {
                return Err(FrustumError::NonFinite { plane });
            }
        }
    }
    Ok(planes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_extraction_rejects_nonfinite_and_degenerate_planes() {
        let mut nonfinite = Camera::new(1.0).view_proj;
        nonfinite[0][0] = f32::NAN;
        assert!(matches!(
            extract_frustum_planes(nonfinite),
            Err(FrustumError::NonFinite { .. })
        ));
        assert!(matches!(
            extract_frustum_planes([[0.0; 4]; 4]),
            Err(FrustumError::Degenerate { .. })
        ));
    }

    #[test]
    fn frustum_extraction_normalizes_without_overflow() {
        let mut matrix = Camera::new(1.0).view_proj;
        for value in matrix.iter_mut().flatten() {
            *value *= 1.0e20;
        }
        let planes = extract_frustum_planes(matrix).unwrap();
        for plane in planes {
            let length = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
            assert!((length - 1.0).abs() < 1.0e-5);
            assert!(plane.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn shared_state_round_trips_and_rejects_invalid_projection() {
        let mut camera = Camera::new(1.0);
        let state = [
            2.0,
            3.0,
            10.0,
            1.0,
            1.0,
            -1.0,
            0.5,
            1.0,
            0.0,
            1.0,
            0.0,
            0.0,
            PI / 4.0,
            16.0 / 9.0,
            0.25,
            500.0,
        ];
        assert!(camera.apply_shared_state(state));
        assert_eq!(camera.shared_state(), state);
        assert!(camera
            .view_proj
            .iter()
            .flatten()
            .all(|component| component.is_finite()));

        let mut invalid = state;
        invalid[15] = invalid[14];
        assert!(!camera.apply_shared_state(invalid));
        assert_eq!(camera.shared_state(), state);
    }
}
