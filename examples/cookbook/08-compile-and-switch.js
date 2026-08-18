import { loadGraph } from "@yawn/render-graph-js";

/** Compile a complete AST/JSO and make its prepared loadout active. */
export async function compileAndSwitch(core, graph) {
  const compiled = await loadGraph(core, graph);
  try {
    await core.switchCompiledGraph(compiled.compiledId);
    return compiled;
  } catch (error) {
    await core.dropCompiledGraph(compiled.compiledId).catch(() => {});
    throw error;
  }
}

/** Return to clear-only mode before releasing an active compiled loadout. */
export async function dropActiveGraph(core, compiled) {
  await core.switchToImmediate();
  await core.dropCompiledGraph(compiled.compiledId);
}
