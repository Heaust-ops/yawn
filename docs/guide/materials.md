# Materials and textures

`PBRMaterial` is a conventional view over `materials` and `materialTextures` SOA rows.

```ts
const paint = new PBRMaterial(scene, {
  baseColor: [0.8, 0.05, 0.03, 1],
  metallic: 0.65,
  roughness: 0.22,
  emissive: [0, 0, 0],
});
await paint.ready;

paint.roughness = 0.5;       // direct SAB write
paint.baseColor[1] = 0.35;   // direct SAB write
mesh.material = paint;
```

<Playground example="materials" />

## Graph textures

```ts
const albedo = new Texture(scene, {
  source: "/textures/paint.png",
  format: "rgba8unorm-srgb",
});
await albedo.ready;

const textured = new PBRMaterial(scene, { baseColorTexture: albedo });
```

Creating or removing a `Texture` rebuilds the single graph loadout so the GPU resource is allocated up front. The addon generates and transfers a complete mip chain by default; pass `mipmaps: false` only for data that must remain single-level. Compatible loadout rebuilds reuse the allocated GPU texture without uploading it again.

## Custom WGSL

`ShaderMaterial` adds its external WGSL pipeline to the scene graph. Updating the code updates the loadout.

```ts
const shader = new ShaderMaterial(scene, {
  code: `
    struct Out { @builtin(position) position: vec4<f32> }
    @vertex fn vertex(@builtin(vertex_index) id: u32) -> Out {
      let p = array(vec2(-.2, -.2), vec2(.2, -.2), vec2(0., .2));
      var out: Out; out.position = vec4(p[id], 0., 1.); return out;
    }
    @fragment fn fragment() -> @location(0) vec4<f32> {
      return vec4(1., .2, .7, 1.);
    }
  `,
});
await shader.ready;
```

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
