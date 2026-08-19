const GLB_MAGIC = 0x46546c67;
const JSON_CHUNK = 0x4e4f534a;
const BIN_CHUNK = 0x004e4942;
const PACKET_MAGIC = 0x50445259;
const PACKET_VERSION = 1;
const COMPONENT_WIDTH = Object.freeze({ SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT2: 4, MAT3: 9, MAT4: 16 });
const COMPONENT_SIZE = Object.freeze({ 5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4 });
const IDENTITY = Object.freeze([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
const decoder = new TextDecoder();
const encoder = new TextEncoder();

const fail = code => { throw new Error(code); };
const align4 = value => (value + 3) & ~3;
const finite = values => values.every(Number.isFinite);

function parseContainer(source) {
  if (!(source instanceof Uint8Array) || !source.byteLength) fail("GLTF_EMPTY");
  const view = new DataView(source.buffer, source.byteOffset, source.byteLength);
  if (source.byteLength >= 12 && view.getUint32(0, true) === GLB_MAGIC) {
    if (view.getUint32(4, true) !== 2 || view.getUint32(8, true) !== source.byteLength)
      fail("GLTF_INVALID_CONTAINER");
    let offset = 12, json, binary;
    while (offset < source.byteLength) {
      if (offset + 8 > source.byteLength) fail("GLTF_INVALID_CONTAINER");
      const length = view.getUint32(offset, true);
      const type = view.getUint32(offset + 4, true);
      const end = offset + 8 + length;
      if (end > source.byteLength) fail("GLTF_INVALID_CONTAINER");
      const chunk = source.subarray(offset + 8, end);
      if (type === JSON_CHUNK && !json) json = chunk;
      if (type === BIN_CHUNK && !binary) binary = chunk;
      offset = end;
    }
    if (!json) fail("GLTF_JSON_MISSING");
    return { document: JSON.parse(decoder.decode(json).replace(/\0+$/u, "").trimEnd()), binary };
  }
  return { document: JSON.parse(decoder.decode(source).replace(/^\uFEFF/u, "")), binary: undefined };
}

async function fetchBytes(uri, baseUrl, fetcher) {
  const response = await fetcher(new URL(uri, baseUrl));
  if (!response.ok) fail(`HTTP_${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

async function loadBuffers(document, binary, baseUrl, fetcher) {
  return Promise.all((document.buffers ?? []).map(async (buffer, index) => {
    const bytes = buffer.uri === undefined
      ? (index === 0 ? binary : undefined)
      : await fetchBytes(buffer.uri, baseUrl, fetcher);
    if (!bytes || bytes.byteLength < buffer.byteLength) fail("GLTF_BUFFER_INVALID");
    return bytes;
  }));
}

function component(data, offset, type) {
  switch (type) {
    case 5120: return data.getInt8(offset);
    case 5121: return data.getUint8(offset);
    case 5122: return data.getInt16(offset, true);
    case 5123: return data.getUint16(offset, true);
    case 5125: return data.getUint32(offset, true);
    case 5126: return data.getFloat32(offset, true);
    default: fail("GLTF_ACCESSOR_COMPONENT");
  }
}

function normalizeComponent(value, type) {
  switch (type) {
    case 5120: return Math.max(value / 127, -1);
    case 5121: return value / 255;
    case 5122: return Math.max(value / 32767, -1);
    case 5123: return value / 65535;
    case 5125: return value / 4294967295;
    default: return value;
  }
}

function viewBytes(document, buffers, index) {
  const view = document.bufferViews?.[index];
  const buffer = view && buffers[view.buffer];
  if (!view || !buffer) fail("GLTF_BUFFER_VIEW_INVALID");
  const start = view.byteOffset ?? 0;
  const end = start + view.byteLength;
  if (end > buffer.byteLength) fail("GLTF_BUFFER_VIEW_INVALID");
  return { view, bytes: buffer.subarray(start, end) };
}

function readAccessor(document, buffers, index, { integer = false } = {}) {
  const accessor = document.accessors?.[index];
  const width = accessor && COMPONENT_WIDTH[accessor.type];
  const size = accessor && COMPONENT_SIZE[accessor.componentType];
  if (!accessor || !width || !size || !Number.isInteger(accessor.count) || accessor.count < 0)
    fail("GLTF_ACCESSOR_INVALID");
  const values = integer ? new Uint32Array(accessor.count * width) : new Float32Array(accessor.count * width);
  if (accessor.bufferView !== undefined) {
    const { view, bytes } = viewBytes(document, buffers, accessor.bufferView);
    const stride = view.byteStride ?? width * size;
    const start = accessor.byteOffset ?? 0;
    if (stride < width * size || start + Math.max(0, accessor.count - 1) * stride + width * size > bytes.byteLength)
      fail("GLTF_ACCESSOR_RANGE");
    const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    for (let item = 0; item < accessor.count; item++) {
      for (let lane = 0; lane < width; lane++) {
        let value = component(data, start + item * stride + lane * size, accessor.componentType);
        if (!integer && accessor.normalized) value = normalizeComponent(value, accessor.componentType);
        values[item * width + lane] = value;
      }
    }
  }
  if (accessor.sparse) {
    const sparse = accessor.sparse;
    const indices = sparse.indices;
    const indexSize = COMPONENT_SIZE[indices.componentType];
    if (!indexSize || ![5121, 5123, 5125].includes(indices.componentType)) fail("GLTF_SPARSE_INVALID");
    const indexView = viewBytes(document, buffers, indices.bufferView).bytes;
    const valueView = viewBytes(document, buffers, sparse.values.bufferView).bytes;
    const indexStart = indices.byteOffset ?? 0;
    const valueStart = sparse.values.byteOffset ?? 0;
    if (indexStart + sparse.count * indexSize > indexView.byteLength || valueStart + sparse.count * width * size > valueView.byteLength)
      fail("GLTF_SPARSE_INVALID");
    const indexData = new DataView(indexView.buffer, indexView.byteOffset, indexView.byteLength);
    const valueData = new DataView(valueView.buffer, valueView.byteOffset, valueView.byteLength);
    for (let item = 0; item < sparse.count; item++) {
      const target = component(indexData, indexStart + item * indexSize, indices.componentType);
      if (target >= accessor.count) fail("GLTF_SPARSE_INVALID");
      for (let lane = 0; lane < width; lane++) {
        let value = component(valueData, valueStart + (item * width + lane) * size, accessor.componentType);
        if (!integer && accessor.normalized) value = normalizeComponent(value, accessor.componentType);
        values[target * width + lane] = value;
      }
    }
  }
  if (!finite(values)) fail("GLTF_ACCESSOR_NONFINITE");
  return { count: accessor.count, width, values };
}

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
function normalize(value, fallback) {
  const length = Math.hypot(...value);
  return length > Number.EPSILON && Number.isFinite(length) ? value.map(item => item / length) : fallback;
}
function lanes(values, index, width) { return Array.from(values.subarray(index * width, (index + 1) * width)); }

function repairGeometry(positions, normals, tangents, uvs, indices) {
  const count = positions.length / 3;
  if (indices.length % 3 || Array.from(indices).some(index => index >= count)) fail("GLTF_TRIANGLES_INVALID");
  const normalsValid = normals?.length === positions.length;
  const tangentsValid = tangents?.length === count * 4;
  if (normalsValid && tangentsValid)
    return { positions, normals, tangents, uvs, indices };
  const outPositions = [], outNormals = [], outTangents = [], outUvs = [];
  for (let triangle = 0; triangle < indices.length; triangle += 3) {
    const ids = [indices[triangle], indices[triangle + 1], indices[triangle + 2]];
    const p = ids.map(index => lanes(positions, index, 3));
    const uv = ids.map(index => lanes(uvs, index, 2));
    const faceNormal = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])), [0, 1, 0]);
    const duv1 = sub([...uv[1], 0], [...uv[0], 0]);
    const duv2 = sub([...uv[2], 0], [...uv[0], 0]);
    const determinant = duv1[0] * duv2[1] - duv1[1] * duv2[0];
    const edge1 = sub(p[1], p[0]), edge2 = sub(p[2], p[0]);
    const rawTangent = Math.abs(determinant) > Number.EPSILON
      ? edge1.map((value, lane) => (value * duv2[1] - edge2[lane] * duv1[1]) / determinant)
      : [0, 0, 0];
    const rawBitangent = Math.abs(determinant) > Number.EPSILON
      ? edge2.map((value, lane) => (value * duv1[0] - edge1[lane] * duv2[0]) / determinant)
      : [0, 0, 0];
    for (let corner = 0; corner < 3; corner++) {
      const normal = normalize(normalsValid ? lanes(normals, ids[corner], 3) : faceNormal, faceNormal);
      const projected = rawTangent.map((value, lane) => value - normal[lane] * dot(normal, rawTangent));
      const axis = Math.abs(normal[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
      const tangent = normalize(projected, normalize(cross(axis, normal), [0, 0, 1]));
      const generated = [...tangent, dot(cross(normal, tangent), rawBitangent) < 0 ? -1 : 1];
      outPositions.push(...p[corner]);
      outNormals.push(...normal);
      outTangents.push(...(tangentsValid ? lanes(tangents, ids[corner], 4) : generated));
      outUvs.push(...uv[corner]);
    }
  }
  const repairedIndices = Uint32Array.from({ length: outPositions.length / 3 }, (_, index) => index);
  return {
    positions: new Float32Array(outPositions),
    normals: new Float32Array(outNormals),
    tangents: new Float32Array(outTangents),
    uvs: new Float32Array(outUvs),
    indices: repairedIndices,
  };
}

function multiply(a, b) {
  const result = Array(16).fill(0);
  for (let column = 0; column < 4; column++)
    for (let row = 0; row < 4; row++)
      for (let lane = 0; lane < 4; lane++)
        result[column * 4 + row] += a[lane * 4 + row] * b[column * 4 + lane];
  return result;
}

function nodeMatrix(node) {
  if (node.matrix) {
    if (node.matrix.length !== 16 || !finite(node.matrix)) fail("GLTF_NODE_TRANSFORM");
    return Array.from(node.matrix);
  }
  const [x, y, z, w] = node.rotation ?? [0, 0, 0, 1];
  const [sx, sy, sz] = node.scale ?? [1, 1, 1];
  const [tx, ty, tz] = node.translation ?? [0, 0, 0];
  const matrix = [
    (1 - 2 * y * y - 2 * z * z) * sx, (2 * x * y + 2 * z * w) * sx, (2 * x * z - 2 * y * w) * sx, 0,
    (2 * x * y - 2 * z * w) * sy, (1 - 2 * x * x - 2 * z * z) * sy, (2 * y * z + 2 * x * w) * sy, 0,
    (2 * x * z + 2 * y * w) * sz, (2 * y * z - 2 * x * w) * sz, (1 - 2 * x * x - 2 * y * y) * sz, 0,
    tx, ty, tz, 1,
  ];
  if (!finite(matrix)) fail("GLTF_NODE_TRANSFORM");
  return matrix;
}

function textureReference(reference) {
  return reference ? { texture: reference.index, texCoord: reference.texCoord ?? 0 } : null;
}

function materialMetadata(material, index) {
  const pbr = material.pbrMetallicRoughness ?? {};
  const ior = material.extensions?.KHR_materials_ior?.ior ?? 1.5;
  if (!Number.isFinite(ior) || (ior !== 0 && ior < 1)) fail("GLTF_MATERIAL_IOR");
  return {
    key: index + 1,
    baseColorFactor: pbr.baseColorFactor ?? [1, 1, 1, 1],
    metallicFactor: pbr.metallicFactor ?? 1,
    roughnessFactor: pbr.roughnessFactor ?? 1,
    emissiveFactor: material.emissiveFactor ?? [0, 0, 0],
    ior,
    alphaMode: (material.alphaMode ?? "OPAQUE").toLowerCase(),
    alphaCutoff: material.alphaCutoff ?? 0.5,
    doubleSided: material.doubleSided ?? false,
    baseColorTexture: textureReference(pbr.baseColorTexture),
    metallicRoughnessTexture: textureReference(pbr.metallicRoughnessTexture),
    normalTexture: textureReference(material.normalTexture),
    normalScale: material.normalTexture?.scale ?? 1,
    occlusionTexture: textureReference(material.occlusionTexture),
    occlusionStrength: material.occlusionTexture?.strength ?? 1,
    emissiveTexture: textureReference(material.emissiveTexture),
  };
}

function samplerMetadata(sampler) {
  const min = sampler.minFilter;
  return {
    magFilter: sampler.magFilter === 9728 ? "nearest" : "linear",
    minFilter: [9728, 9984, 9986].includes(min) ? "nearest" : "linear",
    mipmapFilter: [9984, 9985].includes(min) ? "nearest" : "linear",
    addressU: sampler.wrapS === 33071 ? "clamp_to_edge" : sampler.wrapS === 33648 ? "mirror_repeat" : "repeat",
    addressV: sampler.wrapT === 33071 ? "clamp_to_edge" : sampler.wrapT === 33648 ? "mirror_repeat" : "repeat",
  };
}

function inferMime(image) {
  if (image.mimeType) return image.mimeType;
  const uri = image.uri?.toLowerCase() ?? "";
  if (uri.startsWith("data:image/png") || uri.endsWith(".png")) return "image/png";
  if (uri.startsWith("data:image/jpeg") || /\.jpe?g(?:$|[?#])/u.test(uri)) return "image/jpeg";
  fail("GLTF_IMAGE_MIME");
}

async function decodeScene(document, buffers, baseUrl, fetcher) {
  const geometries = [], occurrences = [], geometryIds = new Map();
  for (let meshIndex = 0; meshIndex < (document.meshes ?? []).length; meshIndex++) {
    const mesh = document.meshes[meshIndex];
    for (let primitiveIndex = 0; primitiveIndex < (mesh.primitives ?? []).length; primitiveIndex++) {
      const primitive = mesh.primitives[primitiveIndex];
      if ((primitive.mode ?? 4) !== 4 || primitive.attributes?.POSITION === undefined) fail("GLTF_TRIANGLES_REQUIRED");
      const position = readAccessor(document, buffers, primitive.attributes.POSITION);
      if (position.width !== 3 || !position.count) fail("GLTF_POSITION_INVALID");
      const normal = primitive.attributes.NORMAL === undefined ? null : readAccessor(document, buffers, primitive.attributes.NORMAL);
      const tangent = primitive.attributes.TANGENT === undefined ? null : readAccessor(document, buffers, primitive.attributes.TANGENT);
      const texcoord = primitive.attributes.TEXCOORD_0 === undefined ? null : readAccessor(document, buffers, primitive.attributes.TEXCOORD_0);
      if (normal && normal.width !== 3 || tangent && tangent.width !== 4 || texcoord && texcoord.width !== 2)
        fail("GLTF_ATTRIBUTE_INVALID");
      const uvs = new Float32Array(position.count * 2);
      if (texcoord) uvs.set(texcoord.values.subarray(0, uvs.length));
      const indexAccessor = primitive.indices === undefined
        ? null
        : readAccessor(document, buffers, primitive.indices, { integer: true });
      if (indexAccessor && indexAccessor.width !== 1) fail("GLTF_INDEX_INVALID");
      const indices = indexAccessor?.values
        ?? Uint32Array.from({ length: position.count }, (_, index) => index);
      const repaired = repairGeometry(position.values, normal?.values, tangent?.values, uvs, indices);
      const id = geometries.length;
      geometryIds.set(`${meshIndex}:${primitiveIndex}`, id);
      const material = primitive.material === undefined ? undefined : document.materials?.[primitive.material];
      geometries.push({
        id,
        material: primitive.material === undefined ? 0 : primitive.material + 1,
        instanceType: [1 | 4 | (material?.doubleSided ? 8 : 0), ...Array(15).fill(0)],
        ...repaired,
      });
    }
  }

  const children = new Set((document.nodes ?? []).flatMap(node => node.children ?? []));
  const scene = document.scenes?.[document.scene ?? 0];
  const roots = scene?.nodes ?? (document.nodes ?? []).map((_, index) => index).filter(index => !children.has(index));
  const active = new Set();
  const visit = (nodeIndex, parent) => {
    const node = document.nodes?.[nodeIndex];
    if (!node || active.has(nodeIndex)) fail("GLTF_NODE_INVALID");
    active.add(nodeIndex);
    const world = multiply(parent, nodeMatrix(node));
    if (node.mesh !== undefined) {
      const mesh = document.meshes?.[node.mesh];
      if (!mesh) fail("GLTF_MESH_INVALID");
      for (let primitive = 0; primitive < mesh.primitives.length; primitive++) {
        const geometry = geometryIds.get(`${node.mesh}:${primitive}`);
        if (geometry !== undefined) occurrences.push({ geometry, transform: world });
      }
    }
    for (const child of node.children ?? []) visit(child, world);
    active.delete(nodeIndex);
  };
  for (const root of roots) visit(root, IDENTITY);

  const images = await Promise.all((document.images ?? []).map(async image => {
    const data = image.bufferView === undefined
      ? await fetchBytes(image.uri, baseUrl, fetcher)
      : viewBytes(document, buffers, image.bufferView).bytes.slice();
    return { mimeType: inferMime(image), bytes: data };
  }));
  return {
    geometries,
    occurrences,
    materials: [materialMetadata({}, -1), ...(document.materials ?? []).map(materialMetadata)],
    textures: (document.textures ?? []).map(texture => ({ image: texture.source, sampler: texture.sampler ?? null })),
    samplers: (document.samplers ?? []).map(samplerMetadata),
    images,
  };
}

function encodePacket(scene) {
  const chunks = [];
  let payloadLength = 0;
  const append = (source, alignment = 4) => {
    const bytes = source instanceof Uint8Array
      ? source
      : new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
    const offset = alignment === 4 ? align4(payloadLength) : payloadLength;
    if (offset > payloadLength) chunks.push({ offset: payloadLength, bytes: new Uint8Array(offset - payloadLength) });
    chunks.push({ offset, bytes });
    payloadLength = offset + bytes.byteLength;
    return offset;
  };
  const stream = (values, width) => ({ offset: append(values), count: values.length / width });
  const metadata = {
    geometries: scene.geometries.map(geometry => ({
      id: geometry.id,
      material: geometry.material,
      instanceType: geometry.instanceType,
      positions: stream(geometry.positions, 3),
      normals: stream(geometry.normals, 3),
      tangents: stream(geometry.tangents, 4),
      uvs: stream(geometry.uvs, 2),
      indices: stream(geometry.indices, 1),
    })),
    occurrences: scene.occurrences,
    materials: scene.materials,
    textures: scene.textures,
    samplers: scene.samplers,
    images: scene.images.map(image => ({
      mimeType: image.mimeType,
      data: { offset: append(image.bytes), byteLength: image.bytes.byteLength },
    })),
  };
  const metadataBytes = encoder.encode(JSON.stringify(metadata));
  const payloadOffset = align4(16 + metadataBytes.byteLength);
  const packet = new Uint8Array(payloadOffset + payloadLength);
  const header = new DataView(packet.buffer);
  header.setUint32(0, PACKET_MAGIC, true);
  header.setUint32(4, PACKET_VERSION, true);
  header.setUint32(8, metadataBytes.byteLength, true);
  header.setUint32(12, payloadLength, true);
  packet.set(metadataBytes, 16);
  for (const chunk of chunks) packet.set(chunk.bytes, payloadOffset + chunk.offset);
  return packet;
}

/** Convert glTF 2.0/GLB bytes into Yawn's format-neutral render-data packet. */
export async function gltfToRenderDataPacket(source, baseUrl, fetcher = fetch) {
  const { document, binary } = parseContainer(source);
  if (document.asset?.version !== "2.0") fail("GLTF_VERSION_UNSUPPORTED");
  const buffers = await loadBuffers(document, binary, baseUrl, fetcher);
  return encodePacket(await decodeScene(document, buffers, baseUrl, fetcher));
}
