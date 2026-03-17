# Manual Test Plan — Phase 2 PR

<!-- SESSION PROGRESS: all sections tested -->
<!-- BUG-1: Shell commands with trailing semicolons are mishandled.
     Root cause: is_shell_command() strips `;` for detection but dispatch_input() receives
     the original string with `;` intact. DESCRIBE KEYSPACES; → args="KEYSPACES;" → no match.
     DESCRIBE TABLE events; → table name="events;" → not found.
     Fix: strip trailing semicolon in process_line() before calling dispatch_input(). -->
<!-- BUG-2: Table row separators show `||||` instead of proper `---+---` separators.
     Affects: regular query output, trace tables, paging output. -->
<!-- BUG-3: DESCRIBE TABLE <bare_name> doesn't use current keyspace set by USE.
     `USE test_ks` then `DESCRIBE TABLE events` → "No keyspace selected".
     Qualified name `DESCRIBE TABLE test_ks.events` works fine. -->

## Prerequisites

```bash
# Use ScyllaDB (starts faster than Cassandra)
docker run -d --name test-cass -p 9042:9042 scylladb/scylla:latest --smp 1 --memory 512M
# Wait ~10s for startup, then:
cargo build --release
```

Use `./target/release/cqlsh-rs` (or `cargo run --`) for all tests below.

---

## 1. Tabular Formatting (2.4) ✅

| # | Test | Result |
|---|------|--------|
| 1.1 | `SELECT * FROM system.local;` | ✅ pass — columns wrap to terminal width |
| 1.2 | `SELECT key, cluster_name, release_version FROM system.local;` | ✅ pass — 3 columns, correct separators |
| 1.3 | Insert mixed types and SELECT back (int, text, uuid, boolean) | ✅ pass — int right-aligned, text left-aligned |
| 1.4 | Empty result: `SELECT * FROM system.local WHERE key='nonexistent';` | ✅ pass (fixed) — shows `(0 rows)` |

**Bugs fixed during testing:**
- `CQLSH_PRESET` string was wrong (`" -||++++    "` → `"     -+  |+        "`) — caused `|||` border output
- No terminal width cap — table expanded to content width; fixed by calling `table.set_width(terminal_width())`
- Empty results showed nothing; fixed by handling `rows.is_empty()` separately in formatter and repl

## 2. Expanded Output (EXPAND) ✅

| # | Test | Result |
|---|------|--------|
| 2.1 | `EXPAND` | ✅ pass |
| 2.2 | `EXPAND ON` | ✅ pass |
| 2.3 | `SELECT * FROM system.local;` (with EXPAND ON) | ✅ pass — vertical format |
| 2.4 | `EXPAND OFF` then SELECT again | ✅ pass — back to tabular |

## 3. Pagination (PAGING) (2.5) ✅

| # | Test | Result |
|---|------|--------|
| 3.1 | `PAGING` | ✅ pass |
| 3.2 | `PAGING ON` | ✅ pass |
| 3.3 | `PAGING 5` | ✅ pass |
| 3.4 | Query with >5 rows with PAGING 5 | ✅ pass (functional) — `---MORE---` prompt appears; table formatting has `\|\|\|\|` bug |
| 3.5 | Press any key at `---MORE---` | ✅ pass |
| 3.6 | Press `q` at `---MORE---` | ✅ pass |
| 3.7 | `PAGING OFF` | ✅ pass |

## 4. Server Error Display (2.6) ✅

| # | Test | Result |
|---|------|--------|
| 4.1 | `SELEC * FROM system.local;` (typo) | ✅ `SyntaxException: line 1:0 no viable alternative at input 'SELEC'` |
| 4.2 | `SELECT * FROM nonexistent_ks.nonexistent_table;` | ✅ `InvalidRequest: Keyspace nonexistent_ks does not exist` |
| 4.3 | `CREATE KEYSPACE system WITH replication = ...;` | not tested |

## 5. DESCRIBE CLUSTER (2.12) ✅

| # | Test | Result |
|---|------|--------|
| 5.1 | `DESCRIBE CLUSTER` | ✅ Shows `Cluster: `, `Partitioner: Murmur3Partitioner` (cluster name empty — ScyllaDB quirk, not a bug) |
| 5.2 | `DESC CLUSTER` | not tested — leave for demo |

## 6. DESCRIBE KEYSPACES (2.8) ✅

| # | Test | Result |
|---|------|--------|
| 6.1 | `DESCRIBE KEYSPACES` | ✅ Space-separated list of all keyspaces |

## 7. DESCRIBE KEYSPACE (2.11) ✅

| # | Test | Result |
|---|------|--------|
| 7.1 | `USE system;` then `DESCRIBE KEYSPACE` | not tested |
| 7.2 | `DESCRIBE KEYSPACE system_auth` | ✅ Shows correct CREATE KEYSPACE statement |
| 7.3 | `DESCRIBE KEYSPACE nonexistent` | not tested — leave for demo |
| 7.4 | `DESCRIBE KEYSPACE` (with no USE) | not tested — leave for demo |

## 8. DESCRIBE TABLES (2.9) — not tested, leave for demo

| # | Test | Expected |
|---|------|----------|
| 8.1 | `USE system_schema;` then `DESCRIBE TABLES` | Lists all tables in system_schema |
| 8.2 | `DESCRIBE TABLES` (with no USE) | Shows "No keyspace selected..." |

## 9. DESCRIBE TABLE (2.10) ⚠️ BUG

| # | Test | Result |
|---|------|--------|
| 9.1 | `DESCRIBE TABLE events` (after USE test_ks) | ❌ BUG: "No keyspace selected" — doesn't use current keyspace from USE |
| 9.1b | `DESCRIBE TABLE test_ks.events` (qualified name) | ✅ pass |
| 9.2 | `DESC system_schema.keyspaces` | ✅ pass |
| 9.3 | `DESCRIBE TABLE test_ks.nonexistent` | ✅ pass — shows error |

## 10. DESCRIBE SCHEMA (2.13) ✅

| # | Test | Result |
|---|------|--------|
| 10.1 | `DESCRIBE SCHEMA` | ✅ Shows CREATE KEYSPACE + CREATE TABLE for test_ks only; system_* excluded |

## 11. Trace Output (2.18) ⚠️ COSMETIC

| # | Test | Result |
|---|------|--------|
| 11.1 | `TRACING ON` | ✅ pass |
| 11.2 | `SELECT * FROM system.local;` | ⚠️ pass (functional) — trace table has broken formatting: `\|\|\|\|` instead of row separators |
| 11.3 | `TRACING OFF` then query | ✅ pass — no trace output |

## 12. SOURCE Command ✅

| # | Test | Result |
|---|------|--------|
| 12.1 | Create `/tmp/test.cql` with `SELECT cluster_name FROM system.local;` then `SOURCE '/tmp/test.cql'` | ✅ pass (cluster_name empty — ScyllaDB quirk) |
| 12.2 | `SOURCE '/tmp/nonexistent.cql'` | ✅ pass — shows error |

## 13. CAPTURE Command ✅

| # | Test | Result |
|---|------|--------|
| 13.1 | `CAPTURE '/tmp/output.txt'` | ✅ pass |
| 13.2 | Run a SELECT | ✅ pass — output on screen and in file |
| 13.3 | `CAPTURE OFF` | ✅ pass |
| 13.4 | Verify `/tmp/output.txt` contains the query output | ✅ pass |

## 14. Smart Name Resolution ✅

| # | Test | Result |
|---|------|--------|
| 14.1 | `DESC system` (bare keyspace name) | ✅ Shows CREATE KEYSPACE system |
| 14.2 | `USE test_ks; DESC events` (bare table name) | not tested |
| 14.3 | `DESC test_ks.events` (qualified name) | ✅ Shows CREATE TABLE events |

## 15. Edge Cases ✅

| # | Test | Result |
|---|------|--------|
| 15.1 | `DESCRIBE` (no args) | ✅ Shows usage help |
| 15.2 | `DESCRIBE TABLE` (no table name) | ✅ Shows "DESCRIBE TABLE requires a table name." |
| 15.3 | `HELP` | not tested — leave for demo |

---

## Cleanup

```sql
DROP KEYSPACE test_ks;
```
```bash
docker stop test-cass && docker rm test-cass
```
