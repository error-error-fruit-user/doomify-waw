//! The ZScript every map gets once: the weapons' base class and the actors they fire. The
//! prefix is the map's (`Nacht_DoomifyWeapon`).

use indoc::formatdoc;

/// The base of every generated weapon.
pub fn weapon(p: &str) -> String {
    formatdoc! {r#####"
        // The base of the weapons doomify rebuilt from World at War. Every frame was rendered
        // at 35 a second, Doom's tic rate, so it shows for one tic; Speed Cola and Double Tap
        // speed an animation up by skipping frames instead of shortening them.

        class {p}_DoomifyWeapon : HH_Weapon {{
            int frameClock;         // how far Pace is through skipping frames
            bool raisedBefore;      // the first time it is raised it plays its first raise
            meta double aimZoom;    // the zoom when aimed (World at War's adsZoomFov)
            // The flamethrower doesn't reload: its tank refills while it isn't burning, and
            // emptied it overheats until full again
            meta bool regenerates;
            meta Sound burnSound;   // looped while it burns
            meta Sound igniteSound;
            meta Sound cooldownSound;
            int lastBurned;
            bool overheated;
            int joltTics;           // the Wunderwaffe jolting to life: the hands shake, the screen shocks
            int nextClick;          // when an empty weapon may click again, the trigger held

            property AimZoom: aimZoom;
            property Regenerates: regenerates;
            property BurnSound: burnSound;
            property IgniteSound: igniteSound;
            property CooldownSound: cooldownSound;

            const JOLT_TICS = 24;
            const JOLT_RANGE = 160;     // how far the reload's jolt reaches
            const REFILL_DELAY = 12;    // tics after the last flame before the tank refills
            const BURN_CHANNEL = CHAN_7;

            Default {{
                {p}_DoomifyWeapon.AimZoom 1.2;
                // Speed Cola takes one off each (HH_Weapon): every other reload frame, and a
                // quarter of the fire frames, are skipped
                HH_Weapon.ReloadFrameRate 2;
                HH_Weapon.FireFrameRate 4;
                Weapon.BobRangeX 0.7;
                Weapon.BobRangeY 0.5;
            }}

            // Whether the next frame shows (1 tic) or is skipped (0), so that `rate` frames show
            // for every `normal` ones
            int PaceTics(int rate, int normal) {{
                self.frameClock += rate;
                if (self.frameClock >= normal) {{
                    self.frameClock -= normal;
                    return 1;
                }}
                return 0;
            }}

            action void ResetClock() {{
                invoker.frameClock = 0;
            }}

            // A reload frame, sped up by Speed Cola
            action void ReloadTic() {{
                A_SetTics(invoker.PaceTics(invoker.reloadFrameRate, invoker.Default.reloadFrameRate));
            }}

            // A firing frame, sped up by Double Tap
            action void FireTic() {{
                A_SetTics(invoker.PaceTics(invoker.fireFrameRate, invoker.Default.fireFrameRate));
            }}

            // The rest of an animation, during which the player may already fire, aim, reload or
            // switch again
            action void Recover() {{
                if (invoker.zoomed && !invoker.toggledZoom) {{
                    A_WeaponReady(WRF_ALLOWRELOAD | WRF_NOBOB);
                }} else {{
                    A_WeaponReady(WRF_ALLOWRELOAD | WRF_ALLOWZOOM | WRF_NOBOB);
                }}
            }}

            // Aiming down the sights, by the weapon's own zoom (HH_Weapon.ZoomOut leaves it)
            action void AimIn() {{
                A_ZoomFactor(invoker.aimZoom);
                invoker.zoomed = true;
                invoker.BobRangeX = 0.15;
                invoker.BobRangeY = 0.15;
            }}

            // The animations start where the weapon rests, not where the engine raises it from
            action void AtRest() {{
                A_WeaponOffset(0, WEAPONTOP);
            }}

            action State WhenFirstRaise(StateLabel label) {{
                let first = !invoker.raisedBefore;
                invoker.raisedBefore = true;
                return invoker.When(first, label);
            }}

            action State WhenActiveAmmo(int count, StateLabel label) {{
                return invoker.When(invoker.activeAmmo == count, label);
            }}

            action State WhenFireHeld(StateLabel label) {{
                return invoker.When(self.player != null && (self.player.cmd.buttons & BT_ATTACK) != 0, label);
            }}

            // A reload that loads one round at a time (the Trench Gun): one from the reserve
            action void ReloadOne() {{
                if (invoker.activeAmmo < invoker.Default.activeAmmo && self.CountInv(invoker.ammoType1) > 0) {{
                    self.TakeInventory(invoker.ammoType1, 1);
                    ++invoker.activeAmmo;
                }}
            }}

            action State WhenLoaded(StateLabel label) {{
                return invoker.When(invoker.activeAmmo >= invoker.Default.activeAmmo || self.CountInv(invoker.ammoType1) == 0, label);
            }}

            // Fire, a stream of it: one flame, and a round every other tic
            action void ShootFlame(class<Actor> flame) {{
                A_FireProjectile(flame, FRandom[Doomify](-3, 3), false, 4, -4, 0, FRandom[Doomify](-2, 2));
                invoker.lastBurned = level.maptime;
                if (self.player != null && (level.maptime & 1) == 0) {{
                    invoker.activeAmmo = max(0, invoker.activeAmmo - 1);
                    if (invoker.activeAmmo == 0 && invoker.regenerates) {{
                        invoker.overheated = true;
                    }}
                }}
            }}

            action State WhenOverheated(StateLabel label) {{
                return invoker.When(invoker.overheated || invoker.activeAmmo <= 0, label);
            }}

            // The flamethrower's sound and tank: it roars while flames come out, and refills a
            // round a tic once it has been quiet a moment
            // An empty weapon's click, at most every few tics while the trigger is held
            action void DryClick(Sound click) {{
                if (level.maptime >= invoker.nextClick) {{
                    invoker.nextClick = level.maptime + 8;
                    A_StartSound(click, CHAN_WEAPON);
                }}
            }}

            // Its switch thrown, the Wunderwaffe comes back to life with a jolt that electrocutes
            // every zombie the player can see close by: the kills, and their points, are the player's
            action void Jolt() {{
                invoker.joltTics = JOLT_TICS;
                A_SetBlend("7fd8ff", 0.25, 10);
                A_StartSound("waw/tesla_bounce", CHAN_AUTO);
                Array<Actor> victims;
                let things = BlockThingsIterator.Create(self, JOLT_RANGE);
                while (things.Next()) {{
                    let thing = things.thing;
                    if ({p}_TeslaArc.Strikable(thing) && self.Distance3D(thing) <= JOLT_RANGE && self.CheckSight(thing, SF_IGNOREVISIBILITY)) {{
                        victims.Push(thing);
                    }}
                }}
                foreach (victim : victims) {{
                    if (!{p}_TeslaArc.Strikable(victim)) {{
                        continue;
                    }}
                    victim.A_StartSound("waw/elec_vocals", CHAN_VOICE);
                    for (let i = 0; i < 12; ++i) {{
                        victim.A_SpawnParticle("7fd8ff", SPF_FULLBRIGHT, 16, 4, 0,
                            FRandom[Doomify](-12, 12), FRandom[Doomify](-12, 12), FRandom[Doomify](8, victim.height),
                            FRandom[Doomify](-2, 2), FRandom[Doomify](-2, 2), FRandom[Doomify](0, 3), startalphaf: 1, fadestepf: 0.06);
                    }}
                    if ({p}_TeslaArc.Strikable(victim)) {{
                        victim.DamageMobj(invoker, self, max(victim.health, 1), 'Tesla', DMG_THRUSTLESS);
                    }}
                }}
            }}

            // The hands shaking with the jolt, less and less, then still again
            void Shake() {{
                if (self.joltTics <= 0 || self.owner == null || self.owner.player == null || self.owner.player.ReadyWeapon != self) {{
                    return;
                }}
                let layer = self.owner.player.FindPSprite(PSP_WEAPON);
                --self.joltTics;
                if (layer == null) {{
                    return;
                }}
                let strength = double(self.joltTics) / JOLT_TICS;
                layer.x = FRandom[Doomify](-5, 5) * strength;
                layer.y = WEAPONTOP + FRandom[Doomify](-3, 4) * strength;
            }}

            override void DoEffect() {{
                super.DoEffect();
                self.Shake();
                if (!self.regenerates || self.owner == null || self.owner.player == null) {{
                    return;
                }}
                let burning = level.maptime - self.lastBurned <= 2 && self.owner.player.ReadyWeapon == self;
                let roaring = self.owner.IsActorPlayingSound(BURN_CHANNEL);
                if (burning && !roaring) {{
                    self.owner.A_StartSound(self.igniteSound, CHAN_AUTO);
                    self.owner.A_StartSound(self.burnSound, BURN_CHANNEL, CHANF_LOOPING);
                }} else if (!burning && roaring) {{
                    self.owner.A_StopSound(BURN_CHANNEL);
                    self.owner.A_StartSound(self.cooldownSound, CHAN_AUTO);
                }}
                if (!burning && level.maptime - self.lastBurned > REFILL_DELAY && self.activeAmmo < self.Default.activeAmmo) {{
                    ++self.activeAmmo;
                    if (self.activeAmmo >= self.Default.activeAmmo) {{
                        self.overheated = false;
                    }}
                }}
            }}

            // Throws or plants equipment: one from what the player carries
            action void ThrowOne(class<Actor> thrown, double pitch = 0) {{
                if (self.CountInv(invoker.ammoType1) > 0) {{
                    self.TakeInventory(invoker.ammoType1, 1);
                    A_FireProjectile(thrown, 0, false, 0, 0, 0, pitch);
                }}
            }}

            // A knife: a swing that counts as melee for points
            action void Stab(int damage, double range) {{
                A_CustomPunch(damage, true, CPF_NOTURN, "{p}_DoomifyStab", range, meleeSound: "waw/melee_hit", missSound: "");
            }}
        }}

        class {p}_DoomifyStab : BulletPuff {{
            Default {{
                DamageType "Melee";
                +PUFFONACTORS
                +NOBLOOD
            }}
        }}

        // A projectile drawn with particles and a light, that bursts on impact
        class {p}_DoomifyShot : Actor {{
            meta Color trailColor;
            meta int splash;            // explosion radius; 0 for none
            meta int splashDamage;
            meta Sound impactSound;
            meta bool pulse;            // grows and shrinks as it flies (the Ray Gun's)

            property TrailColor: trailColor;
            property Splash: splash;
            property SplashDamage: splashDamage;
            property ImpactSound: impactSound;
            property Pulse: pulse;

            const PULSE_TICS = 10;      // tics growing, then as many shrinking
            const PULSE_GROWTH = 1.2;   // each tic's

            Default {{
                Projectile;
                Radius 5;
                Height 6;
                Speed 40;
                DamageType "Explosive";
                {p}_DoomifyShot.TrailColor "6cff3a";
                +BRIGHT
                +FORCEXYBILLBOARD
            }}

            override void PostBeginPlay() {{
                super.PostBeginPlay();
                self.A_AttachLight('DoomifyGlow', DynamicLight.PointLight, self.trailColor, 56, 0);
            }}

            override void Tick() {{
                super.Tick();
                if (self.bDestroyed || self.bMissile == false) {{
                    return;
                }}
                if (self.pulse) {{
                    let step = self.GetAge() % (2 * PULSE_TICS);
                    let size = self.Default.scale.x * PULSE_GROWTH ** min(step, 2 * PULSE_TICS - step);
                    self.scale = (size, size);
                }}
                for (let i = 0; i < 3; ++i) {{
                    self.A_SpawnParticle(self.trailColor, SPF_FULLBRIGHT | SPF_RELATIVE, 14, FRandom[Doomify](4, 8), 0,
                        FRandom[Doomify](-6, 0), FRandom[Doomify](-2, 2), FRandom[Doomify](-2, 2),
                        FRandom[Doomify](-0.5, 0.5), FRandom[Doomify](-0.5, 0.5), FRandom[Doomify](-0.5, 0.5),
                        startalphaf: 0.9, fadestepf: 0.06, sizestep: -0.3);
                }}
            }}

            // Sparks thrown out where it burst
            void Burst(int count) {{
                for (let i = 0; i < count; ++i) {{
                    self.A_SpawnParticle(self.trailColor, SPF_FULLBRIGHT, 24, FRandom[Doomify](3, 7), FRandom[Doomify](0, 360),
                        0, 0, 0, FRandom[Doomify](-5, 5), FRandom[Doomify](-5, 5), FRandom[Doomify](-2, 6),
                        accelz: -0.3, startalphaf: 1, fadestepf: 0.04);
                }}
            }}

            // Where it lands: its sound, its blast, sparks and a flash
            virtual void Impact() {{
                self.A_StartSound(self.impactSound, CHAN_AUTO);
                if (self.splash > 0) {{
                    self.A_Explode(self.splashDamage, self.splash, 0, false, self.splash / 3, damagetype: 'Explosive');
                }}
                self.Burst(24);
                self.A_AttachLight('DoomifyGlow', DynamicLight.PointLight, self.trailColor, 128, 0);
            }}

            States {{
            Spawn:
                TNT1 A -1;
                Stop;
            Death:
                TNT1 A 0 Impact;
                TNT1 A 6;
                TNT1 A 6 A_RemoveLight('DoomifyGlow');
                Stop;
            }}
        }}

        // A tongue of fire: slow, short-lived, through what it burns
        class {p}_DoomifyFlame : {p}_DoomifyShot {{
            Default {{
                Speed 22;
                Radius 10;
                Height 12;
                DamageType "Fire";
                ReactionTime 12;
                {p}_DoomifyShot.TrailColor "ff8a24";
                +RIPPER
                +NODAMAGETHRUST
            }}

            override void Tick() {{
                super.Tick();
                if (!self.bDestroyed && --self.reactionTime <= 0) {{
                    self.Destroy();
                }}
            }}
        }}

        // The Wunderwaffe's bolt: where it lands, lightning jumps from zombie to zombie
        class {p}_TeslaBolt : {p}_DoomifyShot {{
            meta int arcs;          // how many it can kill
            meta int arcRange;      // how far the lightning jumps

            property Arcs: arcs;
            property ArcRange: arcRange;

            Default {{
                Speed 60;
                DamageType "Tesla";
                {p}_DoomifyShot.TrailColor "7fd8ff";
                +HITTRACER
            }}

            override void Impact() {{
                // it does no damage of its own: a box it hits is knocked by hand
                HH_MysteryBox.Knock(self.tracer, self.pos, 400);
                let arc = {p}_TeslaArc(Actor.Spawn('{p}_TeslaArc', self.pos));
                if (arc != null) {{
                    arc.target = self.target;
                    arc.next = self.tracer;
                    arc.left = self.arcs;
                    arc.range = self.arcRange;
                    arc.trailColor = self.trailColor;
                }}
                self.A_StartSound(self.impactSound, CHAN_AUTO);
                self.Burst(32);
            }}
        }}

        // The lightning: kills a zombie, then jumps to the nearest one it hasn't struck that it
        // can see, until it has killed as many as it may or finds nobody close enough
        class {p}_TeslaArc : Actor {{
            Actor next;             // the zombie to strike next
            int left;
            int range;
            int kills;
            Color trailColor;
            Array<Actor> struck;

            Default {{
                +NOINTERACTION
                +NOBLOCKMAP
            }}

            // The nearest living monster within range that hasn't been struck, in sight
            Actor Nearest() {{
                Actor best = null;
                let bestDistance = double(self.range);
                let things = BlockThingsIterator.Create(self, self.range);
                while (things.Next()) {{
                    let thing = things.thing;
                    if (!{p}_TeslaArc.Strikable(thing) || self.struck.Find(thing) != self.struck.Size()) {{
                        continue;
                    }}
                    let distance = self.Distance3D(thing);
                    if (distance < bestDistance && self.CheckSight(thing, SF_IGNOREVISIBILITY)) {{
                        best = thing;
                        bestDistance = distance;
                    }}
                }}
                return best;
            }}

            // A jagged line of light from here to `to`
            void Bolt(Vector3 to) {{
                let from = self.pos;
                let steps = int(clamp(level.Vec3Diff(from, to).Length() / 6, 2, 96));
                for (let i = 0; i <= steps; ++i) {{
                    let along = level.Vec3Offset(from, level.Vec3Diff(from, to) * (double(i) / steps));
                    let jitter = (i == 0 || i == steps)? 0.0: 5.0;
                    self.A_SpawnParticle(i % 3 == 0? Color(255, 255, 255): self.trailColor, SPF_FULLBRIGHT, 8, FRandom[Doomify](3, 5), 0,
                        along.x - self.pos.x + FRandom[Doomify](-jitter, jitter),
                        along.y - self.pos.y + FRandom[Doomify](-jitter, jitter),
                        along.z - self.pos.z + FRandom[Doomify](-jitter, jitter),
                        startalphaf: 1, fadestepf: 0.12);
                }}
            }}

            // Only a living monster is ever struck: the bolt may have hit a door or a machine
            static bool Strikable(Actor thing) {{
                return thing != null && !thing.bDestroyed && thing.bIsMonster && thing.bShootable && thing.health > 0;
            }}

            void Strike() {{
                if (!{p}_TeslaArc.Strikable(self.next) || self.struck.Find(self.next) != self.struck.Size()) {{
                    self.next = self.Nearest();
                }}
                let victim = self.next;
                self.next = null;
                if (!{p}_TeslaArc.Strikable(victim) || self.left <= 0) {{
                    if (self.kills >= 4 && self.target != null) {{
                        self.target.A_StartSound("waw/tesla_happy", CHAN_AUTO);
                    }}
                    self.Destroy();
                    return;
                }}
                let chest = victim.pos + (0, 0, victim.height * 0.6);
                self.Bolt(chest);
                self.SetOrigin(chest, false);
                self.struck.Push(victim);
                --self.left;
                ++self.kills;
                self.A_StartSound("waw/tesla_bounce", CHAN_AUTO);
                self.A_AttachLight('DoomifyArc', DynamicLight.PointLight, self.trailColor, 96, 0);
                victim.A_StartSound("waw/elec_vocals", CHAN_VOICE);
                for (let i = 0; i < 16; ++i) {{
                    victim.A_SpawnParticle(self.trailColor, SPF_FULLBRIGHT, 16, 4, 0,
                        FRandom[Doomify](-12, 12), FRandom[Doomify](-12, 12), FRandom[Doomify](8, victim.height),
                        FRandom[Doomify](-2, 2), FRandom[Doomify](-2, 2), FRandom[Doomify](0, 3), startalphaf: 1, fadestepf: 0.06);
                }}
                // the shooter is the source, so the kill and its points are theirs; checked again,
                // as the sounds and sparks above may have run code on it
                if ({p}_TeslaArc.Strikable(victim)) {{
                    victim.DamageMobj(self, self.target, max(victim.health, 1), 'Tesla', DMG_THRUSTLESS);
                }}
            }}

            States {{
            Spawn:
                TNT1 A 3 Strike;
                Loop;
            }}
        }}

        // Something thrown that bounces and goes off after a while
        class {p}_DoomifyThrown : Actor {{
            meta int fuse;              // tics until it goes off
            meta int blastDamage;
            meta int blastRadius;
            meta Sound bounceSound;
            meta Sound blastSound;

            property Fuse: fuse;
            property BlastDamage: blastDamage;
            property BlastRadius: blastRadius;
            property BounceSound: bounceSound;
            property BlastSound: blastSound;

            Default {{
                Projectile;
                -NOGRAVITY
                // comes to rest instead of going off on the floor; the fuse sets it off
                BounceType "Grenade";
                +USEBOUNCESTATE
                +ALLOWBOUNCEONACTORS
                -NOBLOCKMAP
                BounceFactor 0.4;
                WallBounceFactor 0.4;
                Gravity 0.6;
                Radius 4;
                Height 6;
                Speed 22;
                Scale 0.5;
                DamageType "Explosive";
                {p}_DoomifyThrown.Fuse 105;
                {p}_DoomifyThrown.BlastDamage 30;
                {p}_DoomifyThrown.BlastRadius 220;
            }}

            override void PostBeginPlay() {{
                super.PostBeginPlay();
                self.reactionTime = self.fuse;
            }}

            override void Tick() {{
                super.Tick();
                if (self.bDestroyed) {{
                    return;
                }}
                if (self.vel.Length() > 0.5) {{
                    self.angle += 24;
                }}
                if (--self.reactionTime == 0) {{
                    self.Blast();
                }}
            }}

            virtual void Blast() {{
                self.A_StartSound(self.blastSound, CHAN_AUTO, attenuation: ATTN_NONE);
                self.A_Explode(self.blastDamage, self.blastRadius, 0, true, self.blastRadius / 4, damagetype: 'Explosive');
                for (let i = 0; i < 40; ++i) {{
                    self.A_SpawnParticle(Random[Doomify](0, 1)? Color(255, 170, 60): Color(90, 80, 70), SPF_FULLBRIGHT, 30, FRandom[Doomify](4, 10), 0,
                        0, 0, 4, FRandom[Doomify](-6, 6), FRandom[Doomify](-6, 6), FRandom[Doomify](0, 8), accelz: -0.3, startalphaf: 1, fadestepf: 0.03);
                }}
                self.A_AttachLight('DoomifyBlast', DynamicLight.PointLight, Color(255, 160, 70), 200, 0);
                self.SetStateLabel("Gone");
            }}

            States {{
            Bounce:
                "####" "#" 0 A_StartSound(invoker.bounceSound, CHAN_BODY);
                Goto Spawn;
            Death:
                "####" "#" -1;
                Stop;
            Gone:
                TNT1 A 8;
                TNT1 A 0 A_RemoveLight('DoomifyBlast');
                Stop;
            }}
        }}

        // The cymbal monkey: lands, plays its cymbals and every zombie near comes to it, then it
        // goes off
        class {p}_DoomifyMonkey : {p}_DoomifyThrown {{
            meta int lure;              // how far zombies hear it

            property Lure: lure;

            Default {{
                {p}_DoomifyThrown.Fuse 245;
                {p}_DoomifyMonkey.Lure 1600;
                +SHOOTABLE
                +NODAMAGE
                +NOBLOOD
                +NOTAUTOAIMED
                -NOTARGET
                Health 1000;
            }}

            override void Tick() {{
                super.Tick();
                if (self.bDestroyed || self.health <= 0 || self.reactionTime <= 0) {{
                    return;
                }}
                if (self.reactionTime % 12 == 0) {{
                    self.A_StartSound("waw/monk_cymb_m", CHAN_AUTO);
                }}
                // Horde Hammer sends zombies back after players; the monkey calls them every tic
                let things = BlockThingsIterator.Create(self, self.lure);
                while (things.Next()) {{
                    let zombie = things.thing;
                    if (zombie != null && zombie.bIsMonster && zombie.health > 0 && zombie.Distance2D(self) < self.lure) {{
                        zombie.target = self;
                    }}
                }}
            }}

            override void Blast() {{
                let blaster = self.target;
                super.Blast();
                // zombies it drew in go back after the players
                let things = BlockThingsIterator.Create(self, self.lure);
                while (things.Next()) {{
                    let zombie = things.thing;
                    if (zombie != null && zombie.target == self) {{
                        zombie.target = blaster;
                    }}
                }}
                self.bShootable = false;
            }}
        }}

        // A Bouncing Betty: planted, it waits for a zombie to come close, jumps and goes off
        class {p}_DoomifyMine : Actor {{
            meta int blastDamage;
            meta int blastRadius;

            property BlastDamage: blastDamage;
            property BlastRadius: blastRadius;

            Default {{
                Projectile;
                -NOGRAVITY
                Speed 6;
                Radius 6;
                Height 4;
                Scale 0.5;
                ReactionTime 35;
                DamageType "Explosive";
                {p}_DoomifyMine.BlastDamage 60;
                {p}_DoomifyMine.BlastRadius 200;
                +DONTSPLASH
            }}

            bool Triggered() {{
                let things = BlockThingsIterator.Create(self, 64);
                while (things.Next()) {{
                    let thing = things.thing;
                    if (thing != null && thing.bIsMonster && thing.health > 0 && self.Distance2D(thing) < 56) {{
                        return true;
                    }}
                }}
                return false;
            }}

            States {{
            Death:
                // planted: a second to arm
                "####" "#" 35 {{
                    self.vel = (0, 0, 0);
                    self.A_StartSound("waw/bbetty_plant_default", CHAN_BODY);
                }}
            Watch:
                "####" "#" 2 A_JumpIf(invoker.Triggered(), "Jump");
                Loop;
            Jump:
                "####" "#" 0 {{
                    self.A_StartSound("waw/bbetty_plant_default", CHAN_BODY);
                    self.vel.z = 9;
                    self.bNoGravity = false;
                }}
                "####" "#" 10;
                TNT1 A 0 {{
                    self.A_StartSound("waw/grenade_explode", CHAN_AUTO, attenuation: ATTN_NONE);
                    self.A_Explode(self.blastDamage, self.blastRadius, 0, true, self.blastRadius / 4, damagetype: 'Explosive');
                    self.A_AttachLight('DoomifyBlast', DynamicLight.PointLight, Color(255, 160, 70), 200, 0);
                }}
                TNT1 A 8;
                Stop;
            }}
        }}
    "#####}
}

/// What the screen does while a Wunderwaffe jolts to life: the shock shader (`shock_shader`),
/// switched on for the player whose gun it is, fading with the jolt.
pub fn shock(p: &str) -> String {
    formatdoc! {r#"
        // The shock on screen while the Wunderwaffe in hand jolts to life (GLDEFS' {p}_Shock)
        class {p}_DoomifyShock : EventHandler {{
            override void UiTick() {{
                let player = players[consolePlayer];
                let weapon = {p}_DoomifyWeapon(player.ReadyWeapon);
                if (weapon != null && weapon.joltTics > 0) {{
                    Shader.SetEnabled(player, "{p}_Shock", true);
                    Shader.SetUniform1f(player, "{p}_Shock", "strength", double(weapon.joltTics) / {p}_DoomifyWeapon.JOLT_TICS);
                    Shader.SetUniform1f(player, "{p}_Shock", "shockTime", level.maptime / double(TICRATE));
                }} else {{
                    Shader.SetEnabled(player, "{p}_Shock", false);
                }}
            }}
        }}
    "#}
}

/// The shock: jagged lightning crawling all around the gun in hand, flickering, drawn at
/// Doom's pixel size so they look like the rest of the game, and a cold tint over everything.
pub fn shock_gldefs(p: &str) -> String {
    formatdoc! {r#"
        // The Wunderwaffe's shock, switched on by {p}_DoomifyShock
        HardwareShader PostProcess scene
        {{
            Name "{p}_Shock"
            Shader "shaders/doomify/shock.fp" 330
            Uniform float strength
            Uniform float shockTime
        }}
    "#}
}

pub const SHOCK_SHADER: &str = r#"// The Wunderwaffe's shock: lightning crawling all around the gun in hand, jagged arcs jumping
// between points of a ring about where the gun sits on screen (bottom right of centre), in blocks
// of Doom's pixels (200 rows a screen), flickering 24 times a second; `strength` fades it.

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float wobble(float x, float seed) {
    float i = floor(x);
    float f = fract(x);
    return mix(hash(vec2(i, seed)), hash(vec2(i + 1.0, seed)), f * f * (3.0 - 2.0 * f));
}

// How far p is from a jagged bolt between a and b
float bolt(vec2 p, vec2 a, vec2 b, float seed) {
    vec2 ab = b - a;
    float len = max(length(ab), 0.0001);
    vec2 dir = ab / len;
    vec2 side = vec2(-dir.y, dir.x);
    float s = clamp(dot(p - a, dir) / len, 0.0, 1.0);
    float bend = (wobble(s * 7.0, seed) - 0.5) * 0.35 + (wobble(s * 29.0, seed + 7.0) - 0.5) * 0.12;
    vec2 along = a + ab * s + side * bend * sin(3.14159 * s) * len;
    return length(p - along);
}

void main() {
    vec2 size = vec2(textureSize(InputTexture, 0));
    float block = max(1.0, floor(size.y / 200.0));
    vec2 cell = (floor(TexCoord * size / block) + 0.5) * block / size;
    vec4 scene = texture(InputTexture, TexCoord);
    float aspect = size.x / size.y;
    vec2 p = vec2(cell.x * aspect, cell.y);
    // the ring around the gun: its centre, and how far it reaches across and up
    vec2 centre = vec2((0.5 + 0.17) * aspect, 0.2);
    vec2 reach = vec2(0.24 * aspect, 0.28);
    float pixel = block / size.y;
    float flicker = floor(shockTime * 24.0);
    float glow = 0.0;
    float core = 0.0;
    for (int i = 0; i < 9; ++i) {
        float seed = flicker * 9.0 + float(i);
        if (hash(vec2(seed, 1.0)) > strength * 0.8 + 0.3) {
            continue;
        }
        // an arc from one point of the ring to another a little way round, in or out a bit
        float from = 6.28318 * hash(vec2(seed, 2.0));
        float to = from + (0.5 + 1.1 * hash(vec2(seed, 3.0))) * (hash(vec2(seed, 4.0)) < 0.5 ? -1.0 : 1.0);
        float r1 = 0.85 + 0.3 * hash(vec2(seed, 5.0));
        float r2 = 0.85 + 0.3 * hash(vec2(seed, 6.0));
        vec2 a = centre + vec2(cos(from), sin(from)) * reach * r1;
        vec2 b = centre + vec2(cos(to), sin(to)) * reach * r2;
        float d = bolt(p, a, b, seed);
        core = max(core, step(d, pixel * 1.5));
        glow = max(glow, 1.0 - smoothstep(0.0, pixel * 7.0, d));
    }
    vec3 arc = vec3(0.5, 0.85, 1.0);
    vec3 colour = mix(scene.rgb, scene.rgb * vec3(0.8, 0.95, 1.25), strength * 0.5);
    colour += arc * glow * 0.8 * strength;
    colour = mix(colour, vec3(0.92, 0.98, 1.0), core * min(1.0, strength * 1.6));
    FragColor = vec4(colour, scene.a);
}
"#;
