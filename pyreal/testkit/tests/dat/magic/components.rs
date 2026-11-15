use super::*;
pub fn the_component_a_slot_stands_for_is_a_representative_of_its_kind() {
    let (mut c, formula) = examine::a_pack_on(FLAME_BOLT, &[]);
    let scid = formula[0];
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, scid, 10);

    let first = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the kind is carried");
    let again = c.view().world().component_object_id(scid);
    let one_of_the_two =
        first == examine::component_object(0) || first == examine::A_SECOND_OF_ITS_KIND;

    // Take the answered one away; the kind is still owned, so the slot still answers.
    examine::drop_it(&mut c, first);
    let rest = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the other one is still here");

    // A slot number the shipped table does not know, and one whose kind is not carried.
    let unknown = c.view().world().component_object_id(0);
    let outside = c.view().world().component_object_id(0x00FF_FFFF);

    c.assert_behaviour(
        "spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object",
        move |_| {
            one_of_the_two
                && again == Some(first)
                && rest != first
                && unknown.is_none()
                && outside.is_none()
        },
    );
    c.shutdown();
}

pub fn the_notice_moves_for_a_component_and_for_nothing_else() {
    let (mut c, formula, _) = examine::a_pack_missing_unique(ACID_STREAM_V, 2);

    let before = c.view().world().magic.component_serial;
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, formula[0], 10);
    let after_component = c.view().world().magic.component_serial;

    // An ordinary item in the same pack: it is not a component, so it raises nothing.
    examine::carry_a_plain_item(&mut c, examine::A_PLAIN_ITEM);
    let after_item = c.view().world().magic.component_serial;

    c.tick(2);
    let after_idle_frames = c.view().world().magic.component_serial;

    c.assert_behaviour(
        "spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else",
        move |_| {
            after_component > before
                && after_item == after_component
                && after_idle_frames == after_component
        },
    );
    c.shutdown();
}
