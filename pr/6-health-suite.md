# PR: Suite — Health & System (5 tests) (#6)

**Branch:** `feat/6-health-suite`
**Base:** `main`

## Summary

- Add `src/suites/health.rs` — first test suite implementation with 5 tests (H-01 to H-05) covering all ElysianDB system endpoints: `/health`, `/stats`, `/config`, `/save`, and the `X-Elysian-Version` response header
- Update `src/suites/mod.rs` — register `HealthSuite` in `all_suites()`, add `mod health` declaration, update unit test to verify suite registration

## Tests implemented

| ID | Name | Action | Validation |
|----|------|--------|------------|
| H-01 | Health endpoint returns 200 | `GET /health` | Status 200 |
| H-02 | Stats returns valid JSON | `GET /stats` | Status 200, body has `keys_count`, `uptime_seconds`, `total_requests` |
| H-03 | Config returns current config | `GET /config` | Status 200, body has `Engine.Name: "internal"`, `Server.HTTP.Port` and `Server.TCP.Port` as numbers |
| H-04 | Force save succeeds | `POST /save` | Status 200 or 204 |
| H-05 | Version header present | `GET /health` | `X-Elysian-Version` header present and non-empty |

## Implementation notes

- ElysianDB returns config JSON with PascalCase keys (Go struct serialization), not the lowercase keys from `elysian.yaml` — tests use `/Engine/Name`, `/Server/HTTP/Port`, `/Server/TCP/Port` JSON pointers
- `POST /save` returns 204 (No Content) on success, not 200 — test accepts both
- Health suite has no setup/teardown (no test data needed)
- Each test function handles both success and error cases, never panics, returns `TestResult` with descriptive error messages

## Files changed

- `src/suites/health.rs` (new) — HealthSuite struct implementing TestSuite trait with 5 tests
- `src/suites/mod.rs` (modified) — register health suite, add module declaration, update unit test

## Acceptance checklist

### Automated checks

- [x] `cargo build` compiles without errors and zero warnings
- [x] `cargo clippy` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] `cargo test` — 28 tests pass (including `all_suites_includes_health`)

### Manual recette: full end-to-end with health suite

- [x] Run `./target/release/elysian-battle --version latest --suite health --report text` and verify:
  - Suite "Health & System" appears in results table with 5 passed, 0 failed
  - Summary line: `5 tests | 5 passed | 0 failed | 0 skipped | ALL PASSED`
  - Process exits with code 0

### Manual recette: standalone suite filtering

- [ ] Run with `--suite health` and verify only "Health & System" runs
- [ ] Run with `--suite crud` and verify "Health & System" does NOT run (filter excludes it)

### Manual recette: full pipeline (no filter)

- [ ] Run `./target/release/elysian-battle --version latest` without `--suite` and verify:
  - "Health & System" suite runs with 5/5 passed
  - JSON report in `.battle/reports/latest.json` contains the suite results

### Manual recette: JSON report validation

- [ ] Open `.battle/reports/latest.json` and verify:
  - `suites[0].name` is `"Health & System"`
  - `suites[0].tests` has 5 entries, all with `"status": "passed"`
  - Each test has `request` and `response_status` fields populated
