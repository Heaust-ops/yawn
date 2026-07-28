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
    residency: TextureResidency,
    texture: TextureDescriptor,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CullParameters {
    camera: ActiveCamera,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct QueryParameters {
    visible_predicate: TriStatePredicate,
    visible_default: bool,
    frustum_culled_predicate: TriStatePredicate,
    frustum_culled_default: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PipelineParameters {
    pipeline: String,
    depth_compare: CompareFunction,
    depth_write_enabled: bool,
    clear_depth: f32,
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

pub fn mesh_predicate_matches(predicate: RuntimePredicate, flag: bool) -> bool {
    match predicate {
        RuntimePredicate::Any => true,
        RuntimePredicate::RequiredTrue => flag,
        RuntimePredicate::RequiredFalse => !flag,
        RuntimePredicate::Never => false,
    }
}

pub fn parse_and_compile(bytes: &[u8]) -> Result<CompiledGraph, GraphError> {
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
    compile(graph)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn compatible_view(a: TextureFormat, b: TextureFormat) -> bool {
    matches!(
        (a, b),
        (TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb)
            | (TextureFormat::Rgba8UnormSrgb, TextureFormat::Rgba8Unorm)
            | (TextureFormat::Bgra8Unorm, TextureFormat::Bgra8UnormSrgb)
            | (TextureFormat::Bgra8UnormSrgb, TextureFormat::Bgra8Unorm)
    )
}
fn normalize_texture(
    d: TextureDescriptor,
    base: &str,
) -> Result<NormalizedTextureDescriptor, GraphError> {
    let bad = |message: &str, suffix: &str| {
        error(
            "GRAPH_PARAMETERS_INVALID",
            message,
            format!("{base}.texture.{suffix}"),
        )
    };
    let (extent, w, h, layers, relative) = match d.extent {
        TextureExtent::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => (
            NormalizedTextureExtent::Absolute {
                width,
                height,
                depth_or_array_layers,
            },
            width,
            height,
            depth_or_array_layers,
            false,
        ),
        TextureExtent::SurfaceRelative {
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
                NormalizedTextureExtent::SurfaceRelative {
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
    if relative && d.dimension != TextureDimension::D2 {
        return Err(bad("surface-relative textures must be d2", "extent"));
    }
    if d.dimension == TextureDimension::D1 && (h != 1 || layers != 1) {
        return Err(bad(
            "d1 textures require height and layers equal to one",
            "extent",
        ));
    }
    if d.format == TextureFormat::Depth32Float && d.dimension != TextureDimension::D2 {
        return Err(bad("depth textures must be d2", "dimension"));
    }
    if !matches!(d.sample_count, 1 | 4) {
        return Err(bad("sampleCount must be 1 or 4", "sampleCount"));
    }
    if d.mip_level_count == 0 {
        return Err(bad("mipLevelCount must be at least one", "mipLevelCount"));
    }
    if d.sample_count == 4
        && (d.dimension != TextureDimension::D2 || d.mip_level_count != 1 || layers != 1)
    {
        return Err(bad(
            "multisampled textures must be d2, single-mip, single-layer",
            "sampleCount",
        ));
    }
    let max_dim = w.max(h).max(if d.dimension == TextureDimension::D3 {
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
    let limit = if d.dimension == TextureDimension::D3 {
        2048
    } else {
        8192
    };
    if w > limit
        || h > limit
        || (d.dimension == TextureDimension::D3 && layers > 2048)
        || (d.dimension != TextureDimension::D3 && layers > 256)
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
    Ok(NormalizedTextureDescriptor {
        dimension: d.dimension,
        format: d.format,
        extent,
        mip_level_count: d.mip_level_count,
        sample_count: d.sample_count,
        view_formats: views,
    })
}

fn decode(node: &Node, i: usize) -> Result<NormalizedParameters, GraphError> {
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
        "mesh" => empty!(NormalizedParameters::Mesh),
        "frustum_cull" => {
            let p: CullParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::FrustumCull { camera: p.camera }
        }
        "fullscreen_copy" => empty!(NormalizedParameters::FullscreenCopy),
        "tone_map" => {
            let p: ToneMapParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::ToneMap {
                exposure: range(p.exposure, 0.0, 32.0, format!("{base}.exposure"))?,
            }
        }
        "bloom_extract" => {
            let p: BloomExtractParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::BloomExtract {
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
            NormalizedParameters::BloomBlur {
                direction: [x, y],
                radius: range(p.radius, 1.0, 16.0, format!("{base}.radius"))?,
            }
        }
        "bloom_composite" => {
            let p: BloomCompositeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::BloomComposite {
                intensity: range(p.intensity, 0.0, 16.0, format!("{base}.intensity"))?,
            }
        }
        "luminance_edge" => {
            let p: LuminanceEdgeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::LuminanceEdge {
                strength: range(p.strength, 0.0, 16.0, format!("{base}.strength"))?,
            }
        }
        "frame_out" => empty!(NormalizedParameters::FrameOut),
        "texture" => {
            let p: TextureParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if matches!(
                p.residency,
                TextureResidency::History | TextureResidency::Readback
            ) {
                return Err(error(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "history and readback textures are unsupported",
                    format!("{base}.residency"),
                ));
            }
            let unsupported = |suffix: &str| {
                error(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "texture feature is unsupported",
                    format!("{base}.texture.{suffix}"),
                )
            };
            if p.texture.dimension != TextureDimension::D2 {
                return Err(unsupported("dimension"));
            }
            if p.texture.mip_level_count != 1 {
                return Err(unsupported("mipLevelCount"));
            }
            if p.texture.sample_count != 1 {
                return Err(unsupported("sampleCount"));
            }
            let depth_or_array_layers = match &p.texture.extent {
                TextureExtent::Absolute {
                    depth_or_array_layers,
                    ..
                }
                | TextureExtent::SurfaceRelative {
                    depth_or_array_layers,
                    ..
                } => *depth_or_array_layers,
            };
            if depth_or_array_layers != 1 {
                return Err(unsupported("extent.depthOrArrayLayers"));
            }
            NormalizedParameters::Texture {
                residency: p.residency,
                descriptor: normalize_texture(p.texture, &base)?,
            }
        }
        "mesh_query" => {
            let p: QueryParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            let fold = |predicate, default, linked| match (predicate, linked, default) {
                (TriStatePredicate::Any, _, _) => RuntimePredicate::Any,
                (TriStatePredicate::RequiredTrue, true, _) => RuntimePredicate::RequiredTrue,
                (TriStatePredicate::RequiredFalse, true, _) => RuntimePredicate::RequiredFalse,
                (TriStatePredicate::RequiredTrue, false, true)
                | (TriStatePredicate::RequiredFalse, false, false) => RuntimePredicate::Any,
                _ => RuntimePredicate::Never,
            };
            let mut visible = fold(
                p.visible_predicate,
                p.visible_default,
                node.inputs.contains_key("isVisible"),
            );
            let mut culled = fold(
                p.frustum_culled_predicate,
                p.frustum_culled_default,
                node.inputs.contains_key("isFrustumCulled"),
            );
            if visible == RuntimePredicate::Never || culled == RuntimePredicate::Never {
                visible = RuntimePredicate::Never;
                culled = RuntimePredicate::Never;
            }
            NormalizedParameters::MeshQuery {
                visible_predicate: visible,
                frustum_culled_predicate: culled,
            }
        }
        "pipeline_registry" => empty!(NormalizedParameters::PipelineRegistry),
        "pipeline" => {
            let p: PipelineParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            let valid_name = !p.pipeline.is_empty()
                && p.pipeline.len() <= 64
                && p.pipeline.bytes().enumerate().all(|(i, c)| {
                    c == b'_' || c.is_ascii_alphanumeric() && (i > 0 || c.is_ascii_alphabetic())
                });
            if !valid_name {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "pipeline must be a 1-64 byte identifier",
                    format!("{base}.pipeline"),
                ));
            }
            if !p.clear_depth.is_finite() || !(0.0..=1.0).contains(&p.clear_depth) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearDepth must be finite and in [0,1]",
                    format!("{base}.clearDepth"),
                ));
            }
            if p.clear_color.iter().any(|x| !x.is_finite()) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearColor must be finite",
                    format!("{base}.clearColor"),
                ));
            }
            NormalizedParameters::Pipeline {
                pipeline: p.pipeline,
                depth_compare: p.depth_compare,
                depth_write_enabled: p.depth_write_enabled,
                clear_depth: p.clear_depth,
                clear_color: p.clear_color,
            }
        }
        _ => unreachable!(),
    })
}

fn accepts(c: TypeConstraint, ty: SemanticType) -> bool {
    match c {
        TypeConstraint::Exact(x) => x == ty,
        TypeConstraint::OneOf(xs) => xs.contains(&ty),
    }
}

pub fn compile(graph: Graph) -> Result<CompiledGraph, GraphError> {
    if graph.nodes.len() > MAX_EXECUTIONS {
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
    if graph
        .nodes
        .iter()
        .filter(|node| node.executor.key == "frame_out" && node.state == NodeState::Enabled)
        .count()
        != 1
    {
        return Err(error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "exactly one frame_out is required",
            "nodes",
        ));
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.state != NodeState::Enabled && n.executor.key != "frame_out" {
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
            let inactive = matches!(&params[i], NormalizedParameters::MeshQuery { visible_predicate, frustum_culled_predicate } if (input.name == "isVisible" && matches!(visible_predicate, RuntimePredicate::Any | RuntimePredicate::Never)) || (input.name == "isFrustumCulled" && matches!(frustum_culled_predicate, RuntimePredicate::Any | RuntimePredicate::Never)));
            if !n.inputs.contains_key(input.name) {
                if input.cardinality == InputCardinality::RequiredOne
                    || (!inactive
                        && matches!(params[i], NormalizedParameters::MeshQuery { .. })
                        && input.name != "mesh")
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
            let inactive = matches!(&params[i], NormalizedParameters::MeshQuery { visible_predicate, frustum_culled_predicate } if (input.name == "isVisible" && matches!(visible_predicate, RuntimePredicate::Any | RuntimePredicate::Never)) || (input.name == "isFrustumCulled" && matches!(frustum_culled_predicate, RuntimePredicate::Any | RuntimePredicate::Never)));
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
            if !accepts(input.accepted, out.semantic_type) {
                return Err(error(
                    "GRAPH_SOCKET_TYPE_MISMATCH",
                    "socket type mismatch",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
            if let Some(flag) = MeshFlag::ORDERED
                .iter()
                .find(|f| f.input_socket() == input.name)
            {
                if out.metadata != (OutputMetadata::BooleanFlag { flag: *flag }) {
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
                contracts: &Vec<&Contract>|
     -> Option<OutputKey> {
        let mut k = key;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(k.0) {
                return None;
            }
            if contracts[k.0].key == "mesh" {
                let ordinal = contracts[k.0]
                    .outputs
                    .iter()
                    .position(|output| output.semantic_type == SemanticType::MeshData)?;
                return Some(OutputKey(k.0, ordinal as u16));
            }
            if contracts[k.0].outputs[k.1 as usize].semantic_type == SemanticType::MeshData {
                return Some(k);
            }
            k = bound[k.0]
                .get("mesh")
                .or_else(|| bound[k.0].get("pipelineIndices"))
                .or_else(|| bound[k.0].get("activation"))?
                .producer;
        }
    };
    for (i, c) in contracts.iter().enumerate() {
        if c.key == "frustum_cull"
            && root(bound[i]["mesh"].producer, &bound, &contracts)
                != root(bound[i]["localAabbs"].producer, &bound, &contracts)
        {
            return Err(error(
                "GRAPH_SOCKET_TYPE_MISMATCH",
                "scene roots differ",
                format!("nodes[{i}].inputs.localAabbs"),
            ));
        }
        if matches!(c.key, "mesh_query" | "pipeline_registry" | "pipeline") {
            let scene_socket = if c.key == "pipeline_registry" {
                "pipelineIndices"
            } else {
                "mesh"
            };
            let scene = root(bound[i][scene_socket].producer, &bound, &contracts);
            for (s, b) in &bound[i] {
                if b.active
                    && matches!(
                        *s,
                        "isVisible"
                            | "isFrustumCulled"
                            | "draws"
                            | "pipelineIndices"
                            | "activation"
                    )
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
        .filter(|(i, c)| c.inherently_observable && graph.nodes[*i].state == NodeState::Enabled)
        .map(|(i, _)| i)
        .collect();
    while let Some(i) = stack.pop() {
        if live.insert(i) {
            stack.extend(deps[i].iter().copied());
        }
    }
    // IDs are independent of scheduling: original node order, then contract output order.
    // Source nodes expose only outputs that survived active-edge/liveness analysis;
    // executable nodes retain their complete output shape for runtime lowering.
    let referenced_outputs: HashSet<_> = edges
        .iter()
        .filter(|edge| live.contains(&edge.to_node))
        .map(|edge| OutputKey(edge.from_node, edge.producer_output_ordinal))
        .collect();
    let mut output_ids = BTreeMap::new();
    let mut resource_meta = Vec::new();
    for i in 0..graph.nodes.len() {
        if live.contains(&i) {
            for (o, out) in contracts[i].outputs.iter().enumerate() {
                let key = OutputKey(i, o as u16);
                if contracts[i].execution == ExecutionClass::Source
                    && !referenced_outputs.contains(&key)
                {
                    continue;
                }
                let id = resource_meta.len() as u32;
                output_ids.insert(key, id);
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
            NormalizedParameters::Texture {
                residency,
                descriptor,
            } => {
                let id = families.len() as u32;
                let r = source.unwrap();
                source_family.insert(OutputKey(i, 0), id);
                families.push(TextureFamily {
                    id,
                    key: TextureFamilyKey {
                        source_node: i as u32,
                        source_socket: 0,
                    },
                    source: TextureFamilySource::AuthoredTexture {
                        resource: r,
                        residency: *residency,
                        descriptor: descriptor.clone(),
                    },
                    lifetime: Lifetime {
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
            "pipeline" => &[("colorTarget", 0), ("depthTarget", 1)],
            _ if contracts[i].fullscreen_policy.is_some() => &[("colorTarget", 0)],
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
        for input in contract
            .inputs
            .iter()
            .filter(|input| matches!(input.role, InputRole::SampledTexture))
        {
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
        if contracts[i].key == "pipeline" {
            if bound[i]["colorTarget"].producer == bound[i]["depthTarget"].producer
                || matches!((version_of.get(&OutputKey(i, 0)), version_of.get(&OutputKey(i, 1))), (Some((cf, _, _)), Some((df, _, _))) if cf == df)
            {
                return Err(error(
                    "GRAPH_SAME_PASS_HAZARD",
                    "color and depth use one texture family",
                    format!("nodes[{i}].inputs"),
                ));
            }
        } else if contracts[i].fullscreen_policy.is_some() {
            let hazard = contracts[i].inputs.iter().filter(|input| matches!(input.role, InputRole::SampledTexture)).any(|input| matches!((version_of.get(&bound[i][input.name].producer), version_of.get(&OutputKey(i, 0))), (Some((sf, _, _)), Some((tf, _, _))) if sf == tf));
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
            families[family as usize].versions.push(TextureVersion {
                version,
                resource: output_ids[&transition.output],
                target,
                initialized: true,
                stored: true,
                lifetime: Lifetime {
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
        if !live.contains(&i) || contracts[i].key != "pipeline" {
            continue;
        }
        let (Some(&(cf, _, _)), Some(&(df, _, _))) = (
            version_of.get(&OutputKey(i, 0)),
            version_of.get(&OutputKey(i, 1)),
        ) else {
            continue;
        };
        let TextureFamilySource::AuthoredTexture { descriptor: cd, .. } =
            &families[cf as usize].source;
        let TextureFamilySource::AuthoredTexture { descriptor: dd, .. } =
            &families[df as usize].source;
        let ok_depth = dd.dimension == TextureDimension::D2
            && dd.format == TextureFormat::Depth32Float
            && dd.sample_count == 1
            && extent_layers(&dd.extent) == 1;
        let ok_color = cd.format != TextureFormat::Depth32Float
            && cd.dimension == dd.dimension
            && cd.extent == dd.extent
            && cd.sample_count == 1;
        if !ok_depth || !ok_color {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "attachments are incompatible",
                format!("nodes[{i}].inputs"),
            ));
        }
    }

    for i in 0..graph.nodes.len() {
        if !live.contains(&i) || contracts[i].fullscreen_policy.is_none() {
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
            TextureFamilySource::AuthoredTexture { descriptor, .. } => descriptor,
        };
        let source_ok = source_descriptor.format == TextureFormat::Rgba16Float
            && is_single_view_d2(source_descriptor);
        let target_descriptor = match &families[target_family_id as usize].source {
            TextureFamilySource::AuthoredTexture { descriptor, .. } => Some(descriptor),
        };
        let authored_target_ok = target_descriptor.is_some_and(|descriptor| {
            descriptor.format == TextureFormat::Rgba16Float && is_single_view_d2(descriptor)
        });
        let bloom_input_ok = if contracts[i].fullscreen_policy
            == Some(FullscreenPolicy::BloomComposite)
        {
            let bloom_key = bound[i]["bloom"].producer;
            let Some(&(bloom_family_id, _, _)) = version_of.get(&bloom_key) else {
                return Err(error(
                    "GRAPH_UNINITIALIZED_RESOURCE",
                    "bloom source is not produced",
                    format!("nodes[{i}].inputs.bloom"),
                ));
            };
            match &families[bloom_family_id as usize].source {
                TextureFamilySource::AuthoredTexture { descriptor, .. } => {
                    descriptor.format == TextureFormat::Rgba16Float && is_single_view_d2(descriptor)
                }
            }
        } else {
            true
        };
        let source_is_full_surface = matches!(&source_descriptor.extent, NormalizedTextureExtent::SurfaceRelative { width, height, depth_or_array_layers: 1 } if *width == Ratio { numerator:1, denominator:1 } && *height == Ratio { numerator:1, denominator:1 });
        let target_matches_source = target_descriptor
            .is_some_and(|descriptor| descriptor.extent == source_descriptor.extent);
        let descriptor_ok = match contracts[i].fullscreen_policy {
            Some(FullscreenPolicy::Copy) => {
                target_descriptor.is_none() && source_is_full_surface
                    || target_descriptor.is_some_and(|descriptor| {
                        descriptor.format != TextureFormat::Depth32Float
                            && is_single_view_d2(descriptor)
                            && descriptor.extent == source_descriptor.extent
                    })
            }
            Some(FullscreenPolicy::ToneMap) => target_descriptor.is_some_and(|descriptor| {
                descriptor.format != TextureFormat::Depth32Float
                    && descriptor.format != TextureFormat::R32Float
                    && is_single_view_d2(descriptor)
                    && descriptor.extent == source_descriptor.extent
            }),
            Some(FullscreenPolicy::BloomExtract) => authored_target_ok,
            Some(FullscreenPolicy::HdrSameExtent) => authored_target_ok && target_matches_source,
            Some(FullscreenPolicy::BloomComposite) => {
                authored_target_ok && target_matches_source && bloom_input_ok
            }
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

    // Frame output must consume an initialized, produced, filterable color texture.
    for (i, contract) in contracts.iter().enumerate() {
        if !live.contains(&i) || contract.key != "frame_out" {
            continue;
        }
        let key = bound[i]["color"].producer;
        let Some(&(family, _, _)) = version_of.get(&key) else {
            if !matches!(resolved.get(&key), Some(ResolvedTransition::Cyclic)) {
                return Err(error(
                    "GRAPH_UNINITIALIZED_RESOURCE",
                    "frame output source is not produced",
                    format!("nodes[{i}].inputs.color"),
                ));
            }
            continue;
        };
        let TextureFamilySource::AuthoredTexture { descriptor, .. } =
            &families[family as usize].source;
        if !is_filterable_frame_color(descriptor) {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "frame output requires a filterable single-view d2 color texture",
                format!("nodes[{i}].inputs.color"),
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
        let mesh = || output_ids[&root(key, &bound, &contracts).unwrap()];
        let plan = match out.semantic_type {
            SemanticType::Texture if matches!(params[i], NormalizedParameters::Texture { .. }) => {
                if let NormalizedParameters::Texture {
                    residency,
                    descriptor,
                } = &params[i]
                {
                    ResourcePlan::TextureSource {
                        family: source_family[&key],
                        residency: *residency,
                        descriptor: descriptor.clone(),
                    }
                } else {
                    unreachable!()
                }
            }
            SemanticType::Texture => {
                let (f, v, t) = version_of[&key];
                ResourcePlan::Texture {
                    family: f,
                    version: v,
                    target: t,
                    initialized: true,
                    stored: true,
                    allocation: None,
                }
            }
            SemanticType::MeshData => ResourcePlan::MeshData,
            SemanticType::LocalAabbBuffer => ResourcePlan::LocalAabbBuffer { mesh: mesh() },
            SemanticType::BooleanFlagBuffer => {
                if let OutputMetadata::BooleanFlag { flag } = out.metadata {
                    ResourcePlan::BooleanFlagBuffer { mesh: mesh(), flag }
                } else {
                    unreachable!()
                }
            }
            SemanticType::PipelineIndexStream => ResourcePlan::PipelineIndexStream { mesh: mesh() },
            SemanticType::PipelineActivation => ResourcePlan::PipelineActivation {
                pipeline_indices: output_ids[&bound[i]["pipelineIndices"].producer],
            },
            SemanticType::DrawStream => ResourcePlan::DrawStream { mesh: mesh() },
        };
        resources.push(CompiledResource {
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
        if contracts[i].execution == ExecutionClass::Source {
            continue;
        }
        let ordinal = executions.len() as u32;
        node_execution.insert(i, ordinal);
        let input_resource = |s: &str| output_ids[&bound[i][s].producer];
        let mut inputs = Vec::new();
        for s in contracts[i].inputs {
            if let Some(b) = bound[i].get(s.name).filter(|b| b.active) {
                inputs.push(CompiledSocketInput {
                    socket: s.name.into(),
                    resource: output_ids[&b.producer],
                });
            }
        }
        let outputs: Vec<_> = contracts[i]
            .outputs
            .iter()
            .enumerate()
            .map(|(o, s)| CompiledSocketOutput {
                socket: s.name.into(),
                resource: output_ids[&OutputKey(i, o as u16)],
            })
            .collect();
        let mut accesses = Vec::new();
        let kind = match contracts[i].key {
            "frustum_cull" => {
                for (s, m) in [
                    ("mesh", AccessMode::StorageRead),
                    ("localAabbs", AccessMode::StorageRead),
                ] {
                    accesses.push(CompiledAccess {
                        socket: s.into(),
                        resource: input_resource(s),
                        mode: m,
                    });
                }
                let r = output_ids[&OutputKey(i, 0)];
                accesses.push(CompiledAccess {
                    socket: "isFrustumCulled".into(),
                    resource: r,
                    mode: AccessMode::StorageWrite {
                        full_overwrite: true,
                    },
                });
                ExecutionKind::Compute {
                    work: ComputeWork::FrustumCull,
                }
            }
            "mesh_query" => {
                for s in ["mesh", "isVisible", "isFrustumCulled"] {
                    if let Some(b) = bound[i].get(s).filter(|b| b.active) {
                        accesses.push(CompiledAccess {
                            socket: s.into(),
                            resource: output_ids[&b.producer],
                            mode: AccessMode::StorageRead,
                        });
                    }
                }
                accesses.push(CompiledAccess {
                    socket: "draws".into(),
                    resource: output_ids[&OutputKey(i, 0)],
                    mode: AccessMode::StorageWrite {
                        full_overwrite: true,
                    },
                });
                ExecutionKind::Compute {
                    work: ComputeWork::MeshQuery,
                }
            }
            "pipeline_registry" => {
                accesses.push(CompiledAccess {
                    socket: "pipelineIndices".into(),
                    resource: input_resource("pipelineIndices"),
                    mode: AccessMode::SemanticRead,
                });
                ExecutionKind::CpuPreparation
            }
            "pipeline" => {
                let color = output_ids[&OutputKey(i, 0)];
                let depth = output_ids[&OutputKey(i, 1)];
                let clear = match params[i] {
                    NormalizedParameters::Pipeline { clear_color, .. } => clear_color,
                    _ => unreachable!(),
                };
                let clear_depth = match &params[i] {
                    NormalizedParameters::Pipeline { clear_depth, .. } => *clear_depth,
                    _ => unreachable!(),
                };
                let first_color = version_of[&OutputKey(i, 0)].1 == 0;
                let first_depth = version_of[&OutputKey(i, 1)].1 == 0;
                let cl = if first_color {
                    NormalizedColorLoad::Clear { value: clear }
                } else {
                    NormalizedColorLoad::Load
                };
                let dl = if first_depth {
                    NormalizedDepthLoad::Clear { value: clear_depth }
                } else {
                    NormalizedDepthLoad::Load
                };
                for s in ["mesh", "draws", "activation"] {
                    accesses.push(CompiledAccess {
                        socket: s.into(),
                        resource: input_resource(s),
                        mode: if s == "draws" {
                            AccessMode::IndirectRead
                        } else {
                            AccessMode::SemanticRead
                        },
                    });
                }
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessMode::ColorAttachment {
                        location: 0,
                        load: cl,
                        store: StoreOp::Store,
                        full_overwrite: first_color,
                    },
                });
                accesses.push(CompiledAccess {
                    socket: "depth".into(),
                    resource: depth,
                    mode: AccessMode::DepthAttachment {
                        load: dl,
                        store: StoreOp::Store,
                        full_overwrite: first_depth,
                    },
                });
                ExecutionKind::Render {
                    color_attachments: vec![ColorAttachmentPlan {
                        resource: color,
                        location: 0,
                        load: cl,
                        store: StoreOp::Store,
                    }],
                    depth_stencil: Some(DepthStencilAttachmentPlan {
                        resource: depth,
                        load: dl,
                        store: StoreOp::Store,
                    }),
                }
            }
            _ if contracts[i].fullscreen_policy.is_some() => {
                let color = output_ids[&OutputKey(i, 0)];
                let load = NormalizedColorLoad::Clear {
                    value: [0.0, 0.0, 0.0, 0.0],
                };
                for input in contracts[i]
                    .inputs
                    .iter()
                    .filter(|input| input.role == InputRole::SampledTexture)
                {
                    accesses.push(CompiledAccess {
                        socket: input.name.into(),
                        resource: input_resource(input.name),
                        mode: AccessMode::SampledTexture,
                    });
                }
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessMode::ColorAttachment {
                        location: 0,
                        load,
                        store: StoreOp::Store,
                        full_overwrite: true,
                    },
                });
                ExecutionKind::Render {
                    color_attachments: vec![ColorAttachmentPlan {
                        resource: color,
                        location: 0,
                        load,
                        store: StoreOp::Store,
                    }],
                    depth_stencil: None,
                }
            }
            "frame_out" => {
                let r = input_resource("color");
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: r,
                    mode: AccessMode::SampledTexture,
                });
                ExecutionKind::FrameOut { color: r }
            }
            _ => unreachable!(),
        };
        executions.push(CompiledExecution {
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
            let life = resources[r as usize].lifetime.get_or_insert(Lifetime {
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
        f.lifetime = Lifetime {
            first_use: first.unwrap_or(0),
            last_use: last,
        };
        f.usage = texture_usage(f, &executions);
        f.aliasable = matches!(
            f.source,
            TextureFamilySource::AuthoredTexture {
                residency: TextureResidency::Transient,
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
    Ok(CompiledGraph {
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

fn extent_layers(e: &NormalizedTextureExtent) -> u32 {
    match e {
        NormalizedTextureExtent::Absolute {
            depth_or_array_layers,
            ..
        }
        | NormalizedTextureExtent::SurfaceRelative {
            depth_or_array_layers,
            ..
        } => *depth_or_array_layers,
    }
}
pub(super) fn is_single_view_d2(descriptor: &NormalizedTextureDescriptor) -> bool {
    descriptor.dimension == TextureDimension::D2
        && descriptor.sample_count == 1
        && descriptor.mip_level_count == 1
        && extent_layers(&descriptor.extent) == 1
}
pub(super) fn is_filterable_frame_color(descriptor: &NormalizedTextureDescriptor) -> bool {
    is_single_view_d2(descriptor)
        && matches!(
            descriptor.format,
            TextureFormat::Rgba8Unorm
                | TextureFormat::Rgba8UnormSrgb
                | TextureFormat::Bgra8Unorm
                | TextureFormat::Bgra8UnormSrgb
                | TextureFormat::Rgba16Float
        )
}
pub(super) fn texture_usage(
    f: &TextureFamily,
    executions: &[CompiledExecution],
) -> Vec<TextureUsage> {
    let rs: HashSet<_> = f.versions.iter().map(|v| v.resource).collect();
    let mut u = BTreeSet::new();
    for e in executions {
        for a in &e.accesses {
            if !rs.contains(&a.resource) {
                continue;
            }
            match a.mode {
                AccessMode::SampledTexture => {
                    u.insert(TextureUsage::Sampled);
                }
                AccessMode::StorageRead | AccessMode::StorageWrite { .. } => {
                    u.insert(TextureUsage::Storage);
                }
                AccessMode::ColorAttachment { .. } => {
                    u.insert(TextureUsage::ColorAttachment);
                }
                AccessMode::DepthAttachment { .. } => {
                    u.insert(TextureUsage::DepthAttachment);
                }
                _ => {}
            }
        }
    }
    u.into_iter().collect()
}
fn allocate(
    families: &mut [TextureFamily],
    resources: &mut [CompiledResource],
) -> (Vec<AllocationClass>, u32) {
    let mut grouped: BTreeMap<TextureCompatibilityKey, Vec<usize>> = BTreeMap::new();
    for (i, f) in families.iter().enumerate() {
        let TextureFamilySource::AuthoredTexture { descriptor, .. } = &f.source;
        grouped
            .entry(TextureCompatibilityKey {
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
    let mut classes = Vec::new();
    let mut transient = 0;
    for (key, ids) in grouped {
        let class = classes.len() as u32;
        let mut slots: Vec<AllocationSlot> = Vec::new();
        let mut aliasable = Vec::new();
        let mut dedicated = Vec::new();
        let mut persistent_ids = Vec::new();
        for fi in ids {
            let persistent = matches!(
                families[fi].source,
                TextureFamilySource::AuthoredTexture {
                    residency: TextureResidency::Persistent,
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
                TextureFamilySource::AuthoredTexture {
                    residency: TextureResidency::Persistent,
                    ..
                }
            );
            let alias = families[fi].aliasable && !persistent;
            let found = if alias {
                slots.iter().position(|s| {
                    s.kind == AllocationKind::AliasedTransient
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
                slots.push(AllocationSlot {
                    kind: if persistent {
                        AllocationKind::Persistent
                    } else if alias {
                        AllocationKind::AliasedTransient
                    } else {
                        AllocationKind::DedicatedTransient
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
            let a = AllocationRef {
                class,
                slot: slot as u32,
            };
            families[fi].allocation = Some(a);
            for v in &families[fi].versions {
                if let ResourcePlan::Texture { allocation, .. } =
                    &mut resources[v.resource as usize].plan
                {
                    *allocation = Some(a);
                }
            }
        }
        classes.push(AllocationClass { key, slots });
    }
    (classes, transient)
}
