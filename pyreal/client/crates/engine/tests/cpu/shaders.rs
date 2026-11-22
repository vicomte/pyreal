//! Every shader module parses and validates, without a device.

use pyreal_engine::materials::MAX_TEXTURES;
use pyreal_engine::renderer::source;

fn validate(name: &str, wgsl: &str) {
    let module = naga::front::wgsl::parse_str(wgsl)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(wgsl)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
}

#[test]
fn every_module_validates() {
    validate("cull", &source::cull());
    validate("gbuffer", &source::gbuffer());
    validate("forward", &source::forward());
    validate("lighting", &source::lighting());
    validate("tonemap", &source::tonemap());
    validate("exposure", &source::exposure());
    validate("bloom", &source::bloom());
    validate("air_tables", &source::air_tables());
    validate("shadow_cast", &source::shadow_cast());
    validate("clusters", &source::clusters());
    validate("lamp_rays", &source::lamp_rays());
    validate("gtao", &source::gtao());
    validate("clouds", &source::clouds());
    validate("probes", &source::probes());
    validate("far", &source::far());
    validate("hiz_first", &source::hiz_first());
    validate("hiz_down", &source::hiz_down());
    validate("water", &source::water());
    validate("water merge", &source::water_merge());
    validate("water_sim", &source::water_sim());
    validate("cover", &source::cover());
    validate("cover_draw", &source::cover_draw());
    validate("oit", &source::oit());
    validate("resolve", &pyreal_engine::temporal::resolve_source());
    validate("composite", &pyreal_engine::temporal::composite_source());
    validate("ui blur", pyreal_engine::frost::source());
    let [overlay, whole] = pyreal_engine::overlay::sources();
    validate("overlay", overlay);
    validate("whole picture", whole);
    validate("preview", &pyreal_engine::preview::source());
    validate("air froxels", &source::air_froxels());
    validate("air apply", &source::air_apply());
    validate("precipitation", &source::precipitation());
}

/// The particles' and translucency's entry points are in the forward module, and the frame
/// uniform's size is what the renderer writes.
#[test]
fn the_forward_module_draws_particles_both_ways() {
    let f = source::forward();
    for entry in [
        "fn vs_particle(",
        "fn fs_particle(",
        "fn fs_particle_oit(",
        "fn fs_translucent_oit(",
    ] {
        assert!(f.contains(entry), "{entry}");
    }
    let module = naga::front::wgsl::parse_str(&source::gbuffer()).expect("the G-buffer module");
    let frame = module
        .types
        .iter()
        .find(|(_, t)| t.name.as_deref() == Some("Frame"))
        .map(|(_, t)| t.inner.size(module.to_ctx()))
        .expect("the frame uniform");
    assert_eq!(u64::from(frame), pyreal_engine::renderer::FRAME_SIZE);
}

/// The binding array the shaders declare is the one the material table binds.
#[test]
fn the_texture_array_is_the_tables_size() {
    assert!(source::gbuffer().contains(&format!("binding_array<texture_2d<f32>, {MAX_TEXTURES}>")));
}

/// The ground's slope threshold the shader stands its pictures up at is the renderer's.
#[test]
fn the_shader_stands_steep_ground_up_at_the_renderers_slope() {
    use pyreal_engine::renderer::{TRIPLANAR_FROM, TRIPLANAR_WHOLE};
    let g = source::gbuffer();
    assert!(g.contains(&format!("const TRIPLANAR_FROM: f32 = {TRIPLANAR_FROM:?};")));
    assert!(g.contains(&format!(
        "const TRIPLANAR_WHOLE: f32 = {TRIPLANAR_WHOLE:?};"
    )));
    const { assert!(TRIPLANAR_WHOLE < TRIPLANAR_FROM && TRIPLANAR_FROM < 1.0) };
}
