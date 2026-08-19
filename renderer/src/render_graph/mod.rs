//! Device-free render graph AST, compiler, and compiled graph registry.

mod ast;
mod compiler;
pub(crate) use compiler::execution_attachments;
mod contracts;
mod expression;
mod plan;
mod registry;
mod runtime;
mod schema;

pub use compiler::{compile, parse_and_compile};
pub use contracts::*;
pub use expression::*;
pub use plan::*;
pub use registry::{CompiledGraphId, Registry};
pub use runtime::*;
pub use schema::*;

pub const MAX_AST_BYTES: usize = 1024 * 1024;
pub const MAX_EXECUTIONS: usize = 1024;

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
pub(crate) mod tests;
