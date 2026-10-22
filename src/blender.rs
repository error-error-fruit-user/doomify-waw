//! Running Blender on a job: the scene and script are built into doomify-waw, written next to
//! the job and run headless.

use anyhow::{Context, Result, bail, ensure};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// The scene every weapon is rendered in (rebuilt with `blender/make_blend.py`).
pub const SCENE: &[u8] = include_bytes!("../blender/weapon.blend");
/// What the scene and script render: raise it when a change to either changes the frames, so
/// cached renders are made again (other changes to the script keep them).
pub const RENDER_VERSION: &str = "2";
/// The script that pulls with `--script` and renders.
pub const SCRIPT: &str = include_str!("../blender/render.py");

/// The Blender to run: `explicit`, else $BLENDER, else `blender` on the PATH, else the usual
/// install places.
pub fn find(explicit: Option<&Path>) -> Result<Vec<String>> {
    if let Some(path) = explicit {
        return Ok(vec![path.display().to_string()]);
    }
    if let Ok(path) = env::var("BLENDER")
        && !path.is_empty()
    {
        return Ok(vec![path]);
    }
    let names: &[&str] = if cfg!(windows) {
        &["blender.exe"]
    } else {
        &["blender"]
    };
    if let Some(paths) = env::var_os("PATH") {
        for folder in env::split_paths(&paths) {
            for name in names {
                let candidate = folder.join(name);
                if candidate.is_file() {
                    return Ok(vec![candidate.display().to_string()]);
                }
            }
        }
    }
    let mut places: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        for root in [
            r"C:\Program Files\Blender Foundation",
            r"C:\Program Files (x86)\Steam\steamapps\common",
        ] {
            if let Ok(entries) = fs::read_dir(root) {
                for entry in entries.flatten() {
                    places.push(entry.path().join("blender.exe"));
                }
            }
        }
    } else if let Some(home) = env::var_os("HOME") {
        places.push(PathBuf::from(&home).join(".steam/steam/steamapps/common/Blender/blender"));
    }
    if let Some(found) = places.into_iter().filter(|p| p.is_file()).max() {
        return Ok(vec![found.display().to_string()]);
    }
    if !cfg!(windows)
        && Command::new("flatpak")
            .args(["info", "org.blender.Blender"])
            .output()
            .is_ok_and(|o| o.status.success())
    {
        return Ok(
            ["flatpak", "run", "--filesystem=host", "org.blender.Blender"]
                .map(String::from)
                .to_vec(),
        );
    }
    bail!("Blender wasn't found: install it (blender.org), or give --blender PATH or $BLENDER")
}

/// Runs the job file `job` with Blender `command`, the scene and script written into `work`.
pub fn run(command: &[String], work: &Path, job: &Path) -> Result<()> {
    fs::create_dir_all(work)?;
    let scene = work.join("weapon.blend");
    let script = work.join("render.py");
    fs::write(&scene, SCENE)?;
    fs::write(&script, SCRIPT)?;
    let (program, arguments) = command.split_first().context("No Blender command")?;
    let status = Command::new(program)
        .args(arguments)
        .arg("--background")
        .arg(&scene)
        .arg("--python-exit-code")
        .arg("1")
        .arg("--python")
        .arg(&script)
        .arg("--")
        .arg("--job")
        .arg(job)
        .status()
        .with_context(|| format!("Cannot run Blender ({program})"))?;
    ensure!(
        status.success(),
        "Blender failed ({status}); its output above says why"
    );
    Ok(())
}
