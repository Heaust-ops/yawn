export const GRAPH_ID = "demo_forward";
export const CATALOG_VERSION = 1;

export const socketTypes = {
  surface: { title: "Surface", color: "#62b0ff", acceptsFrom: ["surface"] },
  depth: { title: "Depth", color: "#b58cff", acceptsFrom: ["depth"] },
};
export const theme = {
  background:"#151820",grid:"#292e3a",frame:"#30343a80",frameHeader:"#59616c",body:"#292e39",control:"#191d26",controlFill:"#4775b8",controlEditing:"#101218",textSelection:"#4775b8",outline:"#0b0d12",text:"#edf1f7",muted:"#969eaa",shadow:"#00000088",nodeSelected:"#ff9f43",nodeActive:"#ffffff",unknownHeader:"#555b64",unknownSocket:"#999999",linkMuted:"#d94b4b",knifeMuted:"#e85b5b",emphasis:"#ffffff",focus:"#f5a623",editOutline:"#666a70",resize:"#8b8e95",muteOverlay:"#14141459",boxSelectionFill:"#f5a6231f",checkerLight:"#aaaaaa",checkerDark:"#777777",widgetBorder:"#111216",rampBorder:"#111111",resourceBackground:"#202228"
};
export const styles = { resource:{header:"#3977a8"}, pass:{header:"#426b43"}, output:{header:"#a75d37"} };
const socket = (title, direction, type) => ({ title,direction,type,maxIncomingLinks:direction === "input" ? 1 : 0,visible:true,value:null,showValue:false });
const node = (title, style, sockets, parameters = {}) => ({ version:1,title,behavior:"standard",style,parameters,sockets,ui:[...Object.keys(parameters).map(parameter=>({kind:"parameter",parameter})),...Object.keys(sockets).map(socket=>({kind:"socket",socket}))],muteBypass:[],migrations:[] });
export const nodeDefinitions = {
  surface_color: node("Surface Color", "resource", { surface:socket("Surface","output","surface") }),
  depth32: node("Depth 32", "resource", { depth:socket("Depth","output","depth") }),
  scene_forward: node("Scene Forward", "pass", { color:socket("Color","input","surface"),depth:socket("Depth","input","depth"),result:socket("Result","output","surface") }, {
    clearColor:{type:"color",default:{kind:"color",value:[0,0,0,1]},minimum:0,maximum:1},
    clearDepth:{type:"number",default:{kind:"number",value:1},minimum:0,maximum:1,step:0.01},
  }),
  present: node("Present", "output", { surface:socket("Surface","input","surface") }),
};

export const descriptors = Object.freeze({
  surface_color:{version:1,sockets:{surface:["output","surface"]},parameters:[]},
  depth32:{version:1,sockets:{depth:["output","depth"]},parameters:[]},
  scene_forward:{version:1,sockets:{color:["input","surface"],depth:["input","depth"],result:["output","surface"]},parameters:["clearColor","clearDepth"]},
  present:{version:1,sockets:{surface:["input","surface"]},parameters:[]},
});
