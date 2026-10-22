//! What gets rendered for a weapon: every pose its states use, each from the clip of its role,
//! held at the hip, aimed, or moving between them.

use crate::weapon::{Kind, Weapon};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    Hip,
    Ads,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, Serialize)]
pub struct Render {
    /// The render's name, and its folder of frames (`fire`, `idle@2` for a variant).
    pub name: String,
    pub clip: String,
    pub stance: Stance,
    /// One frame, the clip's first.
    pub still: bool,
    /// How many of the Wunderwaffe's bulbs glow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lit: Option<u32>,
}

/// The bulbs that light up with the Wunderwaffe's rounds, and the parts of them that glow.
#[derive(Debug, Clone, Serialize)]
pub struct Glow {
    pub materials: Vec<String>,
    pub bones: Vec<String>,
}

/// How many bulbs the Wunderwaffe has: three, or fewer for a smaller magazine.
pub fn bulbs(weapon: &Weapon) -> u32 {
    weapon.ammo.magazine.clamp(1, 3)
}

/// How many bulbs glow with `rounds` in the magazine: each bulb stands for an equal share of it
/// (the DG-3 JZ's six rounds are two a bulb), lit while any of its share is left.
pub fn lit(weapon: &Weapon, rounds: u32) -> u32 {
    let magazine = weapon.ammo.magazine.max(1);
    (rounds * bulbs(weapon)).div_ceil(magazine)
}

/// Every count of lit bulbs, most first; just None for a weapon without them.
pub fn lit_counts(weapon: &Weapon) -> Vec<Option<u32>> {
    if weapon.kind == Kind::Tesla {
        (0..=bulbs(weapon)).rev().map(Some).collect()
    } else {
        vec![None]
    }
}

/// The counts of lit bulbs a shot can leave, most first; just None for a weapon without them.
pub fn shot_counts(weapon: &Weapon) -> Vec<Option<u32>> {
    if weapon.kind != Kind::Tesla {
        return vec![None];
    }
    let mut counts: Vec<_> = (1..=weapon.ammo.magazine)
        .map(|r| lit(weapon, r - 1))
        .collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    counts.dedup();
    counts.into_iter().map(Some).collect()
}

/// The Wunderwaffe's full count of lit bulbs (what reloads and first raises show).
pub fn full(weapon: &Weapon) -> Option<u32> {
    (weapon.kind == Kind::Tesla).then(|| bulbs(weapon))
}

pub fn glow(weapon: &Weapon) -> Option<Glow> {
    (weapon.kind == Kind::Tesla).then(|| Glow {
        materials: vec!["filament".into()],
        bones: (1..=bulbs(weapon)).map(|n| format!("j_bulb_{n}")).collect(),
    })
}

/// The name of `base` lit with `lit` bulbs (just `base` when the weapon has none).
pub fn variant(base: &str, lit: Option<u32>) -> String {
    match lit {
        Some(n) => format!("{base}@{n}"),
        None => base.to_owned(),
    }
}

pub fn renders(weapon: &Weapon) -> Vec<Render> {
    let mut renders = vec![];
    let clip = |role: &str| weapon.clip(role).map(str::to_owned);
    // The Wunderwaffe shows its rounds: a pose for every count of lit bulbs, and shots that
    // leave fewer.
    let counts = lit_counts(weapon);
    let shots = shot_counts(weapon);
    let mut add =
        |name: &str, clip: Option<String>, stance: Stance, still: bool, lit: &[Option<u32>]| {
            if let Some(clip) = clip {
                for lit in lit {
                    renders.push(Render {
                        name: variant(name, *lit),
                        clip: clip.clone(),
                        stance,
                        still,
                        lit: *lit,
                    });
                }
            }
        };
    let distinct = |role: &str, from: &str| clip(role).filter(|c| Some(c) != clip(from).as_ref());
    let all = &counts[..];
    // Reloads end full, and raising it for the first time it is full.
    let full = &[full(weapon)][..];

    add("idle", clip("idle"), Stance::Hip, true, all);
    add(
        "empty_idle",
        distinct("empty_idle", "idle"),
        Stance::Hip,
        true,
        &[None],
    );
    add("fire", clip("fire"), Stance::Hip, false, &shots);
    add(
        "last_fire",
        distinct("last_fire", "fire"),
        Stance::Hip,
        false,
        &shots[shots.len().saturating_sub(1)..],
    );
    add("rechamber", clip("rechamber"), Stance::Hip, false, &[None]);
    if weapon.aims() {
        add("ads_idle", clip("idle"), Stance::Ads, true, all);
        add(
            "ads_empty_idle",
            distinct("empty_idle", "idle"),
            Stance::Ads,
            true,
            &[None],
        );
        add(
            "ads_fire",
            clip("ads_fire").or(clip("fire")),
            Stance::Ads,
            false,
            &shots,
        );
        add(
            "ads_last_fire",
            clip("ads_last_fire").filter(|c| Some(c) != clip("ads_fire").as_ref()),
            Stance::Ads,
            false,
            &shots[shots.len().saturating_sub(1)..],
        );
        add(
            "ads_rechamber",
            clip("ads_rechamber"),
            Stance::Ads,
            false,
            &[None],
        );
        add("zoom_in", clip("idle"), Stance::ZoomIn, false, all);
        add("zoom_out", clip("idle"), Stance::ZoomOut, false, all);
    }
    add("reload", clip("reload"), Stance::Hip, false, full);
    add(
        "reload_empty",
        distinct("reload_empty", "reload"),
        Stance::Hip,
        false,
        full,
    );
    add(
        "reload_start",
        clip("reload_start"),
        Stance::Hip,
        false,
        &[None],
    );
    add(
        "reload_end",
        clip("reload_end"),
        Stance::Hip,
        false,
        &[None],
    );
    // A knife comes up and goes away at once: World at War only swings it
    let knife = weapon.kind == Kind::Melee;
    add("raise", clip("raise"), Stance::Hip, knife, all);
    add(
        "first_raise",
        distinct("first_raise", "raise"),
        Stance::Hip,
        false,
        full,
    );
    add("drop", clip("drop"), Stance::Hip, knife, all);
    renders
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon(text: &str) -> Weapon {
        toml::from_str(text).unwrap()
    }

    const BASE: &str = r#"
        name = "T"
        class = "T"
        sprite = "TT"
        slot = 1
        [game]
        weapon = "w"
        models = ["hands", "gun"]
        [ammo]
        magazine = 3
        [fire]
        time = 0.1
        [animations]
        idle = "idle"
        fire = "fire"
        reload = "reload"
        raise = "raise"
        drop = "drop"
        ads_up = "up"
        ads_down = "down"
    "#;

    #[test]
    fn plain_weapons_render_each_pose_once() {
        let w = weapon(&format!("kind = \"bullet\"\n{BASE}"));
        let names: Vec<_> = renders(&w).into_iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            [
                "idle", "fire", "ads_idle", "ads_fire", "zoom_in", "zoom_out", "reload", "raise",
                "drop"
            ]
        );
    }

    #[test]
    fn the_wunderwaffe_renders_its_bulbs() {
        let w = weapon(&format!("kind = \"tesla\"\n{BASE}"));
        let names: Vec<_> = renders(&w).into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"idle@3".to_owned()) && names.contains(&"idle@0".to_owned()));
        assert!(names.contains(&"fire@2".to_owned()) && !names.contains(&"fire@3".to_owned()));
        assert!(names.contains(&"reload@3".to_owned()) && !names.contains(&"reload@1".to_owned()));
    }

    #[test]
    fn six_rounds_light_three_bulbs_two_rounds_a_bulb() {
        let w =
            weapon(&format!("kind = \"tesla\"\n{BASE}").replace("magazine = 3", "magazine = 6"));
        assert_eq!(
            (0..=6).map(|r| lit(&w, r)).collect::<Vec<_>>(),
            [0, 1, 1, 2, 2, 3, 3]
        );
        let names: Vec<_> = renders(&w).into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"idle@3".to_owned()) && !names.contains(&"idle@6".to_owned()));
        assert_eq!(shot_counts(&w), [Some(3), Some(2), Some(1), Some(0)]);
    }
}
