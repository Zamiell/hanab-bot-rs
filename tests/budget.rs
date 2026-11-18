//! The time budget of a move: with none the move is what it always was, with a generous one the same, and with a spent one the
//! bot still plays a legal move. (Positions from a real game, under each of the three conventions.)

use std::time::Duration;

use hanab_bot::basics::{Game, PerformAction};
use hanab_bot::budget;
use hanab_bot::logger::{self, LogLevel};
use hanab_bot::settings::ConventionKind;
use hanab_bot::tools::game_data::{GameData, build_game, load_variant};
use std::collections::HashMap;

const EXPORT: &str = include_str!("fixtures/cascade_2065891.json");

fn position(convention: ConventionKind, turn: usize) -> Game {
    logger::set_level(LogLevel::Off);
    let mut data = GameData::parse(&serde_json::from_str(EXPORT).unwrap()).expect("the export");
    data.actions.truncate(turn);
    let variant = load_variant("No Variant", &HashMap::new()).unwrap();
    // (The player to move at that turn: the game has 4 players.)
    build_game(convention, &data, variant, turn % 4, true).expect("a game")
}

/// Whether the action is one that the player could take.
fn legal(game: &Game, action: PerformAction) -> bool {
    let state = &game.state;
    let our = state.our_player_index;
    match action {
        PerformAction::Play { target } | PerformAction::Discard { target } => {
            state.hands[our].contains(&target)
        }
        PerformAction::Rank { target, .. } | PerformAction::Colour { target, .. } => {
            target != our && target < state.num_players && state.can_clue()
        }
        _ => true,
    }
}

const CONVENTIONS: [ConventionKind; 3] = [
    ConventionKind::HGroup(5),
    ConventionKind::Reactor,
    ConventionKind::RefSieve,
];
const TURNS: [usize; 5] = [4, 9, 12, 16, 17];

#[test]
fn a_generous_budget_gives_the_same_move_as_none() {
    for convention in CONVENTIONS {
        for turn in TURNS {
            let game = position(convention, turn);
            let none = game.take_action();
            let generous =
                budget::scope_with(Some(Duration::from_secs(3600)), || game.take_action());
            assert_eq!(none, generous, "{convention} at turn {turn}");
        }
    }
}

#[test]
fn a_spent_budget_still_gives_a_legal_move() {
    for convention in CONVENTIONS {
        for turn in TURNS {
            let game = position(convention, turn);
            let action = budget::scope_with(Some(Duration::ZERO), || game.take_action());
            assert!(
                legal(&game, action),
                "{convention} at turn {turn}: {action:?}"
            );
        }
    }
}

#[test]
fn the_default_budget_is_the_one_a_move_has() {
    // (The only test that sets it: it is process-wide.)
    assert_eq!(budget::default_move_timeout(), None);
    budget::set_default_move_timeout(Some(Duration::from_secs(5)));
    assert_eq!(budget::default_move_timeout(), Some(Duration::from_secs(5)));
    budget::set_default_move_timeout(None);
    assert_eq!(budget::default_move_timeout(), None);
}
