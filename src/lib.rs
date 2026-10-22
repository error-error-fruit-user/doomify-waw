//! doomify-waw: World at War's Der Riese weapons and sounds, rebuilt for UZDoom maps from the
//! player's own running game.
pub mod blender;
pub mod code;
pub mod game;
pub mod generate;
pub mod pipeline;
pub mod plan;
pub mod project;
pub mod script;
pub mod sounds;
pub mod sprites;
pub mod weapon;

include!(concat!(env!("OUT_DIR"), "/defaults.rs"));
