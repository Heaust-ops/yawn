use super::MeshFlag;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticType {
    MeshData,
    Texture,
    LocalAabbBuffer,
    BooleanFlagBuffer,
    PipelineIndexStream,
    PipelineActivation,
    DrawStream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionClass {
    Source,
    CpuPreparation,
    Compute,
    Render,
    Frame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenPolicy {
    Copy,
    ToneMap,
    HdrSameExtent,
    BloomExtract,
    BloomComposite,
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
    #[serde(skip)]
    pub fullscreen_policy: Option<FullscreenPolicy>,
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
const TEXTURE_OUT: &[OutputSocketContract] = &[output("texture", Texture, OutputMetadata::None)];
const MESH_OUT: &[OutputSocketContract] = &[
    output("mesh", MeshData, OutputMetadata::None),
    output("localAabbs", LocalAabbBuffer, OutputMetadata::None),
    output(
        "isVisible",
        BooleanFlagBuffer,
        OutputMetadata::BooleanFlag {
            flag: MeshFlag::IsVisible,
        },
    ),
    output("pipelineIndices", PipelineIndexStream, OutputMetadata::None),
];
const CULLED_OUT: &[OutputSocketContract] = &[output(
    "isFrustumCulled",
    BooleanFlagBuffer,
    OutputMetadata::BooleanFlag {
        flag: MeshFlag::IsFrustumCulled,
    },
)];
const DRAW_OUT: &[OutputSocketContract] = &[output("draws", DrawStream, OutputMetadata::None)];
const ACTIVATION_OUT: &[OutputSocketContract] = &[output(
    "activation",
    PipelineActivation,
    OutputMetadata::None,
)];
const PIPELINE_OUT: &[OutputSocketContract] = &[
    output("color", Texture, OutputMetadata::None),
    output("depth", Texture, OutputMetadata::None),
];
const FULLSCREEN_COPY_OUT: &[OutputSocketContract] =
    &[output("color", Texture, OutputMetadata::None)];
const CULL_IN: &[InputSocketContract] = &[
    input(
        "mesh",
        TypeConstraint::Exact(MeshData),
        REQUIRED,
        InputRole::StorageRead,
    ),
    input(
        "localAabbs",
        TypeConstraint::Exact(LocalAabbBuffer),
        REQUIRED,
        InputRole::StorageRead,
    ),
];
const QUERY_IN: &[InputSocketContract] = &[
    input(
        "mesh",
        TypeConstraint::Exact(MeshData),
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
const REGISTRY_IN: &[InputSocketContract] = &[input(
    "pipelineIndices",
    TypeConstraint::Exact(PipelineIndexStream),
    REQUIRED,
    InputRole::SemanticRead,
)];
const PIPELINE_IN: &[InputSocketContract] = &[
    input(
        "mesh",
        TypeConstraint::Exact(MeshData),
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
        "activation",
        TypeConstraint::Exact(PipelineActivation),
        REQUIRED,
        InputRole::SemanticRead,
    ),
    input(
        "colorTarget",
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::ColorTarget { location: 0 },
    ),
    input(
        "depthTarget",
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::DepthTarget,
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
        TypeConstraint::Exact(Texture),
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
        TypeConstraint::Exact(Texture),
        REQUIRED,
        InputRole::ColorTarget { location: 0 },
    ),
];
const FRAME_OUT_IN: &[InputSocketContract] = &[input(
    "color",
    TypeConstraint::Exact(Texture),
    REQUIRED,
    InputRole::SampledTexture,
)];

pub static CONTRACTS: &[Contract] = &[
    Contract {
        key: "mesh",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: MESH_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "texture",
        version: 1,
        execution: ExecutionClass::Source,
        inputs: NONE_IN,
        outputs: TEXTURE_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "frustum_cull",
        version: 1,
        execution: ExecutionClass::Compute,
        inputs: CULL_IN,
        outputs: CULLED_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "mesh_query",
        version: 1,
        execution: ExecutionClass::Compute,
        inputs: QUERY_IN,
        outputs: DRAW_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "pipeline_registry",
        version: 1,
        execution: ExecutionClass::CpuPreparation,
        inputs: REGISTRY_IN,
        outputs: ACTIVATION_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "pipeline",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: PIPELINE_IN,
        outputs: PIPELINE_OUT,
        inherently_observable: false,
        fullscreen_policy: None,
    },
    Contract {
        key: "fullscreen_copy",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::Copy),
    },
    Contract {
        key: "tone_map",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::ToneMap),
    },
    Contract {
        key: "bloom_extract",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::BloomExtract),
    },
    Contract {
        key: "bloom_blur",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::HdrSameExtent),
    },
    Contract {
        key: "bloom_composite",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: BLOOM_COMPOSITE_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::BloomComposite),
    },
    Contract {
        key: "luminance_edge",
        version: 1,
        execution: ExecutionClass::Render,
        inputs: FULLSCREEN_COPY_IN,
        outputs: FULLSCREEN_COPY_OUT,
        inherently_observable: false,
        fullscreen_policy: Some(FullscreenPolicy::HdrSameExtent),
    },
    Contract {
        key: "frame_out",
        version: 1,
        execution: ExecutionClass::Frame,
        inputs: FRAME_OUT_IN,
        outputs: NONE_OUT,
        inherently_observable: true,
        fullscreen_policy: None,
    },
];

pub fn contract(key: &str) -> Option<&'static Contract> {
    CONTRACTS.iter().find(|contract| contract.key == key)
}
