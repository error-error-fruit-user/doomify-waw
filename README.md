# Doomify WaW

> **AI notice:** this project is developed with the help of AI. Much of the code and documentation was written with [Claude](https://claude.ai) (Anthropic), working under my direction and review. Commits that Claude helped write carry a `Co-Authored-By: Claude` trailer, so you can see which ones in the history.

Rebuilds the weapons and sounds of Call of Duty: World at War's **Der Riese** for UZDoom zombies maps, from **your own copy of the game, while it runs**. Nothing of World at War is in this repository or in the maps that use it: Nacht des Schreckens and Asylum ship a `doomify.py` that runs this once before you play.

Every Der Riese weapon comes out ready to play on [Horde Hammer](https://github.com/error-error-fruit-user/horde-hammer): the M1911 to the Wunderwaffe DG-2, the BAR to the Ray Gun, the Pack-a-Punched version of each (Mustang & Sally, the Gibs-o-matic...), the Stielhandgranate, Bouncing Betty, cymbal monkey and Bowie knife, with their wall drawings, their sounds and the zombies' voices, and the four perk bottles drunk as Der Riese drinks them (a map keeps its own machines).

## Using it

You need World at War (Steam) with **Der Riese loaded in solo zombies**, and [Blender](https://www.blender.org) 4.0 or newer. On Linux the game runs through Proton; Hell Hound reads it there too.

```sh
doomify-waw build path/to/map/doomify        # or just doomify-waw in that folder
```

It takes about an hour the first time: it pulls each weapon from the running game, renders it in Blender and turns the frames into sprites. What it rendered is kept in your cache (`~/.cache/doomify-waw`, `%LOCALAPPDATA%\doomify-waw` on Windows) and shared by every map, so the next map, or the next run, only redoes what changed. `--force` redoes everything, `--only thompson` builds one weapon to try things out, `--wait` waits for the game to start, `--blender PATH` picks Blender.

## How it works

1. **Pull** (`doomify-waw --script weapons/thompson.toml --out DIR`): the TOML says what to take from the game, and [Hell Hound](https://github.com/error-error-fruit-user/hell-hound) reads it from the game's memory and IWDs: the first-person models and clips as one GLB (the gun hung on the hands), the world model, the chalk drawing, and every sound: the TOML's, and every sound the clips' notetracks play (a reload's magazine out and in). A sound set's TOML (`sounds/zombie.toml`) pulls the zombies' voices the same way.
2. **Render** (Blender, `blender/render.py` in `blender/weapon.blend`): the Blender script runs those pulls itself with `--script`, converts the sounds Doom can't play (MS ADPCM, xWMA) to Ogg Vorbis, and renders every animation at **35 frames a second**, Doom's tic rate, from the player's eye. `weapon.blend` is dark and brooding: a near-black violet world, a warm key light that keeps the textures' colours, a cold cyan rim that cuts the weapon out of the dark, a faint violet underneath. Its compositor gives every sprite a **one-pixel dark border**. The canvas is wider than Doom's 4:3 screen (480 by 224 of Doom's tall pixels), so weapons are whole on **wide screens up to 21:9**, with room below for bobbing.
3. **Sprites**: every frame is offset for Doom's weapon screen and cropped with [putpng](https://crates.io/crates/putpng), empty ones are left out.
4. **Code**: the ZScript, SNDINFO, MAPINFO and KEYCONF are built with a builder (`src/code.rs`) that reads like the decisions behind it: `.always(...)` adds text, `.when(condition, ...)` adds it if the condition holds, `.otherwise_when(...)` and `.otherwise(...)` finish the chain like `else if` and `else`. The text itself is formatted multiline strings (`indoc::formatdoc!`).

World at War layers its viewmodel animations: a clip (fire, reload) never moves `tag_torso`, which only the aim-down-sights clips do. Doomify renders each clip from the hip with `tag_torso` where `ADS_down` ends, aimed where `ADS_up` ends, and the zoom between them, so the hip and aimed poses are the game's own.

### What the weapons do

- Frames last a tic each. **Speed Cola** and **Double Tap** (Horde Hammer's `ReloadFrameRate` and `FireFrameRate`) skip frames to play an animation faster, rather than shortening frames that are already a tic.
- Firing takes World at War's fire time; the rest of the animation can be cut short by firing again. Automatic weapons hold, bolt actions and the Trench Gun work their bolt and pump, the Trench Gun loads one shell at a time and stops if you fire.
- Aiming zooms by the weapon's own sight (World at War's `adsZoomFov`).
- Projectiles (Ray Gun, Panzerschrek, rifle grenades, Mustang & Sally) are lights and particles; the flamethrower burns.
- **The Wunderwaffe DG-2** fires a bolt of lightning that kills a zombie and **jumps to the next one it can see**, up to 10 (24 for the DG-3 JZ), each one shaking, sparking and screaming, with a flash and a light at every jump; a big kill makes the gun pleased with itself. Its **three bulbs light up with the rounds left**: every pose is rendered with as many filaments glowing as there are rounds, and each shot puts one out.
- **Equipment**: one key (G, "World at War" in the controls menu) brings up the best equipment you have, the cymbal monkey, then the Bouncing Betty, the Bowie knife, then grenades; pressed again, the next. Grenades bounce and go off, the Betty is planted and jumps when a zombie comes near, the monkey plays its cymbals and every zombie near goes to it before it goes off.
- **The Bowie knife**, bought off the wall, plays its flourish, then **replaces the kick**: the kick key stabs with it.
- **Perks**: Juggernog, Quick Revive, Speed Cola and Double Tap are drunk from Der Riese's bottles, hands and all (`kind = "drink"`, `perk = "Juggernog"`); the class is `<Prefix>_JuggernogDrink`, which the map's own machine sells. Speed Cola drinks twice as fast.
- The M1 Garand's clip **pings** out on its last shot (the sound role `eject`).

## For a map

`doomify-waw init path/to/map/doomify --prefix Nacht --output ../nacht-waw` writes a `doomify.toml` and the default weapons and sound sets, which the map then owns:

- `doomify.toml`: the class prefix (`Nacht_Thompson`), the pk3 folder to write (replaced on every run; it holds a `DOOMIFY.txt` so nothing else is ever deleted), the damage scale (World at War's damage times this; Horde Hammer's zombies have less health), the map's own pk3 folders (so sprite names never clash) and the wall drawings' editor numbers.
- `weapons/<id>.toml`: one per weapon (or perk bottle), with World at War's own numbers, read from the running game (`hell-hound list --kind weapon --json`): what to pull, the slot, box weight, last stand rank, wall cost and chalk, ammo, fire time, damage, spread, the clip of every role (`idle`, `fire`, `reload`, `ads_up`...) and the sound of every role.
- `[view]` in a weapon's TOML tunes how it is rendered: `ads_fov`; `hold = "j_wrist_ri"` carries the model by a bone of the hands where World at War's clip leaves it behind the eye (the monkey); `hold_in = { first_raise = "tag_weapon" }` hangs it exactly on a bone in one render (the Bowie's flourish); `hold_frame` picks the throw frame equipment is held at.
- `sounds/<set>.toml`: Doom sound names and the aliases they come from (`zombie/normal` is `amb_vocals`).

The generated pk3 folder is loaded **after** the map. The map's player starts with the weapons it wants (`Nacht_Colt`, `Nacht_Stielhandgranate`), and the weapons put themselves in slots. Classes: `<Prefix>_DoomifyWeapon` (the base), one per weapon with its `Ammo`, `Shot` and `Drawing`, `<Prefix>_DoomifyEquipment` (the equipment key). Sounds are `waw/<alias>`. Electrocuted zombies die by `Tesla` damage, burnt ones by `Fire`, blown up ones by `Explosive`: a map's zombie can have states for them.

## Building

```sh
cargo build --release
```

It is built against a [Hell Hound](https://github.com/error-error-fruit-user/hell-hound) checkout next to it (`../hell-hound`). `blender/weapon.blend` is made by `blender -b --factory-startup --python blender/make_blend.py -- blender/weapon.blend`; the scene and `blender/render.py` are built into the executable, and a change to either renders everything again.

## Licensing

The code is 0BSD (`LICENSE`). putpng is MIT/Apache-2.0. World at War's assets belong to their owners: owning the game lets you play with what you pull from it, not share it. The pk3 doomify writes is yours alone; don't upload it.
