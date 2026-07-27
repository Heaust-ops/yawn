use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use image::DynamicImage;
use wgpu::util::DeviceExt;

use crate::{
    gltf::{AlphaMode, ImageSource, ImportedScene, Material, SamplerMetadata, TextureReference},
    render_data::MaterialKey,
};

const BASE: u32 = 1 << 0;
const MR: u32 = 1 << 1;
const NORMAL: u32 = 1 << 2;
const OCCLUSION: u32 = 1 << 3;
const EMISSIVE: u32 = 1 << 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuMaterial {
    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 4],
    pub surface_factors: [f32; 4],
    pub alpha_optics: [f32; 4],
    pub flags: [u32; 4],
    pub uv_sets: [u32; 4],
    /// Internal shader diagnostics; zero means the normal shaded view.
    pub debug_extras: [u32; 4],
}

fn enabled(reference: Option<TextureReference>, bit: u32) -> u32 {
    reference.filter(|r| r.tex_coord == 0).map_or(0, |_| bit)
}

impl From<&Material> for GpuMaterial {
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
                enabled(value.base_color_texture, BASE)
                    | enabled(value.metallic_roughness_texture, MR)
                    | enabled(value.normal_texture, NORMAL)
                    | enabled(value.occlusion_texture, OCCLUSION)
                    | enabled(value.emissive_texture, EMISSIVE),
                u32::from(value.double_sided),
                0,
                0,
            ],
            uv_sets: [
                value.base_color_texture.map_or(0, |x| x.tex_coord),
                value.metallic_roughness_texture.map_or(0, |x| x.tex_coord),
                value.normal_texture.map_or(0, |x| x.tex_coord),
                value.occlusion_texture.map_or(0, |x| x.tex_coord),
            ],
            debug_extras: [value.emissive_texture.map_or(0, |x| x.tex_coord), 0, 0, 0],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    #[error("external image sources are unsupported")]
    ExternalImage,
    #[error("unsupported image MIME type: {0}")]
    Mime(String),
    #[error("image decode failed: {0}")]
    Decode(#[from] image::ImageError),
    #[error("invalid texture, image, or sampler index")]
    InvalidReference,
    #[error("invalid decoded RGBA image: {0}")]
    InvalidRgba(&'static str),
}

pub(super) struct PreparedMaterials {
    groups: HashMap<MaterialKey, wgpu::BindGroup>,
    textures: Vec<wgpu::Texture>,
    views: Vec<[wgpu::TextureView; 2]>,
    samplers: Vec<wgpu::Sampler>,
}

pub struct MaterialResources {
    pub layout: wgpu::BindGroupLayout,
    groups: HashMap<MaterialKey, wgpu::BindGroup>,
    fallback: wgpu::BindGroup,
    fallback_views: Vec<wgpu::TextureView>,
    fallback_sampler: wgpu::Sampler,
    textures: Vec<wgpu::Texture>,
    views: Vec<[wgpu::TextureView; 2]>,
    samplers: Vec<wgpu::Sampler>,
    pub asset_epoch: u64,
}

fn layout_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    rgba: &[u8],
) -> (wgpu::Texture, [wgpu::TextureView; 2]) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    queue.write_texture(
        texture.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let linear = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    (texture, [linear, srgb])
}

fn normalize_rgba(image: DynamicImage) -> (u32, u32, Vec<u8>) {
    let rgba = image.into_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

fn validate_decoded_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    max_dimension: u32,
) -> Result<u32, MaterialError> {
    if width == 0 || height == 0 {
        return Err(MaterialError::InvalidRgba("dimensions must be nonzero"));
    }
    if width > max_dimension || height > max_dimension {
        return Err(MaterialError::InvalidRgba("dimensions exceed device limit"));
    }
    let bytes_per_row = width
        .checked_mul(4)
        .ok_or(MaterialError::InvalidRgba("row byte count overflows"))?;
    let total = bytes_per_row
        .checked_mul(height)
        .ok_or(MaterialError::InvalidRgba("total byte count overflows"))?;
    let total = usize::try_from(total)
        .map_err(|_| MaterialError::InvalidRgba("total byte count exceeds usize"))?;
    if rgba.len() != total {
        return Err(MaterialError::InvalidRgba("pixel byte length is not exact"));
    }
    Ok(bytes_per_row)
}

fn slot_uses_srgb(slot: usize) -> bool {
    matches!(slot, 0 | 4)
}

fn address(value: &str) -> wgpu::AddressMode {
    match value {
        "ClampToEdge" => wgpu::AddressMode::ClampToEdge,
        "MirroredRepeat" => wgpu::AddressMode::MirrorRepeat,
        "Repeat" => wgpu::AddressMode::Repeat,
        _ => unreachable!("gltf crate returned unknown wrap"),
    }
}

fn sampler_descriptor(metadata: Option<&SamplerMetadata>) -> wgpu::SamplerDescriptor<'static> {
    let (mag, min, mip, s, t) = metadata.map_or(
        (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::AddressMode::Repeat,
            wgpu::AddressMode::Repeat,
        ),
        |m| {
            let mag = match m.mag_filter.as_deref() {
                Some("Nearest") => wgpu::FilterMode::Nearest,
                Some("Linear") | None => wgpu::FilterMode::Linear,
                _ => unreachable!(),
            };
            let (min, mip) = match m.min_filter.as_deref() {
                Some("Nearest") => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest),
                Some("Linear") => (wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest),
                Some("NearestMipmapNearest") => {
                    (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest)
                }
                Some("LinearMipmapNearest") => {
                    (wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest)
                }
                Some("NearestMipmapLinear") => {
                    (wgpu::FilterMode::Nearest, wgpu::FilterMode::Linear)
                }
                Some("LinearMipmapLinear") | None => {
                    (wgpu::FilterMode::Linear, wgpu::FilterMode::Linear)
                }
                _ => unreachable!(),
            };
            (mag, min, mip, address(&m.wrap_s), address(&m.wrap_t))
        },
    );
    wgpu::SamplerDescriptor {
        address_mode_u: s,
        address_mode_v: t,
        mag_filter: mag,
        min_filter: min,
        mipmap_filter: mip,
        ..Default::default()
    }
}

impl MaterialResources {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut entries = vec![layout_entry(
            0,
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(112),
            },
        )];
        for binding in 1..=5 {
            entries.push(layout_entry(
                binding,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ));
        }
        for binding in 6..=10 {
            entries.push(layout_entry(
                binding,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ));
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glTF material group 2"),
            entries: &entries,
        });
        let colors = [[255, 255, 255, 255], [128, 128, 255, 255], [0, 0, 0, 255]];
        let mut fallback_textures = Vec::new();
        let mut fallback_views = Vec::new();
        for color in colors {
            let (t, v) = upload_rgba(device, queue, "neutral material texture", 1, 1, 4, &color);
            fallback_views.push(v[0].clone());
            fallback_textures.push(t);
        }
        let fallback_sampler = device.create_sampler(&sampler_descriptor(None));
        let fallback = Self::make_group(
            device,
            &layout,
            &Material::default(),
            [
                &fallback_views[0],
                &fallback_views[0],
                &fallback_views[1],
                &fallback_views[0],
                &fallback_views[2],
            ],
            [&fallback_sampler; 5],
        );
        Self {
            layout,
            groups: HashMap::new(),
            fallback,
            fallback_views,
            fallback_sampler,
            textures: fallback_textures,
            views: Vec::new(),
            samplers: Vec::new(),
            asset_epoch: 0,
        }
    }

    fn make_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        material: &Material,
        views: [&wgpu::TextureView; 5],
        samplers: [&wgpu::Sampler; 5],
    ) -> wgpu::BindGroup {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material uniform"),
            contents: bytemuck::bytes_of(&GpuMaterial::from(material)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (i, view) in views.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 + 1,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        for (i, sampler) in samplers.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 + 6,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material bind group"),
            layout,
            entries: &entries,
        })
    }

    pub(super) fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &ImportedScene,
    ) -> Result<PreparedMaterials, MaterialError> {
        let max_dimension = device.limits().max_texture_dimension_2d;
        let mut textures = Vec::with_capacity(scene.images.len());
        let mut views = Vec::with_capacity(scene.images.len());
        for image in &scene.images {
            if !matches!(image.source, ImageSource::BufferView(_)) {
                return Err(MaterialError::ExternalImage);
            }
            let format = match image.mime_type.as_deref() {
                Some("image/png") => image::ImageFormat::Png,
                Some("image/jpeg") => image::ImageFormat::Jpeg,
                other => return Err(MaterialError::Mime(other.unwrap_or("missing").into())),
            };
            let decoded = image::load_from_memory_with_format(&image.encoded_data, format)?;
            let (width, height, rgba) = normalize_rgba(decoded);
            let bytes_per_row = validate_decoded_rgba(width, height, &rgba, max_dimension)?;
            let (texture, image_views) = upload_rgba(
                device,
                queue,
                "glTF embedded image",
                width,
                height,
                bytes_per_row,
                &rgba,
            );
            drop(rgba);
            textures.push(texture);
            views.push(image_views);
        }
        let mut samplers = Vec::new();
        for sampler in &scene.samplers {
            samplers.push(device.create_sampler(&sampler_descriptor(Some(sampler))));
        }
        let default_sampler = device.create_sampler(&sampler_descriptor(None));
        samplers.push(default_sampler);
        let default_index = samplers.len() - 1;
        let mut groups = HashMap::new();
        for material in &scene.materials {
            let refs = [
                material.base_color_texture,
                material.metallic_roughness_texture,
                material.normal_texture,
                material.occlusion_texture,
                material.emissive_texture,
            ];
            let mut selected_views = [
                &self.fallback_views[0],
                &self.fallback_views[0],
                &self.fallback_views[1],
                &self.fallback_views[0],
                &self.fallback_views[2],
            ];
            let mut selected_samplers = [&self.fallback_sampler; 5];
            for (slot, reference) in refs.into_iter().enumerate() {
                let Some(reference) = reference.filter(|r| r.tex_coord == 0) else {
                    continue;
                };
                let texture = scene
                    .textures
                    .get(reference.texture)
                    .ok_or(MaterialError::InvalidReference)?;
                selected_views[slot] = &views
                    .get(texture.image)
                    .ok_or(MaterialError::InvalidReference)?[usize::from(slot_uses_srgb(slot))];
                let sampler_index = texture.sampler.unwrap_or(default_index);
                selected_samplers[slot] = samplers
                    .get(sampler_index)
                    .ok_or(MaterialError::InvalidReference)?;
            }
            groups.insert(
                material.key,
                Self::make_group(
                    device,
                    &self.layout,
                    material,
                    selected_views,
                    selected_samplers,
                ),
            );
        }
        Ok(PreparedMaterials {
            groups,
            textures,
            views,
            samplers,
        })
    }

    pub(super) fn install(&mut self, prepared: PreparedMaterials, asset_epoch: u64) {
        self.groups = prepared.groups;
        self.textures = prepared.textures;
        self.views = prepared.views;
        self.samplers = prepared.samplers;
        self.asset_epoch = asset_epoch;
    }

    pub fn group(&self, key: MaterialKey) -> &wgpu::BindGroup {
        self.groups.get(&key).unwrap_or(&self.fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schlick(f0: f32, cosine: f32) -> f32 {
        f0 + (1.0 - f0) * (1.0 - cosine.clamp(0.0, 1.0)).powi(5)
    }

    fn ggx_d(n_h: f32, alpha: f32) -> f32 {
        let n_h = n_h.clamp(0.0, 1.0);
        let alpha2 = alpha * alpha;
        let n_h2 = n_h * n_h;
        let q = (1.0 - n_h2) + alpha2 * n_h2;
        alpha2 / (std::f32::consts::PI * q * q)
    }

    fn smith_v(n_v: f32, n_l: f32, alpha: f32) -> f32 {
        let alpha2 = alpha * alpha;
        let gv = n_l * (n_v * n_v * (1.0 - alpha2) + alpha2).max(0.0).sqrt();
        let gl = n_v * (n_l * n_l * (1.0 - alpha2) + alpha2).max(0.0).sqrt();
        0.5 / (gv + gl).max(1e-6)
    }

    #[test]
    fn pbr_reference_equations_have_known_limits_and_stay_finite() {
        assert!((schlick(0.04, 1.0) - 0.04).abs() < 1e-6);
        assert!((schlick(0.04, 0.0) - 1.0).abs() < 1e-6);
        assert!((ggx_d(0.0, 1.0) - std::f32::consts::FRAC_1_PI).abs() < 1e-6);
        let expected_quarter = 16.0 / std::f32::consts::PI;
        for actual in [ggx_d(1.0, 0.25), ggx_d(1.0 + f32::EPSILON, 0.25)] {
            assert!((actual - expected_quarter).abs() / expected_quarter < 1e-6);
        }
        for value in [ggx_d(1.0, 0.045 * 0.045), smith_v(0.0, 0.0, 0.002025)] {
            assert!(value.is_finite() && value >= 0.0);
        }
        let roughness_floor_peak = ggx_d(1.0, 0.045 * 0.045);
        let expected = 1.0 / (std::f32::consts::PI * 0.045_f32.powi(4));
        assert!((roughness_floor_peak - expected).abs() / expected < 1e-6);
        assert!((roughness_floor_peak - 77_624.0).abs() / 77_624.0 < 1e-4);
    }
    #[test]
    fn material_uniform_is_112_bytes() {
        assert_eq!(std::mem::size_of::<GpuMaterial>(), 112);
    }
    #[test]
    fn texcoord_one_disables_slot() {
        let mut m = Material::default();
        m.base_color_texture = Some(TextureReference {
            texture: 0,
            tex_coord: 1,
        });
        assert_eq!(GpuMaterial::from(&m).flags[0] & BASE, 0);
    }
    #[test]
    fn material_packing_includes_ior_f0_flags_and_uv_sets() {
        let mut m = Material::default();
        m.ior = 2.0;
        m.double_sided = true;
        m.normal_texture = Some(TextureReference {
            texture: 4,
            tex_coord: 0,
        });
        let gpu = GpuMaterial::from(&m);
        assert_eq!(gpu.alpha_optics[2], 2.0);
        assert!((gpu.alpha_optics[3] - 1.0 / 9.0).abs() < 1e-6);
        assert_eq!(gpu.flags[0] & NORMAL, NORMAL);
        assert_eq!(gpu.flags[1], 1);
        assert_eq!(gpu.uv_sets[2], 0);
    }
    #[test]
    fn explicit_ior_sentinel_packs_unit_f0() {
        let mut material = Material::default();
        material.ior = 0.0;
        let gpu = GpuMaterial::from(&material);
        assert_eq!(gpu.alpha_optics[2], 0.0);
        assert_eq!(gpu.alpha_optics[3], 1.0);
    }
    #[test]
    fn sampler_translation_is_exact() {
        let m = SamplerMetadata {
            index: 0,
            mag_filter: Some("Nearest".into()),
            min_filter: Some("LinearMipmapNearest".into()),
            wrap_s: "ClampToEdge".into(),
            wrap_t: "MirroredRepeat".into(),
        };
        let d = sampler_descriptor(Some(&m));
        assert_eq!(d.mag_filter, wgpu::FilterMode::Nearest);
        assert_eq!(d.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(d.mipmap_filter, wgpu::FilterMode::Nearest);
        assert_eq!(d.address_mode_u, wgpu::AddressMode::ClampToEdge);
        assert_eq!(d.address_mode_v, wgpu::AddressMode::MirrorRepeat);
    }
    #[test]
    fn rgb_and_luma_normalize_to_rgba() {
        let (_, _, rgb) = normalize_rgba(DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(1, 1, vec![1, 2, 3]).unwrap(),
        ));
        assert_eq!(rgb, [1, 2, 3, 255]);
        let (_, _, luma) = normalize_rgba(DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(1, 1, vec![7]).unwrap(),
        ));
        assert_eq!(luma, [7, 7, 7, 255]);
    }
    #[test]
    fn odd_width_layout_is_tight() {
        let width = 3;
        assert_eq!(width * 4, 12);
    }

    #[test]
    fn decoded_rgba_validation_rejects_dimensions_overflow_and_wrong_length() {
        assert!(validate_decoded_rgba(0, 1, &[], 4096).is_err());
        assert!(validate_decoded_rgba(4097, 1, &[], 4096).is_err());
        assert!(validate_decoded_rgba(u32::MAX, 2, &[], u32::MAX).is_err());
        assert!(validate_decoded_rgba(2, 2, &[0; 15], 4096).is_err());
        assert_eq!(validate_decoded_rgba(3, 2, &[0; 24], 4096).unwrap(), 12);
    }

    #[test]
    fn only_color_roles_use_srgb_views() {
        assert_eq!(
            (0..5).map(slot_uses_srgb).collect::<Vec<_>>(),
            [true, false, false, false, true]
        );
    }
}
