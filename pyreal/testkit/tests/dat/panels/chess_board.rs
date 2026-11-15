use super::*;

// =============================================================================================
// minigame.*
//
// The chess game: the seat, the moves, and every chess message, on the model backend so that
// what the game says is read off the chat lines it composes.
//
// **No recording is the oracle here and that is measured**, by the first scenario below: all
// eleven of the messages a board game is played in are corpus zeros, so every one the shard says
// is built by this file in the writer order the reference server uses and fed as a real datagram
// through `Peer`. The gestures are the requests the board window raises.
// =============================================================================================

/// The player, and the two boards: a landblock-static guid is the shape a fixed piece of furniture
/// in the world has.
const CHESS_PLAYER: ObjectId = ObjectId(0x5000_0001);
const CHESS_BOARD: ObjectId = ObjectId(0x79DA_F100);
const CHESS_OTHER_BOARD: ObjectId = ObjectId(0x79DA_F200);

/// The channel the window's refusals go out on, the strip's and not a chat window's.
const NOTICE_CHANNEL: u8 = 0x1A;

/// A client with a shard behind it and two boards standing in front of it.
fn chess_a_client_and_two_boards() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::model();
    // The shard creates the player, because an ordered event is addressed to an object and the
    // client's ordered queue only has somewhere to put one for an object it has been told about.
    let peer = Peer::attach_creating(&mut c, CHESS_PLAYER);
    assert!(
        c.world_mut().set_player(CHESS_PLAYER),
        "the identity is adopted once"
    );
    c.world_mut()
        .weenie_mut(CHESS_PLAYER)
        .expect("the shard's create reached the world")
        .pwd
        .name = "Larktest".to_string();
    for id in [CHESS_BOARD, CHESS_OTHER_BOARD] {
        chess_put_a_board_in_the_world(&mut c, id);
    }
    c.tick(6);
    (c, peer)
}

/// A board as the shard describes one, and **every one of these is load-bearing** for a
/// double-click on it to come out as *begin a game* rather than *put it in your backpack*: it is a
/// game piece, it is stuck to the world, it is in no container and no hand, and it is not the
/// player's own.
fn chess_put_a_board_in_the_world(c: &mut HeadlessClient, id: ObjectId) {
    let mut w = pyreal_client_model::Weenie::default();
    w.pwd.obj_type = pyreal_rules::weenie::item_type::GAMEBOARD;
    w.pwd.bitfield |= pyreal_rules::weenie::bitfield::STUCK;
    w.pwd.name = format!("Chess Board {:X}", id.0);
    c.world_mut().tables.weenies.insert(id, w);
}

/// Use a board, as a double-click on it asks the client to.
fn chess_use_the_board(c: &mut HeadlessClient, id: ObjectId) {
    chess_ask(c, UiRequest::Use(id));
}

/// One request the board window raises, and the frames that read what it said.
fn chess_ask(c: &mut HeadlessClient, r: UiRequest) {
    c.when(Player::ui(r));
    c.tick(6);
}

/// One thing the shard says about a game, as a real ordered datagram, and the frames that read it.
fn chess_the_shard_says<M: pyreal_protocol::Message>(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    m: &M,
) {
    peer.event(c, m);
    c.tick(6);
}

fn chess_model(c: &HeadlessClient) -> &pyreal_client_model::minigame::MiniGame {
    &c.view().world().minigame
}

/// Every request about a game this client has put in its outbox, in order.
fn chess_wire(c: &HeadlessClient) -> Vec<pyreal_client_model::Request> {
    use pyreal_client_model::Request as R;
    c.view()
        .outbound()
        .iter()
        .filter(|r| {
            matches!(
                r,
                R::GameJoin(_)
                    | R::GameQuit(_)
                    | R::GameMove(_)
                    | R::GameMovePass(_)
                    | R::GameStalemate(_)
            )
        })
        .cloned()
        .collect()
}

/// The ones sent since `mark`, which is what a scenario takes before the gesture it is about.
fn chess_wire_since(c: &HeadlessClient, mark: usize) -> Vec<pyreal_client_model::Request> {
    chess_wire(c).into_iter().skip(mark).collect()
}

/// Every line the window has said on `keep`'s side of the notice channel, as one log.
fn chess_log(c: &HeadlessClient, keep: fn(u8) -> bool) -> String {
    c.view()
        .chat_lines()
        .iter()
        .filter(|m| keep(m.ty))
        .map(|m| m.body.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The last line of a log, trimmed.
fn last_line(log: &str) -> String {
    log.lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The last thing the window said in chat.
fn chess_last_said(c: &HeadlessClient) -> String {
    last_line(&chess_log(c, |ty| ty != NOTICE_CHANNEL))
}

/// The newest refusal on the notice channel. The two refusals a board's window makes go there and
/// **not** to a chat window.
fn chess_last_spewed(c: &HeadlessClient) -> String {
    last_line(&chess_log(c, |ty| ty == NOTICE_CHANNEL))
}

/// Press one square, as the board window asks for it.
fn chess_press_square(c: &mut HeadlessClient, cell: usize) {
    chess_ask(c, UiRequest::MiniGameBoardPress(cell));
}

/// Press one of the window's three buttons.
fn chess_press_button(c: &mut HeadlessClient, id: u32) {
    chess_ask(c, UiRequest::MiniGameButton(id));
}

/// Take a seat as `my_team` and let `first_team` have the first move.
///
/// The two are separate because the local engine always gives the first move to team 0, so a start
/// naming team 1 leaves a team-0 player waiting and a team-1 player to move. That is the shard's
/// arrangement, measured, and it is why no scenario here takes the second seat and then moves.
///
/// **This is a fixture and not a claim**: the seat request it sends is what
/// [`using_a_board_raises_the_window_and_asks_for_a_seat`] is about, so every caller takes its own
/// mark afterwards and never counts it.
fn chess_sit_down(c: &mut HeadlessClient, peer: &mut Peer, my_team: i32, first_team: i32) {
    chess_use_the_board(c, CHESS_BOARD);
    chess_the_shard_says(
        c,
        peer,
        &pyreal_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: my_team,
        },
    );
    chess_the_shard_says(
        c,
        peer,
        &pyreal_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: first_team,
        },
    );
}

/// The shard's *"the opponent moved"*, in the from-and-to form.
fn chess_opponent_turn(
    game: ObjectId,
    team: i32,
    from: (u32, u32),
    to: (u32, u32),
) -> pyreal_protocol::trade::GameOpponentTurn {
    pyreal_protocol::trade::GameOpponentTurn {
        game_id: game.0,
        team,
        move_data: pyreal_protocol::trade::GameMoveData {
            move_type: pyreal_protocol::trade::GameMoveData::FROM_TO,
            player: ObjectId(0),
            from: Some(from),
            to: Some(to),
            piece_index: None,
        },
    }
}

// ---------------------------------------------------------------------------------------------

/// **The denominator, measured rather than looked up.** Each of the six the shard can send is
/// delivered to both consumers as a real event and the client is asked whether either took an arm,
/// because a decoder that works proves nothing about whether anything calls it; each of the five
/// the client can send is checked to have a sender and to be addressed at the shard.
///
/// **Do not shorten either list**: they are the denominator, and a census that drops rows as they
/// are fixed cannot tell *closed* from *forgotten*.
pub(super) fn every_chess_message_reaches_a_receiver_or_a_sender() {
    use pyreal_protocol::Opcode;

    const INBOUND: [Opcode; 6] = [
        Opcode::GAME_JOIN_GAME_RESPONSE,
        Opcode::GAME_START_GAME,
        Opcode::GAME_MOVE_RESPONSE,
        Opcode::GAME_OPPONENT_TURN,
        Opcode::GAME_OPPONENT_STALEMATE_STATE,
        Opcode::GAME_GAME_OVER,
    ];
    const OUTBOUND: [Opcode; 5] = [
        Opcode::GAME_JOIN,
        Opcode::GAME_QUIT,
        Opcode::GAME_MOVE,
        Opcode::GAME_MOVE_PASS,
        Opcode::GAME_STALEMATE,
    ];

    let mut received = Vec::new();
    let mut dropped = Vec::new();
    for op in INBOUND {
        pyreal_client_runtime::dropped::clear();
        let mut blob = op.0.to_le_bytes().to_vec();
        blob.extend(std::iter::repeat_n(0_u8, 64));
        let events =
            [pyreal_client_net::client_session::SessionEvent::UiEvent { opcode: op, blob }];

        let mut world = pyreal_client_model::World::new();
        let mut hud = pyreal_client_shell::hud::Hud::new();
        hud.apply_events(&events, &mut world);

        let mut world = pyreal_client_model::World::new();
        let mut inter = pyreal_client_runtime::interaction::Interaction::new();
        pyreal_client_runtime::interaction::apply_events(&mut inter, &events, &mut world);

        if pyreal_client_runtime::dropped::unreceived(op) {
            dropped.push(op);
        } else {
            received.push(op);
        }
    }

    // The outbound five, discriminated by construction: each has a request of its own **and** a
    // sender arm, which is what "a sender exists" means here.
    let senders: Vec<(Opcode, pyreal_client_model::Request)> = vec![
        (
            Opcode::GAME_JOIN,
            pyreal_client_model::Request::GameJoin(Default::default()),
        ),
        (
            Opcode::GAME_QUIT,
            pyreal_client_model::Request::GameQuit(pyreal_protocol::trade::GameQuit),
        ),
        (
            Opcode::GAME_MOVE,
            pyreal_client_model::Request::GameMove(Default::default()),
        ),
        (
            Opcode::GAME_MOVE_PASS,
            pyreal_client_model::Request::GameMovePass(pyreal_protocol::trade::GameMovePass),
        ),
        (
            Opcode::GAME_STALEMATE,
            pyreal_client_model::Request::GameStalemate(Default::default()),
        ),
    ];
    let all_are_client_to_server = senders.iter().all(|(op, _)| {
        op.info().map(|i| i.direction) == Some(pyreal_protocol::opcodes::Direction::C2S)
    });
    let sender_count = senders.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender",
        move |_| {
            INBOUND.len() == 6
                && dropped.is_empty()
                && received.len() == 6
                && sender_count == 5
                && all_are_client_to_server
                && INBOUND.len() + OUTBOUND.len() == 11
        },
    );
    c.shutdown();
}

/// **Using the board.** No round trip raises this window: the board is raised by
/// the use itself, and the first thing it does is ask for a seat.
pub(super) fn using_a_board_raises_the_window_and_asks_for_a_seat() {
    let (mut c, _peer) = chess_a_client_and_two_boards();

    // Before: down.
    let built = !chess_model(&c).visible;
    let nothing_yet = chess_model(&c).begin_games == 0
        && chess_model(&c).current_game == ObjectId(0)
        && chess_wire(&c).is_empty();

    chess_use_the_board(&mut c, CHESS_BOARD);

    // The use reached the game-board arm **and** the window was asked for.
    let reached_the_window = c.view().interaction().stats.panels_requested == 1
        && c.view().interaction().stats.minigame_boards_used == 1
        && chess_model(&c).begin_games == 1;
    let the_window_is_up = chess_model(&c).visible
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).state == GameState::AttemptingToJoinGame;
    let asked_for_a_seat = chess_wire(&c)
        == vec![pyreal_client_model::Request::GameJoin(
            pyreal_protocol::trade::GameJoin {
                game_id: CHESS_BOARD.0,
                which_team: pyreal_client_model::minigame::JOIN_ANY_TEAM,
            },
        )];
    let said_so = chess_last_said(&c) == pyreal_client_model::minigame::ATTEMPTING_TO_JOIN.trim();

    // The two refusals, and that neither of them asks the shard for anything.
    let mark = chess_wire(&c).len();
    chess_use_the_board(&mut c, CHESS_BOARD);
    let same_board = chess_last_spewed(&c) == pyreal_client_model::minigame::ALREADY_THIS_GAME;
    // The client's own short throttle on using a thing is measured in its own clock, and six
    // headless frames do not always clear it: settling again is obeying the client's rule, not
    // making the scenario pass.
    c.tick(12);
    chess_use_the_board(&mut c, CHESS_OTHER_BOARD);
    let other_board = chess_last_spewed(&c) == pyreal_client_model::minigame::ALREADY_ANOTHER_GAME;
    let both_refused = chess_model(&c).joins_refused == 2;
    let nothing_more_was_asked = chess_wire_since(&c, mark).is_empty();
    let the_first_game_is_untouched = chess_model(&c).current_game == CHESS_BOARD;

    c.assert_behaviour(
        "minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat",
        move |_| {
            built
                && nothing_yet
                && reached_the_window
                && the_window_is_up
                && asked_for_a_seat
                && said_so
                && same_board
                && other_board
                && both_refused
                && nothing_more_was_asked
                && the_first_game_is_untouched
        },
    );
    c.shutdown();
}

/// **The round trip.** The seat deals the board, the start names whose turn it is -- both arms --
/// and a refused seat puts the window back to no game at all.
pub(super) fn the_seat_deals_the_board_and_the_start_names_whose_turn_it_is() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let reached_an_arm =
        c.view().hud().stats.minigame_events == 1 && c.view().hud().stats.minigame_guarded == 0;
    let dealt = chess_model(&c).team == 0
        && chess_model(&c).state == GameState::WaitingForGameStart
        && chess_model(&c).board.logic.pieces.len() == 32
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 0 })
            .map(|p| p.piece_type)
            == Some(PieceType::King);
    let said_joined = chess_last_said(&c) == pyreal_client_model::minigame::JOINED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let my_turn = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&c) == pyreal_client_model::minigame::BEGUN_YOUR_TURN.trim();
    c.shutdown();

    // The other arm of the same message: the shard says the *other* side moves first.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 1,
        },
    );
    let their_turn = chess_model(&c).state == GameState::PlayingNotMyTurn
        && chess_last_said(&c) == pyreal_client_model::minigame::BEGUN_THEIR_TURN.trim();
    c.shutdown();

    // And a seat the shard refuses.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: -1,
        },
    );
    let refused = chess_last_said(&c) == pyreal_client_model::minigame::COULD_NOT_JOIN.trim()
        && chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).current_game == ObjectId(0)
        && chess_model(&c).board.logic.pieces.is_empty();

    c.assert_behaviour(
        "minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away",
        move |_| reached_an_arm && dealt && said_joined && my_turn && their_turn && refused,
    );
    c.shutdown();
}

/// **The move.** Two presses, the request they make, the move the client's
/// own rules refuse where the player made it, and the move the shard refuses afterwards.
pub(super) fn two_presses_move_a_piece_and_a_refused_move_never_leaves() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square");

    chess_press_square(&mut c, from);
    let picked_up = chess_model(&c).board.selected == Some(ChessCoord { x: 4, y: 1 })
        && chess_wire_since(&c, mark).is_empty();

    chess_press_square(&mut c, to);
    let asked = chess_wire_since(&c, mark)
        == vec![pyreal_client_model::Request::GameMove(
            pyreal_protocol::trade::GameMove {
                x_from: 4,
                y_from: 1,
                x_to: 4,
                y_to: 3,
            },
        )];
    let on_its_way = chess_model(&c).state == GameState::PlayingTryingToMove
        && chess_model(&c).moves_sent == 1
        && chess_last_said(&c) == pyreal_client_model::minigame::MOVE_IN_PROGRESS;

    // Let the move stand and give the turn back, so the next press is the player's to make.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: 1,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 1, (4, 6), (4, 4)),
    );
    let our_turn_again = chess_model(&c).state == GameState::PlayingMyTurn;

    // A move the client's own rules refuse never reaches the shard -- the whole reason the rules
    // are in the client at all. The king cannot step on to its own pawn.
    let before = chess_model(&c).moves_sent;
    let mark = chess_wire(&c).len();
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 0 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 1 }, 0).expect("a square"),
    );
    let refused_here = chess_model(&c).moves_sent == before
        && chess_model(&c).moves_refused_locally == 1
        && chess_wire_since(&c, mark).is_empty()
        && chess_last_said(&c)
            == format!(
                "You cannot attack your own pieces{}{}",
                pyreal_client_model::minigame::TRY_AGAIN,
                pyreal_client_model::minigame::YOUR_TURN
            )
            .trim();
    c.shutdown();

    // And the shard's own refusal, which puts the piece back and gives the turn back.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square"),
    );
    let waiting = chess_model(&c).state == GameState::PlayingTryingToMove;
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: chess_mr::BAD_MOVE_NOT_YOUR_TURN,
        },
    );
    let undone = chess_model(&c).moves_undone == 1
        && chess_model(&c).state == GameState::PlayingMyTurn
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 1 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_last_said(&c) == pyreal_client_model::minigame::NOT_YOUR_TURN.trim();

    c.assert_behaviour(
        "minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves",
        move |_| {
            picked_up && asked && on_its_way && our_turn_again && refused_here && waiting && undone
        },
    );
    c.shutdown();
}

/// **The opponent's move, on the player's own board, laid out from the player's own side.**
pub(super) fn the_opponents_move_is_replayed_on_the_players_own_board() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    // The second seat, with the first side moving first, so the start leaves the player waiting.
    chess_sit_down(&mut c, &mut peer, 1, 0);
    let mark = chess_wire(&c).len();
    let seated = chess_model(&c).team == 1 && chess_model(&c).state == GameState::PlayingNotMyTurn;

    // From the second side the board is laid out the other way round: the far corner is the
    // last square.
    let corner = ChessBoard::cell_of_coord(ChessCoord { x: 0, y: 7 }, 1).expect("a square");
    let from_our_side = corner == 63;
    let before = chess_model(&c)
        .board
        .logic
        .at(ChessCoord { x: 3, y: 1 })
        .map(|p| p.piece_type)
        == Some(PieceType::Pawn)
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 3 })
            .is_none();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 0, (3, 1), (3, 3)),
    );

    let moved = chess_model(&c).opponent_moves == 1
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 3 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 1 })
            .is_none();
    let turn_came_back = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&c) == pyreal_client_model::minigame::YOUR_TURN.trim();
    let sent_nothing = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.the-opponents-move-is-replayed-on-the-players-own-board",
        move |_| seated && from_our_side && before && moved && turn_came_back && sent_nothing,
    );
    c.shutdown();
}

/// **The offer, and the end.** An offer of a draw and its withdrawal are both said in the window
/// and neither turns on the player's own offer -- agreeing is pressing your own button.
pub(super) fn the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 1,
        },
    );
    let offered = chess_last_said(&c) == pyreal_client_model::minigame::STALEMATE_OFFERED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 0,
        },
    );
    let retracted =
        chess_last_said(&c) == pyreal_client_model::minigame::STALEMATE_RETRACTED.trim();
    let both_counted = chess_model(&c).stalemate_offers == 2;
    let ours_is_untouched = !chess_model(&c).stalemate;

    // The player's side is the first, so the first side winning is the player winning.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &pyreal_protocol::trade::GameGameOver {
            game_id: CHESS_BOARD.0,
            team_winner: 0,
        },
    );
    let won = chess_last_said(&c)
        == format!(
            "{}{}",
            pyreal_client_model::minigame::VICTORIOUS,
            pyreal_client_model::minigame::DEFAULT_STATE
        )
        .trim();
    let put_away = chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).team == -1
        && chess_model(&c).current_game == ObjectId(0);

    c.assert_behaviour(
        "minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window",
        move |_| offered && retracted && both_counted && ours_is_untouched && won && put_away,
    );
    c.shutdown();
}

/// **The three buttons.** The draw button toggles and tells the shard the value it toggled *to*;
/// resigning asks first, and only Yes quits; and the button that passes a turn is never shown to
/// the player at all.
pub(super) fn the_window_buttons_offer_a_draw_and_ask_before_resigning() {
    use pyreal_client_model::minigame::button;

    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mut mark = chess_wire(&c).len();

    chess_press_button(&mut c, button::STALEMATE);
    let offered = chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![pyreal_client_model::Request::GameStalemate(
                pyreal_protocol::trade::GameStalemate { on: 1 },
            )];
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, button::STALEMATE);
    let withdrawn = !chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![pyreal_client_model::Request::GameStalemate(
                pyreal_protocol::trade::GameStalemate { on: 0 },
            )];

    // Resigning raises the question and sends nothing until it is answered.
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, button::RESIGN);
    let asked_first = chess_wire_since(&c, mark).is_empty()
        && chess_model(&c).resign_dialog != 0
        && chess_model(&c).resign_prompt_pending;
    let first = chess_model(&c).resign_dialog;

    // A second press while the question is up asks nothing more.
    chess_press_button(&mut c, button::RESIGN);
    let one_question_only =
        chess_model(&c).resign_dialog == first && chess_wire_since(&c, mark).is_empty();

    // No closes it, sends nothing, and lets it be asked again.
    chess_ask(&mut c, UiRequest::MiniGameQuitAnswer(false));
    let no_sends_nothing = chess_wire_since(&c, mark).is_empty()
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).resign_dialog == 0
        && !chess_model(&c).resign_prompt_pending;

    chess_press_button(&mut c, button::RESIGN);
    let a_fresh_question = chess_model(&c).resign_dialog != 0;
    chess_ask(&mut c, UiRequest::MiniGameQuitAnswer(true));
    let yes_quits = chess_wire_since(&c, mark)
        == vec![pyreal_client_model::Request::GameQuit(
            pyreal_protocol::trade::GameQuit,
        )]
        && chess_model(&c).current_game == ObjectId(0)
        && !chess_model(&c).visible;
    c.shutdown();

    // The pass button: never shown, and still a sender. With no game it says so and sends nothing;
    // with a game it sends the pass.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    let never_shown = !pyreal_client_model::minigame::MiniGame::PASS_BUTTON_STARTS_VISIBLE;
    let mark = chess_wire(&c).len();
    chess_press_button(&mut c, button::PASS);
    let no_game = chess_last_spewed(&c) == pyreal_client_model::minigame::NOT_PLAYING
        && chess_wire_since(&c, mark).is_empty();

    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();
    chess_press_button(&mut c, button::PASS);
    let passes = chess_wire_since(&c, mark)
        == vec![pyreal_client_model::Request::GameMovePass(
            pyreal_protocol::trade::GameMovePass,
        )];

    c.assert_behaviour(
        "minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning",
        move |_| {
            offered
                && withdrawn
                && asked_first
                && one_question_only
                && no_sends_nothing
                && a_fresh_question
                && yes_quits
                && never_shown
                && no_game
                && passes
        },
    );
    c.shutdown();
}

/// **The guard.** Every one of the six is guarded on which board it is about. A message about a
/// board this window did not sit down at must change nothing and say nothing -- and must still be
/// counted, so that "no traffic" and "all of it refused" are different answers.
pub(super) fn a_message_about_another_board_changes_nothing() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let before = chess_model(&c).state;
    let events_before = c.view().hud().stats.minigame_events;
    let said_before = chess_log(&c, |_| true);
    let mark = chess_wire(&c).len();

    let other = CHESS_OTHER_BOARD.0;
    peer.event(
        &mut c,
        &pyreal_protocol::trade::GameJoinGameResponse {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &pyreal_protocol::trade::GameStartGame {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &pyreal_protocol::trade::GameMoveResponse {
            game_id: other,
            result: 1,
        },
    );
    peer.event(
        &mut c,
        &chess_opponent_turn(CHESS_OTHER_BOARD, 1, (4, 6), (4, 4)),
    );
    peer.event(
        &mut c,
        &pyreal_protocol::trade::GameOpponentStalemateState {
            game_id: other,
            team: 1,
            on: 1,
        },
    );
    peer.event(
        &mut c,
        &pyreal_protocol::trade::GameGameOver {
            game_id: other,
            team_winner: 1,
        },
    );
    c.tick(6);

    let all_six_arrived = c.view().hud().stats.minigame_events - events_before == 6;
    let all_six_were_refused = c.view().hud().stats.minigame_guarded == 6;
    let nothing_moved = chess_model(&c).state == before
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).team == 0;
    let nothing_was_said = chess_log(&c, |_| true) == said_before;
    let nothing_was_sent = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.a-message-about-another-board-changes-nothing",
        move |_| {
            all_six_arrived
                && all_six_were_refused
                && nothing_moved
                && nothing_was_said
                && nothing_was_sent
        },
    );
    c.shutdown();
}
