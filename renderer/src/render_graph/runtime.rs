use std::collections::{BTreeSet, HashSet};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedExtent {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTextureDescriptor {
    pub dimension: wgpu::TextureDimension,
    pub format: wgpu::TextureFormat,
    pub extent: ResolvedExtent,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub usage: wgpu::TextureUsages,
    pub view_formats: Vec<wgpu::TextureFormat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSurfaceContract {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub usage: wgpu::TextureUsages,
    pub present_mode: wgpu::PresentMode,
    pub alpha_mode: wgpu::CompositeAlphaMode,
    pub view_formats: Vec<wgpu::TextureFormat>,
    pub desired_maximum_frame_latency: u32,
}

fn frame_out_surface_request(
    graph: &CompiledGraph,
) -> Result<(SurfaceFormatRequest, u32), GraphError> {
    graph
        .executions
        .iter()
        .find_map(|execution| match execution.parameters {
            NormalizedParameters::FrameOut { surface_format, .. } => {
                Some((surface_format, execution.original_node_index))
            }
            _ => None,
        })
        .ok_or_else(|| {
            error(
                "GRAPH_EXECUTION_UNSUPPORTED",
                "exactly one frame_out is required",
                "executions",
            )
        })
}

fn surface_format_path(original_node_index: u32) -> String {
    format!("nodes[{original_node_index}].parameters.surfaceFormat")
}

pub fn resolve_graph_surface_contract(
    graph: &CompiledGraph,
    capabilities: &wgpu::SurfaceCapabilities,
    width: u32,
    height: u32,
) -> Result<RuntimeSurfaceContract, GraphError> {
    let (request, frame_out_index) = frame_out_surface_request(graph)?;
    resolve_surface_contract(
        request,
        capabilities,
        width,
        height,
        &surface_format_path(frame_out_index),
    )
}

/// Resolves the authored surface request against current adapter/surface capabilities.
/// The presentation policy is intentionally fixed and is not graph-authored.
pub fn resolve_surface_contract(
    request: SurfaceFormatRequest,
    capabilities: &wgpu::SurfaceCapabilities,
    width: u32,
    height: u32,
    authored_path: &str,
) -> Result<RuntimeSurfaceContract, GraphError> {
    if width == 0 || height == 0 {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "surface extent is zero",
            "surface",
        ));
    }
    if !capabilities
        .usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "surface lacks render attachment usage",
            "surface.usage",
        ));
    }
    if !capabilities
        .present_modes
        .contains(&wgpu::PresentMode::Fifo)
    {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "fixed surface present mode is unsupported",
            "surface",
        ));
    }
    if !capabilities
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::Opaque)
    {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "fixed surface alpha mode is unsupported",
            "surface",
        ));
    }
    let requested = match request {
        SurfaceFormatRequest::Preferred => capabilities.formats.iter().copied().find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Rgba16Float
            )
        }),
        SurfaceFormatRequest::Rgba8Unorm => Some(wgpu::TextureFormat::Rgba8Unorm),
        SurfaceFormatRequest::Bgra8Unorm => Some(wgpu::TextureFormat::Bgra8Unorm),
        SurfaceFormatRequest::Rgba16Float => Some(wgpu::TextureFormat::Rgba16Float),
    };
    let format = requested
        .filter(|format| capabilities.formats.contains(format))
        .ok_or_else(|| {
            error(
                "GRAPH_SURFACE_INCOMPATIBLE",
                "requested surface format is unsupported",
                authored_path,
            )
        })?;
    Ok(RuntimeSurfaceContract {
        format,
        width,
        height,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationSlot {
    pub kind: AllocationKind,
    pub descriptor: RuntimeTextureDescriptor,
    pub occupants: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationClass {
    pub key: TextureCompatibilityKey,
    pub slots: Vec<RuntimeAllocationSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeExecution {
    pub execution: u32,
    pub executor: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationPlan {
    pub classes: Vec<RuntimeAllocationClass>,
    pub resource_allocations: Vec<Option<AllocationRef>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimePlan {
    pub allocations: RuntimeAllocationPlan,
    pub executions: Vec<RuntimeExecution>,
    pub render_passes: Vec<PhysicalRenderPass>,
    pub instance_traversal: Option<InstanceTraversalPlan>,
    pub surface: RuntimeSurfaceContract,
}

fn error(code: &'static str, message: impl Into<String>, path: impl Into<String>) -> GraphError {
    GraphError::at(code, message, path)
}

pub const fn texture_dimension(value: TextureDimension) -> wgpu::TextureDimension {
    match value {
        TextureDimension::D1 => wgpu::TextureDimension::D1,
        TextureDimension::D2 => wgpu::TextureDimension::D2,
        TextureDimension::D3 => wgpu::TextureDimension::D3,
    }
}

pub const fn texture_format(value: TextureFormat) -> wgpu::TextureFormat {
    match value {
        TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        TextureFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
        TextureFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        TextureFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
        TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
    }
}

pub const fn texture_usage(value: TextureUsage) -> wgpu::TextureUsages {
    match value {
        TextureUsage::Sampled => wgpu::TextureUsages::TEXTURE_BINDING,
        TextureUsage::Storage => wgpu::TextureUsages::STORAGE_BINDING,
        TextureUsage::CopySrc => wgpu::TextureUsages::COPY_SRC,
        TextureUsage::CopyDst => wgpu::TextureUsages::COPY_DST,
        TextureUsage::ColorAttachment | TextureUsage::DepthAttachment => {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        }
    }
}

pub fn texture_usages(values: &[TextureUsage]) -> wgpu::TextureUsages {
    values
        .iter()
        .fold(wgpu::TextureUsages::empty(), |usage, value| {
            usage | texture_usage(*value)
        })
}

fn scaled(value: u32, ratio: Ratio, path: &str) -> Result<u32, GraphError> {
    if ratio.denominator == 0 {
        return Err(error(
            "GRAPH_RESOURCE_LIMIT",
            "zero extent denominator",
            path,
        ));
    }
    let product = u64::from(value)
        .checked_mul(u64::from(ratio.numerator))
        .ok_or_else(|| error("GRAPH_RESOURCE_LIMIT", "extent arithmetic overflow", path))?;
    let result = product
        .checked_add(u64::from(ratio.denominator) - 1)
        .ok_or_else(|| error("GRAPH_RESOURCE_LIMIT", "extent arithmetic overflow", path))?
        / u64::from(ratio.denominator);
    u32::try_from(result.max(1))
        .map_err(|_| error("GRAPH_RESOURCE_LIMIT", "extent exceeds u32", path))
}

pub fn resolve_extent(
    extent: &NormalizedTextureExtent,
    surface: [u32; 2],
) -> Result<ResolvedExtent, GraphError> {
    let resolved = match extent {
        NormalizedTextureExtent::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => ResolvedExtent {
            width: *width,
            height: *height,
            depth_or_array_layers: *depth_or_array_layers,
        },
        NormalizedTextureExtent::SurfaceRelative {
            width,
            height,
            depth_or_array_layers,
        } => ResolvedExtent {
            width: scaled(surface[0], *width, "extent.width")?,
            height: scaled(surface[1], *height, "extent.height")?,
            depth_or_array_layers: *depth_or_array_layers,
        },
    };
    if resolved.width == 0 || resolved.height == 0 || resolved.depth_or_array_layers == 0 {
        return Err(error(
            "GRAPH_RESOURCE_LIMIT",
            "texture extent is zero",
            "extent",
        ));
    }
    Ok(resolved)
}

pub fn resolved_mip_level_count(extent: ResolvedExtent) -> u32 {
    32 - extent
        .width
        .max(extent.height)
        .max(extent.depth_or_array_layers)
        .leading_zeros()
}

fn validate_limits(
    dimension: TextureDimension,
    extent: ResolvedExtent,
    mip_count: u32,
    limits: Option<&wgpu::Limits>,
    path: &str,
) -> Result<(), GraphError> {
    let max_mips = resolved_mip_level_count(extent);
    if mip_count == 0 || mip_count > max_mips {
        return Err(error(
            "GRAPH_RESOURCE_LIMIT",
            "invalid mip level count",
            path,
        ));
    }
    if let Some(l) = limits {
        let valid = match dimension {
            TextureDimension::D1 => extent.width <= l.max_texture_dimension_1d,
            TextureDimension::D2 => {
                extent.width <= l.max_texture_dimension_2d
                    && extent.height <= l.max_texture_dimension_2d
                    && extent.depth_or_array_layers <= l.max_texture_array_layers
            }
            TextureDimension::D3 => {
                extent.width <= l.max_texture_dimension_3d
                    && extent.height <= l.max_texture_dimension_3d
                    && extent.depth_or_array_layers <= l.max_texture_dimension_3d
            }
        };
        if !valid {
            return Err(error(
                "GRAPH_RESOURCE_LIMIT",
                "texture exceeds device limits",
                path,
            ));
        }
    }
    Ok(())
}

pub fn runtime_texture_descriptor(
    key: &TextureCompatibilityKey,
    usage: &[TextureUsage],
    surface: [u32; 2],
    limits: Option<&wgpu::Limits>,
) -> Result<RuntimeTextureDescriptor, GraphError> {
    let extent = resolve_extent(&key.extent, surface)?;
    validate_limits(
        key.dimension,
        extent,
        key.mip_level_count,
        limits,
        "allocationClasses.key",
    )?;
    Ok(RuntimeTextureDescriptor {
        dimension: texture_dimension(key.dimension),
        format: texture_format(key.format),
        extent,
        mip_level_count: key.mip_level_count,
        sample_count: key.sample_count,
        usage: texture_usages(usage),
        view_formats: key
            .view_formats
            .iter()
            .copied()
            .map(texture_format)
            .collect(),
    })
}

fn invalid(message: impl Into<String>, path: impl Into<String>) -> GraphError {
    error("GRAPH_RUNTIME_PLAN_INVALID", message, path)
}

fn execution_supported(key: &str) -> bool {
    contract(key).is_some_and(|contract| {
        contract.fullscreen_policy.is_some() || contract.is_raster_draw() || key == "frame_out"
    })
}

fn resource_is_mesh(graph: &CompiledGraph, id: u32) -> bool {
    graph.resources.get(id as usize).is_some_and(|resource| {
        resource.semantic_type == SemanticType::MeshData
            && matches!(resource.plan, ResourcePlan::MeshData)
    })
}

fn texture_descriptor<'a>(
    graph: &'a CompiledGraph,
    id: u32,
) -> Option<&'a NormalizedTextureDescriptor> {
    let family = match graph.resources.get(id as usize)?.plan {
        ResourcePlan::Texture { family, .. } | ResourcePlan::TextureSource { family, .. } => family,
        _ => return None,
    };
    match &graph.texture_families.get(family as usize)?.source {
        TextureFamilySource::AuthoredTexture { descriptor, .. }
        | TextureFamilySource::CompilerDefaultInput { descriptor, .. }
        | TextureFamilySource::CompilerColorResolve { descriptor, .. } => Some(descriptor),
    }
}

fn validate_fullscreen_execution(
    graph: &CompiledGraph,
    producers: &[Option<u32>],
    i: usize,
    execution: &CompiledExecution,
    contract: &Contract,
) -> Result<(), GraphError> {
    let key = execution.executor.key.as_str();
    let Some(policy) = contract.fullscreen_policy else {
        return Ok(());
    };
    let path = |field| format!("executions[{i}].{field}");
    let scalar = |v: &f32, min, max| v.is_finite() && (min..=max).contains(v);
    let vector = |v: &[f32], min, max| v.iter().all(|x| scalar(x, min, max));
    let valid_parameters = match (key, &execution.parameters) {
        ("fullscreen_copy", NormalizedParameters::FullscreenCopy) => true,
        (
            "color_balance",
            NormalizedParameters::ColorBalance {
                factor,
                lift,
                lift_color,
                gamma,
                gamma_color,
                gain,
                gain_color,
                offset,
                offset_color,
                power,
                power_color,
                slope,
                slope_color,
                ..
            },
        ) => {
            scalar(factor, 0.0, 1.0)
                && scalar(lift, -1.0, 1.0)
                && vector(lift_color, 0.0, 4.0)
                && scalar(gamma, 0.01, 4.0)
                && vector(gamma_color, 0.0, 4.0)
                && scalar(gain, 0.0, 4.0)
                && vector(gain_color, 0.0, 4.0)
                && scalar(offset, -1.0, 1.0)
                && vector(offset_color, 0.0, 2.0)
                && scalar(power, 0.01, 4.0)
                && vector(power_color, 0.0, 4.0)
                && scalar(slope, 0.0, 4.0)
                && vector(slope_color, 0.0, 4.0)
        }
        (
            "exposure_contrast",
            NormalizedParameters::ExposureContrast {
                exposure_stops,
                contrast,
                pivot,
                factor,
            },
        ) => {
            scalar(exposure_stops, -10.0, 10.0)
                && scalar(contrast, 0.01, 4.0)
                && scalar(pivot, 0.001, 4.0)
                && scalar(factor, 0.0, 1.0)
        }
        ("saturation", NormalizedParameters::Saturation { saturation, factor }) => {
            scalar(saturation, 0.0, 4.0) && scalar(factor, 0.0, 1.0)
        }
        (
            "channel_mixer",
            NormalizedParameters::ChannelMixer {
                red_output,
                green_output,
                blue_output,
                factor,
            },
        ) => {
            vector(red_output, -2.0, 2.0)
                && vector(green_output, -2.0, 2.0)
                && vector(blue_output, -2.0, 2.0)
                && scalar(factor, 0.0, 1.0)
        }
        ("bloom_extract", NormalizedParameters::BloomExtract { threshold, knee }) => {
            threshold.is_finite()
                && (0.0..=64.0).contains(threshold)
                && knee.is_finite()
                && (0.0..=1.0).contains(knee)
        }
        ("bloom_blur", NormalizedParameters::BloomBlur { direction, radius }) => {
            direction.iter().all(|v| v.is_finite())
                && (direction[0].abs() + direction[1].abs() - 1.0).abs() <= 0.0001
                && direction.iter().all(|v| (-1.0..=1.0).contains(v))
                && radius.is_finite()
                && (1.0..=16.0).contains(radius)
        }
        ("bloom_composite", NormalizedParameters::BloomComposite { intensity }) => {
            intensity.is_finite() && (0.0..=16.0).contains(intensity)
        }
        ("luminance_edge", NormalizedParameters::LuminanceEdge { strength }) => {
            strength.is_finite() && (0.0..=16.0).contains(strength)
        }
        _ => false,
    };
    if !valid_parameters {
        return Err(invalid(
            "fullscreen parameters mismatch",
            path("parameters"),
        ));
    }
    let expected_inputs: Vec<_> = contract.inputs.iter().map(|input| input.name).collect();
    if execution.inputs.len() != expected_inputs.len()
        || execution
            .inputs
            .iter()
            .zip(&expected_inputs)
            .any(|(v, s)| v.socket != *s)
    {
        return Err(invalid("fullscreen inputs mismatch", path("inputs")));
    }
    let [output] = execution.outputs.as_slice() else {
        return Err(invalid("fullscreen outputs mismatch", path("outputs")));
    };
    if output.socket != "color" {
        return Err(invalid("fullscreen outputs mismatch", path("outputs")));
    }
    if !matches!(execution.kind, ExecutionKind::Fullscreen) {
        return Err(invalid("fullscreen render kind mismatch", path("kind")));
    }
    let (color_attachments, depth_stencil) = super::compiler::execution_attachments(execution);
    if depth_stencil.is_some() {
        return Err(invalid("fullscreen depth mismatch", path("kind")));
    }
    let [attachment] = color_attachments.as_slice() else {
        return Err(invalid("fullscreen attachment mismatch", path("kind")));
    };
    let clear = NormalizedColorLoad::Clear { value: [0.0; 4] };
    if attachment.resource != output.resource
        || attachment.location != 0
        || attachment.load != clear
        || attachment.store != StoreOp::Store
    {
        return Err(invalid("fullscreen attachment mismatch", path("kind")));
    }
    let sampled_inputs: Vec<_> = contract
        .inputs
        .iter()
        .filter(|input| input.role == InputRole::SampledTexture)
        .collect();
    let sampled_count = sampled_inputs.len();
    if contract.inputs[..sampled_count]
        .iter()
        .any(|input| input.role != InputRole::SampledTexture)
    {
        return Err(invalid("fullscreen inputs mismatch", path("inputs")));
    }
    if execution.accesses.len() != sampled_count + 1
        || execution.inputs[..sampled_count]
            .iter()
            .zip(&execution.accesses)
            .any(|(input, access)| {
                access.socket != input.socket
                    || access.resource != input.resource
                    || !matches!(access.mode, AccessMode::SampledTexture)
            })
        || !matches!(&execution.accesses[sampled_count], CompiledAccess { socket, resource, mode: AccessMode::ColorAttachment { location: 0, load, store: StoreOp::Store, full_overwrite: true } } if socket == "color" && *resource == output.resource && *load == clear)
    {
        return Err(invalid("fullscreen accesses mismatch", path("accesses")));
    }
    let target = execution.inputs[sampled_count].resource;
    if !matches!(graph.resources.get(output.resource as usize), Some(CompiledResource { semantic_type: SemanticType::Texture, plan: ResourcePlan::Texture { target: t, initialized: true, stored: true, allocation: Some(_), .. }, .. }) if *t == target)
    {
        return Err(invalid(
            "fullscreen output transition mismatch",
            format!("resources[{}].plan", output.resource),
        ));
    }
    for input in &execution.inputs[..sampled_count] {
        let Some(CompiledResource {
            semantic_type: SemanticType::Texture,
            plan:
                ResourcePlan::Texture {
                    family,
                    version,
                    initialized: true,
                    stored: true,
                    allocation: Some(_),
                    ..
                },
            ..
        }) = graph.resources.get(input.resource as usize)
        else {
            return Err(invalid(
                "fullscreen sampled texture is invalid",
                format!("resources[{}].plan", input.resource),
            ));
        };
        let output_family = match graph
            .resources
            .get(output.resource as usize)
            .map(|r| &r.plan)
        {
            Some(ResourcePlan::Texture { family, .. }) => *family,
            _ => unreachable!("output transition was validated above"),
        };
        if *family == output_family {
            return Err(invalid(
                "fullscreen samples its output family",
                path("inputs"),
            ));
        }
        let producer = producers.get(input.resource as usize).copied().flatten();
        if !producer.is_some_and(|producer| producer < i as u32) {
            return Err(invalid(
                "fullscreen sampled producer must precede execution",
                path("inputs"),
            ));
        }
        let stale = graph.resources.iter().any(|resource| {
            matches!(resource.plan, ResourcePlan::Texture { family: f, version: v, .. } if f == *family && v > *version)
                && graph.executions[..=i].iter().any(|candidate| {
                    candidate.outputs.iter().any(|value| {
                        std::ptr::eq(resource, &graph.resources[value.resource as usize])
                    })
                })
        });
        if stale {
            return Err(invalid(
                "fullscreen sampled texture version is stale",
                path("inputs"),
            ));
        }
    }
    let source = texture_descriptor(graph, execution.inputs[0].resource)
        .ok_or_else(|| invalid("fullscreen source descriptor missing", path("inputs")))?;
    let target_d = texture_descriptor(graph, output.resource)
        .ok_or_else(|| invalid("fullscreen target descriptor missing", path("inputs")))?;
    let hdr = |d: &NormalizedTextureDescriptor| {
        super::compiler::is_single_view_d2(d) && d.format == TextureFormat::Rgba16Float
    };
    let descriptors_valid = hdr(source)
        && match policy {
            FullscreenPolicy::Copy => {
                super::compiler::is_single_view_d2(target_d)
                    && target_d.format != TextureFormat::Depth32Float
                    && target_d.extent == source.extent
            }
            FullscreenPolicy::BloomExtract => hdr(target_d),
            FullscreenPolicy::HdrSameExtent => hdr(target_d) && target_d.extent == source.extent,
            FullscreenPolicy::BloomComposite => {
                hdr(target_d)
                    && target_d.extent == source.extent
                    && texture_descriptor(graph, execution.inputs[1].resource).is_some_and(hdr)
            }
        };
    if !descriptors_valid {
        return Err(invalid(
            "fullscreen texture descriptors mismatch",
            path("inputs"),
        ));
    }
    Ok(())
}

fn validate_pipeline_resolve(
    graph: &CompiledGraph,
    producers: &[Option<u32>],
    family_index: usize,
    family: &TextureFamily,
) -> Result<(), GraphError> {
    let TextureFamilySource::CompilerColorResolve {
        resource: root,
        descriptor,
        producer_node_index,
        output_ordinal,
        source_resource,
    } = &family.source
    else {
        return Ok(());
    };
    let path = || format!("textureFamilies[{family_index}].source");
    let source = graph
        .resources
        .get(*source_resource as usize)
        .ok_or_else(|| invalid("resolve source is out of bounds", path()))?;
    let producer_index = producers
        .get(*source_resource as usize)
        .copied()
        .flatten()
        .ok_or_else(|| invalid("resolve source producer is missing", path()))?
        as usize;
    let producer = graph
        .executions
        .get(producer_index)
        .ok_or_else(|| invalid("resolve producer is out of bounds", path()))?;
    let source_family_id = match source.plan {
        ResourcePlan::Texture { family, .. } => family,
        _ => return Err(invalid("resolve source is not a texture version", path())),
    };
    let source_family = graph
        .texture_families
        .get(source_family_id as usize)
        .ok_or_else(|| invalid("resolve source family is out of bounds", path()))?;
    let source_descriptor = super::compiler::family_descriptor(source_family);
    let expected_descriptor = NormalizedTextureDescriptor {
        sample_count: 1,
        ..source_descriptor.clone()
    };
    let origin_matches = |origin: &ResourceOrigin| {
        matches!(origin,
        ResourceOrigin::CompilerColorResolve {
            producer_node_index: node,
            output_ordinal: output,
            source_resource: source,
        } if node == producer_node_index && output == output_ordinal && source == source_resource)
    };
    let [version] = family.versions.as_slice() else {
        return Err(invalid("resolve family must have one version", path()));
    };
    let output = graph
        .resources
        .get(version.resource as usize)
        .ok_or_else(|| invalid("resolve output is out of bounds", path()))?;
    let exact_output = producer
        .outputs
        .get(*output_ordinal as usize)
        .is_some_and(|value| value.socket == "color" && value.resource == *source_resource)
        && *output_ordinal == 0
        && producer
            .outputs
            .iter()
            .filter(|value| value.resource == *source_resource)
            .count()
            == 1;
    let exact_access = producer
        .accesses
        .iter()
        .filter(|access| {
            matches!(access.mode,
        AccessMode::ColorResolve { source, location: 0 } if source == *source_resource)
                && access.resource == version.resource
                && access.socket == "colorResolve"
        })
        .count()
        == 1
        && producer
            .accesses
            .iter()
            .filter(|access| matches!(access.mode, AccessMode::ColorResolve { .. }))
            .count()
            == 1;
    if !contract(&producer.executor.key).is_some_and(Contract::is_raster_draw)
        || producer.original_node_index != *producer_node_index
        || !exact_output
        || !matches!(&source.origin,
            ResourceOrigin::AuthoredOutput { node, socket, output_ordinal: 0 }
                if node == &producer.id && socket == "color")
        || source.original_node_index != *producer_node_index
        || !matches!(graph.resources.get(*root as usize), Some(CompiledResource { plan: ResourcePlan::TextureSource { .. }, origin, .. }) if origin_matches(origin))
        || !origin_matches(&output.origin)
        || version.version != 0
        || version.target != *root
        || output.producer_execution != Some(producer_index as u32)
        || !exact_access
        || family.id == source_family_id
        || family.allocation == source_family.allocation
        || descriptor != &expected_descriptor
    {
        return Err(invalid("pipeline color resolve is not canonical", path()));
    }
    Ok(())
}

fn validate_canonical_plan(graph: &CompiledGraph) -> Result<(), GraphError> {
    fn texture_family_for_resource<'a>(
        graph: &'a CompiledGraph,
        resource: u32,
    ) -> Option<&'a TextureFamily> {
        let resource = graph.resources.get(resource as usize)?;
        let family = match resource.plan {
            ResourcePlan::TextureSource { family, .. } | ResourcePlan::Texture { family, .. } => {
                family
            }
            _ => return None,
        };
        graph.texture_families.get(family as usize)
    }
    for (i, resource) in graph.resources.iter().enumerate() {
        if resource.semantic_type.is_virtual() {
            return Err(invalid(
                "virtual semantic type was materialized",
                format!("resources[{i}].semanticType"),
            ));
        }
    }
    for (i, execution) in graph.executions.iter().enumerate() {
        if !execution_supported(&execution.executor.key) {
            return Err(error(
                "GRAPH_EXECUTION_UNSUPPORTED",
                "unsupported execution",
                format!("executions[{i}]"),
            ));
        }
    }
    if graph.schema_version != 3 {
        return Err(invalid(
            "compiled graph schema version must be 3",
            "schemaVersion",
        ));
    }
    let mut expected_execution = 0usize;
    for (pass_index, pass) in graph.render_passes.iter().enumerate() {
        if pass.executions.is_empty() {
            return Err(invalid(
                "physical pass is empty",
                format!("renderPasses[{pass_index}]"),
            ));
        }
        for &member in &pass.executions {
            if member as usize != expected_execution || expected_execution >= graph.executions.len()
            {
                return Err(invalid(
                    "physical passes must partition logical executions in order",
                    format!("renderPasses[{pass_index}].executions"),
                ));
            }
            expected_execution += 1;
        }
        let singleton = pass.executions.len() == 1;
        let kinds_valid = match &pass.kind {
            PhysicalRenderPassKind::Surface => {
                singleton
                    && matches!(
                        graph.executions[pass.executions[0] as usize].kind,
                        ExecutionKind::FrameOut { .. }
                    )
            }
            PhysicalRenderPassKind::Texture {
                color_attachments,
                depth_stencil,
            } => {
                pass.executions.iter().all(|&member| {
                    !matches!(
                        graph.executions[member as usize].kind,
                        ExecutionKind::FrameOut { .. }
                    )
                }) && (singleton
                    || pass.executions.iter().all(|&member| {
                        matches!(
                            graph.executions[member as usize].kind,
                            ExecutionKind::RasterDraw
                        )
                    }))
                    && color_attachments.iter().all(|attachment| {
                        (attachment.resource as usize) < graph.resources.len()
                            && attachment
                                .resolve_target
                                .is_none_or(|resource| (resource as usize) < graph.resources.len())
                    })
                    && depth_stencil.as_ref().is_none_or(|attachment| {
                        (attachment.resource as usize) < graph.resources.len()
                    })
            }
        };
        if !kinds_valid {
            return Err(invalid(
                "physical pass kind or attachment is invalid",
                format!("renderPasses[{pass_index}]"),
            ));
        }
    }
    if expected_execution != graph.executions.len() {
        return Err(invalid(
            "physical passes omit logical executions",
            "renderPasses",
        ));
    }
    for (resource_index, resource) in graph.resources.iter().enumerate() {
        if let ResourcePlan::Texture { family, .. } | ResourcePlan::TextureSource { family, .. } =
            resource.plan
        {
            if family as usize >= graph.texture_families.len() {
                return Err(invalid(
                    "texture family is out of bounds",
                    format!("resources[{resource_index}].plan.family"),
                ));
            }
        }
    }
    let canonical_passes = super::compiler::build_render_passes(
        &graph.executions,
        &graph.resources,
        &graph.texture_families,
    );
    if graph.render_passes != canonical_passes {
        return Err(invalid(
            "physical render pass plan is not canonical",
            "renderPasses",
        ));
    }
    for (pass_index, pass) in graph.render_passes.iter().enumerate() {
        if !matches!(pass.kind, PhysicalRenderPassKind::Texture { .. }) {
            continue;
        }
        let mut previous = None;
        for &member in &pass.executions {
            let execution = &graph.executions[member as usize];
            let NormalizedParameters::Raster { draw_order, .. } = &execution.parameters else {
                previous = None;
                continue;
            };
            let key = (*draw_order, execution.original_node_index);
            if previous.is_some_and(|value| value > key) {
                return Err(invalid(
                    "raster pass members are not in canonical draw order",
                    format!("renderPasses[{pass_index}].executions"),
                ));
            }
            previous = Some(key);
        }
    }
    let execution_pass: Vec<u32> = graph
        .render_passes
        .iter()
        .enumerate()
        .flat_map(|(pass, value)| value.executions.iter().map(move |_| pass as u32))
        .collect();

    let mut producers = vec![None; graph.resources.len()];
    let mut uses = vec![BTreeSet::new(); graph.resources.len()];
    fn claim_producer(
        producers: &mut [Option<u32>],
        resource: u32,
        execution: u32,
        path: String,
    ) -> Result<(), GraphError> {
        let producer = producers
            .get_mut(resource as usize)
            .ok_or_else(|| invalid("execution producer is out of bounds", path.clone()))?;
        if producer.replace(execution).is_some() {
            return Err(invalid("resource has duplicate producers", path));
        }
        Ok(())
    }
    for (i, execution) in graph.executions.iter().enumerate() {
        let contract = contract(&execution.executor.key).expect("supported executor has contract");
        if execution.executor.version != contract.version {
            return Err(invalid(
                "executor version does not match its contract",
                format!("executions[{i}].executor.version"),
            ));
        }
        for output in &execution.outputs {
            claim_producer(
                &mut producers,
                output.resource,
                i as u32,
                format!("executions[{i}].outputs"),
            )?;
        }
        let mut referenced = HashSet::new();
        for input in &execution.inputs {
            if input.resource as usize >= graph.resources.len() {
                return Err(invalid(
                    "execution input is out of bounds",
                    format!("executions[{i}].inputs"),
                ));
            }
            referenced.insert(input.resource);
        }
        referenced.extend(execution.outputs.iter().map(|v| v.resource));
        for access in &execution.accesses {
            if access.resource as usize >= graph.resources.len() {
                return Err(invalid(
                    "execution access is out of bounds",
                    format!("executions[{i}].accesses"),
                ));
            }
            referenced.insert(access.resource);
            if matches!(access.mode, AccessMode::ColorResolve { .. }) {
                claim_producer(
                    &mut producers,
                    access.resource,
                    i as u32,
                    format!("executions[{i}].accesses"),
                )?;
            }
        }
        validate_fullscreen_execution(graph, &producers, i, execution, contract)?;
        for resource in referenced {
            uses.get_mut(resource as usize)
                .ok_or_else(|| {
                    invalid(
                        "execution resource is out of bounds",
                        format!("executions[{i}]"),
                    )
                })?
                .insert(execution_pass[i]);
        }
    }
    for (ri, resource) in graph.resources.iter().enumerate() {
        if resource.producer_execution != producers[ri] {
            return Err(invalid(
                "resource producer metadata is not canonical",
                format!("resources[{ri}].producerExecution"),
            ));
        }
        let lifetime = uses[ri].iter().next().zip(uses[ri].iter().next_back()).map(
            |(&first_use, &last_use)| Lifetime {
                first_use,
                last_use,
            },
        );
        if resource.lifetime != lifetime {
            return Err(invalid(
                "resource lifetime metadata is not canonical",
                format!("resources[{ri}].lifetime"),
            ));
        }
    }
    for (i, execution) in graph.executions.iter().enumerate() {
        for input in &execution.inputs {
            if let Some(producer) = producers[input.resource as usize] {
                if producer >= i as u32 {
                    return Err(invalid(
                        "input producer must precede consumer",
                        format!("executions[{i}].inputs"),
                    ));
                }
                let consumer_contract =
                    contract(&execution.executor.key).expect("supported executor has contract");
                let is_attachment = consumer_contract
                    .inputs
                    .iter()
                    .find(|candidate| candidate.name == input.socket)
                    .is_some_and(|candidate| {
                        matches!(
                            candidate.role,
                            InputRole::ColorTarget { .. } | InputRole::DepthTarget
                        )
                    });
                let predecessor = &graph.executions[producer as usize];
                if is_attachment
                    && matches!(execution.kind, ExecutionKind::RasterDraw)
                    && matches!(predecessor.kind, ExecutionKind::RasterDraw)
                {
                    let NormalizedParameters::Raster {
                        draw_order: predecessor_order,
                        ..
                    } = predecessor.parameters
                    else {
                        return Err(invalid(
                            "raster predecessor parameters mismatch",
                            format!("executions[{producer}].parameters"),
                        ));
                    };
                    let NormalizedParameters::Raster {
                        draw_order: consumer_order,
                        ..
                    } = execution.parameters
                    else {
                        return Err(invalid(
                            "raster execution parameters mismatch",
                            format!("executions[{i}].parameters"),
                        ));
                    };
                    if (predecessor_order, predecessor.original_node_index)
                        > (consumer_order, execution.original_node_index)
                    {
                        return Err(invalid(
                            "raster attachment predecessors must have canonical draw order",
                            format!("executions[{i}].parameters.drawOrder"),
                        ));
                    }
                }
            }
        }
    }

    let mut family_keys = HashSet::new();
    let mut source_claims = vec![0u8; graph.resources.len()];
    for (fi, family) in graph.texture_families.iter().enumerate() {
        validate_pipeline_resolve(graph, &producers, fi, family)?;
        if family.id as usize != fi || !family_keys.insert(family.key.clone()) {
            return Err(invalid(
                "texture family id/key is not unique and canonical",
                format!("textureFamilies[{fi}]"),
            ));
        }
        let (source, residency, descriptor) = match &family.source {
            TextureFamilySource::AuthoredTexture {
                resource,
                residency,
                descriptor,
            } => (*resource, *residency, descriptor),
            TextureFamilySource::CompilerDefaultInput {
                resource,
                descriptor,
                ..
            } => (*resource, TextureResidency::Transient, descriptor),
            TextureFamilySource::CompilerColorResolve {
                resource,
                descriptor,
                ..
            } => (*resource, TextureResidency::Transient, descriptor),
        };
        let source_resource = graph.resources.get(source as usize).ok_or_else(|| {
            invalid(
                "texture source resource is out of bounds",
                format!("textureFamilies[{fi}].source.resource"),
            )
        })?;
        source_claims[source as usize] = source_claims[source as usize].saturating_add(1);
        if source_resource.semantic_type != SemanticType::Texture
            || source_resource.producer_execution.is_some()
            || !matches!(&source_resource.plan, ResourcePlan::TextureSource { family: f, residency: r, descriptor: d }
                if *f == family.id && *r == residency && d == descriptor)
        {
            return Err(invalid(
                "texture family source is not canonical",
                format!("textureFamilies[{fi}].source"),
            ));
        }
        let origin_ok = match (&family.source, &source_resource.origin) {
            (
                TextureFamilySource::AuthoredTexture { resource, .. },
                ResourceOrigin::AuthoredOutput {
                    node: _,
                    socket,
                    output_ordinal,
                },
            ) => {
                family.key.source_node == source_resource.original_node_index
                    && family.key.source_socket == 0
                    && *output_ordinal == 0
                    && *socket == "texture"
                    && *resource == source
                    && source_resource.producer_execution.is_none()
            }
            (
                TextureFamilySource::CompilerDefaultInput {
                    owner_node_index,
                    input_ordinal,
                    role,
                    ..
                },
                ResourceOrigin::CompilerDefaultInput {
                    owner_node_index: owner,
                    input_ordinal: input,
                    socket,
                    role: origin_role,
                },
            ) => {
                let owner_executions: Vec<_> = graph
                    .executions
                    .iter()
                    .filter(|execution| execution.original_node_index == *owner_node_index)
                    .collect();
                let owner_input_ok = owner_executions
                    .as_slice()
                    .first()
                    .is_some_and(|execution| {
                        execution
                            .inputs
                            .iter()
                            .filter(|input| input.socket == *socket && input.resource == source)
                            .count()
                            == 1
                    })
                    && contract(&owner_executions[0].executor.key)
                        .is_some_and(Contract::is_raster_draw)
                    && owner_executions[0].executor.version == 1
                    && graph
                        .executions
                        .iter()
                        .flat_map(|execution| &execution.inputs)
                        .filter(|input| input.resource == source)
                        .count()
                        == 1
                    && contract(&owner_executions[0].executor.key)
                        .and_then(|contract| contract.inputs.get(*input_ordinal as usize))
                        .is_some_and(|input| {
                            input.name == *socket
                                && matches!(
                                    (input.role, role),
                                    (
                                        InputRole::ColorTarget { location: 0 },
                                        CompilerTextureRole::ColorTarget
                                    ) | (InputRole::DepthTarget, CompilerTextureRole::DepthTarget)
                                )
                        });
                owner == owner_node_index
                    && owner_executions.len() == 1
                    && input == input_ordinal
                    && origin_role == role
                    && owner_input_ok
                    && socket
                        == if *role == CompilerTextureRole::ColorTarget {
                            "colorTarget"
                        } else {
                            "depthTarget"
                        }
            }
            (
                TextureFamilySource::CompilerColorResolve {
                    producer_node_index,
                    output_ordinal,
                    source_resource: resolve_source,
                    ..
                },
                ResourceOrigin::CompilerColorResolve {
                    producer_node_index: origin_node,
                    output_ordinal: origin_output,
                    source_resource: origin_source,
                },
            ) => {
                *producer_node_index == *origin_node
                    && *output_ordinal == *origin_output
                    && *resolve_source == *origin_source
                    && source_resource.semantic_type == SemanticType::Texture
                    && family.key.source_node == *producer_node_index
                    && family.key.source_socket == *output_ordinal
            }
            _ => false,
        };
        if !origin_ok {
            return Err(invalid(
                "texture source origin is not canonical",
                format!("resources[{source}].origin"),
            ));
        }
        if let TextureFamilySource::CompilerDefaultInput {
            owner_node_index,
            input_ordinal,
            role,
            descriptor,
            ..
        } = &family.source
        {
            let fixed = descriptor.dimension == TextureDimension::D2
                && descriptor.format
                    == if *role == CompilerTextureRole::ColorTarget {
                        TextureFormat::Rgba16Float
                    } else {
                        TextureFormat::Depth32Float
                    }
                && descriptor.mip_level_count == 1
                && matches!(descriptor.sample_count, 1 | 4)
                && descriptor.view_formats.is_empty()
                && matches!(
                    descriptor.extent,
                    NormalizedTextureExtent::Absolute {
                        depth_or_array_layers: 1,
                        ..
                    } | NormalizedTextureExtent::SurfaceRelative {
                        depth_or_array_layers: 1,
                        ..
                    }
                )
                && family.key.source_node == *owner_node_index
                && family.key.source_socket == *input_ordinal
                && source_resource.original_node_index == *owner_node_index;
            if !fixed {
                return Err(invalid(
                    "compiler default descriptor is not canonical",
                    format!("textureFamilies[{fi}].source.descriptor"),
                ));
            }
            let owner = graph
                .executions
                .iter()
                .find(|execution| execution.original_node_index == *owner_node_index)
                .ok_or_else(|| {
                    invalid(
                        "compiler default owner is missing",
                        format!("textureFamilies[{fi}].source"),
                    )
                })?;
            let opposite_socket = if *role == CompilerTextureRole::ColorTarget {
                "depthTarget"
            } else {
                "colorTarget"
            };
            let opposite_resource = owner
                .inputs
                .iter()
                .find(|input| input.socket == opposite_socket)
                .map(|input| input.resource)
                .ok_or_else(|| {
                    invalid(
                        "compiler default opposite input is missing",
                        format!("executions[{owner_node_index}].inputs"),
                    )
                })?;
            let opposite_family = texture_family_for_resource(graph, opposite_resource)
                .ok_or_else(|| {
                    invalid(
                        "compiler default opposite input is invalid",
                        format!("executions[{owner_node_index}].inputs"),
                    )
                })?;
            let both_defaults = matches!(&opposite_family.source, TextureFamilySource::CompilerDefaultInput { owner_node_index: opposite_owner, .. } if opposite_owner == owner_node_index);
            let (expected_extent, expected_sample_count) = if both_defaults {
                (
                    NormalizedTextureExtent::SurfaceRelative {
                        width: Ratio {
                            numerator: 1,
                            denominator: 1,
                        },
                        height: Ratio {
                            numerator: 1,
                            denominator: 1,
                        },
                        depth_or_array_layers: 1,
                    },
                    1,
                )
            } else {
                let opposite = super::compiler::family_descriptor(opposite_family);
                (opposite.extent.clone(), opposite.sample_count)
            };
            if descriptor.extent != expected_extent
                || descriptor.sample_count != expected_sample_count
            {
                return Err(invalid(
                    "compiler default descriptor inheritance is not canonical",
                    format!("textureFamilies[{fi}].source.descriptor"),
                ));
            }
        }
        if family.versions.is_empty() {
            return Err(invalid(
                "texture family has no versions",
                format!("textureFamilies[{fi}].versions"),
            ));
        }
        let mut first = u32::MAX;
        let mut last = 0;
        let mut all_initialized = true;
        for (vi, version) in family.versions.iter().enumerate() {
            let expected_target = if vi == 0 {
                source
            } else {
                family.versions[vi - 1].resource
            };
            let resource = graph
                .resources
                .get(version.resource as usize)
                .ok_or_else(|| {
                    invalid(
                        "texture version resource is out of bounds",
                        format!("textureFamilies[{fi}].versions[{vi}].resource"),
                    )
                })?;
            if version.version as usize != vi
                || version.target != expected_target
                || resource.semantic_type != SemanticType::Texture
                || resource.producer_execution.is_none()
                || resource.lifetime != Some(version.lifetime)
                || !matches!(resource.plan, ResourcePlan::Texture { family: f, version: v, target, initialized, stored, allocation }
                    if f == family.id && v == vi as u32 && target == expected_target
                        && initialized == version.initialized && stored == version.stored && allocation == family.allocation)
            {
                return Err(invalid(
                    "texture version is not canonical",
                    format!("textureFamilies[{fi}].versions[{vi}]"),
                ));
            }
            first = first.min(version.lifetime.first_use);
            last = last.max(version.lifetime.last_use);
            all_initialized &= version.initialized;
        }
        if family.lifetime
            != (Lifetime {
                first_use: first,
                last_use: last,
            })
        {
            return Err(invalid(
                "texture family lifetime is not canonical",
                format!("textureFamilies[{fi}].lifetime"),
            ));
        }
        let expected_aliasable = residency == TextureResidency::Transient && all_initialized;
        if family.aliasable != expected_aliasable {
            return Err(invalid(
                "texture family aliasability is not canonical",
                format!("textureFamilies[{fi}].aliasable"),
            ));
        }
    }
    for (ri, resource) in graph.resources.iter().enumerate() {
        let is_source = matches!(resource.plan, ResourcePlan::TextureSource { .. });
        if is_source != (source_claims[ri] == 1) {
            return Err(invalid(
                "texture source must be claimed by exactly one family",
                format!("resources[{ri}].plan"),
            ));
        }
    }
    Ok(())
}

fn validate_instance_traversal(graph: &CompiledGraph) -> Result<(), GraphError> {
    let pipeline_indices: Vec<_> = graph
        .executions
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            contract(&e.executor.key)
                .is_some_and(Contract::is_raster_draw)
                .then_some(i as u32)
        })
        .collect();
    let Some(plan) = &graph.instance_traversal else {
        return if pipeline_indices.is_empty() {
            Ok(())
        } else {
            Err(invalid(
                "live pipelines require instance traversal",
                "instanceTraversal",
            ))
        };
    };
    if pipeline_indices.is_empty() {
        return Err(invalid(
            "instance traversal has no live pipelines",
            "instanceTraversal",
        ));
    }
    if !resource_is_mesh(graph, plan.mesh) {
        return Err(invalid(
            "traversal mesh is invalid",
            "instanceTraversal.mesh",
        ));
    }
    let expressions = &plan.expressions.expressions;
    if expressions.len() > MAX_EXPRESSIONS || plan.pipelines.len() > MAX_PREDICATE_PIPELINES {
        return Err(invalid(
            "instance traversal exceeds runtime limits",
            "instanceTraversal",
        ));
    }
    let ty = |id: ExprId| expressions.get(id.0 as usize).map(|e| e.semantic_type);
    for (i, expression) in expressions.iter().enumerate() {
        let path = format!("instanceTraversal.expressions.expressions[{i}]");
        let ids: Vec<ExprId> = match &expression.op {
            ExpressionOp::Literal { literal } => {
                if literal.semantic_type() != expression.semantic_type || !literal.is_finite() {
                    return Err(invalid("literal type or value is invalid", &path));
                }
                vec![]
            }
            ExpressionOp::InstanceType { mesh } => {
                if expression.semantic_type != SemanticType::U32x16 || *mesh != plan.mesh {
                    return Err(invalid("instance type signature is invalid", &path));
                }
                vec![]
            }
            ExpressionOp::LocalAabb { mesh } => {
                if expression.semantic_type != SemanticType::LocalAabb || *mesh != plan.mesh {
                    return Err(invalid("local aabb signature is invalid", &path));
                }
                vec![]
            }
            ExpressionOp::Not { value } => {
                if expression.semantic_type != SemanticType::Bool
                    || ty(*value) != Some(SemanticType::Bool)
                {
                    return Err(invalid("not signature is invalid", &path));
                }
                vec![*value]
            }
            ExpressionOp::Boolean { operands, .. } => {
                if expression.semantic_type != SemanticType::Bool
                    || operands.len() > 8
                    || operands
                        .iter()
                        .any(|operand| ty(*operand) != Some(SemanticType::Bool))
                {
                    return Err(invalid("boolean signature is invalid", &path));
                }
                operands.clone()
            }
            ExpressionOp::CompareF32 { left, right, .. } => {
                if expression.semantic_type != SemanticType::Bool
                    || ty(*left) != Some(SemanticType::F32)
                    || ty(*right) != Some(SemanticType::F32)
                {
                    return Err(invalid("f32 comparison signature is invalid", &path));
                }
                vec![*left, *right]
            }
            ExpressionOp::CompareU32 { left, right, .. } => {
                if expression.semantic_type != SemanticType::Bool
                    || ty(*left) != Some(SemanticType::U32)
                    || ty(*right) != Some(SemanticType::U32)
                {
                    return Err(invalid("u32 comparison signature is invalid", &path));
                }
                vec![*left, *right]
            }
            ExpressionOp::VectorProject { vector, index } => {
                let n = match ty(*vector) {
                    Some(SemanticType::Vec2) => 2,
                    Some(SemanticType::Vec3) => 3,
                    Some(SemanticType::Vec4) => 4,
                    _ => 0,
                };
                if expression.semantic_type != SemanticType::F32 || usize::from(*index) >= n {
                    return Err(invalid("vector projection signature is invalid", &path));
                }
                vec![*vector]
            }
            ExpressionOp::VectorConstruct { components } => {
                let n = match expression.semantic_type {
                    SemanticType::Vec2 => 2,
                    SemanticType::Vec3 => 3,
                    SemanticType::Vec4 => 4,
                    _ => 0,
                };
                if components.len() != n
                    || components
                        .iter()
                        .any(|id| ty(*id) != Some(SemanticType::F32))
                {
                    return Err(invalid("vector constructor signature is invalid", &path));
                }
                components.clone()
            }
            ExpressionOp::MatrixColumn { matrix, index } => {
                let (n, out) = match ty(*matrix) {
                    Some(SemanticType::Mat2) => (2, SemanticType::Vec2),
                    Some(SemanticType::Mat3) => (3, SemanticType::Vec3),
                    Some(SemanticType::Mat4) => (4, SemanticType::Vec4),
                    _ => (0, SemanticType::Bool),
                };
                if usize::from(*index) >= n || expression.semantic_type != out {
                    return Err(invalid("matrix column signature is invalid", &path));
                }
                vec![*matrix]
            }
            ExpressionOp::MatrixConstruct { columns } => {
                let (n, col) = match expression.semantic_type {
                    SemanticType::Mat2 => (2, SemanticType::Vec2),
                    SemanticType::Mat3 => (3, SemanticType::Vec3),
                    SemanticType::Mat4 => (4, SemanticType::Vec4),
                    _ => (0, SemanticType::Bool),
                };
                if columns.len() != n || columns.iter().any(|id| ty(*id) != Some(col)) {
                    return Err(invalid("matrix constructor signature is invalid", &path));
                }
                columns.clone()
            }
            ExpressionOp::TypeWord { value, index } => {
                if expression.semantic_type != SemanticType::U32
                    || ty(*value) != Some(SemanticType::U32x16)
                    || *index >= 16
                {
                    return Err(invalid("type word signature is invalid", &path));
                }
                vec![*value]
            }
            ExpressionOp::TypeConstruct { words } => {
                if expression.semantic_type != SemanticType::U32x16
                    || words.len() != 16
                    || words.iter().any(|id| ty(*id) != Some(SemanticType::U32))
                {
                    return Err(invalid("type constructor signature is invalid", &path));
                }
                words.clone()
            }
            ExpressionOp::U32Bit { value, index } => {
                if expression.semantic_type != SemanticType::Bool
                    || ty(*value) != Some(SemanticType::U32)
                    || *index >= 32
                {
                    return Err(invalid("bit signature is invalid", &path));
                }
                vec![*value]
            }
            ExpressionOp::U32Construct { bits } => {
                if expression.semantic_type != SemanticType::U32
                    || bits.len() != 32
                    || bits.iter().any(|id| ty(*id) != Some(SemanticType::Bool))
                {
                    return Err(invalid("u32 constructor signature is invalid", &path));
                }
                bits.clone()
            }
            ExpressionOp::AabbMin { aabb } | ExpressionOp::AabbMax { aabb } => {
                if expression.semantic_type != SemanticType::Vec3
                    || ty(*aabb) != Some(SemanticType::LocalAabb)
                {
                    return Err(invalid("aabb projection signature is invalid", &path));
                }
                vec![*aabb]
            }
            ExpressionOp::FrustumCulled { mesh, local_aabb } => {
                if expression.semantic_type != SemanticType::Bool
                    || *mesh != plan.mesh
                    || ty(*local_aabb) != Some(SemanticType::LocalAabb)
                    || expressions
                        .get(local_aabb.0 as usize)
                        .and_then(|e| e.mesh_provenance)
                        != Some(plan.mesh)
                {
                    return Err(invalid("frustum signature or provenance is invalid", &path));
                }
                vec![*local_aabb]
            }
        };
        if ids.iter().any(|id| id.0 as usize >= i) {
            return Err(invalid("expression operands must precede consumer", &path));
        }
        let expected_provenance = match &expression.op {
            ExpressionOp::Literal { .. } => None,
            ExpressionOp::InstanceType { .. } | ExpressionOp::LocalAabb { .. } => Some(plan.mesh),
            _ => {
                let mut p = ids
                    .iter()
                    .filter_map(|id| expressions[id.0 as usize].mesh_provenance);
                let first = p.next();
                if p.any(|v| Some(v) != first) {
                    return Err(invalid("expression mixes mesh provenance", &path));
                }
                first
            }
        };
        if expression.mesh_provenance != expected_provenance {
            return Err(invalid("expression provenance is not canonical", &path));
        }
    }
    let mut seen = HashSet::new();
    let mut reachable = vec![false; expressions.len()];
    for (ordinal, entry) in plan.pipelines.iter().enumerate() {
        if entry.ordinal as usize != ordinal
            || pipeline_indices.get(ordinal) != Some(&entry.execution)
            || !seen.insert(entry.execution)
            || ty(entry.predicate) != Some(SemanticType::Bool)
        {
            return Err(invalid(
                "pipeline predicate table is not canonical",
                format!("instanceTraversal.pipelines[{ordinal}]"),
            ));
        }
        let mut stack = vec![entry.predicate];
        while let Some(id) = stack.pop() {
            if reachable[id.0 as usize] {
                continue;
            }
            reachable[id.0 as usize] = true;
            stack.extend(expression_operands(&expressions[id.0 as usize].op));
        }
    }
    if plan.pipelines.len() != pipeline_indices.len() {
        return Err(invalid(
            "pipeline predicate table is incomplete",
            "instanceTraversal.pipelines",
        ));
    }
    let requires_camera = expressions
        .iter()
        .enumerate()
        .any(|(i, e)| reachable[i] && matches!(e.op, ExpressionOp::FrustumCulled { .. }));
    if plan.requires_camera != requires_camera {
        return Err(invalid(
            "requires_camera is not canonical",
            "instanceTraversal.requiresCamera",
        ));
    }
    Ok(())
}

fn expression_operands(op: &ExpressionOp) -> Vec<ExprId> {
    match op {
        ExpressionOp::Not { value }
        | ExpressionOp::VectorProject { vector: value, .. }
        | ExpressionOp::MatrixColumn { matrix: value, .. }
        | ExpressionOp::TypeWord { value, .. }
        | ExpressionOp::U32Bit { value, .. } => vec![*value],
        ExpressionOp::AabbMin { aabb } | ExpressionOp::AabbMax { aabb } => vec![*aabb],
        ExpressionOp::FrustumCulled { local_aabb, .. } => vec![*local_aabb],
        ExpressionOp::Boolean { operands, .. } => operands.clone(),
        ExpressionOp::CompareF32 { left, right, .. }
        | ExpressionOp::CompareU32 { left, right, .. } => vec![*left, *right],
        ExpressionOp::VectorConstruct { components } => components.clone(),
        ExpressionOp::MatrixConstruct { columns } => columns.clone(),
        ExpressionOp::TypeConstruct { words } => words.clone(),
        ExpressionOp::U32Construct { bits } => bits.clone(),
        _ => vec![],
    }
}

pub fn prepare_runtime_plan(
    graph: &CompiledGraph,
    surface: RuntimeSurfaceContract,
    limits: Option<&wgpu::Limits>,
) -> Result<RuntimePlan, GraphError> {
    validate_canonical_plan(graph)?;
    if surface.width == 0 || surface.height == 0 {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "surface extent is zero",
            "surface",
        ));
    }
    if !surface
        .usage
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "surface lacks render attachment usage",
            "surface.usage",
        ));
    }
    let mut frame_out_index = None;
    let mut executions = Vec::with_capacity(graph.executions.len());
    for (i, execution) in graph.executions.iter().enumerate() {
        let path = format!("executions[{i}]");
        match execution.executor.key.as_str() {
            key if contract(key).is_some_and(Contract::is_raster_draw) => {}
            _ if contract(&execution.executor.key)
                .is_some_and(|contract| contract.fullscreen_policy.is_some()) => {}
            "frame_out" => {
                if frame_out_index.replace(i).is_some() {
                    return Err(error(
                        "GRAPH_EXECUTION_UNSUPPORTED",
                        "exactly one frame_out is required",
                        "executions",
                    ));
                }
            }
            _ => {
                return Err(error(
                    "GRAPH_EXECUTION_UNSUPPORTED",
                    "unsupported execution",
                    &path,
                ))
            }
        }
        executions.push(RuntimeExecution {
            execution: u32::try_from(i).map_err(|_| invalid("execution index overflow", &path))?,
            executor: execution.executor.key.clone(),
        });
    }
    let frame_out_index = frame_out_index.ok_or_else(|| {
        error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "exactly one frame_out is required",
            "executions",
        )
    })?;
    validate_instance_traversal(graph)?;

    for (i, execution) in graph.executions.iter().enumerate() {
        if !contract(&execution.executor.key).is_some_and(Contract::is_raster_draw) {
            continue;
        }
        let NormalizedParameters::Raster {
            draw_order: _,
            clear_depth,
            clear_color,
            ..
        } = &execution.parameters
        else {
            return Err(invalid(
                "pipeline parameters mismatch",
                format!("executions[{i}].parameters"),
            ));
        };
        if !clear_depth.is_finite()
            || !(0.0..=1.0).contains(clear_depth)
            || clear_color.iter().any(|value| !value.is_finite())
        {
            return Err(invalid(
                "pipeline parameters are invalid",
                format!("executions[{i}].parameters"),
            ));
        }
        if !matches!(execution.kind, ExecutionKind::RasterDraw) {
            return Err(invalid(
                "pipeline render kind mismatch",
                format!("executions[{i}].kind"),
            ));
        }
        let (color_attachments, depth_stencil) = super::compiler::execution_attachments(execution);
        let Some(depth_attachment) = depth_stencil.as_ref() else {
            return Err(invalid(
                "pipeline depth attachment missing",
                format!("executions[{i}].kind"),
            ));
        };
        let [color_attachment] = color_attachments.as_slice() else {
            return Err(invalid(
                "pipeline color attachment shape mismatch",
                format!("executions[{i}].kind"),
            ));
        };
        let [mesh_input, color_input, depth_input] = execution.inputs.as_slice() else {
            return Err(invalid(
                "pipeline input shape mismatch",
                format!("executions[{i}].inputs"),
            ));
        };
        if [
            mesh_input.socket.as_str(),
            color_input.socket.as_str(),
            depth_input.socket.as_str(),
        ] != ["mesh", "colorTarget", "depthTarget"]
        {
            return Err(invalid(
                "pipeline input sockets mismatch",
                format!("executions[{i}].inputs"),
            ));
        }
        let [color_output, depth_output] = execution.outputs.as_slice() else {
            return Err(invalid(
                "pipeline output shape mismatch",
                format!("executions[{i}].outputs"),
            ));
        };
        if color_output.socket != "color"
            || depth_output.socket != "depth"
            || color_output.resource != color_attachment.resource
            || depth_output.resource != depth_attachment.resource
        {
            return Err(invalid(
                "pipeline outputs disagree with attachments",
                format!("executions[{i}].outputs"),
            ));
        }
        let (color_version, color_stored, color_target) = match graph
            .resources
            .get(color_output.resource as usize)
            .map(|r| &r.plan)
        {
            Some(ResourcePlan::Texture {
                version,
                stored,
                target,
                ..
            }) => (*version, *stored, *target),
            _ => {
                return Err(invalid(
                    "pipeline color output kind is invalid",
                    format!("resources[{}].plan", color_output.resource),
                ))
            }
        };
        let (depth_version, depth_stored, depth_target) = match graph
            .resources
            .get(depth_output.resource as usize)
            .map(|r| &r.plan)
        {
            Some(ResourcePlan::Texture {
                version,
                stored,
                target,
                ..
            }) => (*version, *stored, *target),
            _ => {
                return Err(invalid(
                    "pipeline depth output kind is invalid",
                    format!("resources[{}].plan", depth_output.resource),
                ))
            }
        };
        let expected_color_load = if color_version == 0 {
            NormalizedColorLoad::Clear {
                value: *clear_color,
            }
        } else {
            NormalizedColorLoad::Load
        };
        let expected_depth_load = if depth_version == 0 {
            NormalizedDepthLoad::Clear {
                value: *clear_depth,
            }
        } else {
            NormalizedDepthLoad::Load
        };
        if color_attachment.load != expected_color_load {
            return Err(invalid(
                "pipeline color load is not canonical",
                format!("executions[{i}].kind"),
            ));
        }
        if depth_attachment.load != expected_depth_load {
            return Err(invalid(
                "pipeline depth load is not canonical",
                format!("executions[{i}].kind"),
            ));
        }
        let stored_op = |stored| {
            if stored {
                StoreOp::Store
            } else {
                StoreOp::Discard
            }
        };
        if color_attachment.store != stored_op(color_stored)
            || depth_attachment.store != stored_op(depth_stored)
            || (color_version > 0
                && !matches!(
                    graph.resources.get(color_target as usize).map(|r| &r.plan),
                    Some(ResourcePlan::Texture { stored: true, .. })
                ))
            || (depth_version > 0
                && !matches!(
                    graph.resources.get(depth_target as usize).map(|r| &r.plan),
                    Some(ResourcePlan::Texture { stored: true, .. })
                ))
        {
            return Err(invalid(
                "pipeline store metadata is not canonical",
                format!("executions[{i}].kind"),
            ));
        }
        let (base_accesses, resolve_access) = match execution.accesses.as_slice() {
            [mesh, color, depth] => (&[mesh, color, depth][..], None),
            [mesh, color, depth, resolve] => (&[mesh, color, depth][..], Some(resolve)),
            _ => {
                return Err(invalid(
                    "pipeline access shape mismatch",
                    format!("executions[{i}].accesses"),
                ));
            }
        };
        let [mesh_access, color_access, depth_access] = base_accesses else {
            unreachable!()
        };
        let expected_store = stored_op(color_stored);
        if mesh_access.socket != "mesh"
            || mesh_access.resource != mesh_input.resource
            || !matches!(mesh_access.mode, AccessMode::SemanticRead)
        {
            return Err(invalid(
                "pipeline semantic accesses mismatch",
                format!("executions[{i}].accesses"),
            ));
        }
        let color_clear = matches!(color_attachment.load, NormalizedColorLoad::Clear { .. });
        let depth_clear = matches!(depth_attachment.load, NormalizedDepthLoad::Clear { .. });
        if color_access.socket != "color"
            || color_access.resource != color_output.resource
            || !matches!(
                color_access.mode,
                AccessMode::ColorAttachment { location: 0, load, store, full_overwrite }
                    if load == color_attachment.load && store == expected_store && full_overwrite == color_clear
            )
            || color_attachment.location != 0
            || depth_access.socket != "depth"
            || depth_access.resource != depth_output.resource
            || !matches!(
                depth_access.mode,
                AccessMode::DepthAttachment { load, store, full_overwrite }
                    if load == depth_attachment.load && store == stored_op(depth_stored) && full_overwrite == depth_clear
            )
        {
            return Err(invalid(
                "pipeline attachment accesses mismatch",
                format!("executions[{i}].accesses"),
            ));
        }
        match (color_attachment.resolve_target, resolve_access) {
            (None, None) => {}
            (Some(target), Some(access))
                if access.socket == "colorResolve"
                    && access.resource == target
                    && matches!(access.mode, AccessMode::ColorResolve { source, location: 0 } if source == color_attachment.resource) =>
                {}
            _ => {
                return Err(invalid(
                    "pipeline color resolve mismatch",
                    format!("executions[{i}].accesses"),
                ))
            }
        }
        let mesh_resource = graph
            .resources
            .get(mesh_input.resource as usize)
            .ok_or_else(|| {
                invalid(
                    "pipeline mesh is out of bounds",
                    format!("executions[{i}].inputs"),
                )
            })?;
        if mesh_resource.semantic_type != SemanticType::MeshData
            || !matches!(mesh_resource.plan, ResourcePlan::MeshData)
        {
            return Err(invalid(
                "pipeline mesh is invalid",
                format!("resources[{}].plan", mesh_input.resource),
            ));
        }
        for (output, target) in [
            (color_output.resource, color_input.resource),
            (depth_output.resource, depth_input.resource),
        ] {
            let resource = graph.resources.get(output as usize).ok_or_else(|| {
                invalid(
                    "pipeline output is out of bounds",
                    format!("executions[{i}].outputs"),
                )
            })?;
            if resource.semantic_type != SemanticType::Texture {
                return Err(invalid(
                    "pipeline output is not a texture",
                    format!("resources[{output}].semanticType"),
                ));
            }
            if resource.producer_execution != Some(i as u32)
                || !matches!(resource.plan, ResourcePlan::Texture { target: actual, .. } if actual == target)
            {
                return Err(invalid(
                    "pipeline texture transition is invalid",
                    format!("resources[{output}].plan"),
                ));
            }
        }
        let color_descriptor =
            texture_descriptor(graph, color_output.resource).ok_or_else(|| {
                invalid(
                    "pipeline color attachment descriptor is invalid",
                    format!("executions[{i}].inputs"),
                )
            })?;
        if color_descriptor.format == TextureFormat::Depth32Float
            || color_descriptor.dimension != TextureDimension::D2
            || !matches!(color_descriptor.sample_count, 1 | 4)
            || color_descriptor.mip_level_count != 1
            || !color_descriptor.view_formats.is_empty()
        {
            return Err(invalid(
                "pipeline color attachment descriptor is invalid",
                format!("executions[{i}].inputs"),
            ));
        }
        let depth_descriptor =
            texture_descriptor(graph, depth_output.resource).ok_or_else(|| {
                invalid(
                    "pipeline depth attachment descriptor is invalid",
                    format!("executions[{i}].inputs"),
                )
            })?;
        if depth_descriptor.format != TextureFormat::Depth32Float
            || depth_descriptor.dimension != TextureDimension::D2
            || !matches!(depth_descriptor.sample_count, 1 | 4)
            || depth_descriptor.mip_level_count != 1
            || !depth_descriptor.view_formats.is_empty()
        {
            return Err(invalid(
                "pipeline depth attachment descriptor is invalid",
                format!("executions[{i}].inputs"),
            ));
        }
        if color_descriptor.extent != depth_descriptor.extent
            || color_descriptor.sample_count != depth_descriptor.sample_count
            || (color_descriptor.sample_count == 1 && color_attachment.resolve_target.is_some())
        {
            return Err(invalid(
                "pipeline attachment extents mismatch",
                format!("executions[{i}].inputs"),
            ));
        }
    }

    let frame_out = &graph.executions[frame_out_index];
    let NormalizedParameters::FrameOut {
        surface_format,
        dynamic_range,
        output_transfer,
        scale_mode: _,
        filter: _,
        background_color,
    } = &frame_out.parameters
    else {
        return Err(invalid(
            "frame_out parameters mismatch",
            format!("executions[{frame_out_index}].parameters"),
        ));
    };
    let requested_format = match surface_format {
        SurfaceFormatRequest::Preferred => None,
        SurfaceFormatRequest::Rgba8Unorm => Some(wgpu::TextureFormat::Rgba8Unorm),
        SurfaceFormatRequest::Bgra8Unorm => Some(wgpu::TextureFormat::Bgra8Unorm),
        SurfaceFormatRequest::Rgba16Float => Some(wgpu::TextureFormat::Rgba16Float),
    };
    if requested_format.is_some_and(|format| format != surface.format) {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "resolved surface format does not match frame output request",
            format!(
                "nodes[{}].parameters.surfaceFormat",
                frame_out.original_node_index
            ),
        ));
    }
    let parameters_valid = background_color
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        && match dynamic_range {
            FrameDynamicRange::Sdr => true,
            FrameDynamicRange::Hdr { exposure_stops, .. } => {
                exposure_stops.is_finite() && (-10.0..=10.0).contains(exposure_stops)
            }
        };
    if !parameters_valid {
        return Err(invalid(
            "frame_out parameters are out of range",
            format!("executions[{frame_out_index}].parameters"),
        ));
    }
    if *output_transfer == OutputTransfer::Linear && surface.format.is_srgb() {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "linear output transfer cannot target an sRGB surface",
            format!("executions[{frame_out_index}].parameters.outputTransfer"),
        ));
    }
    let ExecutionKind::FrameOut { color } = frame_out.kind else {
        return Err(invalid(
            "frame_out execution kind mismatch",
            format!("executions[{frame_out_index}].kind"),
        ));
    };
    if !frame_out.outputs.is_empty() {
        return Err(invalid(
            "frame_out must not have outputs",
            format!("executions[{frame_out_index}].outputs"),
        ));
    }
    if !matches!(
        frame_out.inputs.as_slice(),
        [CompiledSocketInput { socket, resource }] if socket == "color" && *resource == color
    ) {
        return Err(invalid(
            "frame_out input does not match its color resource",
            format!("executions[{frame_out_index}].inputs"),
        ));
    }
    if !matches!(
        frame_out.accesses.as_slice(),
        [CompiledAccess { socket, resource, mode: AccessMode::SampledTexture }]
            if socket == "color" && *resource == color
    ) {
        return Err(invalid(
            "frame_out access does not match its color resource",
            format!("executions[{frame_out_index}].accesses"),
        ));
    }
    let color_resource = graph.resources.get(color as usize).ok_or_else(|| {
        invalid(
            "frame_out resource is out of bounds",
            format!("executions[{frame_out_index}].kind.color"),
        )
    })?;
    if color_resource.semantic_type != SemanticType::Texture {
        return Err(invalid(
            "frame_out resource is not a texture",
            format!("resources[{color}].semanticType"),
        ));
    }
    let ResourcePlan::Texture {
        family,
        initialized: true,
        stored: true,
        allocation: Some(_),
        ..
    } = color_resource.plan
    else {
        return Err(invalid(
            "frame_out color is not a stored initialized allocated texture",
            format!("resources[{color}].plan"),
        ));
    };
    let family = graph.texture_families.get(family as usize).ok_or_else(|| {
        invalid(
            "frame_out texture family is out of bounds",
            format!("resources[{color}].plan.family"),
        )
    })?;
    let descriptor = super::compiler::family_descriptor(family);
    if !super::compiler::frame_out_source_compatible(descriptor, dynamic_range) {
        return Err(invalid(
            "frame_out texture descriptor is incompatible",
            format!("textureFamilies[{}].source.descriptor", family.id),
        ));
    }

    let mut resource_allocations = vec![None; graph.resources.len()];
    for (fi, family) in graph.texture_families.iter().enumerate() {
        if family.id as usize != fi {
            return Err(invalid(
                "texture family id does not match index",
                format!("textureFamilies[{fi}].id"),
            ));
        }
        if family.usage != super::compiler::texture_usage(family, &graph.executions) {
            return Err(invalid(
                "texture family usage is not canonical",
                format!("textureFamilies[{fi}].usage"),
            ));
        }
        match &family.source {
            TextureFamilySource::AuthoredTexture {
                residency,
                descriptor,
                ..
            } => {
                if !matches!(
                    residency,
                    TextureResidency::Transient | TextureResidency::Persistent
                ) || descriptor.dimension != TextureDimension::D2
                    || descriptor.mip_level_count != 1
                    || !matches!(descriptor.sample_count, 1 | 4)
                    || !matches!(
                        descriptor.extent,
                        NormalizedTextureExtent::Absolute {
                            depth_or_array_layers: 1,
                            ..
                        } | NormalizedTextureExtent::SurfaceRelative {
                            depth_or_array_layers: 1,
                            ..
                        }
                    )
                {
                    return Err(error(
                        "GRAPH_EXECUTION_UNSUPPORTED",
                        "unsupported runtime texture descriptor",
                        format!("textureFamilies[{fi}]"),
                    ));
                }
                if family.allocation.is_none() {
                    return Err(invalid(
                        "authored family has no allocation",
                        format!("textureFamilies[{fi}].allocation"),
                    ));
                }
            }
            TextureFamilySource::CompilerDefaultInput { descriptor, .. } => {
                if descriptor.dimension != TextureDimension::D2
                    || !matches!(descriptor.sample_count, 1 | 4)
                    || descriptor.mip_level_count != 1
                    || family.allocation.is_none()
                {
                    return Err(invalid(
                        "compiler default family is not canonical",
                        format!("textureFamilies[{fi}]"),
                    ));
                }
            }
            TextureFamilySource::CompilerColorResolve { descriptor, .. } => {
                if !super::compiler::is_single_view_d2(descriptor) || family.allocation.is_none() {
                    return Err(invalid(
                        "compiler resolve family is not canonical",
                        format!("textureFamilies[{fi}]"),
                    ));
                }
            }
        }
        for (vi, version) in family.versions.iter().enumerate() {
            if version.version as usize != vi {
                return Err(invalid(
                    "texture version does not match index",
                    format!("textureFamilies[{fi}].versions[{vi}]"),
                ));
            }
            let resource = graph
                .resources
                .get(version.resource as usize)
                .ok_or_else(|| {
                    invalid(
                        "version resource is out of bounds",
                        format!("textureFamilies[{fi}].versions[{vi}].resource"),
                    )
                })?;
            let ResourcePlan::Texture {
                family: rf,
                version: rv,
                allocation,
                ..
            } = &resource.plan
            else {
                return Err(invalid(
                    "version resource is not a texture",
                    format!("resources[{}].plan", version.resource),
                ));
            };
            if *rf != family.id || *rv != version.version || *allocation != family.allocation {
                return Err(invalid(
                    "texture resource and family disagree",
                    format!("resources[{}].plan", version.resource),
                ));
            }
            resource_allocations[version.resource as usize] = *allocation;
        }
    }
    // Validate the resource-to-family direction as well; compiled plans are public and may be
    // cloned and modified by callers.
    for (ri, resource) in graph.resources.iter().enumerate() {
        if let ResourcePlan::Texture {
            family,
            version,
            allocation,
            ..
        } = resource.plan
        {
            let family_plan = graph.texture_families.get(family as usize).ok_or_else(|| {
                invalid(
                    "texture resource family is out of bounds",
                    format!("resources[{ri}].plan.family"),
                )
            })?;
            let family_version = family_plan.versions.get(version as usize).ok_or_else(|| {
                invalid(
                    "texture resource version is out of bounds",
                    format!("resources[{ri}].plan.version"),
                )
            })?;
            if family_version.resource as usize != ri || allocation != family_plan.allocation {
                return Err(invalid(
                    "texture resource is inconsistent with its family",
                    format!("resources[{ri}].plan"),
                ));
            }
        }
    }

    let mut classes = Vec::with_capacity(graph.allocation_classes.len());
    for (ci, class) in graph.allocation_classes.iter().enumerate() {
        let mut slots = Vec::with_capacity(class.slots.len());
        for (si, slot) in class.slots.iter().enumerate() {
            let allocation = AllocationRef {
                class: ci as u32,
                slot: si as u32,
            };
            if slot.occupants.is_empty() {
                return Err(invalid(
                    "allocation slot has no occupants",
                    format!("allocationClasses[{ci}].slots[{si}].occupants"),
                ));
            }
            let mut expected_usage = BTreeSet::new();
            let mut occupants = HashSet::new();
            for &family_id in &slot.occupants {
                if !occupants.insert(family_id) {
                    return Err(invalid(
                        "allocation slot has duplicate occupants",
                        format!("allocationClasses[{ci}].slots[{si}].occupants"),
                    ));
                }
                let family = graph
                    .texture_families
                    .get(family_id as usize)
                    .ok_or_else(|| {
                        invalid(
                            "slot occupant is out of bounds",
                            format!("allocationClasses[{ci}].slots[{si}].occupants"),
                        )
                    })?;
                if family.allocation != Some(allocation) {
                    return Err(invalid(
                        "slot occupant allocation disagrees",
                        format!("allocationClasses[{ci}].slots[{si}].occupants"),
                    ));
                }
                let (descriptor, residency) = match &family.source {
                    TextureFamilySource::AuthoredTexture {
                        descriptor,
                        residency,
                        ..
                    } => (descriptor, *residency),
                    TextureFamilySource::CompilerDefaultInput { descriptor, .. } => {
                        (descriptor, TextureResidency::Transient)
                    }
                    TextureFamilySource::CompilerColorResolve { descriptor, .. } => {
                        (descriptor, TextureResidency::Transient)
                    }
                };
                expected_usage.extend(family.usage.iter().copied());
                let kind_valid = match slot.kind {
                    AllocationKind::AliasedTransient => {
                        residency == TextureResidency::Transient && family.aliasable
                    }
                    AllocationKind::DedicatedTransient => {
                        slot.occupants.len() == 1
                            && residency == TextureResidency::Transient
                            && !family.aliasable
                    }
                    AllocationKind::Persistent => {
                        slot.occupants.len() == 1
                            && residency == TextureResidency::Persistent
                            && !family.aliasable
                    }
                };
                if !kind_valid {
                    return Err(invalid(
                        "allocation slot kind disagrees with occupant",
                        format!("allocationClasses[{ci}].slots[{si}].occupants"),
                    ));
                }
                if descriptor.dimension != class.key.dimension
                    || descriptor.format != class.key.format
                    || descriptor.extent != class.key.extent
                    || descriptor.mip_level_count != class.key.mip_level_count
                    || descriptor.sample_count != class.key.sample_count
                    || descriptor.view_formats != class.key.view_formats
                {
                    return Err(invalid(
                        "occupant descriptor does not match class key",
                        format!("allocationClasses[{ci}].key"),
                    ));
                }
            }
            if slot.usage != expected_usage.into_iter().collect::<Vec<_>>() {
                return Err(invalid(
                    "allocation slot usage is not canonical",
                    format!("allocationClasses[{ci}].slots[{si}].usage"),
                ));
            }
            if slot.kind == AllocationKind::AliasedTransient {
                for (a, &left) in slot.occupants.iter().enumerate() {
                    for &right in &slot.occupants[a + 1..] {
                        let l = graph.texture_families[left as usize].lifetime;
                        let r = graph.texture_families[right as usize].lifetime;
                        if l.first_use <= r.last_use && r.first_use <= l.last_use {
                            return Err(invalid(
                                "aliased occupant lifetimes overlap",
                                format!("allocationClasses[{ci}].slots[{si}].occupants"),
                            ));
                        }
                    }
                }
            }
            slots.push(RuntimeAllocationSlot {
                kind: slot.kind,
                descriptor: runtime_texture_descriptor(
                    &class.key,
                    &slot.usage,
                    [surface.width, surface.height],
                    limits,
                )?,
                occupants: slot.occupants.clone(),
            });
        }
        classes.push(RuntimeAllocationClass {
            key: class.key.clone(),
            slots,
        });
    }
    for (fi, family) in graph.texture_families.iter().enumerate() {
        if let Some(allocation) = family.allocation {
            let slot = graph
                .allocation_classes
                .get(allocation.class as usize)
                .and_then(|c| c.slots.get(allocation.slot as usize))
                .ok_or_else(|| {
                    invalid(
                        "family allocation is out of bounds",
                        format!("textureFamilies[{fi}].allocation"),
                    )
                })?;
            if slot.occupants.iter().filter(|&&id| id == family.id).count() != 1 {
                return Err(invalid(
                    "family is not exactly once in allocation occupants",
                    format!("textureFamilies[{fi}].allocation"),
                ));
            }
        }
    }
    Ok(RuntimePlan {
        allocations: RuntimeAllocationPlan {
            classes,
            resource_allocations,
        },
        executions,
        render_passes: graph.render_passes.clone(),
        instance_traversal: graph.instance_traversal.clone(),
        surface,
    })
}

pub fn validate_activatable(graph: &CompiledGraph) -> Result<(), GraphError> {
    // Runtime validation must diagnose noncanonical compiled plans before the
    // live-capability resolver. The validator below remains authoritative for
    // missing or duplicated Frame Out work, so a missing request gets a
    // harmless synthetic default solely for constructing test capabilities.
    let (request, frame_out_index) = graph
        .executions
        .iter()
        .find_map(|execution| match execution.parameters {
            NormalizedParameters::FrameOut { surface_format, .. } => {
                Some((surface_format, execution.original_node_index))
            }
            _ => None,
        })
        .unwrap_or((SurfaceFormatRequest::Preferred, 0));
    let format = match request {
        SurfaceFormatRequest::Preferred | SurfaceFormatRequest::Bgra8Unorm => {
            wgpu::TextureFormat::Bgra8Unorm
        }
        SurfaceFormatRequest::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        SurfaceFormatRequest::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
    };
    let capabilities = wgpu::SurfaceCapabilities {
        formats: vec![format],
        present_modes: vec![wgpu::PresentMode::Fifo],
        alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
        usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
    };
    let surface = resolve_surface_contract(
        request,
        &capabilities,
        1,
        1,
        &surface_format_path(frame_out_index),
    )?;
    prepare_runtime_plan(graph, surface, None).map(|_| ())
}
