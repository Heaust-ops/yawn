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
    pub view_formats: Vec<wgpu::TextureFormat>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshQueryRuntimeKey {
    pub visible: RuntimePredicate,
    pub frustum_culled: RuntimePredicate,
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
    pub query: MeshQueryRuntimeKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePlan {
    pub allocations: RuntimeAllocationPlan,
    pub executions: Vec<RuntimeExecution>,
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

fn valid_pipeline_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().enumerate().all(|(i, byte)| {
            byte == b'_' || byte.is_ascii_alphanumeric() && (i > 0 || byte.is_ascii_alphabetic())
        })
}

fn execution_supported(key: &str) -> bool {
    contract(key).is_some_and(|contract| {
        contract.fullscreen_policy.is_some()
            || matches!(
                key,
                "frustum_cull" | "mesh_query" | "pipeline_registry" | "pipeline" | "frame_out"
            )
    })
}

fn resource_is_mesh(graph: &CompiledGraph, id: u32) -> bool {
    graph.resources.get(id as usize).is_some_and(|resource| {
        resource.semantic_type == SemanticType::MeshData
            && matches!(resource.plan, ResourcePlan::MeshData)
    })
}

fn has_exact_producer(
    graph: &CompiledGraph,
    consumer: usize,
    resource: u32,
    executor: &str,
    socket: &str,
) -> bool {
    graph.executions[..consumer].iter().any(|execution| {
        execution.executor.key == executor
            && matches!(execution.outputs.as_slice(), [output] if output.socket == socket && output.resource == resource)
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
        TextureFamilySource::AuthoredTexture { descriptor, .. } => Some(descriptor),
    }
}

fn single_view_d2(d: &NormalizedTextureDescriptor) -> bool {
    d.dimension == TextureDimension::D2
        && d.mip_level_count == 1
        && d.sample_count == 1
        && d.view_formats.is_empty()
        && matches!(
            d.extent,
            NormalizedTextureExtent::Absolute {
                depth_or_array_layers: 1,
                ..
            } | NormalizedTextureExtent::SurfaceRelative {
                depth_or_array_layers: 1,
                ..
            }
        )
}

fn validate_fullscreen_execution(
    graph: &CompiledGraph,
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
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: None,
    } = &execution.kind
    else {
        return Err(invalid("fullscreen render kind mismatch", path("kind")));
    };
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
        let producer = graph.executions[..i].iter().position(|candidate| {
            candidate
                .outputs
                .iter()
                .any(|value| value.resource == input.resource)
        });
        if producer.is_none() {
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
        single_view_d2(d) && d.format == TextureFormat::Rgba16Float
    };
    let descriptors_valid = hdr(source)
        && match policy {
            FullscreenPolicy::Copy => {
                single_view_d2(target_d)
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

fn validate_compute_execution(
    graph: &CompiledGraph,
    i: usize,
    execution: &CompiledExecution,
) -> Result<(), GraphError> {
    let path = |field| format!("executions[{i}].{field}");
    match execution.executor.key.as_str() {
        "frustum_cull" => {
            if !matches!(
                execution.parameters,
                NormalizedParameters::FrustumCull {
                    camera: ActiveCamera::Active
                }
            ) {
                return Err(invalid(
                    "frustum cull parameters mismatch",
                    path("parameters"),
                ));
            }
            if !matches!(
                execution.kind,
                ExecutionKind::Compute {
                    work: ComputeWork::FrustumCull
                }
            ) {
                return Err(invalid("frustum cull work mismatch", path("kind")));
            }
            let [mesh, aabbs] = execution.inputs.as_slice() else {
                return Err(invalid("frustum cull inputs mismatch", path("inputs")));
            };
            let [flags] = execution.outputs.as_slice() else {
                return Err(invalid("frustum cull outputs mismatch", path("outputs")));
            };
            if mesh.socket != "mesh"
                || aabbs.socket != "localAabbs"
                || flags.socket != "isFrustumCulled"
            {
                return Err(invalid(
                    "frustum cull socket order mismatch",
                    path("inputs"),
                ));
            }
            if !matches!(execution.accesses.as_slice(),
                [CompiledAccess { socket: s0, resource: r0, mode: AccessMode::StorageRead },
                 CompiledAccess { socket: s1, resource: r1, mode: AccessMode::StorageRead },
                 CompiledAccess { socket: s2, resource: r2, mode: AccessMode::StorageWrite { full_overwrite: true } }]
                if s0 == "mesh" && *r0 == mesh.resource && s1 == "localAabbs" && *r1 == aabbs.resource
                    && s2 == "isFrustumCulled" && *r2 == flags.resource)
            {
                return Err(invalid("frustum cull accesses mismatch", path("accesses")));
            }
            if !resource_is_mesh(graph, mesh.resource)
                || !matches!(graph.resources[aabbs.resource as usize], CompiledResource { semantic_type: SemanticType::LocalAabbBuffer, plan: ResourcePlan::LocalAabbBuffer { mesh: m }, .. } if m == mesh.resource)
                || !matches!(graph.resources[flags.resource as usize], CompiledResource { semantic_type: SemanticType::BooleanFlagBuffer, plan: ResourcePlan::BooleanFlagBuffer { mesh: m, flag: MeshFlag::IsFrustumCulled }, .. } if m == mesh.resource)
            {
                return Err(invalid(
                    "frustum cull mesh provenance mismatch",
                    path("inputs"),
                ));
            }
        }
        "mesh_query" => {
            let NormalizedParameters::MeshQuery {
                visible_predicate,
                frustum_culled_predicate,
            } = execution.parameters
            else {
                return Err(invalid(
                    "mesh query parameters mismatch",
                    path("parameters"),
                ));
            };
            if (visible_predicate == RuntimePredicate::Never)
                != (frustum_culled_predicate == RuntimePredicate::Never)
            {
                return Err(invalid(
                    "mesh query never predicates must be paired",
                    path("parameters"),
                ));
            }
            if !matches!(
                execution.kind,
                ExecutionKind::Compute {
                    work: ComputeWork::MeshQuery
                }
            ) {
                return Err(invalid("mesh query work mismatch", path("kind")));
            }
            let active = |p| {
                matches!(
                    p,
                    RuntimePredicate::RequiredTrue | RuntimePredicate::RequiredFalse
                )
            };
            let mut sockets = vec!["mesh"];
            if active(visible_predicate) {
                sockets.push("isVisible");
            }
            if active(frustum_culled_predicate) {
                sockets.push("isFrustumCulled");
            }
            if execution.inputs.len() != sockets.len()
                || execution
                    .inputs
                    .iter()
                    .zip(&sockets)
                    .any(|(v, s)| v.socket != *s)
                || !matches!(execution.outputs.as_slice(), [CompiledSocketOutput { socket, .. }] if socket == "draws")
            {
                return Err(invalid("mesh query socket order mismatch", path("inputs")));
            }
            let output = execution.outputs[0].resource;
            if execution.accesses.len() != sockets.len() + 1
                || execution
                    .inputs
                    .iter()
                    .zip(&execution.accesses)
                    .any(|(input, access)| {
                        access.socket != input.socket
                            || access.resource != input.resource
                            || !matches!(access.mode, AccessMode::StorageRead)
                    })
                || !matches!(&execution.accesses[sockets.len()], CompiledAccess { socket, resource, mode: AccessMode::StorageWrite { full_overwrite: true } } if socket == "draws" && *resource == output)
            {
                return Err(invalid("mesh query accesses mismatch", path("accesses")));
            }
            let mesh = execution.inputs[0].resource;
            if !resource_is_mesh(graph, mesh) {
                return Err(invalid(
                    "mesh query mesh provenance mismatch",
                    path("inputs"),
                ));
            }
            for input in execution.inputs.iter().skip(1) {
                let flag = if input.socket == "isVisible" {
                    MeshFlag::IsVisible
                } else {
                    MeshFlag::IsFrustumCulled
                };
                if !matches!(graph.resources[input.resource as usize], CompiledResource { semantic_type: SemanticType::BooleanFlagBuffer, plan: ResourcePlan::BooleanFlagBuffer { mesh: m, flag: f }, .. } if m == mesh && f == flag)
                {
                    return Err(invalid(
                        "mesh query flag provenance mismatch",
                        path("inputs"),
                    ));
                }
                if flag == MeshFlag::IsFrustumCulled
                    && !has_exact_producer(
                        graph,
                        i,
                        input.resource,
                        "frustum_cull",
                        "isFrustumCulled",
                    )
                {
                    return Err(invalid(
                        "mesh query frustum flag producer mismatch",
                        path("inputs"),
                    ));
                }
            }
            if !matches!(graph.resources[output as usize], CompiledResource { semantic_type: SemanticType::DrawStream, plan: ResourcePlan::DrawStream { mesh: m }, .. } if m == mesh)
            {
                return Err(invalid(
                    "mesh query output provenance mismatch",
                    path("outputs"),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_canonical_plan(graph: &CompiledGraph) -> Result<(), GraphError> {
    for (i, execution) in graph.executions.iter().enumerate() {
        if !execution_supported(&execution.executor.key) {
            return Err(error(
                "GRAPH_EXECUTION_UNSUPPORTED",
                "unsupported execution",
                format!("executions[{i}]"),
            ));
        }
    }
    if graph.schema_version != 2 {
        return Err(invalid(
            "compiled graph schema version must be 2",
            "schemaVersion",
        ));
    }

    let mut producers = vec![None; graph.resources.len()];
    let mut uses = vec![BTreeSet::new(); graph.resources.len()];
    for (i, execution) in graph.executions.iter().enumerate() {
        let contract = contract(&execution.executor.key).expect("supported executor has contract");
        if execution.executor.version != contract.version {
            return Err(invalid(
                "executor version does not match its contract",
                format!("executions[{i}].executor.version"),
            ));
        }
        for output in &execution.outputs {
            let producer = producers.get_mut(output.resource as usize).ok_or_else(|| {
                invalid(
                    "execution output is out of bounds",
                    format!("executions[{i}].outputs"),
                )
            })?;
            if producer.replace(i as u32).is_some() {
                return Err(invalid(
                    "resource has duplicate producers",
                    format!("executions[{i}].outputs"),
                ));
            }
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
        }
        validate_compute_execution(graph, i, execution)?;
        validate_fullscreen_execution(graph, i, execution, contract)?;
        for resource in referenced {
            uses.get_mut(resource as usize)
                .ok_or_else(|| {
                    invalid(
                        "execution resource is out of bounds",
                        format!("executions[{i}]"),
                    )
                })?
                .insert(i as u32);
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
            }
        }
    }

    for (fi, family) in graph.texture_families.iter().enumerate() {
        let TextureFamilySource::AuthoredTexture {
            resource: source,
            residency,
            descriptor,
        } = &family.source;
        let source_resource = graph.resources.get(*source as usize).ok_or_else(|| {
            invalid(
                "texture source resource is out of bounds",
                format!("textureFamilies[{fi}].source.resource"),
            )
        })?;
        if source_resource.semantic_type != SemanticType::Texture
            || source_resource.producer_execution.is_some()
            || !matches!(&source_resource.plan, ResourcePlan::TextureSource { family: f, residency: r, descriptor: d }
                if *f == family.id && r == residency && d == descriptor)
        {
            return Err(invalid(
                "texture family source is not canonical",
                format!("textureFamilies[{fi}].source"),
            ));
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
                *source
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
        let expected_aliasable = *residency == TextureResidency::Transient && all_initialized;
        if family.aliasable != expected_aliasable {
            return Err(invalid(
                "texture family aliasability is not canonical",
                format!("textureFamilies[{fi}].aliasable"),
            ));
        }
    }
    Ok(())
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
    let mut query = None;
    let mut executions = Vec::with_capacity(graph.executions.len());
    for (i, execution) in graph.executions.iter().enumerate() {
        let path = format!("executions[{i}]");
        match execution.executor.key.as_str() {
            "mesh_query" => {
                let NormalizedParameters::MeshQuery {
                    visible_predicate,
                    frustum_culled_predicate,
                } = &execution.parameters
                else {
                    return Err(invalid("mesh query parameters mismatch", &path));
                };
                let key = MeshQueryRuntimeKey {
                    visible: *visible_predicate,
                    frustum_culled: *frustum_culled_predicate,
                };
                if query.replace(key).is_some() {
                    return Err(error(
                        "GRAPH_EXECUTION_UNSUPPORTED",
                        "multiple draw stream queries",
                        &path,
                    ));
                }
            }
            "pipeline_registry" | "pipeline" => {}
            _ if contract(&execution.executor.key)
                .is_some_and(|contract| contract.fullscreen_policy.is_some()) => {}
            "frustum_cull" => {}
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
    let query = query.ok_or_else(|| {
        error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "one mesh query is required",
            "executions",
        )
    })?;

    for (i, execution) in graph.executions.iter().enumerate() {
        if execution.executor.key != "pipeline_registry" {
            continue;
        }
        if !matches!(execution.parameters, NormalizedParameters::PipelineRegistry) {
            return Err(invalid(
                "pipeline registry parameters mismatch",
                format!("executions[{i}].parameters"),
            ));
        }
        if !matches!(execution.kind, ExecutionKind::CpuPreparation) {
            return Err(invalid(
                "pipeline registry kind mismatch",
                format!("executions[{i}].kind"),
            ));
        }
        let [CompiledSocketInput {
            socket: input_socket,
            resource: pipeline_indices,
        }] = execution.inputs.as_slice()
        else {
            return Err(invalid(
                "pipeline registry input shape mismatch",
                format!("executions[{i}].inputs"),
            ));
        };
        if input_socket != "pipelineIndices" {
            return Err(invalid(
                "pipeline registry input socket mismatch",
                format!("executions[{i}].inputs"),
            ));
        }
        let [CompiledSocketOutput {
            socket: output_socket,
            resource: activation,
        }] = execution.outputs.as_slice()
        else {
            return Err(invalid(
                "pipeline registry output shape mismatch",
                format!("executions[{i}].outputs"),
            ));
        };
        if output_socket != "activation" {
            return Err(invalid(
                "pipeline registry output socket mismatch",
                format!("executions[{i}].outputs"),
            ));
        }
        if !matches!(
            execution.accesses.as_slice(),
            [CompiledAccess { socket, resource, mode: AccessMode::SemanticRead }]
                if socket == "pipelineIndices" && resource == pipeline_indices
        ) {
            return Err(invalid(
                "pipeline registry access mismatch",
                format!("executions[{i}].accesses"),
            ));
        }
        let indices_resource =
            graph
                .resources
                .get(*pipeline_indices as usize)
                .ok_or_else(|| {
                    invalid(
                        "pipeline index stream is out of bounds",
                        format!("executions[{i}].inputs"),
                    )
                })?;
        let ResourcePlan::PipelineIndexStream { mesh } = indices_resource.plan else {
            return Err(invalid(
                "pipeline registry input is not a pipeline index stream",
                format!("resources[{pipeline_indices}].plan"),
            ));
        };
        if indices_resource.semantic_type != SemanticType::PipelineIndexStream
            || !graph.resources.get(mesh as usize).is_some_and(|resource| {
                resource.semantic_type == SemanticType::MeshData
                    && matches!(resource.plan, ResourcePlan::MeshData)
            })
        {
            return Err(invalid(
                "pipeline index stream mesh provenance is invalid",
                format!("resources[{pipeline_indices}].plan"),
            ));
        }
        let activation_resource = graph.resources.get(*activation as usize).ok_or_else(|| {
            invalid(
                "pipeline activation is out of bounds",
                format!("executions[{i}].outputs"),
            )
        })?;
        if activation_resource.semantic_type != SemanticType::PipelineActivation
            || !matches!(activation_resource.plan, ResourcePlan::PipelineActivation { pipeline_indices: source } if source == *pipeline_indices)
            || activation_resource.producer_execution != Some(i as u32)
        {
            return Err(invalid(
                "pipeline activation provenance is invalid",
                format!("resources[{activation}].plan"),
            ));
        }
    }

    for (i, execution) in graph.executions.iter().enumerate() {
        if execution.executor.key != "pipeline" {
            continue;
        }
        let NormalizedParameters::Pipeline {
            pipeline,
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
        if !valid_pipeline_name(pipeline)
            || !clear_depth.is_finite()
            || !(0.0..=1.0).contains(clear_depth)
            || clear_color.iter().any(|value| !value.is_finite())
        {
            return Err(invalid(
                "pipeline parameters are invalid",
                format!("executions[{i}].parameters"),
            ));
        }
        let ExecutionKind::Render {
            color_attachments,
            depth_stencil: Some(depth_attachment),
        } = &execution.kind
        else {
            return Err(invalid(
                "pipeline render kind mismatch",
                format!("executions[{i}].kind"),
            ));
        };
        let [color_attachment] = color_attachments.as_slice() else {
            return Err(invalid(
                "pipeline color attachment shape mismatch",
                format!("executions[{i}].kind"),
            ));
        };
        let [mesh_input, draws_input, activation_input, color_input, depth_input] =
            execution.inputs.as_slice()
        else {
            return Err(invalid(
                "pipeline input shape mismatch",
                format!("executions[{i}].inputs"),
            ));
        };
        if [
            mesh_input.socket.as_str(),
            draws_input.socket.as_str(),
            activation_input.socket.as_str(),
            color_input.socket.as_str(),
            depth_input.socket.as_str(),
        ] != ["mesh", "draws", "activation", "colorTarget", "depthTarget"]
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
        let color_version = match graph
            .resources
            .get(color_output.resource as usize)
            .map(|r| &r.plan)
        {
            Some(ResourcePlan::Texture { version, .. }) => *version,
            _ => {
                return Err(invalid(
                    "pipeline color output kind is invalid",
                    format!("resources[{}].plan", color_output.resource),
                ))
            }
        };
        let depth_version = match graph
            .resources
            .get(depth_output.resource as usize)
            .map(|r| &r.plan)
        {
            Some(ResourcePlan::Texture { version, .. }) => *version,
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
        let [mesh_access, draws_access, activation_access, color_access, depth_access] =
            execution.accesses.as_slice()
        else {
            return Err(invalid(
                "pipeline access shape mismatch",
                format!("executions[{i}].accesses"),
            ));
        };
        if mesh_access.socket != "mesh"
            || mesh_access.resource != mesh_input.resource
            || !matches!(mesh_access.mode, AccessMode::SemanticRead)
            || draws_access.socket != "draws"
            || draws_access.resource != draws_input.resource
            || !matches!(draws_access.mode, AccessMode::IndirectRead)
            || activation_access.socket != "activation"
            || activation_access.resource != activation_input.resource
            || !matches!(activation_access.mode, AccessMode::SemanticRead)
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
                AccessMode::ColorAttachment { location: 0, load, store: StoreOp::Store, full_overwrite }
                    if load == color_attachment.load && full_overwrite == color_clear
            )
            || color_attachment.location != 0
            || color_attachment.store != StoreOp::Store
            || depth_access.socket != "depth"
            || depth_access.resource != depth_output.resource
            || !matches!(
                depth_access.mode,
                AccessMode::DepthAttachment { load, store: StoreOp::Store, full_overwrite }
                    if load == depth_attachment.load && full_overwrite == depth_clear
            )
            || depth_attachment.store != StoreOp::Store
        {
            return Err(invalid(
                "pipeline attachment accesses mismatch",
                format!("executions[{i}].accesses"),
            ));
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
        let draws_mesh = match graph.resources.get(draws_input.resource as usize) {
            Some(CompiledResource {
                semantic_type: SemanticType::DrawStream,
                plan: ResourcePlan::DrawStream { mesh },
                ..
            }) => *mesh,
            _ => {
                return Err(invalid(
                    "pipeline draw stream is invalid",
                    format!("resources[{}].plan", draws_input.resource),
                ))
            }
        };
        let activation_resource = graph
            .resources
            .get(activation_input.resource as usize)
            .ok_or_else(|| {
                invalid(
                    "pipeline activation is out of bounds",
                    format!("executions[{i}].inputs"),
                )
            })?;
        let indices = match activation_resource.plan {
            ResourcePlan::PipelineActivation { pipeline_indices }
                if activation_resource.semantic_type == SemanticType::PipelineActivation =>
            {
                pipeline_indices
            }
            _ => {
                return Err(invalid(
                    "pipeline activation is invalid",
                    format!("resources[{}].plan", activation_input.resource),
                ))
            }
        };
        let activation_mesh = match graph.resources.get(indices as usize) {
            Some(CompiledResource {
                semantic_type: SemanticType::PipelineIndexStream,
                plan: ResourcePlan::PipelineIndexStream { mesh },
                ..
            }) => *mesh,
            _ => {
                return Err(invalid(
                    "pipeline activation index stream is invalid",
                    format!("resources[{indices}].plan"),
                ))
            }
        };
        let valid_activation_producer = activation_resource
            .producer_execution
            .and_then(|producer| graph.executions.get(producer as usize))
            .is_some_and(|producer| producer.executor.key == "pipeline_registry");
        if draws_mesh != mesh_input.resource
            || activation_mesh != mesh_input.resource
            || !valid_activation_producer
        {
            return Err(invalid(
                "pipeline mesh provenance disagrees",
                format!("executions[{i}].inputs"),
            ));
        }
        if !has_exact_producer(graph, i, draws_input.resource, "mesh_query", "draws") {
            return Err(invalid(
                "pipeline draw stream producer mismatch",
                format!("executions[{i}].inputs"),
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
            || !super::compiler::is_single_view_d2(color_descriptor)
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
            || !super::compiler::is_single_view_d2(depth_descriptor)
        {
            return Err(invalid(
                "pipeline depth attachment descriptor is invalid",
                format!("executions[{i}].inputs"),
            ));
        }
        if color_descriptor.extent != depth_descriptor.extent {
            return Err(invalid(
                "pipeline attachment extents mismatch",
                format!("executions[{i}].inputs"),
            ));
        }
    }

    let frame_out = &graph.executions[frame_out_index];
    let NormalizedParameters::FrameOut {
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
    let TextureFamilySource::AuthoredTexture { descriptor, .. } = &family.source;
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
                    || descriptor.sample_count != 1
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
                let TextureFamilySource::AuthoredTexture {
                    descriptor,
                    residency,
                    ..
                } = &family.source;
                expected_usage.extend(family.usage.iter().copied());
                let kind_valid = match slot.kind {
                    AllocationKind::AliasedTransient => {
                        *residency == TextureResidency::Transient && family.aliasable
                    }
                    AllocationKind::DedicatedTransient => {
                        slot.occupants.len() == 1
                            && *residency == TextureResidency::Transient
                            && !family.aliasable
                    }
                    AllocationKind::Persistent => {
                        slot.occupants.len() == 1
                            && *residency == TextureResidency::Persistent
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
            query,
        },
        executions,
        surface,
    })
}

pub fn validate_activatable(graph: &CompiledGraph) -> Result<(), GraphError> {
    let surface = RuntimeSurfaceContract {
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: 1,
        height: 1,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: Vec::new(),
    };
    prepare_runtime_plan(graph, surface, None).map(|_| ())
}
