//! A weapon's TOML: what to pull from the game for it, and how it plays in UZDoom. One file per
//! weapon (`weapons/<id>.toml`); the id is the file name. Numbers are World at War's own (damage,
//! seconds), so a file can be checked against the game's weapon definition
//! (`hell-hound list --kind weapon --json`); the map's `doomify.toml` scales damage for Doom.

use crate::types::{List, Map, Text};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// Animation roles a weapon may give clips for.
pub const ANIMATION_ROLES: &[&str] = &[
    "idle",
    "empty_idle",
    "fire",
    "last_fire",
    "ads_fire",
    "ads_last_fire",
    "rechamber",
    "ads_rechamber",
    "reload",
    "reload_empty",
    "reload_start",
    "reload_end",
    "raise",
    "first_raise",
    "drop",
    // with the magazine empty (the bolt locked back)
    "empty_raise",
    "empty_drop",
    "ads_up",
    "ads_down",
];

/// Sound roles; notetracks in the clips add their own sounds (reloads) on top of these.
pub const SOUND_ROLES: &[&str] = &[
    "fire",
    "last_fire",
    "dry",
    "raise",
    "first_raise",
    "drop",
    "rechamber",
    // with the last shot: the M1 Garand's en-bloc clip pinging out
    "eject",
    // projectiles and special weapons
    "projectile",
    "impact",
    "bounce",
    "kill",
    "happy",
    "hum",
    "throw",
    // the flamethrower: burning (looped while the trigger is held), and cooling down after
    "loop",
    "cooldown",
    "arm",
    "plant",
    "melee",
    "hit",
];

/// How a weapon plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Hitscan bullets: pistols, rifles, machine guns.
    Bullet,
    /// Hitscan pellets: shotguns.
    Spread,
    /// A projectile: the Ray Gun, rockets, rifle grenades, Mustang & Sally.
    Projectile,
    /// A stream of fire: the flamethrower.
    Flame,
    /// The Wunderwaffe's chain lightning.
    Tesla,
    /// A thrown grenade.
    Grenade,
    /// The cymbal monkey: thrown, draws zombies in, then explodes.
    Monkey,
    /// A Bouncing Betty, planted on the floor.
    Mine,
    /// A knife: no ammo, swung.
    Melee,
    /// A perk's bottle (the map's own machine sells it).
    Drink,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Game {
    /// The game's weapon definition this file was made from.
    pub weapon: Text,
    /// The first-person models, hands first; the gun is hung on the hands' tag_weapon.
    pub models: List<Text>,
    /// The world model, rendered for the pickup sprite.
    #[serde(default)]
    pub world: Option<Text>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ammo {
    /// Rounds in the magazine (0 for weapons without one, like the knife).
    #[serde(default)]
    pub magazine: u32,
    /// Most rounds carried besides the magazine.
    #[serde(default)]
    pub reserve: u32,
    /// Reserve rounds the weapon comes with.
    #[serde(default)]
    pub give: u32,
    /// A reload only takes what the magazine is missing.
    #[serde(default = "yes")]
    pub partial: bool,
    /// The reload loads one round at a time (reload_start, a reload per round, reload_end).
    #[serde(default)]
    pub segmented: bool,
}

fn yes() -> bool {
    true
}

/// How a projectile looks in flight and when it bursts, from sprites the map or the IWAD has
/// (the Ray Gun's, from the map; Doom's rocket and plasma); without it a projectile is only its
/// light and particles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// A sprite name (four letters or digits) and its frames in flight, two tics each.
    pub sprite: Text,
    #[serde(default = "first_frame")]
    pub frames: Text,
    #[serde(default = "one_f")]
    pub scale: f64,
    /// Grows and shrinks as it flies (the Ray Gun's).
    #[serde(default)]
    pub pulse: bool,
    /// A ZScript translation that recolours it (Porter's X2 Ray Gun's red).
    #[serde(default)]
    pub translation: Option<Text>,
    /// The sprite and frames it bursts into, two tics each.
    #[serde(default)]
    pub burst: Option<Burst>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    pub sprite: Text,
    pub frames: Text,
    #[serde(default = "one_f")]
    pub scale: f64,
}

fn first_frame() -> Text {
    Text::Borrowed("A")
}
fn one_f() -> f64 {
    1.0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fire {
    /// Holding the trigger keeps firing.
    #[serde(default)]
    pub auto: bool,
    /// Seconds between shots (the weapon file's fireTime).
    pub time: f64,
    /// Damage per bullet or projectile, in World at War's numbers.
    #[serde(default)]
    pub damage: u32,
    /// Bullets per shot.
    #[serde(default = "one")]
    pub pellets: u32,
    /// Spread in degrees from the hip, and aimed.
    #[serde(default = "spread")]
    pub spread: f64,
    #[serde(default)]
    pub ads_spread: f64,
    /// Explosion radius in Doom units, for projectiles that splash.
    #[serde(default)]
    pub splash: u32,
    /// Projectile speed in Doom units per tic.
    #[serde(default)]
    pub speed: f64,
    /// How many zombies the Wunderwaffe's lightning can kill in one shot, and how far it jumps.
    #[serde(default)]
    pub arcs: u32,
    #[serde(default)]
    pub arc_range: u32,
    /// The colour of the projectile's light and particles (RRGGBB).
    #[serde(default)]
    pub color: Option<Text>,
}

fn one() -> u32 {
    1
}
fn spread() -> f64 {
    2.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wall {
    /// Points to buy it off the wall.
    pub cost: u32,
    /// The chalk drawing's image in the IWDs.
    pub chalk: Text,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Where the eye sits from the hip, in metres (forward, left, up); the rig's default when unset.
    #[serde(default)]
    pub hip: Option<[f64; 3]>,
    /// Horizontal field of view when aiming (4:3, degrees); the game's adsZoomFov.
    #[serde(default)]
    pub ads_fov: Option<f64>,
    /// A bone of the hands to carry the model by (`j_wrist_ri`), for equipment whose clips
    /// leave `tag_weapon` behind the eye (the cymbal monkey); it keeps where each clip starts.
    #[serde(default)]
    pub hold: Option<Text>,
    /// For some renders only, by render (`first_raise`): a bone to hang the model exactly on, as
    /// the game hangs a gun; the Bowie knife is a gun on `tag_weapon` in its flourish, but in the
    /// left hand when it slashes.
    #[serde(default)]
    pub hold_in: Map<Text>,
    /// Equipment World at War keeps off screen until it is used: the frame of its throw (or
    /// swing) to hold it at while it is up; the frame showing most of it when unset.
    #[serde(default)]
    pub hold_frame: Option<usize>,
    /// The Wunderwaffe: the notetrack of its reload and first raise where its switch turns the
    /// bulbs on; before it they are dark, at it the gun jolts to life.
    #[serde(default)]
    pub lights_on: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weapon {
    /// The name players see.
    pub name: Text,
    /// The class name after the map's prefix (`Nacht_` + `Thompson`).
    pub class: Text,
    /// Two letters or digits that start every sprite of the weapon; unique across weapons.
    pub sprite: Text,
    pub kind: Kind,
    /// The weapon slot it goes in (the box gives what is in the slots); drinks have none.
    #[serde(default)]
    pub slot: u8,
    /// A drink's perk, as Horde Hammer names it: QuickRevive, Juggernog, SpeedCola or DoubleTap.
    #[serde(default)]
    pub perk: Option<Text>,
    pub game: Game,
    /// The id of its Pack-a-Punched version.
    #[serde(default)]
    pub upgrade: Option<Text>,
    /// This is a Pack-a-Punched version: never in the box.
    #[serde(default)]
    pub upgraded: bool,
    /// How often the mystery box gives it (0 for never).
    #[serde(default)]
    pub box_weight: u32,
    /// Its rank as the pistol for going down (the highest held wins; 0 for never).
    #[serde(default)]
    pub last_stand: u32,
    /// The weapon (by file name) given for the last stand instead, when this is the pistol.
    #[serde(default)]
    pub downed: Option<Text>,
    /// Bought off the wall.
    #[serde(default)]
    pub wall: Option<Wall>,
    #[serde(default)]
    pub ammo: Ammo,
    pub fire: Fire,
    /// Role → animation clip.
    pub animations: Map<Text>,
    /// Role → sound alias.
    #[serde(default)]
    pub sounds: Map<Text>,
    #[serde(default)]
    pub view: View,
    /// What it fires, when that is seen (projectiles and the Wunderwaffe).
    #[serde(default)]
    pub projectile: Option<Look>,
}

impl Weapon {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
        let weapon: Self =
            toml::from_str(&text).with_context(|| format!("Invalid weapon {}", path.display()))?;
        weapon
            .check()
            .with_context(|| format!("Invalid weapon {}", path.display()))?;
        Ok(weapon)
    }

    /// Checks what one file can say about itself; the project checks what weapons share.
    pub fn check(&self) -> Result<()> {
        ensure!(
            self.sprite.len() == 2 && self.sprite.chars().all(|c| c.is_ascii_alphanumeric()),
            "sprite must be two letters or digits, not {:?}",
            self.sprite
        );
        ensure!(
            !self.class.is_empty()
                && self
                    .class
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "class must be a ZScript name, not {:?}",
            self.class
        );
        ensure!(!self.game.models.is_empty(), "game.models is empty");
        if self.kind == Kind::Drink {
            ensure!(
                matches!(
                    self.perk.as_deref(),
                    Some("QuickRevive" | "Juggernog" | "SpeedCola" | "DoubleTap")
                ),
                "a drink needs perk = QuickRevive, Juggernog, SpeedCola or DoubleTap"
            );
        } else {
            ensure!((1..=9).contains(&self.slot), "slot must be 1 to 9");
        }
        ensure!(self.fire.time > 0.0, "fire.time must be positive");
        if let Some(look) = &self.projectile {
            ensure!(
                matches!(self.kind, Kind::Projectile | Kind::Tesla),
                "only projectiles and the Wunderwaffe have a [projectile] look"
            );
            let sprites = [(&look.sprite, &look.frames)]
                .into_iter()
                .chain(look.burst.as_ref().map(|b| (&b.sprite, &b.frames)));
            for (sprite, frames) in sprites {
                ensure!(
                    sprite.len() == 4 && sprite.chars().all(|c| c.is_ascii_alphanumeric()),
                    "a projectile's sprite must be four letters or digits, not {sprite:?}"
                );
                ensure!(
                    !frames.is_empty() && frames.chars().all(|c| c.is_ascii_uppercase()),
                    "a projectile's frames must be capital letters, not {frames:?}"
                );
            }
        }
        for role in self.animations.keys() {
            ensure!(
                ANIMATION_ROLES.contains(&role.as_ref()),
                "unknown animation role {role:?} (known: {})",
                ANIMATION_ROLES.join(", ")
            );
        }
        for role in self.sounds.keys() {
            ensure!(
                SOUND_ROLES.contains(&role.as_ref()),
                "unknown sound role {role:?} (known: {})",
                SOUND_ROLES.join(", ")
            );
        }
        for role in ["idle", "raise", "drop"] {
            ensure!(
                self.animations.contains_key(role),
                "animations.{role} is required"
            );
        }
        match self.kind {
            Kind::Melee | Kind::Drink => {}
            Kind::Grenade | Kind::Monkey | Kind::Mine => ensure!(
                self.animations.contains_key("fire"),
                "animations.fire (the throw) is required"
            ),
            _ => {
                ensure!(self.ammo.magazine > 0, "ammo.magazine must be positive");
                ensure!(
                    self.animations.contains_key("fire"),
                    "animations.fire is required"
                );
                // The flamethrower is refuelled without an animation of its own
                ensure!(
                    self.kind == Kind::Flame
                        || self.animations.contains_key("reload")
                        || self.animations.contains_key("reload_empty"),
                    "a reload animation is required"
                );
            }
        }
        if self.ammo.segmented {
            for role in ["reload_start", "reload", "reload_end"] {
                ensure!(
                    self.animations.contains_key(role),
                    "a segmented reload needs animations.{role}"
                );
            }
        }
        if self.kind == Kind::Tesla && (self.fire.arcs == 0 || self.fire.arc_range == 0) {
            bail!("the Wunderwaffe needs fire.arcs and fire.arc_range");
        }
        Ok(())
    }

    /// The clip for `role`, if the weapon has one.
    pub fn clip(&self, role: &str) -> Option<&str> {
        self.animations.get(role).map(|clip| clip.as_ref())
    }

    /// Every distinct clip, sorted.
    pub fn clips(&self) -> Vec<String> {
        let mut clips: Vec<String> = self.animations.values().map(|c| c.to_string()).collect();
        clips.sort();
        clips.dedup();
        clips
    }

    /// Whether the weapon has a magazine to reload.
    pub fn reloads(&self) -> bool {
        !matches!(
            self.kind,
            Kind::Melee | Kind::Grenade | Kind::Monkey | Kind::Mine
        )
    }

    /// Whether it can be aimed down the sights.
    pub fn aims(&self) -> bool {
        self.animations.contains_key("ads_up") && self.animations.contains_key("ads_down")
    }
}

/// Every weapon in `folder`, by id (the file name).
pub fn load_all(folder: &Path) -> Result<BTreeMap<String, Weapon>> {
    let mut weapons = BTreeMap::new();
    let mut entries: Vec<_> = fs::read_dir(folder)
        .with_context(|| format!("Cannot read the weapons in {}", folder.display()))?
        .collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Weapon file names must be UTF-8")?
            .to_owned();
        weapons.insert(id, Weapon::load(&path)?);
    }
    ensure!(!weapons.is_empty(), "No weapons in {}", folder.display());
    let mut sprites = BTreeMap::new();
    let mut classes = BTreeMap::new();
    for (id, weapon) in &weapons {
        if let Some(other) = sprites.insert(weapon.sprite.to_uppercase(), id) {
            bail!("{id} and {other} share the sprite code {}", weapon.sprite);
        }
        if let Some(other) = classes.insert(weapon.class.to_lowercase(), id) {
            bail!("{id} and {other} share the class {}", weapon.class);
        }
        if let Some(upgrade) = &weapon.upgrade {
            let target = weapons.get(upgrade.as_ref()).with_context(|| {
                format!("{id}'s upgrade {upgrade} has no weapons/{upgrade}.toml")
            })?;
            ensure!(
                target.upgraded,
                "{id}'s upgrade {upgrade} must say upgraded = true"
            );
            // an upgrade's class is its weapon's, upgraded (Colt → ColtUpgraded)
            ensure!(
                *target.class == format!("{}Upgraded", weapon.class),
                "{id}'s upgrade {upgrade} must have class = \"{}Upgraded\", not {:?}",
                weapon.class,
                target.class
            );
        }
    }
    Ok(weapons)
}

#[cfg(test)]
mod defaults_tests {
    use super::*;

    #[test]
    fn the_default_weapons_load_their_upgrades_named_after_them() {
        let weapons = load_all(&Path::new(env!("CARGO_MANIFEST_DIR")).join("weapons")).unwrap();
        assert_eq!(weapons["colt_upgraded"].class, "ColtUpgraded");
        assert!(
            weapons
                .values()
                .filter(|weapon| weapon.upgraded)
                .all(|weapon| weapon.class.ends_with("Upgraded"))
        );
    }
}
