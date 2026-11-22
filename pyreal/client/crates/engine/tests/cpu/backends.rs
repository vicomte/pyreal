//! Every shader module translates for Vulkan (SPIR-V) and DX12 (HLSL at shader models 5.1 and
//! 6.0, or 6.5 with ray queries) as wgpu translates it on Linux and Windows, without a device.

use pyreal_engine::renderer::source;

fn modules() -> Vec<(&'static str, String)> {
    let [overlay, whole] = pyreal_engine::overlay::sources();
    vec![
        ("cull", source::cull()),
        ("gbuffer", source::gbuffer()),
        ("forward", source::forward()),
        ("lighting", source::lighting()),
        ("tonemap", source::tonemap()),
        ("exposure", source::exposure()),
        ("bloom", source::bloom()),
        ("air_tables", source::air_tables()),
        ("shadow_cast", source::shadow_cast()),
        ("clusters", source::clusters()),
        ("lamp_rays", source::lamp_rays()),
        ("gtao", source::gtao()),
        ("ao_filter", source::ao_filter()),
        ("ao_blur", source::ao_blur()),
        ("clouds", source::clouds()),
        ("cloud_resolve", source::cloud_resolve()),
        ("probes", source::probes()),
        ("far", source::far()),
        ("hiz_first", source::hiz_first()),
        ("hiz_down", source::hiz_down()),
        ("water", source::water()),
        ("water merge", source::water_merge()),
        ("water_sim", source::water_sim()),
        ("cover", source::cover()),
        ("cover_draw", source::cover_draw()),
        ("oit", source::oit()),
        ("resolve", pyreal_engine::temporal::resolve_source()),
        ("composite", pyreal_engine::temporal::composite_source()),
        ("ui blur", pyreal_engine::frost::source().to_owned()),
        ("overlay", overlay.to_owned()),
        ("whole picture", whole.to_owned()),
        ("preview", pyreal_engine::preview::source()),
        ("air froxels", source::air_froxels()),
        ("air apply", source::air_apply()),
        ("precipitation", source::precipitation()),
    ]
}

#[test]
fn every_module_translates_for_vulkan_and_dx12() {
    use naga::back::{hlsl, spv};
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let mut failures = Vec::new();
    for (name, wgsl) in modules() {
        let parsed = naga::front::wgsl::parse_str(&wgsl).expect(name);
        let parsed_info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&parsed)
            .expect(name);
        // A module with overrides is translated as wgpu translates each of its pipelines: with
        // the overrides at their defaults, and with every one a pipeline sets to off.
        let mut settings = vec![naga::back::PipelineConstants::default()];
        if !parsed.overrides.is_empty() {
            let mut off = naga::back::PipelineConstants::default();
            for (_, o) in parsed.overrides.iter() {
                if let Some(n) = &o.name {
                    off.insert(n.clone(), 0.0);
                }
            }
            settings.push(off);
        }
        for constants in &settings {
            let (module, info) = naga::back::pipeline_constants::process_overrides(
                &parsed,
                &parsed_info,
                None,
                constants,
            )
            .expect(name);
            let (module, info) = (module.as_ref(), info.as_ref());
            let rays = wgsl.contains("wgpu_ray_query");
            let spv_opts = spv::Options {
                lang_version: if rays { (1, 4) } else { (1, 3) },
                ..Default::default()
            };
            if let Err(e) = spv::write_vec(module, info, &spv_opts, None) {
                failures.push(format!("{name}: SPIR-V: {e}"));
            }
            let models: &[hlsl::ShaderModel] = if rays {
                &[hlsl::ShaderModel::V6_5]
            } else {
                &[hlsl::ShaderModel::V5_1, hlsl::ShaderModel::V6_0]
            };
            for &sm in models {
                let opts = hlsl::Options {
                    shader_model: sm,
                    ..Default::default()
                };
                let pipe = hlsl::PipelineOptions::default();
                let mut out = String::new();
                let mut w = hlsl::Writer::new(&mut out, &opts, &pipe);
                if let Err(e) = w.write(module, info, None) {
                    failures.push(format!("{name}: HLSL {sm:?}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
