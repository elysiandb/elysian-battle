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

### Automated checks

- [x] `cargo build` compiles without errors and zero warnings
- [x] `cargo clippy` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] `cargo test` — 28 tests pass (includes unit tests for runner filter, report exit codes, JSON serialization, symlink creation)

### Manual testing: full pipeline in text mode

- [ ] Run `cargo run -- --version latest` (no `--suite` filter) and verify:
  - Smoke test (step 10) executes and prints `✓` lines for HTTP and TCP checks
  - Runner (step 11) prints `No test suites to run` (expected: no suites implemented yet)
  - Report (step 13) writes a JSON file to `.battle/reports/<timestamp>.json`
  - Symlink `.battle/reports/latest.json` points to that file
  - JSON contains fields `version`, `elysiandb_version`, `timestamp`, `suites: []`, `total_passed: 0`
  - Process exits with code 0 (`echo $?`)

### Manual testing: JSON report mode

- [ ] Run `cargo run -- --version latest --report json` and verify:
  - No colored table printed to terminal — only the JSON file path is displayed
  - JSON file is written with the same schema as text mode
  - `latest.json` symlink is updated to the new file

### Manual testing: --suite filter

- [ ] Run `cargo run -- --version latest --suite crud,query` and verify:
  - Runner reports no matching suites (none implemented yet), filter is silent
  - Report JSON contains `suites: []` and exit code is 0

### Manual testing: --keep-alive flag

- [ ] Run `cargo run -- --version latest --keep-alive` and verify:
  - After the runner, message `ElysianDB left running (--keep-alive) on port XXXX` is displayed
  - Report is still generated and written to disk
  - ElysianDB is still accessible: `curl http://127.0.0.1:XXXX/health` returns 200

### Manual testing: smoke test → runner coherence

- [ ] Verify that smoke test (step 10) and runner (step 11) coexist without conflict:
  - Smoke test creates/deletes `battle_smoke` entities and KV keys
  - Runner performs global cleanup before each suite (reset KV + delete all battle_* entities)
  - No errors or data conflicts between the two steps in the log output

### Manual testing: JSON report schema validation

- [ ] Open `.battle/reports/latest.json` and verify the structure:
  - All required keys present: `version`, `elysiandb_version`, `timestamp`, `suites`, `performance`, `total_passed`, `total_failed`, `total_skipped`, `total_duration`
  - `total_duration` is an integer in milliseconds (not a Duration object)
  - `suites` and `performance` are arrays (empty for now)
