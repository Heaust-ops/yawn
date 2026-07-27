import { createFxNode, type CreateFxNodeOptions, type FxNode } from "@lib/index.js";
import {
  APPLICATION_HEADER_STYLES,
  APPLICATION_ID,
  APPLICATION_RESOURCES,
  APPLICATION_VERSION,
  applicationCompatibility,
} from "./nodes/application.js";
import { exampleTheme } from "../shared/theme.js";
import { frameNode } from "./nodes/common/frame.js";
import { rerouteNode } from "./nodes/common/reroute.js";
import {
  anySocket,
  floatSocket,
  vectorSocket,
  colorSocket,
  shaderSocket,
  geometrySocket,
} from "./nodes/socket-types.js";
import { groupInputNode } from "./nodes/common/group-input.js";
import { groupOutputNode } from "./nodes/common/group-output.js";
import { valueNode } from "./nodes/shader/value.js";
import { colorNode } from "./nodes/shader/color.js";
import { mathNode } from "./nodes/shader/math.js";
import { vectorMathNode } from "./nodes/shader/vector-math.js";
import { mixNode } from "./nodes/shader/mix.js";
import { colorRampNode } from "./nodes/shader/color-ramp.js";
import { textureCoordinateNode } from "./nodes/shader/texture-coordinate.js";
import { noiseTextureNode } from "./nodes/shader/noise-texture.js";
import { imageTextureNode } from "./nodes/shader/image-texture.js";
import { principledBsdfNode } from "./nodes/shader/principled-bsdf.js";
import { materialOutputNode } from "./nodes/shader/material-output.js";
import { positionNode } from "./nodes/geometry/position.js";
import { meshCubeNode } from "./nodes/geometry/mesh-cube.js";
import { setPositionNode } from "./nodes/geometry/set-position.js";
import { transformGeometryNode } from "./nodes/geometry/transform-geometry.js";
import { joinGeometryNode } from "./nodes/geometry/join-geometry.js";
import { imageNode } from "./nodes/compositor/image.js";
import { colorBalanceNode } from "../shared/nodes/color-balance.js";
export type ApplicationFxNode = FxNode;
type ApplicationBrowserOptions = Omit<CreateFxNodeOptions, "applicationId" | "applicationVersion" | "resources">;
export async function createApplicationFxNode(options: ApplicationBrowserOptions = {}): Promise<ApplicationFxNode> {
  const api = await createFxNode({
    ...options,
    applicationId: APPLICATION_ID,
    applicationVersion: APPLICATION_VERSION,
    resources: APPLICATION_RESOURCES,
  });
  try {
    await api.setTheme(exampleTheme);
    await api.setHeaderStyles(APPLICATION_HEADER_STYLES);
    await api.composeSocket(...anySocket);
    await api.composeSocket(...floatSocket);
    await api.composeSocket(...vectorSocket);
    await api.composeSocket(...colorSocket);
    await api.composeSocket(...shaderSocket);
    await api.composeSocket(...geometrySocket);
    await api.setCompatibility(applicationCompatibility);
    await api.composeNode(...frameNode);
    await api.composeNode(...rerouteNode);
    await api.composeNode(...groupInputNode);
    await api.composeNode(...groupOutputNode);
    await api.composeNode(...valueNode);
    await api.composeNode(...colorNode);
    await api.composeNode(...mathNode);
    await api.composeNode(...vectorMathNode);
    await api.composeNode(...mixNode);
    await api.composeNode(...colorRampNode);
    await api.composeNode(...textureCoordinateNode);
    await api.composeNode(...noiseTextureNode);
    await api.composeNode(...imageTextureNode);
    await api.composeNode(...principledBsdfNode);
    await api.composeNode(...materialOutputNode);
    await api.composeNode(...positionNode);
    await api.composeNode(...meshCubeNode);
    await api.composeNode(...setPositionNode);
    await api.composeNode(...transformGeometryNode);
    await api.composeNode(...joinGeometryNode);
    await api.composeNode(...imageNode);
    await api.composeNode(...colorBalanceNode);
    return api;
  } catch (error) {
    api.destroy();
    throw error;
  }
}
