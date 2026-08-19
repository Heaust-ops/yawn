use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use ultraviolet::{Mat4, Vec3};

use super::{
    InstanceType, MaterialKey, MeshCreateInfo, MeshHandle, ModelTransform, RenderData,
    RenderDataError, ReplacementStage,
};

const MAGIC: u32 = u32::from_le_bytes(*b"YRDP");
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 16;
const BASE_COLOR_TEXTURE: u32 = 1 << 0;
const METALLIC_ROUGHNESS_TEXTURE: u32 = 1 << 1;
const NORMAL_TEXTURE: u32 = 1 << 2;
const OCCLUSION_TEXTURE: u32 = 1 << 3;
const EMISSIVE_TEXTURE: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

/// SIMD-aligned material row shared with external render-data writers and the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialState {
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 4],
    pub surface_factors: [f32; 4],
    pub alpha_optics: [f32; 4],
    pub flags: [u32; 4],
    pub uv_sets: [u32; 4],
    pub debug_extras: [u32; 4],
}

fn enabled(reference: Option<TextureReference>, bit: u32) -> u32 {
    reference
        .filter(|value| value.tex_coord == 0)
        .map_or(0, |_| bit)
}

impl From<&Material> for MaterialState {
    fn from(value: &Material) -> Self {
        Self {
            base_color_factor: value.base_color_factor,
            emissive_factor: [
                value.emissive_factor[0],
                value.emissive_factor[1],
                value.emissive_factor[2],
                0.0,
            ],
            surface_factors: [
                value.metallic_factor,
                value.roughness_factor,
                value.normal_scale,
                value.occlusion_strength,
            ],
            alpha_optics: [
                match value.alpha_mode {
                    AlphaMode::Opaque => 0.0,
                    AlphaMode::Mask => 1.0,
                    AlphaMode::Blend => 2.0,
                },
                value.alpha_cutoff,
                value.ior,
                if value.ior == 0.0 {
                    1.0
                } else {
                    ((value.ior - 1.0) / (value.ior + 1.0)).powi(2)
                },
            ],
            flags: [
                enabled(value.base_color_texture, BASE_COLOR_TEXTURE)
                    | enabled(value.metallic_roughness_texture, METALLIC_ROUGHNESS_TEXTURE)
                    | enabled(value.normal_texture, NORMAL_TEXTURE)
                    | enabled(value.occlusion_texture, OCCLUSION_TEXTURE)
                    | enabled(value.emissive_texture, EMISSIVE_TEXTURE),
                u32::from(value.double_sided),
                0,
                0,
            ],
            uv_sets: [
                value.base_color_texture.map_or(0, |value| value.tex_coord),
                value
                    .metallic_roughness_texture
                    .map_or(0, |value| value.tex_coord),
                value.normal_texture.map_or(0, |value| value.tex_coord),
                value.occlusion_texture.map_or(0, |value| value.tex_coord),
            ],
            debug_extras: [
                value.emissive_texture.map_or(0, |value| value.tex_coord),
                0,
                0,
                0,
            ],
        }
    }
}

impl MaterialState {
    pub const LANES: u32 = 28;

    pub fn words(self) -> [u32; Self::LANES as usize] {
        bytemuck::cast(self)
    }

    pub fn from_words(words: [u32; Self::LANES as usize]) -> Self {
        bytemuck::cast(words)
    }
}

const _: [(); 112] = [(); std::mem::size_of::<MaterialState>()];

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Nearest,
    #[default]
    Linear,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddressMode {
    ClampToEdge,
    MirrorRepeat,
    #[default]
    Repeat,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextureMetadata {
    pub image: usize,
    pub sampler: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SamplerMetadata {
    #[serde(default)]
    pub mag_filter: FilterMode,
    #[serde(default)]
    pub min_filter: FilterMode,
    #[serde(default)]
    pub mipmap_filter: FilterMode,
    #[serde(default)]
    pub address_u: AddressMode,
    #[serde(default)]
    pub address_v: AddressMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMetadata {
    pub mime_type: String,
    pub encoded_data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct UploadedGeometry {
    pub id: u32,
    pub material: MaterialKey,
    pub instance_type: InstanceType,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct UploadedOccurrence {
    pub geometry: u32,
    pub transform: ModelTransform,
}

#[derive(Clone, Debug, Default)]
pub struct RenderDataUpload {
    pub geometries: Vec<UploadedGeometry>,
    pub occurrences: Vec<UploadedOccurrence>,
    pub materials: Vec<Material>,
    pub textures: Vec<TextureMetadata>,
    pub samplers: Vec<SamplerMetadata>,
    pub images: Vec<ImageMetadata>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl ModelBounds {
    fn include(&mut self, point: [f32; 3]) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(point[axis]);
            self.max[axis] = self.max[axis].max(point[axis]);
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstalledRenderData {
    pub meshes: Vec<MeshHandle>,
    pub bounds: Option<ModelBounds>,
}

pub struct PreparedRenderData {
    pub stage: ReplacementStage,
    pub installed: InstalledRenderData,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderDataUploadError {
    #[error("render-data packet is malformed: {0}")]
    Malformed(&'static str),
    #[error("render-data packet metadata is invalid: {0}")]
    Metadata(#[from] serde_json::Error),
    #[error("render-data packet contains invalid geometry: {0}")]
    InvalidGeometry(&'static str),
    #[error("render-data packet contains invalid material data")]
    InvalidMaterial,
    #[error("failed to install uploaded render data")]
    Install(#[from] RenderDataError),
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DataSlice {
    offset: u32,
    count: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ByteSlice {
    offset: u32,
    byte_length: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeometryMetadata {
    id: u32,
    material: u32,
    instance_type: [u32; 16],
    positions: DataSlice,
    normals: DataSlice,
    tangents: DataSlice,
    uvs: DataSlice,
    indices: DataSlice,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OccurrenceMetadata {
    geometry: u32,
    transform: [f32; 16],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterialMetadata {
    key: u32,
    base_color_factor: [f32; 4],
    metallic_factor: f32,
    roughness_factor: f32,
    emissive_factor: [f32; 3],
    ior: f32,
    alpha_mode: AlphaMode,
    alpha_cutoff: f32,
    double_sided: bool,
    base_color_texture: Option<TextureReference>,
    metallic_roughness_texture: Option<TextureReference>,
    normal_texture: Option<TextureReference>,
    normal_scale: f32,
    occlusion_texture: Option<TextureReference>,
    occlusion_strength: f32,
    emissive_texture: Option<TextureReference>,
}

impl TryFrom<MaterialMetadata> for Material {
    type Error = RenderDataUploadError;

    fn try_from(value: MaterialMetadata) -> Result<Self, Self::Error> {
        let finite = value
            .base_color_factor
            .iter()
            .chain(value.emissive_factor.iter())
            .chain([
                &value.metallic_factor,
                &value.roughness_factor,
                &value.ior,
                &value.alpha_cutoff,
                &value.normal_scale,
                &value.occlusion_strength,
            ])
            .all(|component| component.is_finite());
        if !finite || (value.ior != 0.0 && value.ior < 1.0) {
            return Err(RenderDataUploadError::InvalidMaterial);
        }
        Ok(Self {
            key: MaterialKey::new(value.key),
            base_color_factor: value.base_color_factor,
            metallic_factor: value.metallic_factor,
            roughness_factor: value.roughness_factor,
            emissive_factor: value.emissive_factor,
            ior: value.ior,
            alpha_mode: value.alpha_mode,
            alpha_cutoff: value.alpha_cutoff,
            double_sided: value.double_sided,
            base_color_texture: value.base_color_texture,
            metallic_roughness_texture: value.metallic_roughness_texture,
            normal_texture: value.normal_texture,
            normal_scale: value.normal_scale,
            occlusion_texture: value.occlusion_texture,
            occlusion_strength: value.occlusion_strength,
            emissive_texture: value.emissive_texture,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageMetadataPacket {
    mime_type: String,
    data: ByteSlice,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PacketMetadata {
    #[serde(default)]
    geometries: Vec<GeometryMetadata>,
    #[serde(default)]
    occurrences: Vec<OccurrenceMetadata>,
    #[serde(default)]
    materials: Vec<MaterialMetadata>,
    #[serde(default)]
    textures: Vec<TextureMetadata>,
    #[serde(default)]
    samplers: Vec<SamplerMetadata>,
    #[serde(default)]
    images: Vec<ImageMetadataPacket>,
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, RenderDataUploadError> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(RenderDataUploadError::Malformed("header is truncated"))?
        .try_into()
        .unwrap();
    Ok(u32::from_le_bytes(raw))
}

fn range(payload: &[u8], offset: u32, byte_length: usize) -> Result<&[u8], RenderDataUploadError> {
    let start = usize::try_from(offset)
        .map_err(|_| RenderDataUploadError::Malformed("data offset exceeds usize"))?;
    let end = start
        .checked_add(byte_length)
        .ok_or(RenderDataUploadError::Malformed("data range overflows"))?;
    payload
        .get(start..end)
        .ok_or(RenderDataUploadError::Malformed(
            "data range is out of bounds",
        ))
}

fn f32_vectors<const N: usize>(
    payload: &[u8],
    slice: DataSlice,
) -> Result<Vec<[f32; N]>, RenderDataUploadError> {
    if slice.offset % 4 != 0 {
        return Err(RenderDataUploadError::Malformed("float data is unaligned"));
    }
    let count = usize::try_from(slice.count)
        .map_err(|_| RenderDataUploadError::Malformed("element count exceeds usize"))?;
    let byte_length = count
        .checked_mul(N)
        .and_then(|value| value.checked_mul(4))
        .ok_or(RenderDataUploadError::Malformed(
            "float data size overflows",
        ))?;
    let bytes = range(payload, slice.offset, byte_length)?;
    Ok(bytes
        .chunks_exact(N * 4)
        .map(|chunk| {
            std::array::from_fn(|lane| {
                f32::from_le_bytes(chunk[lane * 4..lane * 4 + 4].try_into().unwrap())
            })
        })
        .collect())
}

fn u32_values(payload: &[u8], slice: DataSlice) -> Result<Vec<u32>, RenderDataUploadError> {
    if slice.offset % 4 != 0 {
        return Err(RenderDataUploadError::Malformed(
            "integer data is unaligned",
        ));
    }
    let byte_length = usize::try_from(slice.count)
        .ok()
        .and_then(|count| count.checked_mul(4))
        .ok_or(RenderDataUploadError::Malformed(
            "integer data size overflows",
        ))?;
    Ok(range(payload, slice.offset, byte_length)?
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

/// Decode the generic binary render-data packet accepted by core.
pub fn decode_render_data_packet(bytes: &[u8]) -> Result<RenderDataUpload, RenderDataUploadError> {
    if bytes.len() < HEADER_BYTES || word(bytes, 0)? != MAGIC {
        return Err(RenderDataUploadError::Malformed("magic is invalid"));
    }
    if word(bytes, 4)? != VERSION {
        return Err(RenderDataUploadError::Malformed("version is unsupported"));
    }
    let metadata_len = usize::try_from(word(bytes, 8)?)
        .map_err(|_| RenderDataUploadError::Malformed("metadata size exceeds usize"))?;
    let payload_len = usize::try_from(word(bytes, 12)?)
        .map_err(|_| RenderDataUploadError::Malformed("payload size exceeds usize"))?;
    let metadata_end = HEADER_BYTES
        .checked_add(metadata_len)
        .ok_or(RenderDataUploadError::Malformed("metadata size overflows"))?;
    let payload_start = metadata_end.checked_add(3).map(|value| value & !3).ok_or(
        RenderDataUploadError::Malformed("payload alignment overflows"),
    )?;
    let packet_end = payload_start
        .checked_add(payload_len)
        .ok_or(RenderDataUploadError::Malformed("packet size overflows"))?;
    if packet_end != bytes.len() {
        return Err(RenderDataUploadError::Malformed(
            "packet length is not exact",
        ));
    }
    let metadata: PacketMetadata = serde_json::from_slice(
        bytes
            .get(HEADER_BYTES..metadata_end)
            .ok_or(RenderDataUploadError::Malformed("metadata is truncated"))?,
    )?;
    let payload = &bytes[payload_start..packet_end];

    let mut geometry_ids = HashSet::new();
    let geometries = metadata
        .geometries
        .into_iter()
        .map(|geometry| {
            if !geometry_ids.insert(geometry.id) {
                return Err(RenderDataUploadError::InvalidGeometry(
                    "geometry id is duplicated",
                ));
            }
            Ok(UploadedGeometry {
                id: geometry.id,
                material: MaterialKey::new(geometry.material),
                instance_type: InstanceType {
                    words: geometry.instance_type,
                },
                positions: f32_vectors(payload, geometry.positions)?,
                normals: f32_vectors(payload, geometry.normals)?,
                tangents: f32_vectors(payload, geometry.tangents)?,
                uvs: f32_vectors(payload, geometry.uvs)?,
                indices: u32_values(payload, geometry.indices)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let occurrences = metadata
        .occurrences
        .into_iter()
        .map(|occurrence| UploadedOccurrence {
            geometry: occurrence.geometry,
            transform: std::array::from_fn(|column| {
                std::array::from_fn(|row| occurrence.transform[column * 4 + row])
            }),
        })
        .collect();
    let materials = metadata
        .materials
        .into_iter()
        .map(Material::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let images = metadata
        .images
        .into_iter()
        .map(|image| -> Result<_, RenderDataUploadError> {
            let byte_length = usize::try_from(image.data.byte_length)
                .map_err(|_| RenderDataUploadError::Malformed("image size exceeds usize"))?;
            Ok(ImageMetadata {
                mime_type: image.mime_type,
                encoded_data: range(payload, image.data.offset, byte_length)?.to_vec(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RenderDataUpload {
        geometries,
        occurrences,
        materials,
        textures: metadata.textures,
        samplers: metadata.samplers,
        images,
    })
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

/// Prepare a complete CPU-side replacement without changing live render data.
pub fn prepare_render_data(
    target: &RenderData,
    upload: &RenderDataUpload,
) -> Result<PreparedRenderData, RenderDataUploadError> {
    let mut stage = target.replacement_stage()?;
    let mut handles = HashMap::new();
    let mut mesh_handles = Vec::with_capacity(upload.geometries.len());
    let mut first = HashMap::new();
    for occurrence in &upload.occurrences {
        first
            .entry(occurrence.geometry)
            .or_insert(occurrence.transform);
    }
    for geometry in &upload.geometries {
        let transform = *first
            .get(&geometry.id)
            .ok_or(RenderDataUploadError::InvalidGeometry(
                "geometry has no occurrence",
            ))?;
        let created = stage.create_mesh(MeshCreateInfo {
            positions: &geometry.positions,
            normals: &geometry.normals,
            tangents: &geometry.tangents,
            uvs: &geometry.uvs,
            indices: &geometry.indices,
            material: geometry.material,
            default_instance_type: geometry.instance_type,
            default_transform: transform,
        })?;
        handles.insert(geometry.id, created.mesh);
        mesh_handles.push(created.mesh);
    }

    let geometries: HashMap<_, _> = upload
        .geometries
        .iter()
        .map(|geometry| (geometry.id, geometry))
        .collect();
    let mut consumed = HashSet::new();
    let mut focus_points = Vec::new();
    let mut bounds: Option<ModelBounds> = None;
    for occurrence in &upload.occurrences {
        let mesh =
            *handles
                .get(&occurrence.geometry)
                .ok_or(RenderDataUploadError::InvalidGeometry(
                    "occurrence has no geometry",
                ))?;
        if !consumed.insert(occurrence.geometry) {
            let instance_type = stage.mesh(mesh).unwrap().default_instance_type;
            stage.create_instance(mesh, occurrence.transform, instance_type)?;
        }
        let geometry = geometries[&occurrence.geometry];
        let transform = Mat4::from(occurrence.transform);
        focus_points.extend(geometry.positions.iter().map(|position| {
            let point = transform.transform_point3(Vec3::from(*position));
            [point.x, point.y, point.z]
        }));
        let local = stage.mesh(mesh).unwrap().local_aabb;
        for x in [local.min[0], local.max[0]] {
            for y in [local.min[1], local.max[1]] {
                for z in [local.min[2], local.max[2]] {
                    let point = transform.transform_point3(Vec3::new(x, y, z));
                    let point = [point.x, point.y, point.z];
                    if let Some(existing) = bounds.as_mut() {
                        existing.include(point);
                    } else {
                        bounds = Some(ModelBounds {
                            min: point,
                            max: point,
                        });
                    }
                }
            }
        }
    }
    bounds = focus_bounds(&focus_points).or(bounds);
    Ok(PreparedRenderData {
        stage,
        installed: InstalledRenderData {
            meshes: mesh_handles,
            bounds,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(metadata: serde_json::Value, payload: &[u8]) -> Vec<u8> {
        let metadata = serde_json::to_vec(&metadata).unwrap();
        let payload_offset = (HEADER_BYTES + metadata.len() + 3) & !3;
        let mut packet = vec![0; payload_offset + payload.len()];
        packet[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        packet[4..8].copy_from_slice(&VERSION.to_le_bytes());
        packet[8..12].copy_from_slice(&(metadata.len() as u32).to_le_bytes());
        packet[12..16].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        packet[HEADER_BYTES..HEADER_BYTES + metadata.len()].copy_from_slice(&metadata);
        packet[payload_offset..].copy_from_slice(payload);
        packet
    }

    #[test]
    fn generic_packet_decodes_typed_streams_without_format_knowledge() {
        let floats: Vec<f32> = [
            0., 0., 0., 1., 0., 0., 0., 1., 0., // positions
            0., 0., 1., 0., 0., 1., 0., 0., 1., // normals
            1., 0., 0., 1., 1., 0., 0., 1., 1., 0., 0., 1., // tangents
            0., 0., 1., 0., 0., 1., // uvs
        ]
        .into();
        let mut payload = bytemuck::cast_slice(&floats).to_vec();
        payload.extend_from_slice(bytemuck::cast_slice(&[0u32, 1, 2]));
        let upload = decode_render_data_packet(&packet(
            serde_json::json!({
                "geometries":[{
                    "id":7,"material":0,"instanceType":[1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                    "positions":{"offset":0,"count":3},
                    "normals":{"offset":36,"count":3},
                    "tangents":{"offset":72,"count":3},
                    "uvs":{"offset":120,"count":3},
                    "indices":{"offset":144,"count":3}
                }],
                "occurrences":[{"geometry":7,"transform":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}],
                "materials":[],"textures":[],"samplers":[],"images":[]
            }),
            &payload,
        ))
        .unwrap();
        assert_eq!(upload.geometries[0].positions.len(), 3);
        assert_eq!(upload.geometries[0].indices, [0, 1, 2]);
        assert_eq!(upload.occurrences[0].geometry, 7);
    }

    #[test]
    fn packet_length_and_ranges_are_exact() {
        let mut invalid = packet(serde_json::json!({}), &[]);
        invalid.push(0);
        assert!(matches!(
            decode_render_data_packet(&invalid),
            Err(RenderDataUploadError::Malformed(
                "packet length is not exact"
            ))
        ));
    }
}
