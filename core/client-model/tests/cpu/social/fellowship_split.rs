//! The even-split flag follows a fellow's level change.
//! Fixture: synthetic state.

use crate::common::model_replay::*;
use pyreal_client_model::fellowship::FellowUpdate;
use pyreal_rules::fellowship::{Fellow, Fellowship};

const LEADER: ObjectId = ObjectId(0x5000_0001);
const FELLOW: ObjectId = ObjectId(0x5000_0002);

#[test]
fn a_fellow_levelling_out_of_range_ends_the_even_split() {
    let mut w = World::new();
    w.player = Some(LEADER);
    let mut f = Fellowship {
        leader: LEADER,
        share_xp: true,
        even_xp_split: true,
        ..Fellowship::default()
    };
    for (id, level) in [(LEADER, 20), (FELLOW, 22)] {
        f.members.insert(
            id,
            Fellow {
                name: format!("{id:?}"),
                level,
                ..Fellow::default()
            },
        );
    }
    w.fellowship = Some(f);
    let wire = pyreal_protocol::social::Fellow {
        name: "Fellow".into(),
        level: 30,
        ..Default::default()
    };
    w.recv_fellowship_update_fellow(FELLOW, &wire, FellowUpdate::Stats);
    assert!(!w.fellowship.as_ref().expect("fellowship").even_xp_split);
}
