//! The whole build for a map: check the game, pull and render in Blender, then assemble the pk3.

use crate::{
    blender,
    game::Game,
    generate::{self, weapon::Built},
    plan,
    project::Project,
    script::Pulled,
    sprites::{self, Sprites},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Options {
    pub blender: Option<PathBuf>,
    /// Build only these weapons (for trying things out; the pk3 then has only them).
    pub only: Vec<String>,
    /// Pull and render again even what is already there.
    pub force: bool,
    pub pid: Option<u32>,
    /// Wait for World at War to start.
    pub wait: bool,
    /// Don't run Blender: assemble from earlier pulls and renders.
    pub offline: bool,
}

pub fn build(project: &Project, options: &Options) -> Result<()> {
    let ids: Vec<String> = if options.only.is_empty() {
        project.weapons.keys().cloned().collect()
    } else {
        for id in &options.only {
            ensure!(project.weapons.contains_key(id), "No weapons/{id}.toml");
        }
        options.only.clone()
    };
    let work = project.work();
    fs::create_dir_all(&work)?;
    let work = work.canonicalize()?;
    let weapon_key = |id: &str| project.weapon_key(id);
    let sound_key = |id: &str| project.keyed(&format!("sounds-{id}"), &project.sound_file(id));
    let pull = |key: &str| work.join("pull").join(key);
    let frames = |key: &str| work.join("frames").join(key);
    // Caches from before weapons were keyed on what they render move to their new keys
    for id in &ids {
        let (old, new) = (project.legacy_weapon_key(id), weapon_key(id));
        for kind in ["pull", "frames"] {
            let (old, new) = (work.join(kind).join(&old), work.join(kind).join(&new));
            if !new.exists() && old.exists() {
                fs::rename(old, new)?;
            }
        }
    }

    // 🗃️ everything pulled and rendered already, every render it plans now among them (and
    // nothing forced): no game, no Blender
    let rendered_all = |id: &String| {
        let done: BTreeMap<String, usize> =
            fs::read_to_string(frames(&weapon_key(id)).join("rendered.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
        !done.is_empty()
            && plan::renders(&project.weapons[id])
                .iter()
                .all(|render| done.contains_key(&render.name))
    };
    let cached = !options.force
        && ids
            .iter()
            .all(|id| pull(&weapon_key(id)).join("pull.json").is_file() && rendered_all(id))
        && project
            .sound_sets
            .keys()
            .all(|id| pull(&sound_key(id)).join("pull.json").is_file());
    if cached {
        println!(
            "Every weapon and sound set is pulled and rendered already, in {}: World at War and Blender aren't needed (--force does it again)",
            work.display()
        );
    }
    if !options.offline && !cached {
        let game = Game::find(options.pid, options.wait)?;
        game.require_weapons(
            ids.iter()
                .map(|id| project.weapons[id].game.weapon.as_str()),
        )?;
        let exe = std::env::current_exe()?;
        let job = json!({
            "doomify": exe,
            "pid": game.pid,
            "sounds": project.sound_sets.keys().map(|id| json!({
                "id": format!("sounds-{id}"),
                "script": project.sound_file(id).canonicalize().unwrap_or(project.sound_file(id)),
                "pull": pull(&sound_key(id)),
                "force": options.force,
            })).collect::<Vec<_>>(),
            "weapons": ids.iter().map(|id| {
                let weapon = &project.weapons[id];
                json!({
                    "id": id,
                    "script": project.weapon_file(id).canonicalize().unwrap_or(project.weapon_file(id)),
                    "pull": pull(&weapon_key(id)),
                    "frames": frames(&weapon_key(id)),
                    "force": options.force,
                    "ads_fov": weapon.view.ads_fov,
                    "hold": weapon.view.hold,
                    "hold_in": weapon.view.hold_in,
                    "stance": {"in": weapon.clip("ads_up"), "out": weapon.clip("ads_down")},
                    "glow": plan::glow(weapon),
                    "renders": plan::renders(weapon),
                })
            }).collect::<Vec<_>>(),
        });
        drop(game);
        // Jobs of different maps may share the cache: one job file per map
        let job_path = work.join(format!("job-{}.json", project.config.prefix.to_lowercase()));
        fs::write(&job_path, serde_json::to_string_pretty(&job)?)?;
        let command = blender::find(options.blender.as_deref())?;
        println!(
            "Pulling from World at War and rendering in Blender ({})...",
            command.join(" ")
        );
        blender::run(&command, &work, &job_path)?;
    }

    assemble(project, &ids, &work)
}

/// The pk3 folder from what was pulled and rendered.
fn assemble(project: &Project, ids: &[String], work: &Path) -> Result<()> {
    let out = project.output();
    if out.exists() {
        ensure!(
            out.join("DOOMIFY.txt").exists(),
            "{} exists and wasn't made by doomify; not replacing it",
            out.display()
        );
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;
    // Marked first, so a build that fails halfway can still be replaced
    fs::write(
        out.join("DOOMIFY.txt"),
        "Made by doomify-waw; replaced when it runs again.\n",
    )?;
    let taken = sprites::existing(
        &project
            .config
            .sprites_of
            .iter()
            .map(|p| project.path(p))
            .collect::<Vec<_>>(),
    )?;

    // Sounds: every alias once, whoever pulled it.
    println!(
        "Gathering the sounds into {}",
        out.join("sounds/waw").display()
    );
    let mut sounds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut folders: Vec<PathBuf> = project
        .sound_sets
        .keys()
        .map(|id| {
            work.join("pull")
                .join(project.keyed(&format!("sounds-{id}"), &project.sound_file(id)))
        })
        .collect();
    folders.extend(
        ids.iter()
            .map(|id| work.join("pull").join(project.weapon_key(id))),
    );
    let mut pulls = BTreeMap::new();
    for folder in &folders {
        let pulled = Pulled::load(folder)?;
        for (alias, files) in &pulled.sounds {
            let alias = alias.to_lowercase();
            if sounds.contains_key(&alias) {
                continue;
            }
            let mut paths = vec![];
            for file in files {
                let stem = Path::new(file)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("Bad sound name")?;
                let source = ["ogg", "wav"]
                    .iter()
                    .map(|e| folder.join("ogg").join(format!("{stem}.{e}")))
                    .find(|p| p.exists())
                    .with_context(|| {
                        format!("{stem} wasn't converted; run doomify again with --force")
                    })?;
                let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("ogg");
                let target = format!("sounds/waw/{}.{extension}", stem.to_lowercase());
                fs::create_dir_all(out.join("sounds/waw"))?;
                fs::copy(&source, out.join(&target))?;
                paths.push(target);
            }
            sounds.insert(alias, paths);
        }
        pulls.insert(folder.clone(), pulled);
    }

    println!(
        "Gathered {} sounds",
        sounds.values().map(Vec::len).sum::<usize>()
    );

    // Sprites and code, a weapon per core at a time.
    let one = |index: usize| -> Result<Built> {
        let id = &ids[index];
        let weapon = &project.weapons[id];
        let key = project.weapon_key(id);
        let pulled = &pulls[&work.join("pull").join(&key)];
        let folder = work.join("frames").join(&key);
        let rendered: BTreeMap<String, usize> = serde_json::from_str(
            &fs::read_to_string(folder.join("rendered.json"))
                .with_context(|| format!("{id} wasn't rendered; run doomify without --offline"))?,
        )?;
        let mut sprites = Sprites::new(&weapon.sprite, out.join("sprites/waw").join(id))?;
        let mut frames = BTreeMap::new();
        for render in plan::renders(weapon) {
            let count = *rendered
                .get(&render.name)
                .with_context(|| format!("{id}'s {} wasn't rendered", render.name))?;
            frames.insert(
                render.name.clone(),
                sprites.sequence(&folder.join(&render.name), count)?,
            );
        }
        let pickup = if rendered.contains_key("world") {
            Some(sprites.standing(&folder.join("world/0000.png"))?)
        } else {
            None
        };
        let chalk = if pulled.chalk {
            Some(sprites.chalk(&work.join("pull").join(&key).join("chalk.png"))?)
        } else {
            None
        };
        for frame in frames
            .values()
            .flatten()
            .chain(pickup.iter())
            .chain(chalk.iter())
        {
            let (name, _) = frame.state();
            if name != "TNT1" && taken.contains(&name) {
                bail!(
                    "{id}'s sprite {name} is already a sprite of the map; give it another sprite code"
                );
            }
        }
        Ok(Built {
            id,
            weapon,
            pulled,
            frames,
            pickup,
            chalk,
            upgrade_built: weapon.upgrade.as_ref().is_some_and(|u| ids.contains(u)),
            downed_built: weapon.downed.as_ref().is_some_and(|d| ids.contains(d)),
        })
    };
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    println!(
        "Cutting the renders of {} weapons into sprites, {} at a time",
        ids.len(),
        cores.min(ids.len())
    );
    let next = std::sync::atomic::AtomicUsize::new(0);
    let cut = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<(usize, Result<Built>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..cores.min(ids.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = vec![];
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if index >= ids.len() {
                            break;
                        }
                        let built = one(index);
                        // 📣 each weapon said as it is done, so a long cut is seen going on
                        let count = cut.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        if let Ok(built) = &built {
                            let sprites: usize = built.frames.values().map(Vec::len).sum();
                            println!(
                                "Cut {} into {sprites} sprites ({count}/{})",
                                ids[index],
                                ids.len()
                            );
                        }
                        done.push((index, built));
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a sprite worker panicked"))
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    let built = results
        .into_iter()
        .map(|(_, built)| built)
        .collect::<Result<Vec<_>>>()?;

    let files = generate::files(project, &built, &sounds);
    println!("Writing {} files of code and definitions", files.len());
    for (path, text) in files {
        let target = out.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, text)?;
    }
    println!("Wrote {}", out.display());
    if let Some(pack) = &project.config.pack {
        let pack = project.path(pack);
        println!("Packing {} into {}", out.display(), pack.display());
        zip_folder(&out, &pack)?;
        println!("Packed {}", pack.display());
    }
    Ok(())
}

/// Zips `folder` into `target`, files in sorted order.
fn zip_folder(folder: &Path, target: &Path) -> Result<()> {
    let mut files = vec![];
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let temporary = target.with_extension("pk3.part");
    let mut zip = zip::ZipWriter::new(fs::File::create(&temporary)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for file in files {
        let name = file
            .strip_prefix(folder)?
            .to_str()
            .context("Non-UTF-8 file name")?
            .replace('\\', "/");
        zip.start_file(name, options)?;
        zip.write_all(&fs::read(&file)?)?;
    }
    zip.finish()?;
    fs::rename(&temporary, target)?;
    Ok(())
}
