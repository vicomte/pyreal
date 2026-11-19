//! Sound reaches the device buffer audibly: the startup UI sound is measured on the block the
//! resampler hands the endpoint (after the 11 025 -> 48 000 conversion), against absolute levels
//! rather than values the test stored.
//!
//! Fixture: the retail UI sound table and waves. The output stream cannot be opened on a headless
//! runner, so the resampler is driven with the same closure the device start installs; the device
//! open itself is not exercised here.

use pyreal_dat::RetailDatStore;
use pyreal_primitives::num::math;
use pyreal_primitives::DataId;
use std::sync::Arc;
use {
    pyreal_client_runtime::audio::Audio, pyreal_client_runtime::audio::Resampler,
    pyreal_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
    pyreal_client_runtime::audio::UI_SOUND_TABLE_ENUM,
    pyreal_client_runtime::audio::UI_SOUND_TABLE_GROUP,
};

/// The rate this machine's endpoint actually reported. Hard-coded rather than probed because a
/// test must not depend on the sound card it happens to run beside; 48 kHz is the shared-mode
/// default on every Windows this rebuild targets, and it is the worst case for the resampler
/// (a non-integer 11025 → 48000 ratio, unlike the 44 100 the unit tests use).
const DEVICE_RATE: u32 = 48_000;

/// One second of device audio, interleaved stereo.
const DEVICE_BLOCK: usize = (DEVICE_RATE as usize) * 2;

/// The level below which a sound effect is, in practice, not heard over a desktop's noise floor.
///
/// −40 dBFS. This is deliberately **far** below the level a correct UI sound reaches (0 dB of
/// attenuation, i.e. the wave's own amplitude) so that the test fails only on a real defect and
/// not on a quiet wave. The attenuation calculation returns `None` — *drop the
/// sound entirely* — below `VOL_MIN_DB`, so the client itself agrees there is a floor under which
/// playing is pointless.
const AUDIBLE: f32 = 0.01;

fn store() -> Arc<RetailDatStore> {
    Arc::new(pyreal_dat::testing::open_store_or_fail())
}

/// A silent `Audio` with the seed pinned, so the probability roll is reproducible.
fn audio() -> Audio {
    Audio::new(pyreal_audio::Prefs::default(), 1, false)
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// Load the UI sound table exactly as `App::start_shell` does, and return it with its rows resident.
fn ui_table(audio: &mut Audio, store: &Arc<RetailDatStore>) -> DataId {
    let table = pyreal_client_runtime::assets::enum_did(
        &**store,
        UI_SOUND_TABLE_GROUP,
        UI_SOUND_TABLE_ENUM,
    )
    .expect("the UI sound table resolves by enum");
    assert!(
        audio.load_sound_table(store, table),
        "the UI sound table loads"
    );
    let rows: Vec<DataId> = audio
        .assets()
        .table(table)
        .and_then(|t| pyreal_audio::table::lookup(t, SOUND_UI_BUTTON_PRESS))
        .expect("the table has the button-press sound")
        .iter()
        .map(|r| r.sound_id)
        .collect();
    assert!(
        !rows.is_empty(),
        "the button-press sound names at least one wave"
    );
    for id in rows {
        audio.create_sound(store, id);
    }
    table
}

/// Drive the resampler with the **same closure `Audio::start_device` installs**, and return the
/// peak of everything it would have submitted to the endpoint.
///
/// `blocks` is in 1/10ths of a second of device time, which is roughly what a `cpal` callback
/// asks for and is long enough for a UI blip to be wholly contained.
fn submitted_peak(audio: &Audio, blocks: usize) -> f32 {
    let mut r = Resampler::new(pyreal_audio::MIX_RATE, DEVICE_RATE, 2);
    let mut out = vec![0.0f32; DEVICE_BLOCK / 10];
    let mut worst = 0.0f32;
    for _ in 0..blocks {
        out.fill(0.0);
        r.fill(&mut out, |src| audio.mix(src));
        worst = worst.max(peak(&out));
    }
    worst
}

// ---------------------------------------------------------------------------------------------
// Station 1 — the sound the client plays without being asked.
// ---------------------------------------------------------------------------------------------

/// **The button-press sound, the one sound `main` plays on every launch, is audible at the
/// device.**
///
/// `App::start_shell` resolves it and `main.rs` plays it before the frame loop, so this is the
/// first thing a player hears and the cheapest proof that the chain works. The assertion is on the
/// resampled block, at an absolute level.
#[test]
fn the_startup_ui_sound_reaches_the_device_boundary_audibly() {
    let store = store();
    let mut a = audio();
    let table = ui_table(&mut a, &store);

    a.play_ui_sound(pyreal_audio::UiSoundRef::Table {
        table,
        stype: SOUND_UI_BUTTON_PRESS,
    });
    assert!(a.active_voices() > 0, "the UI sound started a voice");

    let p = submitted_peak(&a, 20);
    assert!(
        p >= AUDIBLE,
        "the startup UI sound reaches the endpoint at peak {p:.5}, which is below the {AUDIBLE} \
         floor -- nothing would be heard"
    );
}

/// **Each of a storm's six peals of thunder is audible at the device.** They are the UI table's
/// thunder rows, the ones `/environ 118..123` play, which startup makes resident with the rest.
#[test]
fn every_peal_of_a_storms_thunder_reaches_the_device_audibly() {
    let store = store();
    for peal in 0..6 {
        let mut a = audio();
        let table = ui_table(&mut a, &store);
        a.create_table_waves(&store, table);
        a.play_thunder(table, peal, 1.0);
        assert!(a.active_voices() > 0, "peal {peal} started a voice");
        let p = submitted_peak(&a, 20);
        assert!(
            p >= AUDIBLE,
            "peal {peal} reaches the endpoint at peak {p:.5}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Station 2 — the resampler is not the thing that eats it.
// ---------------------------------------------------------------------------------------------

/// **The 11 025 → 48 000 conversion preserves amplitude.**
///
/// Mixer tests measure `AudioSystem::mix`, upstream of the resampler. If the conversion
/// attenuated -- a mis-scaled interpolation weight, a block whose tail is never filled -- they
/// would stay green and nothing would be heard. A full-scale square wave in must come out
/// full-scale.
#[test]
fn the_device_rate_conversion_does_not_attenuate() {
    let mut r = Resampler::new(pyreal_audio::MIX_RATE, DEVICE_RATE, 2);
    let mut out = vec![0.0f32; DEVICE_BLOCK / 10];
    let mut worst = 0.0f32;
    for _ in 0..10 {
        out.fill(0.0);
        r.fill(&mut out, |src| src.fill(1.0));
        worst = worst.max(peak(&out));
    }
    assert!(
        worst >= 0.99,
        "a full-scale input came out of the 11025 -> {DEVICE_RATE} conversion at {worst:.5}"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 3 — the level the endpoint gets is the wave's own, and the client adds no loss.
// ---------------------------------------------------------------------------------------------

/// **A UI sound arrives at the endpoint at the amplitude the dat holds, to within the resampler.**
///
/// A quiet button click is a property of the shipped data, not a defect. The button-press sound
/// is one row —
/// `sound_id 0x0A0003B3, priority 0.9, probability 1.0, **volume 0.1**` — and entry 7
/// passes `row.volume_` as the volume, so attenuation is `ceil(20*log10(0.1))` = **-20 dB** and
/// `gain_from_db(-20)` is 0.1. Retail's own button click was 20 dB down; this one is too, and
/// that is correct rather than a bug to fix.
///
/// The assertion is therefore on the *formula*, against two values read out of the dat — the
/// wave's own peak and the row's own volume — and not against a number this test chose. It still
/// catches the gain chain losing level for any other reason, which is what it is here for.
#[test]
fn the_endpoint_receives_the_waves_amplitude_times_the_rows_own_volume() {
    let store = store();
    let mut a = audio();
    let table = ui_table(&mut a, &store);

    let rows = a
        .assets()
        .table(table)
        .and_then(|t| pyreal_audio::table::lookup(t, SOUND_UI_BUTTON_PRESS))
        .expect("the table has the button-press sound")
        .to_vec();
    // Retail's attenuation lookup quantises to whole decibels, rounded up, so the expectation carries the
    // same rounding rather than the raw product.
    //
    // **`6.0206`, not `20*log10`.** The client's dB conversion is `log2(x) * 6.0206`, and that
    // factor is a *rounded*
    // copy of `20*log10(2) = 6.020599913...`, so the curve is fractionally steeper than the
    // textbook one. It matters here and only just: `log2(0.1) * 6.0206` is `-20.0006`, which
    // ceils to **-20**, while the textbook `20*log10(0.1)` is `-19.999999999999996`, which ceils
    // to -19 and predicts a sound 1 dB too loud. This constant is the oracle, written out
    // independently rather than called out of the code under test.
    const DB_PER_OCTAVE: f64 = 6.0206;
    // A gain of a few decibels either side of one: an f32 holds it.
    #[allow(clippy::cast_possible_truncation)]
    let expected: Vec<f32> = rows
        .iter()
        .map(|r| {
            let s = pyreal_audio::AudioAssets::sample(a.assets(), r.sound_id)
                .expect("the row's wave decodes");
            let db = (math::log2(f64::from(r.volume)) * DB_PER_OCTAVE).ceil();
            peak(&s.frames) * (math::pow(10f64, db / 20.0) as f32)
        })
        .collect();

    a.play_ui_sound(pyreal_audio::UiSoundRef::Table {
        table,
        stype: SOUND_UI_BUTTON_PRESS,
    });
    let submitted = submitted_peak(&a, 20);

    // 3% covers the linear interpolation between two source frames, which cannot overshoot and
    // can undershoot a single-sample peak by less than the step.
    let matched = expected
        .iter()
        .any(|&w| (submitted - w).abs() <= 0.03 * w.max(1e-6));
    assert!(
        matched,
        "the endpoint got peak {submitted:.5}; the dat's rows {rows:?} predict {expected:?} \
         -- the client is losing level between the dat and the device"
    );
}
