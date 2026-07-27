use std::f32::consts::PI;

use ultraviolet::{projection, Bivec3, Mat4, Rotor3, Vec3};
use wgpu::util::DeviceExt;

use crate::renderer::scene::UniformResource;

/// A camera matrix cannot produce a safe, meaningful frustum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrustumError {
    #[error("frustum plane {plane} contains a non-finite component")]
    NonFinite { plane: usize },
    #[error("frustum plane {plane} has a near-degenerate normal")]
    Degenerate { plane: usize },
}

const MIN_DISTANCE: f32 = 0.1;
const MAX_PITCH: f32 = PI / 2.0 - 0.01;
const ORBIT_SENSITIVITY: f32 = 0.005;
const ZOOM_SENSITIVITY: f32 = 0.002;

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

    // Rotor orientation for orbit camera behaviour
    rotor: Rotor3,
    distance: f32,

    // Dirty flag for lazy evaluation
    dirty: bool,
}

struct OrthonormalBasis {
    right: Vec3,
    up: Vec3,
    forward: Vec3,
}

impl OrthonormalBasis {
    pub fn new(right: Vec3, up: Vec3, forward: Vec3) -> Self {
        Self { right, up, forward }
    }

    pub fn from_camera(camera: &Camera) -> Self {
        let mut forward_offset = camera.target - camera.position;
        if forward_offset.mag_sq() <= f32::EPSILON {
            forward_offset = -Vec3::unit_z();
        }

        let forward = forward_offset.normalized();

        let mut right = forward.cross(camera.up);

        // Check if right vector is near zero (forward and up are parallel)
        if right.mag_sq() < 1e-10 {
            // Try alternate axes to find a valid right vector
            let alternate_axes = [Vec3::unit_y(), Vec3::unit_x()];
            for axis in alternate_axes.iter() {
                right = forward.cross(*axis);
                if right.mag_sq() >= 1e-10 {
                    break;
                }
            }
        }

        right = right.normalized();
        let up = right.cross(forward).normalized();

        Self::new(right, up, forward)
    }
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
            rotor: Rotor3::identity(),
            distance: 1.0,
            dirty: true,
        };

        camera.compute_rotor();
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
        self.dirty = false;
    }

    pub fn look_at(&mut self, position: Vec3, target: Vec3) {
        if !vec3_is_finite(position) || !vec3_is_finite(target) {
            return;
        }
        self.position = position;
        self.target = target;
        if (self.position - self.target).mag_sq() <= f32::EPSILON {
            self.position = self.target + Vec3::unit_z() * MIN_DISTANCE;
        }
        self.up = Vec3::unit_y();
        self.compute_rotor();
        self.dirty = true;
        self.compute_view_proj_mat();
    }

    pub fn set_depth_range(&mut self, z_near: f32, z_far: f32) {
        self.z_near = z_near;
        self.z_far = z_far.max(z_near + f32::EPSILON);
        self.dirty = true;
        self.compute_view_proj_mat();
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn update_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = aspect_ratio;
        self.dirty = true;
        self.compute_view_proj_mat();
    }

    pub fn orbit(&mut self, delta_x: f32, delta_y: f32) {
        if !delta_x.is_finite() || !delta_y.is_finite() {
            return;
        }
        // Skip tiny movements to reduce unnecessary computations
        if delta_x.abs() < 0.001 && delta_y.abs() < 0.001 {
            return;
        }

        let yaw_theta = delta_x * ORBIT_SENSITIVITY;
        let yaw_rotor =
            Rotor3::from_angle_plane(yaw_theta, Bivec3::from_normalized_axis(Vec3::unit_y()));

        let basis = OrthonormalBasis::from_camera(self);

        let pitch_angle = (delta_y * ORBIT_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);

        let pitch_rotor =
            Rotor3::from_angle_plane(pitch_angle, Bivec3::from_normalized_axis(basis.right));

        let orbit_rotor = (yaw_rotor * pitch_rotor).normalized();

        self.rotor = (orbit_rotor * self.rotor).normalized();

        let mut offset = self.position - self.target;
        if offset.mag_sq() <= f32::EPSILON {
            offset = Vec3::unit_z() * self.distance.max(MIN_DISTANCE);
        }

        orbit_rotor.rotate_vec(&mut offset);
        self.distance = offset.mag().max(MIN_DISTANCE);
        self.position = offset + self.target;

        self.dirty = true;
        self.compute_view_proj_mat();
    }

    pub fn zoom(&mut self, delta_y_pixels: f32) {
        if !delta_y_pixels.is_finite() || delta_y_pixels.abs() <= f32::EPSILON {
            return;
        }

        let mut offset = self.position - self.target;
        let mut current_distance = offset.mag();
        if !current_distance.is_finite() {
            return;
        }
        if current_distance <= f32::EPSILON {
            offset = Vec3::unit_z() * MIN_DISTANCE;
            current_distance = MIN_DISTANCE;
        }
        let direction = offset / current_distance;
        let max_distance = (self.z_far * 0.95).max(MIN_DISTANCE);
        if !max_distance.is_finite() {
            return;
        }
        let candidate = f64::from(current_distance)
            * (f64::from(ZOOM_SENSITIVITY) * f64::from(delta_y_pixels)).exp();
        let new_distance = candidate.clamp(f64::from(MIN_DISTANCE), f64::from(max_distance)) as f32;
        let new_position = self.target + direction * new_distance;
        if !vec3_is_finite(new_position) {
            return;
        }

        self.position = new_position;
        self.distance = new_distance;
        self.dirty = true;
        self.compute_view_proj_mat();
    }

    pub fn pan(&mut self, delta_x: f32, delta_y: f32, viewport_height: f32) {
        if !delta_x.is_finite()
            || !delta_y.is_finite()
            || !viewport_height.is_finite()
            || !self.fov.is_finite()
        {
            return;
        }
        let distance = (self.position - self.target).mag().max(MIN_DISTANCE);
        if !distance.is_finite() {
            return;
        }
        let world_units_per_pixel =
            2.0 * distance * (self.fov * 0.5).tan() / viewport_height.max(1.0);
        let basis = OrthonormalBasis::from_camera(self);
        let translation = (-basis.right * delta_x + basis.up * delta_y) * world_units_per_pixel;
        let position = self.position + translation;
        let target = self.target + translation;
        if !vec3_is_finite(position) || !vec3_is_finite(target) {
            return;
        }
        self.position = position;
        self.target = target;
        self.dirty = true;
        self.compute_view_proj_mat();
    }

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

    fn compute_rotor(&mut self) {
        let offset = self.position - self.target;
        let distance = (offset.x * offset.x + offset.y * offset.y + offset.z * offset.z).sqrt();
        self.distance = distance.max(MIN_DISTANCE);

        // to compute the initial rotor we will do two rotations
        // these will orient the camera to the new coordinates
        //

        // but first we need the orthonormal basis for the current camera
        let basis = OrthonormalBasis::from_camera(self);

        // first rotation
        // this is the swing to make position face the target
        let camera_local_up = Vec3::unit_z();
        let swing_rotor = Rotor3::from_rotation_between(camera_local_up, -basis.forward);

        // now we need a twist rotor which aligns the camera up
        let mut up_after_swing = self.up.clone();
        swing_rotor.rotate_vec(&mut up_after_swing);

        // to rotate a vector by a rotor we need
        // - a bivector (represents the axis of rotation)
        // - angle of rotation
        let twist_axis = (-basis.forward).normalized();
        let twist_plane = Bivec3::from_normalized_axis(twist_axis);

        // Calculate twist angle between the up vectors:
        //            u1 × uc ⋅ (-f)
        // θ = atan2( ————————————— , u1 ⋅ uc )
        //              ‖u1 × uc‖
        //
        // Where:
        //   u1 = up vector after swing rotation
        //   uc = camera's current up vector
        //   f = forward vector (twist axis)
        let theta = up_after_swing
            .cross(self.up)
            .dot(twist_axis)
            .atan2(up_after_swing.dot(self.up));

        let twist_rotor = Rotor3::from_angle_plane(theta, twist_plane);

        self.rotor = (swing_rotor * twist_rotor).normalized();
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

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
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

    fn assert_vec3_close(actual: Vec3, expected: Vec3, epsilon: f32) {
        assert!(
            (actual.x - expected.x).abs() <= epsilon,
            "x: {actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() <= epsilon,
            "y: {actual:?} != {expected:?}"
        );
        assert!(
            (actual.z - expected.z).abs() <= epsilon,
            "z: {actual:?} != {expected:?}"
        );
    }

    fn assert_camera_finite(camera: &Camera) {
        assert!(vec3_is_finite(camera.position));
        assert!(vec3_is_finite(camera.target));
        assert!(camera.distance.is_finite());
        assert!(camera
            .view_proj
            .iter()
            .flatten()
            .all(|component| component.is_finite()));
    }

    #[test]
    fn zoom_is_multiplicative_and_preserves_target() {
        let mut camera = Camera::new(1.0);
        camera.look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::zero());
        let initial_position = camera.position;
        let initial_target = camera.target;
        let initial_distance = camera.distance;

        camera.zoom(-100.0);
        assert!(camera.distance < initial_distance);
        assert_eq!(camera.target, initial_target);

        camera.zoom(100.0);
        assert_vec3_close(camera.position, initial_position, 1e-4);
        assert!((camera.distance - initial_distance).abs() <= 1e-4);

        camera.zoom(100.0);
        assert!(camera.distance > initial_distance);
        assert_eq!(camera.target, initial_target);
    }

    #[test]
    fn zoom_clamps_and_remains_finite() {
        let mut camera = Camera::new(1.0);
        camera.look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::zero());
        camera.zoom(f32::NEG_INFINITY);
        assert!((camera.distance - 10.0).abs() <= 1e-5);
        camera.zoom(-f32::MAX);
        assert!((camera.distance - MIN_DISTANCE).abs() <= f32::EPSILON);
        camera.zoom(f32::MAX);
        assert!(camera.distance <= camera.z_far * 0.95);
        assert_camera_finite(&camera);
    }

    #[test]
    fn pan_moves_eye_and_target_equally_at_target_plane_scale() {
        let mut camera = Camera::new(1.0);
        camera.look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::zero());
        let initial_position = camera.position;
        let initial_target = camera.target;
        let initial_offset = initial_position - initial_target;
        let units = 2.0 * 10.0 * (PI / 6.0).tan() / 1000.0;

        camera.pan(20.0, 10.0, 1000.0);

        let translation = Vec3::new(-20.0 * units, 10.0 * units, 0.0);
        assert_vec3_close(camera.position, initial_position + translation, 1e-5);
        assert_vec3_close(camera.target, initial_target + translation, 1e-5);
        assert_vec3_close(camera.position - camera.target, initial_offset, 1e-5);
        assert!((camera.distance - 10.0).abs() <= 1e-5);
    }

    #[test]
    fn pan_scale_is_proportional_to_distance() {
        let mut near = Camera::new(1.0);
        near.look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::zero());
        let mut far = Camera::new(1.0);
        far.look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::zero());

        near.pan(10.0, 0.0, 1000.0);
        far.pan(10.0, 0.0, 1000.0);

        assert!((far.target.mag() / near.target.mag() - 2.0).abs() <= 1e-5);
    }

    #[test]
    fn orbit_preserves_target_and_distance() {
        let mut camera = Camera::new(1.0);
        camera.look_at(Vec3::new(2.0, 3.0, 10.0), Vec3::new(1.0, -1.0, 0.5));
        let initial_target = camera.target;
        let initial_distance = (camera.position - camera.target).mag();

        camera.orbit(40.0, -25.0);

        assert_eq!(camera.target, initial_target);
        assert!(((camera.position - camera.target).mag() - initial_distance).abs() <= 1e-5);
        assert!((camera.distance - initial_distance).abs() <= 1e-5);
        assert!(camera.distance >= MIN_DISTANCE);
        assert_camera_finite(&camera);
    }

    #[test]
    fn controls_reject_invalid_input_and_recover_degenerate_look_at() {
        let mut camera = Camera::new(1.0);
        camera.look_at(Vec3::zero(), Vec3::zero());
        assert!((camera.position - camera.target).mag() >= MIN_DISTANCE);
        let position = camera.position;
        let target = camera.target;

        camera.orbit(f32::NAN, 1.0);
        camera.zoom(f32::INFINITY);
        camera.pan(f32::NAN, 1.0, 0.0);
        assert_eq!(camera.position, position);
        assert_eq!(camera.target, target);

        camera.pan(1.0, 1.0, 0.0);
        camera.orbit(4.0, -3.0);
        camera.zoom(2.0);
        assert_camera_finite(&camera);
    }
}
