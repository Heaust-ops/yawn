#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticType {
    MeshData,
    Texture,
    Bool,
    F32,
    U32,
    Vec2,
    Vec3,
    Vec4,
    Mat2,
    Mat3,
    Mat4,
    U32x16,
    LocalAabb,
}

impl SemanticType {
    pub const fn is_virtual(self) -> bool {
        !matches!(self, Self::MeshData | Self::Texture)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionClass {
    Source,
    Expression,
    Render,
    Frame,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenPolicy {
    Copy,
    HdrSameExtent,
    BloomExtract,
    BloomComposite,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InputCardinality {
    pub min: u8,
    pub max: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputDefaultPolicy {
    None,
    ParameterLiteral,
    CompilerTexture,
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
    Expression,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSocketContract {
    pub name: &'static str,
    pub accepted: TypeConstraint,
    pub cardinality: InputCardinality,
    pub default_policy: InputDefaultPolicy,
    pub role: InputRole,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSocketContract {
    pub name: &'static str,
    pub semantic_type: SemanticType,
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
impl Contract {
    pub const fn is_raster_draw(&self) -> bool {
        matches!(self.execution, ExecutionClass::Render) && self.fullscreen_policy.is_none()
    }
}

use SemanticType::*;
const R: InputCardinality = InputCardinality { min: 1, max: 1 };
const O: InputCardinality = InputCardinality { min: 0, max: 1 };
const V: InputCardinality = InputCardinality { min: 0, max: 8 };
const fn i(
    name: &'static str,
    ty: SemanticType,
    cardinality: InputCardinality,
    role: InputRole,
) -> InputSocketContract {
    let default_policy = match (cardinality.min, cardinality.max) {
        (_, 2..) => InputDefaultPolicy::None,
        (1, _) => InputDefaultPolicy::None,
        _ => match role {
            InputRole::ColorTarget { .. } | InputRole::DepthTarget => {
                InputDefaultPolicy::CompilerTexture
            }
            _ => InputDefaultPolicy::ParameterLiteral,
        },
    };
    InputSocketContract {
        name,
        accepted: TypeConstraint::Exact(ty),
        cardinality,
        default_policy,
        role,
    }
}
const fn o(name: &'static str, semantic_type: SemanticType) -> OutputSocketContract {
    OutputSocketContract {
        name,
        semantic_type,
    }
}
const NONE_I: &[InputSocketContract] = &[];
const NONE_O: &[OutputSocketContract] = &[];
const MESH_O: &[OutputSocketContract] = &[
    o("mesh", MeshData),
    o("type", U32x16),
    o("localAabb", LocalAabb),
];
const TEXTURE_O: &[OutputSocketContract] = &[o("texture", Texture)];
const RASTER_I: &[InputSocketContract] = &[
    i("mesh", MeshData, R, InputRole::SemanticRead),
    i("predicate", Bool, O, InputRole::Expression),
    i("color", Texture, O, InputRole::ColorTarget { location: 0 }),
    i("depth", Texture, O, InputRole::DepthTarget),
];
const RASTER_O: &[OutputSocketContract] = &[o("color", Texture), o("depth", Texture)];
const CULL_I: &[InputSocketContract] = &[
    i("mesh", MeshData, R, InputRole::Expression),
    i("localAabb", LocalAabb, R, InputRole::Expression),
];
const CULL_O: &[OutputSocketContract] = &[o("isFrustumCulled", Bool)];
const COPY_I: &[InputSocketContract] = &[
    i("source", Texture, R, InputRole::SampledTexture),
    i(
        "colorTarget",
        Texture,
        R,
        InputRole::ColorTarget { location: 0 },
    ),
];
const BLOOM_I: &[InputSocketContract] = &[
    i("source", Texture, R, InputRole::SampledTexture),
    i("bloom", Texture, R, InputRole::SampledTexture),
    i(
        "colorTarget",
        Texture,
        R,
        InputRole::ColorTarget { location: 0 },
    ),
];
const COLOR_O: &[OutputSocketContract] = &[o("color", Texture)];
const FRAME_I: &[InputSocketContract] = &[i("color", Texture, R, InputRole::SampledTexture)];
macro_rules! ins { ($($n:literal:$t:ident),*) => { &[$(i($n,$t,O,InputRole::Expression)),*] } }
macro_rules! outs { ($($n:literal:$t:ident),*) => { &[$(o($n,$t)),*] } }
macro_rules! c {
    ($k:literal,$v:expr,$e:ident,$ins:expr,$outs:expr,$obs:expr,$policy:expr) => {
        Contract {
            key: $k,
            version: $v,
            execution: ExecutionClass::$e,
            inputs: $ins,
            outputs: $outs,
            inherently_observable: $obs,
            fullscreen_policy: $policy,
        }
    };
}
macro_rules! ex {
    ($k:literal,$ins:expr,$outs:expr) => {
        c!($k, 1, Expression, $ins, $outs, false, None)
    };
}
const BOOL_VARIADIC_I: &[InputSocketContract] = &[i("inputs", Bool, V, InputRole::Expression)];

pub static CONTRACTS: &[Contract] = &[
    c!("mesh", 2, Source, NONE_I, MESH_O, false, None),
    c!("texture", 2, Source, NONE_I, TEXTURE_O, false, None),
    c!("frustum_cull", 2, Expression, CULL_I, CULL_O, false, None),
    c!(
        "and",
        2,
        Expression,
        BOOL_VARIADIC_I,
        outs!("value":Bool),
        false,
        None
    ),
    c!(
        "or",
        2,
        Expression,
        BOOL_VARIADIC_I,
        outs!("value":Bool),
        false,
        None
    ),
    ex!("not", ins!("operand":Bool), outs!("value":Bool)),
    c!(
        "xor",
        2,
        Expression,
        BOOL_VARIADIC_I,
        outs!("value":Bool),
        false,
        None
    ),
    c!(
        "xnor",
        2,
        Expression,
        BOOL_VARIADIC_I,
        outs!("value":Bool),
        false,
        None
    ),
    ex!(
        "greater_than_f32",
        ins!("left":F32,"right":F32),
        outs!("value":Bool)
    ),
    ex!(
        "less_than_f32",
        ins!("left":F32,"right":F32),
        outs!("value":Bool)
    ),
    ex!(
        "equals_f32",
        ins!("left":F32,"right":F32),
        outs!("value":Bool)
    ),
    ex!(
        "greater_than_u32",
        ins!("left":U32,"right":U32),
        outs!("value":Bool)
    ),
    ex!(
        "less_than_u32",
        ins!("left":U32,"right":U32),
        outs!("value":Bool)
    ),
    ex!(
        "equals_u32",
        ins!("left":U32,"right":U32),
        outs!("value":Bool)
    ),
    ex!("separate_vec2", ins!("vector":Vec2), outs!("x":F32,"y":F32)),
    ex!("combine_vec2", ins!("x":F32,"y":F32), outs!("vector":Vec2)),
    ex!(
        "separate_vec3",
        ins!("vector":Vec3),
        outs!("x":F32,"y":F32,"z":F32)
    ),
    ex!(
        "combine_vec3",
        ins!("x":F32,"y":F32,"z":F32),
        outs!("vector":Vec3)
    ),
    ex!(
        "separate_vec4",
        ins!("vector":Vec4),
        outs!("x":F32,"y":F32,"z":F32,"w":F32)
    ),
    ex!(
        "combine_vec4",
        ins!("x":F32,"y":F32,"z":F32,"w":F32),
        outs!("vector":Vec4)
    ),
    ex!(
        "separate_mat2",
        ins!("matrix":Mat2),
        outs!("column0":Vec2,"column1":Vec2)
    ),
    ex!(
        "combine_mat2",
        ins!("column0":Vec2,"column1":Vec2),
        outs!("matrix":Mat2)
    ),
    ex!(
        "separate_mat3",
        ins!("matrix":Mat3),
        outs!("column0":Vec3,"column1":Vec3,"column2":Vec3)
    ),
    ex!(
        "combine_mat3",
        ins!("column0":Vec3,"column1":Vec3,"column2":Vec3),
        outs!("matrix":Mat3)
    ),
    ex!(
        "separate_mat4",
        ins!("matrix":Mat4),
        outs!("column0":Vec4,"column1":Vec4,"column2":Vec4,"column3":Vec4)
    ),
    ex!(
        "combine_mat4",
        ins!("column0":Vec4,"column1":Vec4,"column2":Vec4,"column3":Vec4),
        outs!("matrix":Mat4)
    ),
    ex!(
        "separate_u32x16",
        ins!("value":U32x16),
        outs!("word0":U32,"word1":U32,"word2":U32,"word3":U32,"word4":U32,"word5":U32,"word6":U32,"word7":U32,"word8":U32,"word9":U32,"word10":U32,"word11":U32,"word12":U32,"word13":U32,"word14":U32,"word15":U32)
    ),
    ex!(
        "combine_u32x16",
        ins!("word0":U32,"word1":U32,"word2":U32,"word3":U32,"word4":U32,"word5":U32,"word6":U32,"word7":U32,"word8":U32,"word9":U32,"word10":U32,"word11":U32,"word12":U32,"word13":U32,"word14":U32,"word15":U32),
        outs!("value":U32x16)
    ),
    ex!(
        "separate_u32_bits",
        ins!("value":U32),
        outs!("bit0":Bool,"bit1":Bool,"bit2":Bool,"bit3":Bool,"bit4":Bool,"bit5":Bool,"bit6":Bool,"bit7":Bool,"bit8":Bool,"bit9":Bool,"bit10":Bool,"bit11":Bool,"bit12":Bool,"bit13":Bool,"bit14":Bool,"bit15":Bool,"bit16":Bool,"bit17":Bool,"bit18":Bool,"bit19":Bool,"bit20":Bool,"bit21":Bool,"bit22":Bool,"bit23":Bool,"bit24":Bool,"bit25":Bool,"bit26":Bool,"bit27":Bool,"bit28":Bool,"bit29":Bool,"bit30":Bool,"bit31":Bool)
    ),
    ex!(
        "combine_u32_bits",
        ins!("bit0":Bool,"bit1":Bool,"bit2":Bool,"bit3":Bool,"bit4":Bool,"bit5":Bool,"bit6":Bool,"bit7":Bool,"bit8":Bool,"bit9":Bool,"bit10":Bool,"bit11":Bool,"bit12":Bool,"bit13":Bool,"bit14":Bool,"bit15":Bool,"bit16":Bool,"bit17":Bool,"bit18":Bool,"bit19":Bool,"bit20":Bool,"bit21":Bool,"bit22":Bool,"bit23":Bool,"bit24":Bool,"bit25":Bool,"bit26":Bool,"bit27":Bool,"bit28":Bool,"bit29":Bool,"bit30":Bool,"bit31":Bool),
        outs!("value":U32)
    ),
    ex!(
        "separate_local_aabb",
        ins!("value":LocalAabb),
        outs!("min":Vec3,"max":Vec3)
    ),
    c!(
        "fullscreen_copy",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::Copy)
    ),
    c!(
        "color_balance",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!(
        "exposure_contrast",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!(
        "saturation",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!(
        "channel_mixer",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!(
        "bloom_extract",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::BloomExtract)
    ),
    c!(
        "bloom_blur",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!(
        "bloom_composite",
        1,
        Render,
        BLOOM_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::BloomComposite)
    ),
    c!(
        "luminance_edge",
        1,
        Render,
        COPY_I,
        COLOR_O,
        false,
        Some(FullscreenPolicy::HdrSameExtent)
    ),
    c!("frame_out", 3, Frame, FRAME_I, NONE_O, true, None),
];
pub fn contract(key: &str) -> Option<&'static Contract> {
    CONTRACTS.iter().find(|c| c.key == key)
}

static AUTHORED_RENDER_PIPELINE: Contract = c!(
    "authored_render_pipeline",
    2,
    Render,
    RASTER_I,
    RASTER_O,
    false,
    None
);

/// Resolve static core executors or a render pipeline declared by this graph.
pub fn contract_for(
    key: &str,
    pipelines: &crate::render_graph::PipelineDeclarations,
) -> Option<&'static Contract> {
    contract(key).or_else(|| {
        pipelines
            .render
            .iter()
            .any(|pipeline| pipeline.name == key)
            .then_some(&AUTHORED_RENDER_PIPELINE)
    })
}
