//! Sessions in process: log in, log out back to the screens before the world, log in again, over
//! and over in one client, each session on a socket-free endpoint of its own.

use pyreal_client_runtime::app::SessionStart;
use pyreal_testkit::replay::Peer;
use pyreal_testkit::{ClientSpec, HeadlessClient};

/// The character every session logs on as.
const LARK: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x5000_0001);

/// How many times the client logs in and out.
const SESSIONS: usize = 4;

/// The session's start: the shard's address, an account of its own, and the later data files;
/// every other session plays by another world's rules, and the third, when the earlier files are
/// here, is drawn from them.
fn start(modern: &std::path::Path, n: usize) -> SessionStart {
    let mut world = Vec::new();
    if n % 2 == 1 {
        world.push(("world-profile".into(), "classicace-customdm".into()));
    }
    let classic = (n == 2)
        .then(pyreal_dat::testing::classic_dat_dir)
        .flatten();
    if classic.is_some() {
        world.push(("world-base".into(), "classic".into()));
    }
    SessionStart {
        host: "127.0.0.1".into(),
        port: 19001,
        account: format!("relogin{n}"),
        password: "unused".into(),
        dat_dir: modern.to_path_buf(),
        classic_dat_dir: classic,
        world,
        plugin_policy: None,
    }
}

/// The shard names its world, lists the character, lets it in and describes it, as the relog
/// scenarios' shard does.
fn log_in(c: &mut HeadlessClient, peer: &mut Peer, account: &str) {
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 100,
            world_name: "relogin-world".into(),
        })
        .expect("the world name encodes"),
    );
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginCharacterSet {
            status: 0,
            characters: vec![pyreal_protocol::login::CharacterIdentity {
                gid: LARK,
                name: "Lark".into(),
                seconds_greyed_out: 0,
            }],
            deleted: vec![],
            num_allowed_characters: 5,
            account: account.into(),
            use_turbine_chat: 0,
            has_throne_of_destiny: 0,
        })
        .expect("the list encodes"),
    );
    c.tick(1);
    c.replay_net_mut()
        .expect("the endpoint")
        .session
        .enter_world(LARK, account);
    c.tick(1);
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginEnterGameServerReady)
            .expect("the ready message encodes"),
    );
    c.tick(1);
    let module = pyreal_protocol::login::PlayerModule {
        spell_bars: vec![Vec::new()],
        spell_filters: pyreal_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
        options: pyreal_client_model::player::options::DEFAULT_OPTIONS,
        options2: pyreal_client_model::player::options::DEFAULT_OPTIONS2,
        option_flags: pyreal_protocol::login::player_module_flags::SPELLBOOK_FILTERS
            | pyreal_protocol::login::player_module_flags::CHARACTER_OPTIONS_2,
        ..pyreal_protocol::login::PlayerModule::default()
    };
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..pyreal_protocol::login::LoginPlayerDescription::default()
        })
        .expect("the description encodes"),
    );
    c.tick(3);
}

/// What one session left behind, read off the places each lives.
#[derive(Debug, PartialEq, Eq)]
struct Left {
    in_session: bool,
    described: bool,
    character_list: bool,
    world_drawn: bool,
    in_gameplay: bool,
}

fn left(c: &mut HeadlessClient) -> Left {
    let in_session = c.app_mut().in_session();
    let app = c.view().expect_app();
    Left {
        in_session,
        described: c.view().world().player_system.module.is_some(),
        character_list: app.host_state().character_set.is_some(),
        world_drawn: app.world.is_some(),
        in_gameplay: app.shell().horizon().is_some_and(|h| h.in_gameplay()),
    }
}

/// The client logs in, logs out to the screens before the world and logs in again, four times in
/// one process: each session reaches the world over its own endpoint and account, plays by its
/// own world's rules, and leaves nothing behind it when it ends.
#[test]
fn a_client_logs_in_and_out_again_and_again_in_one_process() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4).with_scratch_settings("relogin"));
    let nothing = Left {
        in_session: false,
        described: false,
        character_list: false,
        world_drawn: false,
        in_gameplay: false,
    };
    assert_eq!(left(&mut c), nothing, "before any session");
    let modern = c.view().expect_app().config().dat_dir.clone();
    for n in 0..SESSIONS {
        let start = start(&modern, n);
        let mut peer = Peer::begin_session(&mut c, LARK, &start);
        log_in(&mut c, &mut peer, &start.account);
        assert_eq!(
            c.replay_net_mut().expect("the endpoint").session_state(),
            pyreal_client_net::client_session::SessionState::Playable,
            "session {n} reaches the world"
        );
        let app = c.view().expect_app();
        assert_eq!(app.config().account, start.account, "session {n}'s account");
        let era = if start.classic_dat_dir.is_some() {
            pyreal_dat::ContainerEra::Classic
        } else {
            pyreal_dat::ContainerEra::Modern
        };
        assert_eq!(
            app.store.era(),
            era,
            "session {n}'s world is drawn from its own files"
        );
        assert_eq!(
            c.view().world().world_rules.profile.as_deref(),
            (n % 2 == 1).then_some("classicace-customdm"),
            "session {n} plays by its own world's rules"
        );
        assert!(
            c.view().world().player_system.module.is_some(),
            "session {n}'s character is described"
        );
        assert!(
            app.shell().horizon().is_some_and(|h| h.in_gameplay()),
            "session {n}'s interface follows it into the world"
        );
        c.app_mut().end_session();
        c.tick(2);
        assert_eq!(left(&mut c), nothing, "after session {n}");
    }
    c.shutdown();
}

/// The screens before the world sign in to a world on the list with the password the vault keeps
/// (the old launcher's entry, found and kept as Pyreal's), start the session in process on the
/// data files they know, and log out back to the list.
#[test]
fn the_world_list_signs_in_with_the_kept_password_and_logs_out() {
    use pyreal_horizon::front_door::FrontDoorRequest;
    use pyreal_launch::vault::{legacy_target, MemoryVault, Vault};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(2).with_scratch_settings("frontdoor"));
    let dir = c.scratch_settings().expect("a folder").dir().to_path_buf();
    let modern = c.view().expect_app().config().dat_dir.clone();
    std::fs::write(
        dir.join("worlds.json"),
        r#"{"schema":1,"worlds":[{"slug":"custom-1","name":"Home","host":"127.0.0.1","port":9,"imported":true}]}"#,
    )
    .expect("the list");
    let mut vault = MemoryVault::default();
    vault
        .set(&legacy_target("custom-1", "lark"), "lark", "s3cret")
        .expect("kept");
    let mut door =
        pyreal_client::front_door::FrontDoor::open(&dir, None, Box::new(vault), Some(&modern));
    assert!(
        door.view().dats.ready,
        "the data files the run has are taken"
    );
    let ask = |c: &mut HeadlessClient, r: FrontDoorRequest| {
        c.app_mut()
            .shell_mut()
            .horizon_mut()
            .expect("the interface is up")
            .ui
            .front_door_requests
            .push(r);
    };
    for round in 0..3 {
        ask(
            &mut c,
            FrontDoorRequest::SignIn {
                slug: "custom-1".into(),
                account: "Lark".into(),
                password: String::new(),
                remember: true,
            },
        );
        door.step(c.app_mut());
        // The world is asked which era it plays first; nothing answers on its port, so the sign-in
        // goes ahead once the question fails.
        for _ in 0..500 {
            if door.view().message.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            door.step(c.app_mut());
        }
        assert_eq!(door.view().message, None, "round {round}");
        assert!(c.app_mut().in_session(), "round {round}: the session is up");
        let cfg = c.view().expect_app().config();
        assert_eq!(
            (
                cfg.account.as_str(),
                cfg.host.as_str(),
                cfg.vg_password.as_str()
            ),
            ("lark", "127.0.0.1", "s3cret")
        );
        c.tick(2);
        ask(&mut c, FrontDoorRequest::LogOut);
        door.step(c.app_mut());
        assert!(!c.app_mut().in_session(), "round {round}: logged out");
        c.tick(2);
    }
    let view = door.view();
    assert_eq!(view.worlds[0].account.as_deref(), Some("Lark"));
    assert!(view.worlds[0].password_saved);
    c.shutdown();
}

/// The shard names its world and lists `characters` of five, and goes no further.
fn list_characters(c: &mut HeadlessClient, peer: &mut Peer, account: &str, characters: u32) {
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 100,
            world_name: "relogin-world".into(),
        })
        .expect("the world name encodes"),
    );
    peer.send(
        c,
        9,
        pyreal_protocol::write_blob(&pyreal_protocol::login::LoginCharacterSet {
            status: 0,
            characters: (0..characters)
                .map(|i| pyreal_protocol::login::CharacterIdentity {
                    gid: pyreal_primitives::ObjectId(LARK.0 + i),
                    name: format!("Lark{i}"),
                    seconds_greyed_out: 0,
                })
                .collect(),
            deleted: vec![],
            num_allowed_characters: 5,
            account: account.into(),
            use_turbine_chat: 0,
            has_throne_of_destiny: 0,
        })
        .expect("the list encodes"),
    );
    c.tick(3);
}

/// What character select's Create waits on from the session: the world's creation tables in the
/// interface, and a free slot. The replayed shard leaves the phase where it is.
fn create_offered(c: &mut HeadlessClient) -> (bool, bool) {
    let app = c.view().expect_app();
    let tables = app
        .shell()
        .horizon()
        .is_some_and(|h| h.ui.pregame.creation_source.is_some());
    let free = app
        .host_state()
        .character_set
        .as_ref()
        .is_some_and(|s| s.set.len() < s.num_allowed_characters as usize);
    (tables, free)
}

/// Logged out of one world and into another, character select still offers Create on an account
/// with room for a character.
#[test]
fn create_is_offered_on_the_second_world_after_logging_out_of_the_first() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4).with_scratch_settings("createagain"));
    let modern = c.view().expect_app().config().dat_dir.clone();
    for n in 0..3 {
        let start = start(&modern, n);
        let mut peer = Peer::begin_session(&mut c, LARK, &start);
        list_characters(&mut c, &mut peer, &start.account, 1);
        assert_eq!(
            create_offered(&mut c),
            (true, true),
            "session {n} offers Create"
        );
        c.app_mut().end_session();
        c.tick(2);
    }
    c.shutdown();
}
