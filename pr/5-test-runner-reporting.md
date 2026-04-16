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

- [x] `cargo build` compiles without errors
- [x] `cargo clippy` passes (no new warnings)
- [x] `cargo fmt --check` passes
- [x] `cargo test` — 28 tests pass (includes unit tests for runner filter, report exit codes, JSON serialization, symlink creation)

### Recette : pipeline complète (text mode)

- [ ] Lancer `cargo run -- --version latest` (sans `--suite`) — vérifier que :
  - Le smoke test (step 10) s'exécute et affiche les `✓` HTTP/TCP
  - Le runner (step 11) affiche `No test suites to run` (attendu : aucune suite implémentée)
  - Le report (step 13) écrit un JSON dans `.battle/reports/<timestamp>.json`
  - Le symlink `.battle/reports/latest.json` pointe vers ce fichier
  - Le JSON contient les champs `version`, `elysiandb_version`, `timestamp`, `suites: []`, `total_passed: 0`
  - Le process exit avec code 0

### Recette : JSON mode

- [ ] Lancer `cargo run -- --version latest --report json` — vérifier que :
  - Pas de table colorée dans le terminal, uniquement le chemin du JSON affiché
  - Le fichier JSON est bien écrit avec le même schéma
  - Le symlink `latest.json` est mis à jour

### Recette : filtre --suite

- [ ] Lancer `cargo run -- --version latest --suite crud,query` — vérifier que :
  - Le runner n'affiche aucune suite (aucune implémentée) et le filtre est silencieux
  - Le report JSON contient `suites: []` et exit code 0

### Recette : --keep-alive

- [ ] Lancer `cargo run -- --version latest --keep-alive` — vérifier que :
  - Après le runner, le message `ElysianDB left running (--keep-alive) on port XXXX` s'affiche
  - Le report est quand même généré
  - ElysianDB reste accessible (`curl http://127.0.0.1:XXXX/health`)

### Recette : cohérence smoke test → runner

- [ ] Vérifier que le smoke test (step 10) et le runner (step 11) cohabitent :
  - Le smoke test crée/supprime `battle_smoke` et des clés KV
  - Le runner fait un cleanup global avant chaque suite (reset KV + delete entities)
  - Aucune erreur ou conflit entre les deux étapes

### Recette : validation du schéma JSON du report

- [ ] Ouvrir le fichier `.battle/reports/latest.json` et vérifier la structure :
  - Clés présentes : `version`, `elysiandb_version`, `timestamp`, `suites`, `performance`, `total_passed`, `total_failed`, `total_skipped`, `total_duration`
  - `total_duration` est un entier en millisecondes (pas un objet Duration)
  - `suites` et `performance` sont des arrays (vides pour l'instant)
