//! Explore mode carried out ([`crate::explore`]): the world built round a place with no server and
//! no body, the mode's keys, what its overlay asks, and its end.

use super::*;

use crate::explore::{Ask, Explore, ExploreStart, ExploreView, Key};

impl<S: Shell> App<S> {
    /// Whether explore mode is up.
    #[must_use]
    pub fn exploring(&self) -> bool {
        self.explore.is_some()
    }

    /// Fly through `start`'s data files with no server: a session up is ended first, the files
    /// opened as installed, and the world built round the place with the free camera standing
    /// there.
    ///
    /// # Errors
    /// [`StartupError::DataFiles`] for files that do not open, [`StartupError::Device`] when the
    /// world cannot be built there.
    pub fn begin_explore(
        &mut self,
        shell: &mut S,
        start: &ExploreStart,
    ) -> Result<(), StartupError> {
        if self.link.is_some() {
            self.end_session(shell);
        }
        self.end_explore(shell);
        let defaults = crate::config::Config::default();
        let mut cfg = self.cfg.clone();
        cfg.era = start.era;
        cfg.world_base = defaults.world_base;
        cfg.era_features = defaults.era_features;
        cfg.world_rules = defaults.world_rules;
        // No server, so no world's overlay: the files as installed.
        cfg.connect = false;
        let files_change = cfg.dat_dir != start.dat_dir
            || cfg.classic_dat_dir != start.classic_dat_dir
            || crate::assets::world_set(&cfg) != crate::assets::world_set(&self.cfg)
            || crate::world_overlay::laid_world(&self.store).is_some();
        cfg.dat_dir.clone_from(&start.dat_dir);
        cfg.classic_dat_dir.clone_from(&start.classic_dat_dir);
        if files_change {
            let (store, _) = crate::assets::open_store_noting(&cfg)?;
            self.cfg = cfg;
            self.base_store = None;
            self.adopt_store(std::sync::Arc::new(store));
            self.apply_world_facts();
            self.reload_store_tables();
        } else {
            self.cfg = cfg;
            self.apply_world_facts();
        }
        let before = self.scene_config;
        let mut scene = before.unwrap_or_else(|| self.cfg.scene_config());
        scene.character = false;
        scene.start_cell = None;
        scene.landblock = crate::explore::start_block(start.place.as_ref());
        self.pending_scene = None;
        self.load_static_scene(scene)?;
        self.scene_before_explore = before;
        if let (Some(place), Some(ws)) = (start.place, self.world.as_mut()) {
            if let Some(block) = ws.viewer_block() {
                ws.camera = place.to_render(block);
            }
        }
        self.input = CameraInput::default();
        self.explore = Some(Explore::new(self.hud.era.era, start.place));
        tracing::info!(
            "exploring {} from {}",
            self.cfg.dat_dir.display(),
            start
                .place
                .map_or_else(|| "Holtburg".to_owned(), |p| format!("{:#06X}", p.block))
        );
        Ok(())
    }

    /// End explore mode, if it is up: the world goes, and the scene the next session builds is the
    /// one it was before.
    pub fn end_explore(&mut self, _shell: &mut S) {
        if self.explore.take().is_none() {
            return;
        }
        self.input = CameraInput::default();
        self.reset_world_view();
        self.scene_config = self.scene_before_explore.take();
        self.pending_scene = self.scene_config;
        tracing::info!("explore mode ended");
    }

    /// A key of explore mode's, held (`down`) or let go.
    pub fn explore_key(&mut self, key: Key, down: bool) {
        let Some(e) = self.explore.as_mut() else {
            return;
        };
        let i = &mut self.input;
        match key {
            Key::Forward => i.forward = down,
            Key::Back => i.back = down,
            Key::Left => i.left = down,
            Key::Right => i.right = down,
            Key::Up => i.up = down,
            Key::Down => i.down = down,
            Key::Faster if down => e.change_speed(1),
            Key::Slower if down => e.change_speed(-1),
            Key::CopyPlace | Key::CopyReport if down => e.want_copy(key),
            _ => {}
        }
    }

    /// Carry out what explore mode's overlay asked. Leaving is the front end's: it ends the mode.
    pub fn explore_ask(&mut self, ask: Ask) {
        let Some(e) = self.explore.as_mut() else {
            return;
        };
        let now = self.timer.cur_time;
        match ask {
            Ask::Speed(steps) => e.change_speed(steps),
            Ask::Time(fraction) => {
                if let Some(ws) = self.world.as_mut() {
                    let set = pyreal_client_model::cmd::tod::TimeOfDayCommand::Set(fraction);
                    let _ = crate::time_of_day::apply(ws, set, now);
                }
            }
            Ask::Weather(w) => {
                if let Some(ws) = self.world.as_mut() {
                    let set = pyreal_client_model::cmd::weather::WeatherCommand::Set(w);
                    let _ = crate::weather::apply(ws, set);
                }
            }
            Ask::Fog(level) => {
                use pyreal_client_contract::options::names::fidelity::FOG_DENSITY;
                let value = pyreal_client_contract::PrefValue::Int(level);
                let _ =
                    pyreal_client_contract::options::store::set_value(FOG_DENSITY, value.clone());
                self.scripted_preferences.push((FOG_DENSITY, value));
            }
            Ask::Leave => {}
        }
    }

    /// What explore mode's overlay shows, while it is up.
    #[must_use]
    pub fn explore_view(&self) -> Option<ExploreView> {
        let e = self.explore.as_ref()?;
        let ws = self.world.as_ref();
        let place = ws.and_then(crate::explore::camera_place);
        let fog = match pyreal_client_contract::options::store::inq_value(
            pyreal_client_contract::options::names::fidelity::FOG_DENSITY,
        ) {
            Some(pyreal_client_contract::PrefValue::Int(v)) => v,
            _ => pyreal_client_contract::options::fog_density::DEFAULT,
        };
        Some(ExploreView {
            era: e.era,
            coordinates: place
                .as_ref()
                .and_then(|p| crate::explore::coordinates(p.cell)),
            place: place
                .as_ref()
                .map(crate::report::loc_text)
                .unwrap_or_default(),
            height: e.height(),
            speed: e.speed(),
            time: ws.map_or(0.0, |w| w.clock.present_time_of_day),
            time_name: ws
                .and_then(|w| w.clock.time_of_day_name())
                .unwrap_or_default()
                .to_owned(),
            weather: ws.map_or_else(Default::default, |w| w.weather),
            fog,
            copied: e.copied(self.timer.cur_time).map(str::to_owned),
        })
    }

    /// The frame's part before the world moves: how fast the camera flies.
    pub(super) fn explore_before_update(&mut self) {
        if let (Some(e), Some(ws)) = (self.explore.as_ref(), self.world.as_mut()) {
            ws.flight_speed = e.speed();
        }
    }

    /// The frame's part after the world moves: the camera kept inside the world and above its
    /// ground.
    pub(super) fn explore_after_update(&mut self) {
        if let (Some(e), Some(ws)) = (self.explore.as_mut(), self.world.as_mut()) {
            e.hold_over_ground(ws, &self.store);
        }
    }

    /// The place or the report line asked for, copied.
    pub(super) fn explore_copy_use_time(&mut self, shell: &mut S) {
        let Some(key) = self.explore.as_mut().and_then(Explore::take_copy) else {
            return;
        };
        let (line, what) = if key == Key::CopyReport {
            (self.visual_report(shell).line(), "The report line")
        } else {
            let place = self.world.as_ref().and_then(crate::explore::camera_place);
            (
                place
                    .as_ref()
                    .map(crate::report::loc_text)
                    .unwrap_or_default(),
                "The place",
            )
        };
        tracing::info!("{line}");
        let copied = !line.is_empty() && shell.copy_text(&line);
        if let Some(e) = self.explore.as_mut() {
            let note = if copied {
                format!("{what} is copied.")
            } else {
                format!("{what} could not be copied.")
            };
            e.note_copied(&note, self.timer.cur_time);
        }
    }
}
