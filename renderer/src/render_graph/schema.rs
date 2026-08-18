use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Graph {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    #[serde(default)]
    pub pipelines: PipelineDeclarations,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub state: NodeState,
    pub executor: ExecutorRef,
    pub parameters: serde_json::Value,
    pub inputs: BTreeMap<String, Vec<NodeOutputRef>>,
}

/// GPU programs shipped with a graph AST and prepared with the graph loadout.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PipelineDeclarations {
    #[serde(default)]
    pub render: Vec<RenderPipelineDeclaration>,
    #[serde(default)]
    pub compute: Vec<ComputePipelineDeclaration>,
}

/// A scene render pipeline using Yawn's fixed mesh/instance SOA vertex layout.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenderPipelineDeclaration {
    pub name: String,
    pub shader: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    #[serde(default)]
    pub double_sided: bool,
}

/// A binding-free compute pass dispatched before the graph's render passes.
///
/// Bindings are deliberately not implicit: shared SOA bindings will be added as an
/// explicit AST resource contract rather than inferred from shader source.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ComputePipelineDeclaration {
    pub name: String,
    pub shader: String,
    pub entry: String,
    pub dispatch: [u32; 3],
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    Enabled,
    Muted,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRef {
    pub key: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct NodeOutputRef {
    pub node: String,
    pub socket: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TextureDimension {
    D1,
    D2,
    D3,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    Rgba16Float,
    R32Float,
    Depth32Float,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextureExtent {
    Absolute {
        width: u32,
        height: u32,
        #[serde(rename = "depthOrArrayLayers")]
        depth_or_array_layers: u32,
    },
    SurfaceRelative {
        width: Ratio,
        height: Ratio,
        #[serde(rename = "depthOrArrayLayers")]
        depth_or_array_layers: u32,
    },
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Ratio {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TextureResidency {
    Transient,
    Persistent,
    History,
    Readback,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextureDescriptor {
    pub dimension: TextureDimension,
    pub format: TextureFormat,
    pub extent: TextureExtent,
    pub mip_level_count: u32,
    pub sample_count: u32,
    #[serde(default)]
    pub view_formats: Vec<TextureFormat>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareFunction {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

pub(crate) fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}
