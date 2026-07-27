import test from "node:test";
import assert from "node:assert/strict";
import * as rendererModule from "../static/renderer-client.js";
const { RendererClient, RendererError } = rendererModule;

class WorkerMock extends EventTarget {
  messages=[]; transfers=[]; terminated=false;
  postMessage(message, transfer=[]) { this.messages.push(message); this.transfers.push(transfer); }
  terminate(){this.terminated=true;}
  reply(data) { this.dispatchEvent(new MessageEvent("message", { data })); }
}
function fixture() {
  const memory = new WebAssembly.Memory({initial:2, maximum:4, shared:true});
  const header = new Int32Array(memory.buffer, 0, 16);
  header.set([0x4e574159,1,1024,24,0,0]);
  const worker = new WorkerMock();
  const bridge = {memory,ringPtr:0,worker,freed:false,free(){this.freed=true;}};
  const client = new RendererClient(bridge);
  return {memory,header,worker,bridge,client};
}
async function imported(f) {
  const loading=f.client.importGlb(new ArrayBuffer(8));
  f.worker.reply({type:"payload-ready",id:1});
  await Promise.resolve();
  f.worker.reply({type:"reply",request:1,ok:true,result:{meshes:[[7,3]]}});
  return (await loading)[0];
}
test("replaceSceneGlb is opcode 1 and importGlb remains an alias", async()=>{
  for(const method of ["replaceSceneGlb","importGlb"]){const f=fixture(),pending=f.client[method](new ArrayBuffer(8));f.worker.reply({type:"payload-ready",id:1});await Promise.resolve();assert.equal(new Int32Array(f.memory.buffer,64,24)[1],1);f.worker.reply({type:"reply",request:1,ok:true,result:{meshes:[]}});assert.deepEqual(await pending,[]);}
});
test("scene replacement carries the framing mode in opcode 1",async()=>{const f=fixture(),pending=f.client.replaceSceneGlb(new ArrayBuffer(8),{framing:"interior"});f.worker.reply({type:"payload-ready",id:1});await Promise.resolve();assert.deepEqual([...new Int32Array(f.memory.buffer,64,24).slice(1,5)],[1,1,1,1]);f.worker.reply({type:"reply",request:1,ok:true,result:{meshes:[]}});await pending;await assert.rejects(f.client.replaceSceneGlb(new ArrayBuffer(8),{framing:"bad"}),TypeError)});
test("writes tagged fixed-slot protocol and resolves reply", async () => {
  const f=fixture(); const mesh=await imported(f);
  const pending=mesh.setVisible(true);
  const {memory,header,worker}=f;
  assert.equal(Atomics.load(header,5),2);
  const slot=new Int32Array(memory.buffer,64+96,24);
  assert.deepEqual([...slot.slice(0,6)],[1,2,2,7,3,1]);
  worker.reply({type:"reply",request:2,ok:true,code:"OK"}); await pending;
});
test("maps stable errors and gates destroyed instances", async () => {
  const f=fixture(), mesh=await imported(f); const {worker}=f;
  const creating=mesh.createInstance(new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]),false);
  worker.reply({type:"reply",request:2,ok:true,result:[4,2]}); const instance=await creating;
  const destroying=instance.destroy(); worker.reply({type:"reply",request:3,ok:true}); await destroying;
  assert.throws(()=>instance.setVisible(true), error=>error instanceof RendererError&&error.code==="STALE_HANDLE");
});
test("rejects protocol mismatch", () => {
  const {memory,worker}=fixture(); new Int32Array(memory.buffer)[1]=2;
  assert.throws(()=>new RendererClient({memory,ringPtr:0,worker}), /PROTOCOL_MISMATCH/);
});
test("pending reply exists before ring publication", async () => {
  const f=fixture(), mesh=await imported(f); const {worker}=f;
  const pending=mesh.setVisible(true);
  worker.reply({type:"reply",request:2,ok:true});
  await pending;
});
test("profile snapshots have a dedicated getter", () => {
  const f=fixture(), snapshot={type:"profile-snapshot",available:true,epoch:3,passes:{forward:1.25}};
  const dispatch=globalThis.dispatchEvent; globalThis.dispatchEvent=()=>true;
  try { f.worker.reply(snapshot); assert.strictEqual(f.client.profile,snapshot); }
  finally { globalThis.dispatchEvent=dispatch; f.client.dispose(); }
});
test("worker failures and dispose reject every pending operation", async () => {
  const f=fixture(), mesh=await imported(f); const {worker,client,bridge}=f;
  const a=mesh.setVisible(true), b=mesh.setVisible(false);
  worker.dispatchEvent(new Event("error"));
  await assert.rejects(a,/WORKER_ERROR/); await assert.rejects(b,/WORKER_ERROR/);
  client.dispose(); assert.equal(worker.terminated,true); assert.equal(bridge.freed,true);
});
test("import always releases staged payload when ring is full", async () => {
  const {header,worker,client}=fixture(); Atomics.store(header,5,1024);
  const loading=client.importGlb(new ArrayBuffer(8));
  worker.reply({type:"payload-ready",id:1});
  await assert.rejects(loading,/RING_FULL/);
  assert.equal(worker.messages.at(-1).type,"payload-release");
});
test("does not export handle constructors or internal mutation methods", () => {
  const {client}=fixture();
  assert.equal(rendererModule.Mesh,undefined);
  assert.equal(rendererModule.Instance,undefined);
  assert.equal(client._meshFlags,undefined);
  assert.equal(client._createInstance,undefined);
});
test("corrupt backlog closes and terminates the transport", async () => {
  const f=fixture(); Atomics.store(f.header,5,1025);
  const loading=f.client.importGlb(new ArrayBuffer(8));
  // Payload staging must first acknowledge before enqueue sees corruption.
  f.worker.reply({type:"payload-ready",id:1});
  await assert.rejects(loading,/RING_CORRUPT/);
  assert.equal(Atomics.load(f.header,6),1);
  assert.equal(f.worker.terminated,true);
});
test("import rejects immediately after disposal", async () => {
  const f=fixture();
  f.client.dispose();
  await assert.rejects(f.client.importGlb(new ArrayBuffer(8)),/DISPOSED/);
  assert.equal(f.worker.messages.length,0);
});
test("import rejects when disposed during asynchronous source loading", async () => {
  const f=fixture();
  const originalFetch=globalThis.fetch;
  let finishFetch;
  globalThis.fetch=()=>new Promise(resolve=>{finishFetch=resolve;});
  try {
    const loading=f.client.importGlb("model.glb");
    f.client.dispose();
    finishFetch({arrayBuffer:async()=>new ArrayBuffer(8)});
    await assert.rejects(loading,/DISPOSED/);
    assert.equal(f.worker.messages.length,0);
  } finally {
    globalThis.fetch=originalFetch;
  }
});

test("compile transfers payload and waits for ready before opcode 7", async()=>{
  const f=fixture(), pending=f.client.compileGraph({schemaVersion:2});
  assert.equal(f.worker.transfers[0].length,1); assert.equal(Atomics.load(f.header,5),0);
  f.worker.reply({type:"payload-ready",id:1}); await Promise.resolve();
  assert.equal(new Int32Array(f.memory.buffer,64,24)[1],7);
  f.worker.reply({type:"reply",request:1,ok:true,result:{compiledId:[2,3]}});
  assert.deepEqual(await pending,{compiledId:[2,3]});
});
test("flat error reply preserves structured details", async()=>{
  const f=fixture(), pending=f.client.compileGraph({}); f.worker.reply({type:"payload-ready",id:1}); await Promise.resolve();
  f.worker.reply({type:"reply",request:1,ok:false,code:"GRAPH_INVALID_ID",details:{message:"bad",path:"graphId"}});
  await assert.rejects(pending,e=>e instanceof RendererError&&e.code==="GRAPH_INVALID_ID"&&e.details.path==="graphId"&&e.message==="bad");
});
test("compile releases payload after success", async()=>{const f=fixture(),p=f.client.compileGraph({});f.worker.reply({type:"payload-ready",id:1});await Promise.resolve();f.worker.reply({type:"reply",request:1,ok:true,result:{}});await p;assert.equal(f.worker.messages.at(-1).type,"payload-release");});
test("compile releases payload after backend error", async()=>{const f=fixture(),p=f.client.compileGraph({});f.worker.reply({type:"payload-ready",id:1});await Promise.resolve();f.worker.reply({type:"reply",request:1,ok:false,code:"X",details:{message:"x"}});await assert.rejects(p);assert.equal(f.worker.messages.at(-1).type,"payload-release");});
test("compile rejects circular and BigInt JSON", async()=>{const f=fixture(),x={};x.x=x;await assert.rejects(f.client.compileGraph(x),/circular/i);await assert.rejects(f.client.compileGraph({x:1n}),e=>e.code==="GRAPH_JSON_INVALID");});
test("compile rejects oversized encoding", async()=>{const f=fixture();await assert.rejects(f.client.compileGraph({x:"x".repeat(1024*1024)}),e=>e.code==="GRAPH_PAYLOAD_TOO_LARGE");assert.equal(f.worker.messages.length,0);});
test("drop graph emits opcode 8 and validates id", async()=>{const f=fixture(),p=f.client.dropCompiledGraph([9,4]);const slot=new Int32Array(f.memory.buffer,64,24);assert.deepEqual([...slot.slice(1,5)],[8,1,9,4]);f.worker.reply({type:"reply",request:1,ok:true});await p;assert.throws(()=>f.client.dropCompiledGraph([1]),TypeError);});
test("ring-full graph compile releases staged payload", async()=>{const f=fixture();Atomics.store(f.header,5,1024);const p=f.client.compileGraph({});f.worker.reply({type:"payload-ready",id:1});await assert.rejects(p,e=>e.code==="RING_FULL");assert.equal(f.worker.messages.at(-1).type,"payload-release");});
test("disposal while graph payload is pending releases and rejects", async()=>{const f=fixture(),p=f.client.compileGraph({});f.client.dispose();await assert.rejects(p,e=>e.code==="DISPOSED");assert.equal(f.worker.messages.at(-1).type,"payload-release");});
test("payload transfer uses the exact encoded ArrayBuffer", async()=>{const f=fixture(),graph={schemaVersion:2};const expected=new TextEncoder().encode(JSON.stringify(graph));const p=f.client.compileGraph(graph);const transferred=f.worker.transfers[0][0];assert.strictEqual(f.worker.messages[0].buffer,transferred);assert.deepEqual(new Uint8Array(transferred),expected);f.client.dispose();await assert.rejects(p);});
test("cycle error details are preserved exactly", async()=>{const f=fixture(),p=f.client.compileGraph({});f.worker.reply({type:"payload-ready",id:1});await Promise.resolve();const details={message:"cycle",kind:"cycle",edges:[{from:"a",resource:{id:"r",version:0},to:"b"}]};f.worker.reply({type:"reply",request:1,ok:false,code:"GRAPH_CYCLE",details});await assert.rejects(p,e=>e.details===details&&e.details.edges[0].from==="a");});
test("error without details leaves details undefined", async()=>{const f=fixture(),p=f.client.dropCompiledGraph([1,1]);f.worker.reply({type:"reply",request:1,ok:false,code:"STALE_GRAPH_ID"});await assert.rejects(p,e=>e instanceof RendererError&&e.details===undefined&&e.message==="STALE_GRAPH_ID");});
test("switch methods emit exact opcode 9 words", async()=>{const f=fixture();const a=f.client.switchCompiledGraph([9,4]);assert.deepEqual([...new Int32Array(f.memory.buffer,64,24).slice(1,7)],[9,1,1,9,4,0]);f.worker.reply({type:"reply",request:1,ok:true});await a;await new Promise(queueMicrotask);const b=f.client.switchToImmediate();assert.deepEqual([...new Int32Array(f.memory.buffer,160,24).slice(1,7)],[9,2,0,0,0,0]);f.worker.reply({type:"reply",request:2,ok:true});await b;assert.throws(()=>f.client.switchCompiledGraph([0,0]),TypeError);});
test("graph lifecycle FIFO recovers after failure", async()=>{const f=fixture();const a=f.client.switchCompiledGraph([1,1]),b=f.client.switchToImmediate();assert.equal(Atomics.load(f.header,5),1);f.worker.reply({type:"reply",request:1,ok:false,code:"X"});await assert.rejects(a);await new Promise(queueMicrotask);assert.equal(Atomics.load(f.header,5),2);f.worker.reply({type:"reply",request:2,ok:true});await b;});
test("dispose rejects queued graph lifecycle calls", async()=>{const f=fixture();const a=f.client.switchCompiledGraph([1,1]),b=f.client.switchToImmediate();f.client.dispose();await assert.rejects(a,/DISPOSED/);await assert.rejects(b,/DISPOSED/);});
