use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU64;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::gpu::Wgpu;
use crate::graph::{Binding, Extent, Pass, RenderGraph, RenderPipeline, Texture};
use crate::render_data::RenderData;

pub struct GpuResources {
    pub buffers: HashMap<String, GpuBuffer>,
    pub textures: Vec<GpuTexture>,
    pub texture_slots: HashMap<String, usize>,
    pub samplers: HashMap<String, wgpu::Sampler>,
    pub render_pipelines: HashMap<String, wgpu::RenderPipeline>,
    pub compute_pipelines: HashMap<String, wgpu::ComputePipeline>,
    pub passes: Vec<GpuPass>,
}

pub struct GpuBuffer {
    pub buffer: wgpu::Buffer,
    pub source: String,
}

pub struct GpuTexture {
    pub _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

pub enum GpuPass {
    Render(wgpu::RenderBundle),
    Compute {
        pipeline: wgpu::ComputePipeline,
        bind_groups: Vec<(u32, wgpu::BindGroup)>,
    },
}

impl GpuResources {
    pub fn activate(graph: &RenderGraph, gpu: &Wgpu, data: &RenderData) -> Result<Self, String> {
        let mut buffers = HashMap::new();
        for source in &graph.resources.buffers {
            let rows = data.rows(&source.array).ok_or("GRAPH_ARRAY_UNKNOWN")?;
            let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&source.id),
                size: u64::from(rows.bytes.max(4)),
                usage: buffer_usage(&source.usage)?,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&buffer, 0, data.bytes(&source.array).unwrap());
            buffers.insert(
                source.id.clone(),
                GpuBuffer {
                    buffer,
                    source: source.array.clone(),
                },
            );
        }

        let mut textures = Vec::new();
        let mut physical_slots = HashMap::new();
        let mut texture_slots = HashMap::new();
        for source in &graph.resources.textures {
            let physical = match physical_slots.get(&source.slot) {
                Some(&physical) => physical,
                None => {
                    let descriptor = texture_descriptor(source, gpu.width, gpu.height)?;
                    let texture = gpu.device.create_texture(&descriptor);
                    let physical = textures.len();
                    textures.push(GpuTexture {
                        view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                        _texture: texture,
                    });
                    physical_slots.insert(source.slot, physical);
                    physical
                }
            };
            texture_slots.insert(source.id.clone(), physical);
        }

        let mut samplers = HashMap::new();
        for source in &graph.resources.samplers {
            samplers.insert(
                source.id.clone(),
                gpu.device
                    .create_sampler(&sampler_descriptor(&source.id, &source.descriptor)?),
            );
        }

        let render_pipelines = graph
            .pipelines
            .render
            .iter()
            .map(|source| {
                create_render_pipeline(source, gpu).map(|pipeline| (source.id.clone(), pipeline))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let compute_pipelines = graph
            .pipelines
            .compute
            .iter()
            .map(|source| {
                let module = gpu
                    .device
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(&source.id),
                        source: wgpu::ShaderSource::Wgsl(source.code.clone().into()),
                    });
                let pipeline =
                    gpu.device
                        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                            label: Some(&source.id),
                            layout: None,
                            module: &module,
                            entry_point: Some(&source.entry),
                            compilation_options: Default::default(),
                            cache: None,
                        });
                Ok::<_, String>((source.id.clone(), pipeline))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        let mut resources = Self {
            buffers,
            textures,
            texture_slots,
            samplers,
            render_pipelines,
            compute_pipelines,
            passes: Vec::new(),
        };
        for pass in &graph.passes {
            let compiled = match pass.kind.as_str() {
                "render" => GpuPass::Render(resources.render_bundle(graph, pass, gpu)?),
                "compute" => {
                    let pipeline = resources
                        .compute_pipelines
                        .get(&pass.pipeline)
                        .ok_or("GRAPH_PIPELINE")?
                        .clone();
                    let bind_groups = resources.bind_groups(
                        pass,
                        |group| pipeline.get_bind_group_layout(group),
                        gpu,
                    )?;
                    GpuPass::Compute {
                        pipeline,
                        bind_groups,
                    }
                }
                _ => return Err("GRAPH_PASS".into()),
            };
            resources.passes.push(compiled);
        }
        Ok(resources)
    }

    pub fn texture_view(&self, id: &str) -> Option<&wgpu::TextureView> {
        self.texture_slots
            .get(id)
            .and_then(|slot| self.textures.get(*slot))
            .map(|texture| &texture.view)
    }

    fn render_bundle(
        &self,
        graph: &RenderGraph,
        pass: &Pass,
        gpu: &Wgpu,
    ) -> Result<wgpu::RenderBundle, String> {
        let pipeline = self
            .render_pipelines
            .get(&pass.pipeline)
            .ok_or("GRAPH_PIPELINE")?;
        let declaration = graph
            .pipelines
            .render
            .iter()
            .find(|pipeline| pipeline.id == pass.pipeline)
            .ok_or("GRAPH_PIPELINE")?;
        let color_formats = pass
            .color
            .iter()
            .map(|attachment| attachment_format(graph, &attachment.resource, gpu.format).map(Some))
            .collect::<Result<Vec<_>, _>>()?;
        let depth_stencil = pass
            .depth
            .as_ref()
            .map(|attachment| {
                Ok::<_, String>(wgpu::RenderBundleDepthStencil {
                    format: attachment_format(graph, &attachment.resource, gpu.format)?,
                    depth_read_only: false,
                    stencil_read_only: true,
                })
            })
            .transpose()?;
        let mut encoder =
            gpu.device
                .create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some(&pass.id),
                    color_formats: &color_formats,
                    depth_stencil,
                    sample_count: multisample(&declaration.multisample)?.count,
                    multiview: None,
                });
        encoder.set_pipeline(pipeline);
        for (group, bind_group) in
            self.bind_groups(pass, |group| pipeline.get_bind_group_layout(group), gpu)?
        {
            encoder.set_bind_group(group, &bind_group, &[]);
        }
        for binding in &pass.vertex_buffers {
            let buffer = &self
                .buffers
                .get(&binding.resource)
                .ok_or("GRAPH_RESOURCE_UNKNOWN")?
                .buffer;
            encoder.set_vertex_buffer(binding.slot, buffer.slice(binding.offset..));
        }
        if let Some(binding) = &pass.index_buffer {
            let buffer = &self
                .buffers
                .get(&binding.resource)
                .ok_or("GRAPH_RESOURCE_UNKNOWN")?
                .buffer;
            encoder.set_index_buffer(
                buffer.slice(binding.offset..),
                parse(&binding.format, "GRAPH_INDEX_FORMAT")?,
            );
            encoder.draw_indexed(
                pass.draw.first_index..pass.draw.first_index + pass.draw.indices,
                pass.draw.base_vertex,
                pass.draw.first_instance..pass.draw.first_instance + pass.draw.instances,
            );
        } else {
            encoder.draw(
                pass.draw.first_vertex..pass.draw.first_vertex + pass.draw.vertices,
                pass.draw.first_instance..pass.draw.first_instance + pass.draw.instances,
            );
        }
        Ok(encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some(&pass.id),
        }))
    }

    fn bind_groups(
        &self,
        pass: &Pass,
        layout: impl Fn(u32) -> wgpu::BindGroupLayout,
        gpu: &Wgpu,
    ) -> Result<Vec<(u32, wgpu::BindGroup)>, String> {
        let mut groups: BTreeMap<u32, Vec<&Binding>> = BTreeMap::new();
        for binding in &pass.bindings {
            groups.entry(binding.group).or_default().push(binding);
        }
        groups
            .into_iter()
            .map(|(group, bindings)| {
                let entries = bindings
                    .into_iter()
                    .map(|binding| {
                        let resource = if let Some(buffer) = self.buffers.get(&binding.resource) {
                            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &buffer.buffer,
                                offset: binding.offset,
                                size: binding.size.and_then(NonZeroU64::new),
                            })
                        } else if let Some(view) = self.texture_view(&binding.resource) {
                            wgpu::BindingResource::TextureView(view)
                        } else if let Some(sampler) = self.samplers.get(&binding.resource) {
                            wgpu::BindingResource::Sampler(sampler)
                        } else {
                            return Err("GRAPH_RESOURCE_UNKNOWN".into());
                        };
                        Ok(wgpu::BindGroupEntry {
                            binding: binding.binding,
                            resource,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&pass.id),
                    layout: &layout(group),
                    entries: &entries,
                });
                Ok((group, bind_group))
            })
            .collect()
    }
}

fn create_render_pipeline(
    source: &RenderPipeline,
    gpu: &Wgpu,
) -> Result<wgpu::RenderPipeline, String> {
    let module = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&source.id),
            source: wgpu::ShaderSource::Wgsl(source.code.clone().into()),
        });
    let attributes = source
        .vertex
        .buffers
        .iter()
        .map(|buffer| {
            buffer
                .attributes
                .iter()
                .map(|attribute| {
                    Ok(wgpu::VertexAttribute {
                        format: parse(&attribute.format, "GRAPH_VERTEX_FORMAT")?,
                        offset: attribute.offset,
                        shader_location: attribute.shader_location,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let layouts = source
        .vertex
        .buffers
        .iter()
        .zip(&attributes)
        .map(|(buffer, attributes)| {
            Ok(wgpu::VertexBufferLayout {
                array_stride: buffer.array_stride,
                step_mode: parse(&buffer.step_mode, "GRAPH_VERTEX_STEP")?,
                attributes,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let targets = source
        .fragment
        .targets
        .iter()
        .map(|target| {
            let format = if target.format == "canvas" {
                gpu.format
            } else {
                parse(&target.format, "GRAPH_TEXTURE_FORMAT")?
            };
            let blend = (!target.blend.is_null())
                .then(|| {
                    serde_json::from_value::<wgpu::BlendState>(target.blend.clone())
                        .map_err(|_| String::from("GRAPH_BLEND"))
                })
                .transpose()?;
            let write_mask = match target.write_mask {
                Some(bits) => wgpu::ColorWrites::from_bits(bits).ok_or("GRAPH_WRITE_MASK")?,
                None => wgpu::ColorWrites::ALL,
            };
            Ok(Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&source.id),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(&source.vertex.entry),
                compilation_options: Default::default(),
                buffers: &layouts,
            },
            primitive: primitive(&source.primitive)?,
            depth_stencil: depth_stencil(&source.depth_stencil)?,
            multisample: multisample(&source.multisample)?,
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(&source.fragment.entry),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview: None,
            cache: None,
        }))
}

fn buffer_usage(names: &[String]) -> Result<wgpu::BufferUsages, String> {
    names
        .iter()
        .try_fold(wgpu::BufferUsages::COPY_DST, |usage, name| {
            Ok(usage
                | match name.as_str() {
                    "uniform" => wgpu::BufferUsages::UNIFORM,
                    "storage" => wgpu::BufferUsages::STORAGE,
                    "vertex" => wgpu::BufferUsages::VERTEX,
                    "index" => wgpu::BufferUsages::INDEX,
                    "indirect" => wgpu::BufferUsages::INDIRECT,
                    "copySrc" => wgpu::BufferUsages::COPY_SRC,
                    _ => return Err("GRAPH_BUFFER_USAGE".into()),
                })
        })
}

fn texture_usage(names: &[String]) -> Result<wgpu::TextureUsages, String> {
    names
        .iter()
        .try_fold(wgpu::TextureUsages::empty(), |usage, name| {
            Ok(usage
                | match name.as_str() {
                    "render" => wgpu::TextureUsages::RENDER_ATTACHMENT,
                    "sampled" => wgpu::TextureUsages::TEXTURE_BINDING,
                    "storage" => wgpu::TextureUsages::STORAGE_BINDING,
                    "copySrc" => wgpu::TextureUsages::COPY_SRC,
                    "copyDst" => wgpu::TextureUsages::COPY_DST,
                    _ => return Err("GRAPH_TEXTURE_USAGE".into()),
                })
        })
}

fn texture_descriptor(
    source: &Texture,
    width: u32,
    height: u32,
) -> Result<wgpu::TextureDescriptor<'_>, String> {
    let value = |index, canvas| -> Result<u32, String> {
        match source.size.get(index) {
            Some(Extent::Pixels(value)) => Ok(*value),
            Some(Extent::Canvas(value)) if value == "canvas" => Ok(canvas),
            Some(Extent::Canvas(_)) => Err("GRAPH_TEXTURE_SIZE".into()),
            None => Ok(canvas),
        }
    };
    Ok(wgpu::TextureDescriptor {
        label: Some(&source.id),
        size: wgpu::Extent3d {
            width: value(0, width)?,
            height: value(1, height)?,
            depth_or_array_layers: value(2, 1)?,
        },
        mip_level_count: source.mip_level_count,
        sample_count: source.sample_count,
        dimension: parse(&source.dimension, "GRAPH_TEXTURE_DIMENSION")?,
        format: parse(&source.format, "GRAPH_TEXTURE_FORMAT")?,
        usage: texture_usage(&source.usage)?,
        view_formats: &[],
    })
}

fn attachment_format(
    graph: &RenderGraph,
    id: &str,
    surface: wgpu::TextureFormat,
) -> Result<wgpu::TextureFormat, String> {
    if id == "canvas" {
        return Ok(surface);
    }
    graph
        .resources
        .textures
        .iter()
        .find(|texture| texture.id == id)
        .ok_or_else(|| "GRAPH_ATTACHMENT".into())
        .and_then(|texture| parse(&texture.format, "GRAPH_TEXTURE_FORMAT"))
}

fn primitive(value: &Value) -> Result<wgpu::PrimitiveState, String> {
    if value.is_null() {
        return Ok(Default::default());
    }
    serde_json::from_value(value.clone()).map_err(|_| "GRAPH_PRIMITIVE".into())
}

fn depth_stencil(value: &Value) -> Result<Option<wgpu::DepthStencilState>, String> {
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|_| "GRAPH_DEPTH_STENCIL".into())
}

fn multisample(value: &Value) -> Result<wgpu::MultisampleState, String> {
    if value.is_null() {
        return Ok(Default::default());
    }
    let object = value.as_object().ok_or("GRAPH_MULTISAMPLE")?;
    Ok(wgpu::MultisampleState {
        count: object.get("count").and_then(Value::as_u64).unwrap_or(1) as u32,
        mask: object
            .get("mask")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX),
        alpha_to_coverage_enabled: object
            .get("alphaToCoverageEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn sampler_descriptor<'a>(
    label: &'a str,
    value: &Value,
) -> Result<wgpu::SamplerDescriptor<'a>, String> {
    let mut descriptor = wgpu::SamplerDescriptor {
        label: Some(label),
        ..Default::default()
    };
    let Some(object) = value.as_object() else {
        return Ok(descriptor);
    };
    macro_rules! enum_field {
        ($json:literal, $field:ident, $code:literal) => {
            if let Some(value) = object.get($json).and_then(Value::as_str) {
                descriptor.$field = parse(value, $code)?;
            }
        };
    }
    enum_field!("addressModeU", address_mode_u, "GRAPH_SAMPLER");
    enum_field!("addressModeV", address_mode_v, "GRAPH_SAMPLER");
    enum_field!("addressModeW", address_mode_w, "GRAPH_SAMPLER");
    enum_field!("magFilter", mag_filter, "GRAPH_SAMPLER");
    enum_field!("minFilter", min_filter, "GRAPH_SAMPLER");
    enum_field!("mipmapFilter", mipmap_filter, "GRAPH_SAMPLER");
    descriptor.lod_min_clamp = object
        .get("lodMinClamp")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    descriptor.lod_max_clamp = object
        .get("lodMaxClamp")
        .and_then(Value::as_f64)
        .unwrap_or(32.0) as f32;
    descriptor.compare = object
        .get("compare")
        .and_then(Value::as_str)
        .map(|value| parse(value, "GRAPH_SAMPLER"))
        .transpose()?;
    descriptor.anisotropy_clamp = object
        .get("anisotropyClamp")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u16;
    Ok(descriptor)
}

fn parse<T: DeserializeOwned>(value: &str, code: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(value.into())).map_err(|_| code.into())
}
