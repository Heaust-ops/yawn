use super::MeshFlag;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticType {
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
pub enum ExecutionClass {
    Source,
    CpuPreparation,
    Compute,
    Render,
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputCardinality {
    RequiredOne,
    OptionalOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "types", rename_all = "snake_case")]
pub enum TypeConstraint {
    Exact(SemanticType),
    OneOf(&'static [SemanticType]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputRole {
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
pub enum OutputMetadata {
    None,
    BooleanFlag { flag: MeshFlag },
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSocketContract {
    pub name: &'static str,
    pub accepted: TypeConstraint,
    pub cardinality: InputCardinality,
    pub role: InputRole,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSocketContract {
    pub name: &'static str,
    pub semantic_type: SemanticType,
    pub metadata: OutputMetadata,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub key: &'static str,
    pub version: u32,
    pub execution: ExecutionClass,
    pub inputs: &'static [InputSocketContract],
    pub outputs: &'static [OutputSocketContract],
    pub inherently_observable: bool,
}

use SemanticType::*;

const fn input(
    name: &'static str,
    accepted: TypeConstraint,
    cardinality: InputCardinality,
    role: InputRole,
) -> InputSocketContract {
    InputSocketContract {
        name,
        accepted,
        cardinality,
        role,
    }
}

const fn output(
    name: &'static str,
    semantic_type: SemanticType,
    metadata: OutputMetadata,
) -> OutputSocketContract {
    OutputSocketContract {
        name,
        semantic_type,
        metadata,
    }
}

const REQUIRED: InputCardinality = InputCardinality::RequiredOne;
const OPTIONAL: InputCardinality = InputCardinality::OptionalOne;
const NONE_IN: &[InputSocketContract] = &[];
const NONE_OUT: &[OutputSocketContract] = &[];
const SURFACE_OUT: &[OutputSocketContract] =
    &[output("surface", SurfaceTarget, OutputMetadata::None)];
const SPEC_OUT: &[OutputSocketContract] = &[output("spec", TextureSpec, OutputMetadata::None)];
const SCENE_OUT: &[OutputSocketContract] = &[output("scene", SceneTable, OutputMetadata::None)];
const AABB_OUT: &[OutputSocketContract] =
    &[output("localAabbs", LocalAabbBuffer, OutputMetadata::None)];
const FRUSTUM_OUT: &[OutputSocketContract] =
    &[output("frustum", CameraFrustum, OutputMetadata::None)];
const VISIBLE_OUT: &[OutputSocketContract] = &[output(
    "flags",
    BooleanFlagBuffer,
    OutputMetadata::BooleanFlag {
        flag: MeshFlag::IsVisible,
    },
)];
const CULLED_OUT: &[OutputSocketContract] = &[output(
    "flags",
    BooleanFlagBuffer,
    OutputMetadata::BooleanFlag {
        flag: MeshFlag::IsFrustumCulled,
    },
)];
const DRAW_OUT: &[OutputSocketContract] = &[output("draws", DrawStream, OutputMetadata::None)];
const CONFIG_OUT: &[OutputSocketContract] =
    &[output("config", DepthStencilConfig, OutputMetadata::None)];
const FORWARD_OUT: &[OutputSocketContract] = &[
    output("color", Texture, OutputMetadata::None),
    output("depth", Texture, OutputMetadata::None),
];
const FULLSCREEN_COPY_OUT: &[OutputSocketContract] =
    &[output("color", Texture, OutputMetadata::None)];
const LOCAL_IN: &[InputSocketContract] = &[input(
    "scene",
    TypeConstraint::Exact(SceneTable),
    REQUIRED,
    InputRole::SemanticRead,
)];
const VISIBILITY_IN: &[InputSocketContract] = LOCAL_IN;
const CULL_IN: &[InputSocketContract] = &[
    input(
        "scene",
        TypeConstraint::Exact(SceneTable),
        REQUIRED,
        InputRole::StorageRead,
    ),
    input(
        "localAabbs",
        TypeConstraint::Exact(LocalAabbBuffer),
        REQUIRED,
        InputRole::StorageRead,
    ),
    input(
        "frustum",
        TypeConstraint::Exact(CameraFrustum),
        REQUIRED,
        InputRole::UniformRead,
    ),
];
const QUERY_IN: &[InputSocketContract] = &[
    input(
        "scene",
        TypeConstraint::Exact(SceneTable),
        REQUIRED,
        InputRole::StorageRead,
    ),
    input(
        "isVisible",
        TypeConstraint::Exact(BooleanFlagBuffer),
        OPTIONAL,
        InputRole::StorageRead,
    ),
    input(
        "isFrustumCulled",
        TypeConstraint::Exact(BooleanFlagBuffer),
        OPTIONAL,
        InputRole::StorageRead,
    ),
];
const FORWARD_IN: &[InputSocketContract] = &[
    input(
        "scene",
        TypeConstraint::Exact(SceneTable),
        REQUIRED,
        InputRole::SemanticRead,
    ),
    input(
        "draws",
        TypeConstraint::Exact(DrawStream),
        REQUIRED,
        InputRole::IndirectRead,
    ),
    input(
        "colorTarget",
        TypeConstraint::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRole::ColorTarget { location: 0 },
    ),
    input(
        "depthTarget",
        TypeConstraint::OneOf(&[TextureSpec, Texture]),
        REQUIRED,
        InputRole::DepthTarget,
    ),
    input(
        "depthStencil",
        TypeConstraint::Exact(DepthStencilConfig),
        REQUIRED,
        InputRole::Configuration,
    ),
];
const FULLSCREEN_COPY_IN: &[InputSocketContract] = &[
    input(
        "source",
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::SampledTexture,
    ),
    input(
        "colorTarget",
        TypeConstraint::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRole::ColorTarget { location: 0 },
    ),
];
const BLOOM_COMPOSITE_IN: &[InputSocketContract] = &[
    input(
        "source",
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::SampledTexture,
    ),
    input(
        "bloom",
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::SampledTexture,
    ),
    input(
        "colorTarget",
        TypeConstraint::OneOf(&[SurfaceTarget, TextureSpec, Texture]),
        REQUIRED,
        InputRole::ColorTarget { location: 0 },
    ),
];
const PRESENT_IN: &[InputSocketContract] = &[input(
    "surface",
    TypeConstraint::Exact(Texture),
    REQUIRED,
    InputRole::Present,
)];

pub static CONTRACTS: &[Contract] = &[
    Contract {
        key: "surface_target",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: SURFACE_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "texture_spec",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: SPEC_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "scene_table",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: SCENE_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "local_aabb_buffer",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: LOCAL_IN,
        outputs: AABB_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "camera_frustum",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: FRUSTUM_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "visibility_flags",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: VISIBILITY_IN,
        outputs: VISIBLE_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "frustum_cull",
        version: 1,
        execution: ExecutionClass::Compute,
        inputs: CULL_IN,
        outputs: CULLED_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "mesh_query",
        version: 1,
        execution: ExecutionClass::Compute,
        inputs: QUERY_IN,
        outputs: DRAW_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "depth_stencil_config",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: CONFIG_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "legacy_forward",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FORWARD_IN,
        outputs: FORWARD_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "fullscreen_copy",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "tone_map",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "bloom_extract",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "bloom_blur",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "bloom_composite",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: BLOOM_COMPOSITE_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "luminance_edge",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
    },
    Contract {
        key: "present",
        version: 1,
        execution: ExecutionClass::Present,
        inputs: PRESENT_IN,
        outputs: NONE_OUT,
        inherently_observable: true,
    },
];

pub fn contract(key: &str) -> Option<&'static Contract> {
    CONTRACTS.iter().find(|contract| contract.key == key)
}
