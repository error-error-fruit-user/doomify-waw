//! doomify: World at War's Der Riese weapons and sounds, rebuilt for UZDoom maps from the
//! player's own running game, as a library a map's installer builds in. It reads a map's
//! `doomify.toml` and its weapon and sound set TOMLs ([`project::Project::load`]) and builds its
//! pk3 ([`pipeline::build`]). Blender's script pulls from the game by running the program that
//! called it with `--script TOML --out DIR [--pid N]`, which it hands to [`script::run`].
//! [`const_toml!`] builds TOMLs into a program as consts.
pub mod blender;
pub mod code;
pub mod const_toml;
pub mod game;
pub mod generate;
pub mod pipeline;
pub mod plan;
pub mod project;
pub mod script;
pub mod sounds;
pub mod sprites;
pub mod types;
pub mod weapon;

include!(concat!(env!("OUT_DIR"), "/defaults.rs"));
