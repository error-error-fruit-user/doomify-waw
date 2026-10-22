//! A sound set's TOML: Doom sound names and the game's aliases they come from, for sounds that
//! belong to no weapon (the zombies' voices). An alias with several variants becomes a
//! `$random` of them, as the game picks one at random.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    /// The game's alias.
    pub alias: String,
    /// What it is for, written next to it in SNDINFO.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundSet {
    /// What the set is, for SNDINFO's comments.
    pub name: String,
    /// Doom sound name → where it comes from.
    pub sounds: BTreeMap<String, Sound>,
}

impl SoundSet {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
        let set: Self = toml::from_str(&text)
            .with_context(|| format!("Invalid sound set {}", path.display()))?;
        for name in set.sounds.keys() {
            ensure!(
                !name.is_empty() && !name.contains(char::is_whitespace),
                "{}: sound names have no spaces: {name:?}",
                path.display()
            );
        }
        Ok(set)
    }

    pub fn aliases(&self) -> Vec<String> {
        self.sounds.values().map(|s| s.alias.clone()).collect()
    }
}

/// Every sound set in `folder`, by id (the file name).
pub fn load_all(folder: &Path) -> Result<BTreeMap<String, SoundSet>> {
    let mut sets = BTreeMap::new();
    if !folder.exists() {
        return Ok(sets);
    }
    let mut entries: Vec<_> = fs::read_dir(folder)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Sound set file names must be UTF-8")?
                .to_owned();
            sets.insert(id, SoundSet::load(&path)?);
        }
    }
    Ok(sets)
}
