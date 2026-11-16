//! The hitch log carried out ([`crate::hitch`]): each frame kept as it finishes, and a report
//! written to the settings folder's `logs/hitches` when one is due. A save the player asked for
//! says where it went.

use super::*;

use crate::hitch::{Due, Frame, Kind, View};

/// Seconds the camera's speed is measured over.
const SPEED_SPAN: f64 = 0.25;

impl<S: Shell> App<S> {
    /// The reports' folder.
    #[must_use]
    pub fn hitch_folder(&self) -> std::path::PathBuf {
        let settings = self
            .cfg
            .preferences_file
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        crate::hitch::folder(&settings)
    }

    /// Save the last thirty seconds of frames with the next frame.
    pub fn save_frame_timings(&mut self) {
        let now = self.hitch_epoch.elapsed().as_secs_f64();
        self.hitches.ask(now);
    }

    /// Open the reports' folder in the system's file browser.
    pub fn open_hitch_folder(&self) {
        let dir = self.hitch_folder();
        if !crate::hitch::open_folder(&dir) {
            tracing::warn!("could not open {}", dir.display());
        }
    }

    /// At the start of a frame: the frame before, `interval` seconds long, kept; and a report
    /// written when one is due.
    pub(super) fn hitch_use_time(&mut self, shell: &mut S, interval: Option<f32>) {
        let now = self.hitch_epoch.elapsed().as_secs_f64();
        let has_world = self.world.is_some();
        if has_world && !self.hitch_world {
            self.hitches.settle(now);
        }
        self.hitch_world = has_world;
        let Some(dt) = interval else {
            return;
        };
        let at = now - f64::from(dt);
        let view = self.hitch_view(at);
        let frame = Frame {
            at,
            ms: dt * 1000.0,
            steps: self.perf_steps.map(|s| s * 1000.0),
            work: self.present.frame_costs().and_then(|c| c.work),
            view,
        };
        if let Some(due) = self.hitches.record(frame) {
            self.write_hitch_report(shell, &due);
        }
    }

    /// Where the camera was at `at`, and how fast it went since the frame before.
    fn hitch_view(&self, at: f64) -> Option<View> {
        let ws = self.world.as_ref()?;
        let global = match ws.character.as_ref() {
            Some(c) => {
                let p = c.position();
                let (bx, by) = crate::report::block_xy(p.cell.landblock().0);
                #[allow(clippy::cast_precision_loss)]
                let g = [
                    bx as f32 * 192.0 + p.frame.origin.x,
                    by as f32 * 192.0 + p.frame.origin.y,
                    p.frame.origin.z,
                ];
                g
            }
            None => {
                let (vx, vy) = ws.viewer_block()?;
                let c = ws.camera.position;
                #[allow(clippy::cast_precision_loss)]
                let g = [vx as f32 * 192.0 + c.x, vy as f32 * 192.0 + c.y, c.z];
                g
            }
        };
        let block = (
            pyreal_primitives::num::floor_to_i32(global[0] / 192.0),
            pyreal_primitives::num::floor_to_i32(global[1] / 192.0),
        );
        #[allow(clippy::cast_precision_loss)]
        let local = [
            global[0] - block.0 as f32 * 192.0,
            global[1] - block.1 as f32 * 192.0,
            global[2],
        ];
        // Over the last quarter second, so one frame's timing does not swing it.
        let (speed, climb) = self
            .hitches
            .frames()
            .rev()
            .filter_map(|f| Some((f.at, f.view?)))
            .take_while(|(was, _)| at - was <= SPEED_SPAN)
            .last()
            .filter(|(was, _)| at > *was)
            .map_or((0.0, 0.0), |(was, v)| {
                let g = v.global();
                let d = ((global[0] - g[0]).powi(2) + (global[1] - g[1]).powi(2)).sqrt();
                #[allow(clippy::cast_possible_truncation)]
                let dt = (at - was) as f32;
                (d / dt, (global[2] - g[2]) / dt)
            });
        Some(View {
            block,
            at: local,
            speed,
            climb,
            flight: self.explore.as_ref().map(crate::explore::Explore::speed),
        })
    }

    /// What a report says the run was: the client, the device, the mode and the frame.
    fn hitch_context(&mut self, shell: &mut S) -> Vec<(&'static str, String)> {
        let render = self.present.render_context(self.world.as_ref());
        let mode = match (&self.explore, &self.world) {
            (Some(e), _) => format!("explore over the {} files", era_words(e.era)),
            (None, Some(_)) => "in a world".to_owned(),
            (None, None) => "the screens before a world".to_owned(),
        };
        let mut context = vec![
            (
                "client",
                format!(
                    "Pyreal {} ({})",
                    env!("CARGO_PKG_VERSION"),
                    self.build_commit
                ),
            ),
            ("mode", mode),
            ("gpu", render.adapter.clone()),
            ("driver", render.driver.clone()),
            ("present mode", render.present_mode.clone()),
            ("better lighting", render.better_lighting.to_string()),
            ("land radius", self.cfg.land_radius.to_string()),
            ("threshold ms", format!("{:.0}", self.hitches.threshold_ms)),
        ];
        if self.world.is_some() {
            context.push(("report", self.visual_report(shell).line()));
        }
        context
    }

    /// Write the report `due` asks for, and say where it went.
    fn write_hitch_report(&mut self, shell: &mut S, due: &Due) {
        let context = self.hitch_context(shell);
        let text = self.hitches.report(due, &context);
        let unix = self
            .clock
            .unix_time()
            .and_then(|d| i64::try_from(d.as_secs()).ok())
            .unwrap_or(0);
        let name = crate::hitch::file_name(due.kind, unix, self.clock.utc_offset_secs(unix));
        let dir = self.hitch_folder();
        let note = match crate::hitch::write(&dir, &name, &text) {
            Ok(path) => {
                tracing::info!("hitch report saved: {}", path.display());
                let file = path
                    .file_name()
                    .map_or_else(|| name.clone(), |n| n.to_string_lossy().into_owned());
                format!("The last 30 seconds of frames were saved to {file}.")
            }
            Err(e) => {
                tracing::warn!("hitch report not saved to {}: {e}", dir.display());
                format!("The frame timings could not be saved: {e}.")
            }
        };
        // A slow frame's report is only logged; a save the player asked for says where it went.
        if due.kind == Kind::Hitch {
            return;
        }
        let now = self.timer.cur_time;
        if let Some(e) = self.explore.as_mut() {
            e.note_copied(&note, now);
        } else if self.world.is_some() {
            self.objects.world.scroll.add_feedback_to_scroll(
                &note,
                pyreal_client_model::chat::text_type::DEFAULT,
                true,
                0,
                pyreal_client_contract::feedback::Feedback::LOCAL,
            );
        }
    }
}

/// The name of the files an era draws, in a sentence.
fn era_words(era: pyreal_primitives::EraId) -> &'static str {
    match era {
        pyreal_primitives::EraId::Launch => "1999",
        pyreal_primitives::EraId::Infiltration => "February 2005",
        pyreal_primitives::EraId::Eor => "End of Retail",
    }
}
