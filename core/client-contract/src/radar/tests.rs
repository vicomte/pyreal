use super::*;

#[test]
fn roles_follow_the_radar_conventions() {
    let mut entry = RadarEntry {
        in_world: true,
        radar_enum: 4,
        bitfield: 0x4200,
        player_space: (-1.0, 1.0, 5.0),
        ..Default::default()
    };
    assert_eq!(color_role(Some(&entry)), ColorRole::Vendor);
    entry.bitfield |= 0x40000;
    {
        assert_eq!(color_role(Some(&entry)), ColorRole::Portal);
    }
    entry.blip_color = 99;
    {
        assert_eq!(color_role(Some(&entry)), ColorRole::Override(99));
    }
    entry.bitfield |= 0x80;
    {
        assert_eq!(color_role(Some(&entry)), ColorRole::Default);
        assert_eq!(shape_role(Some(&entry), None), ShapeRole::Hidden);
    }
    entry.bitfield = 0x8000_0000;
    assert_eq!(shape_role(Some(&entry), None), ShapeRole::Ordinary);
    entry.is_player = true;
    entry.is_pk = true;
    entry.is_allegiance_member = true;
    assert_eq!(shape_role(Some(&entry), None), ShapeRole::Ordinary);
    let viewer = Some(Viewer {
        pk: true,
        pk_lite: false,
    });
    {
        assert_eq!(shape_role(Some(&entry), viewer), ShapeRole::Allegiance);
    }
    entry.is_allegiance_member = false;
    {
        assert_eq!(shape_role(Some(&entry), viewer), ShapeRole::Threat);
    }
    entry.is_fellow = true;
    entry.is_fellowship_leader = true;
    entry.blip_color = 0;
    {
        assert_eq!(
            shape_role(Some(&entry), viewer),
            ShapeRole::FellowshipLeader
        );
        assert_eq!(color_role(Some(&entry)), ColorRole::Fellowship);
    }
    assert_eq!((radar_range(true), radar_range(false)), (75.0, 25.0));
}
