const triangleShader = /* wgsl */ `
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;

struct Vertex { @builtin(position) position: vec4<f32> }

@vertex fn vertex(@builtin(vertex_index) index: u32) -> Vertex {
  let positions = array(vec2(-0.75, -0.65), vec2(0.75, -0.65), vec2(0.0, 0.75));
  var output: Vertex;
  output.position = vec4(positions[index], 0.0, 1.0);
  return output;
}

@fragment fn fragment() -> @location(0) vec4<f32> { return tint.color; }
`;

const noopComputeShader = /* wgsl */ `
@compute @workgroup_size(1) fn main() {}
`;

/** A complete external graph used by the minimal playground; core contains neither program. */
export function triangleGraph(colorArray = "triangle.color") {
  return {
    id: "triangle",
    resources: {
      buffers: [{ id: "color", array: colorArray, usage: ["uniform"] }],
    },
    pipelines: {
      compute: [{ id: "prepare", code: noopComputeShader }],
      render: [{
        id: "triangle",
        code: triangleShader,
        vertex: { entry: "vertex" },
        fragment: { entry: "fragment", targets: [{ format: "canvas" }] },
      }],
    },
    passes: [
      { id: "prepare", type: "compute", pipeline: "prepare", dispatch: [1, 1, 1] },
      {
        id: "draw",
        type: "render",
        pipeline: "triangle",
        after: ["prepare"],
        bindings: [{ group: 0, binding: 0, resource: "color" }],
        color: [{ resource: "canvas", clear: [0.025, 0.035, 0.055, 1] }],
        draw: { vertices: 3 },
      },
    ],
  };
}
