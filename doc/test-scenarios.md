# Elysian-Battle — Test Scenarios Catalog

> Complete list of every test case executed by elysian-battle.
> Each test has a unique ID for traceability in reports.

---

## Conventions

- **Entity names** are prefixed with `battle_` to avoid collisions.
- **Auth context**: unless stated otherwise, tests run as the default `admin` user (full permissions).
- **Expected** column describes the assertion, not just the status code.
- **Tags**: `[happy]` = expected success path, `[error]` = expected error/rejection, `[edge]` = boundary condition.

---

## Suite 1: Health & System (`health`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| H-01 | Health endpoint returns 200 | `GET /health` | Status 200 | [happy] |
| H-02 | Stats returns valid JSON | `GET /stats` | Status 200, body contains `keys_count`, `uptime_seconds`, `total_requests` | [happy] |
| H-03 | Config returns current config | `GET /config` | Status 200, body contains `engine.name: "internal"` and correct ports | [happy] |
| H-04 | Force save succeeds | `POST /save` | Status 200 | [happy] |
| H-05 | Version header present | `GET /health` | Response has `X-Elysian-Version` header with non-empty value | [happy] |

---

## Suite 2: Entity CRUD (`crud`)

### Setup
Create entity type `battle_books` by inserting a seed document.

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| C-01 | Create single document | `POST /api/battle_books` `{"title":"Dune","pages":412}` | Status 200, response has `id` (UUID format), `title`=`"Dune"` | [happy] |
| C-02 | Create with custom ID | `POST /api/battle_books` `{"id":"custom-1","title":"Custom"}` | Status 200, `id`=`"custom-1"` | [happy] |
| C-03 | Create batch | `POST /api/battle_books` `[{"title":"A"},{"title":"B"},{"title":"C"}]` | Status 200, array of 3 objects each with unique `id` | [happy] |
| C-04 | Create with empty body | `POST /api/battle_books` `{}` | Status 200 (empty object with generated `id`) or 400 | [edge] |
| C-05 | Create with invalid JSON | `POST /api/battle_books` `{invalid}` | Status 400 | [error] |
| C-06 | List empty collection | `GET /api/battle_empty` | Status 200, empty array `[]` | [happy] |
| C-07 | List returns all documents | `GET /api/battle_books` | Status 200, array length matches inserted count | [happy] |
| C-08 | List with limit | `GET /api/battle_books?limit=2` | Status 200, exactly 2 items | [happy] |
| C-09 | List with offset | `GET /api/battle_books?offset=1` | Status 200, first item skipped | [happy] |
| C-10 | List with limit + offset | `GET /api/battle_books?limit=2&offset=1` | Status 200, correct 2-item window | [happy] |
| C-11 | List sorted ascending | `GET /api/battle_books?sort[title]=asc` | Items in alphabetical order | [happy] |
| C-12 | List sorted descending | `GET /api/battle_books?sort[pages]=desc` | Items in descending numeric order | [happy] |
| C-13 | List with field projection | `GET /api/battle_books?fields=title` | Only `title` and `id` in response (no `pages`) | [happy] |
| C-14 | List with search | `GET /api/battle_books?search=Dune` | Returns documents matching "Dune" | [happy] |
| C-15 | Get by ID | `GET /api/battle_books/{id}` | Status 200, correct document | [happy] |
| C-16 | Get by ID — not found | `GET /api/battle_books/nonexistent-id` | Status 404 | [error] |
| C-17 | Update single field | `PUT /api/battle_books/{id}` `{"pages":500}` | Status 200, `pages`=500, `title` unchanged | [happy] |
| C-18 | Update nested field | Create doc with nested object, `PUT` with nested path update | Nested field updated, other fields preserved | [happy] |
| C-19 | Batch update | `PUT /api/battle_books` `[{"id":"...","pages":999},{"id":"...","pages":888}]` | Both documents updated | [happy] |
| C-20 | Delete by ID | `DELETE /api/battle_books/{id}` then `GET` same ID | Delete returns 200, subsequent GET returns 404 | [happy] |
| C-21 | Delete all | `DELETE /api/battle_books` then `GET /api/battle_books` | Delete returns 200, list returns empty array | [happy] |
| C-22 | Count | Insert 5 docs, `GET /api/battle_books/count` | `{"count": 5}` (or matches inserted count) | [happy] |
| C-23 | Exists — true | `GET /api/battle_books/{existing_id}/exists` | Status 200, truthy response | [happy] |
| C-24 | Exists — false | `GET /api/battle_books/nonexistent-id/exists` | Status 404 or falsy response | [error] |

---

## Suite 3: Query API (`query`)

### Setup
Insert 20 `battle_articles` with varied fields: `title` (string), `status` ("draft"/"published"), `views` (number 0-1000), `tags` (array of strings), `category` (string), `metadata.source` (nested string).

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| Q-01 | Simple eq filter | `POST /api/query` `{"entity":"battle_articles","filters":{"and":[{"status":{"eq":"published"}}]}}` | Only published articles returned | [happy] |
| Q-02 | neq filter | `filters: {"and":[{"status":{"neq":"draft"}}]}` | No drafts in results | [happy] |
| Q-03 | lt filter (numeric) | `filters: {"and":[{"views":{"lt":100}}]}` | Only articles with views < 100 | [happy] |
| Q-04 | lte filter | `filters: {"and":[{"views":{"lte":100}}]}` | Articles with views <= 100 | [happy] |
| Q-05 | gt filter | `filters: {"and":[{"views":{"gt":500}}]}` | Articles with views > 500 | [happy] |
| Q-06 | gte filter | `filters: {"and":[{"views":{"gte":500}}]}` | Articles with views >= 500 | [happy] |
| Q-07 | contains (string) | `filters: {"and":[{"title":{"contains":"Rust"}}]}` | Titles containing "Rust" | [happy] |
| Q-08 | contains (array) | `filters: {"and":[{"tags":{"contains":"backend"}}]}` | Articles where tags include "backend" | [happy] |
| Q-09 | not_contains | `filters: {"and":[{"tags":{"not_contains":"deprecated"}}]}` | Articles without "deprecated" tag | [happy] |
| Q-10 | all operator | `filters: {"and":[{"tags":{"all":"backend,api"}}]}` | Articles with BOTH tags | [happy] |
| Q-11 | any operator | `filters: {"and":[{"tags":{"any":"frontend,mobile"}}]}` | Articles with at least one tag | [happy] |
| Q-12 | none operator | `filters: {"and":[{"tags":{"none":"legacy,deprecated"}}]}` | Articles with NEITHER tag | [happy] |
| Q-13 | Glob pattern `*mid*` | `filters: {"and":[{"title":{"eq":"*Rust*"}}]}` | Wildcard match on title | [happy] |
| Q-14 | AND compound filter | `{"and":[{"status":{"eq":"published"}},{"views":{"gt":100}}]}` | Both conditions met | [happy] |
| Q-15 | OR filter | `{"or":[{"status":{"eq":"draft"}},{"views":{"gt":900}}]}` | Either condition met | [happy] |
| Q-16 | Nested AND/OR | `{"and":[{"status":{"eq":"published"}},{"or":[{"views":{"gt":500}},{"category":{"eq":"tech"}}]}]}` | Complex logic correct | [happy] |
| Q-17 | Sort + filter + limit | Published articles sorted by views desc, limit 5 | Top 5 most viewed published articles | [happy] |
| Q-18 | countOnly | `{"entity":"battle_articles","countOnly":true}` | `{"count": N}` | [happy] |
| Q-19 | Nested field filter | `filters: {"and":[{"metadata.source":{"eq":"rss"}}]}` | Matches nested path | [happy] |
| Q-20 | Empty result set | Filter that matches nothing | Status 200, empty array | [happy] |

---

## Suite 4: URL Query Parameters (`query_params`)

### Setup
Reuse `battle_articles` data from Query suite.

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| QP-01 | filter[field][eq] | `GET /api/battle_articles?filter[status][eq]=published` | Only published | [happy] |
| QP-02 | filter[field][gt] | `GET /api/battle_articles?filter[views][gt]=500` | Views > 500 | [happy] |
| QP-03 | sort[field]=asc | `GET /api/battle_articles?sort[views]=asc` | Ascending by views | [happy] |
| QP-04 | sort[field]=desc | `GET /api/battle_articles?sort[title]=desc` | Descending by title | [happy] |
| QP-05 | fields projection | `GET /api/battle_articles?fields=title,status` | Only title, status, id | [happy] |
| QP-06 | search | `GET /api/battle_articles?search=Rust` | Full-text results | [happy] |
| QP-07 | countOnly | `GET /api/battle_articles?countOnly=true` | Count JSON | [happy] |
| QP-08 | Combined | `?filter[status][eq]=published&sort[views]=desc&limit=3&fields=title` | Correct combined result | [happy] |

---

## Suite 5: Nested Entities (`nested`)

### Setup
None — creates entities inline.

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| N-01 | Create with @entity | `POST /api/battle_posts` `{"title":"Post","author":{"@entity":"battle_authors_nested","fullname":"Alice"}}` | Post created, author created separately, post.author = `{"@entity":"battle_authors_nested","id":"..."}` | [happy] |
| N-02 | Deep nesting (3 levels) | Post → Author → Job (each with `@entity`) | All 3 entities created, references linked | [happy] |
| N-03 | @entity with existing ID | Create author first, then post referencing author's ID | Links to existing author, no duplicate | [happy] |
| N-04 | includes expands nested | `GET /api/battle_posts/{id}?includes=battle_authors_nested` | Full author object embedded in response | [happy] |
| N-05 | includes=all | `GET /api/battle_posts/{id}?includes=all` | Recursive expansion of all linked entities | [happy] |
| N-06 | Array of nested entities | Post with `comments: [{"@entity":"battle_comments","text":"..."},...]` | All comments created as separate entities | [happy] |

---

## Suite 6: Schema Validation (`schema`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| S-01 | Auto-inferred schema | Insert `{"title":"X","pages":10}` into `battle_schema_auto`, `GET /api/battle_schema_auto/schema` | Schema with `title: string, pages: number` | [happy] |
| S-02 | Type mismatch rejected | Insert `{"title":123}` into same entity | Status 400 (title should be string) | [error] |
| S-03 | Set manual schema (shorthand) | `PUT /api/battle_schema_manual/schema` `{"fields":{"name":"string","age":"number"}}` | Schema set, `_manual: true` | [happy] |
| S-04 | Set manual schema (full) | `PUT` with `{"fields":{"name":{"type":"string","required":true}}}` | Required flag stored | [happy] |
| S-05 | Strict rejects new field | Set manual schema via `PUT /api/{entity}/schema` (makes `_manual: true`), then insert doc with undeclared field | Status 400 — strict enforcement is per-entity via manual schema, not the global `api.schema.strict` flag | [error] |
| S-06 | Required field missing | Manual schema with required field, insert without it | Status 400 | [error] |
| S-07 | Create entity type (shorthand) | `POST /api/battle_typed/create` `{"fields":{"x":"string"}}` | Type created | [happy] |
| S-08 | Create entity type (full) | `POST /api/battle_typed2/create` with full field defs | Type created with required flags | [happy] |
| S-09 | List entity types | `GET /api/entity/types` | Contains all created types | [happy] |
| S-10 | List entity type names | `GET /api/entity/types/name` | Array of name strings | [happy] |

---

## Suite 7: Authentication (`auth`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| A-01 | Unauthenticated request | `GET /api/battle_books` with no auth | Status 401 | [error] |
| A-02 | Token auth (valid) | `Authorization: Bearer battle-test-token-2026` | Status 200 | [happy] |
| A-03 | Token auth (invalid) | `Authorization: Bearer wrong-token` | Status 401 | [error] |
| A-04 | Login default admin | `POST /api/security/login` `{"username":"admin","password":"admin"}` | Status 200, `edb_session` cookie set | [happy] |
| A-05 | Session cookie works | GET request with session cookie | Status 200 | [happy] |
| A-06 | Get /me | `GET /api/security/me` | `{"username":"admin","role":"admin"}` | [happy] |
| A-07 | Create user | `POST /api/security/user` `{"username":"battle_user","password":"pass123","role":"user"}` | Status 200, user created | [happy] |
| A-08 | List users | `GET /api/security/user` | Contains `admin` and `battle_user` | [happy] |
| A-09 | Get user by name | `GET /api/security/user/battle_user` | Returns user info | [happy] |
| A-10 | Change password | `PUT /api/security/user/battle_user/password` `{"password":"newpass"}`, then login with new password | Login succeeds with new password | [happy] |
| A-11 | Change role | `PUT /api/security/user/battle_user/role` `{"role":"admin"}` | Role updated | [happy] |
| A-12 | Logout | `POST /api/security/logout` then access API | Logout returns 200, subsequent request returns 401 | [happy] |
| A-13 | Delete user | `DELETE /api/security/user/battle_user` | User removed | [happy] |
| A-14 | Cannot delete default admin | `DELETE /api/security/user/admin` | Rejected (400 or 403) | [error] |
| A-15 | Login wrong password | `POST /api/security/login` `{"username":"admin","password":"wrong"}` | Status 401 | [error] |

---

## Suite 8: Access Control Lists (`acl`)

### Setup
Create user `battle_acl_user` (role: user). Login as admin and as user (separate clients).

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| ACL-01 | Admin has full access | Admin creates + reads + deletes `battle_acl_data` | All operations succeed | [happy] |
| ACL-02 | User default: can create | User creates doc in `battle_acl_data` | Succeeds (owning_write) | [happy] |
| ACL-03 | User default: can read own | User reads their own doc | Succeeds (owning_read) | [happy] |
| ACL-04 | User default: cannot read others | Admin creates doc, user lists — admin's doc not visible | User's list doesn't include admin's doc | [happy] |
| ACL-05 | Grant global read | Admin sets `read: true` for user on `battle_acl_data` | User can now see all docs | [happy] |
| ACL-06 | Get ACL | `GET /api/acl/battle_acl_user/battle_acl_data` | Returns current permissions | [happy] |
| ACL-07 | Get all ACLs | `GET /api/acl/battle_acl_user` | Returns all entity ACLs | [happy] |
| ACL-08 | Revoke permission | Remove `read` permission, user cannot see admin's docs again | Filtered out | [happy] |
| ACL-09 | Reset to default | `PUT /api/acl/battle_acl_user/battle_acl_data/default` | Owning permissions only | [happy] |
| ACL-10 | User cannot delete others' doc | User tries to delete admin's doc | Rejected (403) | [error] |

---

## Suite 9: Transactions (`transactions`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| TX-01 | Begin transaction | `POST /api/tx/begin` | Status 200, returns `transaction_id` | [happy] |
| TX-02 | Write in transaction | `POST /api/tx/{txId}/entity/battle_tx_items` `{"name":"Item1"}` | Accepted | [happy] |
| TX-03 | Update in transaction | Create item first, then `PUT /api/tx/{txId}/entity/battle_tx_items/{id}` | Accepted | [happy] |
| TX-04 | Delete in transaction | `DELETE /api/tx/{txId}/entity/battle_tx_items/{id}` | Accepted | [happy] |
| TX-05 | Commit applies all | `POST /api/tx/{txId}/commit`, verify items exist/updated/deleted | All operations reflected in DB | [happy] |
| TX-06 | Rollback discards all | Begin, write, rollback, verify item does NOT exist | Item absent | [happy] |
| TX-07 | Isolation: uncommitted not visible | Begin, write, read via normal API before commit | Item not found via normal GET | [happy] |
| TX-08 | Invalid transaction ID | `POST /api/tx/fake-id/commit` | Error response | [error] |

---

## Suite 10: KV Store (`kv`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| KV-01 | Set and get | `PUT /kv/battle_kv_key1` body=`"hello"`, `GET /kv/battle_kv_key1` | Returns `"hello"` | [happy] |
| KV-02 | Set with TTL | `PUT /kv/battle_kv_ttl?ttl=2`, wait 3s, `GET` | Key expired (empty or 404) | [happy] |
| KV-03 | Get non-existent | `GET /kv/battle_kv_nope` | Empty or 404 | [edge] |
| KV-04 | Multi-get | Set 3 keys, `GET /kv/mget?keys=k1,k2,k3` | All 3 values returned | [happy] |
| KV-05 | Delete key | `DELETE /kv/battle_kv_key1` | Deleted, subsequent GET empty | [happy] |
| KV-06 | Overwrite key | Set key, set again with new value, GET | Returns new value | [happy] |
| KV-07 | Large value (100KB) | Set key with 100KB string, GET | Exact value returned | [edge] |
| KV-08 | Special chars in key | Key = `battle_kv_special/chars:test` | Works or clear error | [edge] |

---

## Suite 11: TCP Protocol (`tcp`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| TCP-01 | PING | Send `PING\n` | Receive `PONG` | [happy] |
| TCP-02 | SET and GET | `SET battle_tcp_k1 value1`, `GET battle_tcp_k1` | Returns `value1` | [happy] |
| TCP-03 | SET with TTL | `SET TTL=2 battle_tcp_ttl val`, wait 3s, `GET` | Empty/expired | [happy] |
| TCP-04 | MGET | Set 3 keys, `MGET k1 k2 k3` | 3 values returned | [happy] |
| TCP-05 | DEL | `DEL battle_tcp_k1` | `Deleted 1` | [happy] |
| TCP-06 | RESET | `RESET` | `OK`, all keys cleared | [happy] |
| TCP-07 | SAVE | `SAVE` | `OK` | [happy] |
| TCP-08 | GET non-existent | `GET battle_tcp_nonexistent` | Empty or error | [edge] |

---

## Suite 12: Import/Export (`import_export`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| IE-01 | Export empty database | `GET /api/export` | Status 200, empty or minimal JSON | [happy] |
| IE-02 | Export with data | Create entities, `GET /api/export` | JSON contains all created entities | [happy] |
| IE-03 | Import data | `POST /api/import` with exported JSON | Data restored, GET returns imported items | [happy] |
| IE-04 | Round-trip integrity | Export → reset all → import → export again | Second export matches first | [happy] |

---

## Suite 13: Hooks (`hooks`)

### Setup
Create entity `battle_hooked_entity` with some documents.

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| HK-01 | Create pre_read hook | `POST /api/hook/battle_hooked_entity` with pre_read script that adds virtual field `isOld` | Hook created, returns ID | [happy] |
| HK-02 | pre_read virtual field | `GET /api/battle_hooked_entity/{id}` | Response includes `isOld` field | [happy] |
| HK-03 | Create post_read hook | `POST /api/hook/battle_hooked_entity` with post_read script | Hook created | [happy] |
| HK-04 | post_read enrichment | GET returns enriched data from ctx.query | Cross-entity data present | [happy] |
| HK-05 | List hooks | `GET /api/hook/battle_hooked_entity` | Returns created hooks | [happy] |
| HK-06 | Disable hook | `PUT /api/hook/id/{id}` `{"enabled":false}`, GET entity | Virtual field absent | [happy] |
| HK-07 | Delete hook | `DELETE /api/hook/battle_hooked_entity/{id}` | Hook removed | [happy] |

---

## Suite 14: Migrations (`migrations`)

### Setup
Create 10 `battle_migrate_test` documents with `status: "old"`.

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| MG-01 | Set field on all | `POST /api/battle_migrate_test/migrate` `[{"set":[{"status":"migrated"}]}]` | All docs have `status: "migrated"` | [happy] |
| MG-02 | Set nested path | `[{"set":[{"metadata.migrationVersion":"2"}]}]` | Nested field set on all | [happy] |
| MG-03 | Multiple set actions | `[{"set":[{"a":1}]},{"set":[{"b":2}]}]` | Both fields set | [happy] |

---

## Suite 15: Edge Cases (`edge_cases`)

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| E-01 | Unicode in values | Create doc `{"name":"日本語テスト","emoji":"🚀"}` | Stored and returned correctly | [edge] |
| E-02 | Unicode in field names | Create doc `{"prénom":"Jean"}` | Stored and returned correctly | [edge] |
| E-03 | Empty string value | `{"title":""}` | Stored as empty string | [edge] |
| E-04 | Very long string (100KB) | `{"content":"<100KB string>"}` | Accepted | [edge] |
| E-05 | Deeply nested object (10 levels) | Recursive `{"a":{"b":{"c":...}}}` | Stored and retrieved intact | [edge] |
| E-06 | Large array (1000 items) | `{"items":[1,2,...,1000]}` | Stored correctly | [edge] |
| E-07 | Concurrent creates | 50 parallel POST requests | All succeed, no duplicates, correct count | [edge] |
| E-08 | Duplicate custom ID | Create two docs with same custom `id` | Second overwrites or errors (verify behavior) | [edge] |
| E-09 | Boolean and null values | `{"active":true,"deleted":false,"notes":null}` | Types preserved on read | [edge] |
| E-10 | Numeric precision | `{"price":19.99,"big":9007199254740993}` | Values not silently truncated | [edge] |
| E-11 | Empty array value | `{"tags":[]}` | Stored as empty array | [edge] |
| E-12 | Request with trailing slash | `GET /api/battle_edge_test/` vs `GET /api/battle_edge_test` | Same behavior | [edge] |

---

## Suite 16: Crash Recovery (`crash_recovery`)

> These tests require stopping and restarting the ElysianDB instance. They must run after all other functional suites and before performance.

### Setup
Create 10 `battle_crash_data` documents and force save (`POST /save`).

| ID | Name | Action | Expected | Tag |
|----|------|--------|----------|-----|
| CR-01 | Data survives SIGKILL + restart | Insert docs, `POST /save`, SIGKILL process, restart, `GET /api/battle_crash_data` | All saved documents present | [happy] |
| CR-02 | WAL replay recovers unsaved writes | Insert docs, `POST /save`, insert more docs (no save), SIGKILL, restart, verify | WAL-logged writes recovered after restart | [happy] |
| CR-03 | Graceful recovery from missing shard | `POST /save`, stop process, delete one shard file from `.battle/data/`, restart | Server starts, remaining data accessible (partial data loss acceptable) | [edge] |

---

## Suite 17: Performance (`performance`)

Performance tests are NOT pass/fail — they measure and report latency percentiles and throughput.

| ID | Scenario | Method | Iterations | Measurement |
|----|----------|--------|------------|-------------|
| P-01 | Single create | `POST /api/battle_perf_items` `{"value":N}` | 200 | Latency p50, p95, p99 |
| P-02 | Batch create (100 docs) | `POST /api/battle_perf_items` `[{...} x 100]` | 50 | Latency p50, p95, p99 |
| P-03 | Single get by ID | `GET /api/battle_perf_items/{id}` | 200 | Latency p50, p95, p99 |
| P-04 | List 1000 docs | `GET /api/battle_perf_items?limit=1000` | 50 | Latency p50, p95, p99 |
| P-05 | Filtered query | `POST /api/query` with eq filter | 200 | Latency p50, p95, p99 |
| P-06 | Sorted query | `GET ?sort[value]=asc&limit=100` | 100 | Latency p50, p95, p99 |
| P-07 | Concurrent reads (10 parallel) | 10 concurrent GET requests | 100 batches | Throughput (req/s), latency p99 |
| P-08 | KV set/get cycle | SET then GET same key | 500 | Latency p50, p95, p99 |

### Percentile Computation

```
sorted_durations = sort(all_durations)
p50 = sorted_durations[len * 0.50]
p95 = sorted_durations[len * 0.95]
p99 = sorted_durations[len * 0.99]
throughput = iterations / total_elapsed_seconds
```

---

## Total Test Count

| Suite | Count |
|-------|-------|
| Health & System | 5 |
| Entity CRUD | 24 |
| Query API | 20 |
| URL Query Params | 8 |
| Nested Entities | 6 |
| Schema | 10 |
| Authentication | 15 |
| ACL | 10 |
| Transactions | 8 |
| KV Store | 8 |
| TCP Protocol | 8 |
| Import/Export | 4 |
| Hooks | 7 |
| Migrations | 3 |
| Edge Cases | 12 |
| Crash Recovery | 3 |
| Performance | 8 (metrics, not pass/fail) |
| **Total** | **159** |
