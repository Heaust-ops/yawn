# Clustered lights

The default `Scene` graph runs a clustered compute pass before its HDR forward passes. Light handles write flat rows consumed by that pass.

```ts
const point = new PointLight(scene, {
  position: [0, 1, 1],
  color: [1, 0.25, 0.05],
  intensity: 20,
  range: 8,
});

const sun = new DirectionalLight(scene, {
  quaternion: [0.2, 0, 0, 0.98],
  color: [1, 0.95, 0.8],
  intensity: 3,
});

const fill = new AmbientLight(scene, { color: [0.1, 0.2, 0.4], intensity: 0.2 });
await Promise.all([point.ready, sun.ready, fill.ready]);
```

<Playground example="lights" />

## Rectangles and spots

```ts
const panel = new RectAreaLight(scene, {
  position: [0, 2, 0],
  width: 2,
  height: 0.5,
  intensity: 12,
});

const spot = new SpotLight(scene, {
  position: [0, 1, 1],
  innerAngle: 0.25,
  outerAngle: 0.6,
  range: 15,
});
```

The rectangle handle selects the default graph's linearly transformed cosine (`ltc`) path. Position, orientation, intensity, angles, and colors remain direct SAB mutations after allocation.

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
