# PR: HTTP & TCP clients (#4)

**Branch:** `feat/4-http-tcp-clients`
**Base:** `main`

## Summary

- Add `src/client.rs` — `ElysianClient` wrapping `reqwest::Client` with cookie jar enabled, covering every REST endpoint from SPEC section 7.8: entity CRUD (create, list, get, update, batch_update, delete, delete_all, count, exists), query, schema, auth/users, ACL, transactions, KV, hooks, migrations, import/export, and system endpoints
- Add `src/tcp_client.rs` — `ElysianTcpClient` over `tokio::net::TcpStream` with line-delimited protocol for KV commands: ping, set, set_ttl, get, mget, del, reset, save
- Both clients support cookie-based session auth (automatic after `login()`) and optional `Authorization: Bearer` token auth via `with_token()`
- All methods return `Result<Response>` / `Result<String>` — no deserialization, test suites handle that
- Register both modules in `main.rs`

## Acceptance checklist

- [x] All HTTP methods from SPEC section 7.8 implemented
- [x] Cookie jar handles `edb_session` automatically after login
- [x] Token auth works via `Authorization: Bearer` header (`with_token()`)
- [x] TCP client connects and implements full KV command set (PING/PONG, SET, GET, MGET, DEL, RESET, SAVE)
- [x] All methods compile and match the ElysianDB API reference
- [x] `cargo build` / `cargo clippy` / `cargo fmt` / `cargo test` all pass (18 tests)
