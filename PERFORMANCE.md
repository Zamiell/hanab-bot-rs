# Performance

How to measure, what the numbers were, and what is left. Issue numbers are on GitHub.

## Measuring

```
cargo build --release --bin benchmark
./target/release/benchmark                        # HGroup3, 3 players, 3 games from seed 314, 3 repetitions
./target/release/benchmark convention=Reactor1 players=4 games=2
CARGO_TARGET_DIR=/tmp/target-stats cargo run --release --features stats --bin benchmark   # also counts the work done
```

Every repetition plays the same seeded games, so the times can be compared between runs and between versions. There is no warm-up (no JIT or GC):
the first repetition is as fast as the rest. Run it on an otherwise idle machine and compare the minimum. The endgame solver is off by default
(`solver=on` to include it) because its time limit makes it depend on the machine.

Two numbers do not depend on the machine:

* **fingerprint**: a hash of every move of every game. A change that is meant to leave the bot's behaviour alone must leave it unchanged;
  a change that alters play changes it (then compare scores, not just times).
  The baseline fingerprint below is from before the port of scala-bot v1.3.1, which changed play on purpose: since then the benchmark
  prints `ce841dcd9198b831` (total score 67, 172 moves). Compare a refactor against that.
* **work counters** (`--features stats`): how many times the expensive functions ran per repetition. They compile to nothing without the
  feature and do not change behaviour (the fingerprint is the same with and without).

## Baseline

HGroup3, 3 players, 3 games from seed 314, solver off (commit of the benchmark tool):

| | per repetition |
|---|---|
| time (this development machine) | about 36.0 s |
| fingerprint | `25bf41aa68672330` |
| total score of the 3 games | 70 (170 moves, 167 decisions) |
| decisions (`take_action`) | 167 |
| `Game::elim` | 300,090 |
| `update_hypo_stacks` | 1,158,712 |
| hypothetical plays | 2,253,016 |
| simulated games (`simulate_clue` / `simulate_action`) | 106,174 |

That is about 636 simulated games and 6,900 `update_hypo_stacks` per decision.

## History

Self-play of 2 HGroup5 games, one thread: about 102 s at the start of M7, 52 s after skipping the game copies in hypothetical plays when nothing is waiting
(`uses_refresh_after_play`), 27 s after making `Game::clone` cheap (copy-on-write `Ext`, action list and notes), and about 22 s after skipping the inverted-suit
trash scan in `update_hypo_stacks` (all on a machine that was sometimes busy). About 9x faster than the Scala bot and about 50x less memory.

Benchmark (baseline above), after making the game copy in `update_hypo_stacks` cheaper: 36.0 s -> 30.1 s, fingerprint unchanged. `update_hypo_stacks` copies the whole
game for every call (about 14% of the run, 37% of `update_hypo_stacks` itself), so what a copy costs matters: the perspective being worked out is no longer copied twice,
player names are shared (`Arc<str>`, `Arc<Vec<String>>`) and the discard stacks are copy-on-write. The clone probe (`cargo test --release --test clone_probe -- --ignored --nocapture`)
shows what is left. Sharing `ConvData::reasoning` (copy-on-write) took it to 27.4 s. Making `State::deck` copy-on-write gained nothing measurable, so it was dropped. Sharing each player's `thoughts` (copy-on-write, `Player::thoughts_mut`) took it to 24.7 s. Left in a game copy: `State::hands`, `State::deck`, `meta` and the per-player maps.

Profile of the same run (inclusive timers, temporary): `Game::elim` 16.7 s of 28.7, of which `update_hypo_stacks` 12.0 (game copy 3.6, hypothetical plays 3.8, `thinks_playables` 2.4) and `card_elim` 2.4. `order_trash` is only about 0.5 s in total (1.3M calls), so caching it (idea 3) would not pay.

## Profile after the Scala v1.3.1 port (HGroup3 benchmark, 3 repetitions together, with timers around the pieces)

The port made the bot give fewer speculative clues, so the work per repetition halved (55,814 simulated games instead of 106,174) and the benchmark
takes 13.5 s here. Where the 42 s of the three repetitions went (inclusive; the pieces overlap):

| | time | share |
|---|---|---|
| `Game::elim` | 27.4 s | 65% |
| of which `update_hypo_stacks` | 20.3 s | 48% |
| ... the game copy for it (`clone_for_hypo`) | 5.4 s | 13% |
| ... `thinks_playables` | 2.8 s | 7% |
| ... hypothetical plays | 9.5 s | 23% |
| ... ... of which the copy of `prev` and `refresh_wcs` when connections are waiting | 8.7 s | 21% |
| `card_elim` of the other perspectives | 2.9 s | 7% |

Skipping the `refresh_wcs` copy when no waiting connection reacts to the hypothetical play (`refresh_wcs_changed` returns `None`) took the benchmark from 13.52 s to
12.75 s (-6%) with the same fingerprint. What is left of that branch is the copy of `prev` (2.6 s, which `update_wc` reads: hands, playable set, chop) and
evaluating every waiting connection against the play (3.5 s).

## Where the time goes

`Game::elim` is about 55% of self-play time, and `update_hypo_stacks` about 75% of that: `elim` runs it for the common perspective and once for
every player, each time replaying the hypothetical plays. Ideas, roughly by expected gain (see issues #14, #15):

1. Not computing a perspective's hypo stacks when it cannot differ from the common one. Tried: 92% of the per-player results do equal the common one, but there is
   no cheap test that proves it beforehand (players differ in what they see, `Thought::seen`, which `id_with` reads), so the result can't be skipped soundly.
   Copying less of the game for the hypothetical does not work either: `thinks_playables` reads the other players' views.
2. Fewer game copies in hypothetical plays when connections are waiting (the `prev` copy and `refresh_wcs`).
3. ~~Caching `order_trash` / `thinks_trash`~~: measured, about 2% of the run; not worth the invalidation risk.
4. ~~Searching the endgame in parallel~~: done, opt-in (`HANABI_ENDGAME_THREADS`, `EndgameSolver::with_threads`). The first moves are tried on separate threads and the best is picked in order, so the answer is the same for any thread count when the search finishes. On `it_completes_an_early_endgame` (3.3 s on one thread) 4 threads take 1.3 s; searches dominated by one move gain nothing.

## Late-game moves at 5 players (#113, #115)

At 5 players a few decisions in the last turns of a game (turns 47 to 51, when the deck is almost gone) take seconds to minutes, and they are most of the time of
a game: 8 games of `HGroup1` took 119 s, 54 s of it in three decisions (one of 44 s). `SELF_PLAY_SLOW_MS=3000 benchmark convention=HGroup1 players=5 games=8`
reports them (with `--features stats` it also prints the work counters of that one decision); game `i` is seed `314 + i`, so `seed=318 games=1` plays the 48 s one.

That decision made only 254 simulated games and 14,000 hypothetical plays, no more than others, but 99.5% of its time was the *own* time of `Player::card_elim`,
nothing it calls: 7,074 calls of 6.5 ms. Its search for sets of cards that pin identities down (`cross_elim`, the "sudoku" operation) enumerates subsets of the
candidate cards, and late in a 5-player game there are many cards with few possibilities: 252 million recursive steps in that one decision, 60 million of them
tight subsets that call `perform_cross_elim` (1.5 billion `update_map` calls), nearly all of which find nothing to eliminate. The search is a pure function of what it
reads (the hands, what each card could be, the identities visible, and the certain map), and when it finds nothing it changes nothing, so `card_elim` now remembers
the key of a search that found nothing, per player and in a small set shared by all players of the thread (8192 keys), and skips a search whose key it knows. In
debug builds (so in the tests) a skipped search is also run on a copy and asserted to find nothing.

| | before | after |
|---|---|---|
| the 48 s decision (`HGroup1` 5p, seed 318) | 46 s | 5.3 s |
| that game | 58 s | 11 s |
| `HGroup1` 5p, 8 games | 119 s | 58 s |
| `HGroup5` 5p, 8 games | 115 s | 83 s |
| `HGroup1` 5p, 3 games | 34.2 s | 24.4 s |
| `Reactor1` 4p, 4 games | 6.8 s | 5.7 s |
| `HGroup5` 4p, 5 games | 83.4 s | 79.5 s |
| 2 and 3 players (`HGroup3`, `HGroup5`, `HGroup8`, `HGroup11`, `HGroup13`, `Reactor1`, `RefSieve`) | | within noise |

The fingerprint is the same as before in every run listed (ten configurations from 2 to 5 players, with HGroup levels 1 to 13, Reactor and RefSieve, plus the two 5-player runs of 8 games and the seed 318 game), so the bot plays the same games there. About 3 in 4 searches
are skipped in the slow decision (the counters `cross_elim searches run` / `skipped`).

What is left of that decision (5 s) is the searches that run: they are as expensive as before. Ideas: cutting the enumeration of tight subsets short when they cannot change
anything outside them (it has to stay exactly the same search), or remembering what a search found as well as what it did not.

## A time budget for a move (#115)

`budget::set_default_move_timeout` (the bot: `HANABI_MOVE_TIMEOUT_MS`, default 5000; `benchmark move_timeout=<ms>`, default none) gives choosing a move a deadline. `Game::take_action`
sets it, and the cut-off of each convention's lookahead (`stop_lookahead` in H-Group, the same test in the Reactor and RefSieve `advance`) also stops once it has passed:
every action is still evaluated, only less deep, so the move is bounded and legal. With no budget nothing changes; a move that finishes in time is the same as without one.

Self-play at 5 players, 8 games from seed 314 (one process at a time: a wall-clock budget depends on how busy the machine is, so runs side by side on this 2-CPU container gave
different moves even at 3 players, and are not comparable):

| budget | `HGroup1`: time, total score | `HGroup5`: time, total score |
|---|---|---|
| none | 57.7 s, 187 | 81.2 s, 164 |
| 1000 ms | 53.7 s, 187 | 79.5 s, 164 (the same moves) |
| 250 ms | 48.6 s, 187 | 68.0 s, 170 |
| 50 ms | 45.3 s, 181 | 40.5 s, 151 |

A budget of a second or more costs nothing measurable here (the score is the same, and `HGroup5` plays the same moves); 250 ms is neutral; 50 ms starts to cost (-3%, -8%).
At 3 players (`HGroup3`, 6 games) a budget of 1000 ms changes some moves (a few take longer than a second) and the score by one (139 to 138).
The default is 5 s because it bounds a move on a slow machine and a debug build without changing the moves a desktop plays; an app that embeds the library has none unless it sets one.

## Depth of the lookahead at 5 players (#16)

At 5 or more players the lookahead stops after `budget::max_lookahead_5p()` plies (the default is 1: only the next player's move is modelled; `benchmark lookahead=<n>`,
and `budget::set_max_lookahead_5p`). What does a different depth do to the score? Self-play at 5 players on identical deals (paired), solver off, no time budget, one process at a
time, scores out of 25; the seeds are 314 (16 deals), 2000 (32) and 3000 (64). The first 16 deals suggested that 2 plies are better (+2.6 for `HGroup5`); 32 fresh deals did not
(-0.2), so the pooled numbers are the ones to read:

| | 0 plies | 1 ply (default) | 2 plies | 3 plies |
|---|---|---|---|---|
| `HGroup5`: mean score (se), games under 15, time per game | 22.80 (0.16), 1%, 4.5 s | 22.52 (0.32), 4%, 10.2 s | 22.77 (0.55), 4%, 27.2 s | |
| `HGroup1` (32 deals) | 22.50 (0.40), 2.7 s | 22.00 (0.73), 7.6 s | | |
| `HGroup11` (12 deals) | | 22.67 (1.05), 11.8 s | 22.33 (1.33), 34.2 s | |
| `RefSieve` (48 deals) | 18.75 (0.51), 0.9 s | 19.00 (0.57), 2.8 s | 20.02 (0.56), 14.1 s | 20.50 (0.98, 8 deals), 79 s |

Paired differences: `HGroup5` 1 ply minus 0: -0.29 (se 0.35, 112 deals); 2 minus 1: +0.73 (se 0.73, 48 deals). `RefSieve` 2 minus 0: +1.27 (se 0.76, t = 1.7), 2 minus 1: +1.0 (se 0.78).

In H-Group the depth makes no measurable difference to the score, and the cost grows about 2.5 to 6 times with each ply (a game takes 4.5 s at 0 plies and 27 s at 2). The default of 1 ply is
slower than 0 without a visible gain, and has more bad games (4% under 15 against 1%, which is not significant at this size). RefSieve may gain about a point from 2 plies, at 15 times the time of 1;
its sample is small, and 3 plies cost 79 s a game. The setting is shared by H-Group and RefSieve: choosing different depths would need one each.

The very slow moves of #113 and #115 come from this recursion, so 0 plies would also remove them at 5 players (with the speed-up and the time budget they are rare already).

## The endgame solver's time limit (#14)

The solver searches for 3 s per move (`HANABI_ENDGAME_TIMEOUT_MS`; `benchmark solver_timeout=<ms>`) and plays from what it has found when time is up. In self-play with the solver on, a few decisions
at the end of a game run into the limit: 5 decisions in 12 `HGroup3` games at 3 players, 3 in 8 `HGroup5` games at 4 players, 2 in 8 at 5 players. With a limit long enough for the search to finish, one of those
decisions takes 10 to 37 s for 6,000 to 8,600 positions (seeds 319 and 320 of `HGroup3`), so 2 to 4 ms a position: a search that finishes inside 3 s would need positions 4 to 12 times cheaper.

**It makes no measurable difference to the score.** Self-play with the 3 s limit against a limit that lets every search finish, on the same deals (paired):

| | deals | different score | time at 3 s | time at the long limit |
|---|---|---|---|---|
| `HGroup3`, 3 players (limit 60 s) | 24 | 0 | 109 s, 96 s | 176 s, 254 s |
| `Reactor1`, 3 players (60 s) | 16 | 1 (25 at 3 s, 24 long) | 56 s | 369 s |
| `HGroup5`, 4 players (30 s) | 8 | 1 (25 at 3 s, 24 long) | 130 s | 189 s |
| `HGroup11`, 3 players (30 s) | 8 | 0 | 85 s | 138 s |

The 3 s limit is as good as a complete search here, and up to 6.6 times cheaper; the two games that differ are better with the limit. So there is nothing to gain from a faster search for the score, only for the time per move.

Where a position's time goes (the long decision of seed 319, exclusive time per function, with timers): `update_hypo_stacks` 2.4 s (489,000 calls), `handle_action` 1.5 s, `interpret_clue` 1.4 s, `clone_for_hypo` 1.1 s,
`thinks_playables` 1.0 s, `card_elim` 0.9 s, `elim` 0.8 s, `refresh_wcs` 1.0 s, of 11.9 s. There is no one hotspot. Each position makes about 4 `simulate_action` calls (every candidate action goes through the
convention's whole machinery) and about 78 `update_hypo_stacks` calls. Skipping the hypothetical stacks inside the solver's simulations would not be exact: the clue and discard interpretations read
`hypo_plays` and `hypo_stacks` (`interpret_clue`, `special_clues`, `interpret_dc`), and the solver uses them to decide which clues are valid. A cache like the one for `cross_elim` would need a key that
covers every view of the game. More threads (`HANABI_ENDGAME_THREADS`) make a search finish in time more often, with the same answer.

Also seen in that scan, without the solver: a few decisions at turn 2 of 4-player `HGroup5` games take 3 to 4 s.

## The lookahead at 4 players (#131)

At 4 players some early decisions of `HGroup5` take 3 to 4 s with the solver off (seeds 315, 316 and 321 at turn 2, seed 319 at turn 8: 4 of 8 games). The lookahead is capped at 5 or more players
(`budget::max_lookahead_5p`) but not at 4. The slow one, seed 316 turn 2, makes 3,799 simulated games for 17 candidate actions (about 220 each), and its time is in `update_hypo_stacks` and the waiting connections of the
hypothetical plays: the same kind of cost as at 5 players, with the most clue options in the early game.

`budget::set_max_lookahead_4p` (`benchmark lookahead4=<n>`; the default is no limit, so nothing changes) caps it at 4 players too. Self-play at 4 players on identical deals (paired), solver off, one process at a
time; seeds 2000 and 3000. A cap of 2 plies removes every slow decision (none of the 8 seeds above has one), and costs:

| 4 players | no cap | cap 2 | cap 1 |
|---|---|---|---|
| `HGroup5`, 64 deals (cap 1: 32): mean score (se), time per game | 24.38 (0.13), 18.9 s | 24.12 (0.12), 7.8 s | 23.91 (0.18), 3.5 s |
| `RefSieve`, 24 deals | 22.75 (0.23), 22.0 s | 22.58 (0.25), 5.0 s | |
| `HGroup1`, 24 deals | 24.25 (0.18), 12.9 s | 23.17 (0.77), 4.8 s (one game of 6) | |

Paired differences against no cap: `HGroup5` cap 2 -0.25 (se 0.19, 64 deals), cap 1 -0.34 (se 0.29, 32 deals); `RefSieve` cap 2 -0.17 (se 0.30); `HGroup1` cap 2 -1.08 (se 0.73, one bad game).

So a cap of 2 makes a game 2.4 to 4.4 times faster and removes the slow decisions, at a possible cost of about a quarter of a point (not significant on its own; the first 32 deals showed none, the next 32 about half a point).
It is a trade, not a free speed-up like the one for `cross_elim` (#127), so the default stays: with the move budget (a move over 5 s is cut) and the 2 s the bot waits before every move anyway, the slow
early decisions cost a second or two of latency, and the bot would play slightly weaker for it. An app on a slow machine, where they cost more, can choose the cap.
