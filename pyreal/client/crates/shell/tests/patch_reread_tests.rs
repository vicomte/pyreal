//! After a data patch, the interface draws the patched records: an icon patched through the
//! data-patch path is the one the Horizon interface's art reads, though the interface was built
//! over the files before the patch.

use super::*;
use crate::{app::CoreApp, platform::host::NullHost};
use pyreal_dat::overlay::OverlayDir;
use pyreal_horizon::art::AcArt;
use pyreal_primitives::DataId;
use pyreal_protocol::admin::DddData;

/// The record patched: the vitae icon.
const PATCHED: u32 = pyreal_horizon::ui::panels::info::VITAE_ICON;
/// The record whose picture it is patched to: another status icon.
const REPLACEMENT: u32 = pyreal_horizon::ui::panels::info::BURDENED_ICON;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
}

impl Client {
    /// The client over the retail files, its overlay kept in `overlay` (empty), with the Horizon
    /// interface brought up over the files as they open.
    fn new(state: &std::path::Path, overlay: &std::path::Path) -> Self {
        let cfg = pyreal_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: pyreal_dat::testing::dat_dir(),
            overlay_dat_dir: Some(overlay.to_path_buf()),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(pyreal_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
        )
        .expect("the retail files and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        let mut c = Self { app, shell };
        c.frame();
        c
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    /// The pixels of icon `did` as the interface's art reads it now.
    fn icon(&self, did: u32) -> std::sync::Arc<pyreal_horizon::art::Image> {
        let art = &self.shell.horizon.ui.as_ref().expect("Horizon").ui.art;
        let sprite = art.ac_icon(did).expect("the icon decodes");
        art.image(sprite.tex).expect("its pixels")
    }

    /// Patch `id` of the portal to `payload` through the data-patch path, and end the patch as
    /// `DDD_EndDDD` does.
    fn patch(&mut self, id: u32, payload: &[u8]) {
        let iteration = self
            .app
            .store
            .portal()
            .entry(DataId(id))
            .map_or(0, |e| e.iteration)
            + 1;
        let m = DddData {
            dat_file_type: 0,
            dat_file_id: 1,
            resource_type: 0x0C,
            resource_id: id,
            iteration,
            compressed: 0,
            version: 1,
            data_size: u32::try_from(payload.len()).expect("small") + 4,
            data: payload.to_vec(),
        };
        let (outcome, _) = self.app.ddd.on_data(&m);
        assert!(outcome.wrote(), "the patch is written: {outcome:?}");
        self.app.ddd_invalidation = Some(self.app.ddd.on_end());
        self.app.invalidate_after_ddd();
    }
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_horizon_interface_draws_an_icon_patched_after_it_was_built() {
    let state =
        pyreal_dat::testing::ScratchDir::new("patch-reread-state").expect("a scratch directory");
    let overlay = pyreal_dat::testing::ScratchDir::new("patch-reread").expect("a scratch folder");
    let overlay = overlay.path().join("overlay");
    OverlayDir::new(&overlay).expect("an overlay folder");
    let mut c = Client::new(state.path(), &overlay);

    let before = c.icon(PATCHED);
    // The replacement's picture, as a record of the patched id: a picture record names itself.
    let mut replacement = c
        .app
        .store
        .portal()
        .read(DataId(REPLACEMENT))
        .expect("the replacement record");
    replacement[..4].copy_from_slice(&PATCHED.to_le_bytes());
    let want = c.icon(REPLACEMENT);
    assert!(*before != *want, "the two pictures differ");

    let generation = c.app.store_generation();
    let assets = std::sync::Arc::clone(&c.app.anim_assets);
    c.patch(PATCHED, &replacement);
    assert_eq!(
        c.app.store_generation(),
        generation + 1,
        "the files reopened"
    );
    assert!(
        c.app.store.portal().read(DataId(PATCHED)).expect("reads") == replacement,
        "the reopened files read the patched record"
    );
    assert!(
        !std::sync::Arc::ptr_eq(&assets, &c.app.anim_assets),
        "the preview spaces' assets are built over the reopened files"
    );

    c.frame();
    let after = c.icon(PATCHED);
    assert!(
        *after == *want,
        "the interface's art reads the patched record, not the one it decoded before"
    );
    let art = &c.shell.horizon.ui.as_ref().expect("Horizon").ui.art;
    assert!(
        art.ac_data()
            .is_some_and(|files| std::sync::Arc::ptr_eq(&files, &c.app.store)),
        "the art reads the reopened files"
    );
}
