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
/// The script that pulls with `--script` and renders.
pub const SCRIPT: &str = include_str!("../blender/render.py");

/// A part of the render script and its version, from its `RENDER_VERSIONS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPart {
    pub name: String,
    pub version: u32,
    /// The weapon kinds it draws; empty for every weapon ("*").
    pub kinds: Vec<String>,
}

/// The parts of the render script and their versions, as its `RENDER_VERSIONS` gives them
/// (`"name": (version, "kinds"),`, one to a line).
pub fn render_parts() -> Vec<RenderPart> {
    parts_of(SCRIPT)
}

fn parts_of(script: &str) -> Vec<RenderPart> {
    let Some(start) = script.find("RENDER_VERSIONS = {") else {
        return Vec::new();
    };
    script[start..]
        .lines()
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('}'))
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            let (name, rest) = line.strip_prefix('"')?.split_once("\": (")?;
            let (version, kinds) = rest.split_once(',')?;
            let kinds = kinds.trim().strip_prefix('"')?.split('"').next()?;
            Some(RenderPart {
                name: name.to_string(),
                version: version.trim().parse().ok()?,
                kinds: match kinds {
                    "*" => Vec::new(),
                    kinds => kinds.split_whitespace().map(str::to_string).collect(),
                },
            })
        })
        .collect()
}

/// The versions of the parts that draw `kind` (every weapon's when `None`), joined with dots:
/// part of a render's cache key.
pub fn render_version(kind: Option<&str>) -> String {
    render_parts()
        .iter()
        .filter(|part| match kind {
            None => part.kinds.is_empty(),
            Some(kind) => part.kinds.iter().any(|k| k == kind),
        })
        .map(|part| part.version.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

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
    // 🚿 Blender's own lines (Fra:, Saved:...) stream line by line into a pipe too, where stdbuf can
    // tell it to (not into flatpak's sandbox)
    let mut blender = if cfg!(target_os = "linux") && program != "flatpak" && which("stdbuf") {
        let mut stdbuf = Command::new("stdbuf");
        stdbuf.args(["-oL", "-eL"]).arg(program);
        stdbuf
    } else {
        Command::new(program)
    };
    let status = blender
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

/// Whether `name` is a program on the PATH.
fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|folder| folder.join(name).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_render_script_gives_its_parts_versions() {
        let parts = render_parts();
        assert!(parts.iter().any(|p| p.name == "views" && p.kinds.is_empty()));
        // the cache keys made before the parts had versions stay as they were
        assert_eq!(render_version(None), "2");
        assert_eq!(render_version(Some("drink")), "2");
        assert_eq!(render_version(Some("bullet")), "");
        let parts = parts_of(
            "RENDER_VERSIONS = {\n    \"a\": (3, \"*\"),  # x\n    \"b c\": (1, \"drink tesla\"),\n}\n",
        );
        assert_eq!(
            parts[1],
            RenderPart {
                name: "b c".into(),
                version: 1,
                kinds: vec!["drink".into(), "tesla".into()],
            }
        );
        assert_eq!(parts[0].version, 3);
    }
}
