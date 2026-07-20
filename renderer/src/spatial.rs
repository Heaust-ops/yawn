//! Immutable scene-spatial contracts and broad-phase ray helpers.

use ultraviolet::{Mat4, Vec4};

use crate::render_data::{Aabb, BoundsIdentity, InstanceHandle, RenderData};

pub const VISIBLE: u32 = 1;
pub const SELECTABLE: u32 = 1 << 3;

#[derive(Clone, Debug)]
pub struct SpatialInstance {
    pub handle: InstanceHandle,
    pub geometry: BoundsIdentity,
    pub local_bounds: Aabb,
    pub transform: [f32; 16],
    pub transform_version: u32,
    pub state_version: u32,
    pub flags: u32,
    pub layer_mask: u32,
}

#[derive(Clone, Debug)]
pub struct SpatialSnapshot {
    pub snapshot_id: u32,
    pub scene_commit_epoch: u32,
    pub instances: Vec<SpatialInstance>,
}

impl SpatialSnapshot {
    /// Copies all columns at the render-worker commit boundary. Workers only
    /// receive this owned value and never inspect canonical columns.
    pub fn mint(data: &RenderData, snapshot_id: u32, scene_commit_epoch: u32) -> Self {
        let instances = data
            .instances_with_handles()
            .filter_map(|(handle, instance)| {
                let (geometry, local_bounds) = data.accepted_bounds_identity(instance.geometry)?;
                Some(SpatialInstance {
                    handle,
                    geometry,
                    local_bounds,
                    transform: *instance.transform.as_array(),
                    transform_version: data.transform_version(handle)?,
                    state_version: data.state_version(handle)?,
                    flags: instance.render_flags,
                    layer_mask: data.layer_mask(handle)?,
                })
            })
            .collect();
        Self {
            snapshot_id,
            scene_commit_epoch,
            instances,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
}

pub fn ray_from_view_proj(
    view_proj: [[f32; 4]; 4],
    x: f32,
    y: f32,
    width: u32,
    height: u32,
) -> Option<Ray> {
    if width == 0 || height == 0 {
        return None;
    }
    let ndc_x = 2.0 * x / width as f32 - 1.0;
    let ndc_y = 1.0 - 2.0 * y / height as f32;
    let inverse = Mat4::from(view_proj).inversed();
    let unproject = |z| {
        let p = inverse * Vec4::new(ndc_x, ndc_y, z, 1.0);
        (p / p.w).xyz()
    };
    let near = unproject(0.0);
    let far = unproject(1.0);
    let direction = (far - near).normalized();
    (direction.x.is_finite() && direction.y.is_finite() && direction.z.is_finite()).then_some(Ray {
        origin: near.into(),
        direction: direction.into(),
    })
}

pub fn ray_aabb(ray: Ray, bounds: Aabb) -> Option<f32> {
    let mut near: f32 = 0.0;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if ray.direction[axis].abs() < 1e-8 {
            if ray.origin[axis] < bounds.min[axis] || ray.origin[axis] > bounds.max[axis] {
                return None;
            }
        } else {
            let inv = 1.0 / ray.direction[axis];
            let mut a = (bounds.min[axis] - ray.origin[axis]) * inv;
            let mut b = (bounds.max[axis] - ray.origin[axis]) * inv;
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            near = near.max(a);
            far = far.min(b);
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

pub fn validate_pick(data: &RenderData, handle: InstanceHandle) -> bool {
    data.instance_geometry(handle).is_some()
}

pub fn snapshot_matches(
    expected_snapshot_id: u32,
    expected_commit_epoch: u32,
    snapshot_id: u32,
    commit_epoch: u32,
) -> bool {
    expected_snapshot_id == snapshot_id && expected_commit_epoch == commit_epoch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_data::{cube_geometry, RenderDataConfig};

    fn some<T>(value: Option<T>) -> T {
        match value {
            Some(value) => value,
            None => panic!("test expected Some"),
        }
    }

    #[test]
    fn center_ray_and_aabb_intersection() {
        let ray = some(ray_from_view_proj(
            Mat4::identity().into(),
            50.0,
            50.0,
            100,
            100,
        ));
        assert!(ray_aabb(
            ray,
            Aabb {
                min: [-1.0, -1.0, 0.2],
                max: [1.0, 1.0, 0.8]
            }
        )
        .is_some());
        assert!(ray_aabb(
            ray,
            Aabb {
                min: [2.0; 3],
                max: [3.0; 3]
            }
        )
        .is_none());
    }

    #[test]
    fn generation_is_revalidated() {
        let mut data = RenderData::new(RenderDataConfig::default());
        let (_, handle) = some(data.add_geometry(cube_geometry()));
        assert!(validate_pick(&data, handle));
        data.remove_instance(handle);
        assert!(!validate_pick(&data, handle));
    }

    #[test]
    fn stale_snapshot_is_rejected() {
        assert!(snapshot_matches(4, 9, 4, 9));
        assert!(!snapshot_matches(4, 9, 3, 9));
        assert!(!snapshot_matches(4, 9, 4, 8));
    }
}
