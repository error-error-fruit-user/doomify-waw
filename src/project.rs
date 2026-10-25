//! A map's `doomify.toml`: where its weapons and sound sets are, the prefix its classes take,
//! where the built pk3 goes and the editor numbers of its wall drawings.

use crate::{
    sounds,
    sounds::SoundSet,
    types::{List, Map, Text},
    weapon,
    weapon::Weapon,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    /// The class prefix, without the underscore (`Nacht` → `Nacht_Thompson`).
    pub prefix: Text,
    /// The pk3 folder doomify writes (and replaces), relative to this file.
    pub output: Text,
    /// Also zip the folder into this pk3.
    #[serde(default)]
    pub pack: Option<Text>,
    /// Where pulls and renders are kept between runs; by default the user's cache, shared by
    /// every map (a weapon is rendered again only when its TOML changes).
    #[serde(default)]
    pub work: Option<Text>,
    #[serde(default = "weapons")]
    pub weapons: Text,
    #[serde(default = "sound_sets")]
    pub sounds: Text,
    /// World at War's damage times this is Doom's (the map's zombies have less health).
    #[serde(default = "damage_scale")]
    pub damage_scale: f64,
    /// Editor numbers for the wall drawings, by weapon id.
    #[serde(default)]
    pub editor_numbers: Map<u32>,
    /// The pk3 folders whose sprite names must not clash with the generated ones (the map's).
    #[serde(default)]
    pub sprites_of: List<Text>,
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
    pub name: Text,
    /// What it starts with: a weapon id and the rounds it brings.
    #[serde(default)]
    pub start: List<StartItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StartItem {
    pub weapon: Text,
    #[serde(default)]
    pub ammo: u32,
}

fn player_name() -> Text {
    Text::Borrowed("Marine (World at War)")
}

fn weapons() -> Text {
    Text::Borrowed("weapons")
}
fn sound_sets() -> Text {
    Text::Borrowed("sounds")
}
fn damage_scale() -> f64 {
    0.1
}

/// The user's cache folder for doomify (named `doomify-waw`, as it was, so its renders are kept).
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
    /// Keys caches may have from before they were keyed on the TOMLs' data (by a weapon's id, or
    /// `sounds-<id>`): moved to their keys now when those aren't there yet. `load` gives the
    /// ones of the files it read; a program that built its TOMLs in gives theirs
    /// ([`text_keys`]).
    pub legacy_keys: BTreeMap<String, Vec<String>>,
}

/// The keys a cache of a TOML of this `text` had when caches were keyed on the text (`name` is
/// the weapon's id, or `sounds-<id>`): what it renders, with the drinks' old render version, and
/// the whole file.
pub fn text_keys(name: &str, text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if !name.starts_with("sounds-")
        && let Ok(table) = text.parse::<toml::Table>()
    {
        let drink = table.get("kind").and_then(toml::Value::as_str) == Some("drink");
        let mut table = rendered(table);
        keys.push(hashed(name, table.to_string().as_bytes()));
        if drink {
            table.insert("render_version".into(), "2".into());
            keys.push(hashed(name, table.to_string().as_bytes()));
        }
    }
    keys.push(hashed(name, text.as_bytes()));
    keys
}

/// Of a weapon's table, only what its pulls and renders depend on.
fn rendered(mut table: toml::Table) -> toml::Table {
    table.retain(|key, _| RENDERED.contains(&key));
    if let Some(toml::Value::Table(ammo)) = table.get_mut("ammo") {
        ammo.retain(|key, _| key == "magazine");
    }
    if let Some(toml::Value::Table(wall)) = table.get_mut("wall") {
        wall.retain(|key, _| key == "chalk");
    }
    table
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

/// `id` and an FNV-1a hash of `content` and the scene: stable across runs and platforms, which
/// std's hasher is not. (The "2" is the render version the caches were keyed with before the
/// render script's parts were flagged `changed`, so they stay where they are.)
fn hashed(id: &str, content: &[u8]) -> String {
    let hash = [content, crate::blender::SCENE, b"2"]
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
        let text =
            fs::read_to_string(&file).with_context(|| format!("Cannot read {}", file.display()))?;
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
        let weapons = weapon::load_all(&root.join(config.weapons.as_ref()))?;
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
                .get(id.as_ref())
                .with_context(|| format!("editor_numbers names {id}, which has no weapon file"))?;
            ensure!(
                weapon.wall.is_some(),
                "editor_numbers: {id} has no [wall] to place"
            );
            if let Some(other) = numbers.insert(number, id) {
                bail!("editor number {number} is given to both {other} and {id}");
            }
        }
        for item in config.player.iter().flat_map(|player| player.start.iter()) {
            ensure!(
                weapons.contains_key(item.weapon.as_ref() as &str),
                "[player] starts with {}, which has no weapon file",
                item.weapon
            );
        }
        let sound_sets = sounds::load_all(&root.join(config.sounds.as_ref()))?;
        let mut project = Self {
            root,
            config,
            weapons,
            sound_sets,
            legacy_keys: BTreeMap::new(),
        };
        for id in project.weapons.keys() {
            let text = fs::read_to_string(project.weapon_file(id)).unwrap_or_default();
            project.legacy_keys.insert(id.clone(), text_keys(id, &text));
        }
        for id in project.sound_sets.keys() {
            let name = format!("sounds-{id}");
            let text = fs::read_to_string(project.sound_file(id)).unwrap_or_default();
            project
                .legacy_keys
                .insert(name.clone(), text_keys(&name, &text));
        }
        Ok(project)
    }

    pub fn path(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }

    pub fn output(&self) -> PathBuf {
        self.path(Path::new(self.config.output.as_ref()))
    }

    pub fn work(&self) -> PathBuf {
        match &self.config.work {
            Some(work) => self.path(Path::new(work.as_ref())),
            None => cache(),
        }
    }

    /// A weapon's cache key: a hash of only the data its pulls and renders depend on, so changing
    /// how it plays (its fire, its slot, its look in flight), or a comment, renders nothing again.
    pub fn weapon_key(&self, id: &str) -> String {
        let table = match toml::Value::try_from(&self.weapons[id]) {
            Ok(toml::Value::Table(table)) => table,
            _ => toml::Table::new(),
        };
        hashed(id, rendered(table).to_string().as_bytes())
    }

    /// A sound set's cache key: a hash of its data.
    pub fn sound_key(&self, id: &str) -> String {
        let data = toml::to_string(&self.sound_sets[id]).unwrap_or_default();
        hashed(&format!("sounds-{id}"), data.as_bytes())
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
        self.path(Path::new(self.config.weapons.as_ref()))
            .join(format!("{id}.toml"))
    }

    pub fn sound_file(&self, id: &str) -> PathBuf {
        self.path(Path::new(self.config.sounds.as_ref()))
            .join(format!("{id}.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULTS;

    #[test]
    fn a_toml_written_from_its_data_reads_as_the_same_data() {
        for (path, text) in DEFAULTS {
            if path.starts_with("weapons/") {
                let weapon: Weapon = toml::from_str(text).unwrap();
                let written = toml::to_string(&weapon).unwrap();
                assert_eq!(
                    toml::from_str::<Weapon>(&written).unwrap(),
                    weapon,
                    "{path}"
                );
            } else {
                let set: SoundSet = toml::from_str(text).unwrap();
                let written = toml::to_string(&set).unwrap();
                assert_eq!(toml::from_str::<SoundSet>(&written).unwrap(), set, "{path}");
            }
        }
    }

    #[test]
    fn the_old_keys_of_a_text_include_what_it_renders() {
        let text = "kind = \"drink\"\n# a comment\nslot = 0\n";
        let keys = text_keys("jugg", text);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[2], hashed("jugg", text.as_bytes()));
        assert_eq!(text_keys("sounds-zombie", text).len(), 1);
    }
}
