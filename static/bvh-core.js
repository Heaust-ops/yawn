export class DerivedBvh {
  constructor() { this.count = 0; this.identity = new Uint32Array(); this.meshIdentity = new Uint32Array(); this.pickable = new Uint8Array(); this.bounds = new Float32Array(); this.nodeBounds = new Float32Array(); this.left = this.right = new Int32Array(); this.leafStart = this.leafCount = new Uint32Array(); this.leaves = new Uint32Array(); this.root = -1; this.rebuilds = 0; this.refits = 0; }
  update(snapshot) {
    const s = snapshot.streams, n = snapshot.instanceCount;
    let changed = n !== this.count;
    if (!changed) for (let i = 0; i < n; i++) if (this.identity[i * 2] !== s.instanceSlot[i] || this.identity[i * 2 + 1] !== s.instanceGeneration[i] || this.meshIdentity[i * 2] !== s.instanceMeshSlot[i] || this.meshIdentity[i * 2 + 1] !== s.instanceMeshGeneration[i]) { changed = true; break; }
    this.count = n; this.identity = new Uint32Array(n * 2); this.meshIdentity = new Uint32Array(n * 2); this.pickable = new Uint8Array(n); this.bounds = new Float32Array(n * 6);
    for (let i = 0; i < n; i++) { this.identity.set([s.instanceSlot[i], s.instanceGeneration[i]], i * 2); this.meshIdentity.set([s.instanceMeshSlot[i], s.instanceMeshGeneration[i]], i * 2); this.pickable[i] = !!(s.instanceType[i * 16] & 1); this.bounds.set(s.instanceWorldMin.subarray(i * 3, i * 3 + 3), i * 6); this.bounds.set(s.instanceWorldMax.subarray(i * 3, i * 3 + 3), i * 6 + 3); }
    changed ? this.rebuild() : this.refit();
  }
  rebuild() {
    this.rebuilds++; const nodes = [], leaves = [];
    const build = indices => { const at = nodes.length, node = {left: -1, right: -1, start: 0, count: 0, bounds: [Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity]}; nodes.push(node); for (const i of indices) for (let a = 0; a < 3; a++) { node.bounds[a] = Math.min(node.bounds[a], this.bounds[i * 6 + a]); node.bounds[a + 3] = Math.max(node.bounds[a + 3], this.bounds[i * 6 + a + 3]); } if (indices.length <= 2) { node.start = leaves.length; node.count = indices.length; leaves.push(...indices); return at; } let axis = 0, extent = node.bounds[3] - node.bounds[0]; for (let a = 1; a < 3; a++) if (node.bounds[a + 3] - node.bounds[a] > extent) { axis = a; extent = node.bounds[a + 3] - node.bounds[a]; } indices.sort((a, b) => (this.bounds[a * 6 + axis] + this.bounds[a * 6 + axis + 3]) - (this.bounds[b * 6 + axis] + this.bounds[b * 6 + axis + 3]) || a - b); const mid = indices.length >> 1; node.left = build(indices.slice(0, mid)); node.right = build(indices.slice(mid)); return at; };
    this.root = this.count ? build(Array.from({length: this.count}, (_, i) => i)) : -1; const n = nodes.length;
    this.nodeBounds = new Float32Array(n * 6); this.left = new Int32Array(n); this.right = new Int32Array(n); this.leafStart = new Uint32Array(n); this.leafCount = new Uint32Array(n); this.leaves = Uint32Array.from(leaves);
    nodes.forEach((x, i) => { this.nodeBounds.set(x.bounds, i * 6); this.left[i] = x.left; this.right[i] = x.right; this.leafStart[i] = x.start; this.leafCount[i] = x.count; });
  }
  refit() { this.refits++; for (let n = this.left.length - 1; n >= 0; n--) { const at = n * 6; for (let a = 0; a < 3; a++) { let lo = Infinity, hi = -Infinity; if (this.leafCount[n]) for (let j = 0; j < this.leafCount[n]; j++) { const i = this.leaves[this.leafStart[n] + j]; lo = Math.min(lo, this.bounds[i * 6 + a]); hi = Math.max(hi, this.bounds[i * 6 + a + 3]); } else { lo = Math.min(this.nodeBounds[this.left[n] * 6 + a], this.nodeBounds[this.right[n] * 6 + a]); hi = Math.max(this.nodeBounds[this.left[n] * 6 + a + 3], this.nodeBounds[this.right[n] * 6 + a + 3]); } this.nodeBounds[at + a] = lo; this.nodeBounds[at + a + 3] = hi; } } }
  pick(origin, direction, maxDistance = Infinity, maxHits = 1) {
    if (this.root < 0) return []; let magnitude = Math.hypot(...direction); const dir = direction.map(v => v / magnitude);
    const intersect = (array, at) => { let lo = 0, hi = maxDistance; for (let a = 0; a < 3; a++) { const min = array[at + a], max = array[at + a + 3]; if (dir[a] === 0) { if (origin[a] < min || origin[a] > max) return Infinity; } else { let x = (min - origin[a]) / dir[a], y = (max - origin[a]) / dir[a]; if (x > y) [x, y] = [y, x]; lo = Math.max(lo, x); hi = Math.min(hi, y); if (lo > hi) return Infinity; } } return lo; };
    const hits = [], stack = [this.root]; while (stack.length) { const n = stack.pop(); if (intersect(this.nodeBounds, n * 6) === Infinity) continue; if (this.leafCount[n]) for (let j = 0; j < this.leafCount[n]; j++) { const i = this.leaves[this.leafStart[n] + j]; if (!this.pickable[i]) continue; const distance = intersect(this.bounds, i * 6); if (distance !== Infinity) hits.push({slot: this.identity[i * 2], generation: this.identity[i * 2 + 1], distance}); } else stack.push(this.right[n], this.left[n]); }
    hits.sort((a, b) => a.distance - b.distance || a.slot - b.slot || a.generation - b.generation); return hits.slice(0, maxHits);
  }
}
