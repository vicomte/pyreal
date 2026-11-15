//!
//! Every `0x06` record of the portal decodes; with the reference exports, each has the recorded
//! dimensions and record size and exactly the exported RGBA bytes.

use pyreal_assets::pretod::image::decode_rgb;

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the February 2005 portal: --features retail-dats"
)]
fn every_rgb_record_decodes_to_the_exported_pixels() {
    let Some(portal) = super::portal("every_rgb_record_decodes_to_the_exported_pixels") else {
        return;
    };
    let ids: Vec<u32> = portal
        .ids()
        .into_iter()
        .filter(|id| id >> 24 == 6)
        .collect();
    assert!(!ids.is_empty(), "the portal holds RGB records");
    let reference = super::reference("PYREAL_TEST_PRETOD_REFERENCE", "RGB comparison");
    let manifest = reference
        .as_ref()
        .map(|d| super::read_json(&d.join("manifest.json")));
    if let Some(m) = &manifest {
        let assets = m["assets"].as_object().expect("manifest assets");
        assert_eq!(assets.len(), ids.len(), "one exported image per RGB record");
    }
    for id in &ids {
        let payload = portal.get(*id).expect("record reads");
        let image = decode_rgb(&payload, Some(*id)).unwrap_or_else(|e| panic!("{id:08X}: {e}"));
        let (Some(dir), Some(m)) = (&reference, &manifest) else {
            continue;
        };
        let row = &m["assets"][format!("{id:08X}")];
        assert_eq!(row["width"], image.width, "{id:08X} width");
        assert_eq!(row["height"], image.height, "{id:08X} height");
        assert_eq!(row["record_bytes"], payload.len(), "{id:08X} record size");
        let file = dir.join(row["rgba_file"].as_str().expect("rgba file"));
        let want = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert!(want == image.rgba, "{id:08X} pixels differ from the export");
    }
    println!("{} RGB records decoded", ids.len());
}
