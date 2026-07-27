---
layout: home

hero:
  name: fxnode
  text: A worker-owned node editor
  tagline: Compose a typed graph language, present it on Canvas, and persist it safely.
  actions:
    - theme: brand
      text: Start learning
      link: /learn/
    - theme: alt
      text: API Reference
      link: /reference/

features:
  - title: Worker authority
    details: Commands, validation, hit testing, layout, history, and rendering live behind one explicit boundary.
  - title: Zero or many views
    details: Run headlessly or attach independent canvases with view-local cameras, selections, input, and rendering to one shared graph.
  - title: Application composition
    details: Your application supplies node definitions, socket compatibility, theme, and resource policies.
  - title: Durable documents
    details: Canonical saves, bounded decoding, opaque unknown nodes, and declarative migrations protect user data.
---

::: warning Prerelease
fxnode is an internal `0.x` prerelease. APIs and persistence contracts can still change.
:::

fxnode **presents and persists** node graphs. It does not evaluate or execute them. It is not Blender-compatible and makes no Blender feature or visual parity claim.
