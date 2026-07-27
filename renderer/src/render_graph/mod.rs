//! Device-free V1 render graph compiler and compiled graph registry.

mod compiler;
mod registry;
mod runtime;
mod schema;

pub use compiler::{
    compile, compile_with, parse_and_compile, AllocationClass, CompiledGraph, CompiledOutput,
    CompiledPass, CompiledRead, CompiledResource, CompiledWrite, ExecutorContract,
    ExecutorRegistry, ExecutorResolution, Lifetime, NormalizedParameters, SceneForwardExecutors,
    TextureAllocationKey, TextureUsage, TransientAllocation,
};
pub use registry::{CompiledGraphId, Registry};
pub use runtime::{
    class_offsets, resolve_extent, runtime_texture_key, validate_activatable, ResolvedExtent,
    RuntimeTextureKey,
};
pub use schema::*;

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
