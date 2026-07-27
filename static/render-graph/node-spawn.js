/** Allocates a bounded fxnode-safe ID, reserving candidates for this session. */
export function createNodeIdAllocator(randomUUID = () => crypto.randomUUID()) {
  const reserved = new Set();
  return (existingIds) => {
    const existing = new Set(existingIds);
    for (let attempt = 0; attempt < 64; attempt++) {
      const id = `node_${randomUUID().replaceAll("-", "")}`;
      if (/^node_[A-Za-z0-9_]+$/.test(id) && id.length <= 128 && !existing.has(id) && !reserved.has(id)) {
        reserved.add(id);
        return id;
      }
    }
    throw new Error("Unable to allocate a unique node ID");
  };
}

/** Adds exactly one node when the request still targets the loaded composition. */
export async function spawnRequestedNode(root, view, request, typeId, allocateId, isCurrent = () => true) {
  const current = () =>
    isCurrent() && request.compositionRevision === view.getHostSnapshot().compositionRevision;
  if (!typeId || !current()) return false;
  let state;
  try {
    state = await root.getState();
  } catch (error) {
    if (!isCurrent()) return false;
    throw error;
  }
  if (!current()) return false;
  const nodeId = allocateId(state.nodes.map((node) => node.id));
  if (!current()) return false;
  try {
    await view.addNode(
      { typeId, nodeId, viewPosition: request.viewPosition },
      { expectedVersion: state.version },
    );
  } catch (error) {
    if (!isCurrent()) return false;
    throw error;
  }
  return true;
}
