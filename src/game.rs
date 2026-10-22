//! The running World at War, through hell-hound: found, opened read-only and checked for Der
//! Riese's weapons.

use anyhow::{Context, Result, bail, ensure};
use hell_hound::{
    archive::{self, Asset},
    discovery,
    live::{self, Mode},
    weapon::def::{self, WeaponDef},
};
use std::path::PathBuf;

pub struct Game {
    pub pid: u32,
    pub memory: live::Process,
    pub mode: Mode,
    pub image_base: u64,
    pub archives: Vec<PathBuf>,
}

impl Game {
    /// The running World at War (the one with `pid`, if given), waiting for it if `wait`.
    pub fn find(pid: Option<u32>, wait: bool) -> Result<Self> {
        let options = discovery::Options {
            game: Some("waw".into()),
            pid,
            wait,
            timeout: 0,
        };
        let games = discovery::wait_for_games(&options, wait).context(
            "Start World at War and load Der Riese (nazi_zombie_factory) in solo zombies",
        )?;
        if games.len() > 1 {
            let pids: Vec<_> = games.iter().map(|g| g.pid.to_string()).collect();
            bail!(
                "More than one World at War is running (PIDs {}); choose one with --pid",
                pids.join(", ")
            );
        }
        let game = games.into_iter().next().context("No World at War found")?;
        let mode = game.waw_mode()?;
        ensure!(
            matches!(mode, Mode::Sp),
            "World at War's multiplayer is running; doomify needs zombies (single player) with Der Riese loaded"
        );
        Ok(Self {
            pid: game.pid,
            memory: live::Process::open(game.pid)?,
            mode,
            image_base: game.image_base.unwrap_or(0x400000),
            archives: game.archives()?,
        })
    }

    pub fn weapons(&self) -> Result<Vec<WeaponDef>> {
        def::list(&self.memory, self.mode, self.image_base)
    }

    /// Fails, naming what is missing, unless every one of `names` is a loaded weapon.
    pub fn require_weapons<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let loaded = self.weapons()?;
        let missing: Vec<_> = names
            .into_iter()
            .filter(|name| !loaded.iter().any(|w| w.name == *name))
            .collect();
        ensure!(
            missing.is_empty(),
            "These weapons aren't loaded: {}. Load Der Riese (nazi_zombie_factory) in solo zombies and wait in the map",
            missing.join(", ")
        );
        Ok(())
    }

    /// The installation's IWD catalog (sounds that stream, images).
    pub fn catalog(&self) -> Result<Vec<Asset>> {
        archive::catalog(&self.archives)
    }
}
