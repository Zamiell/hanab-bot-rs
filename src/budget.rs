//! A time budget for choosing a move.
//!
//! The evaluation of a move looks ahead through what the other players might do, which at 5 or more players can take seconds
//! to minutes late in a game. Past its budget the lookahead stops where it is (each action is still evaluated, only less
//! deep), so a move is bounded. Without a budget (the default) nothing changes: the moves are the same, and they do not
//! depend on the machine's speed.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use crate::clock::Instant;
use crate::{log_warn, stats};

/// The budget of every move, in milliseconds (0: none).
static DEFAULT_MS: AtomicU64 = AtomicU64::new(0);

/// How many plies the lookahead goes in games with more than 4 players (the full one never finishes there).
static MAX_LOOKAHEAD_5P: AtomicUsize = AtomicUsize::new(1);

/// How many plies the lookahead goes in games with 4 players (no limit by default).
static MAX_LOOKAHEAD_4P: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Changes how many plies the lookahead of a move goes in games with 5 or more players (process-wide; the default is 1).
pub fn set_max_lookahead_5p(plies: usize) {
    MAX_LOOKAHEAD_5P.store(plies, Ordering::Relaxed);
}

/// How many plies the lookahead goes in games with 5 or more players.
pub fn max_lookahead_5p() -> usize {
    MAX_LOOKAHEAD_5P.load(Ordering::Relaxed)
}

/// Changes how many plies the lookahead of a move goes in games with 4 players (process-wide; the default is no limit).
pub fn set_max_lookahead_4p(plies: usize) {
    MAX_LOOKAHEAD_4P.store(plies, Ordering::Relaxed);
}

/// Whether the lookahead stops at this many plies from us, in a game with this many players.
pub fn lookahead_stops(num_players: usize, offset: usize) -> bool {
    match num_players {
        0..=3 => false,
        4 => offset > MAX_LOOKAHEAD_4P.load(Ordering::Relaxed),
        _ => offset > max_lookahead_5p(),
    }
}

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Changes the time budget of the moves chosen from now on (process-wide). `None` takes it away.
pub fn set_default_move_timeout(timeout: Option<Duration>) {
    DEFAULT_MS.store(
        timeout.map_or(0, |t| (t.as_millis() as u64).max(1)),
        Ordering::Relaxed,
    );
}

/// The budget of a move, if there is one.
pub fn default_move_timeout() -> Option<Duration> {
    match DEFAULT_MS.load(Ordering::Relaxed) {
        0 => None,
        ms => Some(Duration::from_millis(ms)),
    }
}

/// Puts the deadline back when the budget's scope ends (also when it ends in a panic).
struct Restore(Option<Instant>);

impl Drop for Restore {
    fn drop(&mut self) {
        DEADLINE.with(|d| d.set(self.0));
    }
}

/// Runs `f` with a deadline `timeout` from now (the earlier one, inside another scope).
pub fn scope_with<R>(timeout: Option<Duration>, f: impl FnOnce() -> R) -> R {
    let Some(timeout) = timeout else {
        return f();
    };
    let now = Instant::now();
    let deadline = now + timeout;
    let previous = DEADLINE.with(Cell::get);
    let _restore = Restore(previous);
    DEADLINE.with(|d| d.set(Some(previous.map_or(deadline, |p| p.min(deadline)))));

    let result = f();
    if expired() {
        stats::count(stats::Counter::OverBudget);
        log_warn!(
            "the move went over its time budget of {} ms",
            timeout.as_millis()
        );
    }
    result
}

/// Runs `f` with the budget of a move (the default one, if there is one).
pub fn scope<R>(f: impl FnOnce() -> R) -> R {
    scope_with(default_move_timeout(), f)
}

/// Whether the budget of the move being chosen on this thread is spent.
pub fn expired() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|deadline| Instant::now() > deadline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookahead_is_capped_at_5_players_and_not_below() {
        // (The defaults: no limit up to 4 players, one ply at 5 or more.)
        assert!(!lookahead_stops(2, 99));
        assert!(!lookahead_stops(3, 99));
        assert!(!lookahead_stops(4, 99));
        assert!(!lookahead_stops(5, 1));
        assert!(lookahead_stops(5, 2));
        assert!(lookahead_stops(6, 2));
    }

    #[test]
    fn there_is_no_deadline_without_a_budget() {
        assert!(!expired());
        assert!(!scope_with(None, expired));
    }

    #[test]
    fn a_spent_budget_is_expired_inside_its_scope_only() {
        assert!(scope_with(Some(Duration::ZERO), || {
            std::thread::sleep(Duration::from_millis(2));
            expired()
        }));
        assert!(!expired());
    }

    #[test]
    fn a_generous_budget_is_not_spent() {
        assert!(!scope_with(Some(Duration::from_secs(3600)), expired));
    }

    #[test]
    fn the_earlier_deadline_wins_in_nested_scopes_and_is_restored() {
        scope_with(Some(Duration::from_secs(3600)), || {
            let inner = scope_with(Some(Duration::ZERO), || {
                std::thread::sleep(Duration::from_millis(2));
                expired()
            });
            assert!(inner);
            assert!(!expired(), "the outer deadline is back");
        });
    }

    #[test]
    fn the_deadline_is_put_back_after_a_panic() {
        let result = std::panic::catch_unwind(|| {
            scope_with(Some(Duration::ZERO), || panic!("in the scope"));
        });
        assert!(result.is_err());
        assert!(!expired());
    }
}
