let canvas, context, device, surfaceFormat, memory, used = 0, loadout;
const arrays = new Map();
const align = (value, multiple) => Math.ceil(value / multiple) * multiple;
const fail = code => { throw new Error(code); };
const list = value => value === undefined ? [] : Array.isArray(value) ? value : fail("GRAPH_ARRAY");

function parse(source) {
  if (typeof source !== "string") fail("GRAPH_WIRE");
  const tokens = source.match(/\s*(\(|\)|"(?:\\.|[^"\\])*"|[^\s()]+)/gu) ?? [];
  let at = 0;
  const read = () => {
    const token = tokens[at++]?.trim();
    if (token === "(") {
      const value = [];
      while (tokens[at]?.trim() !== ")") {
        if (at >= tokens.length) fail("GRAPH_WIRE");
        value.push(read());
      }
      at++;
      return value;
    }
    if (!token || token === ")") fail("GRAPH_WIRE");
    if (token[0] === '"') return JSON.parse(token);
    if (token === "true") return true;
    if (token === "false") return false;
    if (token === "null") return null;
    return Number.isFinite(Number(token)) ? Number(token) : token;
  };
  const root = read();
  if (at !== tokens.length || root[0] !== "yawn-graph" || root[1] !== 1) fail("GRAPH_WIRE");
  const decode = value => {
    if (!Array.isArray(value)) return value;
    if (value[0] === "array") return value.slice(1).map(decode);
    if (value[0] === "object") return Object.fromEntries(value.slice(1).map(field => {
      if (field[0] !== "field" || field.length !== 3) fail("GRAPH_WIRE");
      return [field[1], decode(field[2])];
    }));
    fail("GRAPH_WIRE");
  };
  return decode(root[2]);
}

function index(items, code) {
  const result = new Map();
  for (const item of list(items)) {
    if (!item || typeof item.id !== "string" || result.has(item.id)) fail(code);
    result.set(item.id, item);
  }
  return result;
}

function sortPasses(passes) {
  const byId = index(passes, "GRAPH_PASS");
  const waiting = new Map([...byId].map(([id, pass]) => [id, new Set(list(pass.after))]));
  for (const dependencies of waiting.values())
    for (const dependency of dependencies) if (!byId.has(dependency)) fail("GRAPH_DEPENDENCY");
  const result = [];
  while (waiting.size) {
    const ready = [...waiting].find(([, dependencies]) => !dependencies.size);
    if (!ready) fail("GRAPH_CYCLE");
    waiting.delete(ready[0]);
    result.push(byId.get(ready[0]));
    for (const dependencies of waiting.values()) dependencies.delete(ready[0]);
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
  const passes = sortPasses(graph.passes);
  const renderDeclarations = index(graph.pipelines?.render, "GRAPH_PIPELINE");
  const computeDeclarations = index(graph.pipelines?.compute, "GRAPH_PIPELINE");
  const resources = new Map(), owned = [];
  try {
    const usedResources = new Set(passes.flatMap(pass => [
      ...list(pass.bindings).map(x => x.resource),
      ...list(pass.color).map(x => x.resource),
      ...(pass.depth ? [pass.depth.resource] : []),
      ...list(pass.vertexBuffers).map(x => x.resource),
      ...(pass.indexBuffer ? [pass.indexBuffer.resource] : []),
    ]));
    for (const declaration of list(graph.resources?.buffers)) {
      if (!usedResources.has(declaration.id)) continue;
      const source = arrays.get(declaration.array);
      if (!source) fail("GRAPH_ARRAY_UNKNOWN");
      const gpu = device.createBuffer({ size: align(source.bytes, 4), usage: bufferUsage(declaration.usage) });
      resources.set(declaration.id, { kind: "buffer", gpu, source });
      owned.push(gpu);
    }

    const textures = index(graph.resources?.textures, "GRAPH_RESOURCE"), lifetimes = new Map(), slots = [];
    passes.forEach((pass, frame) => {
      for (const id of [...list(pass.bindings).map(x => x.resource), ...list(pass.color).map(x => x.resource), ...(pass.depth ? [pass.depth.resource] : [])]) {
        if (!textures.has(id)) continue;
        const lifetime = lifetimes.get(id) ?? [frame, frame];
        lifetime[1] = frame;
        lifetimes.set(id, lifetime);
      }
    });
    for (const declaration of textures.values()) {
      const lifetime = lifetimes.get(declaration.id);
      if (!lifetime) continue;
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
      const key = JSON.stringify(descriptor);
      let slot = declaration.transient === false ? undefined : slots.find(value => value.key === key && value.last < lifetime[0]);
      if (!slot) {
        const gpu = device.createTexture(descriptor);
        slot = { key, last: lifetime[1], gpu, view: gpu.createView() };
        slots.push(slot);
        owned.push(gpu);
      } else slot.last = lifetime[1];
      resources.set(declaration.id, { kind: "texture", gpu: slot.gpu, view: slot.view });
    }
    for (const declaration of list(graph.resources?.samplers))
      if (usedResources.has(declaration.id)) resources.set(declaration.id, {
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
      memory = new SharedArrayBuffer(align(message.arenaBytes, 64));
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
      if (typeof name !== "string" || !name || arrays.has(name) || !Number.isInteger(rows) || rows < 1 ||
          !Number.isInteger(stride) || stride < 16 || stride % 16 || !["f32", "u32", "i32"].includes(format)) fail("ALLOCATION");
      const offset = align(used, 64), bytes = rows * stride;
      if (!Number.isSafeInteger(bytes) || offset + bytes > memory.byteLength) fail("ARENA_OOM");
      result = { name, rows, stride, format, offset };
      arrays.set(name, { ...result, bytes });
      used = offset + bytes;
    } else if (message.type === "load-graph") {
      const next = await compile(parse(message.serialized));
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
