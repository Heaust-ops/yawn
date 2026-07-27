use std::collections::HashMap;

use gltf::Gltf;
use ultraviolet::{Mat4, Vec3};

use crate::render_data::{
    InstanceHandle, MeshCreateInfo, MeshHandle, ModelTransform, PipelineKey, RenderData,
    RenderDataError, RenderFlags,
};

#[derive(Clone, Debug)]
pub struct InstalledScene {
    pub meshes: Vec<MeshHandle>,
    pub instances: Vec<InstanceHandle>,
    pub bounds: Option<ModelBounds>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl ModelBounds {
    fn include(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
}

fn focus_bounds(points: &[[f32; 3]]) -> Option<ModelBounds> {
    let first = *points.first()?;
    if points.len() < 200 {
        let mut bounds = ModelBounds {
            min: first,
            max: first,
        };
        for point in &points[1..] {
            bounds.include(*point);
        }
        return Some(bounds);
    }

    let trim = points.len() / 100;
    let mut min = [0.0; 3];
    let mut max = [0.0; 3];
    for axis in 0..3 {
        let mut values: Vec<_> = points.iter().map(|point| point[axis]).collect();
        values.sort_by(f32::total_cmp);
        min[axis] = values[trim];
        max[axis] = values[values.len() - trim - 1];
    }
    Some(ModelBounds { min, max })
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("failed to decode bytes")]
    GltfParse(#[from] gltf::Error),
    #[error("unsupported or malformed primitive: {0}")]
    InvalidPrimitive(String),
    #[error("failed to install imported scene")]
    Install(#[from] RenderDataError),
}

#[derive(Clone, Debug)]
pub struct ImportedGeometry {
    pub key: (usize, usize),
    pub double_sided: bool,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug)]
pub struct ImportedOccurrence {
    pub key: (usize, usize),
    pub transform: ModelTransform,
}
#[derive(Clone, Debug, Default)]
pub struct ImportedScene {
    pub geometries: Vec<ImportedGeometry>,
    pub occurrences: Vec<ImportedOccurrence>,
}

pub fn decode_gltf(bytes: &[u8]) -> Result<ImportedScene, ImportError> {
    let model = Gltf::from_slice(bytes)?;
    let buffers = gltf::import_buffers(&model.document, None, model.blob.clone())?;
    let mut result = ImportedScene::default();
    let mut seen = HashMap::new();
    fn visit(
        node: gltf::Node<'_>,
        parent: Mat4,
        buffers: &[gltf::buffer::Data],
        result: &mut ImportedScene,
        seen: &mut HashMap<(usize, usize), ()>,
    ) -> Result<(), ImportError> {
        let world = parent * Mat4::from(node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    return Err(ImportError::InvalidPrimitive(
                        "only triangle primitives are supported".into(),
                    ));
                }
                let key = (mesh.index(), primitive.index());
                if seen.insert(key, ()).is_none() {
                    let reader = primitive
                        .reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
                    let Some(read_positions) = reader.read_positions() else {
                        continue;
                    };
                    let positions: Vec<_> = read_positions.collect();
                    let count = positions.len();
                    if count == 0 {
                        continue;
                    }
                    let mut normals: Vec<_> = reader
                        .read_normals()
                        .map(|x| x.collect())
                        .unwrap_or_default();
                    normals.resize(count, [0., 1., 0.]);
                    normals.truncate(count);
                    let mut uvs: Vec<_> = reader
                        .read_tex_coords(0)
                        .map(|x| x.into_f32().collect())
                        .unwrap_or_default();
                    uvs.resize(count, [0., 0.]);
                    uvs.truncate(count);
                    let indices: Vec<u32> = if let Some(indices) = reader.read_indices() {
                        indices.into_u32().collect()
                    } else {
                        let count = u32::try_from(count).map_err(|_| {
                            ImportError::InvalidPrimitive("vertex count exceeds u32".into())
                        })?;
                        (0..count).collect()
                    };
                    if indices.is_empty() {
                        continue;
                    }
                    result.geometries.push(ImportedGeometry {
                        key,
                        double_sided: primitive.material().double_sided(),
                        positions,
                        normals,
                        uvs,
                        indices,
                    });
                } else if !result.geometries.iter().any(|geometry| geometry.key == key) {
                    continue;
                }
                result.occurrences.push(ImportedOccurrence {
                    key,
                    transform: world.into(),
                });
            }
        }
        for child in node.children() {
            visit(child, world, buffers, result, seen)?
        }
        Ok(())
    }
    for scene in model.scenes() {
        for node in scene.nodes() {
            visit(node, Mat4::identity(), &buffers, &mut result, &mut seen)?
        }
    }
    Ok(result)
}

pub fn install_imported(
    target: &mut RenderData,
    imported: &ImportedScene,
    pipelines: [PipelineKey; 2],
) -> Result<InstalledScene, ImportError> {
    let mut stage = target.replacement_stage()?;
    let mut handles = HashMap::new();
    let mut mesh_handles = Vec::with_capacity(imported.geometries.len());
    let mut instance_handles = Vec::new();
    let mut first = HashMap::new();
    for occurrence in &imported.occurrences {
        first.entry(occurrence.key).or_insert(occurrence.transform);
    }
    for geometry in &imported.geometries {
        let transform = *first
            .get(&geometry.key)
            .ok_or_else(|| ImportError::InvalidPrimitive("geometry has no occurrence".into()))?;
        let created = stage.create_mesh(MeshCreateInfo {
            positions: &geometry.positions,
            normals: &geometry.normals,
            uvs: &geometry.uvs,
            indices: &geometry.indices,
            pipeline: pipelines[usize::from(geometry.double_sided)],
            flags: RenderFlags::VISIBLE,
            default_instance_flags: RenderFlags::VISIBLE,
            default_transform: transform,
        })?;
        handles.insert(geometry.key, created.mesh);
        mesh_handles.push(created.mesh);
        instance_handles.push(created.default_instance);
    }
    let mut consumed = HashMap::new();
    let mut bounds: Option<ModelBounds> = None;
    let geometries: HashMap<_, _> = imported
        .geometries
        .iter()
        .map(|geometry| (geometry.key, geometry))
        .collect();
    let mut focus_points = Vec::new();
    for occurrence in &imported.occurrences {
        let mesh = *handles
            .get(&occurrence.key)
            .ok_or_else(|| ImportError::InvalidPrimitive("occurrence has no geometry".into()))?;
        if consumed.insert(occurrence.key, ()).is_some() {
            instance_handles.push(stage.create_instance(
                mesh,
                occurrence.transform,
                RenderFlags::VISIBLE,
            )?);
        }
        let geometry = geometries
            .get(&occurrence.key)
            .expect("installed occurrence must have geometry");
        let transform = Mat4::from(occurrence.transform);
        focus_points.extend(geometry.positions.iter().map(|position| {
            let point = transform.transform_point3(Vec3::from(*position));
            [point.x, point.y, point.z]
        }));
        let local = stage.mesh(mesh).unwrap().aabb;
        for x in [local.min[0], local.max[0]] {
            for y in [local.min[1], local.max[1]] {
                for z in [local.min[2], local.max[2]] {
                    let p = Mat4::from(occurrence.transform).transform_point3(Vec3::new(x, y, z));
                    let p = [p.x, p.y, p.z];
                    if let Some(b) = bounds.as_mut() {
                        b.include(p)
                    } else {
                        bounds = Some(ModelBounds { min: p, max: p })
                    }
                }
            }
        }
    }
    bounds = focus_bounds(&focus_points).or(bounds);
    target.replace_with(stage)?;
    Ok(InstalledScene {
        meshes: mesh_handles,
        instances: instance_handles,
        bounds,
    })
}
