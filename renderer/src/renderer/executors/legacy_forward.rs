use crate::renderer::{
    gpu_scene::GpuSceneCache, material::MaterialResources, ActiveCompiledV1, ActiveCompiledV2,
    PipelineLibrary, PreparedExecutionV2,
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

pub(crate) fn encode_compiled_v2<T: Scene>(
    encoder: &mut wgpu::CommandEncoder,
    surface: &wgpu::TextureView,
    active: &ActiveCompiledV2,
    scene: &T,
    gpu: &GpuSceneCache,
    pipelines: &PipelineLibrary,
    materials: &MaterialResources,
    mut profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) -> Result<(), &'static str> {
    use crate::render_graph::{
        ExecutionKindV2, NormalizedColorLoadV2, NormalizedDepthLoadV2, ResourcePlanV2, StoreOpV2,
    };
    let view = |resource: u32| -> Result<&wgpu::TextureView, &'static str> {
        let is_surface = active
            .graph
            .resources
            .get(resource as usize)
            .is_some_and(|resource| {
                matches!(
                    resource.plan,
                    ResourcePlanV2::SurfaceTarget { family }
                        | ResourcePlanV2::Texture { family, .. }
                        if family == active.runtime.allocations.surface_family
                )
            });
        if is_surface {
            return Ok(surface);
        }
        let a = active
            .runtime
            .allocations
            .resource_allocations
            .get(resource as usize)
            .copied()
            .flatten()
            .ok_or("V2 resource has no allocation")?;
        active
            .textures
            .get(a.class as usize)
            .and_then(|c| c.get(a.slot as usize))
            .map(|s| &s.view)
            .ok_or("V2 allocation out of bounds")
    };
    for (execution_index, prepared) in active.executions.iter().enumerate() {
        let profile_id = &active.graph.executions[execution_index].id;
        match prepared {
            PreparedExecutionV2::FrustumCull => {
                gpu.encode_frustum_cull(encoder, profile.as_deref_mut(), profile_id);
            }
            PreparedExecutionV2::MeshQuery => {
                gpu.encode_mesh_query(encoder, profile.as_deref_mut(), profile_id);
            }
            PreparedExecutionV2::Present => {}
            PreparedExecutionV2::Fullscreen {
                execution,
                bind_group,
                pipeline,
                ..
            } => {
                let execution = active
                    .graph
                    .executions
                    .get(*execution)
                    .ok_or("V2 execution out of bounds")?;
                let ExecutionKindV2::Render {
                    color_attachments, ..
                } = &execution.kind
                else {
                    return Err("fullscreen is not render");
                };
                let color = color_attachments
                    .first()
                    .ok_or("fullscreen target missing")?;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&execution.id),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: view(color.resource)?,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: match color.load {
                                NormalizedColorLoadV2::Load => wgpu::LoadOp::Load,
                                NormalizedColorLoadV2::Clear { value } => {
                                    wgpu::LoadOp::Clear(wgpu::Color {
                                        r: value[0],
                                        g: value[1],
                                        b: value[2],
                                        a: value[3],
                                    })
                                }
                            },
                            store: if color.store == StoreOpV2::Store {
                                wgpu::StoreOp::Store
                            } else {
                                wgpu::StoreOp::Discard
                            },
                        },
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
            PreparedExecutionV2::LegacyForward {
                execution,
                variants,
            } => {
                let execution = active
                    .graph
                    .executions
                    .get(*execution)
                    .ok_or("V2 execution out of bounds")?;
                let ExecutionKindV2::Render {
                    color_attachments,
                    depth_stencil,
                } = &execution.kind
                else {
                    return Err("legacy forward is not render");
                };
                let color = color_attachments.first().ok_or("legacy color missing")?;
                let depth = depth_stencil.as_ref().ok_or("legacy depth missing")?;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&execution.id),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: view(color.resource)?,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: match color.load {
                                NormalizedColorLoadV2::Load => wgpu::LoadOp::Load,
                                NormalizedColorLoadV2::Clear { value } => {
                                    wgpu::LoadOp::Clear(wgpu::Color {
                                        r: value[0],
                                        g: value[1],
                                        b: value[2],
                                        a: value[3],
                                    })
                                }
                            },
                            store: if color.store == StoreOpV2::Store {
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
                                NormalizedDepthLoadV2::Load => wgpu::LoadOp::Load,
                                NormalizedDepthLoadV2::Clear { value } => {
                                    wgpu::LoadOp::Clear(value)
                                }
                            },
                            store: if depth.store == StoreOpV2::Store {
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
                        let slot = draw.instances.start as u64;
                        let start = slot
                            * std::mem::size_of::<crate::renderer::gpu_scene::GpuInstance>() as u64;
                        pass.set_vertex_buffer(3, inst.slice(start..start + 112));
                        let key = variants
                            .iter()
                            .find(|(base, _)| *base == draw.pipeline)
                            .map(|x| &x.1)
                            .ok_or("pipeline variant missing")?;
                        pass.set_pipeline(key);
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
        label: Some("Render pass"),
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
        timestamp_writes: profile.and_then(|p| p.render_writes("immediate.forward")),
    });
    encode_scene(&mut pass, scene, gpu, pipelines, materials);
}

pub(crate) fn encode_compiled_v1<T: Scene>(
    encoder: &mut wgpu::CommandEncoder,
    color_view: &wgpu::TextureView,
    active: &ActiveCompiledV1,
    scene: &T,
    gpu: &GpuSceneCache,
    pipelines: &PipelineLibrary,
    materials: &MaterialResources,
    mut profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
) {
    for pass in &active.graph.passes {
        let depth_resource = pass
            .writes
            .iter()
            .find(|w| w.binding == "depth")
            .unwrap()
            .resource as usize;
        let allocation = active.graph.resources[depth_resource].allocation.unwrap();
        let depth_view =
            &active.views[active.class_bases[allocation.class as usize] + allocation.slot as usize];
        let color = pass.writes.iter().find(|w| w.binding == "color").unwrap();
        let depth = pass.writes.iter().find(|w| w.binding == "depth").unwrap();
        let (color_load, color_store) = match &color.access {
            crate::render_graph::WriteAccess::ColorAttachment { load, store, .. } => (
                match load {
                    crate::render_graph::ColorLoad::Clear { value } => {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: value[0],
                            g: value[1],
                            b: value[2],
                            a: value[3],
                        })
                    }
                    crate::render_graph::ColorLoad::Load => wgpu::LoadOp::Load,
                },
                if matches!(store, crate::render_graph::StoreOp::Store) {
                    wgpu::StoreOp::Store
                } else {
                    wgpu::StoreOp::Discard
                },
            ),
            _ => unreachable!(),
        };
        let (depth_load, depth_store) = match &depth.access {
            crate::render_graph::WriteAccess::DepthAttachment { load, store } => (
                match load {
                    crate::render_graph::DepthLoad::Clear { value } => wgpu::LoadOp::Clear(*value),
                    crate::render_graph::DepthLoad::Load => wgpu::LoadOp::Load,
                },
                if matches!(store, crate::render_graph::StoreOp::Store) {
                    wgpu::StoreOp::Store
                } else {
                    wgpu::StoreOp::Discard
                },
            ),
            _ => unreachable!(),
        };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&pass.id),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: color_store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: depth_load,
                    store: depth_store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: profile
                .as_deref_mut()
                .and_then(|p| p.render_writes(&pass.id)),
        });
        encode_scene(&mut render_pass, scene, gpu, pipelines, materials);
    }
}
