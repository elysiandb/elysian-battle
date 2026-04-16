# PR: Instance lifecycle & config generation (#3)

**Branch:** `feat/3-instance-lifecycle`
**Base:** `main`

## Summary

- Add `src/config.rs` — generates `.battle/config/elysian.yaml` matching SPEC section 9 (serde_yaml serialization, all fields: store, engine, server, log, stats, security, api, adminui)
- Add `src/instance.rs` — manages the ElysianDB process lifecycle: spawn with stdout/stderr redirected to `.battle/logs/elysiandb.log`, poll `GET /health` (30s timeout, 500ms interval), graceful shutdown (SIGTERM + 5s timeout then SIGKILL), and `kill_hard()` for crash recovery tests
- Wire steps 7–11 into `main.rs`: generate config, start instance, stop on exit (or keep alive with `--keep-alive`)
- Add `nix` crate for POSIX signal support (SIGTERM)

## Acceptance checklist

- [x] Generated YAML matches SPEC section 9
- [x] ElysianDB starts and passes health check
- [x] Logs captured to `.battle/logs/elysiandb.log`
- [x] Graceful shutdown works (SIGTERM)
- [x] Forceful kill works (SIGKILL) for crash recovery suite
- [x] Health check timeout produces helpful error with log tail
- [x] `cargo build` / `cargo clippy` / `cargo fmt` / `cargo test` all pass (13 tests)
