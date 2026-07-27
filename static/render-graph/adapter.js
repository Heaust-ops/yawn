import { CATALOG_VERSION, descriptors, GRAPH_ID } from "./catalog.js";

export class AuthoringGraphError extends Error {
  constructor(code, details = {}) { super(code); this.name="AuthoringGraphError"; this.code=code; this.details=Object.freeze(details); }
}
const fail=(code,details)=>{throw new AuthoringGraphError(code,details)};
const object=v=>v !== null && typeof v === "object" && !Array.isArray(v);
const validId=v=>typeof v === "string" && /^[A-Za-z][A-Za-z0-9_.-]*$/.test(v) && new TextEncoder().encode(v).length<=64;
const validSocketId=v=>typeof v === "string" && /^[A-Za-z][A-Za-z0-9_.-]*:[A-Za-z][A-Za-z0-9_.-]*$/.test(v) && new TextEncoder().encode(v).length<=129;
const keysEqual=(a,b)=>a.length===b.length && a.every(x=>b.includes(x));

/** Validate hostile fxnode state and return an app-owned, layout-free projection. */
export function projectAuthoringSnapshot(raw) {
  if(!object(raw)||!Array.isArray(raw.nodes)||!Array.isArray(raw.links)) fail("AUTHORING_SHAPE",{field:"snapshot"});
  if(raw.graphId!==GRAPH_ID||raw.catalogVersion!==CATALOG_VERSION) fail("AUTHORING_CATALOG",{graphId:raw.graphId,catalogVersion:raw.catalogVersion});
  const nodeIds=new Set(), socketIds=new Set(), byId=new Map(), byType=new Map();
  for(const n of raw.nodes){
    if(!object(n)||!validId(n.id)) fail("AUTHORING_ID",{kind:"node",id:n?.id});
    if(nodeIds.has(n.id)) fail("AUTHORING_ID_DUPLICATE",{kind:"node",id:n.id}); nodeIds.add(n.id);
    const d=descriptors[n.typeId];
    if(!d) fail("AUTHORING_NODE_TYPE",{nodeId:n.id,typeId:n.typeId});
    if(n.known!==true) fail("AUTHORING_NODE_UNKNOWN",{nodeId:n.id});
    if(n.typeVersion!==d.version) fail("AUTHORING_NODE_VERSION",{nodeId:n.id,expected:d.version,actual:n.typeVersion});
    if(typeof n.muted!=="boolean") fail("AUTHORING_NODE_MUTED",{nodeId:n.id});
    if(n.muted&&n.typeId!=="scene_forward") fail("AUTHORING_NODE_MUTED",{nodeId:n.id});
    if(byType.has(n.typeId)) fail("AUTHORING_TOPOLOGY",{reason:"duplicate-type",typeId:n.typeId});
    if(!Array.isArray(n.sockets)||!keysEqual(n.sockets.map(s=>s?.key),Object.keys(d.sockets))) fail("AUTHORING_SOCKET_SET",{nodeId:n.id});
    const sockets={};
    for(const s of n.sockets){ const expected=d.sockets[s.key];
      if(!object(s)||!validSocketId(s.id)) fail("AUTHORING_ID",{kind:"socket",id:s?.id});
      if(socketIds.has(s.id)) fail("AUTHORING_ID_DUPLICATE",{kind:"socket",id:s.id}); socketIds.add(s.id);
      if(s.direction!==expected[0]||s.dataType!==expected[1]) fail("AUTHORING_SOCKET",{nodeId:n.id,socket:s.key});
      sockets[s.key]={id:s.id,direction:s.direction,type:s.dataType,nodeId:n.id};
    }
    const p=object(n.parameters)?n.parameters:null;
    if(!p||!keysEqual(Object.keys(p),d.parameters)) fail("AUTHORING_PARAMETERS",{nodeId:n.id});
    let parameters={};
    if(n.typeId==="scene_forward"){
      const c=p.clearColor, z=p.clearDepth;
      if(!object(c)||c.kind!=="color"||!Array.isArray(c.value)||c.value.length!==4||c.value.some(v=>!Number.isFinite(v)||v<0||v>1)) fail("AUTHORING_PARAMETER",{parameter:"clearColor"});
      if(!object(z)||z.kind!=="number"||!Number.isFinite(z.value)||z.value<0||z.value>1) fail("AUTHORING_PARAMETER",{parameter:"clearDepth"});
      parameters={clearColor:[...c.value],clearDepth:z.value};
    }
    const projected={type:n.typeId,id:n.id,sockets,parameters,muted:n.muted}; byId.set(n.id,projected); byType.set(n.typeId,projected);
  }
  if(!keysEqual([...byType.keys()],Object.keys(descriptors))) fail("AUTHORING_TOPOLOGY",{reason:"node-set"});
  const linkIds=new Set(), incoming=new Set(), links=[];
  for(const l of raw.links){
    if(!object(l)||!validId(l.id)) fail("AUTHORING_ID",{kind:"link",id:l?.id});
    if(linkIds.has(l.id)) fail("AUTHORING_ID_DUPLICATE",{kind:"link",id:l.id}); linkIds.add(l.id);
    if(typeof l.muted!=="boolean") fail("AUTHORING_LINK",{linkId:l.id,reason:"muted"});
    const from=byId.get(l.fromNodeId),to=byId.get(l.toNodeId),fs=from&&Object.values(from.sockets).find(s=>s.id===l.fromSocketId),ts=to&&Object.values(to.sockets).find(s=>s.id===l.toSocketId);
    if(!fs||!ts||fs.direction!=="output"||ts.direction!=="input"||fs.type!==ts.type) fail("AUTHORING_LINK",{linkId:l.id});
    if(incoming.has(ts.id)) fail("AUTHORING_LINK_INCOMING",{socketId:ts.id}); incoming.add(ts.id);
    links.push({from:`${from.type}.${Object.keys(from.sockets).find(k=>from.sockets[k]===fs)}`,to:`${to.type}.${Object.keys(to.sockets).find(k=>to.sockets[k]===ts)}`,muted:l.muted});
  }
  const required=["surface_color.surface>scene_forward.color","depth32.depth>scene_forward.depth","scene_forward.result>present.surface"];
  const active=links.filter(l=>!l.muted).map(l=>`${l.from}>${l.to}`).sort();
  if(!keysEqual(active,required.sort())||links.length!==3) fail("AUTHORING_TOPOLOGY",{reason:"links"});
  return Object.freeze({graphId:GRAPH_ID,clearColor:byType.get("scene_forward").parameters.clearColor,clearDepth:byType.get("scene_forward").parameters.clearDepth,passState:byType.get("scene_forward").muted?"disabled":"enabled"});
}

export function semanticProjectionToV1(p, revision=1){
  if(!Number.isInteger(revision)||revision<1||revision>0xffffffff) fail("AUTHORING_REVISION",{revision});
  const extent={kind:"surface_relative",width:{numerator:1,denominator:1},height:{numerator:1,denominator:1},depthOrArrayLayers:1};
  return {schemaVersion:1,graphId:p.graphId,revision,resources:[
    {id:"surface",version:0,residency:{kind:"external",source:"surface_color"},texture:{dimension:"d2",format:"surface",extent,mipLevelCount:1,sampleCount:1}},
    {id:"depth",version:0,residency:{kind:"transient"},texture:{dimension:"d2",format:"depth32_float",extent,mipLevelCount:1,sampleCount:1}},
  ],passes:[{id:"forward",state:p.passState,executor:{key:"scene_forward",version:1},parameters:{},reads:[],writes:[
    {binding:"color",resource:{id:"surface",version:0},access:{kind:"color_attachment",location:0,load:{op:"clear",value:p.clearColor},store:"store"}},
    {binding:"depth",resource:{id:"depth",version:0},access:{kind:"depth_attachment",load:{op:"clear",value:p.clearDepth},store:"store"}},
  ]}],outputs:[{name:"present",resource:{id:"surface",version:0}}]};
}
export const adaptFxNodeSnapshot=(snapshot,revision=1)=>semanticProjectionToV1(projectAuthoringSnapshot(snapshot),revision);
export const adaptGraphSnapshot=adaptFxNodeSnapshot;
