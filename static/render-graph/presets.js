import { semanticProjectionToV1 } from "./adapter.js";
const make=(graphId,clearColor)=>Object.freeze(semanticProjectionToV1({graphId,clearColor,clearDepth:1,passState:"enabled"},1));
export const midnight=make("preset_midnight",[0.015,0.06,0.18,1]);
export const ember=make("preset_ember",[0.18,0.035,0.012,1]);
export const renderGraphPresets=Object.freeze({midnight,ember});
