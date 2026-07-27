use std::{collections::HashMap, num::NonZeroU32};

use crate::render_data::PipelineKey;

use super::DEPTH_FORMAT;

/// Identity of a set of bind-group layouts registered with this library.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct PipelineLayoutKey(u64);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct OwnedVertexBufferLayout {
    pub array_stride: u64,
    pub step_mode: wgpu::VertexStepMode,
    pub attributes: Vec<wgpu::VertexAttribute>,
}

#[derive(Clone, Debug)]
pub struct OwnedProgrammableStage {
    pub shader_source: String,
    pub entry_point: String,
    pub constants: Vec<(String, f64)>,
    pub zero_initialize_workgroup_memory: bool,
}

#[derive(Clone, Debug)]
pub struct RenderPipelineSpec {
    pub layout: Option<PipelineLayoutKey>,
    pub vertex: OwnedProgrammableStage,
    pub vertex_layouts: Vec<OwnedVertexBufferLayout>,
    pub fragment: Option<OwnedProgrammableStage>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub multisample: wgpu::MultisampleState,
    pub targets: Vec<Option<wgpu::ColorTargetState>>,
    pub multiview: Option<NonZeroU32>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct StageKey {
    shader_source: String,
    entry_point: String,
    constants: Vec<(String, u64)>,
    zero_initialize_workgroup_memory: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RenderPipelineKey {
    layout: Option<PipelineLayoutKey>,
    vertex: StageKey,
    vertex_layouts: Vec<OwnedVertexBufferLayout>,
    fragment: Option<StageKey>,
    primitive: wgpu::PrimitiveState,
    depth_stencil: Option<wgpu::DepthStencilState>,
    multisample: wgpu::MultisampleState,
    targets: Vec<Option<wgpu::ColorTargetState>>,
    multiview: Option<NonZeroU32>,
}

impl StageKey {
    fn from_stage(stage: &OwnedProgrammableStage) -> Self {
        let mut constants: Vec<_> = stage
            .constants
            .iter()
            .map(|(name, value)| (name.clone(), value.to_bits()))
            .collect();
        constants.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            shader_source: stage.shader_source.clone(),
            entry_point: stage.entry_point.clone(),
            constants,
            zero_initialize_workgroup_memory: stage.zero_initialize_workgroup_memory,
        }
    }
}

impl RenderPipelineSpec {
    fn key(&self) -> RenderPipelineKey {
        RenderPipelineKey {
            layout: self.layout,
            vertex: StageKey::from_stage(&self.vertex),
            vertex_layouts: self.vertex_layouts.clone(),
            fragment: self.fragment.as_ref().map(StageKey::from_stage),
            primitive: self.primitive,
            depth_stencil: self.depth_stencil.clone(),
            multisample: self.multisample,
            targets: self.targets.clone(),
            multiview: self.multiview,
        }
    }
}

fn target_variant_spec(
    mut spec: RenderPipelineSpec,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    depth_compare: wgpu::CompareFunction,
    depth_write: bool,
) -> RenderPipelineSpec {
    spec.targets = vec![Some(wgpu::ColorTargetState {
        format: color_format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    spec.depth_stencil = depth_format.map(|format| wgpu::DepthStencilState {
        format,
        depth_write_enabled: depth_write,
        depth_compare,
        stencil: Default::default(),
        bias: Default::default(),
    });
    spec
}

pub struct PipelineLibrary {
    pipelines: Vec<wgpu::RenderPipeline>,
    specs: Vec<RenderPipelineSpec>,
    layout_bindings: HashMap<PipelineLayoutKey, Vec<wgpu::BindGroupLayout>>,
    pipeline_layouts: HashMap<PipelineLayoutKey, wgpu::PipelineLayout>,
    default_layout: Option<PipelineLayoutKey>,
    material_layout: Option<PipelineLayoutKey>,
    next_layout: u64,
    pipeline_registry: HashMap<String, (PipelineKey, RenderPipelineKey)>,
    descriptor_cache: HashMap<RenderPipelineKey, PipelineKey>,
}

impl PipelineLibrary {
    pub fn new() -> Self {
        Self {
            pipelines: Vec::new(),
            specs: Vec::new(),
            layout_bindings: HashMap::new(),
            pipeline_layouts: HashMap::new(),
            default_layout: None,
            material_layout: None,
            next_layout: 0,
            pipeline_registry: HashMap::new(),
            descriptor_cache: HashMap::new(),
        }
    }

    /// Registers a new default layout identity. Existing pipelines retain their layout.
    pub fn set_bind_group_layouts(
        &mut self,
        layouts: &[wgpu::BindGroupLayout; 2],
    ) -> PipelineLayoutKey {
        let key = PipelineLayoutKey(self.next_layout);
        self.next_layout = self
            .next_layout
            .checked_add(1)
            .expect("pipeline layout key overflow");
        self.layout_bindings.insert(key, layouts.to_vec());
        self.default_layout = Some(key);
        key
    }

    /// Registers the glTF-only layout, preserving scene groups at 0 and 1.
    pub fn set_material_bind_group_layout(&mut self, layout: &wgpu::BindGroupLayout) {
        let base = self
            .default_layout
            .expect("scene layouts must be registered first");
        let mut layouts = self.layout_bindings[&base].clone();
        layouts.push(layout.clone());
        let key = PipelineLayoutKey(self.next_layout);
        self.next_layout += 1;
        self.layout_bindings.insert(key, layouts);
        self.material_layout = Some(key);
    }

    fn compatibility_spec(
        &self,
        name: &str,
        layouts: &[wgpu::VertexBufferLayout],
        shader: &str,
        format: wgpu::TextureFormat,
    ) -> RenderPipelineSpec {
        let (vertex_entry, fragment_entry) = if name == "triangle_colored" {
            ("v_main", "f_main")
        } else {
            ("vs_main", "fs_main")
        };
        let stage = |entry: &str| OwnedProgrammableStage {
            shader_source: shader.to_owned(),
            entry_point: entry.to_owned(),
            constants: Vec::new(),
            zero_initialize_workgroup_memory: true,
        };
        RenderPipelineSpec {
            layout: if name.starts_with("gltf_") {
                self.material_layout.or(self.default_layout)
            } else {
                self.default_layout
            },
            vertex: stage(vertex_entry),
            vertex_layouts: layouts
                .iter()
                .map(|layout| OwnedVertexBufferLayout {
                    array_stride: layout.array_stride,
                    step_mode: layout.step_mode,
                    attributes: layout.attributes.to_vec(),
                })
                .collect(),
            fragment: Some(stage(fragment_entry)),
            primitive: wgpu::PrimitiveState {
                cull_mode: (name != "gltf_standard_double_sided").then_some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            targets: vec![Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            multiview: None,
        }
    }

    /// Creates or reuses a pipeline solely by its owned descriptor identity.
    pub fn get_or_create_from_spec(
        &mut self,
        device: &wgpu::Device,
        spec: &RenderPipelineSpec,
        label: Option<&str>,
    ) -> PipelineKey {
        let key = spec.key();
        if let Some(pipeline) = self.descriptor_cache.get(&key) {
            return *pipeline;
        }
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label,
            source: wgpu::ShaderSource::Wgsl(spec.vertex.shader_source.as_str().into()),
        });
        let fragment_shader = spec.fragment.as_ref().map(|stage| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label,
                source: wgpu::ShaderSource::Wgsl(stage.shader_source.as_str().into()),
            })
        });
        let vertex_constants: Vec<_> = spec
            .vertex
            .constants
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        let fragment_constants: Option<Vec<_>> = spec.fragment.as_ref().map(|stage| {
            stage
                .constants
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect()
        });
        let vertex_layouts: Vec<_> = spec
            .vertex_layouts
            .iter()
            .map(|layout| wgpu::VertexBufferLayout {
                array_stride: layout.array_stride,
                step_mode: layout.step_mode,
                attributes: &layout.attributes,
            })
            .collect();
        let layout = spec.layout.map(|layout_key| {
            if !self.pipeline_layouts.contains_key(&layout_key) {
                let bindings = self
                    .layout_bindings
                    .get(&layout_key)
                    .unwrap_or_else(|| panic!("unregistered pipeline layout key {layout_key:?}"));
                let refs: Vec<_> = bindings.iter().collect();
                self.pipeline_layouts.insert(
                    layout_key,
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label,
                        bind_group_layouts: &refs,
                        push_constant_ranges: &[],
                    }),
                );
            }
            self.pipeline_layouts.get(&layout_key).unwrap()
        });
        let fragment = spec.fragment.as_ref().map(|stage| wgpu::FragmentState {
            module: fragment_shader.as_ref().unwrap(),
            entry_point: Some(&stage.entry_point),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: fragment_constants.as_ref().unwrap(),
                zero_initialize_workgroup_memory: stage.zero_initialize_workgroup_memory,
            },
            targets: &spec.targets,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label,
            layout,
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: Some(&spec.vertex.entry_point),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &vertex_constants,
                    zero_initialize_workgroup_memory: spec.vertex.zero_initialize_workgroup_memory,
                },
                buffers: &vertex_layouts,
            },
            primitive: spec.primitive,
            depth_stencil: spec.depth_stencil.clone(),
            multisample: spec.multisample,
            fragment,
            multiview: spec.multiview,
            cache: None,
        });
        let pipeline_key = PipelineKey::new(self.pipelines.len() as u32);
        self.pipelines.push(pipeline);
        self.specs.push(spec.clone());
        self.descriptor_cache.insert(key, pipeline_key);
        pipeline_key
    }

    pub fn create_pipeline(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        layouts: &[wgpu::VertexBufferLayout],
        shader: &str,
        format: wgpu::TextureFormat,
    ) -> Result<PipelineKey, String> {
        let spec = self.compatibility_spec(name, layouts, shader, format);
        let descriptor = spec.key();
        if let Some((_, existing)) = self.pipeline_registry.get(name) {
            return Err(if existing == &descriptor {
                format!("Pipeline '{name}' already exists")
            } else {
                format!("Pipeline '{name}' already exists with a different descriptor")
            });
        }
        let key = self.get_or_create_from_spec(device, &spec, Some(name));
        self.pipeline_registry
            .insert(name.to_owned(), (key, descriptor));
        Ok(key)
    }

    pub fn find_pipeline(&self, name: &str) -> Option<PipelineKey> {
        self.pipeline_registry.get(name).map(|v| v.0)
    }
    pub fn get_or_create_pipeline(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        layouts: &[wgpu::VertexBufferLayout],
        shader: &str,
        format: wgpu::TextureFormat,
    ) -> PipelineKey {
        let wanted = self.compatibility_spec(name, layouts, shader, format).key();
        if let Some((key, existing)) = self.pipeline_registry.get(name) {
            assert_eq!(
                existing, &wanted,
                "Pipeline '{name}' requested with a different descriptor"
            );
            return *key;
        }
        self.create_pipeline(device, name, layouts, shader, format)
            .unwrap_or_else(|e| panic!("Failed to create pipeline '{name}': {e}"))
    }
    pub fn get_pipeline(&self, key: PipelineKey) -> &wgpu::RenderPipeline {
        &self.pipelines[key.get() as usize]
    }

    pub fn requires_material(&self, key: PipelineKey) -> bool {
        self.material_layout.is_some()
            && self.specs[key.get() as usize].layout == self.material_layout
    }

    pub fn pipeline_keys(&self) -> impl Iterator<Item = PipelineKey> + '_ {
        let mut keys = self
            .pipeline_registry
            .values()
            .map(|entry| entry.0)
            .collect::<Vec<_>>();
        keys.sort_by_key(|key| key.get());
        keys.dedup();
        keys.into_iter()
    }

    pub fn get_or_create_target_variant(
        &mut self,
        device: &wgpu::Device,
        base: PipelineKey,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        depth_compare: wgpu::CompareFunction,
        depth_write: bool,
    ) -> Result<PipelineKey, String> {
        let spec = self
            .specs
            .get(base.get() as usize)
            .cloned()
            .ok_or_else(|| "unknown base pipeline".to_owned())?;
        let spec =
            target_variant_spec(spec, color_format, depth_format, depth_compare, depth_write);
        Ok(self.get_or_create_from_spec(device, &spec, Some("target variant")))
    }

    pub fn create_target_variant(
        &self,
        device: &wgpu::Device,
        base: PipelineKey,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        depth_compare: wgpu::CompareFunction,
        depth_write: bool,
    ) -> Result<wgpu::RenderPipeline, String> {
        let spec = self
            .specs
            .get(base.get() as usize)
            .cloned()
            .ok_or_else(|| "unknown base pipeline".to_owned())?;
        let spec =
            target_variant_spec(spec, color_format, depth_format, depth_compare, depth_write);
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(" target variant"),
            source: wgpu::ShaderSource::Wgsl(spec.vertex.shader_source.as_str().into()),
        });
        let fragment_shader = spec.fragment.as_ref().map(|stage| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(" target variant"),
                source: wgpu::ShaderSource::Wgsl(stage.shader_source.as_str().into()),
            })
        });
        let vertex_constants: Vec<_> = spec
            .vertex
            .constants
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        let fragment_constants: Option<Vec<_>> = spec.fragment.as_ref().map(|stage| {
            stage
                .constants
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect()
        });
        let vertex_layouts: Vec<_> = spec
            .vertex_layouts
            .iter()
            .map(|layout| wgpu::VertexBufferLayout {
                array_stride: layout.array_stride,
                step_mode: layout.step_mode,
                attributes: &layout.attributes,
            })
            .collect();
        let layout = spec
            .layout
            .map(|key| {
                self.pipeline_layouts
                    .get(&key)
                    .ok_or("pipeline layout missing")
            })
            .transpose()?;
        let fragment = spec.fragment.as_ref().map(|stage| wgpu::FragmentState {
            module: fragment_shader.as_ref().unwrap(),
            entry_point: Some(&stage.entry_point),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: fragment_constants.as_ref().unwrap(),
                zero_initialize_workgroup_memory: stage.zero_initialize_workgroup_memory,
            },
            targets: &spec.targets,
        });
        Ok(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(" target variant"),
                layout,
                vertex: wgpu::VertexState {
                    module: &vertex_shader,
                    entry_point: Some(&spec.vertex.entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &vertex_constants,
                        zero_initialize_workgroup_memory: spec
                            .vertex
                            .zero_initialize_workgroup_memory,
                    },
                    buffers: &vertex_layouts,
                },
                primitive: spec.primitive,
                depth_stencil: spec.depth_stencil,
                multisample: spec.multisample,
                fragment,
                multiview: spec.multiview,
                cache: None,
            }),
        )
    }
}

impl Default for PipelineLibrary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RenderPipelineSpec {
        PipelineLibrary::new().compatibility_spec(
            "x",
            &[],
            "shader",
            wgpu::TextureFormat::Rgba8Unorm,
        )
    }

    #[test]
    fn target_spec_disables_blending_without_mutating_base() {
        let mut base = spec();
        base.primitive.cull_mode = Some(wgpu::Face::Front);
        base.multisample.count = 4;
        base.targets[0].as_mut().unwrap().write_mask = wgpu::ColorWrites::RED;
        let preserved_vertex = StageKey::from_stage(&base.vertex);
        let preserved_fragment = base.fragment.as_ref().map(StageKey::from_stage);
        assert!(base.targets[0].as_ref().unwrap().blend.is_some());
        let variant = target_variant_spec(
            base.clone(),
            wgpu::TextureFormat::Rgba16Float,
            None,
            wgpu::CompareFunction::Always,
            false,
        );
        assert_eq!(variant.targets[0].as_ref().unwrap().blend, None);
        assert_eq!(
            variant.targets[0].as_ref().unwrap().format,
            wgpu::TextureFormat::Rgba16Float
        );
        assert!(base.targets[0].as_ref().unwrap().blend.is_some());
        assert_eq!(variant.primitive, base.primitive);
        assert_eq!(variant.multisample, base.multisample);
        assert_eq!(StageKey::from_stage(&variant.vertex), preserved_vertex);
        assert_eq!(
            variant.fragment.as_ref().map(StageKey::from_stage),
            preserved_fragment
        );
        assert_eq!(
            variant.targets[0].as_ref().unwrap().write_mask,
            wgpu::ColorWrites::ALL
        );
    }

    #[test]
    fn descriptor_identity_covers_optional_and_exact_state() {
        let base = spec().key();
        let mut changed = spec();
        changed.layout = Some(PipelineLayoutKey(1));
        assert_ne!(base, changed.key());
        let first_layout = changed.key();
        changed.layout = Some(PipelineLayoutKey(2));
        assert_ne!(first_layout, changed.key());
        let mut changed = spec();
        changed.fragment = None;
        assert_ne!(base, changed.key());
        let mut changed = spec();
        changed.multiview = NonZeroU32::new(2);
        assert_ne!(base, changed.key());
        let mut changed = spec();
        changed.vertex.constants.push(("x".into(), -0.0));
        assert_ne!(base, changed.key());
        let mut changed2 = changed.clone();
        changed2.vertex.constants[0].1 = 0.0;
        assert_ne!(changed.key(), changed2.key());
        let mut changed = spec();
        changed.vertex.zero_initialize_workgroup_memory = false;
        assert_ne!(base, changed.key());
    }
}
