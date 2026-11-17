//! An orange 1 misdiscard that is not a Gentleman's discard (scala-bot #89), in the real game that was reported: hanab.live game
//! 2022819, `HGroup11`, Orange (6 Suits), 3 players (jabot3, jannisw, jabot4; the bot is jabot4).
//!
//! Turn 2: jabot4 clues 1 to jabot3, touching a y1 and two o1s. Orange plays from 5 down, so an o1 is the last orange card, not a
//! playable 1, but jabot3 cannot see its own cards. At turn 6 it discards one of the o1s (order 4). The discarder did not know
//! the card (it was only a "1" to it), so this cannot be a Gentleman's discard of an o1 to jabot4's chop: jabot4 then played its
//! o3 (order 13) at turn 8 in the game, and now would discard it.

use hanab_bot::basics::Variant;
use hanab_bot::basics::interp::DiscardInterp;
use hanab_bot::basics::{Game, PerformAction};
use hanab_bot::logger::{self, LogLevel};
use hanab_bot::settings::ConventionKind;
use hanab_bot::tools::game_data::{GameData, build_game};
use std::sync::Arc;

const EXPORT: &str = include_str!("fixtures/orange_2022819.json");

/// The game as the player in `seat` sees it, before the action number `turn` (0-based).
fn game_before(turn: usize, seat: usize) -> Game {
    logger::set_level(LogLevel::Off);
    let mut data = GameData::parse(&serde_json::from_str(EXPORT).unwrap()).expect("the export");
    data.actions.truncate(turn);
    let variant = Arc::new(Variant::with_shorts(
        2130,
        "Orange (6 Suits)",
        &["Red", "Yellow", "Green", "Blue", "Purple", "Orange"],
        &['r', 'y', 'g', 'b', 'p', 'o'],
    ));
    build_game(ConventionKind::HGroup(11), &data, variant, seat, true).expect("a game")
}

/// The seat of jabot4, the bot, and the orders in the game.
const JABOT_4: usize = 2;
const DISCARDED_O1: usize = 4;
const JABOT_4_O3: usize = 13;

/// jabot3 discards an o1 at turn 6; jabot4 sees that.
#[test]
fn a_discard_of_a_card_the_discarder_did_not_know_is_not_a_gentlemans_discard() {
    let game = game_before(7, JABOT_4);
    assert!(game.state.deck[DISCARDED_O1].identity.is_some());
    assert_ne!(
        game.last_move(),
        Some(DiscardInterp::GentlemansDiscard.into()),
        "read as a Gentleman's discard"
    );
}

/// So jabot4 does not act on a Gentleman's discard to its o3.
#[test]
fn it_does_not_play_or_discard_its_o3_after_the_misdiscard() {
    let action = game_before(8, JABOT_4).take_action();
    assert!(
        !matches!(
            action,
            PerformAction::Play { target: JABOT_4_O3 }
                | PerformAction::Discard { target: JABOT_4_O3 }
        ),
        "{action:?}"
    );
}
