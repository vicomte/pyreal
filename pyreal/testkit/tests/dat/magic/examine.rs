use pyreal_primitives::ObjectId;
use pyreal_testkit::{ClientSpec, HeadlessClient, Inbound};

/// The character these scenarios put in the world when they need one.
const PLAYER: ObjectId = ObjectId(0x5116_0000);
/// The foci a scenario puts in a side pack.
const FOCI: ObjectId = ObjectId(0x5116_0001);

/// A whole client with the interface up, and nothing else.
pub fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// One row of the shipped spell table.
pub fn shipped_spell(c: &HeadlessClient, spell: u32) -> pyreal_assets::tables::SpellBase {
    c.view()
        .expect_app()
        .hud()
        .spell_table
        .as_ref()
        .expect("the client loads the shipped spell table at startup")
        .spells
        .get(&spell)
        .unwrap_or_else(|| panic!("the shipped table carries spell {spell}"))
        .clone()
}

/// `Login_CharacterSet` with a synthetic account name, **through the codec** -- which is the
/// only way the client ever obtains one.
pub fn greet(c: &mut HeadlessClient, account: &str) {
    let set = pyreal_protocol::login::LoginCharacterSet {
        status: 0,
        characters: Vec::new(),
        deleted: Vec::new(),
        num_allowed_characters: 5,
        account: account.to_owned(),
        use_turbine_chat: 1,
        has_throne_of_destiny: 1,
    };
    let bytes = pyreal_protocol::write_body(&set).expect("the greeting encodes");
    let back: pyreal_protocol::login::LoginCharacterSet =
        pyreal_protocol::read_body(&bytes).expect("and decodes");
    assert_eq!(back.account, account, "the name survives the wire");
    c.when(Inbound::event(
        pyreal_client_net::client_session::SessionEvent::CharacterSet(Box::new(back)),
    ));
    c.tick(1);
}

/// The character's own description. An empty one is enough: what these scenarios are about is
/// the hand-off it carries and not the qualities.
pub fn describe(c: &mut HeadlessClient) {
    c.when(Inbound::event(
        pyreal_client_net::client_session::SessionEvent::PlayerDescription(Box::default()),
    ));
    c.tick(1);
}

/// Put the player in the world with an inventory, so the walk that looks for a foci has a list
/// to walk and the formula does not take its "not in the world yet" arm.
pub fn stand_in_the_world(c: &mut HeadlessClient) -> ObjectId {
    let w = c.world_mut();
    let me = w.player.unwrap_or(PLAYER);
    if w.weenie(me).is_none() {
        let mut wn = pyreal_client_model::weenie::Weenie::new(me);
        wn.pwd = pyreal_protocol::types::PublicWeenieDesc {
            name: "Tester".into(),
            // The type the component walk expects of the thing whose pack it is reading.
            obj_type: 0x0000_0010,
            ..pyreal_protocol::types::PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(me, wn);
    }
    w.set_player(me);
    if let Some(x) = w.tables.weenies.get_mut(me) {
        if x.qualities.is_none() {
            x.qualities = Some(pyreal_client_model::qualities::Qualities::default());
        }
    }
    if w.inventory(me).is_none() {
        w.tables
            .inventories
            .insert(me, pyreal_client_model::objects::ObjectInventory::new(me));
    }
    me
}

/// The class the shipped table gives as `school`'s foci.
pub fn foci_of(c: &HeadlessClient, school: u32) -> u32 {
    c.view()
        .expect_app()
        .hud()
        .school_pack_wcid
        .iter()
        .find(|(s, _)| *s == school)
        .map(|(_, w)| *w)
        .unwrap_or_else(|| panic!("the shipped mapper names a foci for school {school}"))
}

/// Some other school the shipped mapper also names a foci for.
pub fn another_school(c: &HeadlessClient, not: u32) -> u32 {
    c.view()
        .expect_app()
        .hud()
        .school_pack_wcid
        .iter()
        .map(|(s, _)| *s)
        .find(|s| *s != not)
        .expect("the shipped mapper names more than one school")
}

/// A foci in the player's side-pack list, which is the only list the ownership walk reads.
pub fn carry_the_foci(c: &mut HeadlessClient, me: ObjectId, wcid: u32) {
    let w = c.world_mut();
    let mut foci = pyreal_client_model::weenie::Weenie::new(FOCI);
    foci.pwd = pyreal_protocol::types::PublicWeenieDesc {
        name: "A foci".into(),
        wcid,
        container_id: Some(me),
        ..pyreal_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(FOCI, foci);
    let mut inv = w
        .inventory(me)
        .cloned()
        .expect("the player has an inventory");
    inv.containers.push(FOCI);
    w.tables.inventories.insert(me, inv);
}

// -----------------------------------------------------------------------------------------
// The player's pack of components, for `spell-examine.components.*` and `spell-examine.marks.*`.
// -----------------------------------------------------------------------------------------

/// The item type every call site of the component update gates on.
const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;

/// A second object of one slot's kind, for the scenarios about owning a kind rather than a
/// thing.
pub const A_SECOND_OF_ITS_KIND: ObjectId = ObjectId(0x8116_0400);
/// Something that is not a component at all, in the same pack.
pub const A_PLAIN_ITEM: ObjectId = ObjectId(0x8116_0999);
/// Somewhere that is not the player's pack.
const ELSEWHERE: ObjectId = ObjectId(0x7116_0001);

/// The component object this family puts in the pack for formula slot `slot`.
pub fn component_object(slot: usize) -> ObjectId {
    ObjectId(0x8116_0300 + u32::try_from(slot).expect("a small formula"))
}

/// The formula the client would cast `spell` with, truncated to the slots that carry a
/// component -- **not** the shipped table's raw slots.
pub fn formula_of(c: &HeadlessClient, spell: u32) -> Vec<u32> {
    let base = shipped_spell(c, spell);
    let f = c.view().world().spell_formula(&base);
    let n = pyreal_rules::magic::num_spell_components(&f);
    assert!(n > 0, "the shipped formula for spell {spell} has slots");
    f[..n].to_vec()
}

/// A client standing in the world with the shipped component catalogue handed over by its own
/// description, and the formula the client would cast `spell` with.
fn a_client_with_a_pack(spell: u32) -> (HeadlessClient, Vec<u32>) {
    let mut c = a_client();
    describe(&mut c);
    let _ = stand_in_the_world(&mut c);
    assert!(
        !c.view().world().magic.catalogue.is_empty(),
        "the description handed the shipped component table to the game model"
    );
    let formula = formula_of(&c, spell);
    (c, formula)
}

/// A client with every slot of `spell` outside `missing` already in the player's pack.
pub fn a_pack_on(spell: u32, missing: &[usize]) -> (HeadlessClient, Vec<u32>) {
    let (mut c, formula) = a_client_with_a_pack(spell);
    fill(&mut c, &formula, missing);
    (c, formula)
}

/// The same, with `n` slots left out -- chosen as slots whose **kind** appears in that formula
/// exactly once, because two slots of one kind stand or fall together and a scenario built on
/// a duplicated kind would be measuring something else.
pub fn a_pack_missing_unique(spell: u32, n: usize) -> (HeadlessClient, Vec<u32>, Vec<usize>) {
    let (mut c, formula) = a_client_with_a_pack(spell);
    let missing: Vec<usize> = (0..formula.len())
        .filter(|i| formula.iter().filter(|s| **s == formula[*i]).count() == 1)
        .take(n)
        .collect();
    assert_eq!(
        missing.len(),
        n,
        "the shipped formula has {n} slots of their own kind"
    );
    fill(&mut c, &formula, &missing);
    (c, formula, missing)
}

fn fill(c: &mut HeadlessClient, formula: &[u32], missing: &[usize]) {
    for (i, scid) in formula.iter().enumerate() {
        if !missing.contains(&i) {
            carry(c, component_object(i), *scid, 10);
        }
    }
}

/// Put a stack of the kind slot `scid` names in the player's pack, through the client's own
/// move seam. Nothing here adds a row to the component tracker by hand.
pub fn carry(c: &mut HeadlessClient, id: ObjectId, scid: u32, stack: u16) {
    let (wcid, name) = {
        let cat = &c.view().world().magic.catalogue;
        let wcid = cat.scid_to_wcid(scid);
        assert_ne!(wcid, 0, "slot {scid} names a component kind");
        let name = cat
            .inq_spell_component_base(scid)
            .expect("the shipped table has a row for it")
            .name
            .clone();
        (wcid, name)
    };
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    put(
        w,
        id,
        pyreal_protocol::types::PublicWeenieDesc {
            name,
            wcid,
            obj_type: TYPE_SPELL_COMPONENTS,
            stack_size: Some(stack),
            container_id: Some(me),
            ..pyreal_protocol::types::PublicWeenieDesc::default()
        },
    );
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        me,
        0,
        ObjectId(0),
        0,
        true,
        &mut pyreal_client_model::NullSink,
    );
}

/// The same for something that is not a component, which is the control for the notice.
pub fn carry_a_plain_item(c: &mut HeadlessClient, id: ObjectId) {
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    put(
        w,
        id,
        pyreal_protocol::types::PublicWeenieDesc {
            name: "A sceptre".into(),
            wcid: 999,
            obj_type: 0,
            container_id: Some(me),
            ..pyreal_protocol::types::PublicWeenieDesc::default()
        },
    );
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        me,
        0,
        ObjectId(0),
        0,
        true,
        &mut pyreal_client_model::NullSink,
    );
}

/// Move one out of the player's ownership, through the same seam.
pub fn drop_it(c: &mut HeadlessClient, id: ObjectId) {
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    if w.weenie(ELSEWHERE).is_none() {
        put(
            w,
            ELSEWHERE,
            pyreal_protocol::types::PublicWeenieDesc::default(),
        );
    }
    if let Some(x) = w.tables.weenies.get_mut(id) {
        x.pwd.container_id = Some(ELSEWHERE);
    }
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        ELSEWHERE,
        0,
        ObjectId(0),
        0,
        true,
        &mut pyreal_client_model::NullSink,
    );
}

fn put(
    w: &mut pyreal_client_model::World,
    id: ObjectId,
    pwd: pyreal_protocol::types::PublicWeenieDesc,
) {
    let mut wn = pyreal_client_model::weenie::Weenie::new(id);
    wn.pwd = pwd;
    w.tables.weenies.insert(id, wn);
}

fn refresh_contents(w: &mut pyreal_client_model::World, container: ObjectId) {
    let held: Vec<pyreal_protocol::types::ContentProfile> = w
        .tables
        .weenies
        .iter()
        .filter(|(_, x)| x.pwd.container_id == Some(container))
        .map(|(k, x)| pyreal_protocol::types::ContentProfile {
            iid: k,
            container_properties: u32::from(x.is_container()),
        })
        .collect();
    w.view_object_contents(container, &held, &mut pyreal_client_model::NullSink);
}
