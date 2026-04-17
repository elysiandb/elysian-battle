# ElysianDB API Reference

> Quick reference for ElysianDB features, used internally by elysian-battle.
> Based on ElysianDB v0.1.14 documentation at [elysiandb.com](https://elysiandb.com).

---

## Configuration (`elysian.yaml`)

```yaml
store:
  folder: /data                          # Data persistence path
  shards: 512                            # In-memory shards (power of 2)
  flushIntervalSeconds: 5                # Auto-save interval
  crashRecovery:
    enabled: true                        # Write-ahead log for durability
    maxLogMB: 100                        # Max WAL size before flush

engine:
  name: "internal"                       # "internal" | "mongodb"
  uri: "mongodb://user:pass@host:27017/db"  # MongoDB only

server:
  http:
    enabled: true
    host: 0.0.0.0
    port: 8089
  tcp:
    enabled: true
    host: 0.0.0.0
    port: 8088

log:
  flushIntervalSeconds: 5

stats:
  enabled: false

security:
  authentication:
    enabled: true
    mode: "user"                         # "basic" | "token" | "user"
    token: "your_token"                  # Token mode only

api:
  schema:
    enabled: true
    strict: true                         # Reject unknown fields (manual schema)
  index:
    workers: 4                           # Lazy index rebuild workers
  cache:
    enabled: true
    cleanupIntervalSeconds: 10
  hooks:
    enabled: true                        # JavaScript hooks (Goja runtime)

adminui:
  enabled: true                          # Admin UI at /admin
```

---

## Storage Engines

| Engine | Config | Dependencies | Notes |
|--------|--------|-------------|-------|
| `internal` | `engine.name: "internal"` | None | Sharded in-memory + disk persistence + WAL |
| `mongodb` | `engine.name: "mongodb"` + `engine.uri` | MongoDB 7+ | Same API surface, external durability |

---

## CLI Commands

```bash
elysiandb server               # Start server (default command)
elysiandb server --config path # Start with custom config
elysiandb create-user          # Interactive user creation
elysiandb delete-user          # Interactive user deletion
elysiandb change-password      # Interactive password change
elysiandb reset --force        # Reset all data (requires confirmation)
elysiandb help                 # List commands
```

---

## Authentication Modes

### Token (`mode: "token"`)
```
Authorization: Bearer <token>
```

### Basic (`mode: "basic"`)
```
Authorization: Basic base64(username:password)
```

### User / Session (`mode: "user"`)
```
POST /api/security/login  {"username":"admin","password":"admin"}
→ Set-Cookie: edb_session=<32-byte-hex>; HttpOnly; Secure; SameSite=Strict
```
Default credentials: `admin` / `admin`

---

## REST API — Entity CRUD

| Method | Path | Body | Response |
|--------|------|------|----------|
| `POST` | `/api/{entity}` | JSON object or array | Created document(s) with `id` |
| `GET` | `/api/{entity}` | — | Array of documents |
| `GET` | `/api/{entity}/{id}` | — | Single document |
| `PUT` | `/api/{entity}/{id}` | JSON partial update | Updated document |
| `PUT` | `/api/{entity}` | JSON array (each with `id`) | Batch updated documents |
| `DELETE` | `/api/{entity}/{id}` | — | Deletion confirmation (status `200` or `204`) |
| `DELETE` | `/api/{entity}` | — | Delete ALL documents (status `200` or `204`) |
| `GET` | `/api/{entity}/count` | — | `{"count": N}` — historical builds may return a bare integer `N` |
| `GET` | `/api/{entity}/{id}/exists` | — | `200` with `{"exists": true\|false}` — some builds also use bare `true`/`false`, bare `0`, `null`, `{}`, or `404` for "not found" |

### List Query Parameters

| Param | Example | Description |
|-------|---------|-------------|
| `limit` | `?limit=10` | Max items |
| `offset` | `?offset=20` | Skip items |
| `sort[field]` | `?sort[name]=asc` | Sort (asc/desc), auto-creates index |
| `filter[field][op]` | `?filter[age][gt]=18` | Filter by operator |
| `fields` | `?fields=name,email` | Field projection — strict; `id` is **not** auto-included, request it explicitly if you need it (e.g. `?fields=id,name`) |
| `includes` | `?includes=author,author.job` | Expand sub-entities |
| `search` | `?search=keyword` | Full-text search |
| `countOnly` | `?countOnly=true` | Return count only |

### Auto-ID

If `id` field is absent in POST body, a UUID v4 is generated automatically.

### Nested Entity Creation (`@entity`)

```json
{
  "title": "My Post",
  "author": {
    "@entity": "authors",
    "fullname": "Alice",
    "job": { "@entity": "jobs", "title": "Writer" }
  }
}
```
Sub-entities are created recursively. References stored as `{"@entity":"type","id":"uuid"}`.

---

## REST API — Query

```
POST /api/query
```

```json
{
  "entity": "articles",
  "offset": 0,
  "limit": 10,
  "filters": {
    "and": [
      {"status": {"eq": "published"}},
      {"or": [
        {"views": {"gt": 100}},
        {"featured": {"eq": true}}
      ]}
    ]
  },
  "sorts": {"views": "desc"},
  "fields": "id,title,views",
  "countOnly": false
}
```

### Filter Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Equals (supports `*` glob) | `{"title":{"eq":"*Rust*"}}` |
| `neq` | Not equals | `{"status":{"neq":"draft"}}` |
| `lt` | Less than | `{"age":{"lt":18}}` |
| `lte` | Less than or equal | `{"age":{"lte":18}}` |
| `gt` | Greater than | `{"views":{"gt":100}}` |
| `gte` | Greater than or equal | `{"views":{"gte":100}}` |
| `contains` | String/array contains | `{"tags":{"contains":"rust"}}` |
| `not_contains` | Does not contain | `{"tags":{"not_contains":"old"}}` |
| `all` | Array has all values | `{"tags":{"all":"a,b"}}` |
| `any` | Array has any value | `{"tags":{"any":"a,b"}}` |
| `none` | Array has none of values | `{"tags":{"none":"a,b"}}` |

### Execution Order
`Load → Filter → Sort → Offset → Limit`

---

## REST API — Schema

| Method | Path | Body | Response |
|--------|------|------|----------|
| `POST` | `/api/{entity}/create` | `{"fields":{"name":"string"}}` | Entity type created |
| `GET` | `/api/{entity}/schema` | — | Schema definition |
| `PUT` | `/api/{entity}/schema` | `{"fields":{...}}` | Manual schema set |
| `GET` | `/api/entity/types` | — | All types with schemas |
| `GET` | `/api/entity/types/name` | — | Type names only |

### Supported Types
`string`, `number`, `boolean`, `object`, `array`

### Field Definition (full form)
```json
{"fields": {"name": {"type": "string", "required": true}}}
```

---

## REST API — User Management (mode: "user", admin only)

| Method | Path | Body | Notes |
|--------|------|------|-------|
| `POST` | `/api/security/user` | `{"username","password","role"}` | Roles: `admin`, `user` |
| `GET` | `/api/security/user` | — | List all users |
| `GET` | `/api/security/user/{name}` | — | Get user |
| `DELETE` | `/api/security/user/{name}` | — | Cannot delete `admin` |
| `PUT` | `/api/security/user/{name}/password` | `{"password":"..."}` | Change password |
| `PUT` | `/api/security/user/{name}/role` | `{"role":"..."}` | Change role |
| `POST` | `/api/security/login` | `{"username","password"}` | Returns session cookie |
| `POST` | `/api/security/logout` | — | Clears session |
| `GET` | `/api/security/me` | — | Current user info |

---

## REST API — ACL (mode: "user", admin only)

### Permissions
- **Global**: `create`, `read`, `update`, `delete`
- **Owning** (own documents only): `owning_read`, `owning_write`, `owning_update`, `owning_delete`

### Defaults
- Admin: all permissions
- User: owning permissions only

### Endpoints

| Method | Path | Body |
|--------|------|------|
| `GET` | `/api/acl/{user}/{entity}` | — |
| `GET` | `/api/acl/{user}` | — |
| `PUT` | `/api/acl/{user}/{entity}` | Permission object |
| `PUT` | `/api/acl/{user}/{entity}/default` | — (reset) |

Ownership tracked via internal field `_core_username`.

---

## REST API — Transactions

| Method | Path | Body | Response |
|--------|------|------|----------|
| `POST` | `/api/tx/begin` | — | `{"transaction_id":"uuid"}` |
| `POST` | `/api/tx/{txId}/entity/{entity}` | JSON doc | Write in tx |
| `PUT` | `/api/tx/{txId}/entity/{entity}/{id}` | JSON partial | Update in tx |
| `DELETE` | `/api/tx/{txId}/entity/{entity}/{id}` | — | Delete in tx |
| `POST` | `/api/tx/{txId}/commit` | — | Apply all |
| `POST` | `/api/tx/{txId}/rollback` | — | Discard all |

Operations are isolated until commit. Failed commit aborts entire transaction.

---

## REST API — KV Store

| Method | Path | Body | Notes |
|--------|------|------|-------|
| `PUT` | `/kv/{key}?ttl=seconds` | Raw value | Optional TTL |
| `GET` | `/kv/{key}` | — | Get value |
| `GET` | `/kv/mget?keys=k1,k2,k3` | — | Multi-get |
| `DELETE` | `/kv/{key}` | — | Delete key |

---

## REST API — Hooks (admin only)

| Method | Path | Body |
|--------|------|------|
| `POST` | `/api/hook/{entity}` | Hook definition |
| `GET` | `/api/hook/{entity}` | — |
| `GET` | `/api/hook/id/{id}` | — |
| `PUT` | `/api/hook/id/{id}` | Partial update |
| `DELETE` | `/api/hook/{entity}/{id}` | — |

### Hook Definition
```json
{
  "name": "enrich_orders",
  "entity": "users",
  "event": "post_read",
  "language": "javascript",
  "script": "function postRead(ctx) { return ctx.entity; }",
  "priority": 10,
  "bypass_acl": false,
  "enabled": true
}
```

Events: `pre_read`, `post_read`

---

## REST API — Migrations

```
POST /api/{entity}/migrate
```
```json
[
  {"set": [{"status": "active", "migrated": true}]},
  {"set": [{"metadata.version": "2.0"}]}
]
```

---

## REST API — Import/Export

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/export` | Full database dump as JSON |
| `POST` | `/api/import` | Restore from JSON dump |

---

## System Endpoints

| Method | Path | Description | Condition |
|--------|------|-------------|-----------|
| `GET` | `/health` | Health check (200) | Always |
| `POST` | `/save` | Force flush to disk | Always |
| `POST` | `/reset` | Reset all keys | Always |
| `GET` | `/stats` | Runtime metrics JSON | `stats.enabled: true` |
| `GET` | `/config` | Current config JSON | Always |

### Stats Response
```json
{
  "keys_count": "1203",
  "expiration_keys_count": "87",
  "uptime_seconds": "3605",
  "total_requests": "184467",
  "hits": "160002",
  "misses": "24465",
  "entities_count": "11062"
}
```

---

## TCP Protocol (port 8088 default)

| Command | Response |
|---------|----------|
| `PING` | `PONG` |
| `SET key value` | `OK` |
| `SET TTL=N key value` | `OK` |
| `GET key` | value |
| `MGET key1 key2 ...` | values |
| `DEL key` | `Deleted N` |
| `RESET` | `OK` |
| `SAVE` | `OK` |

---

## Response Headers

| Header | Value | When |
|--------|-------|------|
| `X-Elysian-Version` | e.g. `0.1.14` | Every response |
| `X-Elysian-Cache` | `HIT` or `MISS` | When cache enabled |

---

## CORS

- `Access-Control-Allow-Origin`: mirrors request `Origin`
- `Access-Control-Allow-Credentials: true`
- `Access-Control-Allow-Headers: Content-Type, Authorization`
- `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`
- OPTIONS → 204

---

## Internal Entities (reserved names)

| Entity | Purpose |
|--------|---------|
| `_elysiandb_core_user` | User accounts |
| `_elysiandb_core_acl` | ACL definitions |
| `_elysiandb_core_hook` | Hook definitions |

---

## Build Requirements

| Tool | Version | Notes |
|------|---------|-------|
| Go | 1.24+ | `go.mod` specifies `go 1.24.0` |
| Git | any | Clone repository |

### Build Command
```bash
CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o elysiandb .
```

### Run
```bash
./elysiandb server --config elysian.yaml
```

### Docker
```bash
docker run --rm -p 8089:8089 -p 8088:8088 taymour/elysiandb:latest
```

---

## Releases

15 releases from `v0.1.0` to `v0.1.14`.
Repository: `https://github.com/elysiandb/elysiandb`
Default branch: `main`
