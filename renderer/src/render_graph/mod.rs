//! Device-free V1 render graph compiler and compiled graph registry.

mod compiler;
mod compiler_v2;
mod contracts_v2;
mod plan_v2;
mod registry;
mod runtime;
mod runtime_v2;
mod schema;
mod schema_v2;

pub use compiler::{
    compile, compile_with, parse_and_compile, AllocationClass, CompiledGraph, CompiledOutput,
    CompiledPass, CompiledRead, CompiledResource, CompiledWrite, ExecutorContract,
    ExecutorRegistry, ExecutorResolution, Lifetime, NormalizedParameters, SceneForwardExecutors,
    TextureAllocationKey, TextureUsage, TransientAllocation,
};
pub use compiler_v2::{compile_v2, mesh_predicate_matches, parse_and_compile_v2};
pub use contracts_v2::*;
pub use plan_v2::*;
pub use registry::{CompiledGraphId, RegisteredGraph, Registry};
pub use runtime::{
    class_offsets, resolve_extent, runtime_texture_key, validate_activatable, ResolvedExtent,
    RuntimeTextureKey,
};
pub use runtime_v2::*;
pub use schema::*;
pub use schema_v2::*;

pub fn parse_and_compile_any(bytes: &[u8]) -> Result<RegisteredGraph, GraphError> {
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
    match value.get("schemaVersion").and_then(|v| v.as_u64()) {
        Some(1) => parse_and_compile(bytes).map(RegisteredGraph::V1),
        Some(2) => parse_and_compile_v2(bytes).map(RegisteredGraph::V2),
        _ => Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be exactly 1 or 2",
        )),
    }
}

pub const MAX_JSON_BYTES: usize = 1024 * 1024;
pub const MAX_RESOURCES: usize = 1024;
pub const MAX_PASSES: usize = 1024;
pub const MAX_USES: usize = 8192;
pub const MAX_OUTPUTS: usize = 64;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct GraphError {
    pub code: &'static str,
    pub message: String,
    pub details: serde_json::Value,
}

impl GraphError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code,
            details: serde_json::json!({"message": message}),
            message,
        }
    }
    pub(crate) fn at(
        code: &'static str,
        message: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        let message = message.into();
        Self {
            code,
            details: serde_json::json!({"message": message, "path": path.into()}),
            message,
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_v2;
