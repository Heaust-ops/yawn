use std::collections::BTreeSet;

use super::*;
use serde_json::{json, Value};

fn input(node: &str, socket: &str) -> Value {
    json!({"node":node,"socket":socket})
}
fn node(id: &str, key: &str, parameters: Value, inputs: Value) -> Value {
    json!({"id":id,"state":"enabled","executor":{"key":key,"version":1},"parameters":parameters,"inputs":inputs})
}
fn texture(format: &str, residency: &str) -> Value {
    json!({"texture":{"dimension":"d2","format":format,"extent":{"kind":"surface_relative","width":{"numerator":1,"denominator":1},"height":{"numerator":1,"denominator":1},"depthOrArrayLayers":1},"mipLevelCount":1,"sampleCount":1,"viewFormats":[]},"residency":residency})
}
fn full_cull_graph() -> Value {
    json!({"schemaVersion":2,"graphId":"full","revision":1,"nodes":[
        node("surface","surface_target",json!({}),json!({})),
        node("depth","texture_spec",texture("depth32_float","transient"),json!({})),
        node("scene","scene_table",json!({}),json!({})),
        node("aabbs","local_aabb_buffer",json!({}),json!({"scene":input("scene","scene")})),
        node("frustum","camera_frustum",json!({}),json!({})),
        node("visible","visibility_flags",json!({}),json!({"scene":input("scene","scene")})),
        node("cull","frustum_cull",json!({}),json!({"scene":input("scene","scene"),"localAabbs":input("aabbs","localAabbs"),"frustum":input("frustum","frustum")})),
        node("query","mesh_query",json!({"filters":[{"flag":"isFrustumCulled","predicate":"required_false"},{"flag":"isVisible","predicate":"required_true"}]}),json!({"scene":input("scene","scene"),"isVisible":input("visible","flags"),"isFrustumCulled":input("cull","flags")})),
        node("depth_config","depth_stencil_config",json!({"depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0}),json!({})),
        node("forward","legacy_forward",json!({"clearColor":[0,0,0,1]}),json!({"scene":input("scene","scene"),"draws":input("query","draws"),"colorTarget":input("surface","surface"),"depthTarget":input("depth","spec"),"depthStencil":input("depth_config","config")})),
        node("present","present",json!({}),json!({"surface":input("forward","color")}))
    ]})
}
fn forward(id: &str, color: Value, depth: Value) -> Value {
    node(
        id,
        "legacy_forward",
        json!({"clearColor":[0,0,0,1]}),
        json!({
            "scene":input("scene","scene"),
            "draws":input("query","draws"),
            "colorTarget":color,
            "depthTarget":depth,
            "depthStencil":input("depth_config","config")
        }),
    )
}
fn render_support_nodes() -> Vec<Value> {
    vec![
        node("scene", "scene_table", json!({}), json!({})),
        node(
            "visible",
            "visibility_flags",
            json!({}),
            json!({"scene":input("scene","scene")}),
        ),
        node(
            "query",
            "mesh_query",
            json!({"filters":[
                {"flag":"isVisible","predicate":"required_true"},
                {"flag":"isFrustumCulled","predicate":"any"}
            ]}),
            json!({"scene":input("scene","scene"),"isVisible":input("visible","flags")}),
        ),
        node(
            "depth_config",
            "depth_stencil_config",
            json!({"depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0}),
            json!({}),
        ),
    ]
}
fn graph(nodes: Vec<Value>) -> Value {
    json!({"schemaVersion":2,"graphId":"hazards","revision":1,"nodes":nodes})
}
fn hdr_copy_graph() -> Value {
    let mut nodes = vec![
        node("surface", "surface_target", json!({}), json!({})),
        node(
            "hdr",
            "texture_spec",
            texture("rgba16_float", "transient"),
            json!({}),
        ),
        depth_spec("depth", "transient"),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        forward("forward", input("hdr", "spec"), input("depth", "spec")),
        node(
            "copy",
            "fullscreen_copy",
            json!({}),
            json!({"source":input("forward","color"),"colorTarget":input("surface","surface")}),
        ),
        node(
            "present",
            "present",
            json!({}),
            json!({"surface":input("copy","color")}),
        ),
    ]);
    graph(nodes)
}
fn cyclic_forwards() -> Vec<Value> {
    vec![
        forward("A", input("B", "color"), input("B", "depth")),
        forward("B", input("A", "color"), input("A", "depth")),
    ]
}
fn compile(v: Value) -> CompiledGraphV2 {
    compile_v2(serde_json::from_value(v).unwrap()).unwrap()
}

#[test]
fn fullscreen_copy_hdr_graph_lowers_versions_accesses_and_usage() {
    let p = compile(hdr_copy_graph());
    assert_eq!(
        p.executions
            .iter()
            .map(|execution| execution.id.as_str())
            .collect::<Vec<_>>(),
        ["query", "forward", "copy", "present"]
    );
    for (node, socket) in [
        ("forward", "color"),
        ("forward", "depth"),
        ("copy", "color"),
    ] {
        assert!(matches!(
            resource_by_origin(&p, node, socket).plan,
            ResourcePlanV2::Texture { version: 0, .. }
        ));
    }
    let copy = execution(&p, "copy");
    let source = resource_by_origin(&p, "forward", "color");
    let color = resource_by_origin(&p, "copy", "color");
    assert!(copy.accesses.iter().any(|access| access.resource
        == p.resources
            .iter()
            .position(|resource| std::ptr::eq(resource, source))
            .unwrap() as u32
        && access.mode == AccessModeV2::SampledTexture));
    let color_id = p
        .resources
        .iter()
        .position(|resource| std::ptr::eq(resource, color))
        .unwrap() as u32;
    assert!(matches!(
        &copy.kind,
        ExecutionKindV2::Render { color_attachments, depth_stencil: None }
            if color_attachments[0].resource == color_id
                && color_attachments[0].load == NormalizedColorLoadV2::Clear { value: [0.0; 4] }
    ));
    assert!(copy.accesses.iter().any(|access| matches!(
        access.mode,
        AccessModeV2::ColorAttachment {
            full_overwrite: true,
            ..
        }
    ) && access.resource == color_id));
    let hdr = family_by_source(&p, "hdr");
    assert_eq!(
        hdr.usage.iter().copied().collect::<BTreeSet<_>>(),
        [TextureUsageV2::Sampled, TextureUsageV2::ColorAttachment]
            .into_iter()
            .collect()
    );
    let surface = p
        .texture_families
        .iter()
        .find(|family| matches!(family.source, TextureFamilySourceV2::ImportedSurface { .. }))
        .unwrap();
    assert_eq!(surface.versions[0].resource, color_id);
}

#[test]
fn fullscreen_copy_parameters_are_exactly_empty() {
    let mut g = hdr_copy_graph();
    g["nodes"][8]["parameters"] = json!({"obsolete":true});
    assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    assert_eq!(CONTRACTS_V2.len(), 17);
}

#[test]
fn fullscreen_copy_rejects_same_source_and_target_family() {
    let mut g = hdr_copy_graph();
    g["nodes"][8]["inputs"]["colorTarget"] = input("forward", "color");
    let error = compile_error(g);
    assert_eq!(error.code, "GRAPH_SAME_PASS_HAZARD");
    assert_eq!(error.details["path"], "nodes[8].inputs");
}

#[test]
fn duplicate_texture_writer_reports_second_color_target() {
    let mut nodes = vec![node("surface", "surface_target", json!({}), json!({}))];
    nodes.extend(render_support_nodes());
    nodes.extend([
        node(
            "depth_a",
            "texture_spec",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        node(
            "depth_b",
            "texture_spec",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        forward("F0", input("surface", "surface"), input("depth_a", "spec")),
        node(
            "P0",
            "present",
            json!({}),
            json!({"surface":input("F0","color")}),
        ),
        forward("F1", input("surface", "surface"), input("depth_b", "spec")),
        node(
            "P1",
            "present",
            json!({}),
            json!({"surface":input("F1","color")}),
        ),
    ]);
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_DUPLICATE_WRITER");
    assert_eq!(error.details["path"], "nodes[9].inputs.colorTarget");
}

#[test]
fn same_output_bound_to_both_attachments_is_a_same_pass_hazard() {
    let mut nodes = vec![node(
        "target",
        "texture_spec",
        texture("rgba8_unorm", "transient"),
        json!({}),
    )];
    nodes.extend(render_support_nodes());
    nodes.extend([
        forward("F", input("target", "spec"), input("target", "spec")),
        node(
            "P",
            "present",
            json!({}),
            json!({"surface":input("F","color")}),
        ),
    ]);
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_SAME_PASS_HAZARD");
    assert_eq!(error.details["path"], "nodes[5].inputs");
}

#[test]
fn unordered_old_texture_version_read_is_rejected_before_scheduling() {
    let mut nodes = vec![
        node("surface", "surface_target", json!({}), json!({})),
        node(
            "depth_0",
            "texture_spec",
            texture("depth32_float", "transient"),
            json!({}),
        ),
        node(
            "depth_1",
            "texture_spec",
            texture("depth32_float", "transient"),
            json!({}),
        ),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        forward("F0", input("surface", "surface"), input("depth_0", "spec")),
        forward("F1", input("F0", "color"), input("depth_1", "spec")),
        node(
            "P0",
            "present",
            json!({}),
            json!({"surface":input("F0","color")}),
        ),
        node(
            "P1",
            "present",
            json!({}),
            json!({"surface":input("F1","color")}),
        ),
    ]);
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_RESOURCE_VERSION_INVALID");
    assert_eq!(error.details["path"], "nodes[9].inputs.surface");
}

#[test]
fn duplicate_successors_defer_old_version_reachability() {
    let mut nodes = vec![
        node("surface", "surface_target", json!({}), json!({})),
        depth_spec("depth_0", "transient"),
        depth_spec("depth_1", "transient"),
        depth_spec("depth_2", "transient"),
    ];
    nodes.extend(render_support_nodes());
    nodes.extend([
        forward("F0", input("surface", "surface"), input("depth_0", "spec")),
        forward("F1", input("F0", "color"), input("depth_1", "spec")),
        forward("F2", input("F0", "color"), input("depth_2", "spec")),
        node(
            "P0",
            "present",
            json!({}),
            json!({"surface":input("F0","color")}),
        ),
        node(
            "P1",
            "present",
            json!({}),
            json!({"surface":input("F1","color")}),
        ),
        node(
            "P2",
            "present",
            json!({}),
            json!({"surface":input("F2","color")}),
        ),
    ]);
    let error = compile_error(graph(nodes));
    assert_eq!(error.code, "GRAPH_DUPLICATE_WRITER");
    assert_eq!(error.details["path"], "nodes[10].inputs.colorTarget");
}

#[test]
fn live_texture_cycle_reports_the_exact_first_cycle() {
    let mut nodes = render_support_nodes();
    nodes.extend(cyclic_forwards());
    nodes.push(node(
        "present",
        "present",
        json!({}),
        json!({"surface":input("A","color")}),
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
        .extend(cyclic_forwards());
    let plan = compile(value);
    assert_eq!(plan.node_count, 13);
    assert_eq!(plan.culled_node_count, 2);
    assert_eq!(plan.culled_resource_count, 4);
    assert!(!plan
        .executions
        .iter()
        .any(|execution| matches!(execution.id.as_str(), "A" | "B")));
}
fn compile_error(v: Value) -> GraphError {
    compile_v2(serde_json::from_value(v).unwrap()).unwrap_err()
}
fn execution<'a>(p: &'a CompiledGraphV2, authored_id: &str) -> &'a CompiledExecutionV2 {
    p.executions.iter().find(|e| e.id == authored_id).unwrap()
}
fn resource_by_origin<'a>(
    p: &'a CompiledGraphV2,
    node: &str,
    socket: &str,
) -> &'a CompiledResourceV2 {
    p.resources
        .iter()
        .find(|r| r.origin.node == node && r.origin.socket == socket)
        .unwrap()
}

fn family_by_source<'a>(p: &'a CompiledGraphV2, node: &str) -> &'a TextureFamilyV2 {
    let source = resource_by_origin(p, node, "spec");
    let family = match source.plan {
        ResourcePlanV2::TextureSpec { family, .. } => family,
        _ => panic!("{node} is not a texture specification"),
    };
    &p.texture_families[family as usize]
}

fn allocation_slot<'a>(
    p: &'a CompiledGraphV2,
    allocation: AllocationRefV2,
) -> &'a AllocationSlotV2 {
    &p.allocation_classes[allocation.class as usize].slots[allocation.slot as usize]
}

fn independent_depth_graph(
    depth_specs: Vec<Value>,
    forwards: Vec<Value>,
    present_from: &str,
) -> Value {
    let mut nodes = vec![node("surface", "surface_target", json!({}), json!({}))];
    nodes.extend(render_support_nodes());
    nodes.extend(depth_specs);
    nodes.extend(forwards);
    nodes.push(node(
        "present",
        "present",
        json!({}),
        json!({"surface":input(present_from,"color")}),
    ));
    graph(nodes)
}

fn depth_spec(id: &str, residency: &str) -> Value {
    node(
        id,
        "texture_spec",
        texture("depth32_float", residency),
        json!({}),
    )
}

#[test]
fn dense_lifetimes_exclude_authored_source_ordinals() {
    let p = compile(full_cull_graph());
    assert_eq!(
        p.executions
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["cull", "query", "forward", "present"]
    );
    for (node, socket, first, last) in [
        ("scene", "scene", 0, 2),
        ("aabbs", "localAabbs", 0, 0),
        ("frustum", "frustum", 0, 0),
        ("visible", "flags", 1, 1),
        ("depth_config", "config", 2, 2),
    ] {
        assert_eq!(
            resource_by_origin(&p, node, socket).lifetime,
            Some(LifetimeV2 {
                first_use: first,
                last_use: last
            }),
            "lifetime for {node}.{socket}"
        );
    }
    let color = resource_by_origin(&p, "forward", "color");
    let depth = resource_by_origin(&p, "forward", "depth");
    assert_eq!(color.producer_execution, Some(2));
    assert_eq!(depth.producer_execution, Some(2));
    assert_eq!(
        color.lifetime,
        Some(LifetimeV2 {
            first_use: 2,
            last_use: 3
        })
    );
    assert_eq!(
        depth.lifetime,
        Some(LifetimeV2 {
            first_use: 2,
            last_use: 2
        })
    );
    let depth_family = family_by_source(&p, "depth");
    assert_eq!(
        depth_family.lifetime,
        LifetimeV2 {
            first_use: 2,
            last_use: 2
        }
    );
    assert_eq!(depth_family.versions[0].lifetime, depth_family.lifetime);
}

#[test]
fn transient_aliasing_is_declaration_order_independent() {
    let p = compile(independent_depth_graph(
        vec![
            depth_spec("depth_second", "transient"),
            depth_spec("depth_first", "transient"),
        ],
        vec![
            forward(
                "F0",
                input("surface", "surface"),
                input("depth_first", "spec"),
            ),
            forward("F1", input("F0", "color"), input("depth_second", "spec")),
        ],
        "F1",
    ));
    assert_eq!(execution(&p, "F1").original_node_index, 8);
    let f0_ordinal = p.executions.iter().position(|e| e.id == "F0").unwrap() as u32;
    let f1_ordinal = p.executions.iter().position(|e| e.id == "F1").unwrap() as u32;
    assert_eq!(f1_ordinal, f0_ordinal + 1);
    let first = family_by_source(&p, "depth_first");
    let second = family_by_source(&p, "depth_second");
    assert_eq!(
        first.lifetime,
        LifetimeV2 {
            first_use: f0_ordinal,
            last_use: f0_ordinal
        }
    );
    assert_eq!(
        second.lifetime,
        LifetimeV2 {
            first_use: f1_ordinal,
            last_use: f1_ordinal
        }
    );
    assert_eq!(first.versions[0].lifetime, first.lifetime);
    assert_eq!(second.versions[0].lifetime, second.lifetime);
    assert_eq!(first.allocation, second.allocation);
    let slot = allocation_slot(&p, first.allocation.unwrap());
    assert_eq!(slot.kind, AllocationKindV2::AliasedTransient);
    assert_eq!(slot.usage, [TextureUsageV2::DepthAttachment]);
    assert_eq!(
        slot.occupants.iter().copied().collect::<BTreeSet<_>>(),
        [first.id, second.id].into_iter().collect()
    );
}

#[test]
fn overlapping_family_lifetimes_prevent_transient_reuse() {
    let p = compile(independent_depth_graph(
        vec![
            depth_spec("depth_a", "transient"),
            depth_spec("depth_b", "transient"),
        ],
        vec![
            forward("F0", input("surface", "surface"), input("depth_a", "spec")),
            forward("F1", input("F0", "color"), input("depth_b", "spec")),
            forward("F2", input("F1", "color"), input("F0", "depth")),
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
    let p = compile(independent_depth_graph(
        vec![
            depth_spec("persistent_b", "persistent"),
            depth_spec("transient", "transient"),
            depth_spec("persistent_a", "persistent"),
        ],
        vec![
            forward(
                "F0",
                input("surface", "surface"),
                input("persistent_a", "spec"),
            ),
            forward("F1", input("F0", "color"), input("transient", "spec")),
            forward("F2", input("F1", "color"), input("persistent_b", "spec")),
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
            AllocationKindV2::Persistent
        );
        assert_eq!(family.usage, [TextureUsageV2::DepthAttachment]);
        for version in &family.versions {
            let ResourcePlanV2::Texture {
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
    assert_eq!(p.transient_slot_count, 1);
}

#[test]
fn exact_texture_compatibility_separates_allocation_classes() {
    let mut different = texture("depth32_float", "transient");
    different["texture"]["mipLevelCount"] = json!(2);
    let p = compile(independent_depth_graph(
        vec![
            depth_spec("relative", "transient"),
            node("absolute", "texture_spec", different, json!({})),
        ],
        vec![
            forward("F0", input("surface", "surface"), input("relative", "spec")),
            forward("F1", input("F0", "color"), input("absolute", "spec")),
        ],
        "F1",
    ));
    let relative = family_by_source(&p, "relative").allocation.unwrap();
    let absolute = family_by_source(&p, "absolute").allocation.unwrap();
    assert_ne!(relative.class, absolute.class);
    assert_ne!(relative, absolute);
}

#[test]
fn dispatch_and_registry_are_version_isolated() {
    let bytes =
        serde_json::to_vec(&json!({"schemaVersion":2,"graphId":"empty","revision":1,"nodes":[]}))
            .unwrap();
    assert_eq!(
        parse_and_compile(&bytes).unwrap_err().code,
        "GRAPH_SCHEMA_UNSUPPORTED"
    );
    assert!(matches!(
        parse_and_compile_any(&bytes).unwrap(),
        RegisteredGraph::V2(_)
    ));
    let mut r = Registry::default();
    let (id, _) = r.compile(&bytes).unwrap();
    assert!(matches!(
        r.get_registered(id).unwrap(),
        RegisteredGraph::V2(_)
    ));
    assert_eq!(
        r.get(id).unwrap_err().message,
        "schemaVersion 2 activation is unavailable until Phase 4"
    );
}

#[test]
fn authoritative_eleven_node_graph_lowers_exactly() {
    let p = compile(full_cull_graph());
    assert_eq!(p.node_count, 11);
    assert_eq!(p.resources.len(), 11);
    assert_eq!(
        p.executions
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["cull", "query", "forward", "present"]
    );
    for resource in &p.resources {
        if let ResourcePlanV2::Texture { version, .. } = resource.plan {
            assert_eq!(version, 0, "every first produced texture is symbolic v0");
        }
    }
    assert!(p.executions.iter().all(|e| !matches!(
        e.executor.key.as_str(),
        "surface_target" | "texture_spec" | "scene_table"
    )));
}

#[test]
fn exact_wire_catalog_rejections() {
    let cases = [
        ("local_aabb", 0, "GRAPH_UNKNOWN_EXECUTOR"),
        ("frustum", 4, "GRAPH_UNKNOWN_EXECUTOR"),
        ("cull", 6, "GRAPH_UNKNOWN_EXECUTOR"),
    ];
    for (key, i, code) in cases {
        let mut g = full_cull_graph();
        g["nodes"][i]["executor"]["key"] = json!(key);
        assert_eq!(compile_error(g).code, code);
    }
    for field in ["clearDepth", "clearColor"] {
        let mut g = full_cull_graph();
        let i = if field == "clearDepth" { 8 } else { 9 };
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
fn mesh_filters_are_closed_and_any_removes_dependency() {
    let p = compile(full_cull_graph());
    let NormalizedParametersV2::MeshQuery { filters } = execution(&p, "query").parameters.clone()
    else {
        panic!()
    };
    assert_eq!(
        filters.map(|f| f.flag),
        [MeshFlagV2::IsVisible, MeshFlagV2::IsFrustumCulled]
    );
    for filters in [
        json!([{"flag":"isVisible","predicate":"any"}]),
        json!([{"flag":"isVisible","predicate":"any"},{"flag":"isVisible","predicate":"required_true"}]),
        json!([{"flag":"bogus","predicate":"any"},{"flag":"isVisible","predicate":"any"}]),
    ] {
        let mut g = full_cull_graph();
        g["nodes"][7]["parameters"]["filters"] = filters;
        assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    }
    let mut g = full_cull_graph();
    // The authored order is [culled, visible], while normalization is catalog order.
    g["nodes"][7]["parameters"]["filters"][0]["predicate"] = json!("any");
    let p = compile(g);
    let q = execution(&p, "query");
    assert!(!q.inputs.iter().any(|x| x.socket == "isFrustumCulled"));
    assert!(!p.executions.iter().any(|e| e.id == "cull"));
    assert!(!p
        .resources
        .iter()
        .any(|r| r.origin.node == "cull" && r.origin.socket == "flags"));
}

#[test]
fn provenance_and_lowering_are_consistent() {
    let p = compile(full_cull_graph());
    let scene = resource_by_origin(&p, "scene", "scene");
    for id in ["aabbs", "visible", "cull", "query"] {
        let r = p.resources.iter().find(|r| r.origin.node == id).unwrap();
        match r.plan {
            ResourcePlanV2::LocalAabbBuffer { scene: s }
            | ResourcePlanV2::BooleanFlagBuffer { scene: s, .. }
            | ResourcePlanV2::DrawStream { scene: s } => {
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
    let f = execution(&p, "forward");
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
    assert!(
        f.accesses
            .iter()
            .any(|a| a.resource == color_out
                && matches!(a.mode, AccessModeV2::ColorAttachment { .. }))
    );
    assert!(!f
        .accesses
        .iter()
        .any(|a| a.resource == color_in && matches!(a.mode, AccessModeV2::ColorAttachment { .. })));
    for (socket, expected) in [
        ("scene", AccessModeV2::SemanticRead),
        ("draws", AccessModeV2::IndirectRead),
    ] {
        let access = f.accesses.iter().find(|a| a.socket == socket).unwrap();
        assert_eq!(access.mode, expected, "legacy_forward {socket} access");
    }
}

#[test]
fn descriptor_validation_and_normalization_table() {
    for (field, value) in [("sampleCount", json!(3))] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["texture"][field] = value;
        assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");
    }
    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["texture"]["extent"] =
        json!({"kind":"absolute","width":1,"height":1,"depthOrArrayLayers":1});
    g["nodes"][1]["parameters"]["texture"]["mipLevelCount"] = json!(2);
    assert_eq!(compile_error(g).code, "GRAPH_PARAMETERS_INVALID");

    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["texture"]["mipLevelCount"] = json!(99);
    assert_eq!(
        resource_by_origin(&compile(g), "depth", "spec").semantic_type,
        SemanticTypeV2::TextureSpec
    );
    for residency in ["history", "readback"] {
        let mut g = full_cull_graph();
        g["nodes"][1]["parameters"]["residency"] = json!(residency);
        assert_eq!(compile_error(g).code, "GRAPH_UNSUPPORTED_FEATURE");
    }
    let p = compile(full_cull_graph());
    let surface = p
        .texture_families
        .iter()
        .find(|f| matches!(f.source, TextureFamilySourceV2::ImportedSurface { .. }))
        .unwrap();
    assert!(surface.allocation.is_none());
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
    invalid_grammar["nodes"][10]["executor"]["key"] = json!("x".repeat(65));
    let error = compile_error(invalid_grammar);
    assert_eq!(error.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(error.details["path"], "nodes[10].executor.key");

    let mut duplicate = full_cull_graph();
    duplicate["nodes"][1]["id"] = duplicate["nodes"][0]["id"].clone();
    duplicate["nodes"][10]["executor"]["key"] = json!("x".repeat(65));
    let error = compile_error(duplicate);
    assert_eq!(error.code, "GRAPH_LIMIT_EXCEEDED");
    assert_eq!(error.details["path"], "nodes[10].executor.key");
}

#[test]
fn strict_mesh_diagnostics_and_inactive_any_edges() {
    let cases = [
        (
            json!([{"flag":"bogus","predicate":"any"},{"flag":"isVisible","predicate":"any"}]),
            "nodes[7].parameters.filters[0].flag",
        ),
        (
            json!([{"flag":"isFrustumCulled","predicate":"nope"},{"flag":"isVisible","predicate":"any"}]),
            "nodes[7].parameters.filters[0].predicate",
        ),
        (
            json!([{"flag":"isVisible","predicate":"any"},{"flag":"isVisible","predicate":"required_true"}]),
            "nodes[7].parameters.filters[1].flag",
        ),
        (
            json!([{"flag":"isVisible","predicate":"any"}]),
            "nodes[7].parameters.filters",
        ),
    ];
    for (filters, path) in cases {
        let mut g = full_cull_graph();
        g["nodes"][7]["parameters"]["filters"] = filters;
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_PARAMETERS_INVALID");
        assert_eq!(e.details["path"], path);
    }
    for predicate in ["required_true", "required_false"] {
        let mut g = full_cull_graph();
        g["nodes"][7]["parameters"]["filters"][0]["predicate"] = json!(predicate);
        g["nodes"][7]["inputs"]
            .as_object_mut()
            .unwrap()
            .remove("isFrustumCulled");
        let e = compile_error(g);
        assert_eq!(e.code, "GRAPH_SOCKET_CARDINALITY");
        assert_eq!(e.details["path"], "nodes[7].inputs.isFrustumCulled");
    }
    let mut g = full_cull_graph();
    g["nodes"][7]["inputs"]["isVisible"] = input("cull", "flags");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[7].inputs.isVisible");

    let mut g = full_cull_graph();
    g["nodes"][7]["parameters"]["filters"][0]["predicate"] = json!("any");
    let p = compile(g);
    let q = execution(&p, "query");
    assert!(!q.inputs.iter().any(|i| i.socket == "isFrustumCulled"));
    assert!(!q.accesses.iter().any(|a| a.socket == "isFrustumCulled"));
    assert!(!p.executions.iter().any(|e| e.id == "cull"));
}

#[test]
fn transitive_scene_roots_are_vector_resource_ids() {
    let p = compile(full_cull_graph());
    let scene_id = p
        .resources
        .iter()
        .position(|r| r.origin.node == "scene")
        .unwrap() as u32;
    for origin in ["aabbs", "visible", "cull", "query"] {
        let resource = p
            .resources
            .iter()
            .find(|r| r.origin.node == origin)
            .unwrap();
        let rooted = match resource.plan {
            ResourcePlanV2::LocalAabbBuffer { scene }
            | ResourcePlanV2::BooleanFlagBuffer { scene, .. }
            | ResourcePlanV2::DrawStream { scene } => Some(scene),
            _ => None,
        };
        assert_eq!(rooted, Some(scene_id));
    }
    let mut g = full_cull_graph();
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("sceneB", "scene_table", json!({}), json!({})));
    g["nodes"][3]["inputs"]["scene"] = input("sceneB", "scene");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[6].inputs.localAabbs");
    let mut g = full_cull_graph();
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("sceneB", "scene_table", json!({}), json!({})));
    g["nodes"][3]["inputs"]["scene"] = input("sceneB", "scene");
    g["nodes"][5]["inputs"]["scene"] = input("sceneB", "scene");
    g["nodes"][6]["inputs"]["scene"] = input("sceneB", "scene");
    g["nodes"][7]["inputs"]["scene"] = input("sceneB", "scene");
    let e = compile_error(g);
    assert_eq!(e.code, "GRAPH_SOCKET_TYPE_MISMATCH");
    assert_eq!(e.details["path"], "nodes[9].inputs.draws");
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
        assert_eq!(e.code, "GRAPH_PARAMETERS_INVALID");
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
    let p = compile(g);
    let TextureFamilySourceV2::AuthoredTexture { descriptor, .. } =
        &family_by_source(&p, "depth").source
    else {
        panic!()
    };
    assert!(
        matches!(&descriptor.extent, NormalizedTextureExtentV2::SurfaceRelative { width, .. } if *width == RatioV2 { numerator: 1, denominator: 1 })
    );
}

#[test]
fn descriptor_multi_error_precedence_is_exact() {
    let cases = [
        (json!(3), json!(0), 9000, "sampleCount"),
        (json!(1), json!(0), 9000, "mipLevelCount"),
        (json!(1), json!(30), 9000, "mipLevelCount"),
        (json!(1), json!(14), 9000, "extent"),
        (json!(1), json!(14), 8192, "viewFormats[0]"),
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
        assert_eq!(e.code, "GRAPH_PARAMETERS_INVALID");
        assert_eq!(
            e.details["path"],
            format!("nodes[1].parameters.texture.{expected}")
        );
    }
}

#[test]
fn global_raw_limits_have_stable_narrow_paths() {
    for (mut g, path) in [
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
                g["nodes"][9]["inputs"]["x".repeat(65)] = input("scene", "scene");
                g
            },
            "nodes[9].inputs.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][9]["inputs"]["scene"]["node"] = json!("x".repeat(65));
                g
            },
            "nodes[9].inputs.scene.node",
        ),
        (
            {
                let mut g = full_cull_graph();
                g["nodes"][9]["inputs"]["scene"]["socket"] = json!("x".repeat(65));
                g
            },
            "nodes[9].inputs.scene.socket",
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
fn empty_v2_plan_has_no_lowered_objects() {
    let p = compile(json!({"schemaVersion":2,"graphId":"empty","revision":1,"nodes":[]}));
    assert_eq!(
        (
            p.node_count,
            p.resources.len(),
            p.executions.len(),
            p.texture_families.len(),
            p.allocation_classes.len()
        ),
        (0, 0, 0, 0, 0)
    );
}

#[test]
fn registry_revision_handles_are_immutable_and_drop_is_transactional() {
    let bytes = |revision| {
        serde_json::to_vec(
            &json!({"schemaVersion":2,"graphId":"registry","revision":revision,"nodes":[]}),
        )
        .unwrap()
    };
    let mut r = Registry::new(2);
    let (id, _) = r.compile(&bytes(1)).unwrap();
    assert_eq!(
        r.compile(&bytes(1)).unwrap_err().message,
        "revision must increase"
    );
    let (second, _) = r.compile(&bytes(2)).unwrap();
    assert_ne!(id, second);
    assert!(matches!(
        r.get_registered(id).unwrap(),
        RegisteredGraph::V2(graph) if graph.revision == 1
    ));
    r.drop_graph(id).unwrap();
    assert_eq!(r.get_registered(id).unwrap_err().code, "STALE_GRAPH_ID");
    let (next, _) = r.compile(&bytes(3)).unwrap();
    assert_ne!(id, next);
    assert!(r.get_registered(second).is_ok());
}

#[test]
fn v1_parse_compile_regression() {
    let v1=br#"{"schemaVersion":1,"graphId":"v1","revision":1,"resources":[],"passes":[],"outputs":[]}"#;
    assert!(parse_and_compile(v1).is_ok());
    assert!(matches!(
        parse_and_compile_any(v1).unwrap(),
        RegisteredGraph::V1(_)
    ));
}

#[test]
fn socket_validation_is_globally_phased() {
    let mut g = full_cull_graph();
    g["nodes"][3]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("scene");
    g["nodes"][9]["inputs"]["bogus"] = input("scene", "scene");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_SOCKET", Some("nodes[9].inputs.bogus"))
    );

    let mut g = full_cull_graph();
    g["nodes"][3]["inputs"]["scene"] = input("frustum", "frustum");
    g["nodes"][9]["inputs"]["colorTarget"]["socket"] = json!("bogus");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        (
            "GRAPH_UNKNOWN_SOCKET",
            Some("nodes[9].inputs.colorTarget.socket")
        )
    );

    let mut g = full_cull_graph();
    g["nodes"][3]["inputs"]["scene"] = input("frustum", "frustum");
    g["nodes"][9]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("draws");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_SOCKET_CARDINALITY", Some("nodes[9].inputs.draws"))
    );

    let mut g = full_cull_graph();
    g["nodes"][3]["inputs"]["scene"] = input("frustum", "frustum");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_SOCKET_TYPE_MISMATCH", Some("nodes[3].inputs.scene"))
    );
}

#[test]
fn executor_version_parameter_and_state_precedence_is_global() {
    let mut g = full_cull_graph();
    g["nodes"][2]["inputs"]["bad"] = input("missing", "bad");
    g["nodes"][10]["executor"]["key"] = json!("unknown");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_NODE", Some("nodes[2].inputs.bad.node"))
    );

    let mut g = full_cull_graph();
    g["nodes"][0]["parameters"] = json!({"bad":1});
    g["nodes"][1]["state"] = json!("muted");
    g["nodes"][2]["inputs"]["bad"] = input("scene", "bad");
    g["nodes"][10]["executor"]["key"] = json!("unknown");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_UNKNOWN_EXECUTOR", Some("nodes[10].executor.key"))
    );

    let mut g = full_cull_graph();
    g["nodes"][0]["parameters"] = json!({"bad":1});
    g["nodes"][1]["state"] = json!("muted");
    g["nodes"][2]["inputs"]["bad"] = input("scene", "bad");
    g["nodes"][10]["executor"]["version"] = json!(2);
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        (
            "GRAPH_EXECUTOR_VERSION_UNSUPPORTED",
            Some("nodes[10].executor.version")
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
        assert_eq!(compile_error(g).code, "GRAPH_ILLEGAL_ACCESS");
    }

    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"] = texture("rgba8_unorm", "transient");
    g["nodes"][9]["inputs"]["colorTarget"] = input("depth", "spec");
    g["nodes"][9]["inputs"]["depthTarget"] = input("surface", "surface");
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
            .insert(2, node("color", "texture_spec", color, json!({})));
        g["nodes"][10]["inputs"]["colorTarget"] = input("color", "spec");
        assert_eq!(compile_error(g).code, "GRAPH_ILLEGAL_ACCESS", "{field}");
    }

    let mut g = full_cull_graph();
    g["nodes"].as_array_mut().unwrap().insert(
        2,
        node(
            "color",
            "texture_spec",
            texture("rgba8_unorm", "transient"),
            json!({}),
        ),
    );
    g["nodes"][10]["inputs"]["colorTarget"] = input("color", "spec");
    let e = compile_error(g);
    assert_eq!(
        (e.code, e.details["path"].as_str()),
        ("GRAPH_ILLEGAL_ACCESS", Some("nodes[11].inputs.surface"))
    );
}

#[test]
fn v2_wire_rejects_old_and_unknown_fields_exactly() {
    let mut cases = Vec::new();
    let mut g = full_cull_graph();
    g["nodes"][1]["parameters"]["descriptor"] = g["nodes"][1]["parameters"]["texture"].take();
    cases.push(g);
    for old in ["compare", "writeEnabled", "clear"] {
        let mut g = full_cull_graph();
        g["nodes"][8]["parameters"][old] = json!(1);
        cases.push(g);
    }
    for missing in ["clearDepth", "clearColor"] {
        let mut g = full_cull_graph();
        let i = if missing == "clearDepth" { 8 } else { 9 };
        g["nodes"][i]["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        cases.push(g);
    }
    for mut g in cases {
        assert_eq!(
            parse_and_compile_v2(&serde_json::to_vec(&g).unwrap())
                .unwrap_err()
                .code,
            "GRAPH_PARAMETERS_INVALID"
        );
    }
    for (index, field) in [(0, "legacyOptional"), (0, "unknownNodeField")] {
        let mut g = full_cull_graph();
        g["nodes"][index][field] = json!(null);
        assert_eq!(
            parse_and_compile_v2(&serde_json::to_vec(&g).unwrap())
                .unwrap_err()
                .code,
            "GRAPH_JSON_INVALID"
        );
    }
    let mut g = full_cull_graph();
    g["unknownGraphField"] = json!(true);
    assert_eq!(
        parse_and_compile_v2(&serde_json::to_vec(&g).unwrap())
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
                "present",
                json!({}),
                json!({"surface":input("missing","bad")}),
            )
        })
        .collect();
    assert_eq!(
        compile_error(graph(std::mem::take(&mut nodes))).details["path"],
        "nodes"
    );
    let mut nodes = vec![node("surface", "surface_target", json!({}), json!({}))];
    nodes.extend(render_support_nodes());
    let mut color = input("surface", "surface");
    for i in 0..508 {
        let d = format!("d{i}");
        let f = format!("f{i}");
        nodes.push(node(
            &d,
            "texture_spec",
            texture("depth32_float", "transient"),
            json!({}),
        ));
        nodes.push(forward(&f, color, input(&d, "spec")));
        color = input(&f, "color");
    }
    nodes.push(node(
        "present",
        "present",
        json!({}),
        json!({"surface":color}),
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
        let mut nodes = vec![node("surface", "surface_target", json!({}), json!({}))];
        nodes.extend(render_support_nodes());
        let mut color = input("surface", "surface");
        for i in 0..508 {
            let d = format!("d{i}");
            let f = format!("f{i}");
            nodes.push(node(
                &d,
                "texture_spec",
                texture("depth32_float", "transient"),
                json!({}),
            ));
            nodes.push(forward(&f, color, input(&d, "spec")));
            color = input(&f, "color");
        }
        nodes.push(node(
            "present",
            "present",
            json!({}),
            json!({"surface":color}),
        ));
        if old_present {
            nodes.push(node(
                "old_present",
                "present",
                json!({}),
                json!({"surface":input("f0","color")}),
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
    assert_eq!(polluted.code, "GRAPH_RESOURCE_VERSION_INVALID");
    assert_eq!(polluted.details["path"], "nodes[1022].inputs.surface");
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
            node("surface", "surface_target", json!({}), json!({})),
            node(
                "source_target",
                "texture_spec",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            node("bloom_target", "texture_spec", half, json!({})),
            node(
                "output",
                "texture_spec",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            depth_spec("source_depth_0", "transient"),
            depth_spec("source_depth_1", "transient"),
            node(
                "bloom_depth_0",
                "texture_spec",
                half_depth.clone(),
                json!({}),
            ),
            node("bloom_depth_1", "texture_spec", half_depth, json!({})),
        ];
        nodes.extend(render_support_nodes());
        nodes.extend([
            forward("source_f0", input("source_target","spec"), input("source_depth_0","spec")),
            forward("source_f1", input("source_f0","color"), input("source_depth_1","spec")),
            forward("bloom_f0", input("bloom_target","spec"), input("bloom_depth_0","spec")),
            forward("bloom_f1", input("bloom_f0","color"), input("bloom_depth_1","spec")),
            node(
                "composite",
                "bloom_composite",
                json!({"intensity":1.0}),
                json!({
                    "source":input(if stale_socket == "source" { "source_f0" } else { "source_f1" },"color"),
                    "bloom":input(if stale_socket == "bloom" { "source_f0" } else { "source_f1" },"color"),
                    "colorTarget":input("output","spec")
                }),
            ),
            node(
                "to_surface",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("composite","color"),"colorTarget":input("surface","surface")}),
            ),
            node(
                "present",
                "present",
                json!({}),
                json!({"surface":input("to_surface","color")}),
            ),
        ]);
        let error = compile_error(graph(nodes));
        assert_eq!(error.code, "GRAPH_RESOURCE_VERSION_INVALID");
        assert_eq!(
            error.details["path"],
            format!("nodes[16].inputs.{stale_socket}")
        );
    }
}

#[test]
fn bloom_composite_requires_a_single_view_rgba16_half_resolution_bloom() {
    let mut half = texture("rgba16_float", "transient");
    half["texture"]["extent"]["width"] = json!({"numerator":1,"denominator":2});
    half["texture"]["extent"]["height"] = json!({"numerator":1,"denominator":2});
    let make_graph = || {
        let mut bloom_depth = texture("depth32_float", "transient");
        bloom_depth["texture"]["extent"] = half["texture"]["extent"].clone();
        let mut nodes = vec![
            node("surface", "surface_target", json!({}), json!({})),
            node(
                "source",
                "texture_spec",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            node("bloom", "texture_spec", half.clone(), json!({})),
            node(
                "target",
                "texture_spec",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            depth_spec("source_depth", "transient"),
            node("bloom_depth", "texture_spec", bloom_depth, json!({})),
        ];
        nodes.extend(render_support_nodes());
        nodes.extend([
            forward(
                "source_writer",
                input("source", "spec"),
                input("source_depth", "spec"),
            ),
            forward(
                "bloom_writer",
                input("bloom", "spec"),
                input("bloom_depth", "spec"),
            ),
            node(
                "composite",
                "bloom_composite",
                json!({"intensity":1.0}),
                json!({"source":input("source_writer","color"),"bloom":input("bloom_writer","color"),"colorTarget":input("target","spec")}),
            ),
            node(
                "to_surface",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("composite","color"),"colorTarget":input("surface","surface")}),
            ),
            node(
                "present",
                "present",
                json!({}),
                json!({"surface":input("to_surface","color")}),
            ),
        ]);
        graph(nodes)
    };
    compile(make_graph());

    let mut invalid = make_graph();
    invalid["nodes"][2]["parameters"]["texture"]["format"] = json!("rgba8_unorm");
    invalid["nodes"][2]["parameters"]["texture"]["mipLevelCount"] = json!(2);
    let error = compile_error(invalid);
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[12].inputs");
}

#[test]
fn fullscreen_copy_rejects_incompatible_authored_targets_at_copy_inputs() {
    for (field, value, expected_code, expected_path) in [
        (
            "format",
            json!("depth32_float"),
            "GRAPH_ILLEGAL_ACCESS",
            "nodes[9].inputs",
        ),
        (
            "dimension",
            json!("d3"),
            "GRAPH_PARAMETERS_INVALID",
            "nodes[2].parameters.texture.extent",
        ),
        (
            "sampleCount",
            json!(4),
            "GRAPH_ILLEGAL_ACCESS",
            "nodes[9].inputs",
        ),
        (
            "mipLevelCount",
            json!(2),
            "GRAPH_ILLEGAL_ACCESS",
            "nodes[9].inputs",
        ),
    ] {
        let mut target = texture("rgba16_float", "transient");
        target["texture"][field] = value;
        let mut nodes = vec![
            node("surface", "surface_target", json!({}), json!({})),
            node(
                "source",
                "texture_spec",
                texture("rgba16_float", "transient"),
                json!({}),
            ),
            node("target", "texture_spec", target, json!({})),
            depth_spec("depth", "transient"),
        ];
        nodes.extend(render_support_nodes());
        nodes.extend([
            forward("source_writer", input("source","spec"), input("depth","spec")),
            node(
                "copy",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("source_writer","color"),"colorTarget":input("target","spec")}),
            ),
            node(
                "to_surface",
                "fullscreen_copy",
                json!({}),
                json!({"source":input("copy","color"),"colorTarget":input("surface","surface")}),
            ),
            node(
                "present",
                "present",
                json!({}),
                json!({"surface":input("to_surface","color")}),
            ),
        ]);
        let error = compile_error(graph(nodes));
        assert_eq!(error.code, expected_code, "field {field}");
        assert_eq!(error.details["path"], expected_path, "field {field}");
    }
}
