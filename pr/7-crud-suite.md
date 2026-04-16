# PR: Suite — Entity CRUD (24 tests) (#7)

**Branch:** `feat/7-crud-suite`
**Base:** `main`

## Summary

- Add `src/suites/crud.rs` — second test suite with 24 tests (C-01..C-24) covering every documented Entity CRUD endpoint of ElysianDB: create (single, custom ID, batch, empty body, invalid JSON), list (empty, all, limit, offset, sort asc/desc, projection, search), get (by ID, not-found), update (single field, nested field, batch), delete (by ID, delete-all), count, and exists (true/false).
- Add `create_raw(entity, body, content_type)` to `src/client.rs` — needed by C-05, which sends a malformed JSON body that cannot be expressed via `serde_json::Value`.
- Drop the `POST /reset` call from `cleanup_between_suites` in `src/runner.rs`. In the targeted ElysianDB versions, `/reset` wipes every KV key including the admin session and per-entity ACL grants; after that, even an explicitly re-logged-in admin gets `403 Access denied` on documents they own. Per-entity `DELETE` is sufficient for cleaning test data; KV-suite cleanup will be reintroduced inside the KV suite when that suite exists.
- Register `CrudSuite` in `src/suites/mod.rs`, add `battle_empty` to `BATTLE_ENTITIES` so the runner cleans it between suites, and update the `all_suites_includes_*` unit test.

## Tests implemented (Suite 2)

All 24 tests from `doc/test-scenarios.md` Suite 2:

| ID | Name | Validation |
|----|------|------------|
| C-01 | Create single document | Status 200, response has `id` and `title=Dune` |
| C-02 | Create with custom ID | Status 200, returned `id` equals `"custom-1"` |
| C-03 | Create batch | Status 200, response array has 3 items each with unique `id` |
| C-04 | Create with empty body | Accepts 400 OR 200 with generated `id` |
| C-05 | Create with invalid JSON | Status 400 |
| C-06 | List empty collection | Status 200, body is `[]` |
| C-07 | List returns all documents | Item count matches seed |
| C-08 | List with limit=2 | Exactly 2 items |
| C-09 | List with offset=1 | seed.len() − 1 items |
| C-10 | List with limit + offset | Exactly 2 items |
| C-11 | Sort `title` asc | Order: Anathem, Cryptonomicon, Dune, Snow Crash |
| C-12 | Sort `pages` desc | Pages are non-increasing |
| C-13 | Field projection `fields=title` | Each item has `title`, no `pages` |
| C-14 | Search `?search=Dune` | Returns at least one doc with `title=Dune` |
| C-15 | Get by ID | Status 200, body matches seed |
| C-16 | Get by ID — not found | Status 404 |
| C-17 | Update single field | `pages` updated to 500, `title` preserved |
| C-18 | Update nested field | `metadata.isbn` updated, top-level `title` preserved |
| C-19 | Batch update | Both updated docs reflect new `pages` |
| C-20 | Delete by ID | Delete returns 200/204, subsequent GET returns 404 |
| C-21 | Delete all | Delete returns 200/204, subsequent list is `[]` |
| C-22 | Count | After 5 inserts, response carries `count: 5` |
| C-23 | Exists — true | Status 200 |
| C-24 | Exists — false | Status 404 OR 200 with falsy body (handles `{"exists": false}`) |

## Implementation notes

- **Test isolation** — every test that depends on specific seed state calls a `reseed()` helper (`delete_all` + sequential `create`) so order does not couple cases together. Pure create-tests (C-01..C-05) only `delete_all` first; read-only edge cases (C-06, C-16, C-24) need no seed.
- **Standard seed** — `[Dune (412p), Anathem (932p), Cryptonomicon (918p), Snow Crash (470p)]`, deliberately chosen so ascending-by-title and descending-by-pages produce different visible orderings.
- **C-04 polymorphism** — the spec accepts either 200 (with generated id) or 400 for an empty body; the test verifies whichever response shape ElysianDB returns is internally consistent.
- **C-05 raw body** — `create_raw` was added to `ElysianClient` because malformed JSON cannot pass through `reqwest`'s `.json()` typed builder.
- **C-20 / C-21** — ElysianDB returns `204 No Content` for both `DELETE /api/{entity}/{id}` and `DELETE /api/{entity}`; the assertions accept both 200 and 204.
- **C-22 count tolerance** — accepts `{"count": N}` (documented form) and a bare integer (for forward-compat with older builds).
- **C-24 falsy body** — `is_falsy_exists_body()` parses the body as JSON and accepts: empty body, `false`, `0`, `null`, `{}`, or `{"exists": false}` (with or without whitespace). Falls back to a tolerant lowercase string comparison if JSON parsing fails.
- **Runner cleanup change** — `POST /reset` was removed from `cleanup_between_suites` because it permanently breaks ACL state for the rest of the session: admin can still log in and create docs (which receive `_elysiandb_core_username: admin`), but every subsequent read/update/delete returns `403 Access denied`. The fix makes per-entity `DELETE` the sole between-suite cleanup; this is sufficient for every functional suite and lets each suite manage its own KV/auth state explicitly when needed.

## Files changed

- `src/suites/crud.rs` (new) — `CrudSuite` implementing the `TestSuite` trait with 24 tests, helper functions `pass`/`fail`/`reseed`/`seed_error`/`is_falsy_exists_body`, and a `standard_seed()` constant.
- `src/client.rs` (modified) — adds `create_raw()` for sending raw POST bodies with explicit content types.
- `src/runner.rs` (modified) — `cleanup_between_suites` no longer calls `POST /reset`; `info` import dropped; comment explains the change.
- `src/suites/mod.rs` (modified) — registers `CrudSuite`, adds `battle_empty` to `BATTLE_ENTITIES`, updates the registration unit test.

## Acceptance checklist

### Automated checks

- [x] `cargo build` compiles cleanly
- [x] `cargo build --release` compiles cleanly
- [x] `cargo clippy --all-targets -- -D warnings` passes with zero warnings
- [x] `cargo fmt -- --check` passes
- [x] `cargo test` — 28 unit tests pass (including `all_suites_includes_health_and_crud`)

### Manual recette: standalone CRUD suite

- [x] `./target/release/elysian-battle --version latest --suite crud --report text` reports:
  - `Entity CRUD — 24/24 passed`
  - Summary line `24 tests | 24 passed | 0 failed | 0 skipped | ALL PASSED`
  - Process exits with code 0

### Manual recette: full pipeline (no filter)

- [x] `./target/release/elysian-battle --version latest --report text` reports:
  - `Health & System — 5/5 passed`
  - `Entity CRUD — 24/24 passed`
  - Summary `29 tests | 29 passed | 0 failed`

### Manual recette: standalone health suite still works

- [x] `./target/release/elysian-battle --version latest --suite health --report text` reports `Health & System — 5/5 passed` and 5 tests total — confirms removing `/reset` did not regress the health suite.

### Manual recette: JSON report shape

- [x] `--report json` writes `.battle/reports/latest.json`. `jq` confirms:
  - `total_passed: 29`, `total_failed: 0`
  - `suites[1].name == "Entity CRUD"`, `suites[1].tests | length == 24`
  - Every CRUD test entry has `status: "passed"`, plus `request` and `response_status` fields populated
