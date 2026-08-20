export {};

const decoder = new TextDecoder();
const widths: Record<string, number> = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 };
const sizes: Record<number, number> = {
  5120: 1,
  5121: 1,
  5122: 2,
  5123: 2,
  5125: 4,
  5126: 4,
};
const arrays: Record<number, any> = {
  5120: Int8Array,
  5121: Uint8Array,
  5122: Int16Array,
  5123: Uint16Array,
  5125: Uint32Array,
  5126: Float32Array,
};

function component(view: DataView, offset: number, type: number) {
  if (type === 5120) return view.getInt8(offset);
  if (type === 5121) return view.getUint8(offset);
  if (type === 5122) return view.getInt16(offset, true);
  if (type === 5123) return view.getUint16(offset, true);
  if (type === 5125) return view.getUint32(offset, true);
  if (type === 5126) return view.getFloat32(offset, true);
  throw new Error("GLTF_COMPONENT");
}

function rotate(quaternion: number[], value: number[]) {
  const [qx, qy, qz, qw] = quaternion;
  const [x, y, z] = value;
  const tx = 2 * (qy * z - qz * y);
  const ty = 2 * (qz * x - qx * z);
  const tz = 2 * (qx * y - qy * x);
  return [
    x + qw * tx + qy * tz - qz * ty,
    y + qw * ty + qz * tx - qx * tz,
    z + qw * tz + qx * ty - qy * tx,
  ];
}

function multiply(a: number[], b: number[]) {
  return [
    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
  ];
}

function decompose(matrix: number[]) {
  const scale = [
    Math.hypot(matrix[0], matrix[1], matrix[2]),
    Math.hypot(matrix[4], matrix[5], matrix[6]),
    Math.hypot(matrix[8], matrix[9], matrix[10]),
  ];
  const [sx, sy, sz] = scale;
  const [m00, m01, m02] = [matrix[0] / sx, matrix[4] / sy, matrix[8] / sz];
  const [m10, m11, m12] = [matrix[1] / sx, matrix[5] / sy, matrix[9] / sz];
  const [m20, m21, m22] = [matrix[2] / sx, matrix[6] / sy, matrix[10] / sz];
  let quaternion: number[];
  if (m00 + m11 + m22 > 0) {
    const s = Math.sqrt(1 + m00 + m11 + m22) * 2;
    quaternion = [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, s / 4];
  } else if (m00 > m11 && m00 > m22) {
    const s = Math.sqrt(1 + m00 - m11 - m22) * 2;
    quaternion = [s / 4, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s];
  } else if (m11 > m22) {
    const s = Math.sqrt(1 + m11 - m00 - m22) * 2;
    quaternion = [(m01 + m10) / s, s / 4, (m12 + m21) / s, (m02 - m20) / s];
  } else {
    const s = Math.sqrt(1 + m22 - m00 - m11) * 2;
    quaternion = [(m02 + m20) / s, (m12 + m21) / s, s / 4, (m10 - m01) / s];
  }
  return { position: matrix.slice(12, 15), quaternion, scale };
}

function transform(node: any) {
  return node.matrix
    ? decompose(node.matrix)
    : {
        position: node.translation ?? [0, 0, 0],
        quaternion: node.rotation ?? [0, 0, 0, 1],
        scale: node.scale ?? [1, 1, 1],
      };
}

function compose(parent: any, local: any) {
  const position = rotate(
    parent.quaternion,
    local.position.map(
      (value: number, lane: number) => value * parent.scale[lane],
    ),
  );
  return {
    position: position.map((value, lane) => value + parent.position[lane]),
    quaternion: multiply(parent.quaternion, local.quaternion),
    scale: local.scale.map(
      (value: number, lane: number) => value * parent.scale[lane],
    ),
  };
}

function parse(bytes: Uint8Array) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0, true) !== 0x46546c67)
    return {
      document: JSON.parse(decoder.decode(bytes)),
      binary: undefined as Uint8Array | undefined,
    };
  let offset = 12;
  let document: any;
  let binary: Uint8Array | undefined;
  while (offset < bytes.length) {
    const length = view.getUint32(offset, true);
    const type = view.getUint32(offset + 4, true);
    const chunk = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 0x4e4f534a)
      document = JSON.parse(decoder.decode(chunk).replace(/\0+$/u, ""));
    if (type === 0x004e4942) binary = chunk;
    offset += 8 + length;
  }
  if (!document) throw new Error("GLTF_JSON");
  return { document, binary };
}

async function load(url: string) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`HTTP_${response.status}`);
  const { document, binary } = parse(
    new Uint8Array(await response.arrayBuffer()),
  );
  const buffers = await Promise.all(
    (document.buffers ?? []).map(async (buffer: any, index: number) => {
      if (buffer.uri === undefined) {
        if (index || !binary) throw new Error("GLTF_BUFFER");
        return binary;
      }
      const result = await fetch(new URL(buffer.uri, url));
      if (!result.ok) throw new Error(`HTTP_${result.status}`);
      return new Uint8Array(await result.arrayBuffer());
    }),
  );
  const imageBlobs = await Promise.all(
    (document.images ?? []).map(async (image: any) => {
      if (image.bufferView !== undefined) {
        const view = document.bufferViews[image.bufferView];
        const bytes = buffers[view.buffer];
        const start = view.byteOffset ?? 0;
        return new Blob([bytes.slice(start, start + view.byteLength)], {
          type: image.mimeType,
        });
      }
      const result = await fetch(new URL(image.uri, url));
      if (!result.ok) throw new Error(`HTTP_${result.status}`);
      return result.blob();
    }),
  );
  const textures = await Promise.all(
    (document.textures ?? []).map(async (texture: any) => ({
      image: await createImageBitmap(imageBlobs[texture.source]),
    })),
  );

  const accessor = (id: number, integer = false) => {
    const source = document.accessors[id];
    const width = widths[source.type];
    const size = sizes[source.componentType];
    const bufferView = document.bufferViews[source.bufferView];
    const bytes = buffers[bufferView.buffer];
    const start = (bufferView.byteOffset ?? 0) + (source.byteOffset ?? 0);
    const stride = bufferView.byteStride ?? width * size;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const values = integer
      ? new Uint32Array(source.count * width)
      : new Float32Array(source.count * width);
    if (
      !source.normalized &&
      stride === width * size &&
      (bytes.byteOffset + start) % size === 0
    ) {
      const packed = new arrays[source.componentType](
        bytes.buffer,
        bytes.byteOffset + start,
        source.count * width,
      );
      values.set(packed);
      return values;
    }
    for (let item = 0; item < source.count; item++)
      for (let lane = 0; lane < width; lane++) {
        let value = component(
          view,
          start + item * stride + lane * size,
          source.componentType,
        );
        if (!integer && source.normalized) {
          const maximum =
            source.componentType === 5121
              ? 255
              : source.componentType === 5123
                ? 65535
                : 1;
          value /= maximum;
        }
        values[item * width + lane] = value;
      }
    return values;
  };

  const materials = (document.materials ?? []).map((material: any) => {
    const pbr = material.pbrMetallicRoughness ?? {};
    return {
      baseColor: pbr.baseColorFactor ?? [1, 1, 1, 1],
      metallic: pbr.metallicFactor ?? 0,
      roughness: pbr.roughnessFactor ?? 0.7,
      emissive: material.emissiveFactor ?? [0, 0, 0],
      alphaCutoff: material.alphaCutoff ?? 0.5,
      baseColorTexture: pbr.baseColorTexture?.index ?? -1,
      metallicRoughnessTexture: pbr.metallicRoughnessTexture?.index ?? -1,
      normalTexture: material.normalTexture?.index ?? -1,
      emissiveTexture: material.emissiveTexture?.index ?? -1,
    };
  });
  const primitives: any[] = [];
  const emitMesh = (meshId: number, transform: any) => {
    const mesh = document.meshes?.[meshId];
    for (const primitive of mesh?.primitives ?? []) {
      if (
        (primitive.mode ?? 4) !== 4 ||
        primitive.attributes.POSITION === undefined
      )
        continue;
      const positions = accessor(primitive.attributes.POSITION);
      const indices =
        primitive.indices === undefined
          ? Uint32Array.from(
              { length: positions.length / 3 },
              (_, index) => index,
            )
          : accessor(primitive.indices, true);
      primitives.push({
        positions,
        indices,
        ...(primitive.attributes.NORMAL === undefined
          ? {}
          : { normals: accessor(primitive.attributes.NORMAL) }),
        ...(primitive.attributes.TANGENT === undefined
          ? {}
          : { tangents: accessor(primitive.attributes.TANGENT) }),
        ...(primitive.attributes.TEXCOORD_0 === undefined
          ? {}
          : { uvs: accessor(primitive.attributes.TEXCOORD_0) }),
        ...(primitive.attributes.COLOR_0 === undefined
          ? {}
          : { colors: accessor(primitive.attributes.COLOR_0) }),
        material: primitive.material ?? -1,
        ...transform,
      });
    }
  };

  const nodes = document.nodes ?? [];
  const scene = document.scenes?.[document.scene ?? 0];
  const children = new Set(nodes.flatMap((node: any) => node.children ?? []));
  const roots =
    scene?.nodes ??
    nodes
      .map((_: any, id: number) => id)
      .filter((id: number) => !children.has(id));
  const visit = (
    id: number,
    parent = {
      position: [0, 0, 0],
      quaternion: [0, 0, 0, 1],
      scale: [1, 1, 1],
    },
  ) => {
    const node = nodes[id] ?? {};
    const world = compose(parent, transform(node));
    if (node.mesh !== undefined) emitMesh(node.mesh, world);
    for (const child of node.children ?? []) visit(child, world);
  };
  for (const root of roots) visit(root);
  if (!nodes.length)
    for (let id = 0; id < (document.meshes ?? []).length; id++)
      emitMesh(id, {});
  return { materials, primitives, textures };
}

addEventListener("message", async ({ data }) => {
  try {
    const result = await load(data.url);
    const transfers = result.primitives.flatMap((primitive: any) =>
      ["positions", "indices", "normals", "tangents", "uvs", "colors"]
        .map((name) => primitive[name]?.buffer)
        .filter(Boolean),
    );
    transfers.push(...result.textures.map((texture: any) => texture.image));
    (postMessage as any)({ request: data.request, result }, transfers);
  } catch (error) {
    postMessage({
      request: data.request,
      error: error instanceof Error ? error.message : "GLTF_IMPORT",
    });
  }
});
