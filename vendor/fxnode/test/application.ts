/** Test-only headless authority assembled from the example's public node definitions. */
import { compileFxNodeComposition, composeNode, composeSocket, setTheme } from "@lib/composition/index.js";
import type { FxNodeCompositionData } from "@lib/composition/index.js";
import { bindFxNodeHeadless } from "@lib/headless-runtime.js";
import {
  APPLICATION_HEADER_STYLES,
  APPLICATION_ID,
  APPLICATION_RESOURCES,
  APPLICATION_VERSION,
  applicationCompatibility,
} from "../examples/blender/nodes/application.js";
import { exampleTheme } from "../examples/shared/theme.js";
import {
  anySocket,
  floatSocket,
  vectorSocket,
  colorSocket,
  shaderSocket,
  geometrySocket,
} from "../examples/blender/nodes/socket-types.js";
import { frameNode } from "../examples/blender/nodes/common/frame.js";
import { rerouteNode } from "../examples/blender/nodes/common/reroute.js";
import { groupInputNode } from "../examples/blender/nodes/common/group-input.js";
import { groupOutputNode } from "../examples/blender/nodes/common/group-output.js";
import { valueNode } from "../examples/blender/nodes/shader/value.js";
import { colorNode } from "../examples/blender/nodes/shader/color.js";
import { mathNode } from "../examples/blender/nodes/shader/math.js";
import { vectorMathNode } from "../examples/blender/nodes/shader/vector-math.js";
import { mixNode } from "../examples/blender/nodes/shader/mix.js";
import { colorRampNode } from "../examples/blender/nodes/shader/color-ramp.js";
import { textureCoordinateNode } from "../examples/blender/nodes/shader/texture-coordinate.js";
import { noiseTextureNode } from "../examples/blender/nodes/shader/noise-texture.js";
import { imageTextureNode } from "../examples/blender/nodes/shader/image-texture.js";
import { principledBsdfNode } from "../examples/blender/nodes/shader/principled-bsdf.js";
import { materialOutputNode } from "../examples/blender/nodes/shader/material-output.js";
import { positionNode } from "../examples/blender/nodes/geometry/position.js";
import { meshCubeNode } from "../examples/blender/nodes/geometry/mesh-cube.js";
import { setPositionNode } from "../examples/blender/nodes/geometry/set-position.js";
import { transformGeometryNode } from "../examples/blender/nodes/geometry/transform-geometry.js";
import { joinGeometryNode } from "../examples/blender/nodes/geometry/join-geometry.js";
import { imageNode } from "../examples/blender/nodes/compositor/image.js";
import { colorBalanceNode } from "../examples/shared/nodes/color-balance.js";

const seed = {
  schemaVersion: 2,
  id: APPLICATION_ID,
  version: APPLICATION_VERSION,
  nodeStyles: APPLICATION_HEADER_STYLES,
  resources: APPLICATION_RESOURCES,
  compatibility: applicationCompatibility,
  socketTypes: {},
  nodes: {},
} as const;
const themed = setTheme(seed, exampleTheme);
const socket1 = composeSocket(themed, ...anySocket);
const socket2 = composeSocket(socket1, ...floatSocket);
const socket3 = composeSocket(socket2, ...vectorSocket);
const socket4 = composeSocket(socket3, ...colorSocket);
const socket5 = composeSocket(socket4, ...shaderSocket);
const socket6 = composeSocket(socket5, ...geometrySocket);
const node1 = composeNode(socket6, ...frameNode);
const node2 = composeNode(node1, ...rerouteNode);
const node3 = composeNode(node2, ...groupInputNode);
const node4 = composeNode(node3, ...groupOutputNode);
const node5 = composeNode(node4, ...valueNode);
const node6 = composeNode(node5, ...colorNode);
const node7 = composeNode(node6, ...mathNode);
const node8 = composeNode(node7, ...vectorMathNode);
const node9 = composeNode(node8, ...mixNode);
const firstBranch = composeNode(node9, ...colorRampNode);
const node11 = composeNode(socket6, ...textureCoordinateNode);
const node12 = composeNode(node11, ...noiseTextureNode);
const node13 = composeNode(node12, ...imageTextureNode);
const node14 = composeNode(node13, ...principledBsdfNode);
const node15 = composeNode(node14, ...materialOutputNode);
const secondBranch = composeNode(node15, ...positionNode);
const node17 = composeNode(socket6, ...meshCubeNode);
const node18 = composeNode(node17, ...setPositionNode);
const node19 = composeNode(node18, ...transformGeometryNode);
const node20 = composeNode(node19, ...joinGeometryNode);
const node21 = composeNode(node20, ...imageNode);
const thirdBranch = composeNode(node21, ...colorBalanceNode);
const source: FxNodeCompositionData = {
  ...firstBranch,
  nodes: { ...firstBranch.nodes, ...secondBranch.nodes, ...thirdBranch.nodes },
};
export const APPLICATION_COMPILED = compileFxNodeComposition(source);
export const APPLICATION_HEADLESS = bindFxNodeHeadless(APPLICATION_COMPILED);
