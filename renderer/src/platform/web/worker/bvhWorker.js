// Dedicated single-writer scene BVH worker. Inputs and outputs are immutable
// structured clones. Publications have explicit generation leases; the owner
// acknowledges each pick result before that generation can be retired.
let current = null;
let publicationVersion = 0;
const leased = new Set();

const merge = (a, b) => ({ min: a.min.map((v, i) => Math.min(v, b.min[i])), max: a.max.map((v, i) => Math.max(v, b.max[i])) });
function worldBounds(item) {
  const m = item.transform, b = item.localBounds;
  const c = b.min.map((v, i) => v * .5 + b.max[i] * .5), e = b.min.map((v, i) => (b.max[i] - v) * .5);
  const wc = [m[0]*c[0]+m[4]*c[1]+m[8]*c[2]+m[12], m[1]*c[0]+m[5]*c[1]+m[9]*c[2]+m[13], m[2]*c[0]+m[6]*c[1]+m[10]*c[2]+m[14]];
  const we = [Math.abs(m[0])*e[0]+Math.abs(m[4])*e[1]+Math.abs(m[8])*e[2], Math.abs(m[1])*e[0]+Math.abs(m[5])*e[1]+Math.abs(m[9])*e[2], Math.abs(m[2])*e[0]+Math.abs(m[6])*e[1]+Math.abs(m[10])*e[2]];
  return { min: wc.map((v,i)=>v-we[i]), max: wc.map((v,i)=>v+we[i]) };
}
function build(indices, leaves) {
  if (indices.length === 1) return { bounds: leaves[indices[0]].bounds, leaf: indices[0] };
  let bounds = indices.map(i=>leaves[i].bounds).reduce(merge), extent = bounds.max.map((v,i)=>v-bounds.min[i]);
  const axis = extent.indexOf(Math.max(...extent));
  indices.sort((a,b)=>(leaves[a].bounds.min[axis]+leaves[a].bounds.max[axis])-(leaves[b].bounds.min[axis]+leaves[b].bounds.max[axis]));
  const mid = indices.length >> 1, left = build(indices.slice(0,mid), leaves), right = build(indices.slice(mid), leaves);
  return { bounds: merge(left.bounds,right.bounds), left, right };
}
function refit(node, leaves) { if (node.leaf !== undefined) return node.bounds = leaves[node.leaf].bounds; return node.bounds = merge(refit(node.left,leaves),refit(node.right,leaves)); }
function hit(ray,b) { let n=0,f=Infinity; for(let i=0;i<3;i++){ if(Math.abs(ray.direction[i])<1e-8){if(ray.origin[i]<b.min[i]||ray.origin[i]>b.max[i])return null;}else{let a=(b.min[i]-ray.origin[i])/ray.direction[i],z=(b.max[i]-ray.origin[i])/ray.direction[i];if(a>z)[a,z]=[z,a];n=Math.max(n,a);f=Math.min(f,z);if(n>f)return null;}} return n; }
function query(node, leaves, ray, best) { const d=hit(ray,node.bounds); if(d===null||d>best.distance)return; if(node.leaf!==undefined){best.distance=d;best.item=leaves[node.leaf].item;return;} query(node.left,leaves,ray,best);query(node.right,leaves,ray,best); }
onmessage = ({data}) => {
  if(data.type === "snapshot") {
    const leaves=data.instances.filter(item=>(item.flags & 9) === 9 && item.layerMask !== 0).map(item=>({item,bounds:worldBounds(item)}));
    const key=leaves.map(x=>`${x.item.slot}:${x.item.generation}`).join("|");
    if(leaves.length && current && current.key===key && current.leaves.length===leaves.length){ current.leaves=leaves; refit(current.root,leaves); current.mode="refit"; }
    else current={key,leaves,root:leaves.length?build(leaves.map((_,i)=>i),leaves):null,mode:"rebuild"};
    Object.assign(current,{snapshotId:data.snapshotId,sceneCommitEpoch:data.sceneCommitEpoch,publicationVersion:++publicationVersion});
    postMessage({type:"bvh-published",snapshotId:data.snapshotId,publicationVersion,mode:current.mode});
  } else if(data.type === "pick") {
    if(!current || current.snapshotId!==data.spatialSnapshotId || current.sceneCommitEpoch!==data.sceneCommitEpoch) return postMessage({type:"pick-result",requestId:data.requestId,status:"stale",snapshotId:data.spatialSnapshotId});
    const best={distance:Infinity,item:null}; if(current.root)query(current.root,current.leaves,data.ray,best);
    leased.add(current.publicationVersion);
    postMessage({type:"pick-result",requestId:data.requestId,status:best.item?"hit":"no-hit",slot:best.item?.slot,generation:best.item?.generation,snapshotId:current.snapshotId,publicationVersion:current.publicationVersion});
  } else if(data.type === "ack") leased.delete(data.publicationVersion);
};

export function attachBvh() {}
