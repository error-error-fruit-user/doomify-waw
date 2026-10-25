//! `--script FILE.toml --out DIR`, which the program built with doomify hands to [`run`]: pulls
//! what a TOML names from the running game.
//! The Blender script runs it for every weapon and sound set before rendering. A weapon's TOML
//! (see `weapon.rs`) pulls:
//!
//! - `view.glb`: its first-person models and every clip it names, the gun hung on the hands;
//! - `world.glb`: its world model, for the pickup sprite;
//! - `sounds/<alias>_<n>.wav` (or `.xwm`): every sound role's alias and every alias a clip's
//!   notetracks play, each variant numbered;
//! - `chalk.png`: its wall drawing;
//!
//! and a sound set's TOML (see `sounds.rs`) pulls its aliases. `pull.json` says what came out:
//! clip lengths and notetracks, and each alias's files.

use crate::{game::Game, sounds::SoundSet, weapon::Weapon};
use anyhow::{Context, Result, ensure};
use hell_hound::{
    archive, sound,
    weapon::{self, Problem, texture},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Clip {
    pub frames: u16,
    pub fps: f32,
    /// Notetracks: name and time in seconds.
    pub events: Vec<(String, f32)>,
}

/// What a pull wrote, read back by the build.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Pulled {
    pub clips: BTreeMap<String, Clip>,
    /// Alias → its files in `sounds/`, in variant order.
    pub sounds: BTreeMap<String, Vec<String>>,
    /// Aliases asked for that the game has no files for.
    pub silent: Vec<String>,
    pub world: bool,
    pub chalk: bool,
}

impl Pulled {
    pub fn load(folder: &Path) -> Result<Self> {
        let path = folder.join("pull.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("{} is missing: the pull didn't finish", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Runs a script, `pid` choosing the game when more than one runs.
pub fn run(script: &Path, out: &Path, pid: Option<u32>) -> Result<()> {
    let text = fs::read_to_string(script)
        .with_context(|| format!("Cannot read script {}", script.display()))?;
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("Invalid TOML in {}", script.display()))?;
    let game = Game::find(pid, false)?;
    fs::create_dir_all(out)?;
    let pulled = if table.contains_key("game") {
        let weapon = Weapon::load(script)?;
        pull_weapon(&game, &weapon, out)
            .with_context(|| format!("Cannot pull {}", script.display()))?
    } else {
        let set = SoundSet::load(script)?;
        let mut pulled = Pulled::default();
        pull_sounds(&game, set.aliases(), out, &mut pulled)?;
        pulled
    };
    fs::write(
        out.join("pull.json"),
        serde_json::to_string_pretty(&pulled)?,
    )?;
    println!("Pulled {} into {}", script.display(), out.display());
    Ok(())
}

/// The export's answers: the hands lack a few sleeve helper bones some clips move, and nothing
/// can be done about a texture the IWDs don't have, so both are left out; the files are ours to
/// replace.
fn answer(problem: &Problem) -> Result<bool> {
    match problem {
        Problem::MissingBones { .. } | Problem::OutputExists(_) => Ok(true),
        Problem::MissingTexture(member) => {
            eprintln!("doomify: going on without texture {member}");
            Ok(true)
        }
        Problem::NoTextureArchives => anyhow::bail!("{problem}"),
    }
}

fn export(game: &Game, models: &[String], clips: &[String], output: &Path) -> Result<()> {
    weapon::export(
        &game.memory,
        game.mode,
        game.image_base,
        &game.archives,
        output,
        &weapon::Options {
            models: models.to_vec(),
            animations: clips.to_vec(),
            ..Default::default()
        },
        &mut answer,
    )
}

/// The clips' lengths and notetracks, from the GLB the export wrote.
fn clips_of(glb: &Path) -> Result<BTreeMap<String, Clip>> {
    let bytes = fs::read(glb)?;
    ensure!(
        bytes.len() > 20 && &bytes[..4] == b"glTF",
        "Not a GLB: {}",
        glb.display()
    );
    let length = u32::from_le_bytes(bytes[12..16].try_into()?) as usize;
    let document: serde_json::Value = serde_json::from_slice(&bytes[20..20 + length])?;
    let mut clips = BTreeMap::new();
    if let Some(found) = document["scenes"][0]["extras"]["hellHoundClips"].as_object() {
        for (name, clip) in found {
            clips.insert(
                name.clone(),
                Clip {
                    frames: clip["frames"].as_u64().unwrap_or(0) as u16,
                    fps: clip["fps"].as_f64().unwrap_or(30.0) as f32,
                    events: serde_json::from_value(clip["events"].clone()).unwrap_or_default(),
                },
            );
        }
    }
    Ok(clips)
}

fn pull_weapon(game: &Game, weapon: &Weapon, out: &Path) -> Result<Pulled> {
    game.require_weapons([weapon.game.weapon.as_ref()])?;
    let mut pulled = Pulled::default();
    let view = out.join("view.glb");
    let models: Vec<String> = weapon.game.models.iter().map(|m| m.to_string()).collect();
    export(game, &models, &weapon.clips(), &view)?;
    pulled.clips = clips_of(&view)?;
    if let Some(world) = &weapon.game.world {
        export(game, &[world.to_string()], &[], &out.join("world.glb"))?;
        pulled.world = true;
    }
    if let Some(wall) = &weapon.wall {
        let member = format!("images/{}.iwi", wall.chalk).to_lowercase();
        let catalog = game.catalog()?;
        let asset = catalog
            .iter()
            .find(|a| a.member.to_lowercase() == member)
            .with_context(|| format!("The wall drawing {member} is not in the IWDs"))?;
        let png = texture::iwi_png(&archive::read_asset(asset, archive::DEFAULT_LIMIT)?)
            .with_context(|| format!("Cannot convert {member}"))?;
        fs::write(out.join("chalk.png"), png)?;
        pulled.chalk = true;
    }
    // The roles' sounds, and whatever the clips' notetracks play that is a sound.
    let mut aliases: Vec<String> = weapon.sounds.values().map(|a| a.to_string()).collect();
    for clip in pulled.clips.values() {
        aliases.extend(clip.events.iter().map(|(name, _)| name.clone()));
    }
    pull_sounds(game, aliases, out, &mut pulled)?;
    Ok(pulled)
}

/// Writes every variant of every alias in `wanted` that the game has; notetracks that are not
/// sounds (like `end`) are skipped quietly, named aliases without files are recorded.
fn pull_sounds(
    game: &Game,
    wanted: impl IntoIterator<Item = String>,
    out: &Path,
    pulled: &mut Pulled,
) -> Result<()> {
    let mut wanted: Vec<String> = wanted.into_iter().collect();
    wanted.sort();
    wanted.dedup();
    let aliases = sound::aliases(&game.memory, game.mode, game.image_base)?;
    let catalog = game.catalog()?;
    let folder: PathBuf = out.join("sounds");
    fs::create_dir_all(&folder)?;
    for name in wanted {
        let Some(alias) = aliases.iter().find(|a| a.name == name) else {
            continue;
        };
        let mut files = vec![];
        for (index, file) in alias.files.iter().enumerate() {
            // Placeholders like null.wav are silence the game plays for nothing.
            if file.name().ends_with("null.wav") {
                continue;
            }
            let bytes = sound::read(&game.memory, file, &catalog)
                .with_context(|| format!("Sound alias {name}"))?;
            let extension = sound::extension(&bytes).unwrap_or("wav");
            let file_name = format!("{name}_{index}.{extension}");
            fs::write(folder.join(&file_name), bytes)?;
            files.push(file_name);
        }
        if files.is_empty() {
            pulled.silent.push(name);
        } else {
            pulled.sounds.insert(name, files);
        }
    }
    Ok(())
}
