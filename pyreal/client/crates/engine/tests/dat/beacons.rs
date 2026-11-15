//! The Empyrean lighthouses' beacons are found where they stand, and nowhere else.

use pyreal_assets::world::LandblockInfo;
use pyreal_dat::DbType;
use pyreal_engine::beacons::{is_beacon, HEART, WISP};

/// Each of the seven lighthouses (Tou-Tou's at 0xF85E among them) has its beacon's heart, its
/// wisps standing over it, and five a burst too; the same setups elsewhere (the burst on hilltops
/// and under the sea, the wisps in towns) are not beacons.
#[test]
fn the_lighthouses_beacons_are_found_on_the_seven_lighthouses() {
    let store = pyreal_dat::testing::open_store_or_fail();
    let (mut hearts, mut bursts, mut elsewhere) = (Vec::new(), 0, 0);
    for id in store.ids_of(DbType::Lbi) {
        let Some(info) = super::decode::<LandblockInfo>(&store, DbType::Lbi, id) else {
            continue;
        };
        let buildings: Vec<_> = info
            .buildings
            .iter()
            .map(|b| (b.id.0, b.frame.origin))
            .collect();
        for o in &info.objects {
            match is_beacon(o.id.0, o.frame.origin, &buildings) {
                Some(HEART) => hearts.push(id.0 >> 16),
                Some(WISP) => bursts += 1,
                Some(_) => panic!("an unknown beacon part"),
                None if matches!(o.id.0, 0x0200_03CB | 0x0200_03C5) => elsewhere += 1,
                None => {}
            }
        }
    }
    hearts.sort_unstable();
    assert_eq!(
        hearts,
        [0x0A0F, 0x445F, 0xB8E8, 0xBAE8, 0xED19, 0xEF0D, 0xF85E]
    );
    assert_eq!(bursts, 5);
    assert!(elsewhere > 300, "{elsewhere}");
}
