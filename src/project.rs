//! A map's `doomify.toml`: where its weapons and sound sets are, the prefix its classes take,
//! where the built pk3 goes and the editor numbers of its wall drawings.

use crate::{sounds, sounds::SoundSet, weapon, weapon::Weapon};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    /// The class prefix, without the underscore (`Nacht` → `Nacht_Thompson`).
    pub prefix: String,
    /// The pk3 folder doomify writes (and replaces), relative to this file.
    pub output: PathBuf,
    /// Also zip the folder into this pk3.
    #[serde(default)]
    pub pack: Option<PathBuf>,
    /// Where pulls and renders are kept between runs; by default the user's cache, shared by
    /// every map (a weapon is rendered again only when its TOML changes).
    #[serde(default)]
    pub work: Option<PathBuf>,
    #[serde(default = "weapons")]
    pub weapons: PathBuf,
    #[serde(default = "sound_sets")]
    pub sounds: PathBuf,
    /// World at War's damage times this is Doom's (the map's zombies have less health).
    #[serde(default = "damage_scale")]
    pub damage_scale: f64,
    /// Editor numbers for the wall drawings, by weapon id.
    #[serde(default)]
    pub editor_numbers: BTreeMap<String, u32>,
    /// The pk3 folders whose sprite names must not clash with the generated ones (the map's).
    #[serde(default)]
    pub sprites_of: Vec<PathBuf>,
    /// The player class the pk3 brings, if any: Doom's marine with World at War's start items.
    #[serde(default)]
    pub player: Option<Player>,
}

/// `[player]` in `doomify.toml`: a `<prefix>_Player` the generated pk3 makes the player class, a
/// `DoomPlayer` whose slots hold only World at War's weapons, starting with these.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Player {
    /// The name in the player class menu.
    #[serde(default = "player_name")]
    pub name: String,
    /// What it starts with: a weapon id and the rounds it brings.
    #[serde(default)]
    pub start: Vec<StartItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StartItem {
    pub weapon: String,
    #[serde(default)]
    pub ammo: u32,
}

fn player_name() -> String {
    "Marine (World at War)".into()
}

fn weapons() -> PathBuf {
    "weapons".into()
}
fn sound_sets() -> PathBuf {
    "sounds".into()
}
fn damage_scale() -> f64 {
    0.1
}

/// The user's cache folder for doomify-waw.
pub fn cache() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("doomify-waw")
}

pub struct Project {
    pub root: PathBuf,
    pub config: Config,
    pub weapons: BTreeMap<String, Weapon>,
    pub sound_sets: BTreeMap<String, SoundSet>,
}

/// What in a weapon's TOML its pulls and renders depend on: what to pull, the clips and sounds,
/// how it is drawn, its chalk and kind, and its magazine (the Wunderwaffe's bulbs).
const RENDERED: &[&str] = &[
    "kind",
    "game",
    "animations",
    "sounds",
    "view",
    "wall",
    "ammo",
];

/// `id` and an FNV-1a hash of `content`, the scene and the render version: stable across runs
/// and platforms, which std's hasher is not.
fn hashed(id: &str, content: &[u8]) -> String {
    let hash = [
        content,
        crate::blender::SCENE,
        crate::blender::RENDER_VERSION.as_bytes(),
    ]
    .iter()
    .flat_map(|part| part.iter())
    .fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    format!("{id}-{:08x}", hash as u32)
}

/// Sprite name starts that belong to others: Doom's empty sprite and Horde Hammer's.
const RESERVED_SPRITE_CODES: &[&str] = &["TN", "HH"];

impl Project {
    /// Reads `doomify.toml` in `root` (a folder) or the file `root` names.
    pub fn load(root: &Path) -> Result<Self> {
        let file = if root.is_dir() {
            root.join("doomify.toml")
        } else {
            root.to_path_buf()
        };
        let root = file
            .parent()
            .context("doomify.toml has no folder")?
            .to_path_buf();
        let text = fs::read_to_string(&file).with_context(|| {
            format!(
                "Cannot read {} (doomify-waw init writes one)",
                file.display()
            )
        })?;
        let config: Config =
            toml::from_str(&text).with_context(|| format!("Invalid {}", file.display()))?;
        ensure!(
            config.version == 1,
            "{}: unknown version {}",
            file.display(),
            config.version
        );
        ensure!(
            !config.prefix.is_empty()
                && config.prefix.chars().all(|c| c.is_ascii_alphanumeric())
                && config.prefix.starts_with(|c: char| c.is_ascii_alphabetic()),
            "prefix must be letters and digits, like Nacht"
        );
        ensure!(config.damage_scale > 0.0, "damage_scale must be positive");
        let weapons = weapon::load_all(&root.join(&config.weapons))?;
        for code in RESERVED_SPRITE_CODES {
            if let Some((id, _)) = weapons
                .iter()
                .find(|(_, w)| w.sprite.eq_ignore_ascii_case(code))
            {
                bail!("{id}: the sprite code {code} is taken");
            }
        }
        let mut numbers = BTreeMap::new();
        for (id, number) in &config.editor_numbers {
            let weapon = weapons
                .get(id)
                .with_context(|| format!("editor_numbers names {id}, which has no weapon file"))?;
            ensure!(
                weapon.wall.is_some(),
                "editor_numbers: {id} has no [wall] to place"
            );
            if let Some(other) = numbers.insert(number, id) {
                bail!("editor number {number} is given to both {other} and {id}");
            }
        }
        for item in config.player.iter().flat_map(|player| &player.start) {
            ensure!(
                weapons.contains_key(&item.weapon),
                "[player] starts with {}, which has no weapon file",
                item.weapon
            );
        }
        let sound_sets = sounds::load_all(&root.join(&config.sounds))?;
        Ok(Self {
            root,
            config,
            weapons,
            sound_sets,
        })
    }

    pub fn path(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }

    pub fn output(&self) -> PathBuf {
        self.path(&self.config.output)
    }

    pub fn work(&self) -> PathBuf {
        match &self.config.work {
            Some(work) => self.path(work),
            None => cache(),
        }
    }

    /// The cache folder for a TOML: its id and a hash of its text, the scene and the render
    /// version, so a change to any of them renders it again.
    pub fn keyed(&self, id: &str, file: &Path) -> String {
        hashed(id, &fs::read(file).unwrap_or_default())
    }

    /// A weapon's cache key: a hash of only what its pulls and renders depend on, so changing
    /// how it plays (its fire, its slot, its look in flight) renders nothing again.
    pub fn weapon_key(&self, id: &str) -> String {
        let text = fs::read_to_string(self.weapon_file(id)).unwrap_or_default();
        let rendered = match text.parse::<toml::Table>() {
            Ok(mut table) => {
                table.retain(|key, _| RENDERED.iter().any(|kept| *kept == key));
                if let Some(toml::Value::Table(ammo)) = table.get_mut("ammo") {
                    ammo.retain(|key, _| key == "magazine");
                }
                if let Some(toml::Value::Table(wall)) = table.get_mut("wall") {
                    wall.retain(|key, _| key == "chalk");
                }
                table.to_string()
            }
            Err(_) => text,
        };
        hashed(id, rendered.as_bytes())
    }

    /// The key a weapon's cache had before `weapon_key` (its whole file), to keep its renders.
    pub fn legacy_weapon_key(&self, id: &str) -> String {
        self.keyed(id, &self.weapon_file(id))
    }

    /// `Nacht_Thompson` for the weapon `thompson`.
    pub fn class(&self, id: &str) -> String {
        format!("{}_{}", self.config.prefix, self.weapons[id].class)
    }

    /// A class of the map's own generated code (`Nacht_` + `name`).
    pub fn named(&self, name: &str) -> String {
        format!("{}_{name}", self.config.prefix)
    }

    /// World at War's damage in Doom's numbers (at least 1).
    pub fn damage(&self, waw: u32) -> u32 {
        ((waw as f64 * self.config.damage_scale).round() as u32).max(1)
    }

    pub fn weapon_file(&self, id: &str) -> PathBuf {
        self.path(&self.config.weapons).join(format!("{id}.toml"))
    }

    pub fn sound_file(&self, id: &str) -> PathBuf {
        self.path(&self.config.sounds).join(format!("{id}.toml"))
    }
}
