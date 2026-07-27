use serde::Serialize;

use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledGraph {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    pub node_count: u32,
    pub resources: Vec<CompiledResource>,
    pub executions: Vec<CompiledExecution>,
    pub texture_families: Vec<TextureFamily>,
    pub allocation_classes: Vec<AllocationClass>,
    pub culled_node_count: u32,
    pub culled_resource_count: u32,
    pub transient_slot_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledResource {
    pub original_node_index: u32,
    pub output_ordinal: u16,
    pub origin: NodeOutputRef,
    pub semantic_type: SemanticType,
    pub producer_execution: Option<u32>,
    pub lifetime: Option<Lifetime>,
    pub plan: ResourcePlan,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourcePlan {
    SurfaceTarget {
        family: u32,
    },
    TextureSpec {
        family: u32,
        residency: TextureResidency,
        descriptor: NormalizedTextureDescriptor,
    },
    Texture {
        family: u32,
        version: u32,
        target: u32,
        initialized: bool,
        stored: bool,
        allocation: Option<AllocationRef>,
    },
    SceneTable,
    LocalAabbBuffer {
        scene: u32,
    },
    CameraFrustum,
    BooleanFlagBuffer {
        scene: u32,
        flag: MeshFlag,
    },
    DrawStream {
        scene: u32,
    },
    DepthStencilConfig {
        config: NormalizedDepthStencil,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExecution {
    pub id: String,
    pub original_node_index: u32,
    pub executor: ExecutorRef,
    pub parameters: NormalizedParameters,
    pub kind: ExecutionKind,
    pub inputs: Vec<CompiledSocketInput>,
    pub outputs: Vec<CompiledSocketOutput>,
    pub accesses: Vec<CompiledAccess>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSocketInput {
    pub socket: String,
    pub resource: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSocketOutput {
    pub socket: String,
    pub resource: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionKind {
    CpuPreparation,
    Compute {
        work: ComputeWork,
    },
    Render {
        color_attachments: Vec<ColorAttachmentPlan>,
        depth_stencil: Option<DepthStencilAttachmentPlan>,
    },
    Present {
        surface: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeWork {
    FrustumCull,
    MeshQuery,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorAttachmentPlan {
    pub resource: u32,
    pub location: u32,
    pub load: NormalizedColorLoad,
    pub store: StoreOp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthStencilAttachmentPlan {
    pub resource: u32,
    pub load: NormalizedDepthLoad,
    pub store: StoreOp,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedColorLoad {
    Load,
    Clear { value: [f64; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedDepthLoad {
    Load,
    Clear { value: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreOp {
    Store,
    Discard,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledAccess {
    pub socket: String,
    pub resource: u32,
    pub mode: AccessMode,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccessMode {
    SemanticRead,
    UniformRead,
    StorageRead,
    StorageWrite {
        full_overwrite: bool,
    },
    IndirectRead,
    SampledTexture,
    ColorAttachment {
        location: u32,
        load: NormalizedColorLoad,
        store: StoreOp,
        full_overwrite: bool,
    },
    DepthAttachment {
        load: NormalizedDepthLoad,
        store: StoreOp,
        full_overwrite: bool,
    },
    Present,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedParameters {
    SurfaceTarget,
    TextureSpec {
        residency: TextureResidency,
        texture: NormalizedTextureDescriptor,
    },
    SceneTable,
    LocalAabbBuffer,
    CameraFrustum,
    VisibilityFlags,
    FrustumCull,
    MeshQuery {
        filters: [NormalizedMeshFilter; 2],
    },
    DepthStencilConfig {
        config: NormalizedDepthStencil,
    },
    LegacyForward {
        clear_color: [f64; 4],
    },
    FullscreenCopy,
    ToneMap {
        exposure: f32,
    },
    BloomExtract {
        threshold: f32,
        knee: f32,
    },
    BloomBlur {
        direction: [f32; 2],
        radius: f32,
    },
    BloomComposite {
        intensity: f32,
    },
    LuminanceEdge {
        strength: f32,
    },
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedMeshFilter {
    pub flag: MeshFlag,
    pub predicate: TriStatePredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedDepthStencil {
    pub depth_compare: CompareFunction,
    pub depth_write_enabled: bool,
    pub clear_depth: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedTextureDescriptor {
    pub dimension: TextureDimension,
    pub format: TextureFormat,
    pub extent: NormalizedTextureExtent,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub view_formats: Vec<TextureFormat>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedTextureExtent {
    Absolute {
        width: u32,
        height: u32,
        depth_or_array_layers: u32,
    },
    SurfaceRelative {
        width: Ratio,
        height: Ratio,
        depth_or_array_layers: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifetime {
    pub first_use: u32,
    pub last_use: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFamilyKey {
    pub source_node: u32,
    pub source_socket: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextureFamilySource {
    ImportedSurface {
        resource: u32,
    },
    AuthoredTexture {
        resource: u32,
        residency: TextureResidency,
        descriptor: NormalizedTextureDescriptor,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFamily {
    pub id: u32,
    pub key: TextureFamilyKey,
    pub source: TextureFamilySource,
    pub lifetime: Lifetime,
    pub versions: Vec<TextureVersion>,
    pub usage: Vec<TextureUsage>,
    pub allocation: Option<AllocationRef>,
    pub aliasable: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureVersion {
    pub version: u32,
    pub resource: u32,
    pub target: u32,
    pub initialized: bool,
    pub stored: bool,
    pub lifetime: Lifetime,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureCompatibilityKey {
    pub dimension: TextureDimension,
    pub format: TextureFormat,
    pub extent: NormalizedTextureExtent,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub view_formats: Vec<TextureFormat>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationClass {
    pub key: TextureCompatibilityKey,
    pub slots: Vec<AllocationSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationKind {
    AliasedTransient,
    DedicatedTransient,
    Persistent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlot {
    pub kind: AllocationKind,
    pub usage: Vec<TextureUsage>,
    pub occupants: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationRef {
    pub class: u32,
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureUsage {
    Sampled,
    Storage,
    CopySrc,
    CopyDst,
    ColorAttachment,
    DepthAttachment,
}

impl CompiledGraph {
    pub fn summary(&self, id: [u32; 2]) -> serde_json::Value {
        serde_json::json!({"compiledId":id,"graphId":self.graph_id,"revision":self.revision,"schemaVersion":self.schema_version,"nodeCount":self.node_count,"executionCount":self.executions.len(),"resourceCount":self.resources.len(),"culledNodeCount":self.culled_node_count,"culledResourceCount":self.culled_resource_count,"transientSlotCount":self.transient_slot_count})
    }
}
