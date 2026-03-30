#!/usr/bin/env bash
# analyze-ci-failures.sh — Parse CI artifacts and produce a Markdown PR comment.
#
# Usage:
#   analyze-ci-failures.sh <artifacts-dir> <failed-jobs-file> <repo> <run-id>
#
# Outputs:
#   Writes Markdown comment body to stdout.

set -euo pipefail

ARTIFACTS_DIR="${1:?Usage: $0 <artifacts-dir> <failed-jobs-file> <repo> <run-id>}"
FAILED_JOBS_FILE="${2:?Missing failed-jobs-file}"
REPO="${3:?Missing repo (owner/name)}"
RUN_ID="${4:?Missing run-id}"

RUN_URL="https://github.com/${REPO}/actions/runs/${RUN_ID}"
FAILED_JOBS=$(wc -l < "$FAILED_JOBS_FILE" | tr -d ' ')
TOTAL_JOBS=6  # fmt, clippy, test, build, integration-scylladb, integration-cassandra

IS_FLAKY=false

# ─── Parse clippy JSON ───────────────────────────────────────────────
parse_clippy() {
  if ! grep -qi "Clippy" "$FAILED_JOBS_FILE" 2>/dev/null; then return 0; fi

  local json="${ARTIFACTS_DIR}/clippy-output.json"
  local count=0

  if [ -f "$json" ]; then
    count=$(jq -r '
      select(.reason == "compiler-message") |
      select(.message.level == "error" or .message.level == "warning") |
      select(.message.code != null) | .message.code.code
    ' "$json" 2>/dev/null | sort -u | wc -l | tr -d ' ')
  fi

  if [ "$count" -gt 0 ]; then
    echo "<details>"
    echo "<summary>📏 Clippy — ${count} issue(s)</summary>"
    echo ""
    jq -r '
      select(.reason == "compiler-message") |
      select(.message.level == "error" or .message.level == "warning") |
      select(.message.code != null) |
      "- **\(.message.code.code)** at `\(if .message.spans[0] then "\(.message.spans[0].file_name):\(.message.spans[0].line_start)" else "?" end)`: \(.message.message)" +
      (if .message.children[0].message then " → \(.message.children[0].message)" else "" end)
    ' "$json" 2>/dev/null | sort -u
    echo ""
    echo "</details>"
  else
    echo "<details>"
    echo "<summary>📏 Clippy — failed</summary>"
    echo ""
    echo 'Run `cargo clippy --all-targets --all-features` locally.'
    echo ""
    echo "</details>"
  fi
  echo ""
}

# ─── Parse build JSON ────────────────────────────────────────────────
parse_build() {
  if ! grep -qi "Build" "$FAILED_JOBS_FILE" 2>/dev/null; then return 0; fi

  local json="${ARTIFACTS_DIR}/build-output.json"
  local count=0

  if [ -f "$json" ]; then
    count=$(jq -r '
      select(.reason == "compiler-message") |
      select(.message.level == "error") | .message.message
    ' "$json" 2>/dev/null | wc -l | tr -d ' ')
  fi

  if [ "$count" -gt 0 ]; then
    echo "<details>"
    echo "<summary>🔨 Build — ${count} error(s)</summary>"
    echo ""
    jq -r '
      select(.reason == "compiler-message") |
      select(.message.level == "error") |
      "- `\(if .message.spans[0] then "\(.message.spans[0].file_name):\(.message.spans[0].line_start)" else "?" end)`: \(.message.message)"
    ' "$json" 2>/dev/null | head -20
    echo ""
    echo "</details>"
  else
    echo "<details>"
    echo "<summary>🔨 Build — failed</summary>"
    echo ""
    echo 'Run `cargo build --release` locally.'
    echo ""
    echo "</details>"
  fi
  echo ""
}

# ─── Parse JUnit XML test results ────────────────────────────────────
parse_tests() {
  if ! grep -qi "Tests" "$FAILED_JOBS_FILE" 2>/dev/null; then return 0; fi

  local xml="${ARTIFACTS_DIR}/test-results.xml"

  # Try JUnit XML first
  if [ -f "$xml" ] && command -v python3 &>/dev/null; then
    local result
    result=$(python3 -c "
import xml.etree.ElementTree as ET, sys
try:
    tree = ET.parse('$xml')
except: sys.exit(0)
root = tree.getroot()
failures = []
for tc in root.iter('testcase'):
    fail = tc.find('failure') or tc.find('error')
    if fail is not None:
        name = tc.get('name', '?')
        cls = tc.get('classname', '')
        full = cls + '::' + name if cls else name
        msg = (fail.get('message') or fail.text or '?')[:150]
        failures.append((full, msg))
if failures:
    print('<details>')
    print(f'<summary>🧪 Tests — {len(failures)} failed</summary>')
    print()
    for name, msg in failures:
        print(f'- **{name}**: \`{msg}\`')
    print()
    print('</details>')
" 2>/dev/null || true)

    if [ -n "$result" ]; then
      echo "$result"
      echo ""
      return 0
    fi
  fi

  echo "<details>"
  echo "<summary>🧪 Tests — failed</summary>"
  echo ""
  echo 'Run `cargo nextest run --all-targets --all-features` locally.'
  echo ""
  echo "</details>"
  echo ""
}

# ─── Parse fmt output ────────────────────────────────────────────────
parse_fmt() {
  if ! grep -qi "Rustfmt" "$FAILED_JOBS_FILE" 2>/dev/null; then return 0; fi

  echo "<details>"
  echo "<summary>📐 Rustfmt — formatting issues</summary>"
  echo ""
  echo 'Run `cargo fmt --all` locally to fix.'
  echo ""
  echo "</details>"
  echo ""
}

# ─── Parse integration test failures ─────────────────────────────────
parse_integration() {
  if ! grep -qi "Integration" "$FAILED_JOBS_FILE" 2>/dev/null; then return 0; fi

  # Check all integration logs for infrastructure patterns
  local is_infra=false
  for log in "${ARTIFACTS_DIR}"/integration-*.log; do
    [ -f "$log" ] || continue
    if grep -qiE '(timed? ?out|connection refused|address already in use|container.*fail|docker)' "$log" 2>/dev/null; then
      is_infra=true
      break
    fi
  done

  if [ "$is_infra" = true ]; then
    IS_FLAKY=true
    echo "<details>"
    echo "<summary>🔄 Integration Tests — likely flaky (infrastructure)</summary>"
    echo ""
    echo "Container startup or network issue. Try re-running the failed jobs."
    echo ""
    echo "</details>"
  else
    echo "<details>"
    echo "<summary>🧪 Integration Tests — failed</summary>"
    echo ""
    echo "Run integration tests locally with a ScyllaDB/Cassandra container."
    echo ""
    echo "</details>"
  fi
  echo ""
}

# ─── Main ─────────────────────────────────────────────────────────────
main() {
  local classifications=()
  grep -qi "Clippy"      "$FAILED_JOBS_FILE" 2>/dev/null && classifications+=("📏 lint")
  grep -qi "Build"       "$FAILED_JOBS_FILE" 2>/dev/null && classifications+=("🔨 build")
  grep -qi "Tests"       "$FAILED_JOBS_FILE" 2>/dev/null && classifications+=("🧪 test")
  grep -qi "Integration" "$FAILED_JOBS_FILE" 2>/dev/null && classifications+=("🧪 integration")
  grep -qi "Rustfmt"     "$FAILED_JOBS_FILE" 2>/dev/null && classifications+=("📐 format")

  local class_str
  class_str=$(IFS=', '; echo "${classifications[*]:-unknown}")

  echo "## CI Failure Summary"
  echo ""
  echo "**${FAILED_JOBS} of ${TOTAL_JOBS} jobs failed** | ${class_str}"
  echo ""

  parse_clippy
  parse_build
  parse_tests
  parse_fmt
  parse_integration

  if [ "$IS_FLAKY" = true ]; then
    echo "> **Flaky:** Looks like an infrastructure issue — consider re-running."
    echo ""
  fi

  echo "---"
  echo "[View logs](${RUN_URL}) · [Re-run failed jobs](${RUN_URL})"
}

main
