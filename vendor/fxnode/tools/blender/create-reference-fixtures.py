# SPDX-License-Identifier: GPL-3.0-or-later
"""Create deterministic, project-authored Blender node-editor reference fixtures."""

import argparse
import sys
from pathlib import Path

import bpy

FIXTURES = (
    "shader-basic-linked-near", "shader-basic-linked-far",
    "shader-selected-active-hover-near", "shader-collapsed-near",
    "common-frame-reroute-near", "shader-widget-rich-near",
    "shader-socket-gallery-near", "geometry-socket-gallery-near",
)


def arguments():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", choices=FIXTURES, required=True)
    parser.add_argument("--save", type=Path)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def add(nodes, node_type, name, x, y):
    node = nodes.new(node_type)
    node.name = name
    node.label = name
    node.location = (x, y)
    return node


def shader_tree(fixture):
    material = bpy.data.materials.new("fxnode-reference")
    material.use_nodes = True
    tree = material.node_tree
    tree.nodes.clear()
    nodes, links = tree.nodes, tree.links
    output = add(nodes, "ShaderNodeOutputMaterial", "Material Output", 520, 0)
    principled = add(nodes, "ShaderNodeBsdfPrincipled", "Principled BSDF", 160, 0)
    noise = add(nodes, "ShaderNodeTexNoise", "Noise Texture", -440, 160)
    ramp = add(nodes, "ShaderNodeValToRGB", "Color Ramp", -140, 180)
    links.new(noise.outputs["Fac"], ramp.inputs["Fac"])
    links.new(ramp.outputs["Color"], principled.inputs["Base Color"])
    links.new(principled.outputs["BSDF"], output.inputs["Surface"])
    if fixture == "shader-collapsed-near":
        noise.hide = True
        ramp.hide = True
    elif fixture == "common-frame-reroute-near":
        frame = add(nodes, "NodeFrame", "Texture controls", -500, 100)
        noise.parent = frame
        reroute = add(nodes, "NodeReroute", "Reroute", 60, 260)
        links.new(ramp.outputs["Color"], reroute.inputs[0])
        links.new(reroute.outputs[0], principled.inputs["Base Color"])
    elif fixture == "shader-widget-rich-near":
        add(nodes, "ShaderNodeMix", "Mix", -140, -240)
        add(nodes, "ShaderNodeMath", "Math", 160, -280)
    elif fixture == "shader-socket-gallery-near":
        add(nodes, "ShaderNodeTexCoord", "Texture Coordinate", -700, -260)
        add(nodes, "ShaderNodeVectorMath", "Vector Math", -400, -260)
        add(nodes, "ShaderNodeValue", "Value", -100, -320)
        add(nodes, "ShaderNodeRGB", "Color", 160, -320)
    nodes.active = principled
    principled.select = True
    return tree


def geometry_tree():
    obj = bpy.data.objects.new("fxnode-geometry", bpy.data.meshes.new("fxnode-mesh"))
    bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    modifier = obj.modifiers.new("fxnode-reference", "NODES")
    tree = bpy.data.node_groups.new("fxnode-geometry", "GeometryNodeTree")
    modifier.node_group = tree
    nodes = tree.nodes
    for node_type, name, x, y in (
        ("GeometryNodeInputPosition", "Position", -600, 200),
        ("GeometryNodeMeshCube", "Mesh Cube", -600, -100),
        ("GeometryNodeSetPosition", "Set Position", -200, 80),
        ("GeometryNodeTransform", "Transform Geometry", 160, 80),
        ("GeometryNodeJoinGeometry", "Join Geometry", 500, 80),
    ):
        add(nodes, node_type, name, x, y)
    return tree


def main():
    args = arguments()
    if bpy.app.version[:2] != (4, 5):
        raise RuntimeError(f"Blender 4.5.x required, found {bpy.app.version_string}")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    tree = geometry_tree() if args.fixture == "geometry-socket-gallery-near" else shader_tree(args.fixture)
    for area in bpy.context.screen.areas:
        if area.type == "NODE_EDITOR":
            area.spaces.active.pin = True
            area.spaces.active.geometry_nodes_type = "MODIFIER" if tree.bl_idname == "GeometryNodeTree" else "MODIFIER"
            with bpy.context.temp_override(area=area):
                bpy.ops.node.view_all()
                if args.fixture == "shader-basic-linked-far":
                    bpy.ops.view2d.zoom_out(zoomfacx=3.0, zoomfacy=3.0)
    if args.save:
        bpy.ops.wm.save_as_mainfile(filepath=str(args.save.resolve()))


if __name__ == "__main__":
    main()
