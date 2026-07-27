use serde::Serialize;

use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledGraphV2 {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u32,
    pub node_count: u32,
    pub resources: Vec<CompiledResourceV2>,
    pub executions: Vec<CompiledExecutionV2>,
    pub texture_families: Vec<TextureFamilyV2>,
    pub allocation_classes: Vec<AllocationClassV2>,
    pub culled_node_count: u32,
    pub culled_resource_count: u32,
    pub transient_slot_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledResourceV2 {
    pub original_node_index: u32,
    pub output_ordinal: u16,
    pub origin: NodeOutputRef,
    pub semantic_type: SemanticTypeV2,
    pub producer_execution: Option<u32>,
    pub lifetime: Option<LifetimeV2>,
    pub plan: ResourcePlanV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourcePlanV2 {
    SurfaceTarget {
        family: u32,
    },
    TextureSpec {
        family: u32,
        residency: TextureResidencyV2,
        descriptor: NormalizedTextureDescriptorV2,
    },
    Texture {
        family: u32,
        version: u32,
        target: u32,
        initialized: bool,
        stored: bool,
        allocation: Option<AllocationRefV2>,
    },
    SceneTable,
    LocalAabbBuffer {
        scene: u32,
    },
    CameraFrustum,
    BooleanFlagBuffer {
        scene: u32,
        flag: MeshFlagV2,
    },
    DrawStream {
        scene: u32,
    },
    DepthStencilConfig {
        config: NormalizedDepthStencilV2,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExecutionV2 {
    pub id: String,
    pub original_node_index: u32,
    pub executor: ExecutorRefV2,
    pub parameters: NormalizedParametersV2,
    pub kind: ExecutionKindV2,
    pub inputs: Vec<CompiledSocketInputV2>,
    pub outputs: Vec<CompiledSocketOutputV2>,
    pub accesses: Vec<CompiledAccessV2>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSocketInputV2 {
    pub socket: String,
    pub resource: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledSocketOutputV2 {
    pub socket: String,
    pub resource: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionKindV2 {
    CpuPreparation,
    Compute {
        work: ComputeWorkV2,
    },
    Render {
        color_attachments: Vec<ColorAttachmentPlanV2>,
        depth_stencil: Option<DepthStencilAttachmentPlanV2>,
    },
    Present {
        surface: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeWorkV2 {
    FrustumCull,
    MeshQuery,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorAttachmentPlanV2 {
    pub resource: u32,
    pub location: u32,
    pub load: NormalizedColorLoadV2,
    pub store: StoreOpV2,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthStencilAttachmentPlanV2 {
    pub resource: u32,
    pub load: NormalizedDepthLoadV2,
    pub store: StoreOpV2,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedColorLoadV2 {
    Load,
    Clear { value: [f64; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedDepthLoadV2 {
    Load,
    Clear { value: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreOpV2 {
    Store,
    Discard,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledAccessV2 {
    pub socket: String,
    pub resource: u32,
    pub mode: AccessModeV2,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccessModeV2 {
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
        load: NormalizedColorLoadV2,
        store: StoreOpV2,
        full_overwrite: bool,
    },
    DepthAttachment {
        load: NormalizedDepthLoadV2,
        store: StoreOpV2,
        full_overwrite: bool,
    },
    Present,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedParametersV2 {
    SurfaceTarget,
    TextureSpec {
        residency: TextureResidencyV2,
        texture: NormalizedTextureDescriptorV2,
    },
    SceneTable,
    LocalAabbBuffer,
    CameraFrustum,
    VisibilityFlags,
    FrustumCull,
    MeshQuery {
        filters: [NormalizedMeshFilterV2; 2],
    },
    DepthStencilConfig {
        config: NormalizedDepthStencilV2,
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
pub struct NormalizedMeshFilterV2 {
    pub flag: MeshFlagV2,
    pub predicate: TriStatePredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedDepthStencilV2 {
    pub depth_compare: CompareFunctionV2,
    pub depth_write_enabled: bool,
    pub clear_depth: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedTextureDescriptorV2 {
    pub dimension: TextureDimensionV2,
    pub format: TextureFormatV2,
    pub extent: NormalizedTextureExtentV2,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub view_formats: Vec<TextureFormatV2>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedTextureExtentV2 {
    Absolute {
        width: u32,
        height: u32,
        depth_or_array_layers: u32,
    },
    SurfaceRelative {
        width: RatioV2,
        height: RatioV2,
        depth_or_array_layers: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifetimeV2 {
    pub first_use: u32,
    pub last_use: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFamilyKeyV2 {
    pub source_node: u32,
    pub source_socket: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextureFamilySourceV2 {
    ImportedSurface {
        resource: u32,
    },
    AuthoredTexture {
        resource: u32,
        residency: TextureResidencyV2,
        descriptor: NormalizedTextureDescriptorV2,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFamilyV2 {
    pub id: u32,
    pub key: TextureFamilyKeyV2,
    pub source: TextureFamilySourceV2,
    pub lifetime: LifetimeV2,
    pub versions: Vec<TextureVersionV2>,
    pub usage: Vec<TextureUsageV2>,
    pub allocation: Option<AllocationRefV2>,
    pub aliasable: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureVersionV2 {
    pub version: u32,
    pub resource: u32,
    pub target: u32,
    pub initialized: bool,
    pub stored: bool,
    pub lifetime: LifetimeV2,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureCompatibilityKeyV2 {
    pub dimension: TextureDimensionV2,
    pub format: TextureFormatV2,
    pub extent: NormalizedTextureExtentV2,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub view_formats: Vec<TextureFormatV2>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationClassV2 {
    pub key: TextureCompatibilityKeyV2,
    pub slots: Vec<AllocationSlotV2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationKindV2 {
    AliasedTransient,
    DedicatedTransient,
    Persistent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlotV2 {
    pub kind: AllocationKindV2,
    pub usage: Vec<TextureUsageV2>,
    pub occupants: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationRefV2 {
    pub class: u32,
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureUsageV2 {
    Sampled,
    Storage,
    CopySrc,
    CopyDst,
    ColorAttachment,
    DepthAttachment,
}

impl CompiledGraphV2 {
    pub fn summary(&self, id: [u32; 2]) -> serde_json::Value {
        serde_json::json!({"compiledId":id,"graphId":self.graph_id,"revision":self.revision,"schemaVersion":self.schema_version,"nodeCount":self.node_count,"executionCount":self.executions.len(),"resourceCount":self.resources.len(),"culledNodeCount":self.culled_node_count,"culledResourceCount":self.culled_resource_count,"transientSlotCount":self.transient_slot_count})
    }
}
