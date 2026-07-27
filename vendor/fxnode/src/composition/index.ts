/** Static composition authoring, validation, and compilation. */
export * from "./types.js";
export { setTheme, setHeaderStyles, composeSocket, removeSocket, composeNode, removeNode } from "./compose.js";
export type { ComposedNode, ComposedSocket, Themed, RemovedSocket, RemovedNode, HeaderStyled } from "./compose.js";
export { FXNODE_COMPOSITION_LIMITS, validateFxNodeComposition } from "./validate.js";
export type { FxNodeCompositionIssue, FxNodeCompositionValidation } from "./validate.js";
export {
  compileFxNodeComposition,
  createInitialFxNodeComposition,
  DEFAULT_FXNODE_THEME,
  FxNodeCompositionError,
} from "./compile.js";
