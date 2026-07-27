use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedExtentV2 {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTextureDescriptorV2 {
    pub dimension: wgpu::TextureDimension,
    pub format: wgpu::TextureFormat,
    pub extent: ResolvedExtentV2,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub usage: wgpu::TextureUsages,
    pub view_formats: Vec<wgpu::TextureFormat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSurfaceContractV2 {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub usage: wgpu::TextureUsages,
    pub view_formats: Vec<wgpu::TextureFormat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationSlotV2 {
    pub kind: AllocationKindV2,
    pub descriptor: RuntimeTextureDescriptorV2,
    pub occupants: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationClassV2 {
    pub key: TextureCompatibilityKeyV2,
    pub slots: Vec<RuntimeAllocationSlotV2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshQueryRuntimeKeyV2 {
    pub visible: TriStatePredicate,
    pub frustum_culled: TriStatePredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeExecutionV2 {
    pub execution: u32,
    pub executor: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAllocationPlanV2 {
    pub classes: Vec<RuntimeAllocationClassV2>,
    pub resource_allocations: Vec<Option<AllocationRefV2>>,
    pub surface_family: u32,
    pub surface_resource: u32,
    pub query: MeshQueryRuntimeKeyV2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePlanV2 {
    pub allocations: RuntimeAllocationPlanV2,
    pub executions: Vec<RuntimeExecutionV2>,
    pub surface: RuntimeSurfaceContractV2,
}

fn error(code: &'static str, message: impl Into<String>, path: impl Into<String>) -> GraphError {
    GraphError::at(code, message, path)
}

pub const fn texture_dimension_v2(value: TextureDimensionV2) -> wgpu::TextureDimension {
    match value {
        TextureDimensionV2::D1 => wgpu::TextureDimension::D1,
        TextureDimensionV2::D2 => wgpu::TextureDimension::D2,
        TextureDimensionV2::D3 => wgpu::TextureDimension::D3,
    }
}

pub const fn texture_format_v2(value: TextureFormatV2) -> wgpu::TextureFormat {
    match value {
        TextureFormatV2::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        TextureFormatV2::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormatV2::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
        TextureFormatV2::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        TextureFormatV2::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        TextureFormatV2::R32Float => wgpu::TextureFormat::R32Float,
        TextureFormatV2::Depth32Float => wgpu::TextureFormat::Depth32Float,
    }
}

pub const fn texture_usage_v2(value: TextureUsageV2) -> wgpu::TextureUsages {
    match value {
        TextureUsageV2::Sampled => wgpu::TextureUsages::TEXTURE_BINDING,
        TextureUsageV2::Storage => wgpu::TextureUsages::STORAGE_BINDING,
        TextureUsageV2::CopySrc => wgpu::TextureUsages::COPY_SRC,
        TextureUsageV2::CopyDst => wgpu::TextureUsages::COPY_DST,
        TextureUsageV2::ColorAttachment | TextureUsageV2::DepthAttachment => {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        }
    }
}

pub fn texture_usages_v2(values: &[TextureUsageV2]) -> wgpu::TextureUsages {
    values
        .iter()
        .fold(wgpu::TextureUsages::empty(), |usage, value| {
            usage | texture_usage_v2(*value)
        })
}

fn scaled(value: u32, ratio: RatioV2, path: &str) -> Result<u32, GraphError> {
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

pub fn resolve_extent_v2(
    extent: &NormalizedTextureExtentV2,
    surface: [u32; 2],
) -> Result<ResolvedExtentV2, GraphError> {
    let resolved = match extent {
        NormalizedTextureExtentV2::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => ResolvedExtentV2 {
            width: *width,
            height: *height,
            depth_or_array_layers: *depth_or_array_layers,
        },
        NormalizedTextureExtentV2::SurfaceRelative {
            width,
            height,
            depth_or_array_layers,
        } => ResolvedExtentV2 {
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

pub fn resolved_mip_level_count_v2(extent: ResolvedExtentV2) -> u32 {
    32 - extent
        .width
        .max(extent.height)
        .max(extent.depth_or_array_layers)
        .leading_zeros()
}

fn validate_limits(
    dimension: TextureDimensionV2,
    extent: ResolvedExtentV2,
    mip_count: u32,
    limits: Option<&wgpu::Limits>,
    path: &str,
) -> Result<(), GraphError> {
    let max_mips = resolved_mip_level_count_v2(extent);
    if mip_count == 0 || mip_count > max_mips {
        return Err(error(
            "GRAPH_RESOURCE_LIMIT",
            "invalid mip level count",
            path,
        ));
    }
    if let Some(l) = limits {
        let valid = match dimension {
            TextureDimensionV2::D1 => extent.width <= l.max_texture_dimension_1d,
            TextureDimensionV2::D2 => {
                extent.width <= l.max_texture_dimension_2d
                    && extent.height <= l.max_texture_dimension_2d
                    && extent.depth_or_array_layers <= l.max_texture_array_layers
            }
            TextureDimensionV2::D3 => {
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

pub fn runtime_texture_descriptor_v2(
    key: &TextureCompatibilityKeyV2,
    usage: &[TextureUsageV2],
    surface: [u32; 2],
    limits: Option<&wgpu::Limits>,
) -> Result<RuntimeTextureDescriptorV2, GraphError> {
    let extent = resolve_extent_v2(&key.extent, surface)?;
    validate_limits(
        key.dimension,
        extent,
        key.mip_level_count,
        limits,
        "allocationClasses.key",
    )?;
    Ok(RuntimeTextureDescriptorV2 {
        dimension: texture_dimension_v2(key.dimension),
        format: texture_format_v2(key.format),
        extent,
        mip_level_count: key.mip_level_count,
        sample_count: key.sample_count,
        usage: texture_usages_v2(usage),
        view_formats: key
            .view_formats
            .iter()
            .copied()
            .map(texture_format_v2)
            .collect(),
    })
}

fn invalid(message: impl Into<String>, path: impl Into<String>) -> GraphError {
    error("GRAPH_RUNTIME_PLAN_INVALID", message, path)
}

pub fn prepare_runtime_plan_v2(
    graph: &CompiledGraphV2,
    surface: RuntimeSurfaceContractV2,
    limits: Option<&wgpu::Limits>,
) -> Result<RuntimePlanV2, GraphError> {
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

    let mut present_count = 0;
    let mut query = None;
    let mut executions = Vec::with_capacity(graph.executions.len());
    for (i, execution) in graph.executions.iter().enumerate() {
        let path = format!("executions[{i}]");
        match execution.executor.key.as_str() {
            "mesh_query" => {
                let NormalizedParametersV2::MeshQuery { filters } = &execution.parameters else {
                    return Err(invalid("mesh query parameters mismatch", &path));
                };
                let key = MeshQueryRuntimeKeyV2 {
                    visible: filters[0].predicate,
                    frustum_culled: filters[1].predicate,
                };
                if query.replace(key).is_some() {
                    return Err(error(
                        "GRAPH_EXECUTION_UNSUPPORTED",
                        "multiple draw stream queries",
                        &path,
                    ));
                }
            }
            "legacy_forward" => {}
            "fullscreen_copy" | "tone_map" | "bloom_extract" | "bloom_blur" | "bloom_composite"
            | "luminance_edge" => {}
            "frustum_cull" => {}
            "present" => present_count += 1,
            _ => {
                return Err(error(
                    "GRAPH_EXECUTION_UNSUPPORTED",
                    "unsupported execution",
                    &path,
                ))
            }
        }
        executions.push(RuntimeExecutionV2 {
            execution: u32::try_from(i).map_err(|_| invalid("execution index overflow", &path))?,
            executor: execution.executor.key.clone(),
        });
    }
    if present_count != 1 {
        return Err(error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "exactly one present is required",
            "executions",
        ));
    }
    let query = query.ok_or_else(|| {
        error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "one mesh query is required",
            "executions",
        )
    })?;

    let mut surface_pair = None;
    let mut resource_allocations = vec![None; graph.resources.len()];
    for (fi, family) in graph.texture_families.iter().enumerate() {
        if family.id as usize != fi {
            return Err(invalid(
                "texture family id does not match index",
                format!("textureFamilies[{fi}].id"),
            ));
        }
        match &family.source {
            TextureFamilySourceV2::ImportedSurface { resource } => {
                if family.allocation.is_some()
                    || surface_pair.replace((family.id, *resource)).is_some()
                {
                    return Err(invalid(
                        "invalid imported surface allocation",
                        format!("textureFamilies[{fi}]"),
                    ));
                }
            }
            TextureFamilySourceV2::AuthoredTexture {
                residency,
                descriptor,
                ..
            } => {
                if !matches!(
                    residency,
                    TextureResidencyV2::Transient | TextureResidencyV2::Persistent
                ) || descriptor.dimension != TextureDimensionV2::D2
                    || descriptor.mip_level_count != 1
                    || descriptor.sample_count != 1
                    || !matches!(
                        descriptor.extent,
                        NormalizedTextureExtentV2::Absolute {
                            depth_or_array_layers: 1,
                            ..
                        } | NormalizedTextureExtentV2::SurfaceRelative {
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
            let ResourcePlanV2::Texture {
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
    let (surface_family, surface_resource) = surface_pair
        .ok_or_else(|| invalid("missing imported surface family", "textureFamilies"))?;

    // Validate the resource-to-family direction as well; compiled plans are public and may be
    // cloned and modified by callers.
    for (ri, resource) in graph.resources.iter().enumerate() {
        if let ResourcePlanV2::Texture {
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
            let allocation = AllocationRefV2 {
                class: ci as u32,
                slot: si as u32,
            };
            for &family_id in &slot.occupants {
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
                let TextureFamilySourceV2::AuthoredTexture { descriptor, .. } = &family.source
                else {
                    return Err(invalid(
                        "imported family occupies a slot",
                        format!("allocationClasses[{ci}].slots[{si}]"),
                    ));
                };
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
            slots.push(RuntimeAllocationSlotV2 {
                kind: slot.kind,
                descriptor: runtime_texture_descriptor_v2(
                    &class.key,
                    &slot.usage,
                    [surface.width, surface.height],
                    limits,
                )?,
                occupants: slot.occupants.clone(),
            });
        }
        classes.push(RuntimeAllocationClassV2 {
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
    let imported = graph
        .resources
        .get(surface_resource as usize)
        .ok_or_else(|| invalid("surface resource is out of bounds", "textureFamilies"))?;
    if !matches!(imported.plan, ResourcePlanV2::SurfaceTarget { family } if family == surface_family)
    {
        return Err(invalid(
            "surface source resource mismatch",
            format!("resources[{surface_resource}]"),
        ));
    }
    // The imported family may only flow through texture versions and the single present.
    for (ri, resource) in graph.resources.iter().enumerate() {
        if let ResourcePlanV2::Texture {
            family, allocation, ..
        } = resource.plan
        {
            if family == surface_family && allocation.is_some() {
                return Err(invalid(
                    "surface texture has an allocation",
                    format!("resources[{ri}].plan"),
                ));
            }
        }
    }
    let present = graph
        .executions
        .iter()
        .find(|execution| execution.executor.key == "present")
        .ok_or_else(|| invalid("present execution disappeared", "executions"))?;
    let ExecutionKindV2::Present { surface: presented } = present.kind else {
        return Err(invalid("present execution kind mismatch", "executions"));
    };
    let presented_resource = graph.resources.get(presented as usize).ok_or_else(|| {
        invalid(
            "present resource is out of bounds",
            "executions.present.surface",
        )
    })?;
    if !matches!(presented_resource.plan, ResourcePlanV2::Texture { family, .. } if family == surface_family)
    {
        return Err(error(
            "GRAPH_SURFACE_INCOMPATIBLE",
            "present does not resolve to the imported surface",
            "executions.present.surface",
        ));
    }

    Ok(RuntimePlanV2 {
        allocations: RuntimeAllocationPlanV2 {
            classes,
            resource_allocations,
            surface_family,
            surface_resource,
            query,
        },
        executions,
        surface,
    })
}

pub fn validate_activatable_v2(graph: &CompiledGraphV2) -> Result<(), GraphError> {
    let surface = RuntimeSurfaceContractV2 {
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: 1,
        height: 1,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: Vec::new(),
    };
    prepare_runtime_plan_v2(graph, surface, None).map(|_| ())
}
