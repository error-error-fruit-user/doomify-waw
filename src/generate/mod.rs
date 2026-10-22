//! The text of the generated pk3: ZScript, SNDINFO, MAPINFO and KEYCONF.

pub mod base;
pub mod weapon;

use crate::{code::Code, project::Project, sounds::SoundSet, weapon::Kind};
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
    files.insert(
        "zscript/doomify/equipment.zs".into(),
        equipment(project, built),
    );
    for weapon in built {
        files.insert(
            format!("zscript/doomify/{}.zs", weapon.id),
            weapon::file(project, weapon),
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
    files.insert("SNDINFO.txt".into(), sndinfo(project, sounds));
    files.insert("MAPINFO.txt".into(), mapinfo(project, built));
    files.insert("KEYCONF.txt".into(), keyconf(built));
    files.insert(
        "DOOMIFY.txt".into(),
        formatdoc! {r#"
            Made by doomify-waw from the player's own copy of Call of Duty: World at War, for {p}.
            Everything in this folder is replaced when doomify runs again; don't share it.
        "#},
    );
    files
}

/// The equipment the equipment key goes through, the most valuable first: the monkey, the
/// Betty, the Bowie knife, then grenades.
fn equipment_order<'a>(built: &'a [Built<'a>]) -> Vec<&'a Built<'a>> {
    let rank = |kind| match kind {
        Kind::Monkey => Some(0),
        Kind::Mine => Some(1),
        Kind::Melee => Some(2),
        Kind::Grenade => Some(3),
        _ => None,
    };
    let mut order: Vec<_> = built
        .iter()
        .filter(|b| rank(b.weapon.kind).is_some())
        .collect();
    order.sort_by_key(|b| rank(b.weapon.kind));
    order
}

/// The equipment key: one key brings up the equipment, again the next piece the player has.
fn equipment(project: &Project, built: &[Built]) -> String {
    let p = &project.config.prefix;
    let order = equipment_order(built);
    let pushes = Code::new()
        .each(&order, |code, b| {
            code.always(format!(
                "order.Push((class<Weapon>)('{}'));",
                project.class(b.id)
            ))
        })
        .indented(|c| c);
    formatdoc! {r#"
        // The equipment key (KEYCONF's waw_equipment): brings up the most valuable equipment the
        // player has (the cymbal monkey, the Bouncing Betty, the Bowie knife, then grenades), and
        // pressed again the next one after what is in hand. Input arrives as a network event, so
        // every machine does the same.

        class {p}_DoomifyEquipment : EventHandler {{
            override void NetworkProcess(ConsoleEvent e) {{
                if (e.Name != "doomify_equipment" || e.Player < 0 || e.Player >= MAXPLAYERS || !playeringame[e.Player]) {{
                    return;
                }}
                let player = players[e.Player];
                if (player.mo == null || player.mo.health <= 0) {{
                    return;
                }}
                Array<class<Weapon> > order;
        {pushes}
                let current = player.ReadyWeapon;
                let start = 0;
                if (current != null) {{
                    let at = order.Find(current.GetClass());
                    if (at != order.Size()) {{
                        start = at + 1;
                    }}
                }}
                // the first press starts from the top of the list, the next ones after what is in hand
                for (let i = 0; i < order.Size(); ++i) {{
                    let held = Weapon(player.mo.FindInventory(order[(start + i) % order.Size()]));
                    if (held != null && held != current && (held.ammoType1 == null || player.mo.CountInv(held.ammoType1) > 0)) {{
                        player.PendingWeapon = held;
                        return;
                    }}
                }}
            }}
        }}
    "#, pushes = indent(&pushes.build(), 8)}
}

fn indent(text: &str, spaces: usize) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{}{line}", " ".repeat(spaces))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        .always("// The equipment key's handler, and the wall drawings' editor numbers, from doomify.toml")
        .block("GameInfo", |code| {
            code.always(format!("AddEventHandlers = \"{p}_DoomifyEquipment\""))
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

fn keyconf(built: &[Built]) -> String {
    let names: Vec<_> = equipment_order(built)
        .iter()
        .map(|b| b.weapon.name.clone())
        .collect();
    formatdoc! {r#"
        // The equipment key: {names}
        alias waw_equipment "netevent doomify_equipment"
        addkeysection "World at War" doomify_waw
        addmenukey "Equipment ({names})" waw_equipment
        defaultbind g waw_equipment
    "#, names = names.join(", ")}
}
