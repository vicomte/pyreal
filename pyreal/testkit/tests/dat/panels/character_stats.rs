use super::*;

// =============================================================================================
// advancement.cost.*
// =============================================================================================

/// One shipped table, decoded straight out of the dats the client was built over.
fn table<T: pyreal_assets::Decode>(c: &HeadlessClient, id: u32) -> T {
    use pyreal_primitives::AssetSource as _;
    let id = pyreal_primitives::DataId(id);
    let store = std::sync::Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let bytes = store.read(id).expect("the shipped table is in the dats");
    T::decode_payload(id, &bytes).expect("the shipped table decodes")
}

/// The character this scenario costs against: an attribute at rank 30, with what has been spent
/// on it.
const RANK: u32 = 30;
const SPENT: u32 = 32_676;

/// **The table, and the two ways ten points are costed.** An attribute's ten points cost the
/// distance from where the character is to ten ranks on; a skill's cost the distance between the
/// same two entries of its own table, and **not** the distance to the table's last entry.
pub(super) fn ten_points_cost_the_distance_between_two_entries_of_the_shipped_table() {
    use pyreal_client_model::qualities::Qualities;
    use pyreal_protocol::types::qualities::Skill;
    use pyreal_rules::skills::Sac;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let t: pyreal_assets::tables::XpTable = table(&c, 0x0E00_0018);

    let attribute = pyreal_rules::advancement::max_attribute_level(&t) == 190
        && pyreal_rules::advancement::attribute_cost_to_raise(&t, RANK, SPENT, false) == 2_456
        && pyreal_rules::advancement::attribute_cost_to_raise_10(&t, RANK, SPENT, false) == 31_202;

    // The sibling arithmetic: any skill id, because what is read of the record is its class, its
    // level and what has been sunk into it.
    let id = 1;
    let mut skill = true;
    for sac in [Sac::Trained, Sac::Specialized] {
        let column = if sac == Sac::Trained {
            &t.trained_xp
        } else {
            &t.specialized_xp
        };
        let mut q = Qualities::new();
        q.set_skill(
            id,
            Skill {
                level_from_pp: 30,
                format_version: 1,
                sac: sac as u32,
                pp: column[30],
                init_level: 0,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            },
        );
        let want = column[40] - column[30];
        // The premise: ten ranks on and the table's end are different numbers, so a cost that
        // reached for the cap would be visible here.
        skill &= want != column[column.len() - 1] - column[30]
            && pyreal_rules::advancement::skill_cost_to_raise_10(&q, &t, id) == want;
    }

    c.assert_behaviour(
        "advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table",
        move |_| attribute && skill,
    );
    c.shutdown();
}
