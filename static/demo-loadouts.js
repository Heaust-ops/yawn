const JSON_CHUNK = 0x4e4f534a;
const BIN_CHUNK = 0x004e4942;
const encoder = new TextEncoder();

const align4 = value => (value + 3) & ~3;
const finiteMinMax = (values, width) => {
  const min = Array(width).fill(Infinity), max = Array(width).fill(-Infinity);
  for (let i=0;i<values.length;i++) { const lane=i%width; min[lane]=Math.min(min[lane],values[i]); max[lane]=Math.max(max[lane],values[i]); }
  return {min,max};
};

/** Encode indexed geometry as a deterministic, self-contained GLB 2.0 scene. */
export function encodeGeometryGlb({positions,normals,texcoords,indices}) {
  if([...positions,...normals,...texcoords].some(value=>!Number.isFinite(value))||indices.some(value=>!Number.isInteger(value)||value<0))throw new TypeError("Invalid demo geometry");
  const streams=[new Float32Array(positions),new Float32Array(normals),new Float32Array(texcoords),new Uint32Array(indices)];
  if(!streams[0].length||streams[0].length%3||streams[1].length!==streams[0].length||streams[2].length/2!==streams[0].length/3||streams[3].length%3) throw new TypeError("Invalid demo geometry");
  const offsets=[], chunks=[], views=[]; let byteLength=0;
  for(const stream of streams){byteLength=align4(byteLength);offsets.push(byteLength);const bytes=new Uint8Array(stream.buffer);chunks.push({offset:byteLength,bytes});views.push({buffer:0,byteOffset:byteLength,byteLength:bytes.length});byteLength+=bytes.length;}
  byteLength=align4(byteLength);
  const vertexCount=streams[0].length/3, bounds=finiteMinMax(streams[0],3);
  if(indices.some(value=>value>=vertexCount))throw new TypeError("Invalid demo geometry");
  const nodes=[]; for(let z=-1;z<=1;z++)for(let x=-1;x<=1;x++)nodes.push({mesh:0,translation:[x*3,0,z*3]});
  const json={asset:{version:"2.0",generator:"yawn-phase8"},scene:0,scenes:[{nodes:nodes.map((_,i)=>i)}],nodes,meshes:[{primitives:[{attributes:{POSITION:0,NORMAL:1,TEXCOORD_0:2},indices:3}]}],buffers:[{byteLength}],bufferViews:views,accessors:[
    {bufferView:0,componentType:5126,count:vertexCount,type:"VEC3",min:bounds.min,max:bounds.max},
    {bufferView:1,componentType:5126,count:vertexCount,type:"VEC3"},
    {bufferView:2,componentType:5126,count:vertexCount,type:"VEC2"},
    {bufferView:3,componentType:5125,count:streams[3].length,type:"SCALAR"},
  ]};
  let jsonBytes=encoder.encode(JSON.stringify(json)); const jsonLength=align4(jsonBytes.length), total=12+8+jsonLength+8+byteLength;
  const out=new ArrayBuffer(total), view=new DataView(out), bytes=new Uint8Array(out); view.setUint32(0,0x46546c67,true);view.setUint32(4,2,true);view.setUint32(8,total,true);
  view.setUint32(12,jsonLength,true);view.setUint32(16,JSON_CHUNK,true);bytes.fill(0x20,20,20+jsonLength);bytes.set(jsonBytes,20);
  const binHeader=20+jsonLength;view.setUint32(binHeader,byteLength,true);view.setUint32(binHeader+4,BIN_CHUNK,true);for(const chunk of chunks)bytes.set(chunk.bytes,binHeader+8+chunk.offset);
  return out;
}

export function createCubeGeometry(){
  const positions=[],normals=[],texcoords=[],indices=[];const faces=[[[1,0,0],[1,-1,-1],[1,-1,1],[1,1,1],[1,1,-1]],[[-1,0,0],[-1,-1,1],[-1,-1,-1],[-1,1,-1],[-1,1,1]],[[0,1,0],[-1,1,1],[1,1,1],[1,1,-1],[-1,1,-1]],[[0,-1,0],[-1,-1,-1],[1,-1,-1],[1,-1,1],[-1,-1,1]],[[0,0,1],[-1,-1,1],[1,-1,1],[1,1,1],[-1,1,1]],[[0,0,-1],[1,-1,-1],[-1,-1,-1],[-1,1,-1],[1,1,-1]]];
  for(const [normal,...corners] of faces){
    const base=positions.length/3;corners.forEach((p,i)=>{positions.push(...p);normals.push(...normal);texcoords.push(...[[0,0],[1,0],[1,1],[0,1]][i]);});
    const a=corners[0],b=corners[1],c=corners[2],ab=b.map((value,i)=>value-a[i]),ac=c.map((value,i)=>value-a[i]);
    const cross=[ab[1]*ac[2]-ab[2]*ac[1],ab[2]*ac[0]-ab[0]*ac[2],ab[0]*ac[1]-ab[1]*ac[0]];
    const outward=cross.reduce((sum,value,i)=>sum+value*normal[i],0)>0;
    indices.push(...(outward?[base,base+1,base+2,base,base+2,base+3]:[base,base+2,base+1,base,base+3,base+2]));
  }
  return {positions,normals,texcoords,indices};
}
export function createUvSphereGeometry(segments=24,rings=12){
  const positions=[],normals=[],texcoords=[],indices=[];for(let y=0;y<=rings;y++){const v=y/rings,phi=v*Math.PI;for(let x=0;x<=segments;x++){const u=x/segments,theta=u*Math.PI*2,nx=Math.sin(phi)*Math.cos(theta),ny=Math.cos(phi),nz=Math.sin(phi)*Math.sin(theta);positions.push(nx,ny,nz);normals.push(nx,ny,nz);texcoords.push(u,v);}}
  for(let y=0;y<rings;y++)for(let x=0;x<segments;x++){const a=y*(segments+1)+x,b=a+segments+1;indices.push(a,a+1,b,a+1,b+1,b);}return {positions,normals,texcoords,indices};
}
export function isGitLfsPointer(bytes){const text=new TextDecoder().decode(new Uint8Array(bytes,0,Math.min(bytes.byteLength,256)));return text.startsWith("version https://git-lfs.github.com/spec/v1\n");}
export class LoadoutError extends Error{constructor(code,message){super(message);this.name="LoadoutError";this.code=code;}}
export const loadouts=Object.freeze({cubes:{label:"Procedural cubes"},spheres:{label:"Procedural spheres"},manor:{label:"The Manor"},sponza:{label:"Sponza"}});
const assetUrls=Object.freeze({manor:new URL("./themanor.glb",import.meta.url),sponza:new URL("./sponza.glb",import.meta.url)});
export async function loadDemoLoadout(id,{signal,fetchImpl=fetch}={}){
  if(id==="cubes")return encodeGeometryGlb(createCubeGeometry());if(id==="spheres")return encodeGeometryGlb(createUvSphereGeometry());
  const url=assetUrls[id];if(!url)throw new LoadoutError("LOADOUT_UNKNOWN",`Unknown loadout: ${id}`);
  let response;try{response=await fetchImpl(url,{signal});}catch(error){if(error?.name==="AbortError")throw error;throw new LoadoutError("LOADOUT_FETCH_FAILED",`Could not fetch ${id}: ${error?.message||"network error"}`);}
  if(!response.ok)throw new LoadoutError("LOADOUT_HTTP",`Could not fetch ${id}: HTTP ${response.status}`);const buffer=await response.arrayBuffer();if(isGitLfsPointer(buffer))throw new LoadoutError("LOADOUT_LFS_POINTER",`${id} is a Git LFS pointer; hydrate repository assets first`);return buffer;
}
