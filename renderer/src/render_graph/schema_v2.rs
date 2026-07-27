use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GraphV2 {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    pub nodes: Vec<NodeV2>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeV2 {
    pub id: String,
    pub state: NodeStateV2,
    pub executor: ExecutorRefV2,
    pub parameters: serde_json::Value,
    pub inputs: BTreeMap<String, NodeOutputRef>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStateV2 {
    Enabled,
    Muted,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRefV2 {
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
pub enum TextureDimensionV2 {
    D1,
    D2,
    D3,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TextureFormatV2 {
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
pub enum TextureExtentV2 {
    Absolute {
        width: u32,
        height: u32,
        #[serde(rename = "depthOrArrayLayers")]
        depth_or_array_layers: u32,
    },
    SurfaceRelative {
        width: RatioV2,
        height: RatioV2,
        #[serde(rename = "depthOrArrayLayers")]
        depth_or_array_layers: u32,
    },
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct RatioV2 {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TextureResidencyV2 {
    Transient,
    Persistent,
    History,
    Readback,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextureDescriptorV2 {
    pub dimension: TextureDimensionV2,
    pub format: TextureFormatV2,
    pub extent: TextureExtentV2,
    pub mip_level_count: u32,
    pub sample_count: u32,
    #[serde(default)]
    pub view_formats: Vec<TextureFormatV2>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TriStatePredicate {
    Any,
    RequiredTrue,
    RequiredFalse,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct MeshFilterV2 {
    pub flag: MeshFlagV2,
    pub predicate: TriStatePredicate,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum MeshFlagV2 {
    IsVisible,
    IsFrustumCulled,
}

impl MeshFlagV2 {
    pub const ORDERED: [Self; 2] = [Self::IsVisible, Self::IsFrustumCulled];

    pub const fn input_socket(self) -> &'static str {
        match self {
            Self::IsVisible => "isVisible",
            Self::IsFrustumCulled => "isFrustumCulled",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareFunctionV2 {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}
