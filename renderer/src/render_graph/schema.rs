use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GraphV1 {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    pub resources: Vec<Resource>,
    pub passes: Vec<Pass>,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResourceRef {
    pub id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub version: u32,
    pub residency: Residency,
    pub texture: TextureDescriptor,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Residency {
    External { source: ExternalSource },
    Transient,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSource {
    SurfaceColor,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TextureDescriptor {
    pub dimension: Dimension,
    pub format: Format,
    pub extent: Extent,
    pub mip_level_count: u32,
    pub sample_count: u32,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    D1,
    D2,
    D3,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Surface,
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    Rgba16Float,
    R32Float,
    Depth32Float,
}
impl Format {
    pub(crate) fn depth(self) -> bool {
        self == Self::Depth32Float
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Extent {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pass {
    pub id: String,
    pub state: PassState,
    pub executor: ExecutorRef,
    pub parameters: serde_json::Value,
    pub reads: Vec<ReadBinding>,
    pub writes: Vec<WriteBinding>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PassState {
    Enabled,
    Muted,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRef {
    pub key: String,
    pub version: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadBinding {
    pub binding: String,
    pub resource: ResourceRef,
    pub access: ReadAccess,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReadAccess {
    Sampled,
    Storage,
    CopySrc,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WriteBinding {
    pub binding: String,
    pub resource: ResourceRef,
    pub access: WriteAccess,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriteAccess {
    Storage,
    CopyDst,
    ColorAttachment {
        location: u32,
        load: ColorLoad,
        store: StoreOp,
    },
    DepthAttachment {
        load: DepthLoad,
        store: StoreOp,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColorLoad {
    Clear { value: [f64; 4] },
    Load,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum DepthLoad {
    Clear { value: f32 },
    Load,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreOp {
    Store,
    Discard,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub name: String,
    pub resource: ResourceRef,
}

pub(crate) fn identifier(s: &str) -> bool {
    s.as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'/' | b'-'))
}
