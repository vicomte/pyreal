//! The documented shutdown order and the character-error string tokens: teardown runs in order and
//! a changed preference is written back on the way out; every character-error token is a row of the
//! shipped string table.
//!
//! Fixture: the retail dats; offline headless Apps on a software device.
//!
//! | claim | oracle |
//! |---|---|
//! | the character-error → string-token table | the 25-code mapping in `character_error_string_id`, resolved against shipped string table `0x23000002` |
//! | the teardown order | [`pyreal_client_runtime::shutdown::Step::ORDER`], the emitted cleanup log, and three explicitly load-bearing orderings |

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use pyreal_client::app::App;
use pyreal_client_contract::disconnect;
use pyreal_client_contract::options::store;
use pyreal_client_contract::PrefValue;
use pyreal_client_runtime::config::Config;
use pyreal_primitives::DataId;
use {pyreal_client_runtime::shutdown::Outcome, pyreal_client_runtime::shutdown::Step};

/// Missing dats fail the test; they never skip it.
fn have_dats() {
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set PYREAL_TEST_DAT_DIR",
        client_dir().display()
    );
}

/// The retail store, or a failed test.
fn store() -> std::sync::Arc<pyreal_dat::RetailDatStore> {
    crate::common::dats()
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// An offline `App` with its shell up and the first-pixel scene loaded, settled a few frames.
fn app_up(cfg: Config) -> App {
    let mut app = App::new(cfg)
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell must come up over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first-pixel scene must load: {e}"));
    for _ in 0..4 {
        app.frame();
    }
    app
}

// ---------------------------------------------------------------------------------------------
// 1. The character-error tokens
// ---------------------------------------------------------------------------------------------

/// Oracle: the 25-code character-error mapping against shipped string table `0x23000002`.
///
/// Four codes (0 `UNDEF`, 2 `LOGGED_ON`, 7 `NO_PREMADE`, 22 `CHARACTER_IS_BOOTED`) "fall into the
/// `default:` arm of the switch, which leaves the `StringInfo` untouched and **never queues a
/// mode**", and codes 4 and 8 share one token. Every other token must be a real row.
#[test]
fn every_documented_character_error_token_is_a_row_of_the_shipped_string_table() {
    let store = store();
    // The shipped table's concrete DataId `0x23000002`; this does not test enum indirection.
    let table = {
        use pyreal_assets::Decode as _;
        use pyreal_primitives::AssetSource as _;
        let id = DataId(0x2300_0002);
        let bytes = store.read(id).expect("string table 0x23000002");
        pyreal_assets::ui::StringTable::decode_payload(id, &bytes).expect("it decodes")
    };
    let resolve = |name: &str| {
        let hash = pyreal_primitives::num::hash::str_hash(name.as_bytes());
        table
            .strings
            .iter()
            .find(|(k, _)| *k == hash)
            .and_then(|(_, e)| e.strings.first().cloned())
    };
    assert_eq!(
        resolve(disconnect::SERVER_DIED_STRING_ID).as_deref(),
        Some("The connection to the server has been lost!"),
        "the server-died notice handler's own string"
    );
    let mut named = 0;
    for code in 0..=24u32 {
        let Some(token) = disconnect::character_error_string_id(code) else {
            continue;
        };
        named += 1;
        let text = resolve(token);
        // **A shipped data bug.** All twenty tokens
        // exist in the original client's string inventory, but English table `0x23000002` has
        // **no row** for `ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER` — and the row for
        // `ID_CHAR_ERROR_ENTER_GAME_GENERIC` holds, as its *text*, the literal string
        // `"ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER"`. The localiser pasted the key where the value
        // belonged and lost the row. So error 17 shows nothing and error 11 shows a
        // symbol name. Reproduced rather than repaired: it is what the retail client displays.
        if code == 17 {
            assert!(
                text.is_none(),
                "character error 17's row is missing from the shipped table"
            );
            continue;
        }
        assert!(
            text.is_some(),
            "character error {code} names {token}, which is not in the table"
        );
        let text = text.unwrap();
        assert!(!text.is_empty(), "character error {code}: {token} is blank");
        if code == 11 {
            assert_eq!(
                text, "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER",
                "the shipped paste error"
            );
        }
    }
    assert_eq!(named, 21, "21 of the 25 codes carry a token");
    for silent in [0u32, 2, 7, 22] {
        assert_eq!(disconnect::character_error_string_id(silent), None);
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The shutdown order
// ---------------------------------------------------------------------------------------------

///
/// Oracle: [`Step::ORDER`] names 13 recorded units: two gameplay-client steps, nine enum entries
/// covering the original eleven shared cleanup roles, client release, and the final explicit
/// device-idle wait. This test compares a real teardown log to that sequence and
/// separately asserts three load-bearing relationships: network before UI, UI before database, and
/// database before the idle wait.
///
/// The outcome checks below are deliberately limited to this offline setup: UI, database, and idle
/// work run, while no live link exists for disconnect or network cleanup.
#[test]
fn the_teardown_runs_the_documented_order() {
    let _gpu = gpu_lock();
    have_dats();
    let app = app_up(base_config());
    let log = app.shutdown();
    for (s, o) in &log.0 {
        eprintln!("cleanup {s:?}: {o:?}");
    }
    assert_eq!(
        log.order(),
        Step::ORDER.to_vec(),
        "the teardown ran the documented order"
    );

    let at = |s: Step| {
        log.order()
            .iter()
            .position(|x| *x == s)
            .expect("in the log")
    };
    assert!(
        at(Step::CleanupNet) < at(Step::CleanupUi),
        "net before UI: the world controller holds network references"
    );
    assert!(
        at(Step::CleanupUi) < at(Step::CleanupDatabase),
        "UI elements hold dat objects"
    );
    assert!(at(Step::CleanupDatabase) < at(Step::WaitIdle));

    // What this build actually has a counterpart for. A step named `NotInThisBuild` is a gap that
    // is *reported*; a step that ran must have had something to do.
    assert_eq!(
        log.outcome(Step::CleanupUi),
        Some(Outcome::Ran),
        "the UI was up, so it came down"
    );
    assert_eq!(log.outcome(Step::CleanupDatabase), Some(Outcome::Ran));
    assert_eq!(log.outcome(Step::WaitIdle), Some(Outcome::Ran));
    // Offline: there is no live link, so these two network steps are `Nothing` rather than `Ran`.
    assert_eq!(log.outcome(Step::Disconnect), Some(Outcome::Nothing));
    assert_eq!(log.outcome(Step::CleanupNet), Some(Outcome::Nothing));
}

/// **A setting the player changed survives the quit.**
///
/// Normal-exit preference cleanup ends by saving the registered preferences.
///
/// The observable is the **file on disk after a real `App::shutdown`**, not a call to `save_into`:
/// a test that calls the writer itself cannot see a missing caller.
///
/// No datagram leaves this process. The profile is written into a temporary directory of this
/// test's own making and removed again; the installation's `UserPreferences.ini` is never opened.
#[test]
fn a_changed_preference_is_written_back_on_shutdown() {
    use pyreal_client_contract::options::fidelity;
    let _gpu = gpu_lock();
    have_dats();

    // A path of this test's own, beside the target directory rather than in the install.
    let dir = std::env::temp_dir().join(format!("dere-g19-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("UserPreferences.ini");
    let _ = std::fs::remove_file(&path);

    let cfg = Config {
        preferences_file: path.clone(),
        ..base_config()
    };
    let app = app_up(cfg);

    // The shell registered all 27 supported preferences at their defaults.
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(0)),
        "the registered default is Stereo"
    );
    // Set three representative values directly in the registered store. The option-menu test
    // covers the UI gesture; this test isolates the shutdown flush and does not claim to click
    // those controls.
    store::set_value("Sound.SoundFeatures", PrefValue::Int(1));
    store::set_value("Render.LandscapeDrawDistance", PrefValue::Int(25));
    store::set_value("Misc.TooltipDelay", PrefValue::Float(0.75));
    // And, in a build with the experimental effects, one of their boxes ticked.
    store::set_value(fidelity::OPTIONS[0].name, PrefValue::Bool(true));

    // The options the save may leave out of the profile, with the value each holds at the quit:
    // each graphics box or menu at its default that the profile does not name.
    let at_quit: Vec<(&str, PrefValue)> = pyreal_client_runtime::render_prefs::FidelityFeature::ALL
        .iter()
        .map(|f| f.name())
        .filter_map(|name| store::inq_value(name).map(|v| (name, v)))
        .collect();

    assert!(!path.exists(), "nothing has written it yet");
    let log = app.shutdown();
    assert_eq!(
        log.outcome(Step::CleanupPreferences),
        Some(Outcome::Ran),
        "`CleanupPreferences` must actually save"
    );

    let text = std::fs::read_to_string(&path).expect("the profile was written");
    eprintln!("saved profile:\n{text}");
    // The persisted shape uses one section per category, removes the category prefix from each
    // key, writes enumeration labels, formats floats to two decimal places, and uses `True`/`False`
    // for booleans. The substring checks below cover those representative encodings.
    assert!(text.contains("[Sound]\r\n"), "a section per category");
    assert!(
        text.contains("SoundFeatures=Mono\r\n"),
        "the label, not the number 1"
    );
    assert!(
        text.contains("LandscapeDrawDistance=Extreme\r\n"),
        "25 is `Extreme`, not index 25"
    );
    assert!(text.contains("TooltipDelay=0.75\r\n"), "%.2f");
    assert!(
        text.contains("SoundDisabled=True\r\n"),
        "True/False, and uninverted"
    );
    assert!(
        !text.contains("[Default]"),
        "the saved profile contains no fallback section"
    );

    // Parse and reload the same text into the existing process. Every registered option comes
    // back: the 27 retail attachments and this client's own beside them, each from the profile,
    // or, where the save left it out, as the default it held at the quit. The ticked box and two
    // of the three values set above are then checked explicitly. This is not a fresh `App` restart.
    assert_eq!(
        store::init(),
        pyreal_client_contract::options::preferences::UI_PREFERENCES.len(),
        "every retail variable back to its default"
    );
    let registered = store::len();
    let ini = pyreal_client_contract::persist::preferences::UserPreferences::parse(&text)
        .expect("it parses");
    let left_out: Vec<_> = at_quit
        .iter()
        .filter(|(name, _)| {
            !ini.entries
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(name))
        })
        .collect();
    let (applied, _ignored) = store::load(&ini);
    assert_eq!(
        applied + left_out.len(),
        registered,
        "every registered option came back: {applied} from the profile and {} left out, of \
         {registered}",
        left_out.len()
    );
    for (name, value) in &at_quit {
        assert_eq!(
            store::inq_value(name).as_ref(),
            Some(value),
            "{name} comes back as it was at the quit, written or left out at its default"
        );
    }
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(1)),
        "the setting the player changed survived the quit"
    );
    assert_eq!(
        store::inq_value("Render.LandscapeDrawDistance"),
        Some(PrefValue::Int(25))
    );

    // The whole directory: `Step::SaveKeyMap` writes `<stem>.keymap` beside the profile, so
    // removing the `.ini` alone leaves the directory behind.
    let _ = std::fs::remove_dir_all(&dir);
}
