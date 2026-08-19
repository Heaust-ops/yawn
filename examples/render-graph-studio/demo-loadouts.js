// Procedural example assets keep the package demo self-contained.
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
  const json={asset:{version:"2.0",generator:"yawn-demo"},scene:0,scenes:[{nodes:nodes.map((_,i)=>i)}],nodes,meshes:[{primitives:[{attributes:{POSITION:0,NORMAL:1,TEXCOORD_0:2},indices:3}]}],buffers:[{byteLength}],bufferViews:views,accessors:[
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

const galleryPngBase64=Object.freeze({
  base:"iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAIUlEQVR42mP4ryH3X+NOgIZGwP//DP/vyGn8BwI5Obn/AKsPDa3HqsdFAAAAAElFTkSuQmCC",
  mr:"iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAJUlEQVR42gEaAOX/AP8gAP//YED//7Sg/wD/8P///0Dc///cIP/dYBIQ76JUtAAAAABJRU5ErkJggg==",
  normal:"iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAH0lEQVR42mNoaPj//0TDu/8WQMzQcOLd/wYLIAYKAgDieBGxoS0BjwAAAABJRU5ErkJggg==",
});
const decodeBase64=value=>{const binary=atob(value),bytes=new Uint8Array(binary.length);for(let i=0;i<binary.length;i++)bytes[i]=binary.charCodeAt(i);return bytes;};

/** Build a deterministic PBR shader validation gallery. */
export function createMaterialGalleryGlb(){
  // A modest shared sphere keeps the embedded GLB compact while making roughness
  // and normal-map responses much easier to compare than the former cubes.
  const geometry=createUvSphereGeometry(16,8);
  const streams=[new Float32Array(geometry.positions),new Float32Array(geometry.normals),new Float32Array(geometry.texcoords),new Uint32Array(geometry.indices)];
  const images=Object.values(galleryPngBase64).map(decodeBase64),chunks=[],bufferViews=[];let byteLength=0;
  for(const stream of [...streams,...images]){byteLength=align4(byteLength);const bytes=stream instanceof Uint8Array?stream:new Uint8Array(stream.buffer);chunks.push({offset:byteLength,bytes});bufferViews.push({buffer:0,byteOffset:byteLength,byteLength:bytes.length});byteLength+=bytes.length;}
  byteLength=align4(byteLength);const bounds=finiteMinMax(streams[0],3),vertexCount=geometry.positions.length/3;
  const materials=[
    ...[0.08,0.3,0.6,1].map(roughnessFactor=>({name:`Dielectric roughness ${roughnessFactor}`,pbrMetallicRoughness:{baseColorFactor:[0.72,0.18,0.08,1],metallicFactor:0,roughnessFactor}})),
    ...[0.08,0.3,0.6,1].map(roughnessFactor=>({name:`Metal roughness ${roughnessFactor}`,pbrMetallicRoughness:{baseColorFactor:[0.72,0.76,0.82,1],metallicFactor:1,roughnessFactor}})),
    ...[1,1.5,2].map(ior=>({name:`Dielectric IOR ${ior}`,pbrMetallicRoughness:{baseColorFactor:[0.12,0.48,0.82,1],metallicFactor:0,roughnessFactor:0.18},extensions:{KHR_materials_ior:{ior}}})),
    {name:"Odd-width OpenGL normal map",pbrMetallicRoughness:{baseColorFactor:[0.7,0.7,0.7,1],metallicFactor:0,roughnessFactor:0.4},normalTexture:{index:2,scale:1}},
    {name:"Odd-width AO",pbrMetallicRoughness:{baseColorFactor:[0.8,0.55,0.12,1],metallicFactor:0,roughnessFactor:0.65},occlusionTexture:{index:1,strength:1}},
    {name:"Odd-width emissive",pbrMetallicRoughness:{baseColorFactor:[0.03,0.03,0.03,1],metallicFactor:0,roughnessFactor:0.8},emissiveFactor:[1,0.3,0.05],emissiveTexture:{index:0}},
    {name:"Odd-width alpha MASK",pbrMetallicRoughness:{baseColorFactor:[1,1,1,1],baseColorTexture:{index:0},metallicFactor:0,roughnessFactor:0.55},alphaMode:"MASK",alphaCutoff:0.5,doubleSided:true},
    {name:"Reflected non-uniform double-sided",pbrMetallicRoughness:{baseColorFactor:[0.25,0.85,0.38,1],metallicFactor:0.15,roughnessFactor:0.45},doubleSided:true},
  ];
  const nodes=materials.map((material,index)=>({name:material.name,mesh:index,translation:[(index%4-1.5)*2.5,(1.5-Math.floor(index/4))*2.5,0],...(index===15?{scale:[-1.25,0.7,1.1]}:{})}));
  const meshes=materials.map((material,index)=>({name:material.name,primitives:[{attributes:{POSITION:0,NORMAL:1,TEXCOORD_0:2},indices:3,material:index}]}));
  const json={asset:{version:"2.0",generator:"yawn-pbr-gallery"},extensionsUsed:["KHR_materials_ior"],scene:0,scenes:[{name:"Deterministic PBR gallery",nodes:nodes.map((_,i)=>i)}],nodes,meshes,materials,
    samplers:[{magFilter:9728,minFilter:9728,wrapS:10497,wrapT:10497}],images:images.map((_,i)=>({name:["Odd-width sRGB base color and emissive","Odd-width linear MR and AO","Odd-width OpenGL normal map"][i],bufferView:i+4,mimeType:"image/png"})),textures:images.map((_,i)=>({sampler:0,source:i})),
    buffers:[{byteLength}],bufferViews,accessors:[{bufferView:0,componentType:5126,count:vertexCount,type:"VEC3",min:bounds.min,max:bounds.max},{bufferView:1,componentType:5126,count:vertexCount,type:"VEC3"},{bufferView:2,componentType:5126,count:vertexCount,type:"VEC2"},{bufferView:3,componentType:5125,count:geometry.indices.length,type:"SCALAR"}]};
  let jsonBytes=encoder.encode(JSON.stringify(json));const jsonLength=align4(jsonBytes.length),total=12+8+jsonLength+8+byteLength,out=new ArrayBuffer(total),view=new DataView(out),bytes=new Uint8Array(out);
  view.setUint32(0,0x46546c67,true);view.setUint32(4,2,true);view.setUint32(8,total,true);view.setUint32(12,jsonLength,true);view.setUint32(16,JSON_CHUNK,true);bytes.fill(0x20,20,20+jsonLength);bytes.set(jsonBytes,20);const binHeader=20+jsonLength;view.setUint32(binHeader,byteLength,true);view.setUint32(binHeader+4,BIN_CHUNK,true);for(const chunk of chunks)bytes.set(chunk.bytes,binHeader+8+chunk.offset);return out;
}
export const loadouts=Object.freeze({cubes:{label:"Procedural cubes"},spheres:{label:"Procedural spheres"},materials:{label:"PBR material gallery"}});
export async function loadDemoLoadout(id){
  if(id==="cubes")return encodeGeometryGlb(createCubeGeometry());
  if(id==="spheres")return encodeGeometryGlb(createUvSphereGeometry());
  if(id==="materials")return createMaterialGalleryGlb();
  throw new RangeError(`Unknown loadout: ${id}`);
}
