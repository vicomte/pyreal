//! Roles: what each emitter of a body is, decided once for its emitter and its picture.
//!
//! A creature's look is its emitters'. Each is told by its picture and how its particles behave
//! ([`role_of`]): liquid, a volume of vapour or smoke, flame, lightning, light, a lamp's light
//! sprite, or a picture drawn as it is. The answer is kept for the emitter's info and picture
//! ([`Particles::role`]), so an emitter keeps its role while others come and go (a spell's, a
//! summoning's), and one whose picture is not ready yet is asked again until it is. Fire
//! ([`crate::body_fire`]), the volumes ([`crate::puffs`]), lightning ([`crate::bolts`]), liquid
//! and light ([`crate::glow`]) all read it, and so do a body's lamps ([`Particles::setup_lamps`]).

use std::sync::Arc;

use glam::Vec3;
use pyreal_animation::data::ParticleEmitterInfo;
use pyreal_animation::particles::ParticleEmitter;
use pyreal_animation::MotionDriver;

use crate::lamps::{Lamp, LampKind};
use crate::particles::{blob_of, is_light_sprite, sprite_of, Particles, Sprite};

/// How an emitter of a body is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// As its picture, as [`crate::particles::look_of`] draws it.
    Picture,
    /// One steady additive sprite standing for a lamp's light.
    Light,
    /// Liquid ([`blob_of`]): which.
    Blob(u8),
    /// One of the body's volumes ([`crate::puffs`]): its kind.
    Volume(u8),
    /// Flame ([`crate::body_fire`]): its colour.
    Fire(Vec3),
    /// Lightning, in a body with a volume ([`crate::bolts`]); alone, as [`lone`] says.
    Bolt,
    /// Light, a ball (0) or a star (1) ([`crate::glow`]).
    Glow(u8),
}

/// The role of an emitter `e` drawing picture `id` (`sprite`).
#[must_use]
pub fn role_of(id: u32, sprite: &Sprite, e: &ParticleEmitterInfo) -> Role {
    if let Some(b) = blob_of(id) {
        Role::Blob(b)
    } else if let Some(k) =
        crate::puffs::smoke_of(sprite, e).or_else(|| crate::puffs::kind_of(sprite, e))
    {
        Role::Volume(k)
    } else if crate::bolts::is_bolt(sprite, e) {
        Role::Bolt
    } else if let Some(c) = sprite.fire {
        Role::Fire(c)
    } else {
        lone(sprite, e)
    }
}

/// The role of an emitter that is no volume, fire or lightning: light, a lamp's sprite or a picture.
#[must_use]
pub fn lone(sprite: &Sprite, e: &ParticleEmitterInfo) -> Role {
    match sprite.glow {
        Some(g) if sprite.additive() => Role::Glow(g.kind as u8),
        _ if sprite.additive() && is_light_sprite(e) => Role::Light,
        _ => Role::Picture,
    }
}

/// The roles kept: each by its emitter info (held, so its address is never another's) and picture.
// ORDER-OK: looked up by emitter, never iterated.
pub type Roles = std::collections::HashMap<(usize, u32), (Arc<ParticleEmitterInfo>, Role)>;

impl Particles {
    /// Emitter `e`'s role, kept once its picture is ready; `None` while it is not, or when the
    /// picture draws nothing.
    pub fn role(&mut self, e: &ParticleEmitter) -> Option<Role> {
        let id = sprite_of(&e.info).0;
        let key = (Arc::as_ptr(&e.info) as usize, id);
        if let Some((_, r)) = self.roles.get(&key) {
            return Some(*r);
        }
        let r = role_of(id, self.sprites.get(&id)?.as_ref()?, &e.info);
        self.roles.insert(key, (Arc::clone(&e.info), r));
        Some(r)
    }

    /// Emitter `e`'s role as kept, or as it would be.
    #[must_use]
    pub fn role_now(&self, e: &ParticleEmitter) -> Option<Role> {
        let id = sprite_of(&e.info).0;
        match self.roles.get(&(Arc::as_ptr(&e.info) as usize, id)) {
            Some((_, r)) => Some(*r),
            None => Some(role_of(id, self.sprites.get(&id)?.as_ref()?, &e.info)),
        }
    }

    /// What `driver`'s emitters carry: whether any burns as fire, and any light of its own (a
    /// glow other than a glint, an acid cloud's).
    #[must_use]
    pub fn carries(&self, driver: &MotionDriver) -> (bool, bool) {
        let (mut fire, mut light) = (false, false);
        let molten = crate::body_fire::molten_core(driver);
        for e in driver.particles.iter() {
            match self.role_now(e) {
                Some(Role::Fire(_)) => fire |= self.fire_bodies && !molten,
                Some(Role::Glow(_)) => {
                    let glint = self
                        .sprites
                        .get(&sprite_of(&e.info).0)
                        .and_then(|s| s.as_ref()?.glow)
                        .is_some_and(|g| g.glint());
                    light |= !glint;
                }
                Some(Role::Volume(crate::puffs::ACID)) => light |= self.smoke,
                _ => {}
            }
        }
        (fire, light)
    }

    /// Which of `lamps`, its setup's placed where `driver`'s body stands, still light it: none
    /// while it burns (its fire's lamp stands for them), and not its emitters' glow while they
    /// carry their own ([`Self::gather`]).
    #[must_use]
    pub fn setup_lamps(&self, driver: &MotionDriver, mut lamps: Vec<Lamp>) -> Vec<Lamp> {
        // A portal drawn as a vortex carries its own light ([`crate::gates::lamp`]).
        if self.portals && crate::gates::is_portal(driver) {
            lamps.clear();
            return lamps;
        }
        // A crystal's light is its motes', in its own colour ([`crate::sparks`]).
        if self.crystal_tint(driver).is_some() {
            lamps.clear();
        }
        match self.carries(driver) {
            (true, _) => lamps.clear(),
            (false, true) => lamps.retain(|l| l.kind != LampKind::Glow),
            (false, false) => {}
        }
        // A body of snow ([`crate::puffs::SNOW`]) is lit, not a light: its cold lights would pool
        // their colour on the ground round it.
        let snowy = self.smoke
            && driver
                .particles
                .iter()
                .any(|e| self.role_now(e) == Some(Role::Volume(crate::puffs::SNOW)));
        if snowy {
            lamps.clear();
        }
        lamps
    }
}
