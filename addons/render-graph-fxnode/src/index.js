// FXNode is an optional authoring frontend. Its exporter is intentionally the only
// FXNode-aware code that feeds the canonical AST package.
export {
  adaptFxNodeSnapshot,
  mapAuthoringDiagnostic,
} from "./adapter.js";
