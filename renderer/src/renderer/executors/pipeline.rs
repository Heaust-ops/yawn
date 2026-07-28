use crate::renderer::{
    gpu_scene::GpuSceneCache, material::MaterialResources, ActiveCompiledGraph, PipelineLibrary,
    PreparedExecution,
};

use super::super::scene::Scene;

fn encode_scene<'a, T: Scene>(
    pass: &mut wgpu::RenderPass<'a>,
    scene: &'a T,
    gpu: &'a GpuSceneCache,
    pipelines: &'a PipelineLibrary,
    materials: &'a MaterialResources,
) {
    for (i, bind_group) in scene.bind_groups().iter().enumerate() {
        pass.set_bind_group(i as u32, bind_group, &[]);
    }
    if let (Some(p), Some(n), Some(u), Some(t), Some(i), Some(inst)) = (
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
        pass.set_vertex_buffer(3, inst.slice(..));
        pass.set_vertex_buffer(4, t.slice(..));
        pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
        for draw in &gpu.draws {
            if !draw.effective_visible {
                continue;
            }
            pass.set_pipeline(pipelines.get_pipeline(draw.pipeline));
            if pipelines.requires_material(draw.pipeline) {
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

pub(crate) fn encode_compiled<T: Scene>(
    encoder: &mut wgpu::CommandEncoder,
    surface: &wgpu::TextureView,
    active: &ActiveCompiledGraph,
    scene: &T,
    gpu: &GpuSceneCache,
    pipelines: &PipelineLibrary,
    materials: &MaterialResources,
    mut profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) -> Result<(), &'static str> {
    use crate::render_graph::{ExecutionKind, NormalizedColorLoad, NormalizedDepthLoad, StoreOp};
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
    for (execution_index, prepared) in active.executions.iter().enumerate() {
        let profile_id = &active.graph.executions[execution_index].id;
        match prepared {
            PreparedExecution::PipelineRegistry => {}
            PreparedExecution::FrustumCull => {
                gpu.encode_frustum_cull(encoder, profile.as_deref_mut(), profile_id);
            }
            PreparedExecution::MeshQuery => {
                gpu.encode_mesh_query(encoder, profile.as_deref_mut(), profile_id);
            }
            PreparedExecution::Fullscreen {
                execution,
                frame_out,
                bind_group,
                pipeline,
                ..
            } => {
                let execution = active
                    .graph
                    .executions
                    .get(*execution)
                    .ok_or(" execution out of bounds")?;
                let (target, operations) = if *frame_out {
                    let ExecutionKind::FrameOut { .. } = execution.kind else {
                        return Err("frame_out kind mismatch");
                    };
                    (
                        surface,
                        wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    )
                } else {
                    let ExecutionKind::Render {
                        color_attachments, ..
                    } = &execution.kind
                    else {
                        return Err("fullscreen is not render");
                    };
                    let color = color_attachments
                        .first()
                        .ok_or("fullscreen target missing")?;
                    (
                        view(color.resource)?,
                        wgpu::Operations {
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
                    )
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&execution.id),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: operations,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: profile
                        .as_deref_mut()
                        .and_then(|p| p.render_writes(&execution.id)),
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            PreparedExecution::Pipeline {
                execution,
                base,
                variant,
            } => {
                let execution = active
                    .graph
                    .executions
                    .get(*execution)
                    .ok_or(" execution out of bounds")?;
                let ExecutionKind::Render {
                    color_attachments,
                    depth_stencil,
                } = &execution.kind
                else {
                    return Err("pipeline is not render");
                };
                let color = color_attachments.first().ok_or("pipeline color missing")?;
                let depth = depth_stencil.as_ref().ok_or("pipeline depth missing")?;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&execution.id),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: view(color.resource)?,
                        depth_slice: None,
                        resolve_target: None,
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
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: view(depth.resource)?,
                        depth_ops: Some(wgpu::Operations {
                            load: match depth.load {
                                NormalizedDepthLoad::Load => wgpu::LoadOp::Load,
                                NormalizedDepthLoad::Clear { value } => wgpu::LoadOp::Clear(value),
                            },
                            store: if depth.store == StoreOp::Store {
                                wgpu::StoreOp::Store
                            } else {
                                wgpu::StoreOp::Discard
                            },
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: profile
                        .as_deref_mut()
                        .and_then(|p| p.render_writes(&execution.id)),
                });
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
                    for draw in &gpu.draws {
                        if draw.pipeline != *base {
                            continue;
                        }
                        let slot = draw.instances.start as u64;
                        let start = slot
                            * std::mem::size_of::<crate::renderer::gpu_scene::GpuInstance>() as u64;
                        pass.set_vertex_buffer(3, inst.slice(start..start + 112));
                        pass.set_pipeline(variant);
                        if pipelines.requires_material(draw.pipeline) {
                            pass.set_bind_group(2, materials.group(draw.material), &[]);
                        }
                        pass.draw_indexed_indirect(
                            gpu.indirect_commands
                                .buffer
                                .as_ref()
                                .ok_or("indirect command buffer missing")?,
                            slot * 20,
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn encode_immediate<T: Scene>(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    scene: &T,
    gpu: &GpuSceneCache,
    pipelines: &PipelineLibrary,
    materials: &MaterialResources,
    profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Immediate pipeline pass"),
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
        timestamp_writes: profile.and_then(|p| p.render_writes("immediate.pipeline")),
    });
    encode_scene(&mut pass, scene, gpu, pipelines, materials);
}
