use std::collections::BTreeMap;

use super::{
    CompiledGraph, Dimension, Extent, ExternalSource, Format, GraphError, Residency,
    TextureAllocationKey, TextureUsage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResolvedExtent {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuntimeTextureKey {
    pub dimension: Dimension,
    pub format: Format,
    pub extent: ResolvedExtent,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub usage: Vec<TextureUsage>,
    pub view_formats: Vec<Format>,
}

fn scaled(value: u32, numerator: u32, denominator: u32) -> Result<u32, GraphError> {
    if denominator == 0 {
        return Err(GraphError::new(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "zero extent denominator",
        ));
    }
    let product = u64::from(value)
        .checked_mul(u64::from(numerator))
        .ok_or_else(|| GraphError::new("GRAPH_EXECUTION_UNSUPPORTED", "extent overflow"))?;
    let result = product
        .checked_add(u64::from(denominator) - 1)
        .ok_or_else(|| GraphError::new("GRAPH_EXECUTION_UNSUPPORTED", "extent overflow"))?
        / u64::from(denominator);
    u32::try_from(result.max(1))
        .map_err(|_| GraphError::new("GRAPH_EXECUTION_UNSUPPORTED", "extent overflow"))
}

pub fn resolve_extent(extent: &Extent, surface: [u32; 2]) -> Result<ResolvedExtent, GraphError> {
    let (width, height, depth_or_array_layers) = match extent {
        Extent::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => (*width, *height, *depth_or_array_layers),
        Extent::SurfaceRelative {
            width,
            height,
            depth_or_array_layers,
        } => (
            scaled(surface[0], width.numerator, width.denominator)?,
            scaled(surface[1], height.numerator, height.denominator)?,
            *depth_or_array_layers,
        ),
    };
    if width == 0 || height == 0 || depth_or_array_layers == 0 {
        return Err(GraphError::new(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "texture extent must be nonzero",
        ));
    }
    Ok(ResolvedExtent {
        width,
        height,
        depth_or_array_layers,
    })
}

pub fn runtime_texture_key(
    key: &TextureAllocationKey,
    surface: [u32; 2],
) -> Result<RuntimeTextureKey, GraphError> {
    Ok(RuntimeTextureKey {
        dimension: key.descriptor.dimension,
        format: key.descriptor.format,
        extent: resolve_extent(&key.descriptor.extent, surface)?,
        mip_level_count: key.descriptor.mip_level_count,
        sample_count: key.descriptor.sample_count,
        usage: key.usage.clone(),
        view_formats: key.view_formats.clone(),
    })
}

/// Assigns disjoint physical ranges after merging symbolic allocation classes that
/// resolve to the same concrete descriptor key.
pub fn class_offsets(
    classes: &[(TextureAllocationKey, u32)],
    surface: [u32; 2],
) -> Result<Vec<u32>, GraphError> {
    let mut next = BTreeMap::new();
    let mut offsets = Vec::with_capacity(classes.len());
    for (key, count) in classes {
        let concrete = runtime_texture_key(key, surface)?;
        let offset = next.entry(concrete).or_insert(0u32);
        offsets.push(*offset);
        *offset = offset.checked_add(*count).ok_or_else(|| {
            GraphError::new("GRAPH_EXECUTION_UNSUPPORTED", "transient slot overflow")
        })?;
    }
    Ok(offsets)
}

pub fn validate_activatable(graph: &CompiledGraph) -> Result<(), GraphError> {
    let unsupported = || {
        GraphError::new(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "graph is outside the activatable Phase 6 subset",
        )
    };
    if graph.passes.is_empty() || graph.outputs.is_empty() {
        return Err(unsupported());
    }
    let surface_outputs = graph
        .outputs
        .iter()
        .filter(|o| {
            matches!(
                graph.resources[o.resource as usize].residency,
                Residency::External {
                    source: ExternalSource::SurfaceColor
                }
            )
        })
        .count();
    if surface_outputs == 0 {
        return Err(unsupported());
    }
    for pass in &graph.passes {
        if pass.executor.key != "scene_forward"
            || pass.executor.version != 1
            || !pass.reads.is_empty()
        {
            return Err(unsupported());
        }
        let color = pass
            .writes
            .iter()
            .find(|w| w.binding == "color")
            .ok_or_else(&unsupported)?;
        let depth = pass
            .writes
            .iter()
            .find(|w| w.binding == "depth")
            .ok_or_else(&unsupported)?;
        let c = &graph.resources[color.resource as usize];
        let d = &graph.resources[depth.resource as usize];
        if !matches!(
            c.residency,
            Residency::External {
                source: ExternalSource::SurfaceColor
            }
        ) || !matches!(d.residency, Residency::Transient)
            || d.descriptor.format != Format::Depth32Float
            || d.descriptor.dimension != Dimension::D2
            || d.descriptor.mip_level_count != 1
            || d.descriptor.sample_count != 1
            || d.descriptor.extent != c.descriptor.extent
        {
            return Err(unsupported());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{Dimension, Ratio, TextureDescriptor, TextureUsage};
    fn key(n: u32, d: u32) -> TextureAllocationKey {
        TextureAllocationKey {
            descriptor: TextureDescriptor {
                dimension: Dimension::D2,
                format: Format::Depth32Float,
                extent: Extent::SurfaceRelative {
                    width: Ratio {
                        numerator: n,
                        denominator: d,
                    },
                    height: Ratio {
                        numerator: n,
                        denominator: d,
                    },
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
            },
            usage: vec![TextureUsage::DepthAttachment],
            view_formats: vec![],
        }
    }
    #[test]
    fn extent_uses_checked_ceil_and_minimum_one() {
        assert_eq!(
            resolve_extent(&key(1, 2).descriptor.extent, [3, 1]).unwrap(),
            ResolvedExtent {
                width: 2,
                height: 1,
                depth_or_array_layers: 1
            }
        );
    }
    #[test]
    fn equivalent_symbolic_classes_are_disjoint() {
        assert_eq!(
            class_offsets(&[(key(1, 2), 2), (key(2, 4), 3)], [100, 100]).unwrap(),
            vec![0, 2]
        );
    }
}
