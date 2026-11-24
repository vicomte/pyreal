//! Ordered application shutdown.

use super::*;

impl<S: Shell> App<S> {
    /// Run the normal cleanup path.
    ///
    /// The order is [`crate::shutdown::Step::ORDER`]; the recording it returns is what
    /// `pyreal/client/tests/gpu/presentation/shutdown.rs` asserts on, because an ordering nothing checks is a comment.
    pub fn shutdown(mut self, shell: &mut S) -> crate::shutdown::CleanupLog {
        let log = self.release_session(shell);
        // The device goes first, while the window it presents to is still open. Vulkan on Wayland
        // reads the display connection as it destroys the swap chain, so the other order crashes.
        drop(self.present);
        log
    }

    /// Release the session and its resources, returning the device to its host.
    pub fn shutdown_retaining_presentation(
        mut self,
        shell: &mut S,
    ) -> (Box<S::Present>, crate::shutdown::CleanupLog) {
        let log = self.release_session(shell);
        (self.present, log)
    }

    /// Everything shutting down does but letting the device go.
    fn release_session(&mut self, shell: &mut S) -> crate::shutdown::CleanupLog {
        use crate::shutdown::{Outcome, Step};
        self.state = AppState::ShuttingDown;
        let mut log = crate::shutdown::CleanupLog::default();

        // What the position reporter produced this session, with its denominator.
        // A driven run has to be able to say how many position reports actually left the client,
        // because replay anchors cannot see emission at all and
        // "it looked like it worked" is not a count.
        let p = self.position.stats;
        tracing::debug!(
            "position reporter -- {} x 0xF753, {} x 0xF61C, {} gated, \
             {} encode failures, over {} frame(s)",
            p.position_events,
            p.movement_events,
            p.position_events_gated,
            p.encode_failures,
            self.position_use_times()
        );

        // Detach UI preferences before chaining to the base cleanup.
        log.push(Step::DetachUiPreferences, Outcome::NotInThisBuild);
        // Save the key map when it was loaded and its configured filename
        // is non-empty. It is what makes a rebound key survive the session:
        // keymap serialization writes the **full merged map**, defaults
        // included, and the key-map merge reads it back first on the next run so it wins.
        //
        // `Outcome::Nothing` is the client's own skip when that filename is empty — a `Config`
        // with no preferences file, which is every test that does not ask for one.
        log.push(Step::SaveKeyMap, shell.save_bindings());

        // 1. Disconnect the active session and release its network client.
        //
        // **This step is both halves, not `Session::log_off` alone.** `Session::log_off` sends
        // `0xF653`, a *message* about a *character*, while transport log-off sends
        // a *packet* about the *connection*. The retail client does both, in that order
        // (epilogue-screen construction logs off a selected character before the
        // process exit disconnects from the server either way). Without the transport half an
        // EXIT from **character select**, where `Flow::log_off` emits nothing because no character
        // is selected, leaves without a word and ACE falls back on its 60-second timeout.
        //
        // The count is reported rather than assumed: `Outcome::Ran` means a datagram the OS
        // accepted, and a goodbye that was built and refused reads as `Nothing` with a line saying
        // so. The client-net disconnect tests pin the bytes.
        log.push(
            Step::Disconnect,
            match self.link.as_mut() {
                Some(link) => {
                    link.net.session.log_off();
                    let (sent, queued) = link.log_off_server();
                    if sent == 0 && queued > 0 {
                        tracing::warn!(
                            "the Disconnect was built for {queued} connection(s) and \
                             none of them was written to the socket"
                        );
                        Outcome::Nothing
                    } else {
                        tracing::info!("0x8000 Disconnect sent to {sent} connection(s)");
                        Outcome::Ran
                    }
                }
                None => Outcome::Nothing,
            },
        );
        // 2. Communication cleanup — chat and Turbine Chat, neither of which exists.
        log.push(Step::CommunicationSystem, Outcome::NotInThisBuild);
        // 3. Network cleanup: the packet controller, the client network object, and the twelve
        //    queues.
        log.push(
            Step::CleanupNet,
            if self.link.take().is_some() {
                Outcome::Ran
            } else {
                Outcome::Nothing
            },
        );
        // 4. Language-information shutdown.
        log.push(Step::LanguageInfo, Outcome::NotInThisBuild);
        // 5. UI cleanup: the flow and every root element it holds, then the element manager.
        //    **Before the database**, because UI elements hold dat objects.
        shell.cleanup_ui(&mut UiContext::new(self));
        log.push(Step::CleanupUi, Outcome::Ran);
        // 6. Preference cleanup, whose last two operations are
        //    save preferences, then run the remaining preference cleanup.
        //
        //    Writing the profile back is retail, not a liberty with a retail install's
        //    `UserPreferences.ini`. Both client layers perform the same preference cleanup,
        //    whose whole body unregisters three preferences, performs
        //    IME preference cleanup, **saves preferences**,
        //    and remaining preference cleanup — so retail writes the file on every normal exit, and
        //    a build that did not would lose every option the player changed during the session.
        //    (Contrast saving the screen layout, which retail really does *not* call at shutdown;
        //    that one would be a deviation.)
        //
        //    `store::save_into` is a **merge**, because the save's one write call is
        //    a Win32 profile write: the file is read first and every key this build does not
        //    register — `Net.*`, `Input.KeymapFile`, anything a later client added — survives
        //    untouched.
        log.push(Step::CleanupPreferences, {
            let path = self.cfg.preferences_file.clone();
            if crate::config::save_user_preferences(&path, |ini| self.save_window_size(ini)) {
                Outcome::Ran
            } else {
                Outcome::Nothing
            }
        });
        // 7-9. The quality registrar, the global event handler and the interface system.
        log.push(Step::DeleteGlobals, Outcome::NotInThisBuild);
        // 10. Clean up the database, closing every data-file controller. The store is
        //     an `Arc` and other holders are gone by now; dropping the app's reference is the close.
        log.push(Step::CleanupDatabase, Outcome::Ran);
        // 11. Clean up the sound manager.
        let audio = self.audio.take();
        log.push(
            Step::SoundManager,
            if audio.is_some() {
                Outcome::Ran
            } else {
                Outcome::Nothing
            },
        );
        drop(audio);
        // Releasing the client destroys the game client and then the client. The single-instance semaphore has no
        // counterpart: this build takes no single-instance lock, which is why two of them can run (client
        // divergence CD-008).
        log.push(Step::ReleaseClient, Outcome::NotInThisBuild);
        // Last, and not the client's: every release above must be observed before the device goes.
        log.push(
            Step::WaitIdle,
            match self.present.wait_idle() {
                Ok(()) => Outcome::Ran,
                Err(e) => {
                    tracing::warn!("wait_idle failed: {e}");
                    Outcome::Nothing
                }
            },
        );
        log
    }

    /// Write [`crate::config::DISPLAY_WINDOW_SIZE`] into the profile being saved: the window's last
    /// windowed size, once it differs from the resolution or the key is there already; with where
    /// it stood windowed and whether the window system had made it full screen.
    pub(super) fn save_window_size(
        &self,
        ini: &mut pyreal_client_contract::persist::preferences::UserPreferences,
    ) {
        if self.window.player_sizes_window() {
            // The window's own rectangle answers where it stood before its own full screen.
            let windowed = if self.applied_full_screen {
                self.windowed_rect_before_full_screen
            } else {
                self.window.window_rect()
            };
            crate::config::write_window_place(
                ini,
                self.window.full_screen_of_its_own(),
                windowed.map(|r| self.window.kept_position((r.left, r.top))),
            );
        }
        let Some((w, h)) = self.windowed_size else {
            return;
        };
        let resolution = self.cfg.display.resolution;
        let key = crate::config::DISPLAY_WINDOW_SIZE;
        if (w, h) != (resolution >> 16, resolution & 0xFFFF) || ini.get(key).is_some() {
            let (section, name) = key.split_once('.').unwrap_or(("Display", key));
            ini.write_profile_string(section, name, &format!("{w}x{h}"));
        }
    }
}
