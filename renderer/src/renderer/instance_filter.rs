//! CPU evaluation of graph-owned instance predicates.
//!
//! Shader source belongs to graph packages, so core evaluates its small typed
//! predicate IR directly instead of manufacturing a hidden compute shader.

use crate::render_graph::{
    BooleanOp, CompareOp, ExprId, ExpressionOp, InstanceTraversalPlan, TypedLiteral,
};

use super::gpu_scene::{GpuInstance, GpuLocalAabb};

#[derive(Clone, Debug)]
enum Value {
    Bool(bool),
    F32(f32),
    U32(u32),
    Vector(Vec<f32>),
    Matrix(Vec<Vec<f32>>),
    Type([u32; 16]),
    Aabb { min: [f32; 3], max: [f32; 3] },
}

fn literal(value: &TypedLiteral) -> Value {
    match value {
        TypedLiteral::Bool(value) => Value::Bool(*value),
        TypedLiteral::F32(value) => Value::F32(*value),
        TypedLiteral::U32(value) => Value::U32(*value),
        TypedLiteral::Vec2(value) => Value::Vector(value.to_vec()),
        TypedLiteral::Vec3(value) => Value::Vector(value.to_vec()),
        TypedLiteral::Vec4(value) => Value::Vector(value.to_vec()),
        TypedLiteral::Mat2(value) => {
            Value::Matrix(value.iter().map(|column| column.to_vec()).collect())
        }
        TypedLiteral::Mat3(value) => {
            Value::Matrix(value.iter().map(|column| column.to_vec()).collect())
        }
        TypedLiteral::Mat4(value) => {
            Value::Matrix(value.iter().map(|column| column.to_vec()).collect())
        }
        TypedLiteral::U32x16(value) => Value::Type(*value),
        TypedLiteral::LocalAabb { min, max } => Value::Aabb {
            min: *min,
            max: *max,
        },
    }
}

fn value<'a>(values: &'a [Value], id: ExprId) -> Result<&'a Value, &'static str> {
    values.get(id.0 as usize).ok_or("predicate operand missing")
}

fn boolean(values: &[Value], id: ExprId) -> Result<bool, &'static str> {
    match value(values, id)? {
        Value::Bool(value) => Ok(*value),
        _ => Err("predicate operand is not bool"),
    }
}

fn f32_value(values: &[Value], id: ExprId) -> Result<f32, &'static str> {
    match value(values, id)? {
        Value::F32(value) => Ok(*value),
        _ => Err("predicate operand is not f32"),
    }
}

fn u32_value(values: &[Value], id: ExprId) -> Result<u32, &'static str> {
    match value(values, id)? {
        Value::U32(value) => Ok(*value),
        _ => Err("predicate operand is not u32"),
    }
}

fn compare<T: PartialEq + PartialOrd>(operation: CompareOp, left: T, right: T) -> bool {
    match operation {
        CompareOp::GreaterThan => left > right,
        CompareOp::LessThan => left < right,
        CompareOp::Equals => left == right,
    }
}

fn transformed(model: &[[f32; 4]; 4], point: [f32; 3]) -> [f32; 4] {
    std::array::from_fn(|row| {
        model[0][row] * point[0]
            + model[1][row] * point[1]
            + model[2][row] * point[2]
            + model[3][row]
    })
}

fn frustum_culled(
    bounds: ([f32; 3], [f32; 3]),
    model: &[[f32; 4]; 4],
    planes: &[[f32; 4]; 6],
) -> bool {
    planes.iter().any(|plane| {
        (0..8).all(|corner| {
            let local = std::array::from_fn(|axis| {
                if corner & (1 << axis) == 0 {
                    bounds.0[axis]
                } else {
                    bounds.1[axis]
                }
            });
            let world = transformed(model, local);
            plane
                .iter()
                .zip(world)
                .map(|(left, right)| left * right)
                .sum::<f32>()
                < 0.0
        })
    })
}

/// Evaluates one compiled raster predicate for one dense scene occurrence.
pub fn evaluate(
    plan: &InstanceTraversalPlan,
    predicate: ExprId,
    instance: &GpuInstance,
    local_aabb: &GpuLocalAabb,
    instance_type: [u32; 16],
    planes: Option<&[[f32; 4]; 6]>,
) -> Result<bool, &'static str> {
    let mut values = Vec::with_capacity(plan.expressions.expressions.len());
    for expression in &plan.expressions.expressions {
        let result = match &expression.op {
            ExpressionOp::Literal { literal: item } => literal(item),
            ExpressionOp::InstanceType { .. } => Value::Type(instance_type),
            ExpressionOp::LocalAabb { .. } => Value::Aabb {
                min: local_aabb.min[..3].try_into().unwrap(),
                max: local_aabb.max[..3].try_into().unwrap(),
            },
            ExpressionOp::Not { value: operand } => Value::Bool(!boolean(&values, *operand)?),
            ExpressionOp::Boolean {
                operation,
                operands,
            } => {
                let operands = operands
                    .iter()
                    .map(|operand| boolean(&values, *operand))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::Bool(match operation {
                    BooleanOp::And => operands.into_iter().all(|item| item),
                    BooleanOp::Or => operands.into_iter().any(|item| item),
                    BooleanOp::Xor => operands.into_iter().fold(false, |left, right| left ^ right),
                    BooleanOp::Xnor => {
                        !operands.into_iter().fold(false, |left, right| left ^ right)
                    }
                })
            }
            ExpressionOp::CompareF32 {
                operation,
                left,
                right,
            } => Value::Bool(compare(
                *operation,
                f32_value(&values, *left)?,
                f32_value(&values, *right)?,
            )),
            ExpressionOp::CompareU32 {
                operation,
                left,
                right,
            } => Value::Bool(compare(
                *operation,
                u32_value(&values, *left)?,
                u32_value(&values, *right)?,
            )),
            ExpressionOp::VectorProject { vector, index } => match value(&values, *vector)? {
                Value::Vector(vector) => Value::F32(
                    *vector
                        .get(*index as usize)
                        .ok_or("vector predicate index out of bounds")?,
                ),
                _ => return Err("predicate operand is not vector"),
            },
            ExpressionOp::VectorConstruct { components } => Value::Vector(
                components
                    .iter()
                    .map(|component| f32_value(&values, *component))
                    .collect::<Result<_, _>>()?,
            ),
            ExpressionOp::MatrixColumn { matrix, index } => match value(&values, *matrix)? {
                Value::Matrix(matrix) => Value::Vector(
                    matrix
                        .get(*index as usize)
                        .ok_or("matrix predicate index out of bounds")?
                        .clone(),
                ),
                _ => return Err("predicate operand is not matrix"),
            },
            ExpressionOp::MatrixConstruct { columns } => Value::Matrix(
                columns
                    .iter()
                    .map(|column| match value(&values, *column)? {
                        Value::Vector(column) => Ok(column.clone()),
                        _ => Err("matrix column is not vector"),
                    })
                    .collect::<Result<_, _>>()?,
            ),
            ExpressionOp::TypeWord {
                value: operand,
                index,
            } => match value(&values, *operand)? {
                Value::Type(words) => Value::U32(words[*index as usize]),
                _ => return Err("predicate operand is not u32x16"),
            },
            ExpressionOp::TypeConstruct { words } => {
                if words.len() != 16 {
                    return Err("type predicate requires 16 words");
                }
                let mut result = [0; 16];
                for (index, word) in words.iter().enumerate() {
                    result[index] = u32_value(&values, *word)?;
                }
                Value::Type(result)
            }
            ExpressionOp::U32Bit {
                value: operand,
                index,
            } => Value::Bool(u32_value(&values, *operand)? & (1 << index) != 0),
            ExpressionOp::U32Construct { bits } => {
                if bits.len() > 32 {
                    return Err("u32 predicate has too many bits");
                }
                let mut result = 0;
                for (index, bit) in bits.iter().enumerate() {
                    result |= u32::from(boolean(&values, *bit)?) << index;
                }
                Value::U32(result)
            }
            ExpressionOp::AabbMin { aabb } => match value(&values, *aabb)? {
                Value::Aabb { min, .. } => Value::Vector(min.to_vec()),
                _ => return Err("predicate operand is not aabb"),
            },
            ExpressionOp::AabbMax { aabb } => match value(&values, *aabb)? {
                Value::Aabb { max, .. } => Value::Vector(max.to_vec()),
                _ => return Err("predicate operand is not aabb"),
            },
            ExpressionOp::FrustumCulled { local_aabb, .. } => {
                let bounds = match value(&values, *local_aabb)? {
                    Value::Aabb { min, max } => (*min, *max),
                    _ => return Err("predicate operand is not aabb"),
                };
                let planes = planes.ok_or("camera frustum missing")?;
                Value::Bool(frustum_culled(bounds, &instance.model, planes))
            }
        };
        values.push(result);
    }
    boolean(&values, predicate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{
        Expression, ExpressionPlan, NodeOutputRef, PipelinePredicatePlan, SemanticType,
    };

    fn origin() -> NodeOutputRef {
        NodeOutputRef {
            node: "test".into(),
            socket: "value".into(),
        }
    }

    #[test]
    fn evaluates_type_bits_without_shader_source() {
        let plan = InstanceTraversalPlan {
            mesh: 0,
            expressions: ExpressionPlan {
                expressions: vec![
                    Expression {
                        semantic_type: SemanticType::U32x16,
                        op: ExpressionOp::InstanceType { mesh: 0 },
                        origin: origin(),
                        mesh_provenance: Some(0),
                    },
                    Expression {
                        semantic_type: SemanticType::U32,
                        op: ExpressionOp::TypeWord {
                            value: ExprId(0),
                            index: 0,
                        },
                        origin: origin(),
                        mesh_provenance: Some(0),
                    },
                    Expression {
                        semantic_type: SemanticType::Bool,
                        op: ExpressionOp::U32Bit {
                            value: ExprId(1),
                            index: 3,
                        },
                        origin: origin(),
                        mesh_provenance: Some(0),
                    },
                ],
            },
            pipelines: vec![PipelinePredicatePlan {
                execution: 0,
                predicate: ExprId(2),
                ordinal: 0,
            }],
            requires_camera: false,
        };
        let mut words = [0; 16];
        words[0] = 8;
        assert!(evaluate(
            &plan,
            ExprId(2),
            &GpuInstance {
                model: crate::render_data::IDENTITY_MODEL_TRANSFORM,
                normal_0: [0.; 4],
                normal_1: [0.; 4],
                normal_2: [0.; 4],
            },
            &GpuLocalAabb {
                min: [-1., -1., -1., 0.],
                max: [1., 1., 1., 0.],
            },
            words,
            None,
        )
        .unwrap());
    }
}
