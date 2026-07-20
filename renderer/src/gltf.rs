use std::collections::{HashMap, HashSet};

use gltf::Gltf;
use gltf::Semantic;
use ultraviolet::Mat4;

use crate::render_data::{canonical_affine_transform, Geometry};

pub const MAX_SOURCE_BYTES: usize = 384 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 768 * 1024 * 1024;
const MAX_NODE_DEPTH: usize = 4096;
const MAX_REACHABLE_NODES: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct ImportedInstance {
    pub geometry: usize,
    pub world_transform: Mat4,
}

#[derive(Clone, Debug, Default)]
pub struct ImportedScene {
    pub geometries: Vec<Geometry>,
    pub instances: Vec<ImportedInstance>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("failed to decode glTF: {0}")]
    GltfParse(#[from] gltf::Error),
    #[error("GLB has no binary buffer")]
    MissingBinaryBuffer,
    #[error("unsupported external glTF buffer")]
    ExternalBuffer,
    #[error("mesh {mesh} primitive {primitive} uses unsupported mode {mode:?}; only Triangles is supported")]
    UnsupportedPrimitiveMode {
        mesh: usize,
        primitive: usize,
        mode: gltf::mesh::Mode,
    },
    #[error("mesh {mesh} primitive {primitive} is missing the POSITION attribute")]
    MissingPositions { mesh: usize, primitive: usize },
    #[error("mesh {mesh} primitive {primitive} has a non-finite {attribute} value")]
    NonFiniteAttribute {
        mesh: usize,
        primitive: usize,
        attribute: &'static str,
    },
    #[error(
        "mesh {mesh} primitive {primitive} has {count} indices, which is not triangle-aligned"
    )]
    UnalignedIndices {
        mesh: usize,
        primitive: usize,
        count: usize,
    },
    #[error("mesh {mesh} primitive {primitive} index {index} is out of bounds for {positions} positions")]
    IndexOutOfBounds {
        mesh: usize,
        primitive: usize,
        index: u32,
        positions: usize,
    },
    #[error("mesh {mesh} primitive {primitive} has too many positions to generate u32 indices")]
    TooManyPositions { mesh: usize, primitive: usize },
    #[error("mesh {mesh} primitive {primitive} has {attribute_count} {attribute} values for {position_count} positions")]
    AttributeCountMismatch {
        mesh: usize,
        primitive: usize,
        attribute: &'static str,
        attribute_count: usize,
        position_count: usize,
    },
    #[error("node {node} uses unsupported skinning")]
    UnsupportedSkin { node: usize },
    #[error("mesh {mesh} primitive {primitive} uses unsupported morph targets")]
    UnsupportedMorphTargets { mesh: usize, primitive: usize },
    #[error("node {node} has a non-finite transform")]
    NonFiniteTransform { node: usize },
    #[error("node {node} has a singular transform")]
    SingularTransform { node: usize },
    #[error(
        "node {node} has a mirrored transform, which is unsupported by the fixed winding pipeline"
    )]
    MirroredTransform { node: usize },
    #[error("source exceeds the {MAX_SOURCE_BYTES} byte import limit")]
    SourceBudget,
    #[error("decoded geometry exceeds the {MAX_DECODED_BYTES} byte import limit")]
    DecodedBudget,
    #[error("node graph contains a cycle, excessive depth, or a node with multiple parents")]
    InvalidNodeGraph,
}

/// Pure CPU glTF/GLB decoding. Vertex attributes remain in primitive-local space.
pub fn import_bytes(bytes: &[u8]) -> Result<ImportedScene, ImportError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ImportError::SourceBudget);
    }
    let model = Gltf::from_slice(bytes)?;
    // Reject impossible canonical allocations before any accessor reader collects.
    let mut planned = 0usize;
    for mesh in model.meshes() {
        for primitive in mesh.primitives() {
            let positions = primitive
                .get(&Semantic::Positions)
                .ok_or(ImportError::MissingPositions {
                    mesh: mesh.index(),
                    primitive: primitive.index(),
                })?
                .count();
            let normals = primitive
                .get(&Semantic::Normals)
                .map_or(positions, |a| a.count());
            let uvs = primitive
                .get(&Semantic::TexCoords(0))
                .map_or(positions, |a| a.count());
            let indices = primitive.indices().map_or(positions, |a| a.count());
            let bytes = positions
                .checked_mul(12)
                .and_then(|n| n.checked_add(normals.checked_mul(12)?))
                .and_then(|n| n.checked_add(uvs.checked_mul(8)?))
                .and_then(|n| n.checked_add(indices.checked_mul(4)?))
                .ok_or(ImportError::DecodedBudget)?;
            planned = planned
                .checked_add(bytes)
                .filter(|total| *total <= MAX_DECODED_BYTES)
                .ok_or(ImportError::DecodedBudget)?;
        }
    }
    if model
        .buffers()
        .any(|buffer| matches!(buffer.source(), gltf::buffer::Source::Uri(_)))
    {
        return Err(ImportError::ExternalBuffer);
    }
    let blob = model
        .blob
        .as_deref()
        .ok_or(ImportError::MissingBinaryBuffer)?;
    let mut output = ImportedScene::default();
    let mut geometry_map = HashMap::new();
    let scene = model.default_scene().or_else(|| model.scenes().next());
    if let Some(scene) = scene {
        let mut stack: Vec<_> = scene
            .nodes()
            .map(|node| (node, Mat4::identity(), 0))
            .collect();
        let mut visited = HashSet::new();
        while let Some((node, parent, depth)) = stack.pop() {
            if depth > MAX_NODE_DEPTH
                || visited.len() >= MAX_REACHABLE_NODES
                || !visited.insert(node.index())
            {
                return Err(ImportError::InvalidNodeGraph);
            }
            let world = canonical_affine_transform(parent * Mat4::from(node.transform().matrix()))
                .ok_or(ImportError::SingularTransform { node: node.index() })?;
            visit_node(node.clone(), world, blob, &mut geometry_map, &mut output)?;
            for child in node.children() {
                stack.push((child, world, depth + 1));
            }
        }
    }
    Ok(output)
}

fn visit_node<'a>(
    node: gltf::Node<'a>,
    world: Mat4,
    blob: &[u8],
    geometry_map: &mut HashMap<(usize, usize), usize>,
    output: &mut ImportedScene,
) -> Result<(), ImportError> {
    let determinant = world.determinant();
    if !determinant.is_finite() {
        return Err(ImportError::NonFiniteTransform { node: node.index() });
    }
    if determinant == 0.0 {
        return Err(ImportError::SingularTransform { node: node.index() });
    }
    if node.skin().is_some() {
        return Err(ImportError::UnsupportedSkin { node: node.index() });
    }
    if let Some(mesh) = node.mesh() {
        if determinant < 0.0 {
            return Err(ImportError::MirroredTransform { node: node.index() });
        }
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(ImportError::UnsupportedPrimitiveMode {
                    mesh: mesh.index(),
                    primitive: primitive.index(),
                    mode: primitive.mode(),
                });
            }
            if primitive.morph_targets().next().is_some() {
                return Err(ImportError::UnsupportedMorphTargets {
                    mesh: mesh.index(),
                    primitive: primitive.index(),
                });
            }
            let key = (mesh.index(), primitive.index());
            let geometry = if let Some(index) = geometry_map.get(&key) {
                *index
            } else {
                let reader = primitive.reader(|buffer| match buffer.source() {
                    gltf::buffer::Source::Bin => Some(blob),
                    gltf::buffer::Source::Uri(_) => None,
                });
                let positions: Vec<[f32; 3]> = reader
                    .read_positions()
                    .map(Iterator::collect)
                    .ok_or(ImportError::MissingPositions {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                    })?;
                if !positions.iter().flatten().all(|value| value.is_finite()) {
                    return Err(ImportError::NonFiniteAttribute {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        attribute: "POSITION",
                    });
                }
                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(Iterator::collect)
                    .map_or_else(Vec::new, |values| values);
                if !normals.iter().flatten().all(|value| value.is_finite()) {
                    return Err(ImportError::NonFiniteAttribute {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        attribute: "NORMAL",
                    });
                }
                if !normals.is_empty() && normals.len() != positions.len() {
                    return Err(ImportError::AttributeCountMismatch {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        attribute: "NORMAL",
                        attribute_count: normals.len(),
                        position_count: positions.len(),
                    });
                }
                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|v| v.into_f32().collect())
                    .map_or_else(Vec::new, |values| values);
                if !uvs.iter().flatten().all(|value| value.is_finite()) {
                    return Err(ImportError::NonFiniteAttribute {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        attribute: "TEXCOORD_0",
                    });
                }
                if !uvs.is_empty() && uvs.len() != positions.len() {
                    return Err(ImportError::AttributeCountMismatch {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        attribute: "TEXCOORD_0",
                        attribute_count: uvs.len(),
                        position_count: positions.len(),
                    });
                }
                let indices: Vec<u32> = if let Some(values) = reader.read_indices() {
                    values.into_u32().collect()
                } else {
                    let count = u32::try_from(positions.len()).map_err(|_| {
                        ImportError::TooManyPositions {
                            mesh: mesh.index(),
                            primitive: primitive.index(),
                        }
                    })?;
                    (0..count).collect()
                };
                let decoded = positions
                    .len()
                    .checked_mul(12)
                    .and_then(|n| n.checked_add(normals.len().checked_mul(12)?))
                    .and_then(|n| n.checked_add(uvs.len().checked_mul(8)?))
                    .and_then(|n| n.checked_add(indices.len().checked_mul(4)?))
                    .ok_or(ImportError::DecodedBudget)?;
                let admitted: usize = output
                    .geometries
                    .iter()
                    .try_fold(0usize, |sum, geometry| {
                        sum.checked_add(geometry.positions.len().checked_mul(12)?)
                            .and_then(|n| n.checked_add(geometry.normals.len().checked_mul(12)?))
                            .and_then(|n| n.checked_add(geometry.uvs.len().checked_mul(8)?))
                            .and_then(|n| n.checked_add(geometry.indices.len().checked_mul(4)?))
                    })
                    .ok_or(ImportError::DecodedBudget)?;
                if admitted
                    .checked_add(decoded)
                    .filter(|total| *total <= MAX_DECODED_BYTES)
                    .is_none()
                {
                    return Err(ImportError::DecodedBudget);
                }
                if indices.len() % 3 != 0 {
                    return Err(ImportError::UnalignedIndices {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        count: indices.len(),
                    });
                }
                if let Some(index) = indices
                    .iter()
                    .copied()
                    .find(|index| (*index as usize) >= positions.len())
                {
                    return Err(ImportError::IndexOutOfBounds {
                        mesh: mesh.index(),
                        primitive: primitive.index(),
                        index,
                        positions: positions.len(),
                    });
                }
                let index = output.geometries.len();
                output
                    .geometries
                    .push(Geometry::new(positions, normals, uvs, indices));
                geometry_map.insert(key, index);
                index
            };
            output.instances.push(ImportedInstance {
                geometry,
                world_transform: world,
            });
        }
    }
    Ok(())
}
