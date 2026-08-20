import initWasm, { Core as WasmCore } from "../pkg/yawn_core.js";

let canvas, context, device, surfaceFormat, memory, core, loadout;
const arrays = new Map();
const align = (value, multiple) => Math.ceil(value / multiple) * multiple;
const fail = code => { throw new Error(code); };
const list = value => value === undefined ? [] : Array.isArray(value) ? value : fail("GRAPH_ARRAY");

function index(items, code) {
  const result = new Map();
  for (const item of list(items)) {
    if (!item || typeof item.id !== "string" || result.has(item.id)) fail(code);
    result.set(item.id, item);
  }
  return result;
}

const bufferUsage = names => list(names).reduce((usage, name) => usage | ({
  uniform: GPUBufferUsage.UNIFORM, storage: GPUBufferUsage.STORAGE,
  vertex: GPUBufferUsage.VERTEX, index: GPUBufferUsage.INDEX,
  indirect: GPUBufferUsage.INDIRECT, copySrc: GPUBufferUsage.COPY_SRC,
}[name] ?? fail("GRAPH_BUFFER_USAGE")), GPUBufferUsage.COPY_DST);
const textureUsage = names => list(names).reduce((usage, name) => usage | ({
  render: GPUTextureUsage.RENDER_ATTACHMENT, sampled: GPUTextureUsage.TEXTURE_BINDING,
  storage: GPUTextureUsage.STORAGE_BINDING, copySrc: GPUTextureUsage.COPY_SRC,
  copyDst: GPUTextureUsage.COPY_DST,
}[name] ?? fail("GRAPH_TEXTURE_USAGE")), 0);

async function compile(graph) {
  if (!graph || typeof graph.id !== "string") fail("GRAPH_SHAPE");
  const passes = list(graph.passes);
  const renderDeclarations = index(graph.pipelines?.render, "GRAPH_PIPELINE");
  const computeDeclarations = index(graph.pipelines?.compute, "GRAPH_PIPELINE");
  const resources = new Map(), owned = [];
  try {
    for (const declaration of list(graph.resources?.buffers)) {
      const source = arrays.get(declaration.array);
      if (!source) fail("GRAPH_ARRAY_UNKNOWN");
      const gpu = device.createBuffer({ size: align(source.bytes, 4), usage: bufferUsage(declaration.usage) });
      resources.set(declaration.id, { kind: "buffer", gpu, source });
      owned.push(gpu);
    }

    const textureSlots = new Map();
    for (const declaration of list(graph.resources?.textures)) {
      if (!Number.isInteger(declaration.slot)) fail("GRAPH_RESOURCE");
      const size = declaration.size ?? ["canvas", "canvas"];
      if (!Array.isArray(size) || size.length < 2 || size.length > 3) fail("GRAPH_TEXTURE_SIZE");
      const descriptor = {
        size: [size[0] === "canvas" ? canvas.width : size[0], size[1] === "canvas" ? canvas.height : size[1], size[2] ?? 1],
        format: declaration.format,
        usage: textureUsage(declaration.usage),
        mipLevelCount: declaration.mipLevelCount ?? 1,
        sampleCount: declaration.sampleCount ?? 1,
        dimension: declaration.dimension ?? "2d",
      };
      let slot = textureSlots.get(declaration.slot);
      if (!slot) {
        const gpu = device.createTexture(descriptor);
        slot = { gpu, view: gpu.createView() };
        textureSlots.set(declaration.slot, slot);
        owned.push(gpu);
      }
      resources.set(declaration.id, { kind: "texture", gpu: slot.gpu, view: slot.view });
    }
    for (const declaration of list(graph.resources?.samplers))
      resources.set(declaration.id, {
        kind: "sampler", gpu: device.createSampler(declaration.descriptor),
      });

    const renderPipelines = new Map(), computePipelines = new Map();
    await Promise.all([...new Set(passes.filter(x => x.type === "render").map(x => x.pipeline))].map(async id => {
      const declaration = renderDeclarations.get(id);
      if (typeof declaration?.code !== "string") fail("GRAPH_PIPELINE");
      const module = device.createShaderModule({ code: declaration.code });
      renderPipelines.set(id, await device.createRenderPipelineAsync({
        layout: "auto",
        vertex: { module, entryPoint: declaration.vertex?.entry ?? "vertex", buffers: declaration.vertex?.buffers ?? [] },
        fragment: {
          module,
          entryPoint: declaration.fragment?.entry ?? "fragment",
          targets: list(declaration.fragment?.targets).map(target => ({
            ...target, format: target.format === "canvas" ? surfaceFormat : target.format,
          })),
        },
        primitive: declaration.primitive,
        depthStencil: declaration.depthStencil,
        multisample: declaration.multisample,
      }));
    }));
    await Promise.all([...new Set(passes.filter(x => x.type === "compute").map(x => x.pipeline))].map(async id => {
      const declaration = computeDeclarations.get(id);
      if (typeof declaration?.code !== "string") fail("GRAPH_PIPELINE");
      const module = device.createShaderModule({ code: declaration.code });
      computePipelines.set(id, await device.createComputePipelineAsync({
        layout: "auto", compute: { module, entryPoint: declaration.entry ?? "main" },
      }));
    }));

    const bindGroups = (pass, pipeline) => {
      const groups = new Map();
      for (const binding of list(pass.bindings)) {
        const resource = resources.get(binding.resource);
        if (!resource) fail("GRAPH_RESOURCE_UNKNOWN");
        const value = resource.kind === "buffer"
          ? { buffer: resource.gpu, offset: binding.offset ?? 0, ...(binding.size ? { size: binding.size } : {}) }
          : resource.kind === "texture" ? resource.view : resource.gpu;
        if (!groups.has(binding.group ?? 0)) groups.set(binding.group ?? 0, []);
        groups.get(binding.group ?? 0).push({ binding: binding.binding, resource: value });
      }
      return [...groups].map(([group, entries]) => [group, device.createBindGroup({
        layout: pipeline.getBindGroupLayout(group), entries,
      })]);
    };
    const compiled = passes.map(pass => {
      const pipeline = (pass.type === "render" ? renderPipelines : computePipelines).get(pass.pipeline);
      if (!pipeline) fail("GRAPH_PASS");
      return { pass, pipeline, bindGroups: bindGroups(pass, pipeline) };
    });
    return { id: graph.id, passes: compiled, resources, owned };
  } catch (error) {
    owned.forEach(resource => resource.destroy?.());
    throw error;
  }
}

const clearColor = (value = [0, 0, 0, 1]) => Array.isArray(value)
  ? { r: value[0], g: value[1], b: value[2], a: value[3] }
  : value;

function render() {
  if (!loadout) return;
  for (const resource of loadout.resources.values())
    if (resource.kind === "buffer") device.queue.writeBuffer(
      resource.gpu, 0, new Uint8Array(memory, resource.source.offset, resource.source.bytes),
    );
  const encoder = device.createCommandEncoder();
  for (const { pass, pipeline, bindGroups } of loadout.passes) {
    if (pass.type === "compute") {
      const command = encoder.beginComputePass();
      command.setPipeline(pipeline);
      bindGroups.forEach(([group, value]) => command.setBindGroup(group, value));
      command.dispatchWorkgroups(...(pass.dispatch ?? [1, 1, 1]));
      command.end();
      continue;
    }
    const view = id => id === "canvas"
      ? context.getCurrentTexture().createView()
      : loadout.resources.get(id)?.view ?? fail("GRAPH_ATTACHMENT");
    const command = encoder.beginRenderPass({
      colorAttachments: list(pass.color).map(attachment => ({
        view: view(attachment.resource), loadOp: attachment.load ?? "clear",
        storeOp: attachment.store ?? "store", clearValue: clearColor(attachment.clear),
      })),
      ...(pass.depth ? { depthStencilAttachment: {
        view: view(pass.depth.resource), depthLoadOp: pass.depth.load ?? "clear",
        depthStoreOp: pass.depth.store ?? "store", depthClearValue: pass.depth.clear ?? 1,
      } } : {}),
    });
    command.setPipeline(pipeline);
    bindGroups.forEach(([group, value]) => command.setBindGroup(group, value));
    list(pass.vertexBuffers).forEach(binding => command.setVertexBuffer(
      binding.slot ?? 0, loadout.resources.get(binding.resource)?.gpu ?? fail("GRAPH_RESOURCE_UNKNOWN"), binding.offset ?? 0,
    ));
    if (pass.indexBuffer) command.setIndexBuffer(
      loadout.resources.get(pass.indexBuffer.resource)?.gpu ?? fail("GRAPH_RESOURCE_UNKNOWN"),
      pass.indexBuffer.format ?? "uint32", pass.indexBuffer.offset ?? 0,
    );
    const draw = pass.draw ?? {};
    if (pass.indexBuffer) command.drawIndexed(draw.indices ?? 0, draw.instances ?? 1, draw.firstIndex ?? 0, draw.baseVertex ?? 0, draw.firstInstance ?? 0);
    else command.draw(draw.vertices ?? 3, draw.instances ?? 1, draw.firstVertex ?? 0, draw.firstInstance ?? 0);
    command.end();
  }
  device.queue.submit([encoder.finish()]);
}

function tick() {
  try { render(); } catch (error) {
    postMessage({ type: "runtime-error", error: error?.message ?? "RENDER_ERROR" });
    loadout = undefined;
  }
  (globalThis.requestAnimationFrame ?? (callback => setTimeout(callback, 16)))(tick);
}

addEventListener("message", async ({ data: message }) => {
  try {
    let result;
    if (message.type === "init") {
      if (!(message.canvas instanceof OffscreenCanvas) || !Number.isInteger(message.arenaBytes) || message.arenaBytes < 64) fail("INIT");
      canvas = message.canvas;
      const wasm = await initWasm();
      core = new WasmCore(message.arenaBytes);
      memory = wasm.memory.buffer;
      if (!(memory instanceof SharedArrayBuffer)) fail("WASM_MEMORY_NOT_SHARED");
      const adapter = await navigator.gpu?.requestAdapter();
      if (!adapter) fail("WEBGPU_UNAVAILABLE");
      device = await adapter.requestDevice();
      context = canvas.getContext("webgpu");
      surfaceFormat = navigator.gpu.getPreferredCanvasFormat();
      context.configure({ device, format: surfaceFormat, alphaMode: "opaque" });
      device.lost.then(() => { postMessage({ type: "runtime-error", error: "DEVICE_LOST" }); loadout = undefined; });
      result = { buffer: memory };
      tick();
    } else if (message.type === "allocate") {
      const { name, rows, stride, format } = message;
      if (typeof name !== "string" || !name || arrays.has(name)) fail("ALLOCATION");
      const offset = core.allocate(rows, stride, format), bytes = rows * stride;
      result = { name, rows, stride, format, offset };
      arrays.set(name, { ...result, bytes });
    } else if (message.type === "load-graph") {
      const next = await compile(JSON.parse(core.compile_graph(message.serialized)));
      const previous = loadout;
      loadout = next;
      previous?.owned.forEach(resource => resource.destroy?.());
      result = { id: next.id };
    } else fail("MESSAGE");
    postMessage({ request: message.request, result });
  } catch (error) {
    postMessage({ request: message.request, error: error?.message ?? "CORE_ERROR" });
  }
});
