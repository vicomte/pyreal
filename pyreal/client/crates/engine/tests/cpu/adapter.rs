//! Which adapters can run the engine: every required feature, and room for the material table's
//! textures in one binding array.

use pyreal_engine::gpu::{unfit, REQUIRED};
use pyreal_engine::materials::MAX_TEXTURES;

fn limits(textures: u32) -> wgpu::Limits {
    wgpu::Limits {
        max_binding_array_elements_per_shader_stage: textures,
        ..wgpu::Limits::default()
    }
}

#[test]
fn an_adapter_with_the_features_and_the_texture_room_is_fit() {
    assert_eq!(unfit(REQUIRED, &limits(MAX_TEXTURES)), None);
    assert_eq!(
        unfit(REQUIRED | wgpu::Features::TIMESTAMP_QUERY, &limits(1 << 20)),
        None
    );
}

#[test]
fn a_missing_feature_is_named() {
    let why = unfit(
        REQUIRED - wgpu::Features::TEXTURE_COMPRESSION_BC,
        &limits(MAX_TEXTURES),
    )
    .expect("unfit");
    assert!(why.contains("TEXTURE_COMPRESSION_BC"), "{why}");
}

#[test]
fn too_few_textures_in_a_binding_array_is_unfit() {
    let why = unfit(REQUIRED, &limits(MAX_TEXTURES - 1)).expect("unfit");
    assert!(why.contains(&MAX_TEXTURES.to_string()), "{why}");
}
