"""doomify-waw's Blender side: pulls weapons and sounds from the running game and renders them.

doomify-waw runs it with weapon.blend open, both written next to a job file:

    blender -b weapon.blend --python render.py -- --job job.json

For each sound set and weapon in the job it:

1. pulls what the item's TOML names from the game with doomify-waw's own `--script` mode
   (`doomify-waw --script weapons/thompson.toml --out work/pull/thompson`): the first-person
   models and clips as view.glb, the world model as world.glb, and the sounds;
2. converts the sounds Doom can't play (MS ADPCM, xWMA) to Ogg Vorbis;
3. renders every animation the job asks for, at 35 frames a second, from the player's eye, into
   numbered PNGs, and the world model from the side for the pickup sprite.

World at War layers its viewmodel animations: the clips (idle, fire, reload...) never move
tag_torso, which only the ADS clips do; aimed, it is where ADS_up ends, from the hip where
ADS_down ends. So each render is its clip with tag_torso taken from the stance it is in.

Nothing here writes outside the job's folders, and finished renders are kept unless forced.
"""

import json
import math
import os
import subprocess
import sys

import aud
import bpy

FPS = 35
WIDTH = 480
SCREEN_WIDTH = 320
TORSO = "tag_torso"
# Pickup sprites: pixels per metre (Horde Hammer draws pickups at half size).
PICKUP_PPM = 130
PIXEL_ASPECT = 1.2
# Where the lights in weapon.blend aim; the world model is moved there for its picture.
GUN = (0.45, -0.08, -0.12)


def say(*text):
    print("doomify:", *text, flush=True)


def argument(name):
    args = sys.argv[sys.argv.index("--") + 1:]
    return args[args.index(name) + 1]


# Pulling


def pull(job, item):
    """Runs doomify-waw's --script mode for the item, unless its pull is already there."""
    done = os.path.join(item["pull"], "pull.json")
    if os.path.exists(done) and not item.get("force"):
        return
    say("pulling", item["id"])
    command = [job["doomify"], "--script", item["script"], "--out", item["pull"]]
    if job.get("pid"):
        command += ["--pid", str(job["pid"])]
    subprocess.run(command, check=True)


def convert(item):
    """Every pulled sound as Ogg Vorbis (or PCM WAV if this Blender can't write Vorbis)."""
    source = os.path.join(item["pull"], "sounds")
    target = os.path.join(item["pull"], "ogg")
    if not os.path.isdir(source):
        return
    os.makedirs(target, exist_ok=True)
    for name in sorted(os.listdir(source)):
        stem = os.path.splitext(name)[0]
        out = os.path.join(target, stem + ".ogg")
        if os.path.exists(out) and not item.get("force"):
            continue
        sound = aud.Sound(os.path.join(source, name))
        rate, channels = sound.specs
        try:
            sound.write(out, rate=int(rate), channels=int(channels), format=aud.FORMAT_S16,
                        container=aud.CONTAINER_OGG, codec=aud.CODEC_VORBIS, bitrate=160000)
        except Exception as error:  # Blender built without Vorbis: fall back to plain WAV
            say("no Vorbis encoder (", error, "), writing WAV for", stem)
            if os.path.exists(out):
                os.remove(out)
            sound.write(out[:-4] + ".wav", rate=int(rate), channels=int(channels),
                        format=aud.FORMAT_S16, container=aud.CONTAINER_WAV, codec=aud.CODEC_PCM)


# The scene


def angle_across(width, fov):
    return 2 * math.atan(math.tan(math.radians(fov) / 2) * width / SCREEN_WIDTH)


def lens(camera_data, fov):
    camera_data.sensor_width = 36
    return 18 / math.tan(angle_across(WIDTH, fov) / 2)


def import_glb(path, collection):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    for obj in new:
        # The importer's stand-in for the root node: not part of the weapon.
        if obj.name.startswith("Icosphere") or obj.name.startswith("glTF_not_exported"):
            bpy.data.objects.remove(obj)
            continue
        for owner in list(obj.users_collection):
            owner.objects.unlink(obj)
        collection.objects.link(obj)
    return [o for o in collection.objects]


def see_through(objects):
    """Glass (a perk bottle's outside, the Wunderwaffe's tubes) lets what is inside show, as
    its texture's alpha says; the importer makes every material opaque."""
    for obj in objects:
        for slot in getattr(obj, "material_slots", []):
            material = slot.material
            name = material.name.lower() if material else ""
            if material is None or not material.use_nodes or not ("glass" in name or name.endswith("_out")):
                continue
            nodes, links = material.node_tree.nodes, material.node_tree.links
            shader = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
            image = next((n for n in nodes if n.type == "TEX_IMAGE"), None)
            if shader is None or image is None:
                continue
            links.new(image.outputs["Alpha"], shader.inputs["Alpha"])
            # blended, not hashed: at a few samples hashing is noise
            material.blend_method = "BLEND"
            if hasattr(material, "show_transparent_back"):
                material.show_transparent_back = False


def armature(objects):
    rigs = [o for o in objects if o.type == "ARMATURE"]
    if len(rigs) != 1:
        raise RuntimeError(f"expected one armature in the view model, found {len(rigs)}")
    rig = rigs[0]
    if rig.animation_data is None:
        rig.animation_data_create()
    # Clips imported as NLA strips would blend into every render.
    for track in list(rig.animation_data.nla_tracks):
        rig.animation_data.nla_tracks.remove(track)
    return rig


def action(rig, clip):
    """The imported clip; names match ignoring case, as in the game, and Blender cuts names
    (the importer's "<clip>_<armature>") at 63 bytes."""
    wanted = {name.lower()[:63] for name in (f"{clip}_{rig.name}", clip)}
    for found in bpy.data.actions:
        if found.name.lower() in wanted:
            return found
    raise RuntimeError(f"clip {clip} was not pulled")


def torso_curves(source):
    return [fc for fc in source.fcurves if f'"{TORSO}"' in fc.data_path]


def torso_pose(source, end):
    """tag_torso's channels where `source` starts or ends."""
    frame = source.frame_range[1] if end else source.frame_range[0]
    return {(fc.data_path, fc.array_index): fc.evaluate(frame) for fc in torso_curves(source)}


def layered(base, stance, pose):
    """`base` with tag_torso from `stance` (its keys) or `pose` (held still)."""
    result = base.copy()
    result.name = base.name + " (layered)"
    for fc in torso_curves(result):
        result.fcurves.remove(fc)
    if stance is not None:
        for fc in torso_curves(stance):
            copy = result.fcurves.new(fc.data_path, index=fc.array_index, action_group=TORSO)
            for key in fc.keyframe_points:
                copy.keyframe_points.insert(key.co[0], key.co[1], options={"FAST"})
    for (path, index), value in (pose or {}).items():
        curve = result.fcurves.new(path, index=index, action_group=TORSO)
        curve.keyframe_points.insert(0, value)
    return result


def reset(rig):
    for bone in rig.pose.bones:
        bone.location = (0, 0, 0)
        bone.rotation_quaternion = (1, 0, 0, 0)
        bone.rotation_euler = (0, 0, 0)
        bone.scale = (1, 1, 1)


def finished(folder, count):
    return os.path.isdir(folder) and len([n for n in os.listdir(folder) if n.endswith(".png")]) >= count


def light_bulbs(objects, glow, lit):
    """Makes the glowing parts (`glow["materials"]`) of the first `lit` bulbs (`glow["bones"]`,
    the bones they hang on) emit light, the way the Wunderwaffe shows its rounds. Returns what
    to put back."""
    undo = []
    if not glow or lit is None:
        return undo
    bones = set(glow["bones"][:lit])
    for obj in objects:
        if obj.type != "MESH":
            continue
        groups = {group.index: group.name for group in obj.vertex_groups}
        lit_index = None
        for index, slot in enumerate(obj.material_slots):
            material = slot.material
            if material is None or not any(part in material.name for part in glow["materials"]):
                continue
            if lit_index is None:
                lit_index = len(obj.material_slots)
                obj.data.materials.append(bulb_material())
            for polygon in obj.data.polygons:
                if polygon.material_index != index:
                    continue
                weights = obj.data.vertices[polygon.vertices[0]].groups
                if weights and groups[max(weights, key=lambda g: g.weight).group] in bones:
                    undo.append((obj, polygon.index, index))
                    polygon.material_index = lit_index
    return undo


def bulb_material():
    if "Lit bulb" in bpy.data.materials:
        return bpy.data.materials["Lit bulb"]
    material = bpy.data.materials.new("Lit bulb")
    material.use_nodes = True
    shader = material.node_tree.nodes["Principled BSDF"]
    shader.inputs["Base Color"].default_value = (0.55, 0.85, 1.0, 1)
    colour = shader.inputs.get("Emission Color") or shader.inputs.get("Emission")
    colour.default_value = (0.45, 0.8, 1.0, 1)
    shader.inputs["Emission Strength"].default_value = 30.0
    return material


def unlight(undo):
    for obj, polygon, index in undo:
        obj.data.polygons[polygon].material_index = index


def hold(rig, bone, exact):
    """Carries the model (j_gun, or a knife's tag_origin) by `bone`: exactly on it, as the game
    hangs a gun (the TOML's view.hold_in), or from where the current clip starts (view.hold)."""
    gun = rig.pose.bones.get("j_gun") or rig.pose.bones.get("tag_origin")
    for constraint in list(gun.constraints) if gun else []:
        gun.constraints.remove(constraint)
    if gun is None or not bone:
        return
    if exact:
        constraint = gun.constraints.new("COPY_TRANSFORMS")
        constraint.target = rig
        constraint.subtarget = bone
        return
    constraint = gun.constraints.new("CHILD_OF")
    constraint.target = rig
    constraint.subtarget = bone
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()
    constraint.inverse_matrix = (rig.matrix_world @ rig.pose.bones[bone].matrix).inverted()


def render_views(weapon, scene, objects, rig, done):
    view = scene.camera
    hip_fov = view.get("hip_fov", 65.0)
    ads_fov = weapon.get("ads_fov") or hip_fov
    stance = weapon.get("stance") or {}
    hip = torso_pose(action(rig, stance["out"]), True) if stance.get("out") else None
    aimed = torso_pose(action(rig, stance["in"]), True) if stance.get("in") else None
    for item in weapon["renders"]:
        name, mode = item["name"], item["stance"]
        folder = os.path.join(weapon["frames"], name)
        base = action(rig, item["clip"])
        # Zooms last as long as their ADS clip; the base clip only gives the rest of the pose.
        timed = action(rig, stance["in" if mode == "zoom_in" else "out"]) if mode.startswith("zoom") else base
        span = timed.frame_range[1] - timed.frame_range[0]
        count = 1 if item.get("still") else max(1, int(math.floor(span + 1e-4)) + 1)
        count = item.get("frames") or count
        done[name] = count
        if finished(folder, count) and not weapon.get("force"):
            continue
        if mode == "zoom_in":
            combined = layered(base, action(rig, stance["in"]), None)
            fovs = (hip_fov, ads_fov)
        elif mode == "zoom_out":
            combined = layered(base, action(rig, stance["out"]), None)
            fovs = (ads_fov, hip_fov)
        elif mode == "ads":
            combined = layered(base, None, aimed)
            fovs = (ads_fov, ads_fov)
        else:
            combined = layered(base, None, hip)
            fovs = (hip_fov, hip_fov)
        reset(rig)
        rig.animation_data.action = combined
        exact = (weapon.get("hold_in") or {}).get(name.split("@")[0])
        hold(rig, exact or weapon.get("hold"), exact is not None)
        camera = view.data
        if camera.animation_data:
            camera.animation_data_clear()
        camera.lens = lens(camera, fovs[0])
        if fovs[0] != fovs[1]:
            camera.keyframe_insert("lens", frame=0)
            camera.lens = lens(camera, fovs[1])
            camera.keyframe_insert("lens", frame=max(1, count - 1))
        undo = light_bulbs(objects, weapon.get("glow"), item.get("lit"))
        os.makedirs(folder, exist_ok=True)
        scene.frame_start = 0
        scene.frame_end = count - 1
        scene.render.filepath = os.path.join(folder, "####")
        say("rendering", weapon["id"], name, count, "frames")
        bpy.ops.render.render(animation=True)
        unlight(undo)
        bpy.data.actions.remove(combined)


def render_world(weapon, scene, done):
    """The world model from its right side, for the pickup sprite."""
    path = os.path.join(weapon["pull"], "world.glb")
    folder = os.path.join(weapon["frames"], "world")
    if not os.path.exists(path):
        return
    done["world"] = 1
    if finished(folder, 1) and not weapon.get("force"):
        return
    for obj in bpy.data.collections["Weapon"].objects:
        obj.hide_render = True
    collection = bpy.data.collections.new("World")
    scene.collection.children.link(collection)
    objects = import_glb(path, collection)
    bpy.context.view_layer.update()
    corners = [obj.matrix_world @ v.co for obj in objects if obj.type == "MESH" for v in obj.data.vertices]
    low = [min(c[i] for c in corners) for i in range(3)]
    high = [max(c[i] for c in corners) for i in range(3)]
    centre = [(a + b) / 2 for a, b in zip(low, high)]
    for obj in objects:
        if obj.parent is None:
            obj.location = [obj.location[i] + GUN[i] - centre[i] for i in range(3)]
    width, height = high[0] - low[0], high[2] - low[2]
    side = bpy.data.objects["Side"]
    side.location = (GUN[0], GUN[1] - 5, GUN[2])
    side.data.sensor_fit = "HORIZONTAL"
    scene.render.resolution_x = math.ceil(width * PICKUP_PPM) + 4
    scene.render.resolution_y = math.ceil(height * PICKUP_PPM / PIXEL_ASPECT) + 4
    side.data.ortho_scale = scene.render.resolution_x / PICKUP_PPM
    scene.camera = side
    os.makedirs(folder, exist_ok=True)
    scene.frame_start = scene.frame_end = 0
    scene.render.filepath = os.path.join(folder, "####")
    say("rendering", weapon["id"], "pickup")
    bpy.ops.render.render(animation=True)


def render_weapon(weapon, blend):
    bpy.ops.wm.open_mainfile(filepath=blend)
    scene = bpy.context.scene
    collection = bpy.data.collections["Weapon"]
    objects = import_glb(os.path.join(weapon["pull"], "view.glb"), collection)
    see_through(objects)
    rig = armature(objects)
    done = {}
    render_views(weapon, scene, objects, rig, done)
    render_world(weapon, scene, done)
    with open(os.path.join(weapon["frames"], "rendered.json"), "w") as out:
        json.dump(done, out, indent=1, sort_keys=True)


def main():
    with open(argument("--job")) as source:
        job = json.load(source)
    blend = bpy.data.filepath
    for item in job.get("sounds", []):
        pull(job, item)
        convert(item)
    for weapon in job.get("weapons", []):
        pull(job, weapon)
        convert(weapon)
        os.makedirs(weapon["frames"], exist_ok=True)
        render_weapon(weapon, blend)
    say("done")


try:
    main()
except Exception:
    import traceback

    traceback.print_exc()
    sys.exit(1)
