use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use doomify_waw::{
    DEFAULTS,
    pipeline::{self, Options},
    project::Project,
    script,
};
use indoc::formatdoc;
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "Rebuilds World at War's Der Riese weapons and sounds for UZDoom maps, from your own running game"
)]
struct Cli {
    /// Pull what a weapon's or sound set's TOML names from the running game into --out (what
    /// the Blender script runs).
    #[arg(long, value_name = "TOML")]
    script: Option<PathBuf>,
    /// Where --script writes.
    #[arg(long, requires = "script")]
    out: Option<PathBuf>,
    /// The World at War process, when more than one runs.
    #[arg(long, global = true)]
    pid: Option<u32>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Pull, render and build the map's pk3 (what running without a command does, in the
    /// current folder).
    Build {
        /// The folder with doomify.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        blender: Option<PathBuf>,
        /// Only these weapons (by file name), to try something out.
        #[arg(long)]
        only: Vec<String>,
        /// Pull and render again what is already there.
        #[arg(long)]
        force: bool,
        /// Wait for World at War to start.
        #[arg(long)]
        wait: bool,
        /// Don't run Blender or read the game: assemble from earlier pulls and renders.
        #[arg(long)]
        offline: bool,
    },
    /// Write a doomify.toml and the default Der Riese weapons and sound sets into a folder.
    Init {
        folder: PathBuf,
        /// The map's class prefix, like Nacht.
        #[arg(long)]
        prefix: String,
        /// The pk3 folder to build, relative to the new doomify.toml.
        #[arg(long)]
        output: PathBuf,
        /// Replace weapon and sound files that are already there.
        #[arg(long)]
        overwrite: bool,
    },
}

fn init(folder: PathBuf, prefix: String, output: PathBuf, overwrite: bool) -> Result<()> {
    fs::create_dir_all(&folder)?;
    let config = folder.join("doomify.toml");
    if !config.exists() {
        fs::write(
            &config,
            formatdoc! {r#"
                # doomify-waw's settings for this map. The weapons are weapons/*.toml, the zombies' and
                # other sounds sounds/*.toml; run doomify-waw in this folder with Der Riese loaded.
                version = 1
                prefix = "{prefix}"
                output = "{output}"
                damage_scale = 0.1
                # The map's own pk3 folders, so generated sprite names never clash with its sprites
                sprites_of = []

                # Editor numbers for the wall drawings placed in the map, by weapon
                [editor_numbers]
            "#, output = output.display()},
        )?;
        println!("Wrote {}", config.display());
    }
    for (path, text) in DEFAULTS {
        let target = folder.join(path);
        if target.exists() && !overwrite {
            continue;
        }
        fs::create_dir_all(target.parent().context("bad default path")?)?;
        fs::write(&target, text)?;
    }
    println!("Wrote {} default weapons and sound sets", DEFAULTS.len());
    Ok(())
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(path) = cli.script {
        let out = cli.out.context("--script needs --out DIR")?;
        return script::run(&path, &out, cli.pid);
    }
    match cli.command.unwrap_or(Command::Build {
        project: ".".into(),
        blender: None,
        only: vec![],
        force: false,
        wait: false,
        offline: false,
    }) {
        Command::Build {
            project,
            blender,
            only,
            force,
            wait,
            offline,
        } => {
            let project = Project::load(&project)?;
            pipeline::build(
                &project,
                &Options {
                    blender,
                    only,
                    force,
                    pid: cli.pid,
                    wait,
                    offline,
                },
            )
        }
        Command::Init {
            folder,
            prefix,
            output,
            overwrite,
        } => {
            ensure!(!prefix.is_empty(), "--prefix is needed");
            init(folder, prefix, output, overwrite)
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("doomify-waw: {error:#}");
        std::process::exit(1);
    }
}
