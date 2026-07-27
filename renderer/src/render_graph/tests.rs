use super::*;

struct TestExecutors;
struct TestExecutor {
    observable: bool,
}

static TEST_EXECUTOR: TestExecutor = TestExecutor { observable: false };
static OBSERVABLE_EXECUTOR: TestExecutor = TestExecutor { observable: true };

impl ExecutorRegistry for TestExecutors {
    fn resolve(&self, executor: &ExecutorRef) -> ExecutorResolution<'_> {
        if executor.version != 1 {
            return ExecutorResolution::UnsupportedVersion;
        }
        match executor.key.as_str() {
            "test" => ExecutorResolution::Found(&TEST_EXECUTOR),
            "observable" => ExecutorResolution::Found(&OBSERVABLE_EXECUTOR),
            _ => ExecutorResolution::UnknownKey,
        }
    }
}

impl ExecutorContract for TestExecutor {
    fn inherently_observable(&self) -> bool {
        self.observable
    }

    fn normalize_parameters(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<NormalizedParameters, String> {
        if parameters == &serde_json::json!({}) {
            Ok(NormalizedParameters::SceneForward)
        } else {
            Err("test parameters must be empty".into())
        }
    }

    fn validate_bindings(
        &self,
        _pass: &Pass,
        _resources: &std::collections::HashMap<ResourceRef, &Resource>,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn compile_json(value: serde_json::Value) -> Result<CompiledGraph, GraphError> {
    compile_with(&serde_json::to_vec(&value).unwrap(), &TestExecutors)
}

fn texture(format: &str) -> serde_json::Value {
    serde_json::json!({
        "dimension": "d2",
        "format": format,
        "extent": {"kind":"absolute", "width":16, "height":16, "depthOrArrayLayers":1},
        "mipLevelCount": 1,
        "sampleCount": 1
    })
}

fn transient(id: &str, format: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "version": 0,
        "residency": {"kind":"transient"},
        "texture": texture(format)
    })
}

fn pass(
    id: &str,
    executor: &str,
    reads: serde_json::Value,
    writes: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "state": "enabled",
        "executor": {"key":executor, "version":1},
        "parameters": {},
        "reads": reads,
        "writes": writes
    })
}

fn resource_ref(id: &str) -> serde_json::Value {
    serde_json::json!({"id":id, "version":0})
}

fn sampled(binding: &str, id: &str) -> serde_json::Value {
    serde_json::json!({"binding":binding, "resource":resource_ref(id), "access":"sampled"})
}

fn copy_write(binding: &str, id: &str) -> serde_json::Value {
    serde_json::json!({"binding":binding, "resource":resource_ref(id), "access":{"kind":"copy_dst"}})
}

fn color_write(binding: &str, id: &str, location: u32) -> serde_json::Value {
    serde_json::json!({
        "binding":binding,
        "resource":resource_ref(id),
        "access":{
            "kind":"color_attachment",
            "location":location,
            "load":{"op":"clear", "value":[0.0, 0.0, 0.0, 1.0]},
            "store":"store"
        }
    })
}

fn color_load(binding: &str, id: &str, location: u32) -> serde_json::Value {
    serde_json::json!({
        "binding":binding,
        "resource":resource_ref(id),
        "access":{
            "kind":"color_attachment",
            "location":location,
            "load":{"op":"load"},
            "store":"store"
        }
    })
}

fn graph(
    resources: serde_json::Value,
    passes: serde_json::Value,
    outputs: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion":1,
        "graphId":"test_graph",
        "revision":1,
        "resources":resources,
        "passes":passes,
        "outputs":outputs
    })
}

fn empty(id: &str, revision: u32) -> Vec<u8> {
    format!(r#"{{"schemaVersion":1,"graphId":"{id}","revision":{revision},"resources":[],"passes":[],"outputs":[]}}"#).into_bytes()
}
fn error(bytes: &[u8]) -> &'static str {
    parse_and_compile(bytes).unwrap_err().code
}

#[test]
fn size_precedes_encoding() {
    assert_eq!(
        error(&vec![0xff; MAX_JSON_BYTES + 1]),
        "GRAPH_PAYLOAD_TOO_LARGE"
    );
}
#[test]
fn encoding_precedes_schema() {
    assert_eq!(error(&[0xff]), "GRAPH_ENCODING_INVALID");
}
#[test]
fn malformed_json() {
    assert_eq!(error(b"{"), "GRAPH_JSON_INVALID");
}
#[test]
fn missing_schema_probe() {
    assert_eq!(error(b"{}"), "GRAPH_SCHEMA_UNSUPPORTED");
}
#[test]
fn unsupported_schema_probe() {
    assert_eq!(error(br#"{"schemaVersion":2}"#), "GRAPH_SCHEMA_UNSUPPORTED");
}
#[test]
fn strict_unknown_field() {
    assert_eq!(error(br#"{"schemaVersion":1,"graphId":"g","revision":1,"resources":[],"passes":[],"outputs":[],"extra":0}"#), "GRAPH_JSON_INVALID");
}
#[test]
fn identifier_first_character() {
    assert_eq!(error(&empty("1bad", 1)), "GRAPH_INVALID_ID");
}
#[test]
fn underscore_identifier() {
    assert_eq!(parse_and_compile(&empty("_ok", 1)).unwrap().graph_id, "_ok");
}
#[test]
fn revision_required() {
    assert_eq!(error(&empty("g", 0)), "GRAPH_INVALID_ID");
}

#[test]
fn resource_version_zero_and_wire_names() {
    let json = br#"{"schemaVersion":1,"graphId":"g","revision":1,"resources":[{"id":"r","version":0,"residency":{"kind":"transient"},"texture":{"dimension":"d2","format":"rgba8_unorm","extent":{"kind":"absolute","width":1,"height":1,"depthOrArrayLayers":1},"mipLevelCount":1,"sampleCount":1}}],"passes":[],"outputs":[]}"#;
    assert_eq!(parse_and_compile(json).unwrap().culled_resource_count, 1);
}
#[test]
fn old_mip_wire_rejected() {
    let mut s = String::from_utf8(empty("g", 1)).unwrap();
    s=s.replace("\"resources\":[]", "\"resources\":[{\"id\":\"r\",\"version\":0,\"residency\":{\"kind\":\"transient\"},\"texture\":{\"dimension\":\"d2\",\"format\":\"rgba8_unorm\",\"extent\":{\"kind\":\"absolute\",\"width\":1,\"height\":1,\"depthOrArrayLayers\":1},\"mipLevels\":1,\"sampleCount\":1}}]");
    assert_eq!(error(s.as_bytes()), "GRAPH_JSON_INVALID");
}

#[test]
fn registry_transaction_on_parse_failure() {
    let mut r = Registry::new(1);
    assert!(r.compile(b"{").is_err());
    assert!(r.compile(&empty("g", 1)).is_ok());
}
#[test]
fn registry_capacity() {
    let mut r = Registry::new(1);
    r.compile(&empty("a", 1)).unwrap();
    assert_eq!(
        r.compile(&empty("b", 1)).unwrap_err().code,
        "GRAPH_REGISTRY_FULL"
    );
}
#[test]
fn registry_revision_replaces_in_place() {
    let mut r = Registry::new(1);
    let (a, _) = r.compile(&empty("g", 1)).unwrap();
    let (b, _) = r.compile(&empty("g", 2)).unwrap();
    assert_eq!(a, b);
    assert_eq!(r.get(a).unwrap().revision, 2);
}
#[test]
fn registry_revision_conflict() {
    let mut r = Registry::new(1);
    r.compile(&empty("g", 2)).unwrap();
    assert_eq!(
        r.compile(&empty("g", 2)).unwrap_err().code,
        "GRAPH_REVISION_CONFLICT"
    );
}
#[test]
fn registry_drop_and_stale() {
    let mut r = Registry::new(1);
    let (id, _) = r.compile(&empty("g", 1)).unwrap();
    r.drop_graph(id).unwrap();
    assert_eq!(r.get(id).unwrap_err().code, "STALE_GRAPH_ID");
    assert_eq!(r.drop_graph(id).unwrap_err().code, "STALE_GRAPH_ID");
}
#[test]
fn registry_reuse_increments_generation() {
    let mut r = Registry::new(1);
    let (a, _) = r.compile(&empty("a", 1)).unwrap();
    r.drop_graph(a).unwrap();
    let (b, _) = r.compile(&empty("b", 1)).unwrap();
    assert_eq!(a.slot, b.slot);
    assert_eq!(a.generation + 1, b.generation);
}
#[test]
fn graph_error_details_always_have_message() {
    let e = parse_and_compile(b"{}").unwrap_err();
    assert!(e.details["message"].is_string());
}

#[test]
fn zero_surface_ratio_is_rejected_without_panicking() {
    for field in ["width", "height"] {
        let mut resource = transient("r", "rgba8_unorm");
        resource["texture"]["extent"] = serde_json::json!({
            "kind":"surface_relative",
            "width":{"numerator":1,"denominator":1},
            "height":{"numerator":1,"denominator":1},
            "depthOrArrayLayers":1
        });
        resource["texture"]["extent"][field] = serde_json::json!({"numerator":0,"denominator":0});
        let error = compile_json(graph(
            serde_json::json!([resource]),
            serde_json::json!([]),
            serde_json::json!([]),
        ))
        .unwrap_err();
        assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
    }
}

#[test]
fn depth_texture_accepts_copy_destination_access() {
    let compiled = compile_json(graph(
        serde_json::json!([transient("depth", "depth32_float")]),
        serde_json::json!([pass(
            "write_depth",
            "observable",
            serde_json::json!([]),
            serde_json::json!([copy_write("destination", "depth")])
        )]),
        serde_json::json!([]),
    ))
    .unwrap();
    assert_eq!(compiled.passes.len(), 1);
}

#[test]
fn unknown_resources_precede_executor_and_parameter_errors() {
    let mut invalid = pass(
        "bad",
        "missing_executor",
        serde_json::json!([sampled("input", "missing_resource")]),
        serde_json::json!([]),
    );
    invalid["parameters"] = serde_json::json!({"also":"invalid"});
    let error = compile_json(graph(
        serde_json::json!([]),
        serde_json::json!([invalid]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_UNKNOWN_RESOURCE");
}

#[test]
fn executor_contract_normalizes_parameters_and_reports_invalid_parameters() {
    let valid = compile_json(graph(
        serde_json::json!([transient("r", "rgba8_unorm")]),
        serde_json::json!([pass(
            "p",
            "observable",
            serde_json::json!([]),
            serde_json::json!([copy_write("out", "r")])
        )]),
        serde_json::json!([]),
    ))
    .unwrap();
    assert_eq!(
        valid.passes[0].parameters,
        NormalizedParameters::SceneForward
    );

    let mut invalid = pass(
        "p",
        "observable",
        serde_json::json!([]),
        serde_json::json!([copy_write("out", "r")]),
    );
    invalid["parameters"] = serde_json::json!({"unexpected":true});
    let error = compile_json(graph(
        serde_json::json!([transient("r", "rgba8_unorm")]),
        serde_json::json!([invalid]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_PARAMETERS_INVALID");
}

#[test]
fn culls_dead_branches_and_orders_live_dependencies_deterministically() {
    let compiled = compile_json(graph(
        serde_json::json!([
            transient("middle", "rgba8_unorm"),
            transient("output", "rgba8_unorm"),
            transient("dead", "rgba8_unorm")
        ]),
        serde_json::json!([
            pass(
                "consumer",
                "test",
                serde_json::json!([sampled("input", "middle")]),
                serde_json::json!([copy_write("out", "output")])
            ),
            pass(
                "producer",
                "test",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "middle")])
            ),
            pass(
                "dead",
                "test",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "dead")])
            )
        ]),
        serde_json::json!([{"name":"present", "resource":resource_ref("output")}]),
    ))
    .unwrap();
    assert_eq!(
        compiled
            .passes
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["producer", "consumer"]
    );
    assert_eq!(compiled.culled_pass_count, 1);
    assert_eq!(compiled.culled_resource_count, 1);
}

#[test]
fn output_extends_inclusive_lifetime_to_graph_boundary() {
    let compiled = compile_json(graph(
        serde_json::json!([transient("out", "rgba8_unorm")]),
        serde_json::json!([pass(
            "write",
            "test",
            serde_json::json!([]),
            serde_json::json!([copy_write("out", "out")])
        )]),
        serde_json::json!([{"name":"present", "resource":resource_ref("out")}]),
    ))
    .unwrap();
    assert_eq!(compiled.resources[0].lifetime.first_use, 0);
    assert_eq!(compiled.resources[0].lifetime.last_use, 1);
}

#[test]
fn transient_slots_reuse_only_for_non_overlapping_compatible_lifetimes() {
    let compiled = compile_json(graph(
        serde_json::json!([
            transient("first", "rgba8_unorm"),
            transient("second", "rgba8_unorm"),
            transient("incompatible", "rgba16_float")
        ]),
        serde_json::json!([
            pass(
                "a",
                "observable",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "first")])
            ),
            pass(
                "b",
                "observable",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "second")])
            ),
            pass(
                "c",
                "observable",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "incompatible")])
            )
        ]),
        serde_json::json!([]),
    ))
    .unwrap();
    let allocations: Vec<_> = compiled
        .resources
        .iter()
        .map(|resource| resource.allocation.unwrap())
        .collect();
    assert_eq!(allocations[0], allocations[1]);
    assert_ne!(allocations[0].class, allocations[2].class);
    assert_eq!(compiled.allocation_classes.len(), 2);
}

#[test]
fn cycle_details_survive_a_non_cycle_dfs_branch() {
    let result = compile_json(graph(
        serde_json::json!([
            transient("ab", "rgba8_unorm"),
            transient("bc", "rgba8_unorm"),
            transient("ca", "rgba8_unorm"),
            transient("branch", "rgba8_unorm")
        ]),
        serde_json::json!([
            pass(
                "a",
                "observable",
                serde_json::json!([sampled("ca", "ca")]),
                serde_json::json!([copy_write("ab", "ab"), copy_write("branch", "branch")])
            ),
            pass(
                "branch",
                "observable",
                serde_json::json!([sampled("input", "branch")]),
                serde_json::json!([])
            ),
            pass(
                "b",
                "observable",
                serde_json::json!([sampled("ab", "ab")]),
                serde_json::json!([copy_write("bc", "bc")])
            ),
            pass(
                "c",
                "observable",
                serde_json::json!([sampled("bc", "bc")]),
                serde_json::json!([copy_write("ca", "ca")])
            )
        ]),
        serde_json::json!([]),
    ));
    let error = result.unwrap_err();
    assert_eq!(error.code, "GRAPH_CYCLE");
    assert_eq!(error.details["kind"], "cycle");
    let edges = error.details["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 3);
    assert!(edges.iter().all(|edge| edge["from"] != "branch"));
}

#[test]
fn duplicate_external_source_is_an_identity_error_before_descriptor_validation() {
    let external = |id: &str, texture: serde_json::Value| {
        serde_json::json!({
            "id":id,
            "version":0,
            "residency":{"kind":"external", "source":"surface_color"},
            "texture":texture
        })
    };
    let surface = serde_json::json!({
        "dimension":"d2",
        "format":"surface",
        "extent":{
            "kind":"surface_relative",
            "width":{"numerator":1,"denominator":1},
            "height":{"numerator":1,"denominator":1},
            "depthOrArrayLayers":1
        },
        "mipLevelCount":1,
        "sampleCount":1
    });
    let error = compile_json(graph(
        serde_json::json!([
            external("first", surface),
            external("second", texture("rgba8_unorm"))
        ]),
        serde_json::json!([]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_DUPLICATE_ID");
}

#[test]
fn duplicate_writer_precedes_illegal_access_on_the_second_writer() {
    let illegal_depth_color = color_write("bad", "depth", 0);
    let error = compile_json(graph(
        serde_json::json!([transient("depth", "depth32_float")]),
        serde_json::json!([
            pass(
                "first",
                "observable",
                serde_json::json!([]),
                serde_json::json!([copy_write("out", "depth")])
            ),
            pass(
                "second",
                "observable",
                serde_json::json!([]),
                serde_json::json!([illegal_depth_color])
            )
        ]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_DUPLICATE_WRITER");
}

#[test]
fn rejects_device_invalid_dimensions_and_mismatched_attachments() {
    let mut d1 = transient("d1", "rgba8_unorm");
    d1["texture"]["dimension"] = serde_json::json!("d1");
    assert_eq!(
        compile_json(graph(
            serde_json::json!([d1]),
            serde_json::json!([]),
            serde_json::json!([])
        ))
        .unwrap_err()
        .code,
        "GRAPH_ILLEGAL_ACCESS"
    );

    let mut depth_d3 = transient("depth", "depth32_float");
    depth_d3["texture"]["dimension"] = serde_json::json!("d3");
    assert_eq!(
        compile_json(graph(
            serde_json::json!([depth_d3]),
            serde_json::json!([]),
            serde_json::json!([])
        ))
        .unwrap_err()
        .code,
        "GRAPH_ILLEGAL_ACCESS"
    );

    let first = transient("first", "rgba8_unorm");
    let mut second = transient("second", "rgba8_unorm");
    second["texture"]["extent"]["width"] = serde_json::json!(32);
    let error = compile_json(graph(
        serde_json::json!([first, second]),
        serde_json::json!([pass(
            "attachments",
            "observable",
            serde_json::json!([]),
            serde_json::json!([
                color_write("first", "first", 0),
                color_write("second", "second", 1)
            ])
        )]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_ILLEGAL_ACCESS");
}

#[test]
fn cycle_tie_breaks_parallel_edges_by_original_resource_index() {
    let error = compile_json(graph(
        serde_json::json!([
            transient("z_declared_first", "rgba8_unorm"),
            transient("a_declared_second", "rgba8_unorm"),
            transient("back", "rgba8_unorm")
        ]),
        serde_json::json!([
            pass(
                "a",
                "observable",
                serde_json::json!([sampled("back", "back")]),
                serde_json::json!([
                    copy_write("first", "z_declared_first"),
                    copy_write("second", "a_declared_second")
                ])
            ),
            pass(
                "b",
                "observable",
                serde_json::json!([
                    sampled("first", "z_declared_first"),
                    sampled("second", "a_declared_second")
                ]),
                serde_json::json!([copy_write("back", "back")])
            )
        ]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_CYCLE");
    assert_eq!(
        error.details["edges"][0]["resource"]["id"],
        "z_declared_first"
    );
}

#[test]
fn identifier_byte_limit_precedes_reference_and_executor_resolution() {
    let overlong = "a".repeat(65);
    let invalid = pass(
        "p",
        "unknown_executor",
        serde_json::json!([sampled(&overlong, &overlong)]),
        serde_json::json!([]),
    );
    let error = compile_json(graph(
        serde_json::json!([]),
        serde_json::json!([invalid]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_LIMIT_EXCEEDED");
}

#[test]
fn uninitialized_resource_precedes_transient_attachment_load_legality() {
    let error = compile_json(graph(
        serde_json::json!([
            transient("loaded", "rgba8_unorm"),
            transient("uninitialized", "rgba8_unorm")
        ]),
        serde_json::json!([pass(
            "conflicting_errors",
            "observable",
            serde_json::json!([sampled("missing_writer", "uninitialized")]),
            serde_json::json!([color_load("loaded", "loaded", 0)])
        )]),
        serde_json::json!([]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_UNINITIALIZED_RESOURCE");
}

#[test]
fn malformed_resource_reference_ids_precede_resolution() {
    for bindings in ["reads", "writes"] {
        let mut invalid = pass(
            "p",
            "unknown_executor",
            serde_json::json!([]),
            serde_json::json!([]),
        );
        invalid[bindings] = if bindings == "reads" {
            serde_json::json!([sampled("input", "1bad")])
        } else {
            serde_json::json!([copy_write("output", "1bad")])
        };
        let error = compile_json(graph(
            serde_json::json!([]),
            serde_json::json!([invalid]),
            serde_json::json!([]),
        ))
        .unwrap_err();
        assert_eq!(error.code, "GRAPH_INVALID_ID");
    }

    let error = compile_json(graph(
        serde_json::json!([]),
        serde_json::json!([]),
        serde_json::json!([{"name":"present", "resource":resource_ref("1bad")}]),
    ))
    .unwrap_err();
    assert_eq!(error.code, "GRAPH_INVALID_ID");
}
