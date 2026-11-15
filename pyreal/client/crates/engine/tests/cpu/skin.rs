//! Posed bodies' clips and their motion words, with no device.

use std::sync::Arc;

use pyreal_art_pack::skin::{sample_body, write_gltf, Slot};
use pyreal_art_pack::ArtPack;
use pyreal_engine::skin::{half_bits, Animator, MotionNow};

/// A pack whose one body is the sample body, its walk and idle the `Sway` clip (two seconds,
/// a metre a stride) and its medium attack `Nod` (one second).
fn sample_pack() -> ArtPack {
    let (json, bin) = write_gltf(&sample_body(0.5, [1.0; 4]), "body.bin");
    let manifest = "[pack]\nname = \"Body\"\nformat = 1\n[[skin]]\nsetup = \"0x02000001\"\n\
        file = \"b/body.gltf\"\nclips = { idle = \"Sway\", walk = \"Sway\", melee_med = \"Nod\" }\n\
        author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    ArtPack::open(manifest, &mut |p| match p {
        "b/body.gltf" => Ok(json.as_bytes().to_vec()),
        "b/body.bin" => Ok(bin.clone()),
        other => Err(format!("no {other}")),
    })
    .expect("opens")
}

fn now(slot: Slot, action: Option<u32>) -> MotionNow {
    MotionNow {
        slot,
        backwards: false,
        rate: 1.0,
        action,
    }
}

/// An idle loops at its own pace, a walk goes round once a stride, an attack plays once and
/// then the idle plays until a new one starts, and a slot with no clip leaves the parts to lead.
#[test]
fn clips_play_as_their_slots_say() {
    let pack = Arc::new(sample_pack());
    let skin = &pack.skins[0];
    let mut a = Animator::default();
    let (sway, t) = a.step(skin, now(Slot::Idle, None), 2.5, 0.0).expect("idle");
    assert_eq!(skin.model.clips[sway].name, "Sway");
    assert!((t - 0.5).abs() < 1e-5, "looped round to {t}");
    // A quarter metre run is a quarter of a two-second clip a metre long, from its start.
    let (_, t) = a
        .step(skin, now(Slot::Run, None), 0.1, 0.25)
        .expect("the run takes the walk's clip");
    assert!((t - 0.5).abs() < 1e-5, "{t}");
    let attack = Some(0x1000_0063);
    let (nod, t) = a
        .step(skin, now(Slot::MeleeMed, attack), 0.4, 0.0)
        .expect("an attack");
    assert_eq!(skin.model.clips[nod].name, "Nod");
    assert!(
        (t - 0.4).abs() < 1e-5,
        "an attack starts from its beginning: {t}"
    );
    let _ = a.step(skin, now(Slot::MeleeMed, attack), 0.7, 0.0);
    let (after, _) = a
        .step(skin, now(Slot::MeleeMed, attack), 0.1, 0.0)
        .expect("played");
    assert_eq!(
        after, sway,
        "a finished attack the motion still names plays the idle"
    );
    assert!(a.step(skin, now(Slot::Sleep, None), 0.1, 0.0).is_none());
}

/// A motion word's halves read back as the signed distances written.
#[test]
fn half_bits_keep_their_sign() {
    for v in [0.0f32, 0.5, -0.5, 0.0123, -3.25] {
        let h = half_bits(v);
        let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
        let exp = i32::try_from((h >> 10) & 0x1F).unwrap();
        #[allow(clippy::cast_precision_loss)]
        let frac = (h & 0x3FF) as f32;
        let back = if exp == 0 {
            0.0
        } else {
            sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15)
        };
        assert!(
            (back - v).abs() <= v.abs() * 2e-3 + 1e-6,
            "{v} came back {back}"
        );
    }
}
