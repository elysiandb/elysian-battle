# PR — #2 Git & build pipeline

**Branch:** `feat/#2-git-build-pipeline`
**Base:** `main`
**Closes:** #2

## Summary

- Implement `src/git.rs`: clone ElysianDB repo into `.battle/elysiandb/`, fetch refs, list remote branches/tags, checkout any ref (branch, tag, SHA), detect current ref for reporting.
- Implement `src/builder.rs`: Go build orchestration with `CGO_ENABLED=0`, `--no-build` skip logic, build duration reporting, and clear error surfacing.
- Wire both modules into `main.rs` pipeline (steps 2-6): clone/fetch happens before interactive version selection so users pick from real branches/tags.
- Enhance `cli.rs` interactive prompt to display actual remote branches and tags from the cloned repo.
- Add `tempfile` dev-dependency for builder unit tests.

## Files changed

| File | Change |
|------|--------|
| `src/git.rs` | New — repository clone, fetch, ref listing, checkout |
| `src/builder.rs` | New — Go build orchestration |
| `src/main.rs` | Integrate git + builder into pipeline, reorder steps |
| `src/cli.rs` | Enhanced `resolve_version_interactive` to accept real refs |
| `Cargo.toml` | Add `tempfile` dev-dependency |

## Test plan

- [ ] `cargo build` — compiles with zero warnings
- [ ] `cargo clippy` — no lints
- [ ] `cargo fmt -- --check` — clean
- [ ] `cargo test` — 10 tests pass (3 new: resolve_ref_latest, resolve_ref_branch, resolve_ref_tag + 1 builder skip test)
- [ ] Run `cargo run -- --version main` — clones repo, checks out main, builds binary
- [ ] Run `cargo run -- --version main --no-build` — skips build when binary exists
- [ ] Run `cargo run -- --version main --no-build` after deleting `.battle/bin/` — builds anyway since binary is missing
- [ ] Run `cargo run` (no --version) — interactive prompt shows real branches/tags from ElysianDB repo
- [ ] Run `cargo run -- --version nonexistent-ref` — clear error message about missing ref
