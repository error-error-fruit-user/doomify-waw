//! Running Blender on a job: the scene and script are built into doomify, written next to
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

/// A part of the render script, from its `RENDER_PARTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPart {
    pub name: String,
    /// The weapon kinds it draws; empty for every weapon ("*").
    pub kinds: Vec<String>,
    /// Changed so that it draws them differently: they are rendered again, once.
    pub changed: bool,
}

impl RenderPart {
    /// Whether it draws weapons of `kind` (snake_case, as in a TOML).
    pub fn draws(&self, kind: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind)
    }
}

/// The parts of the render script, as its `RENDER_PARTS` gives them
/// (`"name": {"kinds": "drink tesla", "changed": True},`, one to a line).
pub fn render_parts() -> Vec<RenderPart> {
    parts_of(SCRIPT)
}

fn parts_of(script: &str) -> Vec<RenderPart> {
    let Some(start) = script.find("RENDER_PARTS = {") else {
        return Vec::new();
    };
    let quoted = |text: &str, key: &str| -> Option<String> {
        let rest = &text[text.find(&format!("\"{key}\""))? + key.len() + 2..];
        let rest = &rest[rest.find('"')? + 1..];
        Some(rest[..rest.find('"')?].to_string())
    };
    script[start..]
        .lines()
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('}'))
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            let name = line.strip_prefix('"')?.split('"').next()?.to_string();
            let kinds = quoted(line, "kinds")?;
            Some(RenderPart {
                name,
                kinds: match kinds.as_str() {
                    "*" => Vec::new(),
                    kinds => kinds.split_whitespace().map(str::to_string).collect(),
                },
                changed: line.contains("\"changed\": True"),
            })
        })
        .collect()
}

/// A mark of the render script as it is, noted beside each weapon's renders: a part `changed`
/// renders the weapons it draws again while the mark they were rendered with differs.
pub fn script_mark() -> String {
    let hash = SCRIPT.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
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
pub fn run(
    command: &[String],
    work: &Path,
    job: &Path,
    run: fn(&mut Command, &str) -> std::io::Result<std::process::ExitStatus>,
) -> Result<()> {
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
    blender
        .args(arguments)
        .arg("--background")
        .arg(&scene)
        .arg("--python-exit-code")
        .arg("1")
        .arg("--python")
        .arg(&script)
        .arg("--")
        .arg("--job")
        .arg(job);
    let status = run(
        &mut blender,
        &format!(
            "{} --background weapon.blend --python render.py",
            command.join(" ")
        ),
    )
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
    fn the_render_script_names_its_parts() {
        assert!(
            render_parts()
                .iter()
                .any(|p| p.name == "views" && p.kinds.is_empty())
        );
        let parts = parts_of(
            "RENDER_PARTS = {\n    \"a\": {\"kinds\": \"*\"},  # x\n    \"b c\": {\"kinds\": \"drink tesla\", \"changed\": True},\n}\n",
        );
        assert_eq!(
            parts[1],
            RenderPart {
                name: "b c".into(),
                kinds: vec!["drink".into(), "tesla".into()],
                changed: true,
            }
        );
        assert!(!parts[0].changed && parts[0].draws("bullet"));
        assert!(parts[1].draws("tesla") && !parts[1].draws("bullet"));
    }
}
