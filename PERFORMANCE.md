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
| 2 and 3 players (`HGroup3`, `HGroup5`, `HGroup8`, `HGroup11`, `HGroup13`, `Reactor1`, `RefSieve`) | | within noise |

The fingerprint of every configuration is the same as before (the 18 configurations above and the 5-player ones), so the bot plays the same games. About 3 in 4 searches
are skipped in the slow decision (the counters `cross_elim searches run` / `skipped`).

What is left of that decision (5 s) is the searches that run: they are as expensive as before. Ideas: cutting the enumeration of tight subsets short when they cannot change
anything outside them (it has to stay exactly the same search), or remembering what a search found as well as what it did not.
