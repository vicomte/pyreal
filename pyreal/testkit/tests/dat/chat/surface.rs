use super::*;
// ---------------------------------------------------------------------------------------------
// What a typed line becomes, by the talk focus
// ---------------------------------------------------------------------------------------------

/// Run one typed line through the client's own frame with the talk focus set, and answer with what
/// it produced.
fn line_with_focus(focus: u32, text: &str) -> Vec<pyreal_client_model::Request> {
    use pyreal_client_contract::UiRequest;
    let store = pyreal_dat::testing::open_store().expect("the retail data is the client's own");
    let mut inter = pyreal_client_runtime::interaction::Interaction::new();
    let mut objects = pyreal_client_runtime::objects::ObjectStream::new();
    let mut requests = vec![UiRequest::SetTalkFocus { focus }];
    if focus == 2 {
        // The tell row is taken only with a named player picked out, so one is; the chat window
        // then gives up its target before the line is typed -- whoever it was has gone.
        let world = &mut objects.world;
        let me = ObjectId(0x5000_0001);
        let them = ObjectId(0x5000_0002);
        let mut peer = pyreal_client_model::Weenie::new(them);
        peer.pwd.name = "Alba".into();
        peer.pwd.obj_type = pyreal_rules::weenie::item_type::CREATURE;
        peer.pwd.bitfield |= pyreal_rules::weenie::bitfield::PLAYER;
        world.tables.weenies.insert(them, peer);
        world.player = Some(me);
        world.selected = Some(them);
        requests.push(UiRequest::SetLastSpeakableTarget {
            object: ObjectId(0),
        });
    }
    requests.push(UiRequest::ChatLine {
        text: text.to_owned(),
        window: 8,
    });
    inter.queue(Vec::new(), requests);
    let (unowned, _) = pyreal_client_runtime::interaction::use_time(
        &mut inter,
        &store,
        None,
        &mut objects,
        None,
        Vec::new(),
        false,
        (800, 600),
        pyreal_primitives::LocalTime(1.0),
    );
    assert!(
        unowned.is_empty(),
        "focus {focus}: nobody owned {unowned:?}"
    );
    inter.last_sent.clone()
}

/// What a typed line becomes follows the talk-to menu: an ordinary spoken line, or a line on one
/// of four channels -- and the row that talks to whoever is selected sends nothing at all when
/// nobody is.
pub fn what_a_typed_line_becomes_follows_the_menu() {
    use pyreal_client_model::Request;
    let talk = |m: &str| {
        Request::Talk(pyreal_protocol::comms::CommunicationTalk {
            message: m.to_owned(),
        })
    };
    let channel = |ch: u32, m: &str| {
        Request::ChannelBroadcast(pyreal_protocol::comms::CommunicationChannelBroadcast {
            channel: ch,
            message: m.to_owned(),
        })
    };
    // **The channels do not follow the menu's own naming**, which is why each is named here: the
    // fellowship row carries one number and the vassals row another, and getting them the wrong
    // way round sends a line where everybody but the player can read it.
    let cases: [(u32, Option<Request>); 6] = [
        (1, Some(talk("well met"))),
        // The row that talks to whoever is selected, with nobody there any more: dropped rather
        // than said out loud, which is what stops a private line becoming a public one.
        (2, None),
        (3, Some(channel(0x800, "well met"))),
        (4, Some(channel(0x2000, "well met"))),
        (5, Some(channel(0x4000, "well met"))),
        (6, Some(channel(0x1000, "well met"))),
    ];
    let mut every = true;
    for (focus, want) in cases {
        let got = line_with_focus(focus, "well met");
        let holds = match &want {
            Some(w) => got == vec![w.clone()],
            None => got.is_empty(),
        };
        assert!(holds, "focus {focus}: got {got:?}");
        every &= holds;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.talk-focus.what-a-typed-line-becomes-follows-the-menu",
        move |_| every,
    );
}

/// A line the client handles itself is never spoken to the shard, whichever channel the menu is
/// on -- saying a command out loud on a channel everybody reads would be both wrong and rude.
pub fn a_command_the_client_handles_is_never_spoken() {
    let mut every = true;
    for focus in [1_u32, 3, 4, 5, 6] {
        let got = line_with_focus(focus, "@help");
        assert!(got.is_empty(), "focus {focus}: got {got:?}");
        every &= got.is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard",
        move |_| every,
    );
}
