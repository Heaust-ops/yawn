use serde_json::{json, Value};

use super::*;

fn compile_value(value: Value) -> Result<CompiledGraph, GraphError> {
    compile(serde_json::from_value(value).unwrap())
}

fn input(node: &str, socket: &str) -> Value {
    json!([{ "node": node, "socket": socket }])
}

fn texture(id: &str, format: &str) -> Value {
    json!({
        "id": id, "state": "enabled", "executor": { "key": "texture", "version": 2 },
        "parameters": { "residency": "transient", "texture": {
            "dimension": "d2", "format": format,
            "extent": { "kind": "surface_relative", "width": { "numerator": 1, "denominator": 1 },
                "height": { "numerator": 1, "denominator": 1 }, "depthOrArrayLayers": 1 },
            "mipLevelCount": 1, "sampleCount": 1, "viewFormats": [] } }, "inputs": {}
    })
}

fn set_extent_ratio(texture: &mut Value, numerator: u32, denominator: u32) {
    texture["parameters"]["texture"]["extent"]["width"] =
        json!({"numerator":numerator,"denominator":denominator});
    texture["parameters"]["texture"]["extent"]["height"] =
        json!({"numerator":numerator,"denominator":denominator});
}

fn set_sample_count(texture: &mut Value, sample_count: u32) {
    texture["parameters"]["texture"]["sampleCount"] = json!(sample_count);
}

fn node(id: &str, key: &str, version: u32, parameters: Value, inputs: Value) -> Value {
    json!({ "id": id, "state": "enabled", "executor": { "key": key, "version": version },
        "parameters": parameters, "inputs": inputs })
}

pub(crate) fn full_cull_graph() -> Value {
    json!({ "schemaVersion": 3, "graphId": "typed", "revision": 1, "nodes": [
        texture("color", "rgba16_float"), texture("depth", "depth32_float"),
        node("mesh", "mesh", 2, json!({}), json!({})),
        node("words", "separate_u32x16", 1, json!({"valueDefault":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}),
            json!({"value":input("mesh","type")})),
        node("bits", "separate_u32_bits", 1, json!({"valueDefault":0}), json!({"value":input("words","word0")})),
        node("cull", "frustum_cull", 2, json!({"camera":"active"}),
            json!({"mesh":input("mesh","mesh"),"localAabb":input("mesh","localAabb")})),
        node("visible", "not", 1, json!({"operandDefault":false}), json!({"operand":input("cull","isFrustumCulled")})),
        node("class", "and", 2, json!({}),
            json!({"inputs":[input("bits","bit0")[0].clone(),input("visible","value")[0].clone()]})),
        node("pipeline", "pipeline", 4,
            json!({"pipeline":"gltf_standard","depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0,"clearColor":[0,0,0,1],"predicateDefault":true}),
            json!({"mesh":input("mesh","mesh"),"predicate":input("class","value"),"colorTarget":input("color","texture"),"depthTarget":input("depth","texture")})),
        node("frame", "frame_out", 3,
            json!({"surfaceFormat":"preferred","hdrEnabled":true,"toneMapper":"aces","exposureStops":0,
                "outputTransfer":"srgb","scaleMode":"stretch","filter":"linear","backgroundColor":[0,0,0,1]}),
            json!({"color":input("pipeline","color")}))
    ]})
}

#[test]
fn catalog_exposes_final_mesh_pipeline_and_generic_expression_contracts() {
    assert_eq!(contract("mesh").unwrap().version, 2);
    assert_eq!(contract("pipeline").unwrap().version, 4);
    assert_eq!(
        contract("mesh")
            .unwrap()
            .outputs
            .iter()
            .map(|o| (o.name, o.semantic_type))
            .collect::<Vec<_>>(),
        [
            ("mesh", SemanticType::MeshData),
            ("type", SemanticType::U32x16),
            ("localAabb", SemanticType::LocalAabb)
        ]
    );
    let predicate = contract("pipeline")
        .unwrap()
        .inputs
        .iter()
        .find(|i| i.name == "predicate")
        .unwrap();
    assert_eq!(predicate.cardinality, InputCardinality { min: 0, max: 1 });
    for key in [
        "and",
        "xnor",
        "greater_than_f32",
        "equals_u32",
        "combine_vec4",
        "separate_mat4",
        "combine_u32_bits",
        "separate_u32x16",
        "separate_local_aabb",
    ] {
        assert!(contract(key).is_some(), "missing {key}");
    }
}

#[test]
fn variadic_boolean_inputs_reject_empty_and_over_capacity_bindings() {
    let mut empty = full_cull_graph();
    empty["nodes"][7]["inputs"]["inputs"] = json!([]);
    let error = compile_value(empty).unwrap_err();
    assert_eq!(error.code, "GRAPH_SOCKET_CARDINALITY");
    assert_eq!(error.details["path"], "nodes[7].inputs.inputs");

    let mut over_capacity = full_cull_graph();
    over_capacity["nodes"][7]["inputs"]["inputs"] = json!((0..9)
        .map(|_| json!({ "node": "bits", "socket": "bit0" }))
        .collect::<Vec<_>>());
    let error = compile_value(over_capacity).unwrap_err();
    assert_eq!(error.code, "GRAPH_SOCKET_CARDINALITY");
    assert_eq!(error.details["path"], "nodes[7].inputs.inputs");
}

#[test]
fn typed_graph_builds_one_dense_deterministic_traversal() {
    let first = compile_value(full_cull_graph()).unwrap();
    let second = compile_value(full_cull_graph()).unwrap();
    let traversal = first.instance_traversal.as_ref().unwrap();
    assert!(traversal.requires_camera);
    assert_eq!(traversal.pipelines.len(), 1);
    assert_eq!(traversal.pipelines[0].ordinal, 0);
    assert!(traversal
        .expressions
        .expressions
        .iter()
        .enumerate()
        .all(|(id, expression)| { expression.origin.node.len() > 0 && id < MAX_EXPRESSIONS }));
    assert_eq!(
        serde_json::to_value(traversal).unwrap(),
        serde_json::to_value(second.instance_traversal.unwrap()).unwrap()
    );
}

#[test]
fn msaa_pipeline_to_frame_out_materializes_distinct_canonical_resolve() {
    let mut value = full_cull_graph();
    set_sample_count(&mut value["nodes"][0], 4);
    set_sample_count(&mut value["nodes"][1], 4);
    let graph = compile_value(value).unwrap();
    let family = graph
        .texture_families
        .iter()
        .find(|family| {
            matches!(
                family.source,
                TextureFamilySource::CompilerColorResolve { .. }
            )
        })
        .unwrap();
    let TextureFamilySource::CompilerColorResolve {
        resource: root,
        source_resource,
        ..
    } = family.source
    else {
        unreachable!()
    };
    assert_ne!(root, family.versions[0].resource);
    assert_ne!(source_resource, family.versions[0].resource);
    assert_eq!(family.versions[0].target, root);
    assert!(matches!(
        graph.resources[root as usize].plan,
        ResourcePlan::TextureSource { .. }
    ));
    assert!(
        validate_activatable(&graph).is_ok(),
        "{:?}",
        validate_activatable(&graph)
    );
}

#[test]
fn msaa_resolve_can_feed_fullscreen_and_default_depth_inherits_four_samples() {
    let mut value = full_cull_graph();
    set_sample_count(&mut value["nodes"][0], 4);
    value["nodes"][8]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("depthTarget");
    value["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(9, texture("post_target", "rgba16_float"));
    value["nodes"].as_array_mut().unwrap().insert(
        10,
        node(
            "post",
            "saturation",
            1,
            json!({"saturation":1,"factor":1}),
            json!({
                "source":input("pipeline","color"),
                "colorTarget":input("post_target","texture")
            }),
        ),
    );
    value["nodes"][11]["inputs"]["color"] = input("post", "color");

    let graph = compile_value(value).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let default_depth = graph
        .texture_families
        .iter()
        .find(|family| {
            matches!(
                family.source,
                TextureFamilySource::CompilerDefaultInput {
                    role: CompilerTextureRole::DepthTarget,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(
        super::compiler::family_descriptor(default_depth).sample_count,
        4
    );
    let resolve_output = graph
        .texture_families
        .iter()
        .find_map(|family| match family.source {
            TextureFamilySource::CompilerColorResolve { .. } => Some(family.versions[0].resource),
            _ => None,
        })
        .unwrap();
    let post = graph
        .executions
        .iter()
        .find(|execution| execution.id == "post")
        .unwrap();
    assert_eq!(post.inputs[0].resource, resolve_output);
}

#[test]
fn msaa_resolve_accepts_default_color_inferred_from_authored_depth() {
    let mut value = full_cull_graph();
    set_sample_count(&mut value["nodes"][1], 4);
    value["nodes"][8]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("colorTarget");

    let graph = compile_value(value).unwrap();
    let (resolve_descriptor, source_resource) = graph
        .texture_families
        .iter()
        .find_map(|family| match &family.source {
            TextureFamilySource::CompilerColorResolve {
                descriptor,
                source_resource,
                ..
            } => Some((descriptor, *source_resource)),
            _ => None,
        })
        .unwrap();
    assert_eq!(resolve_descriptor.sample_count, 1);
    let source_family = match graph.resources[source_resource as usize].plan {
        ResourcePlan::Texture { family, .. } => family,
        _ => panic!("expected resolve source texture version"),
    };
    assert!(matches!(
        &graph.texture_families[source_family as usize].source,
        TextureFamilySource::CompilerDefaultInput {
            role: CompilerTextureRole::ColorTarget,
            descriptor,
            ..
        } if descriptor.sample_count == 4
    ));
    assert!(
        validate_activatable(&graph).is_ok(),
        "{:?}",
        validate_activatable(&graph)
    );
}

#[test]
fn runtime_rejects_resolve_origin_and_store_tampering() {
    let mut value = full_cull_graph();
    set_sample_count(&mut value["nodes"][0], 4);
    set_sample_count(&mut value["nodes"][1], 4);
    let graph = compile_value(value).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let family = graph
        .texture_families
        .iter()
        .find(|family| {
            matches!(
                family.source,
                TextureFamilySource::CompilerColorResolve { .. }
            )
        })
        .unwrap();
    let root = match family.source {
        TextureFamilySource::CompilerColorResolve { resource, .. } => resource,
        _ => unreachable!(),
    };
    let source = match family.source {
        TextureFamilySource::CompilerColorResolve {
            source_resource, ..
        } => source_resource,
        _ => unreachable!(),
    };

    let mut bad_origin = graph.clone();
    bad_origin.resources[root as usize].origin = ResourceOrigin::CompilerColorResolve {
        producer_node_index: u32::MAX,
        output_ordinal: 0,
        source_resource: source,
    };
    assert_runtime_plan_invalid(&bad_origin);

    let mut bad_store = graph.clone();
    let ResourcePlan::Texture { stored, .. } = &mut bad_store.resources[source as usize].plan
    else {
        panic!("expected source texture version")
    };
    *stored = !*stored;
    assert_runtime_plan_invalid(&bad_store);
}

#[test]
fn multisampled_depth_sample_is_rejected_at_exact_socket() {
    let mut value = full_cull_graph();
    set_sample_count(&mut value["nodes"][0], 4);
    set_sample_count(&mut value["nodes"][1], 4);
    value["nodes"][9]["inputs"]["color"] = input("pipeline", "depth");
    let error = compile_value(value).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[9].inputs.color");
}

#[test]
fn pipeline_predicate_defaults_true_and_expression_edges_are_validated() {
    let mut graph = full_cull_graph();
    graph["nodes"][8]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("predicate");
    let compiled = compile_value(graph).unwrap();
    let pipeline = compiled
        .executions
        .iter()
        .find(|e| e.id == "pipeline")
        .unwrap();
    assert!(matches!(
        pipeline.parameters,
        NormalizedParameters::Pipeline {
            predicate_default: true,
            ..
        }
    ));

    let mut cycle = full_cull_graph();
    cycle["nodes"][6]["inputs"]["operand"] = input("class", "value");
    assert_eq!(compile_value(cycle).unwrap_err().code, "GRAPH_CYCLE");
}

#[test]
fn expression_provenance_rejects_cross_mesh_values() {
    let mut graph = full_cull_graph();
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(3, node("other", "mesh", 2, json!({}), json!({})));
    // The insertion shifts cull to index 6; pair another mesh with the first mesh's AABB.
    graph["nodes"][6]["inputs"]["mesh"] = input("other", "mesh");
    let error = compile_value(graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_SOCKET_TYPE_MISMATCH");
}

fn implicit_pipeline_graph() -> Value {
    json!({ "schemaVersion": 3, "graphId": "implicit", "revision": 1, "nodes": [
        node("mesh", "mesh", 2, json!({}), json!({})),
        node("first", "pipeline", 4,
            json!({"pipeline":"ground_plane","depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0,"clearColor":[0,0,0,1],"predicateDefault":true}),
            json!({"mesh":input("mesh","mesh")})),
        node("second", "pipeline", 4,
            json!({"pipeline":"gltf_standard","depthCompare":"less_equal","depthWriteEnabled":true,"clearDepth":1.0,"clearColor":[0,0,0,1],"predicateDefault":true}),
            json!({"mesh":input("mesh","mesh"),"colorTarget":input("first","color"),"depthTarget":input("first","depth")})),
        node("frame", "frame_out", 3,
            json!({"surfaceFormat":"preferred","hdrEnabled":true,"toneMapper":"aces","exposureStops":0,"outputTransfer":"srgb","scaleMode":"stretch","filter":"linear","backgroundColor":[0,0,0,1]}),
            json!({"color":input("second","color")}))
    ]})
}

#[test]
fn contract_v4_declares_strict_default_policies() {
    let pipeline = contract("pipeline").unwrap();
    assert_eq!(pipeline.version, 4);
    assert_eq!(pipeline.inputs[0].default_policy, InputDefaultPolicy::None);
    assert_eq!(
        pipeline.inputs[1].default_policy,
        InputDefaultPolicy::ParameterLiteral
    );
    assert_eq!(
        pipeline.inputs[2].default_policy,
        InputDefaultPolicy::CompilerTexture
    );
    assert_eq!(
        pipeline.inputs[3].default_policy,
        InputDefaultPolicy::CompilerTexture
    );
    assert!(CONTRACTS
        .iter()
        .flat_map(|c| c.inputs)
        .all(|input| matches!(
            (input.cardinality, input.default_policy),
            (
                InputCardinality { min: 1, max: 1 },
                InputDefaultPolicy::None
            ) | (
                InputCardinality { min: 0, max: 1 },
                InputDefaultPolicy::ParameterLiteral
            ) | (
                InputCardinality { min: 0, max: 1 },
                InputDefaultPolicy::CompilerTexture
            ) | (
                InputCardinality { min: 0, max: 8 },
                InputDefaultPolicy::None
            )
        )));
}

#[test]
fn disconnected_targets_have_tagged_roots_and_clear_version_zero() {
    let compiled = compile_value(implicit_pipeline_graph()).unwrap();
    let roots: Vec<_> = compiled
        .resources
        .iter()
        .filter(|r| matches!(r.origin, ResourceOrigin::CompilerDefaultInput { .. }))
        .collect();
    assert_eq!(roots.len(), 2);
    assert_eq!(compiled.culled_resource_count, 0);
    assert!(compiled
        .texture_families
        .iter()
        .take(2)
        .all(|f| f.aliasable));
    let first = compiled
        .executions
        .iter()
        .find(|e| e.id == "first")
        .unwrap();
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &first.kind
    else {
        panic!()
    };
    assert!(matches!(
        color_attachments[0].load,
        NormalizedColorLoad::Clear { .. }
    ));
    assert!(matches!(depth.load, NormalizedDepthLoad::Clear { .. }));
}

#[test]
fn implicit_chain_is_deterministic_and_loads_successors() {
    let a = compile_value(implicit_pipeline_graph()).unwrap();
    let b = compile_value(implicit_pipeline_graph()).unwrap();
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    let second = a.executions.iter().find(|e| e.id == "second").unwrap();
    let ExecutionKind::Render {
        color_attachments,
        depth_stencil: Some(depth),
    } = &second.kind
    else {
        panic!()
    };
    assert_eq!(color_attachments[0].load, NormalizedColorLoad::Load);
    assert_eq!(depth.load, NormalizedDepthLoad::Load);
}

#[test]
fn one_missing_attachment_copies_authored_opposite_extent() {
    for missing in ["colorTarget", "depthTarget"] {
        let mut graph = full_cull_graph();
        graph["nodes"][8]["inputs"]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        let compiled = compile_value(graph).unwrap();
        let default = compiled
            .texture_families
            .iter()
            .find(|f| matches!(f.source, TextureFamilySource::CompilerDefaultInput { .. }))
            .unwrap();
        let explicit = compiled
            .texture_families
            .iter()
            .find(|f| {
                matches!(f.source, TextureFamilySource::AuthoredTexture { .. })
                    && f.id != default.id
            })
            .unwrap();
        assert_eq!(
            super::compiler::family_descriptor(default).extent,
            super::compiler::family_descriptor(explicit).extent
        );
    }
}

#[test]
fn default_extent_follows_half_surface_and_prior_default_families() {
    let mut half = full_cull_graph();
    set_extent_ratio(&mut half["nodes"][0], 1, 2);
    half["nodes"][8]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("depthTarget");
    let compiled = compile_value(half).unwrap();
    let default = compiled
        .texture_families
        .iter()
        .find(|family| {
            matches!(
                family.source,
                TextureFamilySource::CompilerDefaultInput { .. }
            )
        })
        .unwrap();
    assert_eq!(
        super::compiler::family_descriptor(default).extent,
        NormalizedTextureExtent::SurfaceRelative {
            width: Ratio {
                numerator: 1,
                denominator: 2
            },
            height: Ratio {
                numerator: 1,
                denominator: 2
            },
            depth_or_array_layers: 1
        }
    );

    let compiled = compile_value(implicit_pipeline_graph()).unwrap();
    let defaults: Vec<_> = compiled
        .texture_families
        .iter()
        .filter(|family| {
            matches!(
                family.source,
                TextureFamilySource::CompilerDefaultInput { .. }
            )
        })
        .collect();
    assert_eq!(defaults.len(), 2);
    assert_eq!(
        super::compiler::family_descriptor(defaults[0]).extent,
        super::compiler::family_descriptor(defaults[1]).extent
    );
}

#[test]
fn explicit_attachment_diagnostics_are_socket_specific_then_mutual() {
    let mut color = full_cull_graph();
    color["nodes"][0]["parameters"]["texture"]["format"] = json!("depth32_float");
    let error = compile_value(color).unwrap_err();
    assert_eq!(error.details["path"], "nodes[8].inputs.colorTarget");

    let mut depth = full_cull_graph();
    depth["nodes"][1]["parameters"]["texture"]["format"] = json!("rgba16_float");
    let error = compile_value(depth).unwrap_err();
    assert_eq!(error.details["path"], "nodes[8].inputs.depthTarget");

    let mut mismatch = full_cull_graph();
    set_extent_ratio(&mut mismatch["nodes"][1], 1, 2);
    let error = compile_value(mismatch).unwrap_err();
    assert_eq!(error.details["path"], "nodes[8].inputs");
}

#[test]
fn descriptor_dependency_cycle_keeps_graph_cycle_priority() {
    let mut graph = implicit_pipeline_graph();
    graph["nodes"][1]["inputs"]["depthTarget"] = input("second", "depth");
    graph["nodes"][2]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("depthTarget");
    assert_eq!(compile_value(graph).unwrap_err().code, "GRAPH_CYCLE");
}

#[test]
fn known_attachment_error_precedes_cycle() {
    let mut graph = full_cull_graph();
    graph["nodes"][0]["parameters"]["texture"]["format"] = json!("depth32_float");
    graph["nodes"][8]["inputs"]["depthTarget"] = input("pipeline", "depth");
    let error = compile_value(graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[8].inputs.colorTarget");
}

fn self_targeting_copy(source_format: &str) -> Value {
    let mut graph = full_cull_graph();
    graph["nodes"][0]["parameters"]["texture"]["format"] = json!(source_format);
    graph["nodes"].as_array_mut().unwrap().insert(
        9,
        node(
            "copy",
            "fullscreen_copy",
            1,
            json!({}),
            json!({
                "source": input("pipeline", "color"),
                "colorTarget": input("copy", "color")
            }),
        ),
    );
    graph["nodes"][10]["inputs"]["color"] = input("copy", "color");
    graph
}

#[test]
fn known_invalid_fullscreen_source_precedes_target_cycle() {
    let error = compile_value(self_targeting_copy("rgba8_unorm")).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[9].inputs");

    assert_eq!(
        compile_value(self_targeting_copy("rgba16_float"))
            .unwrap_err()
            .code,
        "GRAPH_CYCLE"
    );
}

#[test]
fn known_invalid_fullscreen_target_precedes_source_cycle() {
    let mut graph = implicit_pipeline_graph();
    graph["nodes"][1]["inputs"]["depthTarget"] = input("second", "depth");
    graph["nodes"][2]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("depthTarget");
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(3, texture("saturation_target", "rgba8_unorm"));
    graph["nodes"].as_array_mut().unwrap().insert(
        4,
        node(
            "saturation",
            "saturation",
            1,
            json!({"saturation":1,"factor":1}),
            json!({
                "source": input("second", "color"),
                "colorTarget": input("saturation_target", "texture")
            }),
        ),
    );
    graph["nodes"][5]["parameters"]["hdrEnabled"] = json!(false);
    graph["nodes"][5]["inputs"]["color"] = input("saturation", "color");
    let error = compile_value(graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[4].inputs");
}

fn self_targeting_composite(bloom_format: &str) -> Value {
    let mut graph = full_cull_graph();
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(9, texture("bloom_target", bloom_format));
    graph["nodes"].as_array_mut().unwrap().insert(
        10,
        node(
            "bloom_copy",
            "fullscreen_copy",
            1,
            json!({}),
            json!({
                "source": input("pipeline", "color"),
                "colorTarget": input("bloom_target", "texture")
            }),
        ),
    );
    graph["nodes"].as_array_mut().unwrap().insert(
        11,
        node(
            "composite",
            "bloom_composite",
            1,
            json!({"intensity":1}),
            json!({
                "source": input("pipeline", "color"),
                "bloom": input("bloom_copy", "color"),
                "colorTarget": input("composite", "color")
            }),
        ),
    );
    graph["nodes"][12]["inputs"]["color"] = input("composite", "color");
    graph
}

#[test]
fn known_invalid_bloom_input_precedes_target_cycle() {
    let error = compile_value(self_targeting_composite("rgba8_unorm")).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[11].inputs");
    assert_eq!(
        compile_value(self_targeting_composite("rgba16_float"))
            .unwrap_err()
            .code,
        "GRAPH_CYCLE"
    );
}

fn cyclic_fullscreen_source(target_format: &str) -> Value {
    let mut graph = full_cull_graph();
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(9, texture("copy_target", target_format));
    graph["nodes"].as_array_mut().unwrap().insert(
        10,
        node(
            "source_cycle",
            "fullscreen_copy",
            1,
            json!({}),
            json!({
                "source": input("pipeline", "color"),
                "colorTarget": input("source_cycle", "color")
            }),
        ),
    );
    graph["nodes"].as_array_mut().unwrap().insert(
        11,
        node(
            "copy",
            "fullscreen_copy",
            1,
            json!({}),
            json!({
                "source": input("source_cycle", "color"),
                "colorTarget": input("copy_target", "texture")
            }),
        ),
    );
    graph["nodes"][12]["inputs"]["color"] = input("copy", "color");
    graph
}

#[test]
fn cyclic_fullscreen_source_defers_uninitialized_error() {
    let error = compile_value(cyclic_fullscreen_source("depth32_float")).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[11].inputs");
    assert_eq!(
        compile_value(cyclic_fullscreen_source("rgba16_float"))
            .unwrap_err()
            .code,
        "GRAPH_CYCLE"
    );
}

fn cyclic_bloom_input(target_format: &str) -> Value {
    let mut graph = full_cull_graph();
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(9, texture("composite_target", target_format));
    graph["nodes"].as_array_mut().unwrap().insert(
        10,
        node(
            "bloom_cycle",
            "fullscreen_copy",
            1,
            json!({}),
            json!({
                "source": input("pipeline", "color"),
                "colorTarget": input("bloom_cycle", "color")
            }),
        ),
    );
    graph["nodes"].as_array_mut().unwrap().insert(
        11,
        node(
            "composite",
            "bloom_composite",
            1,
            json!({"intensity":1}),
            json!({
                "source": input("pipeline", "color"),
                "bloom": input("bloom_cycle", "color"),
                "colorTarget": input("composite_target", "texture")
            }),
        ),
    );
    graph["nodes"][12]["inputs"]["color"] = input("composite", "color");
    graph
}

#[test]
fn cyclic_bloom_input_defers_uninitialized_error() {
    let error = compile_value(cyclic_bloom_input("depth32_float")).unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    assert_eq!(error.details["path"], "nodes[11].inputs");
    assert_eq!(
        compile_value(cyclic_bloom_input("rgba16_float"))
            .unwrap_err()
            .code,
        "GRAPH_CYCLE"
    );
}

fn compiler_default_source(graph: &CompiledGraph) -> (usize, u32, u32) {
    graph
        .texture_families
        .iter()
        .enumerate()
        .find_map(|(family_index, family)| match family.source {
            TextureFamilySource::CompilerDefaultInput {
                resource,
                owner_node_index,
                ..
            } => Some((family_index, resource, owner_node_index)),
            _ => None,
        })
        .unwrap()
}

fn assert_runtime_plan_invalid(graph: &CompiledGraph) {
    let error = validate_activatable(graph).unwrap_err();
    assert_eq!(error.code, "GRAPH_RUNTIME_PLAN_INVALID");
}

#[test]
fn runtime_rejects_duplicate_compiler_default_owner_identity() {
    let mut graph = compile_value(implicit_pipeline_graph()).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let (_, _, owner) = compiler_default_source(&graph);
    graph
        .executions
        .iter_mut()
        .find(|execution| execution.executor.key == "frame_out")
        .unwrap()
        .original_node_index = owner;
    assert_runtime_plan_invalid(&graph);
}

#[test]
fn runtime_rejects_coherently_retagged_authored_texture_origin() {
    let mut graph = compile_value(full_cull_graph()).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let family = graph
        .texture_families
        .iter_mut()
        .find(|family| matches!(family.source, TextureFamilySource::AuthoredTexture { .. }))
        .unwrap();
    let TextureFamilySource::AuthoredTexture { resource, .. } = family.source else {
        unreachable!()
    };
    family.key.source_node = 2;
    family.key.source_socket = 1;
    let source = &mut graph.resources[resource as usize];
    source.original_node_index = 2;
    source.origin = ResourceOrigin::AuthoredOutput {
        node: "mesh".into(),
        socket: "type".into(),
        output_ordinal: 1,
    };
    assert_runtime_plan_invalid(&graph);
}

#[test]
fn runtime_rejects_duplicate_compiler_default_input_occurrence() {
    let mut graph = compile_value(implicit_pipeline_graph()).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let (_, resource, owner) = compiler_default_source(&graph);
    graph
        .executions
        .iter_mut()
        .find(|execution| execution.original_node_index == owner)
        .unwrap()
        .inputs
        .push(CompiledSocketInput {
            socket: "duplicate".into(),
            resource,
        });
    assert_runtime_plan_invalid(&graph);
}

#[test]
fn runtime_rejects_out_of_range_opposite_default_family_without_panicking() {
    let mut graph = compile_value(implicit_pipeline_graph()).unwrap();
    assert!(validate_activatable(&graph).is_ok());
    let (_, _, owner) = compiler_default_source(&graph);
    let opposite_resource = graph
        .executions
        .iter()
        .find(|execution| execution.original_node_index == owner)
        .unwrap()
        .inputs
        .iter()
        .find(|input| input.socket == "depthTarget")
        .unwrap()
        .resource;
    let out_of_range = graph.texture_families.len() as u32;
    let ResourcePlan::TextureSource { family, .. } =
        &mut graph.resources[opposite_resource as usize].plan
    else {
        panic!("expected opposite texture source")
    };
    *family = out_of_range;
    assert_runtime_plan_invalid(&graph);
}
