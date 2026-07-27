use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use serde::Serialize;

use super::schema::*;
use super::{GraphError, MAX_JSON_BYTES, MAX_OUTPUTS, MAX_PASSES, MAX_RESOURCES, MAX_USES};

pub enum ExecutorResolution<'a> {
    Found(&'a dyn ExecutorContract),
    UnknownKey,
    UnsupportedVersion,
}
pub trait ExecutorRegistry {
    fn resolve(&self, executor: &ExecutorRef) -> ExecutorResolution<'_>;
}
pub trait ExecutorContract {
    fn inherently_observable(&self) -> bool;
    fn normalize_parameters(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<NormalizedParameters, String>;
    fn validate_bindings(
        &self,
        pass: &Pass,
        resources: &HashMap<ResourceRef, &Resource>,
    ) -> Result<(), String>;
}
pub struct SceneForwardExecutors;
static SCENE_FORWARD: SceneForward = SceneForward;
struct SceneForward;
impl ExecutorRegistry for SceneForwardExecutors {
    fn resolve(&self, e: &ExecutorRef) -> ExecutorResolution<'_> {
        if e.key != "scene_forward" {
            ExecutorResolution::UnknownKey
        } else if e.version != 1 {
            ExecutorResolution::UnsupportedVersion
        } else {
            ExecutorResolution::Found(&SCENE_FORWARD)
        }
    }
}
impl ExecutorContract for SceneForward {
    fn inherently_observable(&self) -> bool {
        false
    }
    fn normalize_parameters(
        &self,
        value: &serde_json::Value,
    ) -> Result<NormalizedParameters, String> {
        if value == &serde_json::json!({}) {
            Ok(NormalizedParameters::SceneForward)
        } else {
            Err("requires parameters {}".into())
        }
    }
    fn validate_bindings(
        &self,
        p: &Pass,
        r: &HashMap<ResourceRef, &Resource>,
    ) -> Result<(), String> {
        if !p.reads.is_empty() || p.writes.len() != 2 {
            return Err(
                "requires parameters {}, no reads, and exactly color and depth bindings".into(),
            );
        }
        let color = p.writes.iter().find(|x| x.binding == "color");
        let depth = p.writes.iter().find(|x| x.binding == "depth");
        let (Some(c), Some(d)) = (color, depth) else {
            return Err("requires bindings named color and depth".into());
        };
        if !matches!(c.access, WriteAccess::ColorAttachment { location: 0, .. })
            || !matches!(d.access, WriteAccess::DepthAttachment { .. })
        {
            return Err("color must be attachment location 0 and depth a depth attachment".into());
        }
        let (c, d) = (r[&c.resource], r[&d.resource]);
        if d.texture.format != Format::Depth32Float
            || c.texture.format == Format::Depth32Float
            || c.texture.extent != d.texture.extent
            || c.texture.sample_count != d.texture.sample_count
        {
            return Err(
                "attachments must match extent/sample count and depth must be depth32_float".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedParameters {
    SceneForward,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledRead {
    pub binding: String,
    pub resource: u32,
    pub access: ReadAccess,
}
#[derive(Debug, Clone, Serialize)]
pub struct CompiledWrite {
    pub binding: String,
    pub resource: u32,
    pub access: WriteAccess,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledPass {
    pub id: String,
    pub original_index: u32,
    pub executor: ExecutorRef,
    pub parameters: NormalizedParameters,
    pub reads: Vec<CompiledRead>,
    pub writes: Vec<CompiledWrite>,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Lifetime {
    pub first_use: u32,
    pub last_use: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum TextureUsage {
    Sampled,
    Storage,
    CopySrc,
    CopyDst,
    ColorAttachment,
    DepthAttachment,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct TextureAllocationKey {
    pub descriptor: TextureDescriptor,
    pub usage: Vec<TextureUsage>,
    #[serde(rename = "viewFormats")]
    pub view_formats: Vec<Format>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CompiledResource {
    pub original_index: u32,
    #[serde(rename = "ref")]
    pub resource_ref: ResourceRef,
    pub residency: Residency,
    pub descriptor: TextureDescriptor,
    pub writer: Option<u32>,
    pub lifetime: Lifetime,
    pub allocation: Option<TransientAllocation>,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TransientAllocation {
    pub class: u32,
    pub slot: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct CompiledOutput {
    pub name: String,
    pub resource: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct AllocationClass {
    pub key: TextureAllocationKey,
    pub slot_count: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct CompiledGraph {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    pub passes: Vec<CompiledPass>,
    pub resources: Vec<CompiledResource>,
    pub outputs: Vec<CompiledOutput>,
    pub allocation_classes: Vec<AllocationClass>,
    pub culled_pass_count: u32,
    pub culled_resource_count: u32,
    pub transient_slot_count: u32,
}
impl CompiledGraph {
    pub fn summary(&self, id: [u32; 2]) -> serde_json::Value {
        serde_json::json!({"compiledId":id,"graphId":self.graph_id,"revision":self.revision,"schemaVersion":self.schema_version,"passCount":self.passes.len(),"resourceCount":self.resources.len(),"culledPassCount":self.culled_pass_count,"culledResourceCount":self.culled_resource_count,"transientSlotCount":self.transient_slot_count})
    }
}

fn fail(code: &'static str, msg: impl Into<String>, path: impl Into<String>) -> GraphError {
    GraphError::at(code, msg, path)
}
fn norm(mut d: TextureDescriptor) -> TextureDescriptor {
    fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let n = a % b;
            a = b;
            b = n
        }
        a
    }
    if let Extent::SurfaceRelative { width, height, .. } = &mut d.extent {
        let g = gcd(width.numerator, width.denominator);
        width.numerator /= g;
        width.denominator /= g;
        let g = gcd(height.numerator, height.denominator);
        height.numerator /= g;
        height.denominator /= g
    }
    d
}
fn usage_read(a: ReadAccess) -> TextureUsage {
    match a {
        ReadAccess::Sampled => TextureUsage::Sampled,
        ReadAccess::Storage => TextureUsage::Storage,
        ReadAccess::CopySrc => TextureUsage::CopySrc,
    }
}
fn usage_write(a: &WriteAccess) -> TextureUsage {
    match a {
        WriteAccess::Storage => TextureUsage::Storage,
        WriteAccess::CopyDst => TextureUsage::CopyDst,
        WriteAccess::ColorAttachment { .. } => TextureUsage::ColorAttachment,
        WriteAccess::DepthAttachment { .. } => TextureUsage::DepthAttachment,
    }
}

pub fn parse_and_compile(bytes: &[u8]) -> Result<CompiledGraph, GraphError> {
    compile_with(bytes, &SceneForwardExecutors)
}
pub fn compile_with(
    bytes: &[u8],
    executors: &dyn ExecutorRegistry,
) -> Result<CompiledGraph, GraphError> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(GraphError::new(
            "GRAPH_PAYLOAD_TOO_LARGE",
            "graph payload exceeds 1 MiB",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| GraphError::new("GRAPH_ENCODING_INVALID", "graph payload is not UTF-8"))?;
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| GraphError::new("GRAPH_JSON_INVALID", e.to_string()))?;
    let version = value.get("schemaVersion").and_then(|v| v.as_u64());
    if version != Some(1) {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be 1",
        ));
    }
    let g: GraphV1 = serde_json::from_str(text)
        .map_err(|e| GraphError::new("GRAPH_JSON_INVALID", e.to_string()))?;
    compile(g, executors)
}
pub fn compile(
    mut g: GraphV1,
    executors: &dyn ExecutorRegistry,
) -> Result<CompiledGraph, GraphError> {
    if g.schema_version != 1 {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be 1",
        ));
    }
    if g.resources.len() > MAX_RESOURCES
        || g.passes.len() > MAX_PASSES
        || g.outputs.len() > MAX_OUTPUTS
        || g.passes
            .iter()
            .map(|p| p.reads.len() + p.writes.len())
            .sum::<usize>()
            > MAX_USES
    {
        return Err(GraphError::new(
            "GRAPH_LIMIT_EXCEEDED",
            "graph limit exceeded",
        ));
    }
    let identifiers = std::iter::once(&g.graph_id)
        .chain(g.resources.iter().map(|resource| &resource.id))
        .chain(g.passes.iter().flat_map(|pass| {
            std::iter::once(&pass.id)
                .chain(std::iter::once(&pass.executor.key))
                .chain(
                    pass.reads
                        .iter()
                        .flat_map(|binding| [&binding.binding, &binding.resource.id]),
                )
                .chain(
                    pass.writes
                        .iter()
                        .flat_map(|binding| [&binding.binding, &binding.resource.id]),
                )
        }))
        .chain(
            g.outputs
                .iter()
                .flat_map(|output| [&output.name, &output.resource.id]),
        );
    if identifiers
        .into_iter()
        .any(|identifier| identifier.len() > 64)
    {
        return Err(GraphError::new(
            "GRAPH_LIMIT_EXCEEDED",
            "identifier exceeds 64 UTF-8 bytes",
        ));
    }
    if !identifier(&g.graph_id) || g.revision == 0 {
        return Err(fail(
            "GRAPH_INVALID_ID",
            "invalid graphId or revision",
            "graphId",
        ));
    }
    let mut resource_ids = HashSet::new();
    let mut external = HashSet::new();
    for (i, r) in g.resources.iter().enumerate() {
        let rr = ResourceRef {
            id: r.id.clone(),
            version: r.version,
        };
        if !identifier(&r.id) {
            return Err(fail(
                "GRAPH_INVALID_ID",
                "invalid resource id",
                format!("resources[{i}].id"),
            ));
        }
        if !resource_ids.insert(rr) {
            return Err(fail(
                "GRAPH_DUPLICATE_ID",
                "duplicate resource id/version",
                format!("resources[{i}]"),
            ));
        }
        if let Residency::External { source } = r.residency {
            if !external.insert(source) {
                return Err(fail(
                    "GRAPH_DUPLICATE_ID",
                    "duplicate external source",
                    format!("resources[{i}].residency"),
                ));
            }
        }
    }
    let mut pass_ids = HashSet::new();
    for (pi, p) in g.passes.iter().enumerate() {
        if !identifier(&p.id) || !identifier(&p.executor.key) {
            return Err(fail(
                "GRAPH_INVALID_ID",
                "invalid pass or executor id",
                format!("passes[{pi}]"),
            ));
        }
        if !pass_ids.insert(&p.id) {
            return Err(fail(
                "GRAPH_DUPLICATE_ID",
                "duplicate pass id",
                format!("passes[{pi}].id"),
            ));
        }
    }
    let mut output_names = HashSet::new();
    for (i, o) in g.outputs.iter().enumerate() {
        if !identifier(&o.name) || !output_names.insert(&o.name) {
            return Err(fail(
                if identifier(&o.name) {
                    "GRAPH_DUPLICATE_ID"
                } else {
                    "GRAPH_INVALID_ID"
                },
                "invalid or duplicate output",
                format!("outputs[{i}]"),
            ));
        }
    }
    for (i, resource) in g.resources.iter_mut().enumerate() {
        let valid_ratio = match &resource.texture.extent {
            Extent::SurfaceRelative { width, height, .. } => {
                width.numerator != 0
                    && width.denominator != 0
                    && height.numerator != 0
                    && height.denominator != 0
            }
            Extent::Absolute { .. } => true,
        };
        if !valid_ratio {
            return Err(fail(
                "GRAPH_ILLEGAL_ACCESS",
                "invalid extent",
                format!("resources[{i}].texture.extent"),
            ));
        }
        resource.texture = norm(resource.texture.clone());
    }
    for (i, r) in g.resources.iter().enumerate() {
        if r.texture.mip_level_count != 1 || r.texture.sample_count != 1 {
            return Err(fail(
                "GRAPH_UNSUPPORTED_FEATURE",
                "V1 mipLevels and sampleCount must be 1",
                format!("resources[{i}].texture"),
            ));
        }
        let valid = match &r.texture.extent {
            Extent::Absolute {
                width,
                height,
                depth_or_array_layers,
            } => *width > 0 && *height > 0 && *depth_or_array_layers > 0,
            Extent::SurfaceRelative {
                width,
                height,
                depth_or_array_layers,
            } => {
                width.numerator > 0
                    && width.denominator > 0
                    && height.numerator > 0
                    && height.denominator > 0
                    && *depth_or_array_layers > 0
            }
        };
        if !valid {
            return Err(fail(
                "GRAPH_ILLEGAL_ACCESS",
                "invalid extent",
                format!("resources[{i}].texture.extent"),
            ));
        }
        match r.residency {
            Residency::External { .. } => {
                let surface = TextureDescriptor {
                    dimension: Dimension::D2,
                    format: Format::Surface,
                    extent: Extent::SurfaceRelative {
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
                    mip_level_count: 1,
                    sample_count: 1,
                };
                if r.texture != surface {
                    return Err(fail(
                        "GRAPH_ILLEGAL_ACCESS",
                        "external source must use the exact surface descriptor",
                        format!("resources[{i}]"),
                    ));
                }
            }
            Residency::Transient => {
                if r.texture.format == Format::Surface {
                    return Err(fail(
                        "GRAPH_ILLEGAL_ACCESS",
                        "transient cannot use surface format",
                        format!("resources[{i}]"),
                    ));
                }
            }
        }
        if matches!(r.texture.extent, Extent::SurfaceRelative { .. })
            && r.texture.dimension != Dimension::D2
        {
            return Err(fail(
                "GRAPH_ILLEGAL_ACCESS",
                "surface-relative extent requires d2",
                format!("resources[{i}].texture"),
            ));
        }
        if r.texture.format == Format::Depth32Float && r.texture.dimension != Dimension::D2 {
            return Err(fail(
                "GRAPH_ILLEGAL_ACCESS",
                "depth32_float requires d2",
                format!("resources[{i}].texture"),
            ));
        }
        if r.texture.dimension == Dimension::D1
            && !matches!(
                r.texture.extent,
                Extent::Absolute {
                    height: 1,
                    depth_or_array_layers: 1,
                    ..
                }
            )
        {
            return Err(fail(
                "GRAPH_ILLEGAL_ACCESS",
                "d1 textures require height and depthOrArrayLayers to be 1",
                format!("resources[{i}].texture.extent"),
            ));
        }
    }
    let map: HashMap<ResourceRef, &Resource> = g
        .resources
        .iter()
        .map(|r| {
            (
                ResourceRef {
                    id: r.id.clone(),
                    version: r.version,
                },
                r,
            )
        })
        .collect();
    for (pi, pass) in g.passes.iter().enumerate() {
        for resource_ref in pass
            .reads
            .iter()
            .map(|binding| &binding.resource)
            .chain(pass.writes.iter().map(|binding| &binding.resource))
        {
            if !identifier(&resource_ref.id) {
                return Err(fail(
                    "GRAPH_INVALID_ID",
                    "invalid resource reference id",
                    format!("passes[{pi}]"),
                ));
            }
            if !resource_ids.contains(resource_ref) {
                return Err(fail(
                    "GRAPH_UNKNOWN_RESOURCE",
                    "unknown resource reference",
                    format!("passes[{pi}]"),
                ));
            }
        }
    }
    for (i, output) in g.outputs.iter().enumerate() {
        if !identifier(&output.resource.id) {
            return Err(fail(
                "GRAPH_INVALID_ID",
                "invalid output resource id",
                format!("outputs[{i}].resource.id"),
            ));
        }
        if !resource_ids.contains(&output.resource) {
            return Err(fail(
                "GRAPH_UNKNOWN_RESOURCE",
                "unknown output resource",
                format!("outputs[{i}]"),
            ));
        }
    }
    let mut observable = vec![false; g.passes.len()];
    let mut parameters = Vec::with_capacity(g.passes.len());
    // Validate executor signatures and pass-local binding identity before building
    // the graph-wide writer map. Access legality is deliberately checked later.
    for (pi, p) in g.passes.iter().enumerate() {
        let contract = match executors.resolve(&p.executor) {
            ExecutorResolution::Found(contract) => contract,
            resolution => {
                return Err(GraphError::at(
                    if matches!(resolution, ExecutorResolution::UnsupportedVersion) {
                        "GRAPH_EXECUTOR_VERSION_UNSUPPORTED"
                    } else {
                        "GRAPH_UNKNOWN_EXECUTOR"
                    },
                    "executor is not registered",
                    format!("passes[{pi}].executor"),
                ))
            }
        };
        observable[pi] = contract.inherently_observable();
        parameters.push(contract.normalize_parameters(&p.parameters).map_err(|m| {
            GraphError::at(
                "GRAPH_PARAMETERS_INVALID",
                m,
                format!("passes[{pi}].parameters"),
            )
        })?);
        let mut names = HashSet::new();
        let mut refs = HashSet::new();
        let mut color_locations = HashSet::new();
        for b in &p.reads {
            if !identifier(&b.binding) {
                return Err(fail(
                    "GRAPH_INVALID_ID",
                    "invalid binding",
                    format!("passes[{pi}].reads"),
                ));
            }
            if !names.insert(&b.binding) || !refs.insert(&b.resource) {
                return Err(fail(
                    "GRAPH_BINDING_INVALID",
                    "duplicate binding or resource use",
                    format!("passes[{pi}]"),
                ));
            }
        }
        for b in &p.writes {
            if !identifier(&b.binding) {
                return Err(fail(
                    "GRAPH_INVALID_ID",
                    "invalid binding",
                    format!("passes[{pi}].writes"),
                ));
            }
            if !names.insert(&b.binding) || !refs.insert(&b.resource) {
                return Err(fail(
                    "GRAPH_BINDING_INVALID",
                    "duplicate binding or resource use",
                    format!("passes[{pi}]"),
                ));
            }
            if let WriteAccess::ColorAttachment { location, .. } = b.access {
                if !color_locations.insert(location) {
                    return Err(fail(
                        "GRAPH_BINDING_INVALID",
                        "duplicate color location",
                        format!("passes[{pi}].writes"),
                    ));
                }
            }
        }
        contract
            .validate_bindings(p, &map)
            .map_err(|m| GraphError::at("GRAPH_BINDING_INVALID", m, format!("passes[{pi}]")))?;
    }

    let mut writer: HashMap<ResourceRef, usize> = HashMap::new();
    for (pi, pass) in g.passes.iter().enumerate() {
        if pass.state != PassState::Enabled {
            continue;
        }
        for binding in &pass.writes {
            if writer.insert(binding.resource.clone(), pi).is_some() {
                return Err(fail(
                    "GRAPH_DUPLICATE_WRITER",
                    "resource has multiple enabled writers",
                    format!("passes[{pi}]"),
                ));
            }
        }
    }

    for (pi, p) in g.passes.iter().enumerate() {
        for b in &p.reads {
            let r = map[&b.resource];
            if (r.texture.format.depth() && matches!(b.access, ReadAccess::Storage))
                || (matches!(b.access, ReadAccess::Storage)
                    && !matches!(
                        r.texture.format,
                        Format::Rgba8Unorm | Format::Rgba16Float | Format::R32Float
                    ))
                || (r.texture.format == Format::Surface && !matches!(b.access, ReadAccess::CopySrc))
            {
                return Err(fail(
                    "GRAPH_ILLEGAL_ACCESS",
                    "format/access mismatch",
                    format!("passes[{pi}].reads"),
                ));
            }
        }
        let mut attachment_descriptor: Option<&TextureDescriptor> = None;
        for b in &p.writes {
            let r = map[&b.resource];
            let depth = r.texture.format.depth();
            let is_attachment = matches!(
                b.access,
                WriteAccess::ColorAttachment { .. } | WriteAccess::DepthAttachment { .. }
            );
            if (matches!(b.access, WriteAccess::DepthAttachment { .. }) && !depth)
                || (matches!(b.access, WriteAccess::ColorAttachment { .. }) && depth)
                || (matches!(b.access, WriteAccess::Storage) && depth)
                || (matches!(b.access, WriteAccess::Storage)
                    && !matches!(
                        r.texture.format,
                        Format::Rgba8Unorm | Format::Rgba16Float | Format::R32Float
                    ))
                || (is_attachment && r.texture.dimension != Dimension::D2)
            {
                return Err(fail(
                    "GRAPH_ILLEGAL_ACCESS",
                    "format/access mismatch",
                    format!("passes[{pi}].writes"),
                ));
            }
            if is_attachment {
                if let Some(first) = attachment_descriptor {
                    if first.dimension != r.texture.dimension
                        || first.extent != r.texture.extent
                        || first.sample_count != r.texture.sample_count
                    {
                        return Err(fail(
                            "GRAPH_ILLEGAL_ACCESS",
                            "attachments must have matching dimensions, extents, and sample counts",
                            format!("passes[{pi}].writes"),
                        ));
                    }
                } else {
                    attachment_descriptor = Some(&r.texture);
                }
            }
            match &b.access {
                WriteAccess::ColorAttachment { load, .. } => {
                    if let ColorLoad::Clear { value } = load {
                        if value.iter().any(|x| !x.is_finite()) {
                            return Err(fail(
                                "GRAPH_ILLEGAL_ACCESS",
                                "clear values must be finite",
                                format!("passes[{pi}]"),
                            ));
                        }
                    }
                }
                WriteAccess::DepthAttachment { load, .. } => {
                    if let DepthLoad::Clear { value } = load {
                        if !value.is_finite() || !(0.0..=1.0).contains(value) {
                            return Err(fail(
                                "GRAPH_ILLEGAL_ACCESS",
                                "depth clear must be finite and in [0,1]",
                                format!("passes[{pi}]"),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut roots = Vec::new();
    for (i, o) in g.outputs.iter().enumerate() {
        let Some(r) = map.get(&o.resource) else {
            return Err(fail(
                "GRAPH_UNKNOWN_RESOURCE",
                "unknown output resource",
                format!("outputs[{i}]"),
            ));
        };
        if writer.get(&o.resource).is_none() {
            return Err(fail(
                "GRAPH_UNINITIALIZED_RESOURCE",
                if matches!(r.residency, Residency::Transient) {
                    "transient output is uninitialized"
                } else {
                    "external output requires a live writer"
                },
                format!("outputs[{i}]"),
            ));
        }
        roots.push(o.resource.clone())
    }
    for (pi, p) in g.passes.iter().enumerate() {
        if p.state == PassState::Enabled {
            for b in &p.reads {
                if matches!(map[&b.resource].residency, Residency::Transient)
                    && !writer.contains_key(&b.resource)
                {
                    return Err(fail(
                        "GRAPH_UNINITIALIZED_RESOURCE",
                        "transient read is uninitialized",
                        format!("passes[{pi}]"),
                    ));
                }
            }
        }
    }
    for (pi, pass) in g.passes.iter().enumerate() {
        for binding in &pass.writes {
            if matches!(map[&binding.resource].residency, Residency::Transient)
                && matches!(
                    binding.access,
                    WriteAccess::ColorAttachment {
                        load: ColorLoad::Load,
                        ..
                    } | WriteAccess::DepthAttachment {
                        load: DepthLoad::Load,
                        ..
                    }
                )
            {
                return Err(fail(
                    "GRAPH_ILLEGAL_ACCESS",
                    "transient attachment load is illegal",
                    format!("passes[{pi}]"),
                ));
            }
        }
    }
    let mut live: Vec<bool> = observable
        .iter()
        .enumerate()
        .map(|(i, x)| *x && g.passes[i].state == PassState::Enabled)
        .collect();
    let mut stack = roots.clone();
    for (i, is_live) in live.iter().enumerate() {
        if *is_live {
            stack.extend(g.passes[i].reads.iter().map(|b| b.resource.clone()));
        }
    }
    while let Some(r) = stack.pop() {
        if let Some(&p) = writer.get(&r) {
            if !live[p] {
                live[p] = true;
                stack.extend(g.passes[p].reads.iter().map(|b| b.resource.clone()))
            }
        }
    }
    let resource_indices: HashMap<ResourceRef, usize> = g
        .resources
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            (
                ResourceRef {
                    id: resource.id.clone(),
                    version: resource.version,
                },
                index,
            )
        })
        .collect();
    let mut edges = vec![Vec::<(usize, usize, ResourceRef)>::new(); g.passes.len()];
    let mut indeg = vec![0u32; g.passes.len()];
    for (b, p) in g.passes.iter().enumerate() {
        if live[b] {
            for u in &p.reads {
                if let Some(&a) = writer.get(&u.resource) {
                    if live[a] {
                        edges[a].push((b, resource_indices[&u.resource], u.resource.clone()));
                        indeg[b] += 1
                    }
                }
            }
        }
    }
    for e in &mut edges {
        e.sort_by_key(|edge| (edge.0, edge.1))
    }
    let mut q = BinaryHeap::new();
    for i in 0..g.passes.len() {
        if live[i] && indeg[i] == 0 {
            q.push(Reverse(i))
        }
    }
    let mut order = Vec::new();
    while let Some(Reverse(a)) = q.pop() {
        order.push(a);
        for (b, _, _) in &edges[a] {
            indeg[*b] -= 1;
            if indeg[*b] == 0 {
                q.push(Reverse(*b))
            }
        }
    }
    if order.len() != live.iter().filter(|x| **x).count() {
        fn visit(
            node: usize,
            edges: &[Vec<(usize, usize, ResourceRef)>],
            residual: &[bool],
            colors: &mut [u8],
            stack: &mut Vec<usize>,
            incoming: &mut Vec<ResourceRef>,
        ) -> Option<(Vec<usize>, Vec<ResourceRef>)> {
            colors[node] = 1;
            stack.push(node);
            for (next, _, resource) in &edges[node] {
                if !residual[*next] {
                    continue;
                }
                if colors[*next] == 0 {
                    incoming.push(resource.clone());
                    if let Some(cycle) = visit(*next, edges, residual, colors, stack, incoming) {
                        return Some(cycle);
                    }
                    incoming.pop();
                } else if colors[*next] == 1 {
                    let start = stack.iter().position(|pass| pass == next)?;
                    let mut resources = incoming[start..].to_vec();
                    resources.push(resource.clone());
                    return Some((stack[start..].to_vec(), resources));
                }
            }
            stack.pop();
            colors[node] = 2;
            None
        }
        let residual: Vec<bool> = (0..g.passes.len())
            .map(|i| live[i] && indeg[i] > 0)
            .collect();
        let mut colors = vec![0; g.passes.len()];
        let mut stack = Vec::new();
        let mut incoming = Vec::new();
        let mut found = None;
        for node in 0..g.passes.len() {
            if residual[node] && colors[node] == 0 {
                found = visit(
                    node,
                    &edges,
                    &residual,
                    &mut colors,
                    &mut stack,
                    &mut incoming,
                );
                if found.is_some() {
                    break;
                }
            }
        }
        let (cycle_passes, cycle_resources) = found.unwrap_or_default();
        let cycle_edges: Vec<_> = cycle_resources.iter().enumerate().map(|(i, r)| serde_json::json!({"from":g.passes[cycle_passes[i]].id,"resource":r,"to":g.passes[cycle_passes[(i+1)%cycle_passes.len()]].id})).collect();
        return Err(GraphError {
            code: "GRAPH_CYCLE",
            message: "live graph contains a cycle".into(),
            details: serde_json::json!({"message":"live graph contains a cycle","kind":"cycle","edges":cycle_edges}),
        });
    }
    let pos: HashMap<usize, u32> = order
        .iter()
        .enumerate()
        .map(|(i, p)| u32::try_from(i).map(|i| (*p, i)))
        .collect::<Result<_, _>>()
        .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "pass index overflow"))?;
    let boundary = u32::try_from(order.len())
        .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "pass index overflow"))?;
    let output_refs: HashSet<_> = roots.into_iter().collect();
    let mut resources = Vec::new();
    let mut keys = Vec::new();
    for (ri, r) in g.resources.iter().enumerate() {
        let rr = ResourceRef {
            id: r.id.clone(),
            version: r.version,
        };
        let mut points = Vec::new();
        let mut usage = BTreeSet::new();
        for (pi, p) in g.passes.iter().enumerate() {
            if let Some(&x) = pos.get(&pi) {
                for b in &p.reads {
                    if b.resource == rr {
                        points.push(x);
                        usage.insert(usage_read(b.access));
                    }
                }
                for b in &p.writes {
                    if b.resource == rr {
                        points.push(x);
                        usage.insert(usage_write(&b.access));
                    }
                }
            }
        }
        if output_refs.contains(&rr) {
            points.push(boundary)
        }
        if points.is_empty() {
            continue;
        }
        let key = TextureAllocationKey {
            descriptor: norm(r.texture.clone()),
            usage: usage.into_iter().collect(),
            view_formats: vec![],
        };
        keys.push(if matches!(r.residency, Residency::Transient) {
            Some(key)
        } else {
            None
        });
        resources.push(CompiledResource {
            original_index: u32::try_from(ri)
                .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "resource index overflow"))?,
            resource_ref: rr.clone(),
            residency: r.residency.clone(),
            descriptor: norm(r.texture.clone()),
            writer: writer.get(&rr).and_then(|p| pos.get(p)).copied(),
            lifetime: Lifetime {
                first_use: *points.iter().min().unwrap(),
                last_use: *points.iter().max().unwrap(),
            },
            allocation: None,
        })
    }
    let resource_remap: HashMap<ResourceRef, u32> = resources
        .iter()
        .enumerate()
        .map(|(i, r)| u32::try_from(i).map(|i| (r.resource_ref.clone(), i)))
        .collect::<Result<_, _>>()
        .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "resource index overflow"))?;
    let mut groups: BTreeMap<TextureAllocationKey, Vec<usize>> = BTreeMap::new();
    for (i, k) in keys.into_iter().enumerate() {
        if let Some(k) = k {
            groups.entry(k).or_default().push(i)
        }
    }
    let mut classes = Vec::new();
    let mut next = 0u32;
    for (class_index, (k, mut ix)) in groups.into_iter().enumerate() {
        ix.sort_by_key(|i| {
            (
                resources[*i].lifetime.first_use,
                resources[*i].lifetime.last_use,
                resources[*i].original_index,
            )
        });
        let mut active = BinaryHeap::<Reverse<(u32, u32)>>::new();
        let mut free = BinaryHeap::<Reverse<u32>>::new();
        let mut count = 0;
        for i in ix {
            while let Some(Reverse((last, slot))) = active.peek().copied() {
                if last < resources[i].lifetime.first_use {
                    active.pop();
                    free.push(Reverse(slot))
                } else {
                    break;
                }
            }
            let slot = free.pop().map(|x| x.0).unwrap_or_else(|| {
                let x = count;
                count += 1;
                x
            });
            resources[i].allocation = Some(TransientAllocation {
                class: u32::try_from(class_index)
                    .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "class index overflow"))?,
                slot,
            });
            active.push(Reverse((resources[i].lifetime.last_use, slot)))
        }
        next = next
            .checked_add(count)
            .ok_or_else(|| GraphError::new("GRAPH_LIMIT_EXCEEDED", "allocation overflow"))?;
        classes.push(AllocationClass {
            key: k,
            slot_count: count,
        })
    }
    Ok(CompiledGraph {
        schema_version: 1,
        graph_id: g.graph_id,
        revision: g.revision,
        passes: order
            .into_iter()
            .map(|i| {
                Ok(CompiledPass {
                    id: g.passes[i].id.clone(),
                    original_index: u32::try_from(i).map_err(|_| {
                        GraphError::new("GRAPH_LIMIT_EXCEEDED", "pass index overflow")
                    })?,
                    executor: g.passes[i].executor.clone(),
                    parameters: parameters[i].clone(),
                    reads: g.passes[i]
                        .reads
                        .iter()
                        .map(|b| {
                            Ok(CompiledRead {
                                binding: b.binding.clone(),
                                resource: *resource_remap.get(&b.resource).ok_or_else(|| {
                                    GraphError::new(
                                        "GRAPH_UNKNOWN_RESOURCE",
                                        "compiled read remap missing",
                                    )
                                })?,
                                access: b.access,
                            })
                        })
                        .collect::<Result<_, GraphError>>()?,
                    writes: g.passes[i]
                        .writes
                        .iter()
                        .map(|b| {
                            Ok(CompiledWrite {
                                binding: b.binding.clone(),
                                resource: *resource_remap.get(&b.resource).ok_or_else(|| {
                                    GraphError::new(
                                        "GRAPH_UNKNOWN_RESOURCE",
                                        "compiled write remap missing",
                                    )
                                })?,
                                access: b.access.clone(),
                            })
                        })
                        .collect::<Result<_, GraphError>>()?,
                })
            })
            .collect::<Result<_, GraphError>>()?,
        culled_pass_count: u32::try_from(g.passes.len() - live.iter().filter(|x| **x).count())
            .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "pass count overflow"))?,
        culled_resource_count: u32::try_from(g.resources.len() - resources.len())
            .map_err(|_| GraphError::new("GRAPH_LIMIT_EXCEEDED", "resource count overflow"))?,
        resources,
        outputs: g
            .outputs
            .into_iter()
            .map(|o| {
                Ok(CompiledOutput {
                    name: o.name,
                    resource: *resource_remap.get(&o.resource).ok_or_else(|| {
                        GraphError::new("GRAPH_UNKNOWN_RESOURCE", "compiled output remap missing")
                    })?,
                })
            })
            .collect::<Result<_, GraphError>>()?,
        allocation_classes: classes,
        transient_slot_count: next,
    })
}
