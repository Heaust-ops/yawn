//! Typed, device-independent instance predicate IR.

use serde::Serialize;

use super::{NodeOutputRef, SemanticType};

pub const MAX_EXPRESSIONS: usize = 4096;
pub const MAX_PREDICATE_PIPELINES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ExprId(pub u32);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TypedLiteral {
    Bool(bool),
    F32(f32),
    U32(u32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Mat2([[f32; 2]; 2]),
    Mat3([[f32; 3]; 3]),
    Mat4([[f32; 4]; 4]),
    U32x16([u32; 16]),
    LocalAabb { min: [f32; 3], max: [f32; 3] },
}

impl TypedLiteral {
    pub fn semantic_type(&self) -> SemanticType {
        match self {
            Self::Bool(_) => SemanticType::Bool,
            Self::F32(_) => SemanticType::F32,
            Self::U32(_) => SemanticType::U32,
            Self::Vec2(_) => SemanticType::Vec2,
            Self::Vec3(_) => SemanticType::Vec3,
            Self::Vec4(_) => SemanticType::Vec4,
            Self::Mat2(_) => SemanticType::Mat2,
            Self::Mat3(_) => SemanticType::Mat3,
            Self::Mat4(_) => SemanticType::Mat4,
            Self::U32x16(_) => SemanticType::U32x16,
            Self::LocalAabb { .. } => SemanticType::LocalAabb,
        }
    }

    pub fn is_finite(&self) -> bool {
        let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
        match self {
            Self::F32(value) => value.is_finite(),
            Self::Vec2(value) => finite(value),
            Self::Vec3(value) => finite(value),
            Self::Vec4(value) => finite(value),
            Self::Mat2(value) => value.iter().all(|column| finite(column)),
            Self::Mat3(value) => value.iter().all(|column| finite(column)),
            Self::Mat4(value) => value.iter().all(|column| finite(column)),
            Self::LocalAabb { min, max } => finite(min) && finite(max),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    GreaterThan,
    LessThan,
    Equals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanBinaryOp {
    And,
    Or,
    Xor,
    Xnor,
}

/// All operand IDs refer to earlier entries in [`ExpressionPlan::expressions`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ExpressionOp {
    Literal {
        literal: TypedLiteral,
    },
    InstanceType {
        mesh: u32,
    },
    LocalAabb {
        mesh: u32,
    },
    Not {
        value: ExprId,
    },
    BooleanBinary {
        operation: BooleanBinaryOp,
        left: ExprId,
        right: ExprId,
    },
    CompareF32 {
        operation: CompareOp,
        left: ExprId,
        right: ExprId,
    },
    CompareU32 {
        operation: CompareOp,
        left: ExprId,
        right: ExprId,
    },
    VectorProject {
        vector: ExprId,
        index: u8,
    },
    VectorConstruct {
        components: Vec<ExprId>,
    },
    MatrixColumn {
        matrix: ExprId,
        index: u8,
    },
    MatrixConstruct {
        columns: Vec<ExprId>,
    },
    TypeWord {
        value: ExprId,
        index: u8,
    },
    TypeConstruct {
        words: Vec<ExprId>,
    },
    U32Bit {
        value: ExprId,
        index: u8,
    },
    U32Construct {
        bits: Vec<ExprId>,
    },
    AabbMin {
        aabb: ExprId,
    },
    AabbMax {
        aabb: ExprId,
    },
    FrustumCulled {
        mesh: u32,
        local_aabb: ExprId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expression {
    pub semantic_type: SemanticType,
    pub op: ExpressionOp,
    pub origin: NodeOutputRef,
    pub mesh_provenance: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionPlan {
    pub expressions: Vec<Expression>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelinePredicatePlan {
    pub execution: u32,
    pub predicate: ExprId,
    pub ordinal: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceTraversalPlan {
    pub mesh: u32,
    pub expressions: ExpressionPlan,
    pub pipelines: Vec<PipelinePredicatePlan>,
    pub requires_camera: bool,
}
