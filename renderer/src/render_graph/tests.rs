use std::collections::BTreeSet;

use super::*;
use serde_json::{json, Value};

fn input(node: &str, socket: &str) -> Value {
    json!({"node":node,"socket":socket})
}
fn node(id: &str, key: &str, mut parameters: Value, inputs: Value) -> Value {
    if key == "frame_out" && parameters.as_object().is_some_and(|p| p.is_empty()) {
        parameters = json!({"hdrEnabled":false,"toneMapper":"aces","exposureStops":0,"outputTransfer":"srgb","scaleMode":"stretch","filter":"linear","backgroundColor":[0,0,0,1]});
    }
    json!({"id":id,"state":"enabled","executor":{"key":key,"version":if key == "frame_out" { 2 } else { 1 }},"parameters":parameters,"inputs":inputs})
}
fn texture(format: &str, residency: &str) -> Value {
    json!({"texture":{"dimension":"d2","format":format,"extent":{"kind":"surface_relative","width":{"numerator":1,"denominator":1},"height":{"numerator":1,"denominator":1},"depthOrArrayLayers":1},"mipLevelCount":1,"sampleCount":1,"viewFormats":[]},"residency":residency})
}
pub(crate) fn full_cull_graph() -> Value {
    json!({"schemaVersion":2,"graphId":"full","revision":1,"nodes":[
        node("color","texture",texture("rgba8_unorm","transient"),json!({})),
        node("depth","texture",texture("depth32_float","transient"),json!({})),
        node("mesh","mesh",json!({}),json!({})),
        node("cull","frustum_cull",json!({"camera":"active"}),json!({"mesh":input("mesh","mesh"),"localAabbs":input("mesh","localAabbs")})),
        node("query","mesh_query",json!({"visiblePredicate":"required_true","visibleDefault":true,"frustumCulledPredicate":"required_false","frustumCulledDefault":false}),json!({"mesh":input("mesh","mesh"),"isVisible":input("mesh","isVisible"),"isFrustumCulled":input("cull","isFrustumCulled")})),
        node("registry","pipeline_registry",json!({}),json!({"pipelineIndices":input("mesh","pipelineIndices")})),
        node("pipeline_main","pipeline",json!({"pipeline":"gltf_standard","depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0,"clearColor":[0,0,0,1]}),json!({"mesh":input("mesh","mesh"),"draws":input("query","draws"),"activation":input("registry","activation"),"colorTarget":input("color","texture"),"depthTarget":input("depth","texture")})),
        node("frame_out","frame_out",json!({}),json!({"color":input("pipeline_main","color")}))
    ]})
}
fn pipeline_node(id: &str, color: Value, depth: Value) -> Value {
    node(
        id,
        "pipeline",
        json!({"pipeline":"gltf_standard","depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0,"clearColor":[0,0,0,1]}),
        json!({
            "mesh":input("mesh","mesh"),
            "draws":input("query","draws"),
            "colorTarget":color,
            "depthTarget":depth,
            "activation":input("registry","activation")
        }),
    )
}
fn render_support_nodes() -> Vec<Value> {
    vec![
        node("mesh", "mesh", json!({}), json!({})),
        node(
            "query",
            "mesh_query",
            json!({"visiblePredicate":"required_true","visibleDefault":true,"frustumCulledPredicate":"any","frustumCulledDefault":false}),
            json!({"mesh":input("mesh","mesh"),"isVisible":input("mesh","isVisible")}),
        ),
        node(
            "registry",
            "pipeline_registry",
            json!({}),
            json!({"pipelineIndices":input("mesh","pipelineIndices")}),
        ),
    ]
}
fn graph(nodes: Vec<Value>) -> Value {
    json!({"schemaVersion":2,"graphId":"hazards","revision":1,"nodes":nodes})
}
fn node_index(graph: &Value, id: &str) -> usize {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|node| node["id"] == id)
        .unwrap()
}
fn node_path(graph: &Value, id: &str, suffix: &str) -> String {
    format!("nodes[{}].{suffix}", node_index(graph, id))
}
fn hdr_copy_graph() -> Value {
    let mut nodes = vec![
        node(
            "color",
            "texture",
            texture("rgba8_unorm", "transient"),
            json!({}),
        ),
        node(
            "hdr",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        depth_spec("depth", "transient"),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        pipeline_node(
            "pipeline_main",
            input("hdr", "texture"),
            input("depth", "texture"),
        ),
        node(
            "copy",
            "fullscreen_copy",
            json!({}),
            json!({"source":input("pipeline_main","color"),"colorTarget":input("color","texture")}),
        ),
        node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":input("copy","color")}),
        ),
    ]);
    graph(nodes)
}

fn bloom_composite_graph() -> Value {
    let mut half = texture("rgba16_float", "transient");
    half["texture"]["extent"]["width"] = json!({"numerator":1,"denominator":2});
    half["texture"]["extent"]["height"] = json!({"numerator":1,"denominator":2});
    let mut bloom_depth = texture("depth32_float", "transient");
    bloom_depth["texture"]["extent"] = half["texture"]["extent"].clone();
    let mut nodes = vec![
        node(
            "color",
            "texture",
            texture("rgba8_unorm", "transient"),
            json!({}),
        ),
        node(
            "source",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        node("bloom", "texture", half, json!({})),
        node(
            "target",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        depth_spec("source_depth", "transient"),
        node("bloom_depth", "texture", bloom_depth, json!({})),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        pipeline_node(
            "source_writer",
            input("source", "texture"),
            input("source_depth", "texture"),
        ),
        pipeline_node(
            "bloom_writer",
            input("bloom", "texture"),
            input("bloom_depth", "texture"),
        ),
        node(
            "composite",
            "bloom_composite",
            json!({"intensity":1.0}),
            json!({"source":input("source_writer","color"),"bloom":input("bloom_writer","color"),"colorTarget":input("target","texture")}),
        ),
        node(
            "to_surface",
            "fullscreen_copy",
            json!({}),
            json!({"source":input("composite","color"),"colorTarget":input("color","texture")}),
        ),
        node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":input("to_surface","color")}),
        ),
    ]);
    graph(nodes)
}

fn cyclic_pipelines() -> Vec<Value> {
    vec![
        pipeline_node("A", input("B", "color"), input("B", "depth")),
        pipeline_node("B", input("A", "color"), input("A", "depth")),
    ]
}
fn compile_graph(v: Value) -> CompiledGraph {
    super::compile(serde_json::from_value(v).unwrap()).unwrap()
}

#[test]
fn fullscreen_copy_hdr_graph_lowers_versions_accesses_and_usage() {
    let p = compile_graph(hdr_copy_graph());
    assert_eq!(
        p.executions
            .iter()
            .map(|execution| execution.id.as_str())
            .collect::<Vec<_>>(),
        ["query", "registry", "pipeline_main", "copy", "frame_out"]
    );
    for (node, socket) in [
        ("pipeline_main", "color"),
        ("pipeline_main", "depth"),
        ("copy", "color"),
    ] {
        assert!(matches!(
            resource_by_origin(&p, node, socket).plan,
            ResourcePlan::Texture { version: 0, .. }
        ));
    }
    let copy = execution(&p, "copy");
    let source = resource_by_origin(&p, "pipeline_main", "color");
    let color = resource_by_origin(&p, "copy", "color");
    assert!(copy.accesses.iter().any(|access| access.resource
        == p.resources
            .iter()
            .position(|resource| std::ptr::eq(resource, source))
            .unwrap() as u32
        && access.mode == AccessMode::SampledTexture));
    let color_id = p
        .resources
        .iter()
        .position(|resource| std::ptr::eq(resource, color))
        .unwrap() as u32;
    assert!(matches!(
        &copy.kind,
        ExecutionKind::Render { color_attachments, depth_stencil: None }
            if color_attachments[0].resource == color_id
                && color_attachments[0].load == NormalizedColorLoad::Clear { value: [0.0; 4] }
    ));
    assert!(copy.accesses.iter().any(|access| matches!(
        access.mode,
        AccessMode::ColorAttachment {
            full_overwrite: true,
            ..
        }
    ) && access.resource == color_id));
    let hdr = family_by_source(&p, "hdr");
    assert_eq!(
        hdr.usage.iter().copied().collect::<BTreeSet<_>>(),
        [TextureUsage::Sampled, TextureUsage::ColorAttachment]
            .into_iter()
            .collect()
    );
    assert!(p
        .texture_families
        .iter()
        .all(|family| matches!(family.source, TextureFamilySource::AuthoredTexture { .. })));
}

#[test]
fn fullscreen_copy_parameters_are_exactly_empty() {
    let mut g = hdr_copy_graph();
    let copy = node_index(&g, "copy");
    g["nodes"][copy]["parameters"] = json!({"obsolete":true});
    assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    assert_eq!(CONTRACTS.len(), 16);
}

#[test]
fn fullscreen_copy_rejects_same_source_and_target_family() {
    let mut g = hdr_copy_graph();
    let copy = node_index(&g, "copy");
    let path = node_path(&g, "copy", "inputs");
    g["nodes"][copy]["inputs"]["colorTarget"] = input("pipeline_main", "color");
    let error = compile_error(g);
    assert_eq!(error.code, "GRAPH_SAME_PASS_HAZARD");
    assert_eq!(error.details["path"], path);
}

#[test]
fn duplicate_texture_writer_reports_second_color_target() {
    let mut nodes = vec![
        node(
            "color",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        node(
            "output",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        node(
            "depth_a",
            "texture",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        node(
            "depth_b",
            "texture",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        pipeline_node("F0", input("color", "texture"), input("depth_a", "texture")),
        pipeline_node("F1", input("color", "texture"), input("depth_b", "texture")),
        node(
            "join",
            "bloom_composite",
            json!({"intensity":1.0}),
            json!({"source":input("F0","color"),"bloom":input("F1","color"),"colorTarget":input("output","texture")}),
        ),
        node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":input("join","color")}),
        ),
    ]);
    let graph = graph(nodes);
    let path = node_path(&graph, "F1", "inputs.colorTarget");
    let error = compile_error(graph);
    assert_eq!(error.code, "GRAPH_DUPLICATE_WRITER");
    assert_eq!(error.details["path"], path);
}

#[test]
fn same_output_bound_to_both_attachments_is_a_same_pass_hazard() {
    let mut nodes = vec![node(
        "target",
        "texture",
        texture("rgba8_unorm", "transient"),
        json!({}),
    )];
    nodes.extend(render_support_nodes());
    nodes.extend([
        pipeline_node("F", input("target", "texture"), input("target", "texture")),
        node(
            "P",
            "frame_out",
            json!({}),
            json!({"color":input("F","color")}),
        ),
    ]);
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_SAME_PASS_HAZARD");
    assert_eq!(error.details["path"], "nodes[4].inputs");
}

#[test]
fn unordered_old_texture_version_read_is_rejected_before_scheduling() {
    let mut nodes = vec![
        node(
            "color",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        node(
            "output",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        node(
            "depth_0",
            "texture",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        node(
            "depth_1",
            "texture",
            texture("depth32_float", "transient"),
            json!({}),
        ),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        pipeline_node("F0", input("color", "texture"), input("depth_0", "texture")),
        pipeline_node("F1", input("F0", "color"), input("depth_1", "texture")),
        node(
            "join",
            "bloom_composite",
            json!({"intensity":1.0}),
            json!({"source":input("F0","color"),"bloom":input("F1","color"),"colorTarget":input("output","texture")}),
        ),
        node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":input("join","color")}),
        ),
    ]);
    let graph = graph(nodes);
    let path = node_path(&graph, "join", "inputs.source");
    let error = compile_error(graph);
    assert_eq!(error.code, "GRAPH_RESOURCE_VERSION_INVALID");
    assert_eq!(error.details["path"], path);
}

#[test]
fn duplicate_successors_defer_old_version_reachability() {
    let mut nodes = vec![
        node(
            "color",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        node(
            "output",
            "texture",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        depth_spec("depth_0", "transient"),
        depth_spec("depth_1", "transient"),
        depth_spec("depth_2", "transient"),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        pipeline_node("F0", input("color", "texture"), input("depth_0", "texture")),
        pipeline_node("F1", input("F0", "color"), input("depth_1", "texture")),
        pipeline_node("F2", input("F0", "color"), input("depth_2", "texture")),
        node(
            "join",
            "bloom_composite",
            json!({"intensity":1.0}),
            json!({"source":input("F1","color"),"bloom":input("F2","color"),"colorTarget":input("output","texture")}),
        ),
        node(
            "P2",
            "frame_out",
            json!({}),
            json!({"color":input("join","color")}),
        ),
    ]);
    let graph = graph(nodes);
    let path = node_path(&graph, "F2", "inputs.colorTarget");
    let error = compile_error(graph);
    assert_eq!(error.code, "GRAPH_DUPLICATE_WRITER");
    assert_eq!(error.details["path"], path);
}

#[test]
fn live_texture_cycle_reports_the_exact_first_cycle() {
    let mut nodes = render_support_nodes();
    nodes.extend(cyclic_pipelines());
    nodes.push(node(
        "frame_out",
        "frame_out",
        json!({}),
        json!({"color":input("A","color")}),
    ));
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_CYCLE");
    assert_eq!(
        error.details,
        json!({
            "message":"live graph contains a cycle",
            "kind":"cycle",
            "edges":[
                {"fromNode":"A","fromSocket":"color","toNode":"B","toSocket":"colorTarget","resource":{"node":"A","socket":"color"}},
                {"fromNode":"B","fromSocket":"color","toNode":"A","toSocket":"colorTarget","resource":{"node":"B","socket":"color"}}
            ]
        })
    );
}

#[test]
fn dead_texture_cycle_is_culled_without_cycle_execution() {
    let mut value = full_cull_graph();
    value["nodes"]
        .as_array_mut()
        .unwrap()
        .extend(cyclic_pipelines());
    let plan = compile_graph(value);
    assert_eq!(plan.node_count, 10);
    assert_eq!(plan.culled_node_count, 2);
    assert_eq!(plan.culled_resource_count, 4);
    assert!(!plan
        .executions
        .iter()
        .any(|execution| matches!(execution.id.as_str(), "A" | "B")));
}
fn compile_error(v: Value) -> GraphError {
    super::compile(serde_json::from_value(v).unwrap()).unwrap_err()
}
fn execution<'a>(p: &'a CompiledGraph, authored_id: &str) -> &'a CompiledExecution {
    p.executions.iter().find(|e| e.id == authored_id).unwrap()
}
fn resource_by_origin<'a>(p: &'a CompiledGraph, node: &str, socket: &str) -> &'a CompiledResource {
    p.resources
        .iter()
        .find(|r| r.origin.node == node && r.origin.socket == socket)
        .unwrap()
}

fn family_by_source<'a>(p: &'a CompiledGraph, node: &str) -> &'a TextureFamily {
    let source = resource_by_origin(p, node, "texture");
    let family = match source.plan {
        ResourcePlan::TextureSource { family, .. } => family,
        _ => panic!("{node} is not a texture specification"),
    };
    &p.texture_families[family as usize]
}

fn allocation_slot<'a>(p: &'a CompiledGraph, allocation: AllocationRef) -> &'a AllocationSlot {
    &p.allocation_classes[allocation.class as usize].slots[allocation.slot as usize]
}

fn independent_depth_graph(
    depth_specs: Vec<Value>,
    pipelines: Vec<Value>,
    present_from: &str,
) -> Value {
    let mut nodes = vec![node(
        "color",
        "texture",
        texture("rgba8_unorm", "transient"),
        json!({}),
    )];
    nodes.extend(render_support_nodes());
    nodes.extend(depth_specs);
    nodes.extend(pipelines);
    nodes.push(node(
        "frame_out",
        "frame_out",
        json!({}),
        json!({"color":input(present_from,"color")}),
    ));
    graph(nodes)
}

fn depth_spec(id: &str, residency: &str) -> Value {
    node(
        id,
        "texture",
        texture("depth32_float", residency),
        json!({}),
    )
}

#[test]
fn dense_lifetimes_exclude_authored_source_ordinals() {
    let p = compile_graph(full_cull_graph());
    assert_eq!(
        p.executions
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["cull", "query", "registry", "pipeline_main", "frame_out"]
    );
    for (node, socket, first, last) in [
        ("mesh", "mesh", 0, 3),
        ("mesh", "localAabbs", 0, 0),
        ("cull", "isFrustumCulled", 0, 1),
        ("mesh", "isVisible", 1, 1),
        ("mesh", "pipelineIndices", 2, 2),
        ("registry", "activation", 2, 3),
    ] {
        assert_eq!(
            resource_by_origin(&p, node, socket).lifetime,
            Some(Lifetime {
                first_use: first,
                last_use: last
            }),
            "lifetime for {node}.{socket}"
        );
    }
    let color = resource_by_origin(&p, "pipeline_main", "color");
    let depth = resource_by_origin(&p, "pipeline_main", "depth");
    assert_eq!(color.producer_execution, Some(3));
    assert_eq!(depth.producer_execution, Some(3));
    assert_eq!(
        color.lifetime,
        Some(Lifetime {
            first_use: 3,
            last_use: 4
        })
    );
    assert_eq!(
        depth.lifetime,
        Some(Lifetime {
            first_use: 3,
            last_use: 3
        })
    );
    let depth_family = family_by_source(&p, "depth");
    assert_eq!(
        depth_family.lifetime,
        Lifetime {
            first_use: 3,
            last_use: 3
        }
    );
    assert_eq!(depth_family.versions[0].lifetime, depth_family.lifetime);
}

#[test]
fn transient_aliasing_is_declaration_order_independent() {
    let p = compile_graph(independent_depth_graph(
        vec![
            depth_spec("depth_second", "transient"),
            depth_spec("depth_first", "transient"),
        ],
        vec![
            pipeline_node(
                "F0",
                input("color", "texture"),
                input("depth_first", "texture"),
            ),
            pipeline_node("F1", input("F0", "color"), input("depth_second", "texture")),
        ],
        "F1",
    ));
    assert_eq!(execution(&p, "F1").original_node_index, 7);
    let f0_ordinal = p.executions.iter().position(|e| e.id == "F0").unwrap() as u32;
    let f1_ordinal = p.executions.iter().position(|e| e.id == "F1").unwrap() as u32;
    assert_eq!(f1_ordinal, f0_ordinal + 1);
    let first = family_by_source(&p, "depth_first");
    let second = family_by_source(&p, "depth_second");
    assert_eq!(
        first.lifetime,
        Lifetime {
            first_use: f0_ordinal,
            last_use: f0_ordinal
        }
    );
    assert_eq!(
        second.lifetime,
        Lifetime {
            first_use: f1_ordinal,
            last_use: f1_ordinal
        }
    );
    assert_eq!(first.versions[0].lifetime, first.lifetime);
    assert_eq!(second.versions[0].lifetime, second.lifetime);
    assert_eq!(first.allocation, second.allocation);
    let slot = allocation_slot(&p, first.allocation.unwrap());
    assert_eq!(slot.kind, AllocationKind::AliasedTransient);
    assert_eq!(slot.usage, [TextureUsage::DepthAttachment]);
    assert_eq!(
        slot.occupants.iter().copied().collect::<BTreeSet<_>>(),
        [first.id, second.id].into_iter().collect()
    );
}

#[test]
fn overlapping_family_lifetimes_prevent_transient_reuse() {
    let p = compile_graph(independent_depth_graph(
        vec![
            depth_spec("depth_a", "transient"),
            depth_spec("depth_b", "transient"),
        ],
        vec![
            pipeline_node("F0", input("color", "texture"), input("depth_a", "texture")),
            pipeline_node("F1", input("F0", "color"), input("depth_b", "texture")),
            pipeline_node("F2", input("F1", "color"), input("F0", "depth")),
        ],
        "F2",
    ));
    let a = family_by_source(&p, "depth_a");
    let b = family_by_source(&p, "depth_b");
    assert!(a.lifetime.first_use < b.lifetime.first_use);
    assert!(a.lifetime.last_use > b.lifetime.last_use);
    assert_ne!(a.allocation, b.allocation);
    assert_ne!(a.allocation.unwrap().slot, b.allocation.unwrap().slot);
}

#[test]
fn persistent_textures_are_dedicated_and_follow_transient_slots() {
    let p = compile_graph(independent_depth_graph(
        vec![
            depth_spec("persistent_b", "persistent"),
            depth_spec("transient", "transient"),
            depth_spec("persistent_a", "persistent"),
        ],
        vec![
            pipeline_node(
                "F0",
                input("color", "texture"),
                input("persistent_a", "texture"),
            ),
            pipeline_node("F1", input("F0", "color"), input("transient", "texture")),
            pipeline_node("F2", input("F1", "color"), input("persistent_b", "texture")),
        ],
        "F2",
    ));
    let a = family_by_source(&p, "persistent_a");
    let b = family_by_source(&p, "persistent_b");
    assert_ne!(a.allocation, b.allocation);
    for family in [a, b] {
        let allocation = family.allocation.unwrap();
        assert_eq!(
            allocation_slot(&p, allocation).kind,
            AllocationKind::Persistent
        );
        assert_eq!(family.usage, [TextureUsage::DepthAttachment]);
        for version in &family.versions {
            let ResourcePlan::Texture {
                allocation: resource_allocation,
                ..
            } = p.resources[version.resource as usize].plan
            else {
                panic!()
            };
            assert_eq!(resource_allocation, Some(allocation));
        }
    }
    let transient = family_by_source(&p, "transient").allocation.unwrap();
    assert!(transient.slot < a.allocation.unwrap().slot);
    assert!(transient.slot < b.allocation.unwrap().slot);
    assert_eq!(p.transient_slot_count, 2);
}

#[test]
fn unsupported_texture_features_are_rejected_during_decode() {
    let cases = [
        ("dimension", json!("d1"), "dimension"),
        ("dimension", json!("d3"), "dimension"),
        ("mipLevelCount", json!(2), "mipLevelCount"),
        ("sampleCount", json!(4), "sampleCount"),
    ];
    for (field, value, suffix) in cases {
        let mut graph = full_cull_graph();
        graph["nodes"][1]["parameters"]["texture"][field] = value;
        let error = compile_error(graph);
        assert_eq!(error.code, "GRAPH_UNSUPPORTED_FEATURE", "{field}");
        assert_eq!(
            error.details["path"],
            format!("nodes[1].parameters.texture.{suffix}"),
            "{field}"
        );
    }
    for kind in ["absolute", "surface_relative"] {
        let mut graph = full_cull_graph();
        if kind == "absolute" {
            graph["nodes"][1]["parameters"]["texture"]["extent"] =
                json!({"kind":"absolute","width":64,"height":64,"depthOrArrayLayers":2});
        } else {
            graph["nodes"][1]["parameters"]["texture"]["extent"]["depthOrArrayLayers"] = json!(2);
        }
        let error = compile_error(graph);
        assert_eq!(error.code, "GRAPH_UNSUPPORTED_FEATURE", "{kind}");
        assert_eq!(
            error.details["path"], "nodes[1].parameters.texture.extent.depthOrArrayLayers",
            "{kind}"
        );
    }
}

#[test]
fn parser_and_registry_accept_only_canonical_schema() {
    let bytes = serde_json::to_vec(&full_cull_graph()).unwrap();
    assert!(parse_and_compile(&bytes).is_ok());
    assert_eq!(
        parse_and_compile(br#"{"schemaVersion":1}"#)
            .unwrap_err()
            .code,
        "GRAPH_SCHEMA_UNSUPPORTED"
    );
    let mut r = Registry::default();
    let (id, _) = r.compile(&bytes).unwrap();
    assert_eq!(r.get(id).unwrap().graph_id, "full");
}

#[test]
fn authoritative_eight_node_graph_lowers_exactly() {
    let p = compile_graph(full_cull_graph());
    assert_eq!(p.node_count, 8);
    assert_eq!(p.resources.len(), 11);
    assert_eq!(p.culled_resource_count, 0);
    assert_eq!(
        p.resources
            .iter()
            .map(|resource| (
                resource.origin.node.as_str(),
                resource.origin.socket.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("color", "texture"),
            ("depth", "texture"),
            ("mesh", "mesh"),
            ("mesh", "localAabbs"),
            ("mesh", "isVisible"),
            ("mesh", "pipelineIndices"),
            ("cull", "isFrustumCulled"),
            ("query", "draws"),
            ("registry", "activation"),
            ("pipeline_main", "color"),
            ("pipeline_main", "depth"),
        ]
    );
    assert_eq!(
        p.executions
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["cull", "query", "registry", "pipeline_main", "frame_out"]
    );
    for resource in &p.resources {
        if let ResourcePlan::Texture { version, .. } = resource.plan {
            assert_eq!(version, 0, "every first produced texture is symbolic v0");
        }
    }
    assert!(p.executions.iter().all(|e| !matches!(
        e.executor.key.as_str(),
        "surface_target" | "texture" | "mesh"
    )));
    let frame = execution(&p, "frame_out");
    let color = resource_by_origin(&p, "pipeline_main", "color");
    let color_id = p
        .resources
        .iter()
        .position(|resource| std::ptr::eq(resource, color))
        .unwrap() as u32;
    assert!(matches!(frame.kind, ExecutionKind::FrameOut { color } if color == color_id));
    assert!(frame.outputs.is_empty());
    assert_eq!(frame.inputs.len(), 1);
    assert_eq!(frame.accesses.len(), 1);
    assert_eq!(frame.accesses[0].socket, "color");
    assert_eq!(frame.accesses[0].resource, color_id);
    assert_eq!(frame.accesses[0].mode, AccessMode::SampledTexture);
    let family = family_by_source(&p, "color");
    assert_eq!(family.lifetime.last_use, 4);
    assert!(family.allocation.is_some());
    assert_eq!(
        family.usage.iter().copied().collect::<BTreeSet<_>>(),
        [TextureUsage::Sampled, TextureUsage::ColorAttachment]
            .into_iter()
            .collect()
    );
}

#[test]
fn exact_phase_four_contract_catalog_and_mesh_metadata() {
    assert_eq!(
        CONTRACTS
            .iter()
            .map(|contract| (contract.key, contract.version))
            .collect::<Vec<_>>(),
        [
            ("mesh", 1),
            ("texture", 1),
            ("frustum_cull", 1),
            ("mesh_query", 1),
            ("pipeline_registry", 1),
            ("pipeline", 1),
            ("fullscreen_copy", 1),
            ("color_balance", 1),
            ("exposure_contrast", 1),
            ("saturation", 1),
            ("channel_mixer", 1),
            ("bloom_extract", 1),
            ("bloom_blur", 1),
            ("bloom_composite", 1),
            ("luminance_edge", 1),
            ("frame_out", 2),
        ]
    );
    assert_eq!(
        CONTRACTS
            .iter()
            .map(|contract| (contract.key, contract.fullscreen_policy))
            .collect::<Vec<_>>(),
        [
            ("mesh", None),
            ("texture", None),
            ("frustum_cull", None),
            ("mesh_query", None),
            ("pipeline_registry", None),
            ("pipeline", None),
            ("fullscreen_copy", Some(FullscreenPolicy::Copy)),
            ("color_balance", Some(FullscreenPolicy::HdrSameExtent)),
            ("exposure_contrast", Some(FullscreenPolicy::HdrSameExtent)),
            ("saturation", Some(FullscreenPolicy::HdrSameExtent)),
            ("channel_mixer", Some(FullscreenPolicy::HdrSameExtent)),
            ("bloom_extract", Some(FullscreenPolicy::BloomExtract)),
            ("bloom_blur", Some(FullscreenPolicy::HdrSameExtent)),
            ("bloom_composite", Some(FullscreenPolicy::BloomComposite)),
            ("luminance_edge", Some(FullscreenPolicy::HdrSameExtent)),
            ("frame_out", None),
        ]
    );
    for contract in CONTRACTS {
        let serialized = serde_json::to_value(contract).unwrap();
        assert!(serialized.get("fullscreenPolicy").is_none());
    }
    let mesh = contract("mesh").unwrap();
    assert_eq!(mesh.execution, ExecutionClass::Source);
    assert!(mesh.inputs.is_empty());
    assert_eq!(
        mesh.outputs
            .iter()
            .map(|output| (output.name, output.semantic_type, output.metadata))
            .collect::<Vec<_>>(),
        [
            ("mesh", SemanticType::MeshData, OutputMetadata::None),
            (
                "localAabbs",
                SemanticType::LocalAabbBuffer,
                OutputMetadata::None,
            ),
            (
                "isVisible",
                SemanticType::BooleanFlagBuffer,
                OutputMetadata::BooleanFlag {
                    flag: MeshFlag::IsVisible,
                },
            ),
            (
                "pipelineIndices",
                SemanticType::PipelineIndexStream,
                OutputMetadata::None,
            ),
        ]
    );
    let cull = contract("frustum_cull").unwrap();
    assert_eq!(cull.inputs.len(), 2);
    assert_eq!(
        (cull.outputs[0].name, cull.outputs[0].metadata),
        (
            "isFrustumCulled",
            OutputMetadata::BooleanFlag {
                flag: MeshFlag::IsFrustumCulled,
            },
        )
    );
    let query = contract("mesh_query").unwrap();
    assert_eq!(
        query
            .inputs
            .iter()
            .map(|input| (input.name, input.cardinality))
            .collect::<Vec<_>>(),
        [
            ("mesh", InputCardinality::RequiredOne),
            ("isVisible", InputCardinality::OptionalOne),
            ("isFrustumCulled", InputCardinality::OptionalOne),
        ]
    );
    let frame = contract("frame_out").unwrap();
    assert_eq!(frame.execution, ExecutionClass::Frame);
    assert!(frame.inherently_observable);
    assert!(frame.outputs.is_empty());
    assert_eq!(frame.inputs.len(), 1);
    assert_eq!(frame.inputs[0].name, "color");
    assert_eq!(
        frame.inputs[0].accepted,
        TypeConstraint::Exact(SemanticType::Texture)
    );
    assert_eq!(frame.inputs[0].cardinality, InputCardinality::RequiredOne);
    assert_eq!(frame.inputs[0].role, InputRole::SampledTexture);
}

#[test]
fn removed_executor_keys_are_rejected_without_aliases() {
    let cases = [
        "texture_spec",
        "scene_table",
        "local_aabb_buffer",
        "camera_frustum",
        "visibility_flags",
        "surface_target",
        "present",
        "legacy_forward",
        "depth_stencil_config",
    ];
    for key in cases {
        let mut g = full_cull_graph();
        g["nodes"][0]["executor"]["key"] = json!(key);
        let error = compile_error(g);
        assert_eq!(error.code, "GRAPH_UNKNOWN_EXECUTOR", "{key}");
        assert_eq!(error.details["path"], "nodes[0].executor.key", "{key}");
    }
}

#[test]
fn exact_wire_catalog_rejections() {
    for field in [
        "pipeline",
        "depthCompare",
        "depthWriteEnabled",
        "clearDepth",
        "clearColor",
    ] {
        let mut g = full_cull_graph();
        let i = node_index(&g, "pipeline_main");
        g["nodes"][i]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    }
    let mut g = full_cull_graph();
    g["nodes"][0]["executor"]["version"] = json!(2);
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_EXECUTOR_VERSION_UNSUPPORTED");
    assert_eq!(e.details["path"], "nodes[0].executor.version");
}

#[test]
fn mesh_predicates_are_normalized_and_any_removes_dependency() {
    let p = compile_graph(full_cull_graph());
    let NormalizedParameters::MeshQuery {
        visible_predicate,
        frustum_culled_predicate,
    } = execution(&p, "query").parameters.clone()
    else {
        panic!()
    };
    assert_eq!(visible_predicate, RuntimePredicate::RequiredTrue);
    assert_eq!(frustum_culled_predicate, RuntimePredicate::RequiredFalse);
    let mut g = full_cull_graph();
    g["nodes"][4]["parameters"]["frustumCulledPredicate"] = json!("any");
    let p = compile_graph(g);
    assert!(!execution(&p, "query")
        .inputs
        .iter()
        .any(|x| x.socket == "isFrustumCulled"));
    assert!(!p.executions.iter().any(|e| e.id == "cull"));
}

#[test]
fn mesh_query_predicate_defaults_have_a_complete_truth_matrix() {
    for field in ["visible", "frustum"] {
        for predicate in ["any", "required_true", "required_false"] {
            for linked in [false, true] {
                for default in [false, true] {
                    let mut graph = full_cull_graph();
                    let query = &mut graph["nodes"][4];
                    query["parameters"]["visiblePredicate"] = json!("any");
                    query["parameters"]["frustumCulledPredicate"] = json!("any");
                    let (parameter, default_parameter, socket) = if field == "visible" {
                        ("visiblePredicate", "visibleDefault", "isVisible")
                    } else {
                        (
                            "frustumCulledPredicate",
                            "frustumCulledDefault",
                            "isFrustumCulled",
                        )
                    };
                    query["parameters"][parameter] = json!(predicate);
                    query["parameters"][default_parameter] = json!(default);
                    if !linked {
                        query["inputs"].as_object_mut().unwrap().remove(socket);
                    }
                    let expected = match (predicate, linked, default) {
                        ("any", _, _) => RuntimePredicate::Any,
                        ("required_true", true, _) => RuntimePredicate::RequiredTrue,
                        ("required_false", true, _) => RuntimePredicate::RequiredFalse,
                        ("required_true", false, true) | ("required_false", false, false) => {
                            RuntimePredicate::Any
                        }
                        _ => RuntimePredicate::Never,
                    };
                    let plan = compile_graph(graph);
                    let execution = execution(&plan, "query");
                    let NormalizedParameters::MeshQuery {
                        visible_predicate,
                        frustum_culled_predicate,
                    } = &execution.parameters
                    else {
                        panic!()
                    };
                    let (actual, other) = if field == "visible" {
                        (*visible_predicate, *frustum_culled_predicate)
                    } else {
                        (*frustum_culled_predicate, *visible_predicate)
                    };
                    let active = matches!(
                        expected,
                        RuntimePredicate::RequiredTrue | RuntimePredicate::RequiredFalse
                    );
                    let label = format!("{field}/{predicate}/linked={linked}/default={default}");
                    if expected == RuntimePredicate::Never {
                        assert_eq!(actual, RuntimePredicate::Never, "{label}");
                        assert_eq!(other, RuntimePredicate::Never, "{label}");
                    } else {
                        assert_eq!(actual, expected, "{label}");
                        assert_eq!(other, RuntimePredicate::Any, "{label}");
                    }
                    assert_eq!(
                        execution.inputs.iter().any(|input| input.socket == socket),
                        active,
                        "{label}"
                    );
                    assert_eq!(
                        execution
                            .accesses
                            .iter()
                            .any(|access| access.socket == socket),
                        active,
                        "{label}"
                    );
                    if expected == RuntimePredicate::Never {
                        assert!(!plan
                            .executions
                            .iter()
                            .any(|execution| execution.id == "cull"));
                    }
                }
            }
        }
    }
}

#[test]
fn inactive_mesh_source_outputs_are_pruned_but_executable_outputs_remain() {
    let mut graph = full_cull_graph();
    graph["nodes"][4]["parameters"]["visiblePredicate"] = json!("any");
    graph["nodes"][4]["parameters"]["frustumCulledPredicate"] = json!("any");
    let plan = compile_graph(graph);
    for socket in ["localAabbs", "isVisible"] {
        assert!(!plan
            .resources
            .iter()
            .any(|resource| resource.origin.node == "mesh" && resource.origin.socket == socket));
    }
    assert!(!plan
        .executions
        .iter()
        .any(|execution| execution.id == "cull"));
    for socket in ["color", "depth"] {
        assert!(plan
            .resources
            .iter()
            .any(|resource| resource.origin.node == "pipeline_main"
                && resource.origin.socket == socket));
    }

    let mut graph = full_cull_graph();
    graph["nodes"][4]["parameters"]["visiblePredicate"] = json!("any");
    let plan = compile_graph(graph);
    assert!(!plan
        .resources
        .iter()
        .any(|resource| resource.origin.node == "mesh" && resource.origin.socket == "isVisible"));
    assert!(plan
        .resources
        .iter()
        .any(|resource| resource.origin.node == "mesh" && resource.origin.socket == "localAabbs"));
}

#[test]
fn provenance_and_lowering_are_consistent() {
    let p = compile_graph(full_cull_graph());
    let scene = resource_by_origin(&p, "mesh", "mesh");
    for (id, socket) in [
        ("mesh", "localAabbs"),
        ("mesh", "isVisible"),
        ("cull", "isFrustumCulled"),
        ("query", "draws"),
    ] {
        let r = resource_by_origin(&p, id, socket);
        match r.plan {
            ResourcePlan::LocalAabbBuffer { mesh: s }
            | ResourcePlan::BooleanFlagBuffer { mesh: s, .. }
            | ResourcePlan::DrawStream { mesh: s } => {
                assert_eq!(
                    s,
                    p.resources
                        .iter()
                        .position(|r| std::ptr::eq(r, scene))
                        .unwrap() as u32
                )
            }
            _ => {}
        }
    }
    let f = execution(&p, "pipeline_main");
    let color_in = f
        .inputs
        .iter()
        .find(|x| x.socket == "colorTarget")
        .unwrap()
        .resource;
    let color_out = f
        .outputs
        .iter()
        .find(|x| x.socket == "color")
        .unwrap()
        .resource;
    assert_ne!(color_in, color_out);
    assert!(f
        .accesses
        .iter()
        .any(|a| a.resource == color_out && matches!(a.mode, AccessMode::ColorAttachment { .. })));
    assert!(!f
        .accesses
        .iter()
        .any(|a| a.resource == color_in && matches!(a.mode, AccessMode::ColorAttachment { .. })));
    for (socket, expected) in [
        ("mesh", AccessMode::SemanticRead),
        ("draws", AccessMode::IndirectRead),
    ] {
        let access = f.accesses.iter().find(|a| a.socket == socket).unwrap();
        assert_eq!(access.mode, expected, "pipeline {socket} access");
    }

    let registry = execution(&p, "registry");
    assert!(matches!(
        registry.parameters,
        NormalizedParameters::PipelineRegistry
    ));
    assert!(matches!(registry.kind, ExecutionKind::CpuPreparation));
    assert_eq!(registry.inputs.len(), 1);
    assert_eq!(registry.outputs.len(), 1);
    assert_eq!(registry.accesses.len(), 1);
    assert_eq!(registry.inputs[0].socket, "pipelineIndices");
    assert_eq!(registry.outputs[0].socket, "activation");
    assert_eq!(registry.accesses[0].mode, AccessMode::SemanticRead);
    let activation = &p.resources[registry.outputs[0].resource as usize];
    assert_eq!(activation.producer_execution, Some(2));
    assert_eq!(
        activation.lifetime,
        Some(Lifetime {
            first_use: 2,
            last_use: 3
        })
    );
    assert!(matches!(
        activation.plan,
        ResourcePlan::PipelineActivation { pipeline_indices }
            if pipeline_indices == registry.inputs[0].resource
    ));
}

#[test]
fn pipeline_clear_then_chained_load_is_independent_for_color_and_depth() {
    let p = compile_graph(independent_depth_graph(
        vec![depth_spec("depth", "transient")],
        vec![
            pipeline_node(
                "pipeline_first",
                input("color", "texture"),
                input("depth", "texture"),
            ),
            pipeline_node(
                "pipeline_second",
                input("pipeline_first", "color"),
                input("pipeline_first", "depth"),
            ),
        ],
        "pipeline_second",
    ));
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &execution(&p, "pipeline_first").kind
    else {
        panic!()
    };
    assert!(matches!(
        color_attachments[0].load,
        NormalizedColorLoad::Clear { .. }
    ));
    assert!(matches!(depth.load, NormalizedDepthLoad::Clear { .. }));
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &execution(&p, "pipeline_second").kind
    else {
        panic!()
    };
    assert_eq!(color_attachments[0].load, NormalizedColorLoad::Load);
    assert_eq!(depth.load, NormalizedDepthLoad::Load);

    for pipeline in ["", "bad name", "pipeline/name"] {
        let mut graph = full_cull_graph();
        let i = node_index(&graph, "pipeline_main");
        graph["nodes"][i]["parameters"]["pipeline"] = json!(pipeline);
        assert_eq!(
            compile_error(graph).code,
            "GRAPH_PARAMETERS_INVALID",
            "{pipeline:?}"
        );
    }
}

#[test]
fn descriptor_validation_and_normalization_table() {
    for (field, value) in [("sampleCount", json!(3))] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["texture"][field] = value;
        assert_eq!(compile_error(g).code, "GRAPH_UNSUPPORTED_FEATURE");
    }
    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["texture"]["extent"] =
        json!({"kind":"absolute","width":1,"height":1,"depthOrArrayLayers":1});
    g["nodes"][1]["parameters"]["texture"]["mipLevelCount"] = json!(2);
    assert_eq!(compile_error(g).code, "GRAPH_UNSUPPORTED_FEATURE");

    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["texture"]["mipLevelCount"] = json!(99);
    assert_eq!(compile_error(g).code, "GRAPH_UNSUPPORTED_FEATURE");
    for residency in ["history", "readback"] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["residency"] = json!(residency);
        assert_eq!(compile_error(g).code, "GRAPH_UNSUPPORTED_FEATURE");
    }
    let p = compile_graph(full_cull_graph());
    assert!(p
        .texture_families
        .iter()
        .all(|family| matches!(family.source, TextureFamilySource::AuthoredTexture { .. })));
}

#[test]
fn validation_precedence_and_identifier_limits() {
    let mut g = full_cull_graph();
    g["nodes"][0]["id"] = json!("x".repeat(65));
    g["nodes"][1]["executor"]["key"] = json!("bad");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(e.details["path"], "nodes[0].id");
    let mut g = full_cull_graph();
    g["nodes"][0]["id"] = json!("bad id");
    assert_eq!(compile_error(g).code, "GRAPH_INVALID_ID");
    let mut g = full_cull_graph();
    g["nodes"][1]["executor"]["key"] = json!("bad");
    g["nodes"][0]["parameters"] = json!({"bad":1});
    assert_eq!(compile_error(g).details["path"], "nodes[1].executor.key");
}

#[test]
fn global_identifier_lengths_precede_grammar_and_duplicates() {
    let mut invalid_grammar = full_cull_graph();
    invalid_grammar["nodes"][0]["id"] = json!("bad id");
    invalid_grammar["nodes"][4]["executor"]["key"] = json!("x".repeat(65));
    let error = compile_error(invalid_grammar);
    assert_eq!(error.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(error.details["path"], "nodes[4].executor.key");

    let mut duplicate = full_cull_graph();
    duplicate["nodes"][1]["id"] = duplicate["nodes"][0]["id"].clone();
    duplicate["nodes"][4]["executor"]["key"] = json!("x".repeat(65));
    let error = compile_error(duplicate);
    assert_eq!(error.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(error.details["path"], "nodes[4].executor.key");
}

#[test]
fn strict_mesh_diagnostics_and_inactive_any_edges() {
    for field in ["visiblePredicate", "frustumCulledPredicate"] {
        let mut g = full_cull_graph();
        g["nodes"][4]["parameters"][field] = json!("invalid");
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_PARAMETERS_INVALID");
        assert_eq!(e.details["path"], "nodes[4].parameters");
    }
    let mut g = full_cull_graph();
    g["nodes"][4]["parameters"]["frustumCulledPredicate"] = json!("required_true");
    g["nodes"][4]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("isFrustumCulled");
    let p = compile_graph(g);
    assert!(matches!(
        execution(&p, "query").parameters,
        NormalizedParameters::MeshQuery {
            frustum_culled_predicate: RuntimePredicate::Never,
            ..
        }
    ));
    assert!(!p.executions.iter().any(|e| e.id == "cull"));

    let mut g = full_cull_graph();
    g["nodes"][4]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("isFrustumCulled");
    let p = compile_graph(g);
    assert!(!p.executions.iter().any(|e| e.id == "cull"));
    let mut g = full_cull_graph();
    g["nodes"][4]["inputs"]["isVisible"] = input("cull", "isFrustumCulled");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[4].inputs.isVisible");

    let mut g = full_cull_graph();
    g["nodes"][4]["parameters"]["frustumCulledPredicate"] = json!("any");
    let p = compile_graph(g);
    let q = execution(&p, "query");
    assert!(!q.inputs.iter().any(|i| i.socket == "isFrustumCulled"));
    assert!(!q.accesses.iter().any(|a| a.socket == "isFrustumCulled"));
    assert!(!p.executions.iter().any(|e| e.id == "cull"));
}

#[test]
fn transitive_scene_roots_are_vector_resource_ids() {
    let p = compile_graph(full_cull_graph());
    let mesh_id = p
        .resources
        .iter()
        .position(|r| r.origin.node == "mesh")
        .unwrap() as u32;
    for (origin, socket) in [
        ("mesh", "localAabbs"),
        ("mesh", "isVisible"),
        ("cull", "isFrustumCulled"),
        ("query", "draws"),
    ] {
        let resource = resource_by_origin(&p, origin, socket);
        let rooted = match resource.plan {
            ResourcePlan::LocalAabbBuffer { mesh }
            | ResourcePlan::BooleanFlagBuffer { mesh, .. }
            | ResourcePlan::DrawStream { mesh } => Some(mesh),
            _ => None,
        };
        assert_eq!(rooted, Some(mesh_id));
    }
    let mut g = full_cull_graph();
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("sceneB", "mesh", json!({}), json!({})));
    g["nodes"][3]["inputs"]["mesh"] = input("sceneB", "mesh");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[3].inputs.localAabbs");
    let mut g = full_cull_graph();
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("sceneB", "mesh", json!({}), json!({})));
    g["nodes"][3]["inputs"]["mesh"] = input("sceneB", "mesh");
    g["nodes"][3]["inputs"]["localAabbs"] = input("sceneB", "localAabbs");
    g["nodes"][4]["inputs"]["mesh"] = input("sceneB", "mesh");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[4].inputs.isVisible");
}

#[test]
fn descriptor_exact_paths_and_normalization() {
    let cases = [
        ("sampleCount", json!(2), "sampleCount"),
        ("sampleCount", json!(8), "sampleCount"),
        ("mipLevelCount", json!(0), "mipLevelCount"),
    ];
    for (field, value, suffix) in cases {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["texture"][field] = value;
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_UNSUPPORTED_FEATURE");
        assert_eq!(
            e.details["path"],
            format!("nodes[1].parameters.texture.{suffix}")
        );
    }
    for (view, index) in [("depth32_float", 0), ("rgba8_unorm", 0)] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["texture"]["viewFormats"] = json!([view]);
        let e = compile_error(g);
        assert_eq!(
            e.details["path"],
            format!("nodes[1].parameters.texture.viewFormats[{index}]")
        );
    }
    for residency in ["history", "readback"] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["residency"] = json!(residency);
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_UNSUPPORTED_FEATURE");
        assert_eq!(e.details["path"], "nodes[1].parameters.residency");
    }
    let mut g = full_cull_graph();
    let d = &mut g["nodes"][1]["parameters"]["texture"];
    d["extent"]["width"] = json!({"numerator":2,"denominator":2});
    let p = compile_graph(g);
    let TextureFamilySource::AuthoredTexture { descriptor, .. } =
        &family_by_source(&p, "depth").source
    else {
        panic!()
    };
    assert!(
        matches!(&descriptor.extent, NormalizedTextureExtent::SurfaceRelative { width, .. } if *width == Ratio { numerator: 1, denominator: 1 })
    );
}

#[test]
fn descriptor_multi_error_precedence_is_exact() {
    let cases = [
        (json!(3), json!(0), 9000, "mipLevelCount"),
        (json!(1), json!(0), 9000, "mipLevelCount"),
        (json!(1), json!(30), 9000, "mipLevelCount"),
        (json!(1), json!(14), 9000, "mipLevelCount"),
        (json!(1), json!(14), 8192, "mipLevelCount"),
    ];
    for (sample_count, mip_count, width, expected) in cases {
        let mut g = full_cull_graph();
        let d = &mut g["nodes"][1]["parameters"]["texture"];
        d["format"] = json!("rgba8_unorm");
        d["extent"] = json!({
            "kind":"absolute",
            "width":width,
            "height":1,
            "depthOrArrayLayers":1
        });
        d["sampleCount"] = sample_count;
        d["mipLevelCount"] = mip_count;
        d["viewFormats"] = json!(["rgba8_unorm"]);
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_UNSUPPORTED_FEATURE");
        assert_eq!(
            e.details["path"],
            format!("nodes[1].parameters.texture.{expected}")
        );
    }
}

#[test]
fn global_raw_limits_have_stable_narrow_paths() {
    for (g, path) in [
        (
            {
                let mut g = full_cull_graph();
                g["graphId"] = json!("x".repeat(65));
                g
            },
            "graphId",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][0]["executor"]["key"] = json!("x".repeat(65));
                g
            },
            "nodes[0].executor.key",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][3]["inputs"]["x".repeat(65)] = input("mesh", "mesh");
                g
            },
            "nodes[3].inputs.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][3]["inputs"]["mesh"]["node"] = json!("x".repeat(65));
                g
            },
            "nodes[3].inputs.mesh.node",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][3]["inputs"]["mesh"]["socket"] = json!("x".repeat(65));
                g
            },
            "nodes[3].inputs.mesh.socket",
        ),
    ] {
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_LIMIT_EXCEEDED");
        assert_eq!(e.details["path"], path);
    }
    let mut inputs = serde_json::Map::new();
    for i in 0..8193 {
        inputs.insert(format!("s{i}"), input("n", "x"));
    }
    let g = graph(vec![node(
        "n",
        "surface_target",
        json!({}),
        Value::Object(inputs),
    )]);
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(e.details["path"], "nodes[0].inputs");
}

#[test]
fn empty_graph_is_rejected_without_frame_out() {
    let error = compile_error(json!({"schemaVersion":2,"graphId":"empty","revision":1,"nodes":[]}));
    assert_eq!(error.code, "GRAPH_EXECUTION_UNSUPPORTED");
    assert_eq!(
        error.details,
        json!({"message":"exactly one frame_out is required","path":"nodes"})
    );
}

#[test]
fn multiple_frame_outputs_and_uninitialized_sources_are_rejected() {
    let mut value = full_cull_graph();
    value["nodes"].as_array_mut().unwrap().push(node(
        "frame_out_2",
        "frame_out",
        json!({}),
        json!({"color":input("pipeline_main","color")}),
    ));
    let error = compile_error(value);
    assert_eq!(error.code, "GRAPH_EXECUTION_UNSUPPORTED");
    assert_eq!(error.details["path"], "nodes");

    let error = compile_error(graph(vec![
        node(
            "color",
            "texture",
            texture("rgba8_unorm", "transient"),
            json!({}),
        ),
        node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":input("color","texture")}),
        ),
    ]));
    assert_eq!(error.code, "GRAPH_UNINITIALIZED_RESOURCE");
    assert_eq!(error.details["path"], "nodes[1].inputs.color");
}

#[test]
fn frame_out_cardinality_counts_only_enabled_nodes() {
    let mut value = full_cull_graph();
    value["nodes"][7]["state"] = json!("muted");
    let error = compile_error(value);
    assert_eq!(error.code, "GRAPH_EXECUTION_UNSUPPORTED");
    assert_eq!(
        error.details,
        json!({"message":"exactly one frame_out is required","path":"nodes"})
    );

    let mut value = full_cull_graph();
    let mut muted = node(
        "muted_frame_out",
        "frame_out",
        json!({}),
        json!({"color":input("pipeline_main","color")}),
    );
    muted["state"] = json!("muted");
    value["nodes"].as_array_mut().unwrap().push(muted);
    let compiled = compile_graph(value);
    assert_eq!(
        compiled
            .executions
            .iter()
            .filter(|execution| execution.executor.key == "frame_out")
            .count(),
        1
    );
    assert!(!compiled
        .executions
        .iter()
        .any(|execution| execution.id == "muted_frame_out"));

    let mut value = full_cull_graph();
    value["nodes"][3]["state"] = json!("muted");
    let error = compile_error(value);
    assert_eq!(error.code, "GRAPH_NODE_STATE_INVALID");
    assert_eq!(error.details["path"], "nodes[3].state");

    let mut value = full_cull_graph();
    value["nodes"][7]["state"] = json!("muted");
    value["nodes"][6]["parameters"]["clearColor"] = json!("bad");
    assert_eq!(compile_error(value).code, "GRAPH_PARAMETERS_INVALID");

    let mut value = full_cull_graph();
    value["nodes"][7]["state"] = json!("muted");
    value["nodes"][7]["inputs"] = json!({});
    assert_eq!(compile_error(value).code, "GRAPH_EXECUTION_UNSUPPORTED");
}

#[test]
fn runtime_rejects_noncanonical_frame_out_mutations() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let frame_index = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "frame_out")
        .unwrap();
    let ExecutionKind::FrameOut { color } = baseline.executions[frame_index].kind else {
        unreachable!()
    };
    let family_id = match baseline.resources[color as usize].plan {
        ResourcePlan::Texture { family, .. } => family,
        _ => unreachable!(),
    };
    let assert_invalid = |graph: &CompiledGraph, path: &str| {
        let error = validate_activatable(graph).unwrap_err();
        assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID");
        assert_eq!(error.details["path"], path);
    };

    let mut graph = baseline.clone();
    graph.executions.remove(frame_index);
    let error = validate_activatable(&graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID");
    assert_eq!(error.details["path"], "resources[9].lifetime");

    let mut graph = baseline.clone();
    graph.executions.push(graph.executions[frame_index].clone());
    let error = validate_activatable(&graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID");
    assert_eq!(error.details["path"], "resources[9].lifetime");

    let mut graph = baseline.clone();
    graph.executions[frame_index].parameters = NormalizedParameters::FullscreenCopy;
    assert_invalid(&graph, &format!("executions[{frame_index}].parameters"));

    let mut graph = baseline.clone();
    graph.executions[frame_index].kind = ExecutionKind::CpuPreparation;
    assert_invalid(&graph, &format!("executions[{frame_index}].kind"));

    let mut graph = baseline.clone();
    graph.executions[frame_index]
        .outputs
        .push(CompiledSocketOutput {
            socket: "color".into(),
            resource: color,
        });
    assert_invalid(&graph, &format!("executions[{frame_index}].outputs"));

    for mutate in [
        |inputs: &mut Vec<CompiledSocketInput>| inputs.clear(),
        |inputs: &mut Vec<CompiledSocketInput>| inputs.push(inputs[0].clone()),
        |inputs: &mut Vec<CompiledSocketInput>| inputs[0].socket = "source".into(),
        |inputs: &mut Vec<CompiledSocketInput>| inputs[0].resource = u32::MAX - 1,
    ] {
        let mut graph = baseline.clone();
        mutate(&mut graph.executions[frame_index].inputs);
        assert_invalid(&graph, &format!("executions[{frame_index}].inputs"));
    }
    for mutate in [
        |accesses: &mut Vec<CompiledAccess>| accesses.clear(),
        |accesses: &mut Vec<CompiledAccess>| accesses.push(accesses[0].clone()),
        |accesses: &mut Vec<CompiledAccess>| accesses[0].socket = "source".into(),
        |accesses: &mut Vec<CompiledAccess>| accesses[0].resource = u32::MAX - 1,
        |accesses: &mut Vec<CompiledAccess>| accesses[0].mode = AccessMode::StorageRead,
    ] {
        let mut graph = baseline.clone();
        mutate(&mut graph.executions[frame_index].accesses);
        assert_invalid(&graph, &format!("executions[{frame_index}].accesses"));
    }

    let mut graph = baseline.clone();
    graph.executions[frame_index].kind = ExecutionKind::FrameOut { color: u32::MAX };
    graph.executions[frame_index].inputs[0].resource = u32::MAX;
    graph.executions[frame_index].accesses[0].resource = u32::MAX;
    assert_invalid(&graph, &format!("executions[{frame_index}].inputs"));

    let mut graph = baseline.clone();
    graph.resources[color as usize].semantic_type = SemanticType::MeshData;
    assert_invalid(&graph, &format!("textureFamilies[{family_id}].versions[0]"));

    for mutate in [
        |plan: &mut ResourcePlan| *plan = ResourcePlan::MeshData,
        |plan: &mut ResourcePlan| {
            let ResourcePlan::Texture { initialized, .. } = plan else {
                unreachable!()
            };
            *initialized = false;
        },
        |plan: &mut ResourcePlan| {
            let ResourcePlan::Texture { stored, .. } = plan else {
                unreachable!()
            };
            *stored = false;
        },
        |plan: &mut ResourcePlan| {
            let ResourcePlan::Texture { allocation, .. } = plan else {
                unreachable!()
            };
            *allocation = None;
        },
    ] {
        let mut graph = baseline.clone();
        mutate(&mut graph.resources[color as usize].plan);
        assert_invalid(&graph, &format!("textureFamilies[{family_id}].versions[0]"));
    }

    let descriptor_path = format!("textureFamilies[{family_id}].source");
    for mutate in [
        |descriptor: &mut NormalizedTextureDescriptor| descriptor.dimension = TextureDimension::D1,
        |descriptor: &mut NormalizedTextureDescriptor| descriptor.mip_level_count = 2,
        |descriptor: &mut NormalizedTextureDescriptor| descriptor.sample_count = 4,
        |descriptor: &mut NormalizedTextureDescriptor| match &mut descriptor.extent {
            NormalizedTextureExtent::Absolute {
                depth_or_array_layers,
                ..
            }
            | NormalizedTextureExtent::SurfaceRelative {
                depth_or_array_layers,
                ..
            } => *depth_or_array_layers = 2,
        },
        |descriptor: &mut NormalizedTextureDescriptor| descriptor.format = TextureFormat::R32Float,
        |descriptor: &mut NormalizedTextureDescriptor| {
            descriptor.format = TextureFormat::Depth32Float
        },
    ] {
        let mut graph = baseline.clone();
        let TextureFamilySource::AuthoredTexture { descriptor, .. } =
            &mut graph.texture_families[family_id as usize].source;
        mutate(descriptor);
        assert_invalid(&graph, &descriptor_path);
    }
}

#[test]
fn runtime_rejects_noncanonical_pipeline_registry_plan() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let registry = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline_registry")
        .unwrap();
    let indices = baseline.executions[registry].inputs[0].resource as usize;
    let activation = baseline.executions[registry].outputs[0].resource as usize;
    let activation_path = format!("resources[{activation}].plan");
    let indices_path = format!("resources[{indices}].plan");
    let cases: Vec<(&str, Box<dyn Fn(&mut CompiledGraph)>)> = vec![
        (
            "parameters",
            Box::new(move |g| {
                g.executions[registry].parameters = NormalizedParameters::FullscreenCopy
            }),
        ),
        (
            "kind",
            Box::new(
                move |g| g.executions[registry].kind = ExecutionKind::CpuPreparation, /* replaced below */
            ),
        ),
        (
            "inputs",
            Box::new(move |g| g.executions[registry].inputs.clear()),
        ),
        (
            "inputs",
            Box::new(move |g| {
                let duplicate = g.executions[registry].inputs[0].clone();
                g.executions[registry].inputs.push(duplicate)
            }),
        ),
        (
            "inputs",
            Box::new(move |g| g.executions[registry].inputs[0].socket = "mesh".into()),
        ),
        (
            "resources[8].producerExecution",
            Box::new(move |g| g.executions[registry].outputs.clear()),
        ),
        (
            "outputs",
            Box::new(move |g| {
                let duplicate = g.executions[registry].outputs[0].clone();
                g.executions[registry].outputs.push(duplicate)
            }),
        ),
        (
            "outputs",
            Box::new(move |g| g.executions[registry].outputs[0].socket = "draws".into()),
        ),
        (
            "accesses",
            Box::new(move |g| g.executions[registry].accesses[0].mode = AccessMode::StorageRead),
        ),
        (
            "accesses",
            Box::new(move |g| g.executions[registry].accesses.clear()),
        ),
        (
            &activation_path,
            Box::new(move |g| g.resources[activation].semantic_type = SemanticType::DrawStream),
        ),
        (
            &activation_path,
            Box::new(move |g| {
                g.resources[activation].plan = ResourcePlan::PipelineActivation {
                    pipeline_indices: u32::MAX,
                }
            }),
        ),
        (
            "resources[8].producerExecution",
            Box::new(move |g| g.resources[activation].producer_execution = None),
        ),
        (
            &indices_path,
            Box::new(move |g| {
                g.resources[indices].plan = ResourcePlan::PipelineIndexStream { mesh: u32::MAX }
            }),
        ),
    ];
    for (suffix, mutate) in cases {
        let mut graph = baseline.clone();
        if suffix == "kind" {
            graph.executions[registry].kind = ExecutionKind::FrameOut { color: 0 };
        } else {
            mutate(&mut graph);
        }
        let error = validate_activatable(&graph).unwrap_err();
        assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID", "{suffix}");
        let expected = if suffix.starts_with("resources") {
            suffix.to_owned()
        } else {
            format!("executions[{registry}].{suffix}")
        };
        assert_eq!(error.details["path"], expected, "{suffix}");
    }
}

#[test]
fn runtime_rejects_noncanonical_pipeline_plan() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let pipeline = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline")
        .unwrap();
    let outputs = baseline.executions[pipeline]
        .outputs
        .iter()
        .map(|output| output.resource as usize)
        .collect::<Vec<_>>();
    let mesh = baseline.executions[pipeline].inputs[0].resource as usize;
    let draws = baseline.executions[pipeline].inputs[1].resource as usize;
    let activation = baseline.executions[pipeline].inputs[2].resource as usize;
    let indices = match baseline.resources[activation].plan {
        ResourcePlan::PipelineActivation { pipeline_indices } => pipeline_indices as usize,
        _ => unreachable!(),
    };
    let color_family = match baseline.resources[outputs[0]].plan {
        ResourcePlan::Texture { family, .. } => family,
        _ => unreachable!(),
    };
    let color_path = format!("textureFamilies[{color_family}].versions[0]");
    let depth_path = format!("resources[{}].producerExecution", outputs[1]);
    let invalid_activation_path = format!("resources[{activation}].lifetime");
    let invalid_mesh_path = format!("resources[{mesh}].lifetime");
    let indices_path = format!("resources[{indices}].plan");
    let draws_producer = baseline.resources[draws].producer_execution.unwrap();
    let draws_path = format!("executions[{draws_producer}].outputs");
    let cases: Vec<(&str, Box<dyn Fn(&mut CompiledGraph)>)> = vec![
        (
            "parameters",
            Box::new(move |g| {
                g.executions[pipeline].parameters = NormalizedParameters::FullscreenCopy
            }),
        ),
        (
            "parameters",
            Box::new(move |g| {
                if let NormalizedParameters::Pipeline { pipeline: name, .. } =
                    &mut g.executions[pipeline].parameters
                {
                    *name = "1bad".into()
                }
            }),
        ),
        (
            "parameters",
            Box::new(move |g| {
                if let NormalizedParameters::Pipeline { clear_depth, .. } =
                    &mut g.executions[pipeline].parameters
                {
                    *clear_depth = f32::NAN
                }
            }),
        ),
        (
            "kind",
            Box::new(move |g| g.executions[pipeline].kind = ExecutionKind::CpuPreparation),
        ),
        (
            "resources[1].lifetime",
            Box::new(move |g| g.executions[pipeline].inputs.pop().map(|_| ()).unwrap()),
        ),
        (
            "inputs",
            Box::new(move |g| g.executions[pipeline].inputs.swap(0, 1)),
        ),
        (
            "resources[10].producerExecution",
            Box::new(move |g| g.executions[pipeline].outputs.pop().map(|_| ()).unwrap()),
        ),
        (
            "outputs",
            Box::new(move |g| g.executions[pipeline].outputs.swap(0, 1)),
        ),
        (
            "accesses",
            Box::new(move |g| g.executions[pipeline].accesses.pop().map(|_| ()).unwrap()),
        ),
        (
            "accesses",
            Box::new(move |g| g.executions[pipeline].accesses.swap(0, 1)),
        ),
        (
            "accesses",
            Box::new(move |g| g.executions[pipeline].accesses[2].mode = AccessMode::IndirectRead),
        ),
        (
            "kind",
            Box::new(move |g| {
                if let ExecutionKind::Render {
                    color_attachments, ..
                } = &mut g.executions[pipeline].kind
                {
                    color_attachments[0].load = NormalizedColorLoad::Load
                }
            }),
        ),
        (
            "accesses",
            Box::new(move |g| {
                if let AccessMode::ColorAttachment { full_overwrite, .. } =
                    &mut g.executions[pipeline].accesses[3].mode
                {
                    *full_overwrite = false
                }
            }),
        ),
        (
            &color_path,
            Box::new({
                let output = outputs[0];
                move |g| {
                    if let ResourcePlan::Texture { target, .. } = &mut g.resources[output].plan {
                        *target = u32::MAX
                    }
                }
            }),
        ),
        (
            &depth_path,
            Box::new({
                let output = outputs[1];
                move |g| g.resources[output].producer_execution = None
            }),
        ),
        (
            &invalid_activation_path,
            Box::new(move |g| {
                g.executions[pipeline].inputs[2].resource = draws as u32;
                g.executions[pipeline].accesses[2].resource = draws as u32;
            }),
        ),
        (
            &indices_path,
            Box::new(move |g| g.resources[indices].semantic_type = SemanticType::DrawStream),
        ),
        (
            &draws_path,
            Box::new(move |g| {
                g.resources[draws].plan = ResourcePlan::DrawStream { mesh: u32::MAX }
            }),
        ),
        (
            &invalid_mesh_path,
            Box::new(move |g| {
                g.executions[pipeline].inputs[0].resource = draws as u32;
                g.executions[pipeline].accesses[0].resource = draws as u32;
            }),
        ),
    ];
    for (path, mutate) in cases {
        let mut graph = baseline.clone();
        mutate(&mut graph);
        let error = validate_activatable(&graph).unwrap_err();
        assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID", "{path}");
        let expected = if path.starts_with("executions")
            || path.starts_with("resources")
            || path.starts_with("textureFamilies")
        {
            path.to_owned()
        } else {
            format!("executions[{pipeline}].{path}")
        };
        assert_eq!(error.details["path"], expected, "{path}");
    }
    assert!(matches!(
        baseline.resources[mesh].plan,
        ResourcePlan::MeshData
    ));
}

fn assert_runtime_path(graph: &CompiledGraph, path: impl AsRef<str>) {
    let error = validate_activatable(graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID");
    assert_eq!(error.details["path"], path.as_ref());
}

fn mutate_texture_descriptor(
    graph: &mut CompiledGraph,
    resource: u32,
    mutate: impl FnOnce(&mut NormalizedTextureDescriptor),
) {
    let (family, allocation) = match graph.resources[resource as usize].plan {
        ResourcePlan::Texture {
            family, allocation, ..
        } => (family as usize, allocation.unwrap()),
        _ => unreachable!(),
    };
    let TextureFamilySource::AuthoredTexture {
        resource: source,
        descriptor,
        ..
    } = &mut graph.texture_families[family].source;
    mutate(descriptor);
    let descriptor = descriptor.clone();
    let ResourcePlan::TextureSource {
        descriptor: source_descriptor,
        ..
    } = &mut graph.resources[*source as usize].plan
    else {
        unreachable!()
    };
    *source_descriptor = descriptor.clone();
    let key = &mut graph.allocation_classes[allocation.class as usize].key;
    key.dimension = descriptor.dimension;
    key.format = descriptor.format;
    key.extent = descriptor.extent;
    key.mip_level_count = descriptor.mip_level_count;
    key.sample_count = descriptor.sample_count;
    key.view_formats = descriptor.view_formats;
}

#[test]
fn runtime_rejects_coordinated_contract_and_texture_target_mutations() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();

    let mut graph = baseline.clone();
    graph.schema_version = 1;
    assert_runtime_path(&graph, "schemaVersion");

    for key in ["frame_out", "pipeline_registry", "pipeline"] {
        let i = baseline
            .executions
            .iter()
            .position(|execution| execution.executor.key == key)
            .unwrap();
        let mut graph = baseline.clone();
        graph.executions[i].executor.version += 1;
        assert_runtime_path(&graph, format!("executions[{i}].executor.version"));
    }

    let pipeline = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline")
        .unwrap();
    let color = baseline.executions[pipeline]
        .outputs
        .iter()
        .find(|output| output.socket == "color")
        .unwrap()
        .resource as usize;
    let depth = baseline.executions[pipeline]
        .outputs
        .iter()
        .find(|output| output.socket == "depth")
        .unwrap()
        .resource;
    let (color_family, color_version) = match baseline.resources[color].plan {
        ResourcePlan::Texture {
            family, version, ..
        } => (family as usize, version as usize),
        _ => unreachable!(),
    };
    let depth_target = match baseline.resources[depth as usize].plan {
        ResourcePlan::Texture { target, .. } => target,
        _ => unreachable!(),
    };
    let mut graph = baseline.clone();
    graph.executions[pipeline]
        .inputs
        .iter_mut()
        .find(|input| input.socket == "colorTarget")
        .unwrap()
        .resource = depth_target;
    if let ResourcePlan::Texture { target, .. } = &mut graph.resources[color].plan {
        *target = depth_target;
    }
    graph.texture_families[color_family].versions[color_version].target = depth_target;
    graph.resources[match baseline.resources[color].plan {
        ResourcePlan::Texture { target, .. } => target as usize,
        _ => unreachable!(),
    }]
    .lifetime = None;
    assert_runtime_path(
        &graph,
        format!("textureFamilies[{color_family}].versions[{color_version}]"),
    );
}

#[test]
fn runtime_rejects_coordinated_execution_metadata_mutations() {
    let baseline = compile_graph(full_cull_graph());
    let consumer = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "frame_out")
        .unwrap();
    let mut graph = baseline.clone();
    let producer = graph.executions[consumer].inputs[0].resource as usize;
    let producer_execution = graph.resources[producer].producer_execution.unwrap() as usize;
    graph.executions.swap(producer_execution, consumer);
    for resource in &mut graph.resources {
        if resource.producer_execution == Some(producer_execution as u32) {
            resource.producer_execution = Some(consumer as u32);
        } else if resource.producer_execution == Some(consumer as u32) {
            resource.producer_execution = Some(producer_execution as u32);
        }
    }
    let swap_ordinal = |ordinal: &mut u32| {
        if *ordinal == producer_execution as u32 {
            *ordinal = consumer as u32;
        } else if *ordinal == consumer as u32 {
            *ordinal = producer_execution as u32;
        }
    };
    for resource in &mut graph.resources {
        if let Some(lifetime) = &mut resource.lifetime {
            swap_ordinal(&mut lifetime.first_use);
            swap_ordinal(&mut lifetime.last_use);
            if lifetime.first_use > lifetime.last_use {
                std::mem::swap(&mut lifetime.first_use, &mut lifetime.last_use);
            }
        }
    }
    for family in &mut graph.texture_families {
        swap_ordinal(&mut family.lifetime.first_use);
        swap_ordinal(&mut family.lifetime.last_use);
        if family.lifetime.first_use > family.lifetime.last_use {
            std::mem::swap(
                &mut family.lifetime.first_use,
                &mut family.lifetime.last_use,
            );
        }
        for version in &mut family.versions {
            swap_ordinal(&mut version.lifetime.first_use);
            swap_ordinal(&mut version.lifetime.last_use);
            if version.lifetime.first_use > version.lifetime.last_use {
                std::mem::swap(
                    &mut version.lifetime.first_use,
                    &mut version.lifetime.last_use,
                );
            }
        }
    }
    assert_runtime_path(&graph, format!("executions[{producer_execution}].inputs"));

    let pipeline = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline")
        .unwrap();
    let mut graph = baseline.clone();
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &mut graph.executions[pipeline].kind
    else {
        unreachable!()
    };
    color_attachments[0].load = NormalizedColorLoad::Load;
    depth.load = NormalizedDepthLoad::Load;
    for access in &mut graph.executions[pipeline].accesses {
        match &mut access.mode {
            AccessMode::ColorAttachment {
                load,
                full_overwrite,
                ..
            } => {
                *load = NormalizedColorLoad::Load;
                *full_overwrite = false;
            }
            AccessMode::DepthAttachment {
                load,
                full_overwrite,
                ..
            } => {
                *load = NormalizedDepthLoad::Load;
                *full_overwrite = false;
            }
            _ => {}
        }
    }
    assert_runtime_path(&graph, format!("executions[{pipeline}].kind"));

    let mut graph = baseline.clone();
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &mut graph.executions[pipeline].kind
    else {
        unreachable!()
    };
    let changed_color = NormalizedColorLoad::Clear { value: [0.5; 4] };
    let changed_depth = NormalizedDepthLoad::Clear { value: 0.5 };
    color_attachments[0].load = changed_color;
    depth.load = changed_depth;
    for access in &mut graph.executions[pipeline].accesses {
        match &mut access.mode {
            AccessMode::ColorAttachment { load, .. } => *load = changed_color,
            AccessMode::DepthAttachment { load, .. } => *load = changed_depth,
            _ => {}
        }
    }
    assert_runtime_path(&graph, format!("executions[{pipeline}].kind"));
}

#[test]
fn runtime_rejects_coordinated_usage_and_alias_mutations() {
    let baseline = compile_graph(hdr_copy_graph());
    let family = family_by_source(&baseline, "hdr").id as usize;
    let allocation = baseline.texture_families[family].allocation.unwrap();
    let mut graph = baseline.clone();
    graph.texture_families[family].usage = vec![TextureUsage::ColorAttachment];
    graph.allocation_classes[allocation.class as usize].slots[allocation.slot as usize].usage =
        vec![TextureUsage::ColorAttachment];
    assert_runtime_path(&graph, format!("textureFamilies[{family}].usage"));

    let mut graph = baseline.clone();
    graph.texture_families[family]
        .usage
        .push(TextureUsage::Sampled);
    graph.allocation_classes[allocation.class as usize].slots[allocation.slot as usize]
        .usage
        .push(TextureUsage::Sampled);
    assert_runtime_path(&graph, format!("textureFamilies[{family}].usage"));

    let mut graph = baseline.clone();
    graph.allocation_classes[allocation.class as usize].slots[allocation.slot as usize]
        .usage
        .clear();
    assert_runtime_path(
        &graph,
        format!(
            "allocationClasses[{}].slots[{}].usage",
            allocation.class, allocation.slot
        ),
    );

    let mut overlap = compile_graph(independent_depth_graph(
        vec![
            depth_spec("depth_a", "transient"),
            depth_spec("depth_b", "transient"),
        ],
        vec![
            pipeline_node("F0", input("color", "texture"), input("depth_a", "texture")),
            pipeline_node("F1", input("F0", "color"), input("depth_b", "texture")),
            pipeline_node("F2", input("F1", "color"), input("F0", "depth")),
        ],
        "F2",
    ));
    let a = family_by_source(&overlap, "depth_a").id as usize;
    let b = family_by_source(&overlap, "depth_b").id as usize;
    let destination = overlap.texture_families[a].allocation.unwrap();
    let old = overlap.texture_families[b].allocation.unwrap();
    overlap.texture_families[b].allocation = Some(destination);
    for version in overlap.texture_families[b].versions.clone() {
        if let ResourcePlan::Texture { allocation, .. } =
            &mut overlap.resources[version.resource as usize].plan
        {
            *allocation = Some(destination);
        }
    }
    overlap.allocation_classes[old.class as usize].slots[old.slot as usize]
        .occupants
        .retain(|id| *id != b as u32);
    let slot = &mut overlap.allocation_classes[destination.class as usize].slots
        [destination.slot as usize];
    slot.kind = AllocationKind::AliasedTransient;
    slot.occupants.push(b as u32);
    assert_runtime_path(
        &overlap,
        format!(
            "allocationClasses[{}].slots[{}].occupants",
            destination.class, destination.slot
        ),
    );
}

#[test]
fn registry_revision_handles_are_immutable_and_drop_is_transactional() {
    let bytes = |revision| {
        let mut graph = full_cull_graph();
        graph["graphId"] = json!("registry");
        graph["revision"] = json!(revision);
        serde_json::to_vec(&graph).unwrap()
    };
    let mut r = Registry::new(2);
    let (id, _) = r.compile(&bytes(1)).unwrap();
    assert_eq!(
        r.compile(&bytes(1)).unwrap_err().message,
        "revision must increase"
    );
    let (second, _) = r.compile(&bytes(2)).unwrap();
    assert_ne!(id, second);
    assert_eq!(r.get(id).unwrap().revision, 1);
    r.drop_graph(id).unwrap();
    assert_eq!(r.get(id).unwrap_err().code, "STALE_GRAPH_ID");
    let (next, _) = r.compile(&bytes(3)).unwrap();
    assert_ne!(id, next);
    assert!(r.get(second).is_ok());
}

#[test]
fn old_schema_is_rejected() {
    let old = br#"{"schemaVersion":1,"graphId":"old","revision":1,"nodes":[]}"#;
    assert_eq!(
        parse_and_compile(old).unwrap_err().code,
        "GRAPH_SCHEMA_UNSUPPORTED"
    );
}

#[test]
fn socket_validation_is_globally_phased() {
    let mut g = full_cull_graph();
    g["nodes"][3]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("mesh");
    g["nodes"][3]["inputs"]["bogus"] = input("mesh", "mesh");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_SOCKET", Some("nodes[3].inputs.bogus"))
    );

    let mut g = full_cull_graph();
    g["nodes"][6]["inputs"]["colorTarget"]["socket"] = json!("bogus");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        (
            "GRAPH_UNKNOWN_SOCKET",
            Some("nodes[6].inputs.colorTarget.socket")
        )
    );

    let mut g = full_cull_graph();
    g["nodes"][6]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("draws");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_SOCKET_CARDINALITY", Some("nodes[6].inputs.draws"))
    );

    let mut g = full_cull_graph();
    g["nodes"][6]["inputs"]["mesh"] = input("depth", "texture");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_SOCKET_TYPE_MISMATCH", Some("nodes[6].inputs.mesh"))
    );
}

#[test]
fn executor_version_parameter_and_state_precedence_is_global() {
    let mut g = full_cull_graph();
    g["nodes"][2]["inputs"]["bad"] = input("missing", "bad");
    g["nodes"][4]["executor"]["key"] = json!("unknown");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_NODE", Some("nodes[2].inputs.bad.node"))
    );

    let mut g = full_cull_graph();
    g["nodes"][0]["parameters"] = json!({"bad":1});
    g["nodes"][1]["state"] = json!("muted");
    g["nodes"][2]["inputs"]["bad"] = input("mesh", "bad");
    g["nodes"][4]["executor"]["key"] = json!("unknown");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_EXECUTOR", Some("nodes[4].executor.key"))
    );

    let mut g = full_cull_graph();
    g["nodes"][0]["parameters"] = json!({"bad":1});
    g["nodes"][1]["state"] = json!("muted");
    g["nodes"][2]["inputs"]["bad"] = input("mesh", "bad");
    g["nodes"][4]["executor"]["version"] = json!(2);
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        (
            "GRAPH_EXECUTOR_VERSION_UNSUPPORTED",
            Some("nodes[4].executor.version")
        )
    );

    let mut g = full_cull_graph();
    g["nodes"][0]["parameters"] = json!({"bad":1});
    g["nodes"][1]["state"] = json!("muted");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_PARAMETERS_INVALID", Some("nodes[0].parameters"))
    );
}

#[test]
fn attachment_compatibility_matrix_is_enforced() {
    let cases = [
        (
            "depth_surface",
            json!({"kind":"absolute","width":4,"height":4,"depthOrArrayLayers":1}),
            "depth32_float",
            "d2",
            1,
        ),
        (
            "depth_half",
            json!({"kind":"surface_relative","width":{"numerator":1,"denominator":2},"height":{"numerator":1,"denominator":2},"depthOrArrayLayers":1}),
            "depth32_float",
            "d2",
            1,
        ),
        (
            "depth_layers",
            json!({"kind":"surface_relative","width":{"numerator":1,"denominator":1},"height":{"numerator":1,"denominator":1},"depthOrArrayLayers":2}),
            "depth32_float",
            "d2",
            1,
        ),
        (
            "depth_format",
            json!({"kind":"surface_relative","width":{"numerator":1,"denominator":1},"height":{"numerator":1,"denominator":1},"depthOrArrayLayers":1}),
            "rgba8_unorm",
            "d2",
            1,
        ),
    ];
    for (_, extent, format, dimension, samples) in cases {
        let mut g = full_cull_graph();
        let d = &mut g["nodes"][1]["parameters"]["texture"];
        d["extent"] = extent;
        d["format"] = json!(format);
        d["dimension"] = json!(dimension);
        d["sampleCount"] = json!(samples);
        let error = compile_error(g);
        if error.details["path"] == "nodes[1].parameters.texture.extent.depthOrArrayLayers" {
            assert_eq!(error.code, "GRAPH_UNSUPPORTED_FEATURE");
        } else {
            assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
        }
    }

    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"] = texture("rgba8_unorm", "transient");
    g["nodes"][6]["inputs"]["colorTarget"] = input("depth", "texture");
    g["nodes"][6]["inputs"]["depthTarget"] = input("color", "texture");
    assert_eq!(compile_error(g).code, "GRAPH_ILLEGAL_ACCESS");

    for field in ["dimension", "extent", "sampleCount"] {
        let mut g = full_cull_graph();
        let mut color = texture("rgba8_unorm", "transient");
        match field {
            "dimension" => {
                color["texture"][field] = json!("d3");
                color["texture"]["extent"] =
                    json!({"kind":"absolute","width":1,"height":1,"depthOrArrayLayers":1});
            }
            "extent" => {
                color["texture"][field] =
                    json!({"kind":"absolute","width":4,"height":4,"depthOrArrayLayers":1})
            }
            _ => color["texture"][field] = json!(4),
        }
        g["nodes"]
            .as_array_mut()
            .unwrap()
            .insert(2, node("test_color", "texture", color, json!({})));
        g["nodes"][7]["inputs"]["colorTarget"] = input("test_color", "texture");
        assert_eq!(
            compile_error(g).code,
            if field == "extent" {
                "GRAPH_ILLEGAL_ACCESS"
            } else {
                "GRAPH_UNSUPPORTED_FEATURE"
            },
            "{field}"
        );
    }

    let mut g = full_cull_graph();
    g["nodes"].as_array_mut().unwrap().insert(
        2,
        node(
            "test_color",
            "texture",
            texture("r32_float", "transient"),
            json!({}),
        ),
    );
    g["nodes"][7]["inputs"]["colorTarget"] = input("test_color", "texture");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_ILLEGAL_ACCESS", Some("nodes[8].inputs.color"))
    );
}

#[test]
fn wire_rejects_old_and_unknown_fields_exactly() {
    let mut cases = Vec::new();
    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["descriptor"] = g["nodes"][1]["parameters"]["texture"].take();
    cases.push(g);
    for old in ["compare", "writeEnabled", "clear"] {
        let mut g = full_cull_graph();
        g["nodes"][6]["parameters"][old] = json!(1);
        cases.push(g);
    }
    for missing in ["clearDepth", "clearColor"] {
        let mut g = full_cull_graph();
        let i = 6;
        g["nodes"][i]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        cases.push(g);
    }
    for g in cases {
        assert_eq!(
            parse_and_compile(&serde_json::to_vec(&g).unwrap())
                .unwrap_err()
                .code,
            "GRAPH_PARAMETERS_INVALID"
        );
    }
    for (index, field) in [(0, "legacyOptional"), (0, "unknownNodeField")] {
        let mut g = full_cull_graph();
        g["nodes"][index][field] = json!(null);
        assert_eq!(
            parse_and_compile(&serde_json::to_vec(&g).unwrap())
                .unwrap_err()
                .code,
            "GRAPH_JSON_INVALID"
        );
    }
    let mut g = full_cull_graph();
    g["unknownGraphField"] = json!(true);
    assert_eq!(
        parse_and_compile(&serde_json::to_vec(&g).unwrap())
            .unwrap_err()
            .code,
        "GRAPH_JSON_INVALID"
    );
}

#[test]
fn raw_limits_precede_malformed_content_and_cover_live_resources() {
    let mut g = graph(
        (0..1025)
            .map(|i| node(&format!("n{i}"), "surface_target", json!({}), json!({})))
            .collect(),
    );
    g["graphId"] = json!("bad id");
    assert_eq!(compile_error(g).details["path"], "nodes");
    let mut nodes: Vec<_> = (0..65)
        .map(|i| {
            node(
                &format!("p{i}"),
                "frame_out",
                json!({}),
                json!({"color":input("missing","bad")}),
            )
        })
        .collect();
    assert_eq!(
        compile_error(graph(std::mem::take(&mut nodes))).details["path"],
        "nodes[0].inputs.color.node"
    );
    let mut nodes = vec![node(
        "color",
        "texture",
        texture("rgba8_unorm", "transient"),
        json!({}),
    )];
    nodes.extend(render_support_nodes());
    let mut color = input("color", "texture");
    for i in 0..508 {
        let d = format!("d{i}");
        let f = format!("f{i}");
        nodes.push(node(
            &d,
            "texture",
            texture("depth32_float", "transient"),
            json!({}),
        ));
        nodes.push(pipeline_node(&f, color, input(&d, "texture")));
        color = input(&f, "color");
    }
    nodes.push(node(
        "frame_out",
        "frame_out",
        json!({}),
        json!({"color":color}),
    ));
    let e = compile_error(graph(nodes));
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_LIMIT_EXCEEDED", Some("resources"))
    );
}

#[test]
fn generated_resource_limit_is_only_final() {
    fn oversized(old_present: bool) -> Value {
        let mut nodes = vec![node(
            "color",
            "texture",
            texture("rgba8_unorm", "transient"),
            json!({}),
        )];
        nodes.extend(render_support_nodes());
        let mut color = input("color", "texture");
        for i in 0..508 {
            let d = format!("d{i}");
            let f = format!("f{i}");
            nodes.push(node(
                &d,
                "texture",
                texture("depth32_float", "transient"),
                json!({}),
            ));
            nodes.push(pipeline_node(&f, color, input(&d, "texture")));
            color = input(&f, "color");
        }
        nodes.push(node(
            "frame_out",
            "frame_out",
            json!({}),
            json!({"color":color}),
        ));
        if old_present {
            nodes.push(node(
                "old_present",
                "frame_out",
                json!({}),
                json!({"color":input("f0","color")}),
            ));
        }
        assert!(nodes.len() <= 1024);
        graph(nodes)
    }

    let clean = compile_error(oversized(false));
    assert_eq!(
        (clean.code, clean.details["path"].as_str()),
        ("GRAPH_LIMIT_EXCEEDED", Some("resources"))
    );
    let polluted = compile_error(oversized(true));
    assert_eq!(polluted.code, "GRAPH_EXECUTION_UNSUPPORTED");
    assert_eq!(polluted.details["path"], "nodes");
}

#[test]
fn bloom_composite_rejects_each_stale_sampled_texture_version() {
    for stale_socket in ["source", "bloom"] {
        let mut half = texture("rgba16_float", "transient");
        half["texture"]["extent"]["width"] = json!({"numerator":1,"denominator":2});
        half["texture"]["extent"]["height"] = json!({"numerator":1,"denominator":2});
        let mut half_depth = texture("depth32_float", "transient");
        half_depth["texture"]["extent"] = half["texture"]["extent"].clone();
        let mut nodes = vec![
            node(
                "color",
                "texture",
                texture("rgba8_unorm", "transient"),
                json!({}),
            ),
            node(
                "source_target",
                "texture",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            node("bloom_target", "texture", half, json!({})),
            node(
                "output",
                "texture",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            depth_spec("source_depth_0", "transient"),
            depth_spec("source_depth_1", "transient"),
            node("bloom_depth_0", "texture", half_depth.clone(), json!({})),
            node("bloom_depth_1", "texture", half_depth, json!({})),
        ];
        nodes.extend(render_support_nodes());
        nodes.extend([
            pipeline_node("source_f0", input("source_target","texture"), input("source_depth_0","texture")),
            pipeline_node("source_f1", input("source_f0","color"), input("source_depth_1","texture")),
            pipeline_node("bloom_f0", input("bloom_target","texture"), input("bloom_depth_0","texture")),
            pipeline_node("bloom_f1", input("bloom_f0","color"), input("bloom_depth_1","texture")),
            node(
                "composite",
                "bloom_composite",
                json!({"intensity":1.0}),
                json!({
                    "source":input(if stale_socket == "source" { "source_f0" } else { "source_f1" },"color"),
                    "bloom":input(if stale_socket == "bloom" { "source_f0" } else { "source_f1" },"color"),
                    "colorTarget":input("output","texture")
                }),
            ),
            node(
                "to_surface",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("composite","color"),"colorTarget":input("color","texture")}),
            ),
            node(
                "frame_out",
                "frame_out",
                json!({}),
                json!({"color":input("to_surface","color")}),
            ),
        ]);
        let error = compile_error(graph(nodes));
        assert_eq!(error.code, "GRAPH_RESOURCE_VERSION_INVALID");
        assert_eq!(
            error.details["path"],
            format!("nodes[15].inputs.{stale_socket}")
        );
    }
}

#[test]
fn bloom_composite_requires_a_single_view_rgba16_half_resolution_bloom() {
    compile_graph(bloom_composite_graph());

    let mut invalid = bloom_composite_graph();
    invalid["nodes"][2]["parameters"]["texture"]["format"] = json!("rgba8_unorm");
    invalid["nodes"][2]["parameters"]["texture"]["mipLevelCount"] = json!(2);
    let error = compile_error(invalid);
    assert_eq!(error.code, "GRAPH_UNSUPPORTED_FEATURE");
    assert_eq!(
        error.details["path"],
        "nodes[2].parameters.texture.mipLevelCount"
    );
}

#[test]
fn fullscreen_copy_rejects_incompatible_authored_targets_at_copy_inputs() {
    for (field, value, expected_code, expected_path) in [
        (
            "format",
            json!("depth32_float"),
            "GRAPH_ILLEGAL_ACCESS",
            "nodes[8].inputs",
        ),
        (
            "dimension",
            json!("d3"),
            "GRAPH_UNSUPPORTED_FEATURE",
            "nodes[2].parameters.texture.dimension",
        ),
        (
            "sampleCount",
            json!(4),
            "GRAPH_UNSUPPORTED_FEATURE",
            "nodes[2].parameters.texture.sampleCount",
        ),
        (
            "mipLevelCount",
            json!(2),
            "GRAPH_UNSUPPORTED_FEATURE",
            "nodes[2].parameters.texture.mipLevelCount",
        ),
    ] {
        let mut target = texture("rgba16_float", "transient");
        target["texture"][field] = value;
        let mut nodes = vec![
            node(
                "color",
                "texture",
                texture("rgba8_unorm", "transient"),
                json!({}),
            ),
            node(
                "source",
                "texture",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            node("target", "texture", target, json!({})),
            depth_spec("depth", "transient"),
        ];
        nodes.extend(render_support_nodes());
        nodes.extend([
            pipeline_node("source_writer", input("source","texture"), input("depth","texture")),
            node(
                "copy",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("source_writer","color"),"colorTarget":input("target","texture")}),
            ),
            node(
                "to_surface",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("copy","color"),"colorTarget":input("color","texture")}),
            ),
            node(
                "frame_out",
                "frame_out",
                json!({}),
                json!({"color":input("to_surface","color")}),
            ),
        ]);
        let error = compile_error(graph(nodes));
        assert_eq!(error.code, expected_code, "field {field}");
        assert_eq!(error.details["path"], expected_path, "field {field}");
    }
}

#[test]
fn runtime_rejects_coordinated_executor_frustum_and_fullscreen_mutations() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();

    let mut graph = baseline.clone();
    graph.executions[0].executor.key = "unknown_executor".into();
    graph.executions[0].inputs.push(CompiledSocketInput {
        socket: "bad".into(),
        resource: u32::MAX,
    });
    let error = validate_activatable(&graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_EXECUTION_UNSUPPORTED");
    assert_eq!(error.details["path"], "executions[0]");

    let cull = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "frustum_cull")
        .unwrap();
    let mut graph = baseline.clone();
    graph.executions[cull].kind = ExecutionKind::Compute {
        work: ComputeWork::MeshQuery,
    };
    assert_runtime_path(&graph, format!("executions[{cull}].kind"));
    let mut graph = baseline.clone();
    graph.executions[cull].inputs.swap(0, 1);
    graph.executions[cull].accesses.swap(0, 1);
    assert_runtime_path(&graph, format!("executions[{cull}].inputs"));

    let post = compile_graph(hdr_copy_graph());
    validate_activatable(&post).unwrap();
    let copy = post
        .executions
        .iter()
        .position(|execution| execution.executor.key == "fullscreen_copy")
        .unwrap();
    let mut graph = post.clone();
    let ExecutionKind::Render {
        color_attachments, ..
    } = &mut graph.executions[copy].kind
    else {
        unreachable!()
    };
    color_attachments[0].store = StoreOp::Discard;
    if let AccessMode::ColorAttachment { store, .. } = &mut graph.executions[copy].accesses[1].mode
    {
        *store = StoreOp::Discard;
    }
    assert_runtime_path(&graph, format!("executions[{copy}].kind"));
}

fn frame_parameters(hdr: bool) -> Value {
    json!({"hdrEnabled":hdr,"toneMapper":"reinhard","exposureStops":2,
        "outputTransfer":"srgb","scaleMode":"contain","filter":"nearest",
        "backgroundColor":[0.1,0.2,0.3,0.4]})
}

#[test]
fn frame_out_v2_has_exact_seven_fields_normalizes_sdr_and_rejects_v1() {
    let fields = [
        "hdrEnabled",
        "toneMapper",
        "exposureStops",
        "outputTransfer",
        "scaleMode",
        "filter",
        "backgroundColor",
    ];
    for field in fields {
        let mut g = full_cull_graph();
        let i = node_index(&g, "frame_out");
        g["nodes"][i]["parameters"] = frame_parameters(false);
        g["nodes"][i]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(
            compile_error(g).code,
            "GRAPH_PARAMETERS_INVALID",
            "missing {field}"
        );
    }
    let mut g = full_cull_graph();
    let i = node_index(&g, "frame_out");
    g["nodes"][i]["parameters"] = frame_parameters(false);
    g["nodes"][i]["parameters"]["extra"] = json!(0);
    assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    for (field, bad) in [
        ("toneMapper", json!("bad")),
        ("exposureStops", json!(11)),
        ("backgroundColor", json!([0, 0, -0.1, 1])),
    ] {
        let mut g = full_cull_graph();
        let i = node_index(&g, "frame_out");
        g["nodes"][i]["parameters"] = frame_parameters(false);
        g["nodes"][i]["parameters"][field] = bad;
        assert_eq!(
            compile_error(g).code,
            "GRAPH_PARAMETERS_INVALID",
            "hidden {field}"
        );
    }
    let mut g = full_cull_graph();
    let i = node_index(&g, "frame_out");
    g["nodes"][i]["parameters"] = frame_parameters(false);
    assert!(matches!(
        execution(&compile_graph(g), "frame_out").parameters,
        NormalizedParameters::FrameOut {
            dynamic_range: FrameDynamicRange::Sdr,
            ..
        }
    ));
    let mut g = full_cull_graph();
    let i = node_index(&g, "frame_out");
    g["nodes"][i]["executor"]["version"] = json!(1);
    assert_eq!(compile_error(g).code, "GRAPH_EXECUTOR_VERSION_UNSUPPORTED");
}

#[test]
fn frame_out_source_format_matrix_is_exact() {
    let surface = RuntimeSurfaceContract {
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: 1280,
        height: 720,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: vec![],
    };
    for (format, sdr, hdr) in [
        ("rgba8_unorm", true, false),
        ("bgra8_unorm", true, false),
        ("rgba16_float", true, true),
        ("rgba8_unorm_srgb", false, false),
        ("bgra8_unorm_srgb", false, false),
        ("r32_float", false, false),
        ("depth32_float", false, false),
    ] {
        for (hdr_enabled, accepted) in [(false, sdr), (true, hdr)] {
            let mut g = full_cull_graph();
            let i = node_index(&g, "frame_out");
            if format == "depth32_float" {
                g["nodes"][i]["inputs"]["color"] = input("pipeline_main", "depth");
            } else {
                g["nodes"][0]["parameters"]["texture"]["format"] = json!(format);
            }
            g["nodes"][i]["parameters"] = frame_parameters(hdr_enabled);
            match compile(serde_json::from_value(g).unwrap()) {
                Ok(compiled) => {
                    assert!(
                        accepted,
                        "unexpected accept: hdr={hdr_enabled} format={format}"
                    );
                    prepare_runtime_plan(&compiled, surface.clone(), None).unwrap();
                }
                Err(error) => {
                    assert!(
                        !accepted,
                        "unexpected reject: hdr={hdr_enabled} format={format}"
                    );
                    let expected_message = if hdr_enabled {
                        "HDR frame output requires rgba16_float"
                    } else {
                        "SDR frame output requires a linear filterable color texture"
                    };
                    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
                    assert_eq!(error.details["path"], format!("nodes[{i}].inputs.color"));
                    assert_eq!(error.message, expected_message);
                    assert_eq!(error.details["message"], expected_message);
                }
            }
        }
    }
}

#[test]
fn runtime_rejects_every_frame_parameter_lane_and_coordinated_source_mutations() {
    let baseline = compile_graph(full_cull_graph());
    let i = baseline
        .executions
        .iter()
        .position(|e| e.executor.key == "frame_out")
        .unwrap();
    for version in [1, 3] {
        let mut g = baseline.clone();
        g.executions[i].executor.version = version;
        assert_runtime_path(&g, format!("executions[{i}].executor.version"));
    }
    for lane in 0..4 {
        for value in [f32::NAN, -0.1, 1.1] {
            let mut g = baseline.clone();
            let NormalizedParameters::FrameOut {
                background_color, ..
            } = &mut g.executions[i].parameters
            else {
                unreachable!()
            };
            background_color[lane] = value;
            assert_runtime_path(&g, format!("executions[{i}].parameters"));
        }
    }
    for exposure in [f32::NAN, -10.1, 10.1] {
        let mut g = baseline.clone();
        let NormalizedParameters::FrameOut { dynamic_range, .. } = &mut g.executions[i].parameters
        else {
            unreachable!()
        };
        *dynamic_range = FrameDynamicRange::Hdr {
            tone_mapper: ToneMapper::Aces,
            exposure_stops: exposure,
        };
        assert_runtime_path(&g, format!("executions[{i}].parameters"));
    }
    let ExecutionKind::FrameOut { color } = baseline.executions[i].kind else {
        unreachable!()
    };
    let family = match baseline.resources[color as usize].plan {
        ResourcePlan::Texture { family, .. } => family,
        _ => unreachable!(),
    } as usize;
    for format in [
        TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8UnormSrgb,
        TextureFormat::R32Float,
    ] {
        let mut g = baseline.clone();
        mutate_texture_descriptor(&mut g, color, |descriptor| descriptor.format = format);
        assert_runtime_path(&g, format!("textureFamilies[{family}].source.descriptor"));
    }

    let mut sdr_to_hdr = baseline.clone();
    let NormalizedParameters::FrameOut { dynamic_range, .. } =
        &mut sdr_to_hdr.executions[i].parameters
    else {
        unreachable!()
    };
    *dynamic_range = FrameDynamicRange::Hdr {
        tone_mapper: ToneMapper::Aces,
        exposure_stops: 0.,
    };
    assert_runtime_path(
        &sdr_to_hdr,
        format!("textureFamilies[{family}].source.descriptor"),
    );

    let mut hdr_value = full_cull_graph();
    hdr_value["nodes"][0]["parameters"]["texture"]["format"] = json!("rgba16_float");
    let frame_node = node_index(&hdr_value, "frame_out");
    hdr_value["nodes"][frame_node]["parameters"] = frame_parameters(true);
    let mut hdr_to_sdr = compile_graph(hdr_value);
    let hdr_frame = hdr_to_sdr
        .executions
        .iter()
        .position(|e| e.executor.key == "frame_out")
        .unwrap();
    let NormalizedParameters::FrameOut {
        dynamic_range,
        output_transfer,
        ..
    } = &mut hdr_to_sdr.executions[hdr_frame].parameters
    else {
        unreachable!()
    };
    *dynamic_range = FrameDynamicRange::Sdr;
    *output_transfer = OutputTransfer::Linear;
    let linear_surface = RuntimeSurfaceContract {
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: 1280,
        height: 720,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: vec![],
    };
    prepare_runtime_plan(&hdr_to_sdr, linear_surface.clone(), None).unwrap();
    let error = prepare_runtime_plan(
        &hdr_to_sdr,
        RuntimeSurfaceContract {
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            ..linear_surface
        },
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_SURFACE_INCOMPATIBLE");
    assert_eq!(
        error.details["path"],
        format!("executions[{hdr_frame}].parameters.outputTransfer")
    );
}

#[test]
fn runtime_rejects_mesh_query_predicate_shape_mutations() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let query = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "mesh_query")
        .unwrap();

    let mut graph = baseline.clone();
    let removed = graph.executions[query].inputs.pop().unwrap().resource;
    graph.executions[query].accesses.remove(2);
    let producer = graph.resources[removed as usize]
        .producer_execution
        .unwrap();
    graph.resources[removed as usize].lifetime = Some(Lifetime {
        first_use: producer,
        last_use: producer,
    });
    assert_runtime_path(&graph, format!("executions[{query}].inputs"));

    let mut graph = baseline.clone();
    let NormalizedParameters::MeshQuery {
        frustum_culled_predicate,
        ..
    } = &mut graph.executions[query].parameters
    else {
        unreachable!()
    };
    *frustum_culled_predicate = RuntimePredicate::Any;
    assert_runtime_path(&graph, format!("executions[{query}].inputs"));

    let mut graph = baseline;
    let NormalizedParameters::MeshQuery {
        visible_predicate, ..
    } = &mut graph.executions[query].parameters
    else {
        unreachable!()
    };
    *visible_predicate = RuntimePredicate::Never;
    assert_runtime_path(&graph, format!("executions[{query}].parameters"));
}

#[test]
fn runtime_rejects_fullscreen_parameter_and_sample_order_mutations() {
    let baseline = compile_graph(hdr_copy_graph());
    validate_activatable(&baseline).unwrap();
    let copy = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "fullscreen_copy")
        .unwrap();
    let invalid_parameters = [
        (
            "bloom_extract",
            NormalizedParameters::BloomExtract {
                threshold: -0.1,
                knee: 0.5,
            },
        ),
        (
            "bloom_extract",
            NormalizedParameters::BloomExtract {
                threshold: 1.0,
                knee: f32::NAN,
            },
        ),
        (
            "bloom_blur",
            NormalizedParameters::BloomBlur {
                direction: [0.5, 0.5],
                radius: 0.9,
            },
        ),
        (
            "bloom_blur",
            NormalizedParameters::BloomBlur {
                direction: [f32::NAN, 0.0],
                radius: 1.0,
            },
        ),
        (
            "bloom_composite",
            NormalizedParameters::BloomComposite { intensity: 16.1 },
        ),
        (
            "luminance_edge",
            NormalizedParameters::LuminanceEdge { strength: -0.1 },
        ),
    ];
    for (key, parameters) in invalid_parameters {
        let mut graph = baseline.clone();
        graph.executions[copy].executor.key = key.into();
        graph.executions[copy].parameters = parameters;
        assert_runtime_path(&graph, format!("executions[{copy}].parameters"));
    }

    let mut graph = compile_graph(bloom_composite_graph());
    validate_activatable(&graph).unwrap();
    let composite = graph
        .executions
        .iter()
        .position(|execution| execution.executor.key == "bloom_composite")
        .unwrap();
    graph.executions[composite].accesses.swap(0, 1);
    assert_runtime_path(&graph, format!("executions[{composite}].accesses"));
}

#[test]
fn runtime_requires_exact_query_and_draw_stream_producers() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let query = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "mesh_query")
        .unwrap();
    let pipeline = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline")
        .unwrap();

    let mut graph = baseline.clone();
    let original = graph.executions[query].inputs[2].resource;
    let mut synthetic = graph.resources[original as usize].clone();
    synthetic.producer_execution = None;
    synthetic.lifetime = Some(Lifetime {
        first_use: query as u32,
        last_use: query as u32,
    });
    let replacement = graph.resources.len() as u32;
    graph.resources.push(synthetic);
    graph.executions[query].inputs[2].resource = replacement;
    graph.executions[query].accesses[2].resource = replacement;
    let producer = graph.resources[original as usize]
        .producer_execution
        .unwrap();
    graph.resources[original as usize].lifetime = Some(Lifetime {
        first_use: producer,
        last_use: producer,
    });
    assert_runtime_path(&graph, format!("executions[{query}].inputs"));

    let mut graph = baseline;
    let original = graph.executions[pipeline].inputs[1].resource;
    let mut synthetic = graph.resources[original as usize].clone();
    synthetic.producer_execution = None;
    synthetic.lifetime = Some(Lifetime {
        first_use: pipeline as u32,
        last_use: pipeline as u32,
    });
    let replacement = graph.resources.len() as u32;
    graph.resources.push(synthetic);
    graph.executions[pipeline].inputs[1].resource = replacement;
    graph.executions[pipeline].accesses[1].resource = replacement;
    graph.resources[original as usize].lifetime = Some(Lifetime {
        first_use: query as u32,
        last_use: query as u32,
    });
    assert_runtime_path(&graph, format!("executions[{pipeline}].inputs"));
}

#[test]
fn runtime_rechecks_pipeline_attachment_descriptors() {
    let baseline = compile_graph(full_cull_graph());
    validate_activatable(&baseline).unwrap();
    let pipeline = baseline
        .executions
        .iter()
        .position(|execution| execution.executor.key == "pipeline")
        .unwrap();
    let attachment = |graph: &CompiledGraph, socket: &str| {
        graph.executions[pipeline]
            .outputs
            .iter()
            .find(|output| output.socket == socket)
            .unwrap()
            .resource
    };
    let mut graph = baseline.clone();
    let color = attachment(&graph, "color");
    mutate_texture_descriptor(&mut graph, color, |descriptor| {
        descriptor.format = TextureFormat::Depth32Float;
    });
    assert_runtime_path(&graph, format!("executions[{pipeline}].inputs"));

    let mut graph = baseline.clone();
    let depth = attachment(&graph, "depth");
    mutate_texture_descriptor(&mut graph, depth, |descriptor| {
        descriptor.format = TextureFormat::Rgba16Float;
    });
    assert_runtime_path(&graph, format!("executions[{pipeline}].inputs"));

    let mut graph = baseline;
    let color = attachment(&graph, "color");
    mutate_texture_descriptor(&mut graph, color, |descriptor| {
        descriptor.extent = NormalizedTextureExtent::Absolute {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        };
    });
    assert_runtime_path(&graph, format!("executions[{pipeline}].inputs"));
}
