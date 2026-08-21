# Cameras and controls

Every camera allocates generic `cameras`, `cameraMatrices`, and transform rows. Camera handles precompute the view-projection matrix when lens or transform values change, so the forward vertex shader performs four dot products instead of rebuilding the camera projection for every vertex.

## Shared lens and projection controls

```ts
const camera = new Camera(scene, {
  fov: Math.PI / 3,
  near: 0.05,
  far: 2000,
  focalLength: 50,
  aperture: 2.8,
  focusDistance: 8,
});
await camera.ready;

camera.projection = "orthographic";
camera.orthoSize = 12;
camera.projection = "perspective";
```

`fov`, `aspect`, `near`, `far`, `orthoSize`, `focalLength`, `aperture`, `focusDistance`, and `sensorWidth` all write the camera's shared row.

## Arc rotate

```ts
const orbit = new ArcRotateCamera(scene, {
  alpha: 0,
  beta: Math.PI / 3,
  radius: 6,
  target: mesh,
  controls: {
    element: canvas,
    pointer: true,      // left orbit, right pan, wheel zoom
    controller: true,  // sticks + triggers
  },
});
await orbit.ready;
```

## Free spectator camera

```ts
const free = new FreeCamera(scene, {
  position: [0, 1, 5],
  controls: {
    element: canvas,
    keyboard: true,    // WASD + Space/Ctrl
    pointer: true,     // click for pointer lock, mouse to look
    controller: true,
    speed: 6,
  },
});
await free.ready;
```

## Follow a character

```ts
const follow = new FollowCamera(scene, {
  target: player,
  distance: 5,
  height: 1.8,
  smoothing: 0.12,
});
await follow.ready;

follow.target = anotherPlayer;
follow.distance = 7;
follow.stop();
follow.start();
```

The input and follow loops never post camera updates to core: they mutate the position, rotor, camera, and derived matrix rows directly in shared memory.

<Playground example="cameras" />

<script setup>
import Playground from "../.vitepress/Playground.vue";
</script>
