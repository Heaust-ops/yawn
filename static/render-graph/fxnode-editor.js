import { createFxNode } from "@fxnode/index.ts";
import { CATALOG_VERSION, GRAPH_ID, nodeDefinitions, socketTypes, styles, theme } from "./catalog.js";
import { prepareBrowserHost } from "./browser-host.js";

const spec=[
  ["surface","surface_color",{x:40,y:100}],
  ["depth","depth32",{x:40,y:330}],
  ["forward","scene_forward",{x:360,y:190}],
  ["present","present",{x:700,y:220}],
];
async function seed(root){
  await root.setState({graphId:GRAPH_ID,catalogVersion:CATALOG_VERSION,nodes:[],links:[],metadata:{}});
  for(const [nodeId,nodeType,position] of spec) await root.dispatch({type:"node.add",nodeId,nodeType,position});
  for(const link of [
    {id:"surface_link",fromNodeId:"surface",fromSocketId:"surface:surface",toNodeId:"forward",toSocketId:"forward:color",muted:false,extensions:{}},
    {id:"depth_link",fromNodeId:"depth",fromSocketId:"depth:depth",toNodeId:"forward",toSocketId:"forward:depth",muted:false,extensions:{}},
    {id:"present_link",fromNodeId:"forward",fromSocketId:"forward:result",toNodeId:"present",toSocketId:"present:surface",muted:false,extensions:{}},
  ]) await root.dispatch({type:"link.add",link});
}
export async function createRenderGraphEditor(canvas){
  const chooseNodeType=()=>{const value=canvas.ownerDocument.defaultView?.prompt(`Node type: ${Object.keys(nodeDefinitions).join(", ")}`,"scene_forward");return Object.hasOwn(nodeDefinitions,value)?value:null};
  const host=prepareBrowserHost(canvas,{chooseNodeType});let root,view,destroying;
  const destroy=()=>destroying??=(async()=>{host.destroy();try{await view?.detach()}finally{root?.destroy();view=undefined;root=undefined}})();
  try{root=await createFxNode({applicationId:"yawn.render-graph",applicationVersion:1,resources:{}});await root.setTheme(theme);await root.setHeaderStyles(styles);for(const entry of Object.entries(socketTypes))await root.composeSocket(...entry);for(const entry of Object.entries(nodeDefinitions))await root.composeNode(...entry);await seed(root);view=await root.attachView({canvas,viewport:host.initialViewport,initialCamera:{center:{x:470,y:210},zoom:.5}});host.attach(root,view);await view.whenRendered();return {getState:()=>root.getState(),onSnapshots:fn=>root.onSnapshots(fn),whenRendered:()=>view.whenRendered(),destroy};}catch(e){await destroy().catch(()=>{});throw e}
}
