# PR: Test runner & reporting framework (#5)

**Branch:** `feat/5-test-runner-reporting`
**Base:** `main`

## Summary

- Add `src/suites/mod.rs` — `TestSuite` trait (async_trait), result types (`TestResult`, `SuiteResult`, `PerformanceResult`, `BattleReport`), `TestStatus` enum (Passed/Failed/Skipped), known `BATTLE_ENTITIES` constant for cleanup, and `all_suites()` registry (empty until suite implementations land)
- Add `src/runner.rs` — `Runner` struct for sequential suite execution: cleanup between suites (`POST /reset` + `DELETE /api/{entity}` for each known battle_* entity), `--suite` filter support (flexible name matching), progress bar via `indicatif`, per-suite pass/fail summary during execution
- Add `src/report.rs` — text mode (colored tables via `tabled` + pass/fail summary + failed test details), JSON mode (serialized `BattleReport` to `.battle/reports/<timestamp>.json` + `latest.json` symlink), exit codes (0 = all pass, 1 = failures, 2 = infrastructure error)
- Update `src/main.rs` — integrate runner + report into the execution pipeline (steps 11-13), creating an authenticated `ElysianClient` for the runner; the pre-existing smoke test (ticket #4) stays as a pre-flight validation before the runner executes suites
- All `Duration` fields serialize as milliseconds (u64) in JSON for readability
- JSON report always written to disk regardless of output format; text report only shown in text mode

## Coherence with smoke tests (#4)

The smoke test added in #4 runs as a **pre-flight check** (step 10) that validates both clients work against the live instance. It uses `anyhow::ensure!` and fails hard on any error. The runner (step 11) starts a fresh authenticated session and performs its own cleanup before each suite, so leftover `battle_smoke` data from the smoke test is cleaned up automatically.

## Files changed

- `src/suites/mod.rs` (new) — trait, types, entity registry
- `src/runner.rs` (new) — orchestrator
- `src/report.rs` (new) — text + JSON reporting
- `src/main.rs` (modified) — integrate runner/report, add mod declarations

## Acceptance checklist

- [x] Runner executes suites sequentially with cleanup between each
- [x] `--suite crud,query` runs only specified suites (flexible name matching)
- [x] Terminal report shows colored pass/fail per suite with table
- [x] JSON report written to `.battle/reports/` with correct schema
- [x] `latest.json` symlink updated
- [x] Progress bar shows test execution progress
- [x] Exit codes match spec (0/1/2)
- [x] `cargo build` / `cargo clippy` / `cargo fmt --check` / `cargo test` all pass (28 tests)
