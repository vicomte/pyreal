use super::*;

// -------------------------------------------------------------------------------------------
// radar.*
// -------------------------------------------------------------------------------------------

/// Only what the shard marks for the radar reaches it, and that throws most of the scene away.
///
/// The wanted set is read out of each recording's own descriptions rather than out of the thing
/// under test, so the two can disagree.
pub fn only_what_the_shard_marks_reaches_the_radar() {
    use pyreal_primitives::ObjectId;
    use pyreal_ui_logic::radar::inq_showable_on_radar;

    let mut measured = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let Some(player) = objects.world.player else {
            continue;
        };

        let mut want: std::collections::BTreeSet<ObjectId> = std::collections::BTreeSet::new();
        let mut total = 0usize;
        for (id, presence) in objects.presences() {
            let Some(w) = objects.world.weenie(id) else {
                continue;
            };
            total += 1;
            // The three values that mean "show this", and nothing else -- an object the shard
            // said nothing about reads as "undefined" and never reaches the radar.
            let showable = matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4);
            if showable && presence.position.is_some() && id != player {
                want.insert(id);
            }
        }
        if total == 0 {
            continue;
        }

        let list = world_support::radar_list(&objects);
        let got: std::collections::BTreeSet<ObjectId> = list
            .iter()
            .filter(|o| !o.is_self && inq_showable_on_radar(o))
            .map(|o| o.id)
            .collect();
        ok &= got == want;
        // ...and it is a filter and not a no-op: most of the scene is thrown away.
        ok &= want.len() * 2 < total;
        println!(
            "radar: {session}: {} of {total} objects reach the radar",
            want.len()
        );
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it",
        move |_| measured >= 3 && ok,
    );
}

/// Every blip takes the colour its own description asks for.
///
/// Each object is coloured twice -- once by the client, once by a reading of the recording's own
/// description written out separately -- and the two must agree about every object in every
/// recording.
pub fn every_blip_takes_the_colour_its_description_asks_for() {
    use pyreal_ui_logic::radar::{get_blip_color, inq_showable_on_radar};

    let mut checked = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let Some(w) = objects.world.weenie(o.id) else {
                continue;
            };
            let got = get_blip_color(Some(o)).hex;
            let want = world_support::expected_blip_colour(&w.pwd);
            if got != want {
                println!(
                    "radar: {session} {:?}: {got:#08X} against {want:#08X}",
                    o.id
                );
            }
            ok &= got == want;
            checked += 1;
        }
    }
    println!("radar: {checked} things on the radar coloured across the recordings");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.every-one-takes-the-colour-its-own-description-asks-for",
        move |_| checked > 50 && ok,
    );
}

/// The only things on the radar that take no colour of their own are ordinary players.
///
/// A radar of plain white crosses where retail draws coloured ones is the failure this guards,
/// so a thing that takes the plain colour is exactly a thing every rule missed -- **except** an
/// ordinary player at peace, whom the client's own last branch leaves plain on purpose. No
/// creature, trader, portal or item may reach it. More than one colour must appear, or a radar
/// that painted everything alike would satisfy this too.
///
/// "Not one at all" would be wrong: three of the eighteen recordings carry other players.
pub fn no_blip_falls_through_to_the_colour_that_means_nothing() {
    use pyreal_ui_logic::radar::{get_blip_color, inq_showable_on_radar, semantic, YELLOW};

    let mut all: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut white = 0usize;
    let mut plain_players = 0usize;
    let mut measured = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let c = get_blip_color(Some(o)).hex;
            *all.entry(c).or_default() += 1;
            if c == semantic::DEFAULT.hex {
                // Who it is matters: the plain colour is what the client's own last branch gives
                // an ordinary player at peace, and is not what anything else may end up with.
                let is_player = objects
                    .world
                    .weenie(o.id)
                    .is_some_and(|w| w.pwd.bitfield & 0x0000_0008 != 0);
                if is_player {
                    plain_players += 1;
                } else {
                    white += 1;
                    println!("radar: {session} {:?} took no colour of its own", o.id);
                }
            }
        }
        measured += 1;
    }
    println!(
        "radar: the recordings' blip colours are {all:?}; {plain_players} of them are ordinary players taking the plain one"
    );
    let enough_colours = all.len() >= 4;
    // The colour the shard's own marked traders take, which is what a real radar is mostly made
    // of.
    let yellow = all.contains_key(&YELLOW.hex);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players",
        move |_| measured >= 3 && white == 0 && plain_players > 0 && enough_colours && yellow,
    );
}

// -------------------------------------------------------------------------------------------
// radar.range.*
// -------------------------------------------------------------------------------------------

/// There are two radar ranges and one question decides between them: is the player inside?
///
/// The two distances are written out here as numbers rather than fetched through the same call
/// the client makes, so a wrong one is visible. The boundary is driven on both sides, and the
/// piece of land an id belongs to must not reach the question at all.
pub fn there_are_two_radar_ranges_and_one_question() {
    use pyreal_client_contract::radar::radar_range;
    use pyreal_physics::landdefs::is_outdoors;
    use pyreal_primitives::CellId;

    let outdoors_is_75 = (radar_range(true) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON;
    let indoors_is_25 = (radar_range(false) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;
    // The indoor one must be the smaller: that is the whole of the claim.
    let smaller = radar_range(false) < radar_range(true);

    let block = 0xA9B4_0000u32;
    let outside = [1u32, 0x20, 0x3F, 0x40, 0xFF]
        .into_iter()
        .all(|idx| is_outdoors(CellId(block | idx)));
    let inside = [world_support::ENV_CELL_FLOOR, 0x101, 0x1FF, 0xFFFF]
        .into_iter()
        .all(|idx| !is_outdoors(CellId(block | idx)));
    // A different piece of land, the same answers: the top half of an id is masked off first.
    let masked = is_outdoors(CellId(0x0001_0001)) && !is_outdoors(CellId(0x0001_0100));

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.there-are-two-and-one-question-about-the-players-own-room-decides",
        move |_| outdoors_is_75 && indoors_is_25 && smaller && outside && inside && masked,
    );
}

/// Every place a recording puts the player takes the range its own room asks for, and the
/// recordings between them offer both rooms.
pub fn every_recorded_place_takes_the_range_its_room_asks_for() {
    use pyreal_client_contract::radar::radar_range;
    use pyreal_client_contract::GameView as _;
    use pyreal_physics::landdefs::is_outdoors;

    let mut indoors = 0usize;
    let mut outdoors = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        for (what, station) in [("outdoor", st.outdoor), ("indoor", st.indoor)] {
            let Some((at, recorded, here)) = station else {
                continue;
            };
            let (objects, hud) = world_support::scene_at(session, at);
            let Some(cell) = hud.player_cell else {
                continue;
            };
            // The two passes must reproduce each other, or nothing below is about that room.
            ok &= cell == recorded;
            let view = hud.view(&objects);
            let outside = view.player_outside();
            // Two readings that could disagree: one through the display and its own view, one
            // straight off the recording's own room id.
            ok &= outside == is_outdoors(cell);
            ok &= outside == (what == "outdoor");
            let want = if outside {
                world_support::OUTDOOR_RANGE
            } else {
                world_support::INDOOR_RANGE
            };
            ok &= (radar_range(outside) - want).abs() < f32::EPSILON;
            if outside {
                outdoors += 1;
            } else {
                indoors += 1;
            }
            println!(
                "range: {session} {what}: room {:#010X}, {here} things, range {want}",
                cell.0
            );
        }
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for",
        move |_| indoors > 0 && outdoors > 0 && ok,
    );
}

/// A dungeon and the inside of an above-ground building are the same case.
///
/// The client has one question and cannot tell them apart; the shipped data agrees, because
/// every room in it is numbered above the boundary and both kinds take the short range. The
/// open land of the same two pieces of land is the control.
pub fn a_dungeon_and_a_building_inside_are_the_same_case() {
    use pyreal_assets::{world::LandblockInfo, Decode};
    use pyreal_client_contract::radar::radar_range;
    use pyreal_dat::DbType;
    use pyreal_physics::landdefs::is_outdoors;
    use pyreal_primitives::{CellId, DataId};

    let store = support::store();

    // Every room in the shipped data, grouped by the piece of land it belongs to.
    let mut rooms: std::collections::BTreeMap<u16, Vec<u32>> = std::collections::BTreeMap::new();
    let mut below_the_boundary = 0usize;
    for id in store.ids_of(DbType::Cell) {
        let idx = id.0 & 0xFFFF;
        if idx == 0xFFFF || idx == 0xFFFE {
            continue;
        }
        if idx < world_support::ENV_CELL_FLOOR {
            below_the_boundary += 1;
        }
        rooms
            .entry(u16::try_from(id.0 >> 16).expect("sixteen bits"))
            .or_default()
            .push(idx);
    }
    // The premise for the whole claim: the shipped numbering really does put every room above
    // the boundary, which is why one question suffices.
    let numbering = !rooms.is_empty() && below_the_boundary == 0;

    let buildings_of = |block: u16| -> Option<usize> {
        let id = DataId((u32::from(block) << 16) | 0xFFFE);
        let bytes = store.read_typed(DbType::Lbi, id).ok()?;
        LandblockInfo::decode_payload(id, &bytes)
            .ok()
            .map(|l| l.buildings.len())
    };

    let mut building_block = None;
    let mut dungeon_block = None;
    for (&block, cells) in &rooms {
        if cells.is_empty() {
            continue;
        }
        match buildings_of(block) {
            Some(n) if n > 0 && building_block.is_none() => building_block = Some(block),
            Some(0) if dungeon_block.is_none() => dungeon_block = Some(block),
            _ => {}
        }
        if building_block.is_some() && dungeon_block.is_some() {
            break;
        }
    }
    let bb = building_block.expect("some piece of land has both rooms and buildings on it");
    let db = dungeon_block.expect("some piece of land has rooms and no buildings");
    let building_room = CellId((u32::from(bb) << 16) | rooms[&bb][0]);
    let dungeon_room = CellId((u32::from(db) << 16) | rooms[&db][0]);

    let both_short = [building_room, dungeon_room].into_iter().all(|cell| {
        !is_outdoors(cell)
            && (radar_range(is_outdoors(cell)) - world_support::INDOOR_RANGE).abs() < f32::EPSILON
    });
    let same_case =
        (radar_range(is_outdoors(building_room)) - radar_range(is_outdoors(dungeon_room))).abs()
            < f32::EPSILON;
    // The control from the same two pieces of land, so this is not simply answering the short
    // range to everything.
    let control = [bb, db].into_iter().all(|block| {
        let land = CellId((u32::from(block) << 16) | 1);
        is_outdoors(land)
            && (radar_range(is_outdoors(land)) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
    });
    println!(
        "range: {} pieces of land carry rooms; a building's inside and a dungeon both take \
         the short range",
        rooms.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case",
        move |_| numbering && both_short && same_case && control,
    );
}

/// The range follows the player as he steps inside and out again.
///
/// One display, one world, and two places that differ only in the room the player is in -- so
/// nothing but the room can be what moves the range. And back out again in the same scenario,
/// because a change that only ever went one way would satisfy every step before it.
pub fn the_range_follows_the_player_in_and_out() {
    use pyreal_client_contract::radar::radar_range;
    use pyreal_client_contract::GameView as _;
    use pyreal_physics::landdefs::is_outdoors;
    use pyreal_primitives::CellId;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, _) = world_support::scene_at(session, st.busiest.0);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = pos.cell.0 & 0xFFFF_0000;

    let mut hud = pyreal_client_shell::hud::Hud::new();
    // No player at all: the client's own answer is "not outside", which takes the short range.
    let nobody = !hud.player_outside()
        && (radar_range(hud.player_outside()) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;

    let mut outdoor = pos;
    outdoor.cell = CellId(block | 0x0001);
    hud.sync(
        &objects,
        Some(pyreal_client_runtime::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let out_ok = hud.player_cell == Some(outdoor.cell) && hud.view(&objects).player_outside();
    let out_range = radar_range(hud.view(&objects).player_outside());

    let mut indoor = pos;
    indoor.cell = CellId(block | world_support::ENV_CELL_FLOOR);
    hud.sync(
        &objects,
        Some(pyreal_client_runtime::hud::ViewerFrame {
            position: indoor,
            heading_degrees: 0.0,
        }),
    );
    let in_ok = hud.player_cell == Some(indoor.cell) && !hud.view(&objects).player_outside();
    let in_range = radar_range(hud.view(&objects).player_outside());

    let ranges = (out_range - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
        && (in_range - world_support::INDOOR_RANGE).abs() < f32::EPSILON
        && out_range > in_range;

    hud.sync(
        &objects,
        Some(pyreal_client_runtime::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let back = hud.view(&objects).player_outside() && is_outdoors(outdoor.cell);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.follows-the-player-as-he-steps-inside-and-out-again",
        move |_| nobody && out_ok && in_ok && ranges && back,
    );
}

/// The chat window's own sweep for something to talk to uses the same radius as the radar.
pub fn the_chat_sweep_uses_the_same_radius() {
    use pyreal_physics::landdefs::is_outdoors;
    use pyreal_primitives::CellId;

    let (session, at, recorded) = world_support::a_recorded_indoor_place();
    let (objects, _) = world_support::scene_at(session, at);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = recorded.0 & 0xFFFF_0000;

    let mut counts = Vec::new();
    let mut agreed = true;
    for cell in [CellId(block | 0x0001), recorded] {
        let mut p = pos;
        p.cell = cell;
        let mut hud = pyreal_client_shell::hud::Hud::new();
        hud.sync(
            &objects,
            Some(pyreal_client_runtime::hud::ViewerFrame {
                position: p,
                heading_degrees: 0.0,
            }),
        );
        agreed &= hud.player_outside() == is_outdoors(cell);
        counts.push(
            hud.auto_target_world(&objects.world)
                .in_range_of_player
                .len(),
        );
    }
    let (out, inside) = (counts[0], counts[1]);
    println!(
        "range: the chat sweep found {out} things at the long range and {inside} at the short"
    );
    // Both must be more than nothing: an indoor zero would satisfy "fewer" for the wrong reason.
    let discriminating = out > 0 && inside > 0 && inside < out;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar",
        move |_| agreed && discriminating,
    );
}

// -------------------------------------------------------------------------------------------
// radar.lock.*
// -------------------------------------------------------------------------------------------

/// Whether the interface is locked is one bit of the player's own options, and it survives the
/// blob the shard sends it in.
pub fn the_lock_is_one_bit_of_the_players_options() {
    use pyreal_protocol::login::PlayerModule;

    /// The bit, as a number: reading it back through the client's own name would not notice a
    /// wrong one.
    const LOCK: u32 = 0x0100_0000;

    let bit = LOCK == 1 << 24
        // ...and it is not the bit another option uses, in another word entirely.
        && LOCK != 1 << 21;
    // A fresh character starts with the interface unlocked.
    let fresh = PlayerModule::DEFAULT_OPTIONS2 & LOCK == 0;

    let mut ok = bit && fresh;
    for want in [false, true] {
        let mut m = PlayerModule {
            options2: PlayerModule::DEFAULT_OPTIONS2,
            ..Default::default()
        };
        m.options2 = if want {
            m.options2 | LOCK
        } else {
            m.options2 & !LOCK
        };
        let mut hud = pyreal_client_shell::hud::Hud::new();
        hud.player_module = Some(m.clone());
        ok &= hud.lock_ui() == want;

        // And it survives the blob it is kept in. The word only reaches the wire when the blob
        // says it is there, which is why a bit that is never written comes back as the default.
        let m = PlayerModule {
            option_flags: 0x0040,
            spell_bars: vec![Vec::new()],
            ..m
        };
        let mut w = pyreal_protocol::Writer::body();
        m.write(&mut w).expect("the module packs");
        let bytes = w.into_inner();
        let mut r = pyreal_protocol::Reader::body(&bytes);
        let back = PlayerModule::read(&mut r).expect("and unpacks again");
        ok &= (back.options2 & LOCK != 0) == want;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options",
        move |_| ok,
    );
}
