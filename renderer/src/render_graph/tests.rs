use serde_json::{json, Value};

use super::*;

fn compile_value(value: Value) -> Result<CompiledGraph, GraphError> {
    compile(serde_json::from_value(value).unwrap())
}

fn input(node: &str, socket: &str) -> Value {
    json!({ "node": node, "socket": socket })
}

fn texture(id: &str, format: &str) -> Value {
    json!({
        "id": id, "state": "enabled", "executor": { "key": "texture", "version": 1 },
        "parameters": { "residency": "transient", "texture": {
            "dimension": "d2", "format": format,
            "extent": { "kind": "surface_relative", "width": { "numerator": 1, "denominator": 1 },
                "height": { "numerator": 1, "denominator": 1 }, "depthOrArrayLayers": 1 },
            "mipLevelCount": 1, "sampleCount": 1, "viewFormats": [] } }, "inputs": {}
    })
}

fn node(id: &str, key: &str, version: u32, parameters: Value, inputs: Value) -> Value {
    json!({ "id": id, "state": "enabled", "executor": { "key": key, "version": version },
        "parameters": parameters, "inputs": inputs })
}

pub(crate) fn full_cull_graph() -> Value {
    json!({ "schemaVersion": 2, "graphId": "typed", "revision": 1, "nodes": [
        texture("color", "rgba16_float"), texture("depth", "depth32_float"),
        node("mesh", "mesh", 2, json!({}), json!({})),
        node("words", "separate_u32x16", 1, json!({"valueDefault":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}),
            json!({"value":input("mesh","type")})),
        node("bits", "separate_u32_bits", 1, json!({"valueDefault":0}), json!({"value":input("words","word0")})),
        node("cull", "frustum_cull", 2, json!({"camera":"active"}),
            json!({"mesh":input("mesh","mesh"),"localAabb":input("mesh","localAabb")})),
        node("visible", "not", 1, json!({"operandDefault":false}), json!({"operand":input("cull","isFrustumCulled")})),
        node("class", "and", 1, json!({"leftDefault":true,"rightDefault":true}),
            json!({"left":input("bits","bit0"),"right":input("visible","value")})),
        node("pipeline", "pipeline", 2,
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
    assert_eq!(contract("pipeline").unwrap().version, 2);
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
    assert_eq!(predicate.cardinality, InputCardinality::OptionalOne);
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
