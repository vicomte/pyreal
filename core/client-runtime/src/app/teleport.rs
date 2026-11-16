//! Portal transitions and authoritative player placement.

use super::*;

impl<S: Shell> App<S> {
    /// The teleport animation's per-frame step, inside frame step 7.
    ///
    /// Three things reach the rest of the client from here and nothing else does: the world stops
    /// being drawn, the projection's field-of-view distance is overridden, and
    /// the two portal sounds are played. Everything is a function of the teleport state; nothing
    /// accumulates per frame.
    pub fn teleport_use_time(&mut self, shell: &S) {
        self.duties.teleport_ticked = true;
        let now = self.timer.cur_time;
        let before = (self.teleport.tunnels_played, self.teleport.anim.state);
        let was_teleporting = self.teleport.anim.teleport_in_progress;
        self.teleport.anim_use_time(now, self.game_view_distance());
        // The player's teleport-in-progress flag holds the busy cursor up from the moment a
        // portal (or the log-in, or a log-off's fade) starts until the world has faded back in.
        let teleporting = self.teleport.anim.teleport_in_progress;
        if teleporting != was_teleporting {
            let busy = &mut self.objects.world.magic.busy_count;
            *busy = if teleporting {
                busy.saturating_add(1)
            } else {
                busy.saturating_sub(1)
            };
        }
        // A report line per state change: an animation that is *absent* on a live run must be
        // visible in the log, and a run that plays it must be able to say so.
        let after = self.teleport.anim.state;
        if after != before.1 {
            tracing::debug!(
                "portal-tunnel animation {:?} -> {after:?} at t={now:.2} \
                 (world {}, vdist {:?}, tunnel #{})",
                before.1,
                if self.teleport.world_hidden() {
                    "hidden"
                } else {
                    "drawn"
                },
                self.teleport.view_distance(),
                self.teleport.tunnels_played,
            );
        }
        let _ = before.0;
        // The rotation block's
        // notice send -- one fixed
        // built-in literal ( `L"In Portal Space - Please Wait..."`) on the tunnel's
        // entry frame and on every re-aim — lands on the client scroll handler,
        // which adds `(text, 0x1A, true, 0)`, drained by the
        // HUD at the top of the next frame into the spew panel (type `0x1A`'s only default
        // destination). Without this the animation raises the effect and nothing takes it.
        for (channel, text) in self.teleport.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(channel, &text);
        }
        // World drawing reads `hidden`; the normal world render reads the override.
        // Credits has no backdrop element. Retail's black is the frame-begin clear:
        // the character-management screen builds the rotating preview, while the credits screen
        // only creates its two authored roots. Keep this first slice mode-specific so
        // the character-management presentation remains independent; entering Credits hides the
        // retained world immediately, and its existing action back to character management shows
        // that presentation again on the next frame.
        let credits = shell.hides_world();
        self.present.set_world_view_state(
            self.teleport.world_hidden() || credits,
            self.teleport.view_distance(),
        );
        if self.teleport.look_began {
            self.choose_portal_look();
        }
        let look = self.teleport.portal_look(now);
        self.present.set_portal_look(look);
        // The client sends the character login-complete notification, which is `0x00A1` on the Weenie queue.
        //
        // ACE uses it to run `Player.HandleLoginComplete`: measured on the live shard, a client
        // that never sends it is answered with `0xF659 Character_CharacterError(1)
        // ID_CHAR_ERROR_LOGON` a few seconds after entering the world. It belongs here because the
        // *animation* is what calls it -- once at the end of the world fade-in, and once in
        // the idle teleport-animation state for a teleport that played no animation.
        //
        // The client's own gate requires the player object to exist **and** every id in the two
        // content-profile lists to have a world object.
        // Only the first half is checked here; the contained-objects half needs a reachable answer
        // from the object tables and is not checked. Until it passes, the client's own
        // pending-login-complete retry is reproduced: the flag stays set and is tried again
        // next frame.
        if self.teleport.login_complete_owed() && self.objects.player().is_some() {
            if let Some(link) = self.link.as_mut() {
                match link
                    .net
                    .session
                    .send_action(&pyreal_protocol::login::CharacterLoginCompleteNotification)
                {
                    Ok(stamp) => {
                        tracing::debug!("0x00A1 login complete (stamp {stamp})");
                        self.teleport.login_completes_sent += 1;
                        self.teleport.login_complete_sent();
                        // The server takes module saves from here on.
                        self.objects.world.player_system.login_complete_sent = true;
                    }
                    Err(e) => tracing::warn!("0x00A1 would not encode: {e}"),
                }
            } else {
                // No server to tell; do not spin on it every frame.
                self.teleport.login_complete_sent();
            }
        }
        let sounds = self.teleport.take_sounds();
        if !sounds.is_empty() {
            if let (Some(table), Some(audio)) = (self.ui_sound_table, self.audio.as_mut()) {
                for stype in sounds {
                    audio.play_ui_sound(pyreal_audio::UiSoundRef::Table { table, stype });
                }
            }
        }
    }

    /// What a trip through portal space came from, and its tunnel's shape: the same portal, spell
    /// or character always gives the same tunnel.
    fn choose_portal_look(&mut self) {
        use pyreal_client_contract::teleport::PortalSource as Src;
        use pyreal_client_model::world::Departure;
        fn fold(h: u32, bytes: &[u8]) -> u32 {
            bytes.iter().fold(h ^ 0x811c_9dc5, |h, b| {
                (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)
            })
        }
        /// The item type of a portal.
        const PORTAL: u32 = 0x0001_0000;
        let world = &mut self.objects.world;
        let me = world.player.map_or(0, |p| p.0);
        let departure = world.departure.take();
        // The portal stepped through, for its colour.
        let mut portal = None;
        let (source, seed) =
            if self.teleport.anim.state == crate::teleport::TeleportAnimState::WorldFadeOut {
                (Src::LogOff, fold(me, b"leaving"))
            } else if !self.teleport.entered {
                (Src::Login, fold(me, b"arriving"))
            } else {
                match departure {
                    Some(Departure::Used(id)) => match world.weenie(id) {
                        Some(w) if w.pwd.obj_type & PORTAL != 0 => {
                            portal = world.physics(id).map(|p| p.setup_id);
                            (Src::Portal, fold(w.pwd.wcid, w.pwd.name.as_bytes()))
                        }
                        _ => (Src::Portal, fold(id.0, b"portal")),
                    },
                    Some(Departure::Cast(spell)) => {
                        let name = world
                            .magic
                            .spell_table
                            .as_ref()
                            .and_then(|t| t.spells.get(&spell))
                            .map(|b| b.name.to_lowercase())
                            .unwrap_or_default();
                        let source = if name.contains("lifestone") {
                            Src::Lifestone
                        } else if ["house", "mansion", "hometown", "villa", "allegiance"]
                            .iter()
                            .any(|w| name.contains(w))
                        {
                            Src::House
                        } else if ["recall", "portal", "gateway", "summon"]
                            .iter()
                            .any(|w| name.contains(w))
                        {
                            Src::Recall
                        } else {
                            Src::Portal
                        };
                        (source, fold(spell, &me.to_le_bytes()))
                    }
                    None => (Src::Portal, fold(self.teleport.look_seed, b"elsewhere")),
                }
            };
        // `PYREAL_PORTAL_SOURCE` picks one, for captures.
        let source = match std::env::var("PYREAL_PORTAL_SOURCE").as_deref() {
            Ok("portal") => Src::Portal,
            Ok("login") => Src::Login,
            Ok("lifestone") => Src::Lifestone,
            Ok("house") => Src::House,
            Ok("recall") => Src::Recall,
            Ok("logoff") => Src::LogOff,
            _ => source,
        };
        let seed = std::env::var("PYREAL_PORTAL_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(seed);
        // A portal walked into names no use: the one the player stood at.
        if source == Src::Portal && portal.is_none() {
            portal = self.teleport.look_walked;
        }
        self.teleport.look_walked = None;
        tracing::debug!("portal space from {source:?}, portal setup {portal:#X?}");
        // `PYREAL_PORTAL_SETUP` the portal's, for captures.
        let portal = std::env::var("PYREAL_PORTAL_SETUP")
            .ok()
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .or(portal);
        self.teleport.look_source = source;
        self.teleport.look_seed = seed;
        self.teleport.look_portal = portal.filter(|_| source == Src::Portal);
    }

    /// The teleport tunnel's swirl, stepped once a frame straight after the teleport tick.
    ///
    /// The tunnel is not just a hidden world: it is a preview space, built and driven from its UI
    /// update:
    ///
    /// * one object, the enum `0x10000001` setup -- `portalspace_background`;
    /// * one `DISTANT_LIGHT` at intensity **2.0**, direction `(0.3, -1.9, 0.65)` -- **negative**
    ///   y, where the 3D character preview's is positive;
    /// * the camera at `(0.24, -2.7, 0.88)`, re-issued every tunnel frame by the update;
    /// * the smart-box field of view, which makes the teleport's projection collapse apply to the
    ///   swirl as well as the world;
    /// * `set_sequence_animation(<DID for enum 0x10000002>, clear = 1, low = 1, **40.0** fps)`,
    ///   started when the portal-space element becomes visible, and cleared when the
    ///   tunnel ends;
    /// * a camera direction of `(0, current rotation angle, 0)` every frame, which is the eased
    ///   spin the teleport animation computes and exposes as `TeleportAnim::rotation_angle`.
    ///
    /// A 1.1 scale is **not** applied to the portal space: the client's 1.1 scale is set on the
    /// *world* camera, unconditionally.
    ///
    /// The sequence's real frame counter is what ends the tunnel ([`crate::teleport::Teleport`]),
    /// so the space is stepped whatever draws it; where it is drawn is the front end's
    /// ([`Shell::place_portal_space`]). A front end whose UI step needs this frame's tunnel state
    /// runs it at its own point, straight after [`Self::teleport_use_time`]; otherwise the runtime
    /// runs it at the foot of the UI step. See [`FrameDuties`].
    pub fn portal_space_use_time(&mut self, shell: &mut S) {
        use pyreal_client_contract::overlay::{PreviewLight, PreviewSpace};
        use pyreal_client_contract::teleport::{portal_space as ps, timing};

        self.duties.portal_driven = true;
        let now = self.timer.cur_time;
        // Elapsed seconds, on the same terms as the char-gen space's: nothing accumulated.
        let dt = (now - self.duties.portal_last_time).clamp(0.0, 0.25);
        self.duties.portal_last_time = now;
        let id = PreviewSpace::Portal;
        let tunnel = self.teleport.anim.state.is_tunnel();
        if !tunnel {
            // The tunnel fade-out's end: clear the teleport object's sequence anims and hide.
            if self.present.preview_has_anims(id, 0) {
                self.present.preview_clear_sequence_anims(id, 0);
            }
            self.teleport.portal_anim_frame = None;
            return;
        }

        let assets = std::sync::Arc::clone(&self.anim_assets);
        let store = std::sync::Arc::clone(&self.store);
        if self.present.preview_ensure(id, &assets) {
            // Start the portal-space animation once, on the transition to visible.
            let obj = crate::assets::enum_did(&*store, ps::UIASSET_GROUP, ps::OBJECT_ENUM);
            match obj {
                Some(o) => match self.present.preview_add_object(id, &store, o) {
                    Ok(Some(_)) => {}
                    Ok(None) => tracing::warn!("the portal object {o:?} would not load"),
                    Err(e) => tracing::warn!("the portal space failed: {e}"),
                },
                None => tracing::warn!("UIASSET portalspace_background does not resolve"),
            }
            self.present.preview_set_light(
                id,
                PreviewLight::Directional,
                ps::LIGHT_INTENSITY,
                pyreal_primitives::Vec3::new(
                    ps::LIGHT_DIRECTION.0,
                    ps::LIGHT_DIRECTION.1,
                    ps::LIGHT_DIRECTION.2,
                ),
            );
            self.present.preview_use_world_fov(id);
        }

        // The update's portal-space-not-visible arm: start the sequence, place the camera,
        // show the space and hide the world. The teleport animation has already hidden the world.
        let anim = crate::assets::enum_did(&*store, ps::UIASSET_GROUP, ps::ANIMATION_ENUM);
        let angle = self.teleport.anim.rotation_angle;
        if !self.present.preview_has_anims(id, 0) {
            if let Some(a) = anim {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: `set_sequence_animation`'s framerate argument is a `float` literal
                // in the client -- 40.0.
                let fps = timing::PORTAL_FRAMERATE as f32;
                if !self
                    .present
                    .preview_set_sequence_animation(id, 0, a, true, 1, fps)
                {
                    // Said once: a presentation with no preview (the headless world) answers so
                    // every frame of portal space.
                    static SAID: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        tracing::warn!("portalspace_animation {a:?} is not in the dat");
                    }
                }
            }
        }
        self.present.preview_set_camera_position(
            id,
            pyreal_primitives::Vec3::new(
                ps::CAMERA_POSITION.0,
                ps::CAMERA_POSITION.1,
                ps::CAMERA_POSITION.2,
            ),
        );
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the client's current rotation angle is a `double`, while the preview camera direction
        // is a `Vec3` of `float`s; the client narrows it here too.
        self.present.preview_set_camera_direction_degrees(
            id,
            pyreal_primitives::Vec3::new(0.0, angle as f32, 0.0),
        );
        self.present.preview_use_time(id, dt);
        // The sequence's real frame counter.
        self.teleport.portal_anim_frame = self.present.preview_curr_frame_number(id, 0);
        // Where it draws is the front end's.
        shell.place_portal_space(&mut *self.present);
    }

    /// Move the player's own body to where the server has just put him.
    ///
    /// [`complete_player_teleport_at`] includes the accepted-edge relocation, teleport_hook and
    /// player-teleported command tail. The free seam lets tests drive the same path over a real
    /// body and inspect outgoing movement without a live connection.
    pub(super) fn player_teleport_use_time(&mut self, shell: &mut S) {
        // Before every early return: this counts the **call**, not the teleport, which is the only
        // thing that can tell a frame that skipped this step from a frame with nothing to do. The
        // pattern is the same as `position_use_times`: a step nothing drives is invisible to every
        // anchor in this project, and a mutation that deletes this call from `App::frame` survives
        // everything but this counter.
        //
        // Accepted local movement and its control/teleport callbacks complete before another
        // queued message is admitted, so this also runs at each delivery boundary inside
        // `deliver_session_events` — the analogue of the smart box's event dispatch handling
        // a received position once per admitted blob. The per-frame call is
        // `step_incoming_world_objects`'s, and it is unconditional; see the note there.
        self.events.push(FrameEvent::PlayerTeleportUseTime);
        if self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_none()
        {
            // No local body can execute the tail. Preserve the latest viewer/initial-placement
            // snapshots, but never retain a journal that a later body would replay out of time.
            let discarded = self.objects.discard_bodyless_player_dispatches();
            if discarded > 0 {
                self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                    count: discarded,
                    reason: NoBodyReason::JournalDiscarded,
                });
            }
            return;
        }
        if self.objects.has_player_motion_dispatches() {
            // The packet stage has already accepted this batch's creates. Materialize their
            // existing shared scene prefix before calling the movement manager: a later setup
            // rebuild would otherwise erase the command, and a newly created target would be
            // mistaken for the missing-object MoveToPosition fallback. This is the pre-existing
            // create-before-movement batch contract, not a journal of all WorldObjects object edges.
            let store = std::sync::Arc::clone(&self.store);
            if let Err(e) =
                self.present
                    .prepare_object_dispatch(&store, &mut self.objects, self.world.as_mut())
            {
                tracing::warn!("player dispatch preparation failed: {e}");
                return;
            }
            if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                if let Some(c) = world.world_mut().character.as_mut() {
                    // As in ordinary object sync, claim the local id before adding remote
                    // bodies. No physics clock advances here. A target's DAT dimensions must
                    // exist before MoveToObject captures them, not after its first request.
                    c.adopt_server_id(self.objects.player());
                    self.objects.sync_physics_at(
                        &store,
                        &mut c.world,
                        pyreal_primitives::LocalTime(self.timer.cur_time),
                    );
                }
                world.world_mut().refresh_object_geometry(&self.objects);
            }
        }
        // Movement-event and received-position handlers
        // execute these calls in message order. Independent pending snapshots would invert a
        // `0xF74C` followed by a teleport: the approach would be installed after teleport_hook
        // canceled it.
        for event in self.objects.take_player_motion_dispatches() {
            use crate::objects::PlayerMotionDispatch;
            let Some(mut world) = self.present.scene_mut(self.world.as_mut()) else {
                // The destination remains on Presence for initial scene placement. No body's
                // motion/teleport callback exists yet, so neither can be replayed on a new body.
                if matches!(event, PlayerMotionDispatch::Teleport { .. }) {
                    self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                        count: 1,
                        reason: NoBodyReason::NoScene,
                    });
                }
                continue;
            };
            match event {
                PlayerMotionDispatch::Movement(buf) => {
                    if world.character().is_some() {
                        world
                            .world_mut()
                            .dispatch_player_movement(&buf, &self.objects);
                        if world.world_mut().take_player_movement_applied() {
                            // The server made this state, so it is not echoed as a `0xF61C`.
                            if let Some(c) = world.character() {
                                self.position
                                    .rebaseline_motion(&raw_motion_state_to_wire_in(
                                        &c.driver().movement.interp.raw_state,
                                        self.objects.command_numbering(),
                                    ));
                            }
                            // set_object_movement's accepted-player return is acted on immediately,
                            // not after a later player-teleported reapplication in this batch.
                            self.movement.lose_control_to_server_with_finish(
                                &mut self.char_input,
                                || {
                                    crate::jump::finish(
                                        &mut self.objects.world.combat,
                                        world.character(),
                                    )
                                },
                            );
                            self.events.push(FrameEvent::ServerControlLost(
                                ControlLossSite::MovementDispatch,
                            ));
                        }
                    }
                }
                PlayerMotionDispatch::Teleport {
                    position: pos,
                    timestamps,
                } => {
                    let numbering = self.objects.command_numbering();
                    let Some(character) = world.world_mut().character.as_mut() else {
                        self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                            count: 1,
                            reason: NoBodyReason::NoCharacter,
                        });
                        continue;
                    };
                    let previous_cell = character.position().cell;
                    let had_previous_cell = character
                        .world
                        .get(character.handle)
                        .and_then(|body| body.cell)
                        .is_some();
                    complete_player_teleport_at(
                        pos,
                        character,
                        &mut self.movement,
                        &mut self.char_input,
                        |character| {
                            if let Some(link) = self.link.as_mut() {
                                self.position.send_movement_event(
                                    self.timer.cur_time,
                                    &body_motion_in(character, timestamps, numbering),
                                    &mut link.net.session,
                                );
                            }
                        },
                    );
                    self.events.push(FrameEvent::PlayerTeleportApplied);
                    if !had_previous_cell || pos.cell != previous_cell {
                        // Cell-release processing sees the body after the simple position set has
                        // committed the destination. Publish that live membership before the
                        // queued landscape release asks ObjectStream which cell the player left;
                        // the ordinary post-physics publication is later than this edge.
                        self.objects.publish_physics_cells(&character.world);
                        world.release_landscape_for_teleport(pos.cell.landblock());
                    }
                    world.world_mut().follow_character_now();
                    tracing::info!(
                        "the server teleported the player to cell {:#010X} at ({:.2}, {:.2}, {:.2})",
                        pos.cell.0, pos.frame.origin.x, pos.frame.origin.y, pos.frame.origin.z,
                    );
                }
            }
            // The view holds the presentation and the world state; hand both back first.
            drop(world);
            self.deliver_jump_power_bar_notices(shell);
        }
        for text in self.movement.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(pyreal_client_model::scroll::LOCAL_ERROR_TYPE, text);
        }
    }
}

/// The setup of the portal nearest `character`, as close as a body that walked into it stands.
pub(super) fn portal_at(
    world: &pyreal_client_model::world::World,
    character: &crate::character::Character,
) -> Option<u32> {
    /// `ObjectDescriptionFlag` bit 18: the object is a portal.
    const PORTAL: u32 = 0x0004_0000;
    /// How far from a portal's foot such a body stands at most, metres (seen at 7 as the
    /// server's teleport arrives).
    const REACH: f32 = 10.0;
    let at = character.position();
    world
        .tables
        .physics
        .iter()
        .filter(|(id, _)| {
            world
                .weenie(*id)
                .is_some_and(|w| w.pwd.bitfield & PORTAL != 0)
        })
        .filter_map(|(id, p)| {
            let o = character
                .world
                .by_object_id(id)
                .and_then(|h| character.world.get(h))?;
            let d = pyreal_primitives::position::distance(&at, &o.position);
            (d <= REACH).then_some((d, p.setup_id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, setup)| setup)
}
