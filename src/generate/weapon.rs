//! One weapon's ZScript: its ammo, the weapon and its states, what it fires and its wall drawing,
//! from its TOML, its pulled clips and its rendered sprites.

use crate::{
    code::Code,
    plan::{self, variant},
    project::Project,
    script::Pulled,
    sprites::Frame,
    weapon::{Kind, Weapon},
};
use indoc::formatdoc;
use std::collections::BTreeMap;

/// What the generator knows about a weapon once it is rendered.
pub struct Built<'a> {
    pub id: &'a str,
    pub weapon: &'a Weapon,
    pub pulled: &'a Pulled,
    /// Render name → its frames.
    pub frames: BTreeMap<String, Vec<Frame>>,
    pub pickup: Option<Frame>,
    pub chalk: Option<Frame>,
    /// Its upgrade is in this build (a build of only some weapons may leave it out).
    pub upgrade_built: bool,
    /// Its last stand replacement is in this build.
    pub downed_built: bool,
}

/// How long a frame lasts and what it does.
#[derive(Clone, Copy)]
enum Tic {
    /// One tic.
    Plain,
    /// Sped up by Double Tap.
    Fire,
    /// Sped up by Speed Cola.
    Reload,
    /// The player may act again.
    Recover,
    /// Drinking: a tic and a half on average, a tic with Speed Cola.
    Sip,
}

impl Tic {
    fn suffix(self) -> &'static str {
        match self {
            Self::Plain => "1",
            Self::Fire => "1 FireTic",
            Self::Reload => "1 ReloadTic",
            Self::Recover => "1 Recover",
            Self::Sip => "1 Sip",
        }
    }
}

/// `frames` without the empty frames before and after what shows (World at War swings knives and
/// throws grenades in from off screen).
fn visible(frames: &[Frame]) -> &[Frame] {
    let start = frames
        .iter()
        .position(|f| *f != Frame::Empty)
        .unwrap_or(frames.len());
    let end = frames
        .iter()
        .rposition(|f| *f != Frame::Empty)
        .map_or(start, |e| e + 1);
    &frames[start..end]
}

/// A Doom sound name for a game alias.
pub fn sound(alias: &str) -> String {
    format!("waw/{}", alias.to_lowercase())
}

/// Frame lines: neighbouring frames of one sprite share a line; `before` puts zero-tic lines (the
/// notetracks' sounds) before the frame they belong to.
fn frames(
    mut code: Code,
    frames: &[Frame],
    tic: Tic,
    before: &BTreeMap<usize, Vec<String>>,
) -> Code {
    let mut run: Option<(String, String)> = None;
    let flush = |code: Code, run: &mut Option<(String, String)>| match run.take() {
        Some((sprite, letters)) => code.always(format!("{sprite} {letters} {};", tic.suffix())),
        None => code,
    };
    for (index, frame) in frames.iter().enumerate() {
        if let Some(lines) = before.get(&index) {
            code = flush(code, &mut run);
            for line in lines {
                code = code.always(line);
            }
        }
        let (sprite, letter) = frame.state();
        match &mut run {
            Some((current, letters)) if *current == sprite => letters.push(letter),
            _ => {
                code = flush(code, &mut run);
                run = Some((sprite, letter.to_string()));
            }
        }
    }
    flush(code, &mut run)
}

impl Built<'_> {
    fn has(&self, render: &str) -> bool {
        self.frames.contains_key(render)
    }

    fn get(&self, render: &str) -> &[Frame] {
        self.frames.get(render).map(Vec::as_slice).unwrap_or(&[])
    }

    /// A still pose's frame as `SPRT A`. Equipment World at War keeps off screen until it is
    /// used (grenades, the monkey, the knife) is held as its throw or swing shows it most.
    fn still(&self, render: &str) -> String {
        let mut frame = self.get(render).first().cloned().unwrap_or(Frame::Empty);
        if frame == Frame::Empty && render.starts_with("idle") {
            // the throw's frame that shows most of it: wound up, the thing in hand
            frame = match self.weapon.view.hold_frame {
                Some(index) => self.get("fire").get(index).cloned(),
                None => visible(self.get("fire"))
                    .iter()
                    .max_by_key(|f| f.area())
                    .cloned(),
            }
            .unwrap_or(Frame::Empty);
        }
        let (sprite, letter) = frame.state();
        format!("{sprite} {letter}")
    }

    /// The notetrack sounds of the clip of `role`, by the frame they play on.
    /// The frame of `role`'s clip where its notetrack `track` falls.
    fn event_frame(&self, role: &str, track: &str) -> Option<usize> {
        let clip = self.weapon.clip(role)?;
        let pulled = self
            .pulled
            .clips
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(clip))
            .map(|(_, c)| c)?;
        pulled
            .events
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(track))
            .map(|(_, time)| (*time as f64 * 35.0).round() as usize)
    }

    /// A reload's (or first raise's) frames and sounds; the Wunderwaffe's are dark until its
    /// switch turns the bulbs on (`view.lights_on`), where it jolts to life (`Jolt`), and lit
    /// from there.
    fn switched_frames(
        &self,
        role: &str,
        render: &str,
    ) -> (Vec<Frame>, BTreeMap<usize, Vec<String>>) {
        let lit = self.get(render);
        let mut events = self.events(role, lit.len());
        let dark = variant(role, Some(0));
        let switch = self
            .weapon
            .view
            .lights_on
            .as_deref()
            .filter(|_| self.has(&dark))
            .and_then(|track| self.event_frame(role, track))
            .filter(|on| *on < lit.len());
        let Some(on) = switch else {
            return (lit.to_vec(), events);
        };
        let dark = self.get(&dark);
        let frames = dark[..on.min(dark.len())]
            .iter()
            .chain(&lit[on..])
            .cloned()
            .collect();
        events.entry(on).or_default().push("TNT1 A 0 Jolt;".into());
        (frames, events)
    }

    fn events(&self, role: &str, count: usize) -> BTreeMap<usize, Vec<String>> {
        let mut events: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let Some(clip) = self.weapon.clip(role) else {
            return events;
        };
        let Some(pulled) = self
            .pulled
            .clips
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(clip))
            .map(|(_, c)| c)
        else {
            return events;
        };
        for (name, time) in &pulled.events {
            if self.pulled.sounds.contains_key(name) {
                let frame = ((*time as f64 * 35.0).round() as usize).min(count.saturating_sub(1));
                events.entry(frame).or_default().push(format!(
                    "TNT1 A 0 A_StartSound(\"{}\", CHAN_AUTO);",
                    sound(name)
                ));
            }
        }
        events
    }

    /// The sound of `role`, as a zero-tic line, if the weapon has one that was pulled.
    fn sound_line(&self, role: &str, channel: &str) -> Option<String> {
        let alias = self.weapon.sounds.get(role)?;
        self.pulled
            .sounds
            .contains_key(alias)
            .then(|| format!("TNT1 A 0 A_StartSound(\"{}\", {channel});", sound(alias)))
    }

    /// The visible frames of `render` (see `visible`) and the notetrack sounds of `role` on them.
    fn trimmed(&self, render: &str, role: &str) -> (&[Frame], BTreeMap<usize, Vec<String>>) {
        let all = self.get(render);
        let shown = visible(all);
        let start = all.iter().position(|f| *f != Frame::Empty).unwrap_or(0);
        let mut events: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (frame, lines) in self.events(role, all.len()) {
            let at = frame
                .saturating_sub(start)
                .min(shown.len().saturating_sub(1));
            events.entry(at).or_default().extend(lines);
        }
        (shown, events)
    }

    /// Tics in a shot: World at War's fire time at 35 a second, at least one.
    fn cycle(&self) -> usize {
        ((self.weapon.fire.time * 35.0).round() as usize).max(1)
    }
}

/// The line that fires a shot, aimed or not.
fn shot(project: &Project, built: &Built, aimed: bool) -> String {
    let weapon = built.weapon;
    let damage = project.damage(weapon.fire.damage);
    let spread = if aimed {
        weapon.fire.ads_spread
    } else {
        weapon.fire.spread
    };
    match weapon.kind {
        Kind::Bullet | Kind::Spread => format!(
            "TNT1 A 0 ShootBullets({spread:.1}, {damage}, {});",
            weapon.fire.pellets
        ),
        Kind::Projectile | Kind::Tesla => format!(
            "TNT1 A 0 ShootProjectile(\"{}Shot\");",
            project.class(built.id)
        ),
        Kind::Flame => format!("TNT1 A 0 ShootFlame(\"{}Shot\");", project.class(built.id)),
        Kind::Grenade | Kind::Monkey | Kind::Mine | Kind::Melee | Kind::Drink => String::new(),
    }
}

/// A firing state: the shot on its first frame, the shot's frames sped up by Double Tap, then
/// (automatic weapons) the trigger checked, then the rest of the animation, which the player may
/// cut short. Weapons that work a bolt or pump between shots go on to rechamber.
fn fire_state(
    code: Code,
    project: &Project,
    built: &Built,
    label: &str,
    render: &str,
    aimed: bool,
    last: bool,
) -> Code {
    let weapon = built.weapon;
    let all = built.get(render);
    let cycle = built.cycle().min(all.len());
    let (shooting, rest) = all.split_at(cycle);
    let sound = if last {
        built.sound_line("last_fire", "CHAN_WEAPON")
    } else {
        None
    }
    .or_else(|| built.sound_line("fire", "CHAN_WEAPON"));
    let rechamber = if aimed { "ZoomRechamber" } else { "Rechamber" };
    let has_rechamber = built.has(if aimed { "ads_rechamber" } else { "rechamber" });
    code.always(format!("{label}:")).indented(|code| {
        code.always("TNT1 A 0 ResetClock;")
            .when_some(sound, |code, line| code.always(line))
            .when_some(
                last.then(|| built.sound_line("eject", "CHAN_AUTO"))
                    .flatten(),
                |code, line| code.always(line),
            )
            .always(shot(project, built, aimed))
            .when(weapon.kind == Kind::Tesla, |code| {
                code.always(r#"TNT1 A 0 A_SetBlend("7fd8ff", 0.3, 10);"#)
            })
            .append(frames(Code::new(), shooting, Tic::Fire, &BTreeMap::new()))
            .when(weapon.fire.auto && !last, |code| {
                code.always("TNT1 A 0 A_ReFire;")
            })
            .when(has_rechamber && !last, |code| {
                code.always(format!("TNT1 A 0 WhenAnyActiveAmmo(\"{rechamber}\");"))
            })
            .append(frames(Code::new(), rest, Tic::Recover, &BTreeMap::new()))
            .always(if aimed {
                "Goto ZoomReady;"
            } else {
                "Goto Ready;"
            })
    })
}

/// The weapon's states.
fn states(project: &Project, built: &Built) -> Code {
    let weapon = built.weapon;
    let tesla = weapon.kind == Kind::Tesla;
    let thrown = matches!(weapon.kind, Kind::Grenade | Kind::Monkey | Kind::Mine);
    let melee = weapon.kind == Kind::Melee;
    let gun = !thrown && !melee;
    let aims = gun && built.has(&variant("ads_idle", plan::full(weapon)));
    let counts = plan::lit_counts(weapon);
    // The Wunderwaffe's labels end in how many of its bulbs glow: Idle3, Idle2...
    let label = |base: &str, lit: Option<u32>| match lit {
        Some(n) => format!("{base}{n}"),
        None => base.to_owned(),
    };
    // Every round count goes to the pose with as many bulbs lit as it shows
    let dispatch = |code: Code, base: &str| {
        code.when(tesla, |code| {
            code.each(0..=weapon.ammo.magazine, |code, rounds| {
                code.always(format!(
                    "TNT1 A 0 WhenActiveAmmo({rounds}, \"{}\");",
                    label(base, Some(plan::lit(weapon, rounds)))
                ))
            })
        })
    };
    let pickup = built.pickup.clone().unwrap_or(Frame::Empty).state();
    let full = plan::full(weapon);

    Code::new()
        .always("Spawn:")
        .indented(|code| {
            code.always(format!("{} {} -1;", pickup.0, pickup.1))
                .always("Stop;")
        })
        // Raising: the animation from where the weapon rests, then straight to Ready
        .always("Select:")
        .indented(|code| {
            code.always("TNT1 A 0 AtRest;")
                .always("TNT1 A 0 ResetClock;")
                .when(built.has(&variant("first_raise", full)), |code| {
                    code.always("TNT1 A 0 WhenFirstRaise(\"FirstRaise\");")
                })
                .when(built.has("empty_raise"), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"EmptyRaise\");")
                })
                .when(tesla, |code| dispatch(code, "Raise"))
                .otherwise(|code| code.always("Goto Raise;"))
        })
        .each(&counts, |code, lit| {
            code.always(format!("{}:", label("Raise", *lit)))
                .indented(|code| {
                    code.when_some(built.sound_line("raise", "CHAN_WEAPON"), |c, l| c.always(l))
                        .append(frames(
                            Code::new(),
                            built.get(&variant("raise", *lit)),
                            Tic::Plain,
                            &built.events("raise", built.get(&variant("raise", *lit)).len()),
                        ))
                        .always("TNT1 A 0 A_Raise(256);")
                        .always("Wait;")
                })
        })
        .when(built.has(&variant("first_raise", full)), |code| {
            let (shown, events) =
                built.switched_frames("first_raise", &variant("first_raise", full));
            code.always("FirstRaise:").indented(|code| {
                code.when_some(built.sound_line("first_raise", "CHAN_WEAPON"), |c, l| {
                    c.always(l)
                })
                .append(frames(Code::new(), &shown, Tic::Plain, &events))
                .always("TNT1 A 0 A_Raise(256);")
                .always("Wait;")
            })
        })
        .when(built.has("empty_raise"), |code| {
            code.always("EmptyRaise:").indented(|code| {
                code.when_some(built.sound_line("raise", "CHAN_WEAPON"), |c, l| c.always(l))
                    .append(frames(
                        Code::new(),
                        built.get("empty_raise"),
                        Tic::Plain,
                        &built.events("empty_raise", built.get("empty_raise").len()),
                    ))
                    .always("TNT1 A 0 A_Raise(256);")
                    .always("Wait;")
            })
        })
        // Ready: aimed, empty, or (the Wunderwaffe) by its rounds
        .always("Ready:")
        .indented(|code| {
            code.when(aims, |code| {
                code.always("TNT1 A 0 WhenZoomed(\"ZoomReady\");")
            })
            .when(built.has("empty_idle"), |code| {
                code.always("TNT1 A 0 WhenNoActiveAmmo(\"EmptyIdle\");")
            })
            .when(thrown, |code| {
                code.always("TNT1 A 0 WhenNoAmmo(\"EmptyIdle\");")
            })
            .when(tesla, |code| dispatch(code, "Idle"))
        })
        .each(&counts, |code, lit| {
            code.always(format!("{}:", label("Idle", *lit)))
                .indented(|code| {
                    code.always(format!(
                        "{} 1 ReadyWeapon;",
                        built.still(&variant("idle", *lit))
                    ))
                    .always("Loop;")
                })
        })
        .when(built.has("empty_idle") || thrown, |code| {
            code.always("EmptyIdle:").indented(|code| {
                code.when(thrown, |code| code.always("TNT1 A 1 ReadyWeapon;"))
                    .otherwise(|code| {
                        code.always(format!("{} 1 ReadyWeapon;", built.still("empty_idle")))
                    })
                    .always("Loop;")
            })
        })
        // Lowering: the animation, then away
        .always("Deselect:")
        .indented(|code| {
            code.when(aims, |code| code.always("TNT1 A 0 ZoomOut;"))
                .always("TNT1 A 0 AtRest;")
                .when(built.has("empty_drop"), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"EmptyLower\");")
                })
                .when(tesla, |code| dispatch(code, "Lower"))
                .otherwise(|code| code.always("Goto Lower;"))
        })
        .when(built.has("empty_drop"), |code| {
            code.always("EmptyLower:").indented(|code| {
                code.when_some(built.sound_line("drop", "CHAN_WEAPON"), |c, l| c.always(l))
                    .append(frames(
                        Code::new(),
                        built.get("empty_drop"),
                        Tic::Plain,
                        &BTreeMap::new(),
                    ))
                    .always("TNT1 A 0 A_Lower(256);")
                    .always("Wait;")
            })
        })
        .each(&counts, |code, lit| {
            code.always(format!("{}:", label("Lower", *lit)))
                .indented(|code| {
                    code.when_some(built.sound_line("drop", "CHAN_WEAPON"), |c, l| c.always(l))
                        .append(frames(
                            Code::new(),
                            built.get(&variant("drop", *lit)),
                            Tic::Plain,
                            &BTreeMap::new(),
                        ))
                        .always("TNT1 A 0 A_Lower(256);")
                        .always("Wait;")
                })
        })
        .when(gun, |code| gun_states(code, project, built, aims, &counts))
        .when(thrown, |code| thrown_states(code, project, built))
}

fn gun_states(
    code: Code,
    project: &Project,
    built: &Built,
    aims: bool,
    counts: &[Option<u32>],
) -> Code {
    let weapon = built.weapon;
    let tesla = weapon.kind == Kind::Tesla;
    let flame = weapon.kind == Kind::Flame;
    // The Wunderwaffe's shot is named by the bulbs lit after it: Fire2 from three rounds of three.
    let shots = plan::shot_counts(weapon);
    let fire_label = |aimed: bool, lit: Option<u32>| {
        let base = if aimed { "ZoomFire" } else { "Fire" };
        match lit {
            Some(n) => format!("{base}{n}"),
            None => base.to_owned(),
        }
    };
    let has_last = built.has("last_fire");
    let has_ads_last = built.has("ads_last_fire");
    code.always("Fire:")
        .indented(|code| {
            code.when(flame, |code| {
                code.always("TNT1 A 0 WhenOverheated(\"Dry\");")
            })
            .otherwise(|code| code.always("TNT1 A 0 WhenNoActiveAmmo(\"Dry\");"))
            .when(aims, |code| {
                code.always("TNT1 A 0 WhenZoomed(\"ZoomFireNext\");")
            })
            .when(has_last, |code| {
                code.always("TNT1 A 0 WhenLastActiveAmmo(\"LastFire\");")
            })
            .when(tesla, |code| {
                code.each(1..=weapon.ammo.magazine, |code, rounds| {
                    code.always(format!(
                        "TNT1 A 0 WhenActiveAmmo({rounds}, \"{}\");",
                        fire_label(false, Some(plan::lit(weapon, rounds - 1)))
                    ))
                })
            })
            .when(!tesla, |code| code.always("Goto FireShot;"))
        })
        .when(flame, |code| {
            // The flamethrower burns for as long as the trigger is held
            // (it roars and cools down by itself: DoEffect)
            code.always("FireShot:").indented(|code| {
                code.each(
                    built.get("fire").iter().take(built.cycle().max(4)),
                    |code, frame| {
                        let (sprite, letter) = frame.state();
                        code.always(format!(
                            "{sprite} {letter} 1 ShootFlame(\"{}Shot\");",
                            project.class(built.id)
                        ))
                    },
                )
                .always("TNT1 A 0 A_ReFire;")
                .always("Goto Ready;")
            })
        })
        .otherwise_when(tesla, |code| {
            code.each(&shots, |code, lit| {
                fire_state(
                    code,
                    project,
                    built,
                    &fire_label(false, *lit),
                    &variant("fire", *lit),
                    false,
                    false,
                )
            })
        })
        .otherwise(|code| fire_state(code, project, built, "FireShot", "fire", false, false))
        .when(has_last && !tesla, |code| {
            fire_state(code, project, built, "LastFire", "last_fire", false, true)
        })
        .when(built.has("rechamber"), |code| {
            code.always("Rechamber:").indented(|code| {
                code.always("TNT1 A 0 ResetClock;")
                    .when_some(built.sound_line("rechamber", "CHAN_WEAPON"), |c, l| {
                        c.always(l)
                    })
                    .append(frames(
                        Code::new(),
                        built.get("rechamber"),
                        Tic::Fire,
                        &built.events("rechamber", built.get("rechamber").len()),
                    ))
                    .always("Goto Ready;")
            })
        })
        // Empty: only a click, no animation; reloading is the reload key's
        .always("Dry:")
        .indented(|code| {
            code.when_some(
                weapon
                    .sounds
                    .get("dry")
                    .filter(|a| built.pulled.sounds.contains_key(*a)),
                |c, alias| c.always(format!("TNT1 A 0 DryClick(\"{}\");", sound(alias))),
            )
            .always("Goto Ready;")
        })
        .when(flame, |code| {
            code.always("Reload:")
                .indented(|code| code.always("Goto Ready;"))
        })
        .otherwise(|code| code.append(reload_states(built)))
        .when(aims, |code| {
            aim_states(
                code,
                project,
                built,
                counts,
                &shots,
                has_ads_last,
                &fire_label,
            )
        })
}

fn reload_states(built: &Built) -> Code {
    let weapon = built.weapon;
    let full = plan::full(weapon);
    let reload = variant("reload", full);
    let reload_empty = variant("reload_empty", full);
    Code::new()
        .always("Reload:")
        .indented(|code| {
            code.always("TNT1 A 0 WhenFullAmmo(\"Ready\");")
                .always("TNT1 A 0 WhenNoAmmo(\"Ready\");")
                .always("TNT1 A 0 ZoomOut;")
                .always("TNT1 A 0 ResetClock;")
                .when(weapon.ammo.segmented, |code| {
                    code.always("Goto ReloadStart;")
                })
                .otherwise_when(built.has(&reload_empty), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"ReloadEmpty\");")
                })
        })
        .when(weapon.ammo.segmented, |code| {
            // One round at a time: start, a round per loop until full (or the trigger is
            // pulled), finish
            code.always("ReloadStart:")
                .indented(|code| {
                    code.append(frames(
                        Code::new(),
                        built.get("reload_start"),
                        Tic::Reload,
                        &built.events("reload_start", built.get("reload_start").len()),
                    ))
                })
                .always("ReloadRound:")
                .indented(|code| {
                    code.append(frames(
                        Code::new(),
                        built.get(&reload),
                        Tic::Reload,
                        &built.events("reload", built.get(&reload).len()),
                    ))
                    .always("TNT1 A 0 ReloadOne;")
                    .always("TNT1 A 0 WhenLoaded(\"ReloadEnd\");")
                    .always("TNT1 A 0 WhenFireHeld(\"ReloadEnd\");")
                    .always("Loop;")
                })
                .always("ReloadEnd:")
                .indented(|code| {
                    code.append(frames(
                        Code::new(),
                        built.get("reload_end"),
                        Tic::Reload,
                        &built.events("reload_end", built.get("reload_end").len()),
                    ))
                    .always("Goto Ready;")
                })
        })
        .otherwise_when(!built.has(&reload), |code| {
            // No reload animation (the flamethrower): the weapon held still while it refills
            code.indented(|code| {
                code.always(format!("{} 52 ReloadTic;", built.still("idle")))
                    .always("TNT1 A 0 Reload;")
                    .always("Goto Ready;")
            })
        })
        .otherwise(|code| {
            let (shown, events) = built.switched_frames("reload", &reload);
            code.indented(|code| {
                code.append(frames(Code::new(), &shown, Tic::Reload, &events))
                    .always("TNT1 A 0 Reload;")
                    .always("Goto Ready;")
            })
            .when(built.has(&reload_empty), |code| {
                let (shown, events) = built.switched_frames("reload_empty", &reload_empty);
                code.always("ReloadEmpty:").indented(|code| {
                    code.append(frames(Code::new(), &shown, Tic::Reload, &events))
                        .always("TNT1 A 0 Reload;")
                        .always("Goto Ready;")
                })
            })
        })
}

#[allow(clippy::too_many_arguments)]
fn aim_states(
    code: Code,
    project: &Project,
    built: &Built,
    counts: &[Option<u32>],
    shots: &[Option<u32>],
    has_ads_last: bool,
    fire_label: &dyn Fn(bool, Option<u32>) -> String,
) -> Code {
    let weapon = built.weapon;
    let tesla = weapon.kind == Kind::Tesla;
    let label = |base: &str, lit: Option<u32>| match lit {
        Some(n) => format!("{base}{n}"),
        None => base.to_owned(),
    };
    let dispatch = |code: Code, base: &str| {
        code.each(0..=weapon.ammo.magazine, |code, rounds| {
            code.when(tesla, |code| {
                code.always(format!(
                    "TNT1 A 0 WhenActiveAmmo({rounds}, \"{}\");",
                    label(base, Some(plan::lit(weapon, rounds)))
                ))
            })
        })
    };
    code.always("Zoom:")
        .indented(|code| code.always("TNT1 A 0 ToggleZoom;"))
        .always("ZoomIn:")
        .indented(|code| {
            code.always("TNT1 A 0 AimIn;")
                .when(built.has("empty_zoom_in"), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"EmptyZoomInFrom\");")
                })
                .when(tesla, |code| dispatch(code, "ZoomInFrom"))
                .otherwise(|code| code.always("Goto ZoomInFrom;"))
        })
        .when(built.has("empty_zoom_in"), |code| {
            code.always("EmptyZoomInFrom:").indented(|code| {
                code.append(frames(
                    Code::new(),
                    built.get("empty_zoom_in"),
                    Tic::Plain,
                    &BTreeMap::new(),
                ))
                .always("Goto ZoomReady;")
            })
        })
        .each(counts, |code, lit| {
            code.always(format!("{}:", label("ZoomInFrom", *lit)))
                .indented(|code| {
                    code.append(frames(
                        Code::new(),
                        built.get(&variant("zoom_in", *lit)),
                        Tic::Plain,
                        &BTreeMap::new(),
                    ))
                    .always("Goto ZoomReady;")
                })
        })
        .always("ZoomReady:")
        .indented(|code| {
            code.always("TNT1 A 0 WhenShouldZoomOut(\"ZoomOut\");")
                .when(built.has("ads_empty_idle"), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"ZoomEmptyIdle\");")
                })
                .when(tesla, |code| dispatch(code, "ZoomIdle"))
                .otherwise(|code| code.always("Goto ZoomIdle;"))
        })
        .each(counts, |code, lit| {
            code.always(format!("{}:", label("ZoomIdle", *lit)))
                .indented(|code| {
                    code.always(format!(
                        "{} 1 ReadyWeapon;",
                        built.still(&variant("ads_idle", *lit))
                    ))
                    .always("Goto ZoomReady;")
                })
        })
        .when(built.has("ads_empty_idle"), |code| {
            code.always("ZoomEmptyIdle:").indented(|code| {
                code.always(format!("{} 1 ReadyWeapon;", built.still("ads_empty_idle")))
                    .always("Goto ZoomReady;")
            })
        })
        .always("ZoomOut:")
        .indented(|code| {
            code.always("TNT1 A 0 ZoomOut;")
                .when(built.has("empty_zoom_out"), |code| {
                    code.always("TNT1 A 0 WhenNoActiveAmmo(\"EmptyZoomOutFrom\");")
                })
                .when(tesla, |code| dispatch(code, "ZoomOutFrom"))
                .otherwise(|code| code.always("Goto ZoomOutFrom;"))
        })
        .when(built.has("empty_zoom_out"), |code| {
            code.always("EmptyZoomOutFrom:").indented(|code| {
                code.append(frames(
                    Code::new(),
                    built.get("empty_zoom_out"),
                    Tic::Plain,
                    &BTreeMap::new(),
                ))
                .always("Goto Ready;")
            })
        })
        .each(counts, |code, lit| {
            code.always(format!("{}:", label("ZoomOutFrom", *lit)))
                .indented(|code| {
                    code.append(frames(
                        Code::new(),
                        built.get(&variant("zoom_out", *lit)),
                        Tic::Plain,
                        &BTreeMap::new(),
                    ))
                    .always("Goto Ready;")
                })
        })
        .always("ZoomFireNext:")
        .indented(|code| {
            code.when(has_ads_last, |code| {
                code.always("TNT1 A 0 WhenLastActiveAmmo(\"ZoomLastFire\");")
            })
            .when(tesla, |code| {
                code.each(1..=weapon.ammo.magazine, |code, rounds| {
                    code.always(format!(
                        "TNT1 A 0 WhenActiveAmmo({rounds}, \"{}\");",
                        fire_label(true, Some(plan::lit(weapon, rounds - 1)))
                    ))
                })
            })
            .otherwise(|code| code.always("Goto ZoomFire;"))
        })
        .when(tesla, |code| {
            code.each(shots, |code, lit| {
                fire_state(
                    code,
                    project,
                    built,
                    &fire_label(true, *lit),
                    &variant("ads_fire", *lit),
                    true,
                    false,
                )
            })
        })
        .otherwise(|code| fire_state(code, project, built, "ZoomFire", "ads_fire", true, false))
        .when(has_ads_last && !tesla, |code| {
            fire_state(
                code,
                project,
                built,
                "ZoomLastFire",
                "ads_last_fire",
                true,
                true,
            )
        })
        .when(built.has("ads_rechamber"), |code| {
            code.always("ZoomRechamber:").indented(|code| {
                code.always("TNT1 A 0 ResetClock;")
                    .when_some(built.sound_line("rechamber", "CHAN_WEAPON"), |c, l| {
                        c.always(l)
                    })
                    .append(frames(
                        Code::new(),
                        built.get("ads_rechamber"),
                        Tic::Fire,
                        &BTreeMap::new(),
                    ))
                    .always("Goto ZoomReady;")
            })
        })
}

/// Grenades, the monkey and the Betty: the throw (or plant) is the fire animation, letting go
/// of it two thirds of the way in.
fn thrown_states(code: Code, project: &Project, built: &Built) -> Code {
    let (all, events) = built.trimmed("fire", "fire");
    let release = all.len() * 2 / 3;
    let (before, after) = all.split_at(release);
    let pitch = if built.weapon.kind == Kind::Mine {
        30.0
    } else {
        -8.0
    };
    code.always("Fire:").indented(|code| {
        code.always("TNT1 A 0 WhenNoAmmo(\"EmptyIdle\");")
            .append(frames(Code::new(), before, Tic::Plain, &events))
            .when_some(built.sound_line("throw", "CHAN_WEAPON"), |c, l| c.always(l))
            .always(format!(
                "TNT1 A 0 ThrowOne(\"{}Shot\", {pitch:.1});",
                project.class(built.id)
            ))
            .append(frames(Code::new(), after, Tic::Recover, &BTreeMap::new()))
            .always("Goto Ready;")
    })
}

/// The knife replaces the kick: owning it swaps Horde Hammer's kick for a stab drawn with the
/// knife's slash, and the weapon itself only plays the flourish when it is bought.
fn melee_file(project: &Project, built: &Built, header: String) -> String {
    let weapon = built.weapon;
    let p = &project.config.prefix;
    let class = project.class(built.id);
    let slash = visible(built.get("fire"));
    let (before, after) = slash.split_at(slash.len() / 2);
    let damage = project.damage(weapon.fire.damage);
    let swing = built.sound_line("melee", "CHAN_WEAPON");
    let hit = weapon
        .sounds
        .get("hit")
        .filter(|a| built.pulled.sounds.contains_key(*a))
        .map(|a| sound(a))
        .unwrap_or_default();
    let idle = built.still("idle");
    let flourish = Code::new()
        .always("Select:")
        .indented(|code| {
            code.always("TNT1 A 0 AtRest;")
                .when(built.has("first_raise"), |code| {
                    code.always("TNT1 A 0 WhenFirstRaise(\"Flourish\");")
                })
                .when_some(built.sound_line("raise", "CHAN_WEAPON"), |c, l| c.always(l))
                .append(frames(
                    Code::new(),
                    built.get("raise"),
                    Tic::Plain,
                    &BTreeMap::new(),
                ))
                .always("TNT1 A 0 A_Raise(256);")
                .always("Wait;")
        })
        .when(built.has("first_raise"), |code| {
            // Bought: the flourish, then the gun the player had comes back
            code.always("Flourish:").indented(|code| {
                let (shown, events) = built.trimmed("first_raise", "first_raise");
                code.append(frames(Code::new(), shown, Tic::Plain, &events))
                    .always("TNT1 A 0 GiveBack;")
                    .always("TNT1 A 0 A_Raise(256);")
                    .always("Wait;")
            })
        })
        .always("Ready:")
        .indented(|code| {
            code.always(format!("{idle} 1 A_WeaponReady;"))
                .always("Loop;")
        })
        .always("Deselect:")
        .indented(|code| {
            code.append(frames(
                Code::new(),
                built.get("drop"),
                Tic::Plain,
                &BTreeMap::new(),
            ))
            .always("TNT1 A 0 A_Lower(256);")
            .always("Wait;")
        })
        .always("Fire:")
        .indented(|code| {
            code.when_some(built.sound_line("melee", "CHAN_WEAPON"), |c, l| c.always(l))
                .append(frames(Code::new(), before, Tic::Plain, &BTreeMap::new()))
                .always(format!("TNT1 A 0 Stab({damage}, 80);"))
                .append(frames(Code::new(), after, Tic::Plain, &BTreeMap::new()))
                .always("Goto Ready;")
        });
    let stab = Code::new().always("Kick:").indented(|code| {
        code.always("TNT1 A 0 HideWeapon(true);")
            .when_some(swing, |c, l| c.always(l))
            .append(frames(Code::new(), before, Tic::Plain, &BTreeMap::new()))
            .always(format!("TNT1 A 0 Stab({damage});"))
            .append(frames(Code::new(), after, Tic::Plain, &BTreeMap::new()))
            .always("TNT1 A 0 HideWeapon(false);")
            .always("Stop;")
    });
    Code::new()
        .always(header)
        .gap()
        .always(formatdoc! {r#"
            // Bought off the wall, it plays its flourish and gives the player's gun back; from then
            // on the kick key stabs with it ({class}Kick), and the equipment key brings it up.
            class {class} : {p}_DoomifyWeapon {{
                Weapon previous;

                Default {{
                    Tag "{name}";
                    HH_Weapon.Counted false;
                    HH_Weapon.Equipment true;
                    HH_Weapon.BoxWeight 0;
                    +WEAPON.CHEATNOTWEAPON
                    +INVENTORY.UNDROPPABLE
                }}

                override void AttachToOwner(Actor other) {{
                    if (other.player != null) {{
                        self.previous = other.player.ReadyWeapon;
                    }}
                    super.AttachToOwner(other);
                }}

                // Whoever owns the knife kicks with it, a respawn's fresh kick included
                override void DoEffect() {{
                    super.DoEffect();
                    if (self.owner == null || self.owner.FindInventory('{class}Kick') != null) {{
                        return;
                    }}
                    let kick = self.owner.FindInventory('HH_KickItem');
                    if (kick != null) {{
                        kick.Destroy();
                    }}
                    self.owner.GiveInventory('{class}Kick', 1);
                }}

                // Back to the gun the player had
                action void GiveBack() {{
                    if (self.player != null && invoker.previous != null && invoker.previous.owner == self) {{
                        self.player.PendingWeapon = invoker.previous;
                    }} else {{
                        A_SelectWeapon(null, SWF_SELECTPRIORITY);
                    }}
                }}

                States {{
        "#, name = weapon.name})
        .indented(|code| code.append(flourish))
        .always(formatdoc! {r#"
                }}
            }}

            // The kick with the knife: its slash over the gun, which hides meanwhile
            class {class}Kick : HH_KickItem {{
                action void HideWeapon(bool hide) {{
                    A_OverlayFlags(PSP_WEAPON, PSPF_ALPHA | PSPF_FORCEALPHA, hide);
                    A_OverlayAlpha(PSP_WEAPON, hide? 0.0: 1.0);
                }}

                action void Stab(int damage) {{
                    FTranslatedLineTarget victim;
                    self.LineAttack(self.angle, 80, self.pitch, damage, 'Melee', '{p}_DoomifyStab', LAF_ISMELEEATTACK, victim);
                    if (victim.linetarget != null) {{
                        self.A_StartSound("{hit}", CHAN_WEAPON);
                    }}
                }}

                States {{
                Use:
                    TNT1 A 0 A_Overlay(77, "Kick");
                    // drawn where the renders were made, not moved with the hidden gun
                    TNT1 A 0 A_OverlayFlags(77, PSPF_ADDWEAPON | PSPF_ADDBOB, false);
                    TNT1 A 0 A_OverlayOffset(77, 0, WEAPONTOP);
                    Fail;
        "#})
        .indented(|code| code.append(stab))
        .always("    }\n}")
        .gap()
        .append(wall_drawing(project, built))
        .build()
}

/// A perk's bottle, drunk as World at War drinks it (Horde Hammer's perk and drink underneath;
/// the map's own machines sell it by this class name). Quick Revive's has a solo drink too.
fn drink_file(project: &Project, built: &Built, header: String) -> String {
    let weapon = built.weapon;
    let p = &project.config.prefix;
    let perk = weapon.perk.as_deref().unwrap_or_default();
    let class = project.class(built.id);
    let drink = built.get("raise");
    // The perk takes hold at the swallow, or three fifths of the way in
    let swallow = built
        .events("raise", drink.len())
        .into_iter()
        .find(|(_, lines)| {
            lines
                .iter()
                .any(|l| l.contains("swallow") || l.contains("drink"))
        })
        .map(|(frame, _)| frame)
        .unwrap_or(drink.len() * 3 / 5);
    let mut events = built.events("raise", drink.len());
    events
        .entry(swallow)
        .or_default()
        .push("TNT1 A 0 Consume;".into());
    // World at War takes the bottle away once it is drunk: its lower clip would show it full
    // again
    let ready = Code::new().always("Ready:").indented(|code| {
        code.when_some(built.sound_line("raise", "CHAN_WEAPON"), |c, l| c.always(l))
            .append(frames(Code::new(), drink, Tic::Sip, &events))
            .always("TNT1 A 0 Consume;")
            .always("TNT1 A 0 A_StartSound(\"hh/bottle_break\", CHAN_AUTO);")
            .always("Goto Deselect;")
    });
    let pickup = built.pickup.clone().unwrap_or(Frame::Empty).state();
    let states = |code: Code| {
        code.always("States {")
            .always("Spawn:")
            .indented(|c| {
                c.always(format!("{} {} -1;", pickup.0, pickup.1))
                    .always("Stop;")
            })
            .append(ready.clone())
            .always("}")
    };
    let sip = formatdoc! {r#"
        int sips;

        // A frame of drinking, a tic and a half on average so that it is savoured; it can still
        // be thrown. Speed Cola drinks it in a tic.
        action void Sip() {{
            A_WeaponReady(WRF_NOSWITCH | WRF_NOBOB);
            let quick = invoker.reloadFrameRate < invoker.Default.reloadFrameRate;
            A_SetTics(quick? 1: 1 + (invoker.sips++ & 1));
        }}
    "#};
    let solo = perk == "QuickRevive";
    Code::new()
        .always(header)
        .gap()
        .block(format!("class {class}Drink : HH_{perk}Drink"), |code| {
            code.always(sip.clone()).gap().append(states(Code::new()))
        })
        .when(solo, |code| {
            code.gap().block(
                format!("class {p}_SoloReviveDrink : HH_SoloReviveDrink"),
                |code| code.always(sip.clone()).gap().append(states(Code::new())),
            )
        })
        .build()
}

/// The chalk outline to buy it off the wall, if it has one.
fn wall_drawing(project: &Project, built: &Built) -> Code {
    let weapon = built.weapon;
    let class = project.class(built.id);
    Code::new().when_some(
        built.chalk.as_ref().zip(weapon.wall.as_ref()),
        |code, (chalk, wall)| {
            let (sprite, letter) = chalk.state();
            code.always(formatdoc! {r#"
            // The chalk outline to buy it off the wall
            class {class}Drawing : HH_WallBuy {{
                Default {{
                    //$Category "World at War"
                    //$Title "{name} Wall Buy"
                    HH_WallBuy.Weapon "{class}";
                    HH_WallBuy.Cost {cost};
                    Scale 0.5;
                }}

                States {{
                Spawn:
                    {sprite} {letter} -1 Bright;
                    Stop;
                }}
            }}
        "#, name = weapon.name, cost = wall.cost})
        },
    )
}

/// What the weapon fires, when it fires a thing.
/// What a projectile's `[projectile]` look adds to its class: its defaults, and its states (it
/// flies as its frames, bursts as its burst's); empty for one drawn only by its light and particles.
fn look(project: &Project, weapon: &Weapon) -> (String, String) {
    let Some(look) = &weapon.projectile else {
        return (String::new(), String::new());
    };
    let p = &project.config.prefix;
    let defaults = Code::new()
        .always(format!("Scale {:.3};", look.scale))
        .when(look.pulse, |code| {
            code.always(format!("{p}_DoomifyShot.Pulse true;"))
        })
        .when_some(look.translation.as_ref(), |code, translation| {
            code.always(format!("Translation \"{translation}\";"))
        })
        .build();
    let states = Code::new()
        .always("States {")
        .always("Spawn:")
        .indented(|code| {
            code.always(format!("{} {} 2;", look.sprite, look.frames))
                .always("Loop;")
        })
        .when_some(look.burst.as_ref(), |code, burst| {
            code.always("Death:").indented(|code| {
                code.always("TNT1 A 0 Impact;")
                    .always(format!("TNT1 A 0 A_SetScale({:.3});", burst.scale))
                    .always(format!("{} {} 2 Bright;", burst.sprite, burst.frames))
                    .always("Stop;")
            })
        })
        .always("}")
        .build();
    // each line after the one before it, at the class's indentation
    let lines = |text: String, indent: &str| -> String {
        text.lines()
            .map(|line| format!("\n{indent}{line}"))
            .collect()
    };
    (
        lines(defaults, "        "),
        format!("\n{}", lines(states, "    ")),
    )
}

fn shot_class(project: &Project, built: &Built) -> Code {
    let weapon = built.weapon;
    let class = project.class(built.id);
    let p = &project.config.prefix;
    let damage = project.damage(weapon.fire.damage);
    let color = weapon.fire.color.clone().unwrap_or_else(|| "6cff3a".into());
    let impact = weapon
        .sounds
        .get("impact")
        .filter(|a| built.pulled.sounds.contains_key(*a))
        .map(|a| sound(a))
        .unwrap_or_default();
    let speed = if weapon.fire.speed > 0.0 {
        weapon.fire.speed
    } else {
        40.0
    };
    let pickup = built.pickup.clone().unwrap_or(Frame::Empty).state();
    let bounce = weapon
        .sounds
        .get("bounce")
        .filter(|a| built.pulled.sounds.contains_key(*a))
        .map(|a| sound(a))
        .unwrap_or_default();
    let (look_defaults, look_states) = look(project, weapon);
    Code::new()
        .when(matches!(weapon.kind, Kind::Projectile), |code| {
            code.always(formatdoc! {r#"
                class {class}Shot : {p}_DoomifyShot {{
                    Default {{
                        Speed {speed:.0};
                        DamageFunction ({damage});
                        {p}_DoomifyShot.TrailColor "{color}";
                        {p}_DoomifyShot.Splash {splash};
                        {p}_DoomifyShot.SplashDamage {damage};
                        {p}_DoomifyShot.ImpactSound "{impact}";{look_defaults}
                    }}{look_states}
                }}
            "#, splash = weapon.fire.splash})
        })
        .otherwise_when(weapon.kind == Kind::Tesla, |code| {
            code.always(formatdoc! {r#"
                class {class}Shot : {p}_TeslaBolt {{
                    Default {{
                        Speed {speed:.0};
                        DamageFunction (0);
                        {p}_DoomifyShot.TrailColor "{color}";
                        {p}_DoomifyShot.ImpactSound "{impact}";
                        {p}_TeslaBolt.Arcs {arcs};
                        {p}_TeslaBolt.ArcRange {range};{look_defaults}
                    }}{look_states}
                }}
            "#, arcs = weapon.fire.arcs, range = weapon.fire.arc_range})
        })
        .otherwise_when(weapon.kind == Kind::Flame, |code| {
            code.always(formatdoc! {r#"
                class {class}Shot : {p}_DoomifyFlame {{
                    Default {{
                        DamageFunction ({damage});
                    }}
                }}
            "#})
        })
        .otherwise_when(
            matches!(weapon.kind, Kind::Grenade | Kind::Monkey),
            |code| {
                let parent = if weapon.kind == Kind::Monkey {
                    "DoomifyMonkey"
                } else {
                    "DoomifyThrown"
                };
                code.always(formatdoc! {r#"
                class {class}Shot : {p}_{parent} {{
                    Default {{
                        {p}_DoomifyThrown.BlastDamage {damage};
                        {p}_DoomifyThrown.BlastRadius {radius};
                        {p}_DoomifyThrown.BounceSound "{bounce}";
                        {p}_DoomifyThrown.BlastSound "waw/grenade_explode";
                    }}

                    States {{
                    Spawn:
                        {sprite} {letter} -1;
                        Stop;
                    }}
                }}
            "#, radius = weapon.fire.splash.max(64), sprite = pickup.0, letter = pickup.1})
            },
        )
        .otherwise_when(weapon.kind == Kind::Mine, |code| {
            code.always(formatdoc! {r#"
                class {class}Shot : {p}_DoomifyMine {{
                    Default {{
                        {p}_DoomifyMine.BlastDamage {damage};
                        {p}_DoomifyMine.BlastRadius {radius};
                    }}

                    States {{
                    Spawn:
                        {sprite} {letter} -1;
                        Stop;
                    }}
                }}
            "#, radius = weapon.fire.splash.max(64), sprite = pickup.0, letter = pickup.1})
        })
}

/// The whole file for one weapon.
pub fn file(project: &Project, built: &Built) -> String {
    let weapon = built.weapon;
    let p = &project.config.prefix;
    let class = project.class(built.id);
    let thrown = matches!(weapon.kind, Kind::Grenade | Kind::Monkey | Kind::Mine);
    let melee = weapon.kind == Kind::Melee;
    let ammo = !melee;
    let aim_zoom = weapon
        .view
        .ads_fov
        .map(|fov| (32.5f64.to_radians().tan() / (fov / 2.0).to_radians().tan()).max(1.0))
        .unwrap_or(1.2);
    let header = formatdoc! {r#"
        // {name}, from Der Riese's {source}. Generated by doomify-waw from weapons/{id}.toml: edit
        // that and run doomify again, not this file.
    "#, name = weapon.name, source = weapon.game.weapon, id = built.id};
    if melee {
        return melee_file(project, built, header);
    }
    if weapon.kind == Kind::Drink {
        return drink_file(project, built, header);
    }
    let defaults = Code::new()
        .always(format!("Tag \"{}\";", weapon.name))
        .always(format!("Weapon.SlotNumber {};", weapon.slot))
        .when(ammo, |code| {
            code.always(format!("Weapon.AmmoType \"{class}Ammo\";"))
                .always(format!("Weapon.AmmoGive {};", weapon.ammo.give))
        })
        .when(!thrown && !melee, |code| {
            code.always(format!("HH_Weapon.ActiveAmmo {};", weapon.ammo.magazine))
                .always(format!(
                    "HH_Weapon.KeepPartialReload {};",
                    weapon.ammo.partial
                ))
        })
        .when(thrown || melee, |code| {
            code.always("HH_Weapon.Counted false;")
                .always("HH_Weapon.Equipment true;")
        })
        .always(format!(
            "HH_Weapon.BoxWeight {};",
            if weapon.upgraded {
                0
            } else {
                weapon.box_weight
            }
        ))
        .when(weapon.last_stand > 0, |code| {
            code.always(format!("HH_Weapon.LastStand {};", weapon.last_stand))
        })
        .when_some(
            weapon.downed.as_ref().filter(|_| built.downed_built),
            |code, downed| code.always(format!("HH_Weapon.Downed \"{}\";", project.class(downed))),
        )
        .when_some(
            weapon.upgrade.as_ref().filter(|_| built.upgrade_built),
            |code, upgrade| {
                code.always(format!("HH_Weapon.Upgrade \"{}\";", project.class(upgrade)))
            },
        )
        .always(format!("{p}_DoomifyWeapon.AimZoom {aim_zoom:.2};"))
        .when(weapon.kind == Kind::Flame, |code| {
            let named = |role: &str| {
                weapon
                    .sounds
                    .get(role)
                    .filter(|a| built.pulled.sounds.contains_key(*a))
                    .map(|a| sound(a))
                    .unwrap_or_default()
            };
            code.always(format!("{p}_DoomifyWeapon.Regenerates true;"))
                .always(format!(
                    "{p}_DoomifyWeapon.BurnSound \"{}\";",
                    named("loop")
                ))
                .always(format!(
                    "{p}_DoomifyWeapon.IgniteSound \"{}\";",
                    named("fire")
                ))
                .always(format!(
                    "{p}_DoomifyWeapon.CooldownSound \"{}\";",
                    named("cooldown")
                ))
        })
        .when(weapon.fire.auto, |code| code.always("-WEAPON.NOAUTOFIRE"))
        .when(thrown, |code| code.always("+WEAPON.NOAUTOAIM"));

    Code::new()
        .always(header)
        .gap()
        .when(ammo, |code| {
            code.block(format!("class {class}Ammo : Ammo"), |code| {
                code.block("Default", |code| {
                    let most = if thrown {
                        weapon.ammo.magazine
                    } else {
                        weapon.ammo.reserve
                    };
                    code.always(format!("Inventory.MaxAmount {most};"))
                        .always(format!("Ammo.BackpackMaxAmount {most};"))
                        .always("+INVENTORY.IGNORESKILL")
                })
            })
            .gap()
        })
        .block(format!("class {class} : {p}_DoomifyWeapon"), |code| {
            code.block("Default", |code| code.append(defaults))
                .gap()
                .always("States {")
                .append(states(project, built))
                .always("}")
        })
        .gap()
        .append(shot_class(project, built))
        .gap()
        .append(wall_drawing(project, built))
        .build()
}
