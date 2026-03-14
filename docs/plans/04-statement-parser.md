# Sub-Plan SP4: Statement Parser

> Parent: [high-level-design.md](high-level-design.md) | Phase: 1-2

## Objective

Implement a statement parser that handles multi-line input buffering, semicolon-terminated statement detection, comment stripping, string literal handling, and routing between CQL statements and built-in shell commands.

---

## Research Phase

### Tasks

1. **Python cqlsh parser behavior** — How it detects statement boundaries, handles quotes, comments
2. **CQL comment syntax** — `--` line comments, `/* */` block comments
3. **CQL string literals** — Single-quoted strings, `$$` dollar-quoted strings, escape sequences
4. **Built-in command detection** — How Python cqlsh distinguishes DESCRIBE/COPY/etc. from CQL
5. **Edge cases** — Semicolons in strings, nested comments, empty statements

### Research Deliverables

- [ ] Statement boundary detection algorithm spec
- [ ] Comment handling rules
- [ ] String literal handling rules
- [ ] Built-in command routing rules
- [ ] Edge case test catalog

---

## Execution Phase

### Implementation Steps

| Step | Description | Module | Tests |
|------|-------------|--------|-------|
| 1 | Basic semicolon detection (ignoring strings) | `parser.rs` | Unit: simple statements |
| 2 | Single-quoted string handling | `parser.rs` | Unit: strings with semicolons |
| 3 | Double-quoted identifier handling | `parser.rs` | Unit: quoted identifiers |
| 4 | Dollar-quoted string handling (`$$...$$`) | `parser.rs` | Unit: dollar-quoted strings |
| 5 | Line comment stripping (`--`) | `parser.rs` | Unit: comments removed |
| 6 | Block comment stripping (`/* */`) | `parser.rs` | Unit: block comments |
| 7 | Multi-line statement buffering | `parser.rs` | Unit: multi-line input |
| 8 | Empty statement handling (bare `;`) | `parser.rs` | Unit: skip empty |
| 9 | Built-in command detection (case-insensitive prefix match) | `parser.rs` | Unit: all built-in commands |
| 10 | Command routing (built-in vs CQL dispatch) | `parser.rs` | Unit: routing logic |
| 11 | Whitespace normalization | `parser.rs` | Unit: leading/trailing whitespace |
| 12 | Multiple statements on one line | `parser.rs` | Unit: `stmt1; stmt2;` |

### Upstream Bug Fixes to Account For

> See [SP16: Upstream PR Review](16-upstream-pr-review.md) for full details.

**scylla-cqlsh PR #150 (SCYLLADB-341): `/*` in string literals misinterpreted as comment.**
The Python cqlsh used regex-based `strip_comment_blocks()` preprocessing that naively matched `/*` inside string literals. The fix: remove preprocessing, tokenize in order (string literals → comments → other tokens). cqlsh-rs MUST NOT use regex preprocessing on raw CQL input for comment handling. The lexer must be context-aware.

**scylla-cqlsh PR #151 (SCYLLADB-338): O(n²) batch mode parsing.**
In batch mode, the Python cqlsh re-parsed the entire accumulated buffer on every new line, causing >2hr processing for 1MB+ UDF files. The fix: only invoke the parser when a semicolon terminator is detected. cqlsh-rs MUST use an incremental approach — track string/comment context as lines are added, detect semicolons in O(1) per line, only attempt full parse when a potential terminator is found.

### Acceptance Criteria

- [ ] Semicolons inside string literals do not terminate statements
- [ ] Comments are stripped before execution
- [ ] **Block comments (`/* */`) inside string literals do NOT split or terminate statements** (PR #150)
- [ ] **`/*` and `*/` characters inside single-quoted, double-quoted, and dollar-quoted strings are treated as literal text** (PR #150)
- [ ] Multi-line input accumulates correctly
- [ ] **Batch mode parsing is O(n) not O(n²) — verified by benchmark with 1MB+ input completing in <1s** (PR #151)
- [ ] Built-in commands are detected case-insensitively
- [ ] CQL statements are forwarded to the driver
- [ ] Empty statements are silently skipped
- [ ] Multiple statements on one line are handled sequentially

---

## Skills Required

- Parser design (S6)
- CQL language syntax (D1)
- Property-based testing for parser fuzzing (C10)
