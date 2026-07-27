import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

const root = new URL("..", import.meta.url).pathname;
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
  exports: Record<string, unknown>;
};
assert.deepEqual(
  Object.keys(packageJson.exports).sort(),
  [".", "./headless", "./widgets/color-ramp"].sort(),
  "package exports must be exact",
);
const run = (command: string, args: string[], cwd: string) =>
  execFileSync(command, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
run("npm", ["run", "build"], root);
const declarationSurfaces = {
  "dist/index.d.ts": [
    "AddNodeParams",
    "AnyNodeParameterId",
    "AnyNodeSocketId",
    "BatchCommand",
    "Command",
    "CommandId",
    "CommandIntent",
    "CommandReceipt",
    "CompiledFxNodeComposition",
    "CompiledNode",
    "CompiledResource",
    "ComposedNode",
    "ComposedSocket",
    "CompositionChange",
    "CompositionChangeEnvelope",
    "CompositionReceipt",
    "CompositionUpdateOptions",
    "CreateFxNodeOptions",
    "DEFAULT_FXNODE_THEME",
    "FXNODE_COMPOSITION_LIMITS",
    "FXNODE_VIEW_LIMITS",
    "FxNode",
    "FxNodeActionOptions",
    "FxNodeAddNodeMenuRequest",
    "FxNodeCapabilityError",
    "FxNodeComposition",
    "FxNodeCompositionData",
    "FxNodeCompositionError",
    "FxNodeCompositionIssue",
    "FxNodeCompositionSeed",
    "FxNodeCompositionValidation",
    "FxNodeDefinition",
    "FxNodeDestroyedError",
    "FxNodeCamera",
    "FxNodeGradingBinding",
    "FxNodeHexColor",
    "FxNodeHostRequest",
    "FxNodeHostSnapshot",
    "FxNodeImageResourceDefinition",
    "FxNodeImageResourceDescriptor",
    "FxNodeInput",
    "FxNodeIssue",
    "FxNodeJsonValue",
    "FxNodeMigration",
    "FxNodeMigrationStep",
    "FxNodeModifiers",
    "FxNodeParameterValue",
    "FxNodeProtocolError",
    "FxNodeReadonlyMap",
    "FxNodeReplayCommand",
    "FxNodeResourceAuthorization",
    "FxNodeResourceData",
    "FxNodeResourceDefinition",
    "FxNodeResourceOpenRequest",
    "FxNodeSaveData",
    "FxNodeSelectionSnapshot",
    "FxNodeSocketDefinition",
    "FxNodeSocketTypeDefinition",
    "FxNodeStyleDefinition",
    "FxNodeTheme",
    "FxNodeUiRow",
    "FxNodeValueSchema",
    "FxNodeViewport",
    "FxNodeView",
    "FxNodeViewOptions",
    "FxNodeViewDetachedError",
    "FxNodeVisibility",
    "FxNodeWorkerError",
    "GraphDocument",
    "GraphId",
    "GraphLayoutNodeV1",
    "GraphLayoutV1",
    "GraphLayoutV2",
    "GraphLink",
    "GraphLinkV1",
    "GraphNode",
    "GraphSnapshot",
    "GraphState",
    "HeaderStyled",
    "JsonValue",
    "KnownNode",
    "LinkId",
    "Mutation",
    "MutationEnvelope",
    "NodeBase",
    "NodeId",
    "NodeParameterId",
    "NodeSocketId",
    "NodeStyleId",
    "NodeTypeId",
    "ParameterValue",
    "RemovedNode",
    "RemovedSocket",
    "ResourceId",
    "SnapshotEnvelope",
    "Socket",
    "SocketDataType",
    "SocketId",
    "SocketTypeId",
    "Themed",
    "UnknownNode",
    "Vec2",
    "commandId",
    "compileFxNodeComposition",
    "composeNode",
    "composeSocket",
    "createFxNode",
    "createInitialFxNodeComposition",
    "graphId",
    "fxNodeDevicePixels",
    "linkId",
    "nodeId",
    "removeNode",
    "removeSocket",
    "setHeaderStyles",
    "setTheme",
    "socketId",
    "validateFxNodeComposition",
  ],
  "dist/headless.d.ts": [
    "AnyNodeParameterId",
    "AnyNodeSocketId",
    "BatchCommand",
    "Command",
    "CommandError",
    "CommandId",
    "CommandRequest",
    "CompatibleFxNodeSaveData",
    "CompiledFxNodeComposition",
    "CompiledNode",
    "CompiledResource",
    "ComposedNode",
    "ComposedSocket",
    "DEFAULT_FXNODE_THEME",
    "DecodeResult",
    "EngineState",
    "FXNODE_COMPOSITION_LIMITS",
    "FXNODE_SAVE_DATA_LIMITS",
    "FxNodeComposition",
    "FxNodeCompositionData",
    "FxNodeCompositionError",
    "FxNodeCompositionIssue",
    "FxNodeCompositionSeed",
    "FxNodeCompositionValidation",
    "FxNodeDefinition",
    "FxNodeGradingBinding",
    "FxNodeHeadless",
    "FxNodeHexColor",
    "FxNodeImageResourceDefinition",
    "FxNodeJsonValue",
    "FxNodeMigration",
    "FxNodeMigrationStep",
    "FxNodeParameterValue",
    "FxNodeReadonlyMap",
    "FxNodeReplayCommand",
    "FxNodeResourceDefinition",
    "FxNodeSaveData",
    "FxNodeSocketDefinition",
    "FxNodeSocketTypeDefinition",
    "FxNodeStyleDefinition",
    "FxNodeTheme",
    "FxNodeUiRow",
    "FxNodeValueSchema",
    "FxNodeVisibility",
    "GraphDocument",
    "GraphId",
    "GraphLayoutNodeV1",
    "GraphLayoutV1",
    "GraphLayoutV2",
    "GraphLink",
    "GraphLinkV1",
    "GraphNode",
    "GraphSnapshot",
    "GraphState",
    "HeaderStyled",
    "JsonValue",
    "KnownNode",
    "LinkId",
    "LoadResult",
    "Mutation",
    "MutationEnvelope",
    "NodeBase",
    "NodeId",
    "NodeParameterId",
    "NodeSocketId",
    "NodeStyleId",
    "NodeTypeId",
    "ParameterValue",
    "RemovedNode",
    "RemovedSocket",
    "ReplayResult",
    "ResourceId",
    "SnapshotEnvelope",
    "Socket",
    "SocketDataType",
    "SocketId",
    "SocketTypeId",
    "StateReplacementRequest",
    "Themed",
    "TransitionResult",
    "UnknownNode",
    "ValidationIssue",
    "Vec2",
    "commandId",
    "compileFxNodeComposition",
    "composeNode",
    "composeSocket",
    "createFxNodeHeadless",
    "createInitialFxNodeComposition",
    "graphId",
    "linkId",
    "nodeId",
    "removeNode",
    "removeSocket",
    "setHeaderStyles",
    "setTheme",
    "socketId",
    "validateFxNodeComposition",
  ],
  "dist/widgets/color-ramp.d.ts": [
    "ColorRamp",
    "ColorRampInterpolation",
    "ColorRampMode",
    "ColorRampStop",
    "HueInterpolation",
    "addRampMidpoint",
    "addRampStop",
    "distributeColorRamp",
    "flipColorRamp",
    "isColorRamp",
    "migrateColorRamp",
    "moveRampStop",
    "removeRampStop",
    "sampleColorRamp",
    "selectRampStop",
    "setRampColor",
  ],
} as const;
const declarationProgram = ts.createProgram(
  Object.keys(declarationSurfaces).map((file) => join(root, file)),
  {
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
  },
);
const declarationChecker = declarationProgram.getTypeChecker();
for (const [file, expected] of Object.entries(declarationSurfaces)) {
  const source = declarationProgram.getSourceFile(join(root, file));
  assert(source, `missing declaration entrypoint: ${file}`);
  const moduleSymbol = declarationChecker.getSymbolAtLocation(source);
  assert(moduleSymbol, `declaration entrypoint is not a module: ${file}`);
  const actual = declarationChecker
    .getExportsOfModule(moduleSymbol)
    .map((symbol) => symbol.name)
    .sort();
  assert.deepEqual(actual, [...expected].sort(), `declaration exports changed for ${file}`);
}
const builtFiles = readdirSync(join(root, "dist"), { recursive: true })
  .map(String)
  .filter((file) => /\.(?:js|d\.ts)$/.test(file));
const builtText = builtFiles.map((file) => readFileSync(join(root, "dist", file), "utf8")).join("\n");
const runtimeSurfaces = {
  ".": [
    "DEFAULT_FXNODE_THEME",
    "FXNODE_COMPOSITION_LIMITS",
    "FXNODE_VIEW_LIMITS",
    "FxNodeCapabilityError",
    "FxNodeCompositionError",
    "FxNodeDestroyedError",
    "FxNodeViewDetachedError",
    "FxNodeProtocolError",
    "FxNodeWorkerError",
    "commandId",
    "compileFxNodeComposition",
    "composeNode",
    "composeSocket",
    "createFxNode",
    "createInitialFxNodeComposition",
    "graphId",
    "fxNodeDevicePixels",
    "linkId",
    "nodeId",
    "removeNode",
    "removeSocket",
    "setHeaderStyles",
    "setTheme",
    "socketId",
    "validateFxNodeComposition",
  ],
  "./headless": [
    "DEFAULT_FXNODE_THEME",
    "FXNODE_COMPOSITION_LIMITS",
    "FXNODE_SAVE_DATA_LIMITS",
    "FxNodeCompositionError",
    "commandId",
    "compileFxNodeComposition",
    "composeNode",
    "composeSocket",
    "createFxNodeHeadless",
    "createInitialFxNodeComposition",
    "graphId",
    "linkId",
    "nodeId",
    "removeNode",
    "removeSocket",
    "setHeaderStyles",
    "setTheme",
    "socketId",
    "validateFxNodeComposition",
  ],
  "./widgets/color-ramp": [
    "addRampMidpoint",
    "addRampStop",
    "distributeColorRamp",
    "flipColorRamp",
    "isColorRamp",
    "migrateColorRamp",
    "moveRampStop",
    "removeRampStop",
    "sampleColorRamp",
    "selectRampStop",
    "setRampColor",
  ],
} as const;
for (const [entry, expected] of Object.entries(runtimeSurfaces)) {
  const target = packageJson.exports[entry] as { import: string };
  const actual = Object.keys(await import(join(root, target.import))).sort();
  assert.deepEqual(actual, [...expected].sort(), `runtime exports changed for ${entry}`);
}
for (const symbol of [
  "LEGACY_COMPOSITION",
  "INTERNAL_LEGACY_HEADLESS",
  "BUILTIN_DESCRIPTORS",
  "CATALOG_NODE_IDS",
  "BuiltinNodeTypeId",
  "DESCRIPTOR_REGISTRY",
  "BLENDER_DARK_THEME",
  "getDescriptor",
])
  assert(!builtText.includes(symbol), `obsolete concrete symbol built: ${symbol}`);
assert(!builtText.includes("defineFxNodeComposition"), "removed composition identity helper was built");
assert(
  !builtFiles.some((file) => /^composition\/define\.(?:js|d\.ts)$/.test(file)),
  "stale composition/define output was built",
);
const temporary = mkdtempSync(join(tmpdir(), "fxnode-package-"));
try {
  const json = JSON.parse(run("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], root)) as {
    files: { path: string }[];
  }[];
  const files = json[0]?.files.map((item) => item.path) ?? [];
  assert(
    files.includes("dist/index.js") &&
      files.includes("dist/headless.js") &&
      files.includes("dist/index.d.ts") &&
      files.includes("LICENSE") &&
      files.includes("NOTICE.md"),
  );
  assert(
    !files.some((file) => /(^|\/)(\.env|src|test|tools|playwright)(\/|\.|$)/i.test(file)),
    `forbidden package file: ${files.join(", ")}`,
  );
  assert(!files.some((file) => /^docs\//.test(file)), "documentation must not be packed");
  assert(
    !files.some((file) => /dist\/(catalog|core\/document|engine\/engine|render\/theme)(\/|\.|$)/.test(file)),
    "obsolete compatibility module was built",
  );
  assert(!files.some((file) => /dist\/browser\/add-node-menu\./.test(file)), "host-owned add-node menu was built");
  const packed = run("npm", ["pack", "--json", "--ignore-scripts", "--pack-destination", temporary], root);
  const tarball = join(temporary, (JSON.parse(packed) as { filename: string }[])[0]!.filename);
  const consumer = join(temporary, "consumer");
  run("mkdir", [consumer], temporary);
  writeFileSync(
    join(consumer, "package.json"),
    JSON.stringify({
      private: true,
      type: "module",
      dependencies: { fxnode: `file:${tarball}`, typescript: "5.7.2", vite: "6.1.0" },
    }),
  );
  run("npm", ["install", "--ignore-scripts"], consumer);
  writeFileSync(
    join(consumer, "ramp.mjs"),
    "import { isColorRamp } from 'fxnode/widgets/color-ramp'; if (typeof isColorRamp !== 'function') throw new Error('color ramp export missing');\n",
  );
  run("node", ["ramp.mjs"], consumer);
  writeFileSync(
    join(consumer, "headless.mjs"),
    "import * as fxnode from 'fxnode/headless'; if (!fxnode.createFxNodeHeadless || !fxnode.compileFxNodeComposition || !fxnode.compileFxNodeComposition || fxnode.bindFxNodeHeadless || fxnode.createEngine || fxnode.materializeNode || fxnode.socketsCompatible) throw new Error('headless export boundary invalid');\n",
  );
  run("node", ["headless.mjs"], consumer);
  writeFileSync(
    join(consumer, "main.ts"),
    `import { createFxNode, compileFxNodeComposition, type CompositionChange, type CompositionChangeEnvelope, type CompositionReceipt, type CompositionUpdateOptions, type FxNode, type FxNodeSaveData, type FxNodeView, type FxNodeViewOptions, type NodeTypeId } from 'fxnode';
import { createFxNodeHeadless } from 'fxnode/headless';
// @ts-expect-error color ramps have a dedicated entrypoint
import { isColorRamp } from 'fxnode';
// @ts-expect-error protocol-only version expectation is not public
import type { VersionExpectation } from 'fxnode';
// @ts-expect-error implementation-bound state names are not public
import type { BoundEngineState } from 'fxnode/headless';
type Equal<A,B> = (<T>()=>T extends A?1:2) extends (<T>()=>T extends B?1:2) ? true : false;
type Expect<T extends true> = T;
const color='#000000' as const, theme={background:color,grid:color,frame:color,frameHeader:color,body:color,control:color,controlFill:color,controlEditing:color,textSelection:color,outline:color,text:color,muted:color,shadow:color,nodeSelected:color,nodeActive:color,unknownHeader:color,unknownSocket:color,linkMuted:color,knifeMuted:color,emphasis:color,focus:color,editOutline:color,resize:color,muteOverlay:color,boxSelectionFill:color,checkerLight:color,checkerDark:color,widgetBorder:color,rampBorder:color,resourceBackground:color};
const composition={schemaVersion:2,id:'consumer',version:1,compatibility:{wildcardInputTypes:[]},theme,socketTypes:{value:{title:'Value',color,acceptsFrom:['value']}},nodeStyles:{basic:{header:color}},resources:{},nodes:{tiny:{version:1,title:'Tiny',behavior:'standard',style:'basic',parameters:{},sockets:{},ui:[],muteBypass:[],migrations:[]}}} as const;
type InferredNode = Expect<Equal<NodeTypeId<typeof composition>,'tiny'>>;
const compiled=compileFxNodeComposition(composition); compiled.nodes.get('tiny');
// @ts-expect-error packed declarations retain exact node IDs
compiled.nodes.get('missing');
const runtime=createFxNodeHeadless(composition); runtime.materializeNode('n','tiny');
// @ts-expect-error complete compilation rejects unknown style references
compileFxNodeComposition({...composition,nodes:{tiny:{...composition.nodes.tiny,style:'missing'}}});
async function liveCompositionSmoke(api:FxNode,options:CompositionUpdateOptions):Promise<void>{
  const canvas=null as unknown as HTMLCanvasElement,viewOptions:FxNodeViewOptions={canvas,viewport:{width:1,height:1,dpr:1}};
  const view:FxNodeView=await api.attachView(viewOptions);view.id;view.feedInput({kind:'focus',phase:'focus'});await view.detach();
  // @ts-expect-error view handles do not own graph authority
  view.dispatch({type:'undo'});
  // @ts-expect-error root authority does not own presentation input
  api.feedInput({kind:'focus',phase:'focus'});
  // @ts-expect-error removed copy API
  api.copyTo(canvas);
  // @ts-expect-error removed mirror API
  api.addMirror(canvas);
  const saveData:FxNodeSaveData=await api.getSaveData();await api.load(saveData);
  const before=await api.getState(),known=before.nodes.find(node=>node.known);if(known){type MutableOutput=Expect<Equal<typeof known.typeId,string>>;void(null as MutableOutput|null);}
  const themed=await api.setTheme(theme,{expectedRevision:options.expectedRevision??0});
  const receipt:CompositionReceipt=themed;
  if(receipt.status==='committed'){const reset:true=receipt.historyReset;void reset;}else{const reset:false=receipt.historyReset,changed:false=receipt.graphChanged;void reset;void changed;}
  const socket=await api.composeSocket('dynamic',{title:'Dynamic',color,acceptsFrom:['dynamic']},{expectedRevision:themed.revision});
  const node=await api.composeNode('dynamic-node',{version:1,title:'Dynamic',behavior:'standard',style:'basic',parameters:{},sockets:{out:{title:'Out',direction:'output',type:'dynamic',maxIncomingLinks:0,visible:true,value:null,showValue:false}},ui:[{kind:'socket',socket:'out'}],muteBypass:[],migrations:[]},{expectedRevision:socket.revision});
  await api.dispatch({type:'node.add',nodeType:'dynamic-node',position:{x:0,y:0}});
  const removedNode=await api.removeNode('dynamic-node',{expectedRevision:node.revision});
  const removedSocket=await api.removeSocket('dynamic',{expectedRevision:removedNode.revision});void removedSocket;
  await api.loadComposition(composition);
  api.onCompositionChanges(event=>{const envelope:CompositionChangeEnvelope=event,change:CompositionChange=event.change;envelope.revision;if(change.kind!=='theme.set'&&change.kind!=='header-styles.set'&&change.kind!=='compatibility.set'&&change.kind!=='composition.load')change.id;
    // @ts-expect-error composition events do not expose definition payloads
    event.change.definition;});
}
async function ownershipSmoke():Promise<void>{
  const api:FxNode=await createFxNode({applicationId:'consumer',applicationVersion:1,resources:composition.resources});
  const canvas=null as unknown as HTMLCanvasElement,view:FxNodeView=await api.attachView({canvas,viewport:{width:1,height:1,dpr:1}});
  view.getHostSnapshot();const offHost=view.subscribeHost(()=>{}),offRequests=view.onHostRequests(()=>{});view.feedInput({kind:'focus',phase:'focus'});view.setViewport({width:2,height:2,dpr:1});
  await view.addNode({typeId:'tiny',viewPosition:{x:0,y:0}});await view.removeSelected();await view.setSelectedMuted(false);await view.provideResource({viewId:view.id,token:'token',graphVersion:0,compositionRevision:0},{name:'x',mime:'image/png',bytes:new ArrayBuffer(1)});await view.whenRendered();offHost();offRequests();await view.detach();
  await api.getState();api.onSnapshots(()=>{});api.onMutations(()=>{});await api.dispatch({type:'undo'});await api.setTheme(theme);api.onCompositionChanges(()=>{});
  // @ts-expect-error views do not own graph snapshots
  view.getState();
  // @ts-expect-error views do not own composition mutation
  view.setTheme(theme);
  // @ts-expect-error roots do not own rendering barriers
  api.whenRendered();
  api.destroy();
}
void runtime; void compiled; void liveCompositionSmoke; void ownershipSmoke; void (null as InferredNode | null);
`,
  );
  writeFileSync(
    join(consumer, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        strict: true,
        noEmit: true,
        target: "ES2022",
        module: "NodeNext",
        moduleResolution: "NodeNext",
        skipLibCheck: true,
      },
      include: ["main.ts"],
    }),
  );
  run("npx", ["tsc"], consumer);
  writeFileSync(join(consumer, "index.html"), "<script type=module src=/main.ts></script>");
  run("npx", ["vite", "build", "--base", "./"], consumer);
  const output = readdirSync(join(consumer, "dist", "assets"));
  const js = output
    .filter((file) => file.endsWith(".js"))
    .map((file) => readFileSync(join(consumer, "dist", "assets", file), "utf8"))
    .join("\n");
  assert(!js.includes('new URL("/assets/'), "worker URL is root-absolute");
  const packageIndex = readFileSync(join(consumer, "node_modules", "fxnode", "dist", "index.js"), "utf8");
  assert(!packageIndex.includes('new URL("/assets/'), "packed worker URL is root-absolute");
  const workerMatch = packageIndex.match(/"assets\/([^"?]*worker[^"?]*\.js)"/);
  assert(workerMatch, "packed package has no package-relative worker URL");
  const packagedAssets = readdirSync(join(consumer, "node_modules", "fxnode", "dist", "assets"));
  assert(packagedAssets.includes(workerMatch[1]!), `worker asset ${workerMatch[1]} does not exist`);
  console.log(`package smoke passed (${files.length} files; worker assets/${workerMatch[1]})`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
