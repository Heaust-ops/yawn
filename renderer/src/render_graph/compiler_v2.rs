use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureParameters {
    residency: TextureResidencyV2,
    texture: TextureDescriptorV2,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DepthParameters {
    depth_compare: CompareFunctionV2,
    depth_write_enabled: bool,
    clear_depth: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ForwardParameters {
    clear_color: [f64; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToneMapParameters {
    exposure: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomExtractParameters {
    threshold: f32,
    knee: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomBlurParameters {
    direction: [f32; 2],
    radius: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomCompositeParameters {
    intensity: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LuminanceEdgeParameters {
    strength: f32,
}

fn range(value: f32, min: f32, max: f32, path: String) -> Result<f32, GraphError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(error(
            "GRAPH_PARAMETERS_INVALID",
            &format!("value must be finite and in [{min},{max}]"),
            path,
        ))
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct OutputKey(usize, u16);
#[derive(Clone, Copy)]
struct BoundInput {
    producer: OutputKey,
    active: bool,
}
#[derive(Clone)]
struct DependencyEdge {
    from_node: usize,
    from_socket: String,
    producer_output_ordinal: u16,
    to_node: usize,
    to_socket: String,
    consumer_input_ordinal: u16,
    resource: NodeOutputRef,
}

#[derive(Clone, Copy)]
struct TextureTransition {
    writer_node: usize,
    input_socket: &'static str,
    target: OutputKey,
    output: OutputKey,
}

#[derive(Clone, Copy)]
enum ResolvedTransition {
    Resolved {
        family: u32,
        version: u32,
        target: OutputKey,
    },
    Cyclic,
}

fn reaches(
    from: usize,
    to: usize,
    outgoing_edges: &[Vec<usize>],
    edges: &[DependencyEdge],
    live: &HashSet<usize>,
    memo: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(&answer) = memo.get(&(from, to)) {
        return answer;
    }
    let mut stack = vec![from];
    let mut visited = HashSet::new();
    let mut answer = false;
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        if node == to {
            answer = true;
            break;
        }
        for &edge_index in &outgoing_edges[node] {
            let next = edges[edge_index].to_node;
            if live.contains(&next) {
                stack.push(next);
            }
        }
    }
    memo.insert((from, to), answer);
    answer
}

fn error(code: &'static str, message: &str, path: impl Into<String>) -> GraphError {
    GraphError::at(code, message, path)
}
fn validate_name_length(s: &str, path: impl Into<String>) -> Result<(), GraphError> {
    if s.len() > 64 {
        Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "identifier exceeds 64 bytes",
            path.into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_name_grammar(s: &str, path: impl Into<String>) -> Result<(), GraphError> {
    if s.is_empty() || !identifier(s) {
        Err(error("GRAPH_INVALID_ID", "invalid identifier", path))
    } else {
        Ok(())
    }
}

pub fn mesh_predicate_matches(predicate: TriStatePredicate, flag: bool) -> bool {
    match predicate {
        TriStatePredicate::Any => true,
        TriStatePredicate::RequiredTrue => flag,
        TriStatePredicate::RequiredFalse => !flag,
    }
}

pub fn parse_and_compile_v2(bytes: &[u8]) -> Result<CompiledGraphV2, GraphError> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(GraphError::new(
            "GRAPH_PAYLOAD_TOO_LARGE",
            "graph payload exceeds 1 MiB",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| GraphError::new("GRAPH_ENCODING_INVALID", "graph payload is not UTF-8"))?;
    let probe: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| GraphError::new("GRAPH_JSON_INVALID", e.to_string()))?;
    if probe.get("schemaVersion").and_then(|v| v.as_u64()) != Some(2) {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be 2",
        ));
    }
    let graph = serde_json::from_str(text)
        .map_err(|e| GraphError::new("GRAPH_JSON_INVALID", e.to_string()))?;
    compile_v2(graph)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn compatible_view(a: TextureFormatV2, b: TextureFormatV2) -> bool {
    matches!(
        (a, b),
        (TextureFormatV2::Rgba8Unorm, TextureFormatV2::Rgba8UnormSrgb)
            | (TextureFormatV2::Rgba8UnormSrgb, TextureFormatV2::Rgba8Unorm)
            | (TextureFormatV2::Bgra8Unorm, TextureFormatV2::Bgra8UnormSrgb)
            | (TextureFormatV2::Bgra8UnormSrgb, TextureFormatV2::Bgra8Unorm)
    )
}
fn normalize_texture(
    d: TextureDescriptorV2,
    base: &str,
) -> Result<NormalizedTextureDescriptorV2, GraphError> {
    let bad = |message: &str, suffix: &str| {
        error(
            "GRAPH_PARAMETERS_INVALID",
            message,
            format!("{base}.texture.{suffix}"),
        )
    };
    let (extent, w, h, layers, relative) = match d.extent {
        TextureExtentV2::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => (
            NormalizedTextureExtentV2::Absolute {
                width,
                height,
                depth_or_array_layers,
            },
            width,
            height,
            depth_or_array_layers,
            false,
        ),
        TextureExtentV2::SurfaceRelative {
            mut width,
            mut height,
            depth_or_array_layers,
        } => {
            if width.numerator == 0
                || width.denominator == 0
                || height.numerator == 0
                || height.denominator == 0
                || depth_or_array_layers == 0
            {
                return Err(bad(
                    "extent components and ratio terms must be nonzero",
                    "extent",
                ));
            }
            let g = gcd(width.numerator, width.denominator);
            width.numerator /= g;
            width.denominator /= g;
            let g = gcd(height.numerator, height.denominator);
            height.numerator /= g;
            height.denominator /= g;
            (
                NormalizedTextureExtentV2::SurfaceRelative {
                    width,
                    height,
                    depth_or_array_layers,
                },
                1,
                1,
                depth_or_array_layers,
                true,
            )
        }
    };
    if w == 0 || h == 0 || layers == 0 {
        return Err(bad("extent components must be nonzero", "extent"));
    }
    if relative && d.dimension != TextureDimensionV2::D2 {
        return Err(bad("surface-relative textures must be d2", "extent"));
    }
    if d.dimension == TextureDimensionV2::D1 && (h != 1 || layers != 1) {
        return Err(bad(
            "d1 textures require height and layers equal to one",
            "extent",
        ));
    }
    if d.format == TextureFormatV2::Depth32Float && d.dimension != TextureDimensionV2::D2 {
        return Err(bad("depth textures must be d2", "dimension"));
    }
    if !matches!(d.sample_count, 1 | 4) {
        return Err(bad("sampleCount must be 1 or 4", "sampleCount"));
    }
    if d.mip_level_count == 0 {
        return Err(bad("mipLevelCount must be at least one", "mipLevelCount"));
    }
    if d.sample_count == 4
        && (d.dimension != TextureDimensionV2::D2 || d.mip_level_count != 1 || layers != 1)
    {
        return Err(bad(
            "multisampled textures must be d2, single-mip, single-layer",
            "sampleCount",
        ));
    }
    let max_dim = w.max(h).max(if d.dimension == TextureDimensionV2::D3 {
        layers
    } else {
        1
    });
    let max_mips = 32 - max_dim.leading_zeros();
    if !relative && d.mip_level_count > max_mips {
        return Err(bad(
            "mipLevelCount exceeds the full mip chain",
            "mipLevelCount",
        ));
    }
    let limit = if d.dimension == TextureDimensionV2::D3 {
        2048
    } else {
        8192
    };
    if w > limit
        || h > limit
        || (d.dimension == TextureDimensionV2::D3 && layers > 2048)
        || (d.dimension != TextureDimensionV2::D3 && layers > 256)
    {
        return Err(bad("texture exceeds dimension limits", "extent"));
    }
    for (j, &view) in d.view_formats.iter().enumerate() {
        if view == d.format || !compatible_view(d.format, view) {
            return Err(bad(
                "view format must be compatible and exclude the base format",
                &format!("viewFormats[{j}]"),
            ));
        }
    }
    let mut views = d.view_formats;
    views.sort();
    views.dedup();
    Ok(NormalizedTextureDescriptorV2 {
        dimension: d.dimension,
        format: d.format,
        extent,
        mip_level_count: d.mip_level_count,
        sample_count: d.sample_count,
        view_formats: views,
    })
}

fn decode(node: &NodeV2, i: usize) -> Result<NormalizedParametersV2, GraphError> {
    let base = format!("nodes[{i}].parameters");
    let invalid =
        |e: serde_json::Error| error("GRAPH_PARAMETERS_INVALID", &e.to_string(), base.clone());
    macro_rules! empty {
        ($variant:expr) => {{
            serde_json::from_value::<Empty>(node.parameters.clone()).map_err(invalid)?;
            $variant
        }};
    }
    Ok(match node.executor.key.as_str() {
        "surface_target" => empty!(NormalizedParametersV2::SurfaceTarget),
        "scene_table" => empty!(NormalizedParametersV2::SceneTable),
        "local_aabb_buffer" => empty!(NormalizedParametersV2::LocalAabbBuffer),
        "camera_frustum" => empty!(NormalizedParametersV2::CameraFrustum),
        "visibility_flags" => empty!(NormalizedParametersV2::VisibilityFlags),
        "frustum_cull" => empty!(NormalizedParametersV2::FrustumCull),
        "fullscreen_copy" => empty!(NormalizedParametersV2::FullscreenCopy),
        "tone_map" => {
            let p: ToneMapParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParametersV2::ToneMap {
                exposure: range(p.exposure, 0.0, 32.0, format!("{base}.exposure"))?,
            }
        }
        "bloom_extract" => {
            let p: BloomExtractParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParametersV2::BloomExtract {
                threshold: range(p.threshold, 0.0, 64.0, format!("{base}.threshold"))?,
                knee: range(p.knee, 0.0, 1.0, format!("{base}.knee"))?,
            }
        }
        "bloom_blur" => {
            let p: BloomBlurParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            let x = range(p.direction[0], -1.0, 1.0, format!("{base}.direction[0]"))?;
            let y = range(p.direction[1], -1.0, 1.0, format!("{base}.direction[1]"))?;
            if (x.abs() + y.abs() - 1.0).abs() > 0.0001 {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "direction must be a unit axis",
                    format!("{base}.direction"),
                ));
            }
            NormalizedParametersV2::BloomBlur {
                direction: [x, y],
                radius: range(p.radius, 1.0, 16.0, format!("{base}.radius"))?,
            }
        }
        "bloom_composite" => {
            let p: BloomCompositeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParametersV2::BloomComposite {
                intensity: range(p.intensity, 0.0, 16.0, format!("{base}.intensity"))?,
            }
        }
        "luminance_edge" => {
            let p: LuminanceEdgeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParametersV2::LuminanceEdge {
                strength: range(p.strength, 0.0, 16.0, format!("{base}.strength"))?,
            }
        }
        "present" => empty!(NormalizedParametersV2::Present),
        "texture_spec" => {
            let p: TextureParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if matches!(
                p.residency,
                TextureResidencyV2::History | TextureResidencyV2::Readback
            ) {
                return Err(error(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "history and readback textures are unsupported",
                    format!("{base}.residency"),
                ));
            }
            NormalizedParametersV2::TextureSpec {
                residency: p.residency,
                texture: normalize_texture(p.texture, &base)?,
            }
        }
        "mesh_query" => {
            let object = node.parameters.as_object().ok_or_else(|| {
                error(
                    "GRAPH_PARAMETERS_INVALID",
                    "parameters must be an object",
                    base.clone(),
                )
            })?;
            if object.len() != 1 || !object.contains_key("filters") {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "mesh query parameters must contain only filters",
                    base.clone(),
                ));
            }
            let filters = object["filters"].as_array().ok_or_else(|| {
                error(
                    "GRAPH_PARAMETERS_INVALID",
                    "filters must be an array",
                    format!("{base}.filters"),
                )
            })?;
            let mut found = [None, None];
            for (j, value) in filters.iter().enumerate() {
                let filter = value.as_object().ok_or_else(|| {
                    error(
                        "GRAPH_PARAMETERS_INVALID",
                        "filter must be an object",
                        format!("{base}.filters[{j}]"),
                    )
                })?;
                if filter.len() != 2
                    || !filter.contains_key("flag")
                    || !filter.contains_key("predicate")
                {
                    return Err(error(
                        "GRAPH_PARAMETERS_INVALID",
                        "filter must contain flag and predicate",
                        format!("{base}.filters[{j}]"),
                    ));
                }
                let flag: MeshFlagV2 =
                    serde_json::from_value(filter["flag"].clone()).map_err(|e| {
                        error(
                            "GRAPH_PARAMETERS_INVALID",
                            &e.to_string(),
                            format!("{base}.filters[{j}].flag"),
                        )
                    })?;
                let predicate: TriStatePredicate =
                    serde_json::from_value(filter["predicate"].clone()).map_err(|e| {
                        error(
                            "GRAPH_PARAMETERS_INVALID",
                            &e.to_string(),
                            format!("{base}.filters[{j}].predicate"),
                        )
                    })?;
                let index = if flag == MeshFlagV2::IsVisible { 0 } else { 1 };
                if found[index].replace(predicate).is_some() {
                    return Err(error(
                        "GRAPH_PARAMETERS_INVALID",
                        "duplicate mesh flag",
                        format!("{base}.filters[{j}].flag"),
                    ));
                }
            }
            if found.iter().any(Option::is_none) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "both mesh flags are required",
                    format!("{base}.filters"),
                ));
            }
            NormalizedParametersV2::MeshQuery {
                filters: [
                    NormalizedMeshFilterV2 {
                        flag: MeshFlagV2::IsVisible,
                        predicate: found[0].unwrap(),
                    },
                    NormalizedMeshFilterV2 {
                        flag: MeshFlagV2::IsFrustumCulled,
                        predicate: found[1].unwrap(),
                    },
                ],
            }
        }
        "depth_stencil_config" => {
            let p: DepthParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if !p.clear_depth.is_finite() || !(0.0..=1.0).contains(&p.clear_depth) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearDepth must be finite and in [0,1]",
                    format!("{base}.clearDepth"),
                ));
            }
            NormalizedParametersV2::DepthStencilConfig {
                config: NormalizedDepthStencilV2 {
                    depth_compare: p.depth_compare,
                    depth_write_enabled: p.depth_write_enabled,
                    clear_depth: p.clear_depth,
                },
            }
        }
        "legacy_forward" => {
            let p: ForwardParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if p.clear_color.iter().any(|x| !x.is_finite()) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearColor must be finite",
                    format!("{base}.clearColor"),
                ));
            }
            NormalizedParametersV2::LegacyForward {
                clear_color: p.clear_color,
            }
        }
        _ => unreachable!(),
    })
}

fn accepts(c: TypeConstraintV2, ty: SemanticTypeV2) -> bool {
    match c {
        TypeConstraintV2::Exact(x) => x == ty,
        TypeConstraintV2::OneOf(xs) => xs.contains(&ty),
    }
}

pub fn compile_v2(graph: GraphV2) -> Result<CompiledGraphV2, GraphError> {
    if graph.nodes.len() > 1024 {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "node count exceeds 1024",
            "nodes",
        ));
    }
    let mut input_count = 0usize;
    for (i, node) in graph.nodes.iter().enumerate() {
        input_count = input_count.saturating_add(node.inputs.len());
        if input_count > 8192 {
            return Err(error(
                "GRAPH_LIMIT_EXCEEDED",
                "input count exceeds 8192",
                format!("nodes[{i}].inputs"),
            ));
        }
    }
    if graph
        .nodes
        .iter()
        .filter(|n| n.executor.key == "present")
        .count()
        > 64
    {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "present count exceeds 64",
            "nodes",
        ));
    }
    if graph.schema_version != 2 {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be 2",
        ));
    }
    validate_name_length(&graph.graph_id, "graphId")?;
    for (i, n) in graph.nodes.iter().enumerate() {
        for (value, path) in [
            (&n.id, format!("nodes[{i}].id")),
            (&n.executor.key, format!("nodes[{i}].executor.key")),
        ] {
            validate_name_length(value, path)?;
        }
        for (socket, r) in &n.inputs {
            for (value, path) in [
                (socket, format!("nodes[{i}].inputs.{socket}")),
                (&r.node, format!("nodes[{i}].inputs.{socket}.node")),
                (&r.socket, format!("nodes[{i}].inputs.{socket}.socket")),
            ] {
                validate_name_length(value, path)?;
            }
        }
    }

    validate_name_grammar(&graph.graph_id, "graphId")?;
    let mut ids = HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        for (value, path) in [
            (&n.id, format!("nodes[{i}].id")),
            (&n.executor.key, format!("nodes[{i}].executor.key")),
        ] {
            validate_name_grammar(value, path)?;
        }
        if ids.insert(n.id.as_str(), i).is_some() {
            return Err(error(
                "GRAPH_DUPLICATE_ID",
                "duplicate node id",
                format!("nodes[{i}].id"),
            ));
        }
        for (socket, r) in &n.inputs {
            for (value, path) in [
                (socket, format!("nodes[{i}].inputs.{socket}")),
                (&r.node, format!("nodes[{i}].inputs.{socket}.node")),
                (&r.socket, format!("nodes[{i}].inputs.{socket}.socket")),
            ] {
                validate_name_grammar(value, path)?;
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for (s, r) in &n.inputs {
            if !ids.contains_key(r.node.as_str()) {
                return Err(error(
                    "GRAPH_UNKNOWN_NODE",
                    "unknown input node",
                    format!("nodes[{i}].inputs.{s}.node"),
                ));
            }
        }
    }
    let contracts: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            contract(&n.executor.key).ok_or_else(|| {
                error(
                    "GRAPH_UNKNOWN_EXECUTOR",
                    "unknown executor",
                    format!("nodes[{i}].executor.key"),
                )
            })
        })
        .collect::<Result<_, _>>()?;
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.executor.version != contracts[i].version {
            return Err(error(
                "GRAPH_EXECUTOR_VERSION_UNSUPPORTED",
                "unsupported executor version",
                format!("nodes[{i}].executor.version"),
            ));
        }
    }
    let params: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| decode(n, i))
        .collect::<Result<_, _>>()?;
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.state != NodeStateV2::Enabled {
            return Err(error(
                "GRAPH_NODE_STATE_INVALID",
                "muted nodes are unsupported",
                format!("nodes[{i}].state"),
            ));
        }
    }

    // Socket validation is intentionally global and phased. In particular, no
    // cardinality or semantic error may hide a later structural socket error.
    for (i, n) in graph.nodes.iter().enumerate() {
        for name in n.inputs.keys() {
            if !contracts[i].inputs.iter().any(|s| s.name == name) {
                return Err(error(
                    "GRAPH_UNKNOWN_SOCKET",
                    "unknown input socket",
                    format!("nodes[{i}].inputs.{name}"),
                ));
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for (name, r) in &n.inputs {
            let pn = ids[r.node.as_str()];
            if !contracts[pn].outputs.iter().any(|out| out.name == r.socket) {
                return Err(error(
                    "GRAPH_UNKNOWN_SOCKET",
                    "unknown output socket",
                    format!("nodes[{i}].inputs.{name}.socket"),
                ));
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for input in contracts[i].inputs {
            let inactive = matches!(&params[i], NormalizedParametersV2::MeshQuery { filters } if filters.iter().any(|f| f.flag.input_socket() == input.name && f.predicate == TriStatePredicate::Any));
            if !n.inputs.contains_key(input.name) {
                if input.cardinality == InputCardinalityV2::RequiredOne
                    || (!inactive
                        && matches!(params[i], NormalizedParametersV2::MeshQuery { .. })
                        && input.name != "scene")
                {
                    return Err(error(
                        "GRAPH_SOCKET_CARDINALITY",
                        "required input is missing",
                        format!("nodes[{i}].inputs.{}", input.name),
                    ));
                }
            }
        }
    }

    let mut bound: Vec<BTreeMap<&str, BoundInput>> = vec![BTreeMap::new(); graph.nodes.len()];
    for (i, n) in graph.nodes.iter().enumerate() {
        for input in contracts[i].inputs {
            let inactive = matches!(&params[i], NormalizedParametersV2::MeshQuery { filters } if filters.iter().any(|f| f.flag.input_socket() == input.name && f.predicate == TriStatePredicate::Any));
            let Some(r) = n.inputs.get(input.name) else {
                continue;
            };
            let pn = ids[r.node.as_str()];
            let (ordinal, out) = contracts[pn]
                .outputs
                .iter()
                .enumerate()
                .find(|(_, o)| o.name == r.socket)
                .expect("producer sockets were globally validated");
            let attachment_shape_checked_later = contracts[i].key == "legacy_forward"
                && input.name == "depthTarget"
                && out.semantic_type == SemanticTypeV2::SurfaceTarget;
            if !accepts(input.accepted, out.semantic_type) && !attachment_shape_checked_later {
                return Err(error(
                    "GRAPH_SOCKET_TYPE_MISMATCH",
                    "socket type mismatch",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
            if let Some(flag) = MeshFlagV2::ORDERED
                .iter()
                .find(|f| f.input_socket() == input.name)
            {
                if out.metadata != (OutputMetadataV2::BooleanFlag { flag: *flag }) {
                    return Err(error(
                        "GRAPH_SOCKET_TYPE_MISMATCH",
                        "mesh flag metadata mismatch",
                        format!("nodes[{i}].inputs.{}", input.name),
                    ));
                }
            }
            bound[i].insert(
                input.name,
                BoundInput {
                    producer: OutputKey(pn, ordinal as u16),
                    active: !inactive,
                },
            );
        }
    }
    let root = |key: OutputKey,
                bound: &Vec<BTreeMap<&str, BoundInput>>,
                contracts: &Vec<&ContractV2>|
     -> Option<OutputKey> {
        let mut k = key;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(k.0) {
                return None;
            }
            if contracts[k.0].outputs[k.1 as usize].semantic_type == SemanticTypeV2::SceneTable {
                return Some(k);
            }
            k = bound[k.0].get("scene")?.producer;
        }
    };
    for (i, c) in contracts.iter().enumerate() {
        if c.key == "frustum_cull"
            && root(bound[i]["scene"].producer, &bound, &contracts)
                != root(bound[i]["localAabbs"].producer, &bound, &contracts)
        {
            return Err(error(
                "GRAPH_SOCKET_TYPE_MISMATCH",
                "scene roots differ",
                format!("nodes[{i}].inputs.localAabbs"),
            ));
        }
        if matches!(c.key, "mesh_query" | "legacy_forward") {
            let scene = root(bound[i]["scene"].producer, &bound, &contracts);
            for (s, b) in &bound[i] {
                if b.active
                    && matches!(*s, "isVisible" | "isFrustumCulled" | "draws")
                    && root(b.producer, &bound, &contracts) != scene
                {
                    return Err(error(
                        "GRAPH_SOCKET_TYPE_MISMATCH",
                        "scene roots differ",
                        format!("nodes[{i}].inputs.{s}"),
                    ));
                }
            }
        }
    }
    let mut edges = Vec::new();
    for i in 0..graph.nodes.len() {
        for (input_ordinal, input) in contracts[i].inputs.iter().enumerate() {
            if let Some(b) = bound[i].get(input.name).filter(|b| b.active) {
                edges.push(DependencyEdge {
                    from_node: b.producer.0,
                    from_socket: contracts[b.producer.0].outputs[b.producer.1 as usize]
                        .name
                        .into(),
                    producer_output_ordinal: b.producer.1,
                    to_node: i,
                    to_socket: input.name.into(),
                    consumer_input_ordinal: input_ordinal as u16,
                    resource: graph.nodes[i].inputs[input.name].clone(),
                });
            }
        }
    }
    edges.sort_by_key(|e| {
        (
            e.to_node,
            e.consumer_input_ordinal,
            e.from_node,
            e.producer_output_ordinal,
        )
    });
    let mut deps = vec![Vec::new(); graph.nodes.len()];
    for edge in &edges {
        deps[edge.to_node].push(edge.from_node);
    }
    for node_deps in &mut deps {
        node_deps.sort();
        node_deps.dedup();
    }
    let mut live = HashSet::new();
    let mut stack: Vec<_> = contracts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.inherently_observable)
        .map(|(i, _)| i)
        .collect();
    while let Some(i) = stack.pop() {
        if live.insert(i) {
            stack.extend(deps[i].iter().copied());
        }
    }
    // IDs are independent of scheduling: original node order, then contract output order.
    let mut output_ids = BTreeMap::new();
    let mut resource_meta = Vec::new();
    for i in 0..graph.nodes.len() {
        if live.contains(&i) {
            for (o, out) in contracts[i].outputs.iter().enumerate() {
                let id = resource_meta.len() as u32;
                output_ids.insert(OutputKey(i, o as u16), id);
                resource_meta.push((i, o as u16, *out));
            }
        }
    }
    let all_outputs: usize = contracts.iter().map(|c| c.outputs.len()).sum();

    // Establish families and transitions without relying on a schedule.
    let mut families = Vec::new();
    let mut source_family = HashMap::new();
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        let source = output_ids.get(&OutputKey(i, 0)).copied();
        match &params[i] {
            NormalizedParametersV2::SurfaceTarget => {
                let id = families.len() as u32;
                let r = source.unwrap();
                source_family.insert(OutputKey(i, 0), id);
                families.push(TextureFamilyV2 {
                    id,
                    key: TextureFamilyKeyV2 {
                        source_node: i as u32,
                        source_socket: 0,
                    },
                    source: TextureFamilySourceV2::ImportedSurface { resource: r },
                    lifetime: LifetimeV2 {
                        first_use: 0,
                        last_use: 0,
                    },
                    versions: vec![],
                    usage: vec![],
                    allocation: None,
                    aliasable: false,
                });
            }
            NormalizedParametersV2::TextureSpec { residency, texture } => {
                let id = families.len() as u32;
                let r = source.unwrap();
                source_family.insert(OutputKey(i, 0), id);
                families.push(TextureFamilyV2 {
                    id,
                    key: TextureFamilyKeyV2 {
                        source_node: i as u32,
                        source_socket: 0,
                    },
                    source: TextureFamilySourceV2::AuthoredTexture {
                        resource: r,
                        residency: *residency,
                        descriptor: texture.clone(),
                    },
                    lifetime: LifetimeV2 {
                        first_use: 0,
                        last_use: 0,
                    },
                    versions: vec![],
                    usage: vec![],
                    allocation: None,
                    aliasable: false,
                });
            }
            _ => {}
        }
    }
    let mut transitions: Vec<TextureTransition> = Vec::new();
    let mut transitions_by_target: BTreeMap<OutputKey, Vec<usize>> = BTreeMap::new();
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        let transition_sockets: &[(&str, u16)] = match contracts[i].key {
            "legacy_forward" => &[("colorTarget", 0), ("depthTarget", 1)],
            "fullscreen_copy" | "tone_map" | "bloom_extract" | "bloom_blur" | "bloom_composite"
            | "luminance_edge" => &[("colorTarget", 0)],
            _ => continue,
        };
        for &(input_socket, output_ordinal) in transition_sockets {
            let transition = TextureTransition {
                writer_node: i,
                input_socket,
                target: bound[i][input_socket].producer,
                output: OutputKey(i, output_ordinal),
            };
            let index = transitions.len();
            transitions.push(transition);
            transitions_by_target
                .entry(transition.target)
                .or_default()
                .push(index);
        }
    }

    fn resolve_transition(
        output: OutputKey,
        transitions: &[TextureTransition],
        transition_for_output: &HashMap<OutputKey, usize>,
        source_family: &HashMap<OutputKey, u32>,
        colors: &mut HashMap<OutputKey, u8>,
        resolved: &mut HashMap<OutputKey, ResolvedTransition>,
    ) -> ResolvedTransition {
        if let Some(&value) = resolved.get(&output) {
            return value;
        }
        if colors.get(&output) == Some(&1) {
            return ResolvedTransition::Cyclic;
        }
        colors.insert(output, 1);
        let transition = transitions[transition_for_output[&output]];
        let value = if let Some(&family) = source_family.get(&transition.target) {
            ResolvedTransition::Resolved {
                family,
                version: 0,
                target: transition.target,
            }
        } else if transition_for_output.contains_key(&transition.target) {
            match resolve_transition(
                transition.target,
                transitions,
                transition_for_output,
                source_family,
                colors,
                resolved,
            ) {
                ResolvedTransition::Resolved {
                    family, version, ..
                } => ResolvedTransition::Resolved {
                    family,
                    version: version + 1,
                    target: transition.target,
                },
                ResolvedTransition::Cyclic => ResolvedTransition::Cyclic,
            }
        } else {
            ResolvedTransition::Cyclic
        };
        colors.insert(output, 2);
        resolved.insert(output, value);
        value
    }

    let transition_for_output: HashMap<_, _> = transitions
        .iter()
        .enumerate()
        .map(|(index, transition)| (transition.output, index))
        .collect();
    let mut resolved = HashMap::new();
    let mut colors = HashMap::new();
    for transition in &transitions {
        resolve_transition(
            transition.output,
            &transitions,
            &transition_for_output,
            &source_family,
            &mut colors,
            &mut resolved,
        );
    }
    let mut version_of: HashMap<OutputKey, (u32, u32, u32)> = HashMap::new();
    for transition in &transitions {
        if let ResolvedTransition::Resolved {
            family,
            version,
            target,
        } = resolved[&transition.output]
        {
            let target_id = output_ids[&target];
            version_of.insert(transition.output, (family, version, target_id));
        }
    }

    let mut outgoing_edges = vec![Vec::new(); graph.nodes.len()];
    for (index, edge) in edges.iter().enumerate() {
        if live.contains(&edge.from_node) && live.contains(&edge.to_node) {
            outgoing_edges[edge.from_node].push(index);
        }
    }
    for outgoing in &mut outgoing_edges {
        outgoing.sort_by_key(|&index| {
            let edge = &edges[index];
            (
                edge.to_node,
                edge.producer_output_ordinal,
                edge.consumer_input_ordinal,
            )
        });
    }

    // Every texture reader must execute before a successor overwrites the
    // physical allocation backing the older symbolic version.
    let mut reachability = HashMap::new();
    for (i, contract) in contracts.iter().enumerate() {
        if !live.contains(&i) {
            continue;
        }
        for input in contract.inputs.iter().filter(|input| {
            matches!(
                input.role,
                InputRoleV2::Present | InputRoleV2::SampledTexture
            )
        }) {
            let key = bound[i][input.name].producer;
            if !version_of.contains_key(&key) {
                continue;
            }
            let Some(next_indices) = transitions_by_target.get(&key) else {
                continue;
            };
            let [next_index] = next_indices.as_slice() else {
                continue;
            };
            let next = transitions[*next_index];
            if i != next.writer_node
                && !reaches(
                    i,
                    next.writer_node,
                    &outgoing_edges,
                    &edges,
                    &live,
                    &mut reachability,
                )
            {
                return Err(error(
                    "GRAPH_RESOURCE_VERSION_INVALID",
                    "older texture version may be read after its successor",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
        }
    }

    // Same-pass hazards are global and precede every duplicate-writer diagnostic.
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        if contracts[i].key == "legacy_forward" {
            if bound[i]["colorTarget"].producer == bound[i]["depthTarget"].producer
                || matches!((version_of.get(&OutputKey(i, 0)), version_of.get(&OutputKey(i, 1))), (Some((cf, _, _)), Some((df, _, _))) if cf == df)
            {
                return Err(error(
                    "GRAPH_SAME_PASS_HAZARD",
                    "color and depth use one texture family",
                    format!("nodes[{i}].inputs"),
                ));
            }
        } else if contracts[i]
            .inputs
            .iter()
            .any(|input| matches!(input.role, InputRoleV2::SampledTexture))
        {
            let hazard = contracts[i].inputs.iter().filter(|input| matches!(input.role, InputRoleV2::SampledTexture)).any(|input| matches!((version_of.get(&bound[i][input.name].producer), version_of.get(&OutputKey(i, 0))), (Some((sf, _, _)), Some((tf, _, _))) if sf == tf));
            if hazard {
                return Err(error(
                    "GRAPH_SAME_PASS_HAZARD",
                    "copy source and target use one texture family",
                    format!("nodes[{i}].inputs"),
                ));
            }
        }
    }
    let mut first_writer = BTreeMap::new();
    for transition in &transitions {
        if first_writer
            .insert(transition.target, transition.writer_node)
            .is_some_and(|writer| writer != transition.writer_node)
        {
            return Err(error(
                "GRAPH_DUPLICATE_WRITER",
                "texture version has multiple writers",
                format!(
                    "nodes[{}].inputs.{}",
                    transition.writer_node, transition.input_socket
                ),
            ));
        }
    }

    // Materialize versions only after hazard and writer precedence has been settled.
    for transition in &transitions {
        if let Some(&(family, version, target)) = version_of.get(&transition.output) {
            families[family as usize].versions.push(TextureVersionV2 {
                version,
                resource: output_ids[&transition.output],
                target,
                initialized: true,
                stored: true,
                lifetime: LifetimeV2 {
                    first_use: 0,
                    last_use: 0,
                },
            });
        }
    }
    for family in &mut families {
        family.versions.sort_by_key(|version| version.version);
        for (index, version) in family.versions.iter().enumerate() {
            if version.version != index as u32 {
                return Err(error(
                    "GRAPH_RESOURCE_VERSION_INVALID",
                    "texture versions must form a dense linear chain",
                    "resources",
                ));
            }
        }
    }

    // Validate every independently resolved attachment before graph cycle reporting.
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) || contracts[i].key != "legacy_forward" {
            continue;
        }
        let (Some(&(cf, _, _)), Some(&(df, _, _))) = (
            version_of.get(&OutputKey(i, 0)),
            version_of.get(&OutputKey(i, 1)),
        ) else {
            continue;
        };
        let cd = match &families[cf as usize].source {
            TextureFamilySourceV2::AuthoredTexture { descriptor, .. } => Some(descriptor),
            _ => None,
        };
        let dd = match &families[df as usize].source {
            TextureFamilySourceV2::AuthoredTexture { descriptor, .. } => descriptor,
            _ => {
                return Err(error(
                    "GRAPH_ILLEGAL_ACCESS",
                    "depth target must be authored",
                    format!("nodes[{i}].inputs.depthTarget"),
                ))
            }
        };
        let ok_depth = dd.dimension == TextureDimensionV2::D2
            && dd.format == TextureFormatV2::Depth32Float
            && dd.sample_count == 1
            && extent_layers(&dd.extent) == 1;
        let ok_color = cd.is_none_or(|d| {
            d.format != TextureFormatV2::Depth32Float
                && d.dimension == dd.dimension
                && d.extent == dd.extent
                && d.sample_count == 1
        });
        let surface_ok = cd.is_some()
            || matches!(&dd.extent,NormalizedTextureExtentV2::SurfaceRelative{width,height,..} if *width==RatioV2{numerator:1,denominator:1}&&*height==RatioV2{numerator:1,denominator:1});
        if !ok_depth || !ok_color || !surface_ok {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "attachments are incompatible",
                format!("nodes[{i}].inputs"),
            ));
        }
    }

    for i in 0..graph.nodes.len() {
        if !live.contains(&i)
            || !contracts[i]
                .inputs
                .iter()
                .any(|input| matches!(input.role, InputRoleV2::SampledTexture))
        {
            continue;
        }
        let source_key = bound[i]["source"].producer;
        let Some(&(source_family_id, _, _)) = version_of.get(&source_key) else {
            return Err(error(
                "GRAPH_UNINITIALIZED_RESOURCE",
                "copy source is not produced",
                format!("nodes[{i}].inputs.source"),
            ));
        };
        let Some(&(target_family_id, _, _)) = version_of.get(&OutputKey(i, 0)) else {
            continue;
        };
        let source_descriptor = match &families[source_family_id as usize].source {
            TextureFamilySourceV2::AuthoredTexture { descriptor, .. } => descriptor,
            TextureFamilySourceV2::ImportedSurface { .. } => {
                return Err(error(
                    "GRAPH_ILLEGAL_ACCESS",
                    "copy source must be an authored texture",
                    format!("nodes[{i}].inputs.source"),
                ))
            }
        };
        let source_ok = source_descriptor.format == TextureFormatV2::Rgba16Float
            && is_single_view_d2(source_descriptor);
        let target_descriptor = match &families[target_family_id as usize].source {
            TextureFamilySourceV2::AuthoredTexture { descriptor, .. } => Some(descriptor),
            TextureFamilySourceV2::ImportedSurface { .. } => None,
        };
        let authored_target_ok = target_descriptor.is_some_and(|descriptor| {
            descriptor.format == TextureFormatV2::Rgba16Float && is_single_view_d2(descriptor)
        });
        let bloom_input_ok = if contracts[i].key == "bloom_composite" {
            let bloom_key = bound[i]["bloom"].producer;
            let Some(&(bloom_family_id, _, _)) = version_of.get(&bloom_key) else {
                return Err(error(
                    "GRAPH_UNINITIALIZED_RESOURCE",
                    "bloom source is not produced",
                    format!("nodes[{i}].inputs.bloom"),
                ));
            };
            match &families[bloom_family_id as usize].source {
                TextureFamilySourceV2::AuthoredTexture { descriptor, .. } => {
                    descriptor.format == TextureFormatV2::Rgba16Float
                        && is_single_view_d2(descriptor)
                }
                TextureFamilySourceV2::ImportedSurface { .. } => {
                    return Err(error(
                        "GRAPH_ILLEGAL_ACCESS",
                        "bloom source must be an authored texture",
                        format!("nodes[{i}].inputs.bloom"),
                    ))
                }
            }
        } else {
            true
        };
        let source_is_full_surface = matches!(&source_descriptor.extent, NormalizedTextureExtentV2::SurfaceRelative { width, height, depth_or_array_layers: 1 } if *width == RatioV2 { numerator:1, denominator:1 } && *height == RatioV2 { numerator:1, denominator:1 });
        let target_matches_source = target_descriptor
            .is_some_and(|descriptor| descriptor.extent == source_descriptor.extent);
        let descriptor_ok = match contracts[i].key {
            "fullscreen_copy" => {
                target_descriptor.is_none() && source_is_full_surface
                    || target_descriptor.is_some_and(|descriptor| {
                        descriptor.format != TextureFormatV2::Depth32Float
                            && is_single_view_d2(descriptor)
                            && descriptor.extent == source_descriptor.extent
                    })
            }
            "tone_map" => target_descriptor.is_none() && source_is_full_surface,
            "bloom_extract" => authored_target_ok,
            "bloom_blur" | "luminance_edge" => authored_target_ok && target_matches_source,
            "bloom_composite" => authored_target_ok && target_matches_source && bloom_input_ok,
            _ => false,
        };
        if !source_ok || !descriptor_ok {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "fullscreen textures are incompatible",
                format!("nodes[{i}].inputs"),
            ));
        }
    }

    // Initialization and presentation legality are later than attachment compatibility.
    for (i, contract) in contracts.iter().enumerate() {
        if !live.contains(&i) || contract.key != "present" {
            continue;
        }
        let key = bound[i]["surface"].producer;
        let Some(&(family, _, _)) = version_of.get(&key) else {
            if !matches!(resolved.get(&key), Some(ResolvedTransition::Cyclic)) {
                return Err(error(
                    "GRAPH_UNINITIALIZED_RESOURCE",
                    "present source is not produced",
                    format!("nodes[{i}].inputs.surface"),
                ));
            }
            continue;
        };
        if !matches!(
            families[family as usize].source,
            TextureFamilySourceV2::ImportedSurface { .. }
        ) {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "offscreen textures cannot be presented",
                format!("nodes[{i}].inputs.surface"),
            ));
        }
    }

    // Stable Kahn scheduling is deliberately after resource and access validation.
    let mut indegree = vec![0; graph.nodes.len()];
    for &node in &live {
        indegree[node] = edges
            .iter()
            .filter(|edge| edge.to_node == node && live.contains(&edge.from_node))
            .count();
    }
    let mut queue = BinaryHeap::new();
    for &node in &live {
        if indegree[node] == 0 {
            queue.push(Reverse(node));
        }
    }
    let mut order = Vec::new();
    while let Some(Reverse(node)) = queue.pop() {
        order.push(node);
        for &edge_index in &outgoing_edges[node] {
            let consumer = edges[edge_index].to_node;
            indegree[consumer] -= 1;
            if indegree[consumer] == 0 {
                queue.push(Reverse(consumer));
            }
        }
    }
    if order.len() != live.len() {
        let residual: Vec<_> = (0..graph.nodes.len())
            .map(|node| live.contains(&node) && indegree[node] != 0)
            .collect();
        fn cycle_dfs(
            node: usize,
            outgoing_edges: &[Vec<usize>],
            edges: &[DependencyEdge],
            residual: &[bool],
            colors: &mut [u8],
            node_stack: &mut Vec<usize>,
            edge_stack: &mut Vec<usize>,
        ) -> Option<Vec<usize>> {
            colors[node] = 1;
            node_stack.push(node);
            for &edge_index in &outgoing_edges[node] {
                let to = edges[edge_index].to_node;
                if !residual[to] {
                    continue;
                }
                if colors[to] == 0 {
                    edge_stack.push(edge_index);
                    if let Some(cycle) = cycle_dfs(
                        to,
                        outgoing_edges,
                        edges,
                        residual,
                        colors,
                        node_stack,
                        edge_stack,
                    ) {
                        return Some(cycle);
                    }
                    edge_stack.pop();
                } else if colors[to] == 1 {
                    let position = node_stack
                        .iter()
                        .position(|&stacked| stacked == to)
                        .unwrap();
                    let mut cycle = edge_stack[position..].to_vec();
                    cycle.push(edge_index);
                    return Some(cycle);
                }
            }
            node_stack.pop();
            colors[node] = 2;
            None
        }
        let mut colors = vec![0; graph.nodes.len()];
        let mut cycle = None;
        for node in 0..graph.nodes.len() {
            if residual[node] && colors[node] == 0 {
                cycle = cycle_dfs(
                    node,
                    &outgoing_edges,
                    &edges,
                    &residual,
                    &mut colors,
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
                if cycle.is_some() {
                    break;
                }
            }
        }
        let mut graph_error = GraphError::new("GRAPH_CYCLE", "live graph contains a cycle");
        let payload: Vec<_> = cycle
            .unwrap_or_default()
            .into_iter()
            .map(|index| {
                let edge = &edges[index];
                serde_json::json!({
                    "fromNode": graph.nodes[edge.from_node].id,
                    "fromSocket": edge.from_socket,
                    "toNode": graph.nodes[edge.to_node].id,
                    "toSocket": edge.to_socket,
                    "resource": edge.resource,
                })
            })
            .collect();
        graph_error.details =
            serde_json::json!({"message":graph_error.message,"kind":"cycle","edges":payload});
        return Err(graph_error);
    }
    if transitions
        .iter()
        .any(|transition| matches!(resolved[&transition.output], ResolvedTransition::Cyclic))
    {
        return Err(error(
            "GRAPH_RESOURCE_VERSION_INVALID",
            "texture predecessor is unresolved in an acyclic graph",
            "resources",
        ));
    }

    let mut resources = Vec::new();
    for (i, o, out) in resource_meta {
        let key = OutputKey(i, o);
        let id = output_ids[&key];
        let scene = || output_ids[&root(bound[i]["scene"].producer, &bound, &contracts).unwrap()];
        let plan = match out.semantic_type {
            SemanticTypeV2::SurfaceTarget => ResourcePlanV2::SurfaceTarget {
                family: source_family[&key],
            },
            SemanticTypeV2::TextureSpec => {
                if let NormalizedParametersV2::TextureSpec { residency, texture } = &params[i] {
                    ResourcePlanV2::TextureSpec {
                        family: source_family[&key],
                        residency: *residency,
                        descriptor: texture.clone(),
                    }
                } else {
                    unreachable!()
                }
            }
            SemanticTypeV2::Texture => {
                let (f, v, t) = version_of[&key];
                ResourcePlanV2::Texture {
                    family: f,
                    version: v,
                    target: t,
                    initialized: true,
                    stored: true,
                    allocation: None,
                }
            }
            SemanticTypeV2::SceneTable => ResourcePlanV2::SceneTable,
            SemanticTypeV2::LocalAabbBuffer => ResourcePlanV2::LocalAabbBuffer { scene: scene() },
            SemanticTypeV2::CameraFrustum => ResourcePlanV2::CameraFrustum,
            SemanticTypeV2::BooleanFlagBuffer => {
                if let OutputMetadataV2::BooleanFlag { flag } = out.metadata {
                    ResourcePlanV2::BooleanFlagBuffer {
                        scene: scene(),
                        flag,
                    }
                } else {
                    unreachable!()
                }
            }
            SemanticTypeV2::DrawStream => ResourcePlanV2::DrawStream { scene: scene() },
            SemanticTypeV2::DepthStencilConfig => {
                if let NormalizedParametersV2::DepthStencilConfig { config } = &params[i] {
                    ResourcePlanV2::DepthStencilConfig { config: *config }
                } else {
                    unreachable!()
                }
            }
        };
        resources.push(CompiledResourceV2 {
            original_node_index: i as u32,
            output_ordinal: o,
            origin: NodeOutputRef {
                node: graph.nodes[i].id.clone(),
                socket: out.name.into(),
            },
            semantic_type: out.semantic_type,
            producer_execution: None,
            lifetime: None,
            plan,
        });
        let _ = id;
    }
    let mut executions = Vec::new();
    let mut node_execution = HashMap::new();
    for &i in &order {
        if contracts[i].execution == ExecutionClassV2::Source {
            continue;
        }
        let ordinal = executions.len() as u32;
        node_execution.insert(i, ordinal);
        let input_resource = |s: &str| output_ids[&bound[i][s].producer];
        let mut inputs = Vec::new();
        for s in contracts[i].inputs {
            if let Some(b) = bound[i].get(s.name).filter(|b| b.active) {
                inputs.push(CompiledSocketInputV2 {
                    socket: s.name.into(),
                    resource: output_ids[&b.producer],
                });
            }
        }
        let outputs: Vec<_> = contracts[i]
            .outputs
            .iter()
            .enumerate()
            .map(|(o, s)| CompiledSocketOutputV2 {
                socket: s.name.into(),
                resource: output_ids[&OutputKey(i, o as u16)],
            })
            .collect();
        let mut accesses = Vec::new();
        let kind = match contracts[i].key {
            "frustum_cull" => {
                for (s, m) in [
                    ("scene", AccessModeV2::StorageRead),
                    ("localAabbs", AccessModeV2::StorageRead),
                    ("frustum", AccessModeV2::UniformRead),
                ] {
                    accesses.push(CompiledAccessV2 {
                        socket: s.into(),
                        resource: input_resource(s),
                        mode: m,
                    });
                }
                let r = output_ids[&OutputKey(i, 0)];
                accesses.push(CompiledAccessV2 {
                    socket: "flags".into(),
                    resource: r,
                    mode: AccessModeV2::StorageWrite {
                        full_overwrite: true,
                    },
                });
                ExecutionKindV2::Compute {
                    work: ComputeWorkV2::FrustumCull,
                }
            }
            "mesh_query" => {
                for s in ["scene", "isVisible", "isFrustumCulled"] {
                    if let Some(b) = bound[i].get(s).filter(|b| b.active) {
                        accesses.push(CompiledAccessV2 {
                            socket: s.into(),
                            resource: output_ids[&b.producer],
                            mode: AccessModeV2::StorageRead,
                        });
                    }
                }
                accesses.push(CompiledAccessV2 {
                    socket: "draws".into(),
                    resource: output_ids[&OutputKey(i, 0)],
                    mode: AccessModeV2::StorageWrite {
                        full_overwrite: true,
                    },
                });
                ExecutionKindV2::Compute {
                    work: ComputeWorkV2::MeshQuery,
                }
            }
            "legacy_forward" => {
                let color = output_ids[&OutputKey(i, 0)];
                let depth = output_ids[&OutputKey(i, 1)];
                let clear = match params[i] {
                    NormalizedParametersV2::LegacyForward { clear_color } => clear_color,
                    _ => unreachable!(),
                };
                let config_node = bound[i]["depthStencil"].producer.0;
                let dc = match params[config_node] {
                    NormalizedParametersV2::DepthStencilConfig { config } => config,
                    _ => unreachable!(),
                };
                let cl = NormalizedColorLoadV2::Clear { value: clear };
                let dl = NormalizedDepthLoadV2::Clear {
                    value: dc.clear_depth,
                };
                for s in ["scene", "draws"] {
                    accesses.push(CompiledAccessV2 {
                        socket: s.into(),
                        resource: input_resource(s),
                        mode: if s == "draws" {
                            AccessModeV2::IndirectRead
                        } else {
                            AccessModeV2::SemanticRead
                        },
                    });
                }
                accesses.push(CompiledAccessV2 {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessModeV2::ColorAttachment {
                        location: 0,
                        load: cl,
                        store: StoreOpV2::Store,
                        full_overwrite: true,
                    },
                });
                accesses.push(CompiledAccessV2 {
                    socket: "depth".into(),
                    resource: depth,
                    mode: AccessModeV2::DepthAttachment {
                        load: dl,
                        store: StoreOpV2::Store,
                        full_overwrite: true,
                    },
                });
                ExecutionKindV2::Render {
                    color_attachments: vec![ColorAttachmentPlanV2 {
                        resource: color,
                        location: 0,
                        load: cl,
                        store: StoreOpV2::Store,
                    }],
                    depth_stencil: Some(DepthStencilAttachmentPlanV2 {
                        resource: depth,
                        load: dl,
                        store: StoreOpV2::Store,
                    }),
                }
            }
            "fullscreen_copy" | "tone_map" | "bloom_extract" | "bloom_blur" | "bloom_composite"
            | "luminance_edge" => {
                let color = output_ids[&OutputKey(i, 0)];
                let load = NormalizedColorLoadV2::Clear {
                    value: [0.0, 0.0, 0.0, 0.0],
                };
                accesses.push(CompiledAccessV2 {
                    socket: "source".into(),
                    resource: input_resource("source"),
                    mode: AccessModeV2::SampledTexture,
                });
                if contracts[i].key == "bloom_composite" {
                    accesses.push(CompiledAccessV2 {
                        socket: "bloom".into(),
                        resource: input_resource("bloom"),
                        mode: AccessModeV2::SampledTexture,
                    });
                }
                accesses.push(CompiledAccessV2 {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessModeV2::ColorAttachment {
                        location: 0,
                        load,
                        store: StoreOpV2::Store,
                        full_overwrite: true,
                    },
                });
                ExecutionKindV2::Render {
                    color_attachments: vec![ColorAttachmentPlanV2 {
                        resource: color,
                        location: 0,
                        load,
                        store: StoreOpV2::Store,
                    }],
                    depth_stencil: None,
                }
            }
            "present" => {
                let r = input_resource("surface");
                accesses.push(CompiledAccessV2 {
                    socket: "surface".into(),
                    resource: r,
                    mode: AccessModeV2::Present,
                });
                ExecutionKindV2::Present { surface: r }
            }
            _ => unreachable!(),
        };
        executions.push(CompiledExecutionV2 {
            id: graph.nodes[i].id.clone(),
            original_node_index: i as u32,
            executor: graph.nodes[i].executor.clone(),
            parameters: params[i].clone(),
            kind,
            inputs,
            outputs,
            accesses,
        });
    }
    for (ordinal, e) in executions.iter().enumerate() {
        for o in &e.outputs {
            resources[o.resource as usize].producer_execution = Some(ordinal as u32);
        }
    }
    // Dense lifetimes touch bindings, outputs, and accesses.
    for (ordinal, e) in executions.iter().enumerate() {
        let ordinal = ordinal as u32;
        let mut touched = BTreeSet::new();
        for x in &e.inputs {
            touched.insert(x.resource);
        }
        for x in &e.outputs {
            touched.insert(x.resource);
        }
        for x in &e.accesses {
            touched.insert(x.resource);
        }
        for r in touched {
            let life = resources[r as usize].lifetime.get_or_insert(LifetimeV2 {
                first_use: ordinal,
                last_use: ordinal,
            });
            life.first_use = life.first_use.min(ordinal);
            life.last_use = life.last_use.max(ordinal);
        }
    }
    for f in &mut families {
        let mut first = None;
        let mut last = 0;
        for v in &mut f.versions {
            v.lifetime = resources[v.resource as usize].lifetime.unwrap();
            first = Some(first.map_or(v.lifetime.first_use, |x: u32| x.min(v.lifetime.first_use)));
            last = last.max(v.lifetime.last_use);
        }
        f.lifetime = LifetimeV2 {
            first_use: first.unwrap_or(0),
            last_use: last,
        };
        f.usage = texture_usage(f, &executions);
        f.aliasable = matches!(
            f.source,
            TextureFamilySourceV2::AuthoredTexture {
                residency: TextureResidencyV2::Transient,
                ..
            }
        ) && f.versions.iter().all(|v| v.initialized);
    }
    let (classes, transient) = allocate(&mut families, &mut resources);
    if resources.len() > 1024 {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "too many final resources",
            "resources",
        ));
    }
    Ok(CompiledGraphV2 {
        schema_version: 2,
        graph_id: graph.graph_id,
        revision: graph.revision,
        node_count: graph.nodes.len() as u32,
        resources,
        executions,
        texture_families: families,
        allocation_classes: classes,
        culled_node_count: (graph.nodes.len() - live.len()) as u32,
        culled_resource_count: (all_outputs - output_ids.len()) as u32,
        transient_slot_count: transient,
    })
}

fn extent_layers(e: &NormalizedTextureExtentV2) -> u32 {
    match e {
        NormalizedTextureExtentV2::Absolute {
            depth_or_array_layers,
            ..
        }
        | NormalizedTextureExtentV2::SurfaceRelative {
            depth_or_array_layers,
            ..
        } => *depth_or_array_layers,
    }
}
fn is_single_view_d2(descriptor: &NormalizedTextureDescriptorV2) -> bool {
    descriptor.dimension == TextureDimensionV2::D2
        && descriptor.sample_count == 1
        && descriptor.mip_level_count == 1
        && extent_layers(&descriptor.extent) == 1
}
fn texture_usage(f: &TextureFamilyV2, executions: &[CompiledExecutionV2]) -> Vec<TextureUsageV2> {
    let rs: HashSet<_> = f.versions.iter().map(|v| v.resource).collect();
    let mut u = BTreeSet::new();
    for e in executions {
        for a in &e.accesses {
            if !rs.contains(&a.resource) {
                continue;
            }
            match a.mode {
                AccessModeV2::SampledTexture => {
                    u.insert(TextureUsageV2::Sampled);
                }
                AccessModeV2::StorageRead | AccessModeV2::StorageWrite { .. } => {
                    u.insert(TextureUsageV2::Storage);
                }
                AccessModeV2::ColorAttachment { .. } => {
                    u.insert(TextureUsageV2::ColorAttachment);
                }
                AccessModeV2::DepthAttachment { .. } => {
                    u.insert(TextureUsageV2::DepthAttachment);
                }
                _ => {}
            }
        }
    }
    u.into_iter().collect()
}
fn allocate(
    families: &mut [TextureFamilyV2],
    resources: &mut [CompiledResourceV2],
) -> (Vec<AllocationClassV2>, u32) {
    let mut grouped: BTreeMap<TextureCompatibilityKeyV2, Vec<usize>> = BTreeMap::new();
    for (i, f) in families.iter().enumerate() {
        if let TextureFamilySourceV2::AuthoredTexture { descriptor, .. } = &f.source {
            grouped
                .entry(TextureCompatibilityKeyV2 {
                    dimension: descriptor.dimension,
                    format: descriptor.format,
                    extent: descriptor.extent.clone(),
                    mip_level_count: descriptor.mip_level_count,
                    sample_count: descriptor.sample_count,
                    view_formats: descriptor.view_formats.clone(),
                })
                .or_default()
                .push(i);
        }
    }
    let mut classes = Vec::new();
    let mut transient = 0;
    for (key, ids) in grouped {
        let class = classes.len() as u32;
        let mut slots: Vec<AllocationSlotV2> = Vec::new();
        let mut aliasable = Vec::new();
        let mut dedicated = Vec::new();
        let mut persistent_ids = Vec::new();
        for fi in ids {
            let persistent = matches!(
                families[fi].source,
                TextureFamilySourceV2::AuthoredTexture {
                    residency: TextureResidencyV2::Persistent,
                    ..
                }
            );
            let alias = families[fi].aliasable && !persistent;
            if persistent {
                persistent_ids.push(fi);
            } else if alias {
                aliasable.push(fi);
            } else {
                dedicated.push(fi);
            }
        }
        aliasable.sort_by_key(|&fi| {
            (
                families[fi].lifetime.first_use,
                families[fi].lifetime.last_use,
                families[fi].key.clone(),
            )
        });
        dedicated.sort_by_key(|&fi| families[fi].key.clone());
        persistent_ids.sort_by_key(|&fi| families[fi].key.clone());
        for fi in aliasable.into_iter().chain(dedicated).chain(persistent_ids) {
            let persistent = matches!(
                families[fi].source,
                TextureFamilySourceV2::AuthoredTexture {
                    residency: TextureResidencyV2::Persistent,
                    ..
                }
            );
            let alias = families[fi].aliasable && !persistent;
            let found = if alias {
                slots.iter().position(|s| {
                    s.kind == AllocationKindV2::AliasedTransient
                        && s.occupants.iter().all(|&old| {
                            families[old as usize].lifetime.last_use
                                < families[fi].lifetime.first_use
                        })
                })
            } else {
                None
            };
            let slot = found.unwrap_or_else(|| {
                let s = slots.len();
                if !persistent {
                    transient += 1;
                }
                slots.push(AllocationSlotV2 {
                    kind: if persistent {
                        AllocationKindV2::Persistent
                    } else if alias {
                        AllocationKindV2::AliasedTransient
                    } else {
                        AllocationKindV2::DedicatedTransient
                    },
                    usage: Vec::new(),
                    occupants: Vec::new(),
                });
                s
            });
            slots[slot].occupants.push(fi as u32);
            slots[slot].usage.extend(families[fi].usage.iter().copied());
            slots[slot].usage.sort();
            slots[slot].usage.dedup();
            let a = AllocationRefV2 {
                class,
                slot: slot as u32,
            };
            families[fi].allocation = Some(a);
            for v in &families[fi].versions {
                if let ResourcePlanV2::Texture { allocation, .. } =
                    &mut resources[v.resource as usize].plan
                {
                    *allocation = Some(a);
                }
            }
        }
        classes.push(AllocationClassV2 { key, slots });
    }
    (classes, transient)
}
