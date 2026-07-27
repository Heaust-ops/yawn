use std::collections::HashMap;

use gltf::Gltf;
use ultraviolet::{Mat4, Vec3};

use crate::render_data::{
    InstanceHandle, MaterialKey, MeshCreateInfo, MeshHandle, ModelTransform, PipelineKey,
    RenderData, RenderDataError, RenderFlags,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureReference {
    pub texture: usize,
    pub tex_coord: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub key: MaterialKey,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    /// Index of refraction for the dielectric Fresnel response.
    pub ior: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub base_color_texture: Option<TextureReference>,
    pub metallic_roughness_texture: Option<TextureReference>,
    pub normal_texture: Option<TextureReference>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<TextureReference>,
    pub occlusion_strength: f32,
    pub emissive_texture: Option<TextureReference>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            key: MaterialKey::DEFAULT,
            base_color_factor: [1.0; 4],
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            emissive_factor: [0.0; 3],
            ior: 1.5,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive_texture: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureMetadata {
    pub image: usize,
    pub sampler: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamplerMetadata {
    pub index: usize,
    pub mag_filter: Option<String>,
    pub min_filter: Option<String>,
    pub wrap_s: String,
    pub wrap_t: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSource {
    Uri(String),
    BufferView(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMetadata {
    pub index: usize,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub source: ImageSource,
    /// Encoded PNG/JPEG bytes. Kept encoded so GPU installation can decode and
    /// upload images one at a time instead of retaining a decoded image batch.
    pub encoded_data: Vec<u8>,
}

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
    #[error("unsupported image source: {0}")]
    UnsupportedImage(String),
    #[error("invalid KHR_materials_ior value: {0}")]
    InvalidIor(f32),
    #[error("failed to install imported scene")]
    Install(#[from] RenderDataError),
}

fn decode_ior(value: Option<f32>) -> Result<f32, ImportError> {
    let ior = value.unwrap_or(1.5);
    if ior == 0.0 || (ior.is_finite() && ior >= 1.0) {
        Ok(ior)
    } else {
        Err(ImportError::InvalidIor(ior))
    }
}

#[derive(Clone, Debug)]
pub struct ImportedGeometry {
    pub key: (usize, usize),
    pub material: MaterialKey,
    pub double_sided: bool,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
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
    pub materials: Vec<Material>,
    pub textures: Vec<TextureMetadata>,
    pub samplers: Vec<SamplerMetadata>,
    pub images: Vec<ImageMetadata>,
}

fn texture_reference(info: gltf::texture::Info<'_>) -> TextureReference {
    TextureReference {
        texture: info.texture().index(),
        tex_coord: info.tex_coord(),
    }
}

fn normalize(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length = value.iter().map(|x| x * x).sum::<f32>().sqrt();
    if length > f32::EPSILON && length.is_finite() {
        value.map(|x| x / length)
    } else {
        fallback
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Completes triangle vertex attributes. Generated attributes use corner vertices,
/// which deliberately splits UV seams, hard normal edges, and opposite handedness.
fn repair_geometry(
    positions: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    tangents: Option<Vec<[f32; 4]>>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Result<
    (
        Vec<[f32; 3]>,
        Vec<[f32; 3]>,
        Vec<[f32; 4]>,
        Vec<[f32; 2]>,
        Vec<u32>,
    ),
    ImportError,
> {
    if indices.len() % 3 != 0 || indices.iter().any(|&i| i as usize >= positions.len()) {
        return Err(ImportError::InvalidPrimitive(
            "triangle indices are malformed".into(),
        ));
    }
    let normals_valid = normals.as_ref().is_some_and(|x| x.len() == positions.len());
    let tangents_valid = tangents
        .as_ref()
        .is_some_and(|x| x.len() == positions.len());
    if normals_valid && tangents_valid {
        return Ok((positions, normals.unwrap(), tangents.unwrap(), uvs, indices));
    }

    let mut out_p = Vec::with_capacity(indices.len());
    let mut out_n = Vec::with_capacity(indices.len());
    let mut out_t = Vec::with_capacity(indices.len());
    let mut out_uv = Vec::with_capacity(indices.len());
    for triangle in indices.chunks_exact(3) {
        let ids = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let p = ids.map(|i| positions[i]);
        let uv = ids.map(|i| uvs.get(i).copied().unwrap_or([0.0; 2]));
        let face_normal = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])), [0.0, 1.0, 0.0]);
        let duv1 = [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]];
        let duv2 = [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]];
        let determinant = duv1[0] * duv2[1] - duv1[1] * duv2[0];
        let edge1 = sub(p[1], p[0]);
        let edge2 = sub(p[2], p[0]);
        let (raw_tangent, raw_bitangent) =
            if determinant.abs() > f32::EPSILON && determinant.is_finite() {
                let r = determinant.recip();
                (
                    std::array::from_fn(|i| (edge1[i] * duv2[1] - edge2[i] * duv1[1]) * r),
                    std::array::from_fn(|i| (edge2[i] * duv1[0] - edge1[i] * duv2[0]) * r),
                )
            } else {
                ([0.0; 3], [0.0; 3])
            };
        for corner in 0..3 {
            let n = normals
                .as_ref()
                .filter(|_| normals_valid)
                .map_or(face_normal, |x| x[ids[corner]]);
            let n = normalize(n, face_normal);
            let projected = std::array::from_fn(|i| raw_tangent[i] - n[i] * dot(n, raw_tangent));
            let fallback_axis = if n[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let tangent3 = normalize(
                projected,
                normalize(cross(fallback_axis, n), [0.0, 0.0, 1.0]),
            );
            let generated = [
                tangent3[0],
                tangent3[1],
                tangent3[2],
                if dot(cross(n, tangent3), raw_bitangent) < 0.0 {
                    -1.0
                } else {
                    1.0
                },
            ];
            out_p.push(p[corner]);
            out_n.push(n);
            out_t.push(
                tangents
                    .as_ref()
                    .filter(|_| tangents_valid)
                    .map_or(generated, |x| x[ids[corner]]),
            );
            out_uv.push(uv[corner]);
        }
    }
    let out_i = (0..u32::try_from(out_p.len())
        .map_err(|_| ImportError::InvalidPrimitive("vertex count exceeds u32".into()))?)
        .collect();
    Ok((out_p, out_n, out_t, out_uv, out_i))
}

pub fn decode_gltf(bytes: &[u8]) -> Result<ImportedScene, ImportError> {
    decode_gltf_model(Gltf::from_slice(bytes)?)
}

pub fn decode_gltf_owned(bytes: Vec<u8>) -> Result<ImportedScene, ImportError> {
    let model = Gltf::from_slice(&bytes)?;
    drop(bytes);
    decode_gltf_model(model)
}

fn decode_gltf_model(mut model: Gltf) -> Result<ImportedScene, ImportError> {
    // Reject external images before buffer import can turn them into a generic
    // import error (or attempt to interpret a data/external URI).
    for image in model.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source() {
            return Err(ImportError::UnsupportedImage(format!(
                "URI/external image '{uri}'"
            )));
        }
    }
    let blob = model.blob.take();
    let buffers = gltf::import_buffers(&model.document, None, blob)?;
    let mut result = ImportedScene::default();
    result.materials.push(Material::default());
    for material in model.materials() {
        let pbr = material.pbr_metallic_roughness();
        let normal = material.normal_texture();
        let normal_texture = normal.as_ref().map(|x| TextureReference {
            texture: x.texture().index(),
            tex_coord: x.tex_coord(),
        });
        let occlusion = material.occlusion_texture();
        let occlusion_texture = occlusion.as_ref().map(|x| TextureReference {
            texture: x.texture().index(),
            tex_coord: x.tex_coord(),
        });
        result.materials.push(Material {
            key: MaterialKey::new(material.index().unwrap() as u32 + 1),
            base_color_factor: pbr.base_color_factor(),
            metallic_factor: pbr.metallic_factor(),
            roughness_factor: pbr.roughness_factor(),
            emissive_factor: material.emissive_factor(),
            ior: decode_ior(material.ior())?,
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => AlphaMode::Mask,
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
            double_sided: material.double_sided(),
            base_color_texture: pbr.base_color_texture().map(texture_reference),
            metallic_roughness_texture: pbr.metallic_roughness_texture().map(texture_reference),
            normal_texture,
            normal_scale: normal.map_or(1.0, |x| x.scale()),
            occlusion_texture,
            occlusion_strength: occlusion.map_or(1.0, |x| x.strength()),
            emissive_texture: material.emissive_texture().map(texture_reference),
        });
    }
    result.textures = model
        .textures()
        .map(|x| TextureMetadata {
            image: x.source().index(),
            sampler: x.sampler().index(),
        })
        .collect();
    result.samplers = model
        .samplers()
        .map(|x| SamplerMetadata {
            index: x.index().unwrap(),
            mag_filter: x.mag_filter().map(|v| format!("{v:?}")),
            min_filter: x.min_filter().map(|v| format!("{v:?}")),
            wrap_s: format!("{:?}", x.wrap_s()),
            wrap_t: format!("{:?}", x.wrap_t()),
        })
        .collect();
    result.images = model
        .images()
        .map(|x| -> Result<_, ImportError> {
            let (source, mime_type, encoded_data) = match x.source() {
                gltf::image::Source::Uri { uri, .. } => {
                    return Err(ImportError::UnsupportedImage(format!(
                        "URI/external image '{uri}'"
                    )))
                }
                gltf::image::Source::View { view, mime_type } => {
                    let data = buffers.get(view.buffer().index()).ok_or_else(|| {
                        ImportError::UnsupportedImage("image buffer is missing".into())
                    })?;
                    let end = view.offset().checked_add(view.length()).ok_or_else(|| {
                        ImportError::UnsupportedImage("image bufferView overflows".into())
                    })?;
                    let bytes = data.0.get(view.offset()..end).ok_or_else(|| {
                        ImportError::UnsupportedImage("image bufferView is out of bounds".into())
                    })?;
                    (
                        ImageSource::BufferView(view.index()),
                        Some(mime_type.to_owned()),
                        bytes.to_vec(),
                    )
                }
            };
            Ok(ImageMetadata {
                index: x.index(),
                name: x.name().map(str::to_owned),
                mime_type,
                source,
                encoded_data,
            })
        })
        .collect::<Result<_, _>>()?;
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
                    let normals = reader.read_normals().map(|x| x.collect());
                    let tangents = reader.read_tangents().map(|x| x.collect());
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
                    let (positions, normals, tangents, uvs, indices) =
                        repair_geometry(positions, normals, tangents, uvs, indices)?;
                    let primitive_material = primitive.material();
                    result.geometries.push(ImportedGeometry {
                        key,
                        material: primitive_material
                            .index()
                            .map_or(MaterialKey::DEFAULT, |index| {
                                MaterialKey::new(index as u32 + 1)
                            }),
                        double_sided: primitive_material.double_sided(),
                        positions,
                        normals,
                        tangents,
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
            tangents: &geometry.tangents,
            uvs: &geometry.uvs,
            indices: &geometry.indices,
            pipeline: pipelines[usize::from(geometry.double_sided)],
            material: geometry.material,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_images_are_rejected_explicitly() {
        let json = br#"{
            "asset":{"version":"2.0"},
            "images":[{"uri":"external.png"}],
            "scenes":[{"nodes":[]}],"scene":0
        }"#;
        let result = decode_gltf(json);
        assert!(
            matches!(
                result,
                Err(ImportError::UnsupportedImage(ref message)) if message.contains("URI/external")
            ),
            "unexpected result: {result:?}"
        );
    }

    #[test]
    fn owned_and_borrowed_decode_paths_remain_compatible() {
        let json = br#"{
            "asset":{"version":"2.0"},
            "scenes":[{"nodes":[]}],"scene":0
        }"#;
        let borrowed = decode_gltf(json).unwrap();
        let owned = decode_gltf_owned(json.to_vec()).unwrap();
        assert_eq!(borrowed.geometries.len(), owned.geometries.len());
        assert_eq!(borrowed.occurrences.len(), owned.occurrences.len());
        assert_eq!(borrowed.materials.len(), owned.materials.len());
    }

    #[test]
    fn repair_duplicates_corners_and_generates_flat_finite_frames() {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let repaired = repair_geometry(
            positions,
            None,
            None,
            vec![[0.0; 2]; 4],
            vec![0, 1, 2, 0, 3, 1],
        )
        .unwrap();
        assert_eq!(repaired.0.len(), 6);
        assert_eq!(repaired.4, (0..6).collect::<Vec<_>>());
        assert_eq!(&repaired.1[..3], &[[0.0, 0.0, 1.0]; 3]);
        assert_eq!(&repaired.1[3..], &[[0.0, 1.0, 0.0]; 3]);
        assert!(repaired
            .2
            .iter()
            .flatten()
            .all(|component| component.is_finite()));
        assert!(repaired.2.iter().all(|tangent| tangent[3].abs() == 1.0));
    }

    #[test]
    fn generated_tangents_split_opposite_handedness_and_supplied_values_survive() {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let normals = vec![[0.0, 0.0, 1.0]; 4];
        let generated = repair_geometry(
            positions.clone(),
            Some(normals.clone()),
            None,
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 0.0]],
            vec![0, 1, 2, 0, 2, 3],
        )
        .unwrap();
        assert_ne!(generated.2[0][3], generated.2[3][3]);

        let supplied = vec![[0.25, 0.5, 0.75, -1.0]; 4];
        let preserved = repair_geometry(
            positions,
            Some(normals),
            Some(supplied.clone()),
            vec![[0.0; 2]; 4],
            vec![0, 1, 2],
        )
        .unwrap();
        assert_eq!(preserved.2, supplied);
    }

    #[test]
    fn material_defaults_match_gltf_core_defaults() {
        let material = Material::default();
        assert_eq!(material.base_color_factor, [1.0; 4]);
        assert_eq!(material.metallic_factor, 1.0);
        assert_eq!(material.roughness_factor, 1.0);
        assert_eq!(material.alpha_cutoff, 0.5);
        assert_eq!(material.ior, 1.5);
        assert_eq!(material.key, MaterialKey::DEFAULT);
    }

    #[test]
    fn imports_khr_materials_ior() {
        let json = br#"{
            "asset":{"version":"2.0"},
            "extensionsUsed":["KHR_materials_ior"],
            "materials":[{"extensions":{"KHR_materials_ior":{"ior":1.33}}}],
            "scenes":[{"nodes":[]}],"scene":0
        }"#;
        let imported = decode_gltf(json).unwrap();
        assert_eq!(imported.materials[1].ior, 1.33);
    }

    #[test]
    fn ior_gate_accepts_default_physical_values_and_explicit_zero_sentinel() {
        assert_eq!(decode_ior(None).unwrap(), 1.5);
        assert_eq!(decode_ior(Some(1.0)).unwrap(), 1.0);
        assert_eq!(decode_ior(Some(1.33)).unwrap(), 1.33);
        assert_eq!(decode_ior(Some(0.0)).unwrap(), 0.0);
    }

    #[test]
    fn ior_gate_rejects_nonphysical_and_nonfinite_values() {
        for ior in [-1.0, 0.5, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert!(matches!(
                decode_ior(Some(ior)),
                Err(ImportError::InvalidIor(_))
            ));
        }
    }

    #[test]
    fn malformed_but_parseable_json_ior_is_rejected_before_packing() {
        for ior in ["-1", "0.5"] {
            let json = format!(
                r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_materials_ior"],"materials":[{{"extensions":{{"KHR_materials_ior":{{"ior":{ior}}}}}}}],"scenes":[{{"nodes":[]}}],"scene":0}}"#
            );
            assert!(matches!(
                decode_gltf(json.as_bytes()),
                Err(ImportError::InvalidIor(_))
            ));
        }
    }
}
