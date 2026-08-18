use crate::renderer::{
    gpu_scene::GpuSceneCache, material::MaterialResources, ActiveCompiledGraph, PipelineLibrary,
    PreparedExecution,
};

use super::super::scene::Scene;

pub(crate) fn encode_compiled<T: Scene>(
    encoder: &mut wgpu::CommandEncoder,
    surface: &wgpu::TextureView,
    active: &ActiveCompiledGraph,
    scene: &T,
    gpu: &GpuSceneCache,
    pipelines: &PipelineLibrary,
    materials: &MaterialResources,
    planes: Option<&[[f32; 4]; 6]>,
    mut profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) -> Result<(), &'static str> {
    use crate::render_graph::{NormalizedColorLoad, NormalizedDepthLoad, StoreOp};
    for compute in &active.compute {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&compute.name),
            timestamp_writes: profile
                .as_deref_mut()
                .and_then(|profile| profile.compute_writes(&compute.name)),
        });
        pass.set_pipeline(&compute.pipeline);
        pass.dispatch_workgroups(
            compute.dispatch[0],
            compute.dispatch[1],
            compute.dispatch[2],
        );
    }
    let view = |resource: u32| -> Result<&wgpu::TextureView, &'static str> {
        let a = active
            .runtime
            .allocations
            .resource_allocations
            .get(resource as usize)
            .copied()
            .flatten()
            .ok_or(" resource has no allocation")?;
        active
            .textures
            .get(a.class as usize)
            .and_then(|c| c.get(a.slot as usize))
            .map(|s| &s.view)
            .ok_or(" allocation out of bounds")
    };
    for physical in &active.runtime.render_passes {
        let first = *physical.executions.first().ok_or("empty physical pass")? as usize;
        let last = *physical.executions.last().ok_or("empty physical pass")? as usize;
        let label = if first == last {
            active
                .graph
                .executions
                .get(first)
                .ok_or("execution out of bounds")?
                .id
                .clone()
        } else {
            format!(
                "{}..{}",
                active
                    .graph
                    .executions
                    .get(first)
                    .ok_or("execution out of bounds")?
                    .id,
                active
                    .graph
                    .executions
                    .get(last)
                    .ok_or("execution out of bounds")?
                    .id
            )
        };
        let (colors, depth) = match &physical.kind {
            crate::render_graph::PhysicalRenderPassKind::Surface => (
                vec![Some(wgpu::RenderPassColorAttachment {
                    view: surface,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                None,
            ),
            crate::render_graph::PhysicalRenderPassKind::Texture {
                color_attachments,
                depth_stencil,
            } => {
                let colors = color_attachments
                    .iter()
                    .map(|color| {
                        Ok(Some(wgpu::RenderPassColorAttachment {
                            view: view(color.resource)?,
                            depth_slice: None,
                            resolve_target: color.resolve_target.map(view).transpose()?,
                            ops: wgpu::Operations {
                                load: match color.load {
                                    NormalizedColorLoad::Load => wgpu::LoadOp::Load,
                                    NormalizedColorLoad::Clear { value } => {
                                        wgpu::LoadOp::Clear(wgpu::Color {
                                            r: value[0],
                                            g: value[1],
                                            b: value[2],
                                            a: value[3],
                                        })
                                    }
                                },
                                store: if color.store == StoreOp::Store {
                                    wgpu::StoreOp::Store
                                } else {
                                    wgpu::StoreOp::Discard
                                },
                            },
                        }))
                    })
                    .collect::<Result<Vec<_>, &'static str>>()?;
                let depth = depth_stencil
                    .as_ref()
                    .map(|depth| -> Result<_, &'static str> {
                        Ok(wgpu::RenderPassDepthStencilAttachment {
                            view: view(depth.resource)?,
                            depth_ops: Some(wgpu::Operations {
                                load: match depth.load {
                                    NormalizedDepthLoad::Load => wgpu::LoadOp::Load,
                                    NormalizedDepthLoad::Clear { value } => {
                                        wgpu::LoadOp::Clear(value)
                                    }
                                },
                                store: if depth.store == StoreOp::Store {
                                    wgpu::StoreOp::Store
                                } else {
                                    wgpu::StoreOp::Discard
                                },
                            }),
                            stencil_ops: None,
                        })
                    })
                    .transpose()?;
                (colors, depth)
            }
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&label),
            color_attachments: &colors,
            depth_stencil_attachment: depth,
            occlusion_query_set: None,
            timestamp_writes: profile.as_deref_mut().and_then(|p| p.render_writes(&label)),
        });
        for &member in &physical.executions {
            match active
                .executions
                .get(member as usize)
                .ok_or("prepared execution out of bounds")?
            {
                PreparedExecution::Fullscreen {
                    bind_group,
                    pipeline,
                    ..
                } => {
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }
                PreparedExecution::Pipeline {
                    base,
                    predicate,
                    variant,
                    ..
                } => {
                    let traversal = active
                        .runtime
                        .instance_traversal
                        .as_ref()
                        .ok_or("compiled graph instance traversal missing")?;
                    for (i, group) in scene.bind_groups().iter().enumerate() {
                        pass.set_bind_group(i as u32, group, &[]);
                    }
                    if let (Some(p), Some(n), Some(u), Some(t), Some(ix), Some(inst)) = (
                        &gpu.positions.buffer,
                        &gpu.normals.buffer,
                        &gpu.uvs.buffer,
                        &gpu.tangents.buffer,
                        &gpu.indices.buffer,
                        &gpu.instances.buffer,
                    ) {
                        pass.set_vertex_buffer(0, p.slice(..));
                        pass.set_vertex_buffer(1, n.slice(..));
                        pass.set_vertex_buffer(2, u.slice(..));
                        pass.set_vertex_buffer(4, t.slice(..));
                        pass.set_index_buffer(ix.slice(..), wgpu::IndexFormat::Uint32);
                        pass.set_vertex_buffer(3, inst.slice(..));
                        for (draw_index, draw) in gpu.draws.iter().enumerate() {
                            if !crate::renderer::instance_filter::evaluate(
                                traversal,
                                *predicate,
                                gpu.instance_records
                                    .get(draw_index)
                                    .ok_or("instance record missing")?,
                                gpu.local_aabb_records
                                    .get(draw_index)
                                    .ok_or("local aabb record missing")?,
                                *gpu.instance_type_records
                                    .get(draw_index)
                                    .ok_or("instance type record missing")?,
                                planes,
                            )? {
                                continue;
                            }
                            pass.set_pipeline(variant);
                            if pipelines.requires_material(*base) {
                                pass.set_bind_group(2, materials.group(draw.material), &[]);
                            }
                            pass.draw_indexed(
                                draw.indices.clone(),
                                draw.base_vertex,
                                draw.instances.clone(),
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn encode_immediate(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("No active render graph"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view: color,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.,
                    g: 0.,
                    b: 0.,
                    a: 1.,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        occlusion_query_set: None,
        timestamp_writes: profile.and_then(|p| p.render_writes("no-active-graph")),
    });
}
