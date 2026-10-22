"""Builds weapon.blend, the scene doomify-waw renders every weapon in. Run it with Blender:

    blender -b --factory-startup --python blender/make_blend.py -- blender/weapon.blend

The scene: a camera where the player's eye is (World at War's tag_view, the origin of every
viewmodel), lit dark and brooding with lights that make the weapons stand out in their own colours, at 35
frames a second like Doom, and a compositor that gives every sprite a dark one-pixel border.

Sizes: Doom draws weapon sprites on a 320x200 screen with pixels 1.2 times taller than wide (a 4:3
picture). The canvas is 480x224 of those pixels: the 4:3 screen sits in the middle (x 80-400, y
0-200), the 80 pixels each side are what a 21:9 screen shows besides, and the 24 below are for
the weapon bobbing up. Doomify crops what is empty.
"""

import math
import sys

import bpy
from mathutils import Vector

WIDTH, HEIGHT = 480, 224
SCREEN_WIDTH, SCREEN_HEIGHT = 320, 200
PIXEL_ASPECT = 1.2
# World at War's field of view (cg_fov), across the 4:3 screen
HIP_FOV = 65.0
BORDER = (0.025, 0.012, 0.035)


def angle_across(width, fov):
    """The horizontal field of view across `width` pixels when SCREEN_WIDTH pixels see `fov`."""
    return 2 * math.atan(math.tan(math.radians(fov) / 2) * width / SCREEN_WIDTH)


def look(obj, direction):
    obj.rotation_euler = Vector(direction).to_track_quat("-Z", "Y").to_euler()


def eevee(scene):
    engines = {e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items}
    scene.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in engines else "BLENDER_EEVEE"
    settings = scene.eevee
    for name, value in (("use_gtao", True), ("use_bloom", True), ("bloom_intensity", 0.03),
                        ("use_soft_shadows", True), ("taa_render_samples", 8)):
        if hasattr(settings, name):
            setattr(settings, name, value)


def colour(scene):
    view = scene.view_settings
    transforms = {i.identifier for i in view.bl_rna.properties["view_transform"].enum_items}
    view.view_transform = "AgX" if "AgX" in transforms else "Filmic"
    looks = [i.identifier for i in view.bl_rna.properties["look"].enum_items]
    for wanted in ("AgX - Punchy", "Punchy", "High Contrast", "Filmic - High Contrast"):
        if wanted in looks:
            view.look = wanted
            break
    view.exposure = 0.6


def camera(scene):
    data = bpy.data.cameras.new("View")
    data.sensor_fit = "HORIZONTAL"
    data.angle = angle_across(WIDTH, HIP_FOV)
    data.clip_start = 0.005
    data.clip_end = 50
    # Straight ahead is the 4:3 screen's centre (y 100), not the canvas's (y 112): shift the view
    # down by those 12 pixels, in units of the canvas's width as displayed.
    data.shift_y = -(HEIGHT / 2 - SCREEN_HEIGHT / 2) * PIXEL_ASPECT / WIDTH
    view = bpy.data.objects.new("View", data)
    scene.collection.objects.link(view)
    look(view, (1, 0, 0))
    view["hip_fov"] = HIP_FOV
    scene.camera = view

    # The pickup camera: orthographic, from the weapon's right side; render.py fits it.
    side_data = bpy.data.cameras.new("Side")
    side_data.type = "ORTHO"
    side = bpy.data.objects.new("Side", side_data)
    scene.collection.objects.link(side)
    side.location = (0, -5, 0)
    look(side, (0, 1, 0))
    return view


def light(scene, name, kind, location, target, colour, energy, size=0.2):
    data = bpy.data.lights.new(name, kind)
    data.color = colour
    data.energy = energy
    if kind == "AREA":
        data.size = size
    elif kind == "SPOT":
        data.spot_size = math.radians(70)
        data.spot_blend = 0.6
    obj = bpy.data.objects.new(name, data)
    scene.collection.objects.link(obj)
    obj.location = location
    look(obj, Vector(target) - Vector(location))
    return obj


def lights(scene):
    # Blender's axes are the game's: x forward, y left, z up. Viewmodels sit about 0.2 to 0.6 m
    # ahead, a little right and below the eye.
    gun = (0.45, -0.08, -0.12)
    # A warm key from above right keeps the textures' own colours; a cold cyan rim from behind
    # cuts the silhouette out of the dark; a faint cool fill from below keeps the shadows from
    # going flat. No coloured tint over the weapon: the purple is only in the border.
    light(scene, "Key", "AREA", (0.15, -0.45, 0.4), gun, (1.0, 0.8, 0.6), 13.0, 0.3)
    light(scene, "Rim", "SPOT", (1.1, 0.35, 0.35), gun, (0.3, 0.9, 1.0), 12.0)
    light(scene, "Under", "AREA", (0.3, 0.25, -0.55), gun, (0.75, 0.8, 0.9), 0.8, 0.4)
    light(scene, "Eye", "POINT", (-0.05, 0.0, 0.05), gun, (1.0, 0.9, 0.8), 0.6)

    world = bpy.data.worlds.new("Dark")
    world.use_nodes = True
    background = world.node_tree.nodes["Background"]
    background.inputs["Color"].default_value = (0.01, 0.01, 0.011, 1)
    background.inputs["Strength"].default_value = 1.0
    scene.world = world


def compositor(scene):
    """Saturation, a hard alpha edge, and the one-pixel border around it."""
    scene.use_nodes = True
    tree = scene.node_tree
    for node in list(tree.nodes):
        tree.nodes.remove(node)
    nodes, links = tree.nodes, tree.links

    def node(kind, x, **inputs):
        n = nodes.new(kind)
        n.location = (x, 0)
        for key, value in inputs.items():
            n.inputs[key].default_value = value
        return n

    render = node("CompositorNodeRLayers", -800)
    saturate = node("CompositorNodeHueSat", -550)
    saturate.inputs["Saturation"].default_value = 1.2
    links.new(render.outputs["Image"], saturate.inputs["Image"])

    # Doom sprites are opaque or not there: half covered or more is in.
    solid = node("CompositorNodeMath", -550)
    solid.operation = "GREATER_THAN"
    solid.inputs[1].default_value = 0.5
    links.new(render.outputs["Alpha"], solid.inputs[0])

    # The border: the shape grown by one pixel, in the border colour, under the weapon.
    grow = node("CompositorNodeDilateErode", -300)
    grow.mode = "STEP"
    grow.distance = 1
    links.new(solid.outputs[0], grow.inputs["Mask"])
    border = node("CompositorNodeRGB", -300)
    border.outputs[0].default_value = (*BORDER, 1)
    border.location = (-300, -200)
    outline = node("CompositorNodeSetAlpha", -100)
    outline.mode = "REPLACE_ALPHA"
    links.new(border.outputs[0], outline.inputs["Image"])
    links.new(grow.outputs[0], outline.inputs["Alpha"])

    weapon = node("CompositorNodeSetAlpha", -100)
    weapon.location = (-100, 200)
    weapon.mode = "REPLACE_ALPHA"
    links.new(saturate.outputs["Image"], weapon.inputs["Image"])
    links.new(solid.outputs[0], weapon.inputs["Alpha"])

    over = node("CompositorNodeAlphaOver", 100)
    links.new(outline.outputs[0], over.inputs[1])
    links.new(weapon.outputs[0], over.inputs[2])
    output = node("CompositorNodeComposite", 300)
    links.new(over.outputs[0], output.inputs["Image"])


def main():
    path = sys.argv[sys.argv.index("--") + 1]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.name = "Weapon"
    scene.render.fps = 35
    scene.render.fps_base = 1
    scene.render.resolution_x = WIDTH
    scene.render.resolution_y = HEIGHT
    scene.render.resolution_percentage = 100
    scene.render.pixel_aspect_x = 1
    scene.render.pixel_aspect_y = PIXEL_ASPECT
    scene.render.film_transparent = True
    scene.render.filter_size = 1.0
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.use_compositing = True
    eevee(scene)
    colour(scene)
    camera(scene)
    lights(scene)
    compositor(scene)
    bpy.data.collections.new("Weapon")
    scene.collection.children.link(bpy.data.collections["Weapon"])
    bpy.ops.wm.save_as_mainfile(filepath=path, compress=True)


main()
