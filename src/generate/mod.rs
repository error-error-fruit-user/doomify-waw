//! The text of the generated pk3: ZScript, SNDINFO and MAPINFO. The equipment is Horde Hammer's
//! (HH_Weapon.Equipment, on its equipment key).

pub mod base;
pub mod weapon;

use crate::{code::Code, project::Project, sounds::SoundSet};
use indoc::formatdoc;
use std::collections::BTreeMap;
use weapon::{Built, sound};

/// Every file of text, by its path in the pk3.
pub fn files(
    project: &Project,
    built: &[Built],
    sounds: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, String> {
    let p = &project.config.prefix;
    let mut files = BTreeMap::new();
    files.insert("zscript/doomify/base.zs".into(), base::weapon(p));
    files.insert("zscript/doomify/shock.zs".into(), base::shock(p));
    for weapon in built {
        files.insert(
            format!("zscript/doomify/{}.zs", weapon.id),
            weapon::file(project, weapon),
        );
    }
    if let Some(player) = &project.config.player {
        files.insert(
            "zscript/doomify/player.zs".into(),
            player_class(project, player),
        );
    }
    let includes: Vec<_> = files.keys().cloned().collect();
    files.insert(
        "ZSCRIPT.zs".into(),
        Code::new()
            .always("version \"5.0.0\"")
            .gap()
            .always("// World at War's weapons, rebuilt by doomify-waw from the player's own game.")
            .each(includes, |code, path| {
                code.always(format!("#include \"{path}\""))
            })
            .build(),
    );
    files.insert("GLDEFS.txt".into(), base::shock_gldefs(p));
    files.insert("shaders/doomify/shock.fp".into(), base::SHOCK_SHADER.into());
    files.insert("SNDINFO.txt".into(), sndinfo(project, sounds));
    files.insert("MAPINFO.txt".into(), mapinfo(project, built));
    files.insert(
        "DOOMIFY.txt".into(),
        formatdoc! {r#"
            Made by doomify-waw from the player's own copy of Call of Duty: World at War, for {p}.
            Everything in this folder is replaced when doomify runs again; don't share it.
        "#},
    );
    files
}

/// `[player]`: `<prefix>_Player`, Doom's marine whose slots hold only World at War's weapons
/// (they put themselves in slots, `Weapon.SlotNumber`; Doom's would be in the mystery box), starting
/// with the weapons the map gives it.
fn player_class(project: &Project, player: &crate::project::Player) -> String {
    let p = &project.config.prefix;
    Code::new()
        .always("// The map's marine, from [player] in doomify.toml: Doom's, with World at War's start items.")
        .block(format!("class {p}_Player : DoomPlayer"), |code| {
            code.block("Default", |code| {
                code.always(format!("Player.DisplayName \"{}\";", player.name.replace('"', "'")))
                    .always("// Doom's slots, emptied: only World at War's weapons fill them")
                    .each(&(1..=7).collect::<Vec<_>>(), |code, slot| {
                        code.always(format!("Player.WeaponSlot {slot}, \"\";"))
                    })
                    .always("// a start item gets no ammo of its own (Weapon.AmmoGive is for pickups)")
                    .each(&player.start, |code, item| {
                        let class = project.class(&item.weapon);
                        code.always(format!("Player.StartItem \"{class}\";"))
                            .when(item.ammo > 0, |code| {
                                code.always(format!("Player.StartItem \"{class}Ammo\", {};", item.ammo))
                            })
                    })
            })
        })
        .build()
}

/// Every pulled alias as `waw/<alias>` (a `$random` of its variants), and the sound sets' names
/// as aliases of them.
fn sndinfo(project: &Project, sounds: &BTreeMap<String, Vec<String>>) -> String {
    Code::new()
        .always("// World at War's sounds, as the game names them (waw/<alias>), pulled by doomify-waw.")
        .gap()
        .each(sounds, |code, (alias, files)| {
            let name = sound(alias);
            code.when(files.len() == 1, |code| code.always(format!("{name} {}", files[0])))
                .otherwise(|code| {
                    code.each(files.iter().enumerate(), |code, (index, file)| {
                        code.always(format!("{name}_{index} {file}"))
                    })
                    .always(format!(
                        "$random {name} {{ {} }}",
                        (0..files.len())
                            .map(|i| format!("{name}_{i}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ))
                })
        })
        .each(&project.sound_sets, |code, (id, set): (&String, &SoundSet)| {
            code.gap()
                .always(format!("// {} (sounds/{id}.toml)", set.name))
                .each(&set.sounds, |code, (name, entry)| {
                    code.when(sounds.contains_key(&entry.alias), |code| {
                        code.always(format!("$alias {name} {}", sound(&entry.alias)))
                    })
                })
        })
        .build()
}

fn mapinfo(project: &Project, built: &[Built]) -> String {
    let p = &project.config.prefix;
    let numbers: Vec<_> = project
        .config
        .editor_numbers
        .iter()
        .filter(|(id, _)| built.iter().any(|b| b.id == id.as_str()))
        .collect();
    Code::new()
        .always("// The Wunderwaffe's shock on screen, and the wall drawings' editor numbers (doomify.toml)")
        .block("GameInfo", |code| {
            code.always(format!("AddEventHandlers = \"{p}_DoomifyShock\""))
                // 🧍 the map's marine with World at War's start items, over Doom's (doomify.toml)
                .when(project.config.player.is_some(), |code| {
                    code.always(format!("PlayerClasses = \"{p}_Player\", \"DoomPlayer\""))
                })
        })
        .when(!numbers.is_empty(), |code| {
            code.gap().block("DoomEdNums", |code| {
                code.each(&numbers, |code, (id, number)| {
                    code.always(format!("{number} = \"{}Drawing\"", project.class(id)))
                })
            })
        })
        .build()
}
