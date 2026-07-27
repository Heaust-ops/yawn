use super::MeshFlagV2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticTypeV2 {
    SurfaceTarget,
    TextureSpec,
    Texture,
    SceneTable,
    LocalAabbBuffer,
    CameraFrustum,
    BooleanFlagBuffer,
    DrawStream,
    DepthStencilConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionClassV2 {
    Source,
    CpuPreparation,
    Compute,
    Render,
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputCardinalityV2 {
    RequiredOne,
    OptionalOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "types", rename_all = "snake_case")]
pub enum TypeConstraintV2 {
    Exact(SemanticTypeV2),
    OneOf(&'static [SemanticTypeV2]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputRoleV2 {
    SemanticRead,
    UniformRead,
    StorageRead,
    IndirectRead,
    SampledTexture,
    ColorTarget { location: u32 },
    DepthTarget,
    Present,
    Configuration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputMetadataV2 {
    None,
    BooleanFlag { flag: MeshFlagV2 },
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSocketContractV2 {
    pub name: &'static str,
    pub accepted: TypeConstraintV2,
    pub cardinality: InputCardinalityV2,
    pub role: InputRoleV2,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSocketContractV2 {
    pub name: &'static str,
    pub semantic_type: SemanticTypeV2,
    pub metadata: OutputMetadataV2,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractV2 {
    pub key: &'static str,
    pub version: u32,
    pub execution: ExecutionClassV2,
    pub inputs: &'static [InputSocketContractV2],
    pub outputs: &'static [OutputSocketContractV2],
    pub inherently_observable: bool,
}

use SemanticTypeV2::*;

const fn input(
    name: &'static str,
    accepted: TypeConstraintV2,
    cardinality: InputCardinalityV2,
    role: InputRoleV2,
) -> InputSocketContractV2 {
    InputSocketContractV2 {
        name,
        accepted,
        cardinality,
        role,
    }
}

const fn output(
    name: &'static str,
    semantic_type: SemanticTypeV2,
    metadata: OutputMetadataV2,
) -> OutputSocketContractV2 {
    OutputSocketContractV2 {
        name,
        semantic_type,
        metadata,
    }
}

const REQUIRED: InputCardinalityV2 = InputCardinalityV2::RequiredOne;
const OPTIONAL: InputCardinalityV2 = InputCardinalityV2::OptionalOne;
const NONE_IN: &[InputSocketContractV2] = &[];
const NONE_OUT: &[OutputSocketContractV2] = &[];
const SURFACE_OUT: &[OutputSocketContractV2] =
    &[output("surface", SurfaceTarget, OutputMetadataV2::None)];
const SPEC_OUT: &[OutputSocketContractV2] = &[output("spec", TextureSpec, OutputMetadataV2::None)];
const SCENE_OUT: &[OutputSocketContractV2] = &[output("scene", SceneTable, OutputMetadataV2::None)];
const AABB_OUT: &[OutputSocketContractV2] = &[output(
    "localAabbs",
    LocalAabbBuffer,
    OutputMetadataV2::None,
)];
const FRUSTUM_OUT: &[OutputSocketContractV2] =
    &[output("frustum", CameraFrustum, OutputMetadataV2::None)];
const VISIBLE_OUT: &[OutputSocketContractV2] = &[output(
    "flags",
    BooleanFlagBuffer,
    OutputMetadataV2::BooleanFlag {
        flag: MeshFlagV2::IsVisible,
    },
)];
const CULLED_OUT: &[OutputSocketContractV2] = &[output(
    "flags",
    BooleanFlagBuffer,
    OutputMetadataV2::BooleanFlag {
        flag: MeshFlagV2::IsFrustumCulled,
    },
)];
const DRAW_OUT: &[OutputSocketContractV2] = &[output("draws", DrawStream, OutputMetadataV2::None)];
const CONFIG_OUT: &[OutputSocketContractV2] =
    &[output("config", DepthStencilConfig, OutputMetadataV2::None)];
const FORWARD_OUT: &[OutputSocketContractV2] = &[
    output("color", Texture, OutputMetadataV2::None),
    output("depth", Texture, OutputMetadataV2::None),
];
const FULLSCREEN_COPY_OUT: &[OutputSocketContractV2] =
    &[output("color", Texture, OutputMetadataV2::None)];
const LOCAL_IN: &[InputSocketContractV2] = &[input(
    "scene",
    TypeConstraintV2::Exact(SceneTable),
    REQUIRED,
    InputRoleV2::SemanticRead,
)];
const VISIBILITY_IN: &[InputSocketContractV2] = LOCAL_IN;
const CULL_IN: &[InputSocketContractV2] = &[
    input(
        "scene",
        TypeConstraintV2::Exact(SceneTable),
        REQUIRED,
        InputRoleV2::StorageRead,
    ),
    input(
        "localAabbs",
        TypeConstraintV2::Exact(LocalAabbBuffer),
        REQUIRED,
        InputRoleV2::StorageRead,
    ),
    input(
        "frustum",
        TypeConstraintV2::Exact(CameraFrustum),
        REQUIRED,
        InputRoleV2::UniformRead,
    ),
];
const QUERY_IN: &[InputSocketContractV2] = &[
    input(
        "scene",
        TypeConstraintV2::Exact(SceneTable),
        REQUIRED,
        InputRoleV2::StorageRead,
    ),
    input(
        "isVisible",
        TypeConstraintV2::Exact(BooleanFlagBuffer),
        OPTIONAL,
        InputRoleV2::StorageRead,
    ),
    input(
        "isFrustumCulled",
        TypeConstraintV2::Exact(BooleanFlagBuffer),
        OPTIONAL,
        InputRoleV2::StorageRead,
    ),
];
const FORWARD_IN: &[InputSocketContractV2] = &[
    input(
        "scene",
        TypeConstraintV2::Exact(SceneTable),
        REQUIRED,
        InputRoleV2::SemanticRead,
    ),
    input(
        "draws",
        TypeConstraintV2::Exact(DrawStream),
        REQUIRED,
        InputRoleV2::IndirectRead,
    ),
    input(
        "colorTarget",
        TypeConstraintV2::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRoleV2::ColorTarget { location: 0 },
    ),
    input(
        "depthTarget",
        TypeConstraintV2::OneOf(&[TextureSpec, Texture]),
        REQUIRED,
        InputRoleV2::DepthTarget,
    ),
    input(
        "depthStencil",
        TypeConstraintV2::Exact(DepthStencilConfig),
        REQUIRED,
        InputRoleV2::Configuration,
    ),
];
const FULLSCREEN_COPY_IN: &[InputSocketContractV2] = &[
    input(
        "source",
        TypeConstraintV2::Exact(Texture),
        REQUIRED,
        InputRoleV2::SampledTexture,
    ),
    input(
        "colorTarget",
        TypeConstraintV2::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRoleV2::ColorTarget { location: 0 },
    ),
];
const BLOOM_COMPOSITE_IN: &[InputSocketContractV2] = &[
    input(
        "source",
        TypeConstraintV2::Exact(Texture),
        REQUIRED,
        InputRoleV2::SampledTexture,
    ),
    input(
        "bloom",
        TypeConstraintV2::Exact(Texture),
        REQUIRED,
        InputRoleV2::SampledTexture,
    ),
    input(
        "colorTarget",
        TypeConstraintV2::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRoleV2::ColorTarget { location: 0 },
    ),
];
const PRESENT_IN: &[InputSocketContractV2] = &[input(
    "surface",
    TypeConstraintV2::Exact(Texture),
    REQUIRED,
    InputRoleV2::Present,
)];

pub static CONTRACTS_V2: &[ContractV2] = &[
    ContractV2 {
        key: "surface_target",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: NONE_IN,
        outputs: SURFACE_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "texture_spec",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: NONE_IN,
        outputs: SPEC_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "scene_table",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: NONE_IN,
        outputs: SCENE_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "local_aabb_buffer",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: LOCAL_IN,
        outputs: AABB_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "camera_frustum",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: NONE_IN,
        outputs: FRUSTUM_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "visibility_flags",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: VISIBILITY_IN,
        outputs: VISIBLE_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "frustum_cull",
        version: 1,
        execution: ExecutionClassV2::Compute,
        inputs: CULL_IN,
        outputs: CULLED_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "mesh_query",
        version: 1,
        execution: ExecutionClassV2::Compute,
        inputs: QUERY_IN,
        outputs: DRAW_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "depth_stencil_config",
        version: 1,
        execution: ExecutionClassV2::Source,
        inputs: NONE_IN,
        outputs: CONFIG_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "legacy_forward",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FORWARD_IN,
        outputs: FORWARD_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "fullscreen_copy",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "tone_map",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "bloom_extract",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "bloom_blur",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "bloom_composite",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: BLOOM_COMPOSITE_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "luminance_edge",
        version: 1,
        execution: ExecutionClassV2::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    ContractV2 {
        key: "present",
        version: 1,
        execution: ExecutionClassV2::Present,
        inputs: PRESENT_IN,
        outputs: NONE_OUT,
        inherently_observable: true,
    },
];

pub fn contract(key: &str) -> Option<&'static ContractV2> {
    CONTRACTS_V2.iter().find(|contract| contract.key == key)
}
