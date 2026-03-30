#!/usr/bin/env bash
# analyze-ci-failures.sh — Parse CI artifacts and produce structured JSON output.
#
# Usage:
#   analyze-ci-failures.sh <artifacts-dir> <failed-jobs-file> <repo> <run-id>
#
# Outputs:
#   Writes JSON to stdout matching the same schema used by AI backends,
#   so the comment-posting step works identically regardless of backend.

set -euo pipefail

ARTIFACTS_DIR="${1:?Usage: $0 <artifacts-dir> <failed-jobs-file> <repo> <run-id>}"
FAILED_JOBS_FILE="${2:?Missing failed-jobs-file}"
REPO="${3:?Missing repo (owner/name)}"
RUN_ID="${4:?Missing run-id}"

TOTAL_JOBS=5  # fmt, clippy, test, integration, build
FAILED_JOBS=$(wc -l < "$FAILED_JOBS_FILE" | tr -d ' ')

# Collect failures as JSON array elements into a temp file
FAILURES_FILE=$(mktemp)
RECURRING_FILE=$(mktemp)
trap 'rm -f "$FAILURES_FILE" "$RECURRING_FILE"' EXIT

IS_FLAKY=false
FLAKY_CONFIDENCE=0

# ─── Parse clippy JSON ───────────────────────────────────────────────
parse_clippy() {
  local json="${ARTIFACTS_DIR}/clippy-output.json"
  [ -f "$json" ] || return 0

  jq -r '
    select(.reason == "compiler-message") |
    select(.message.level == "error" or .message.level == "warning") |
    select(.message.code != null) |
    {
      job_name: "Clippy",
      test_name: .message.code.code,
      error_message: (.message.message // "unknown")[0:200],
      root_cause: ("Clippy " + .message.level + ": " + .message.code.code),
      suggested_fix: (if .message.children[0].message then .message.children[0].message else ("Run `cargo clippy --all-targets --all-features` locally to see suggestions") end),
      file_reference: (if .message.spans[0] then (.message.spans[0].file_name + ":" + (.message.spans[0].line_start | tostring)) else null end),
      classification: "lint_violation"
    }
  ' "$json" 2>/dev/null | jq -s 'unique_by(.test_name + (.file_reference // ""))' 2>/dev/null || echo "[]"
}

# ─── Parse build JSON ────────────────────────────────────────────────
parse_build() {
  local json="${ARTIFACTS_DIR}/build-output.json"
  [ -f "$json" ] || return 0

  jq -r '
    select(.reason == "compiler-message") |
    select(.message.level == "error") |
    {
      job_name: "Build",
      error_message: (.message.message // "unknown")[0:200],
      root_cause: "Compilation error",
      suggested_fix: (if .message.children[0].message then .message.children[0].message else "Fix the compilation error shown above" end),
      file_reference: (if .message.spans[0] then (.message.spans[0].file_name + ":" + (.message.spans[0].line_start | tostring)) else null end),
      classification: "compilation_error"
    }
  ' "$json" 2>/dev/null | jq -s 'unique_by(.error_message + (.file_reference // ""))' 2>/dev/null || echo "[]"
}

# ─── Parse JUnit XML test results ────────────────────────────────────
parse_tests_xml() {
  local xml="${ARTIFACTS_DIR}/test-results.xml"
  [ -f "$xml" ] || return 0

  # Extract failed testcases from JUnit XML using xmllint or sed fallback
  if command -v python3 &>/dev/null; then
    python3 -c "
import xml.etree.ElementTree as ET, json, sys
try:
    tree = ET.parse('$xml')
except Exception:
    sys.exit(0)
root = tree.getroot()
failures = []
for tc in root.iter('testcase'):
    fail = tc.find('failure')
    if fail is None:
        fail = tc.find('error')
    if fail is not None:
        name = tc.get('name', 'unknown')
        classname = tc.get('classname', '')
        msg = (fail.get('message') or fail.text or 'assertion failed')[:200]
        failures.append({
            'job_name': 'Tests',
            'test_name': name if not classname else classname + '::' + name,
            'error_message': msg,
            'root_cause': 'Test assertion failed',
            'suggested_fix': 'Run \`cargo nextest run --all-targets --all-features\` locally to reproduce',
            'classification': 'test_failure'
        })
print(json.dumps(failures))
" 2>/dev/null || echo "[]"
  else
    echo "[]"
  fi
}

# ─── Parse test log (fallback if no XML) ─────────────────────────────
parse_tests_log() {
  local log="${ARTIFACTS_DIR}/test-output.log"
  [ -f "$log" ] || return 0

  local failed_tests
  failed_tests=$(grep -E '^\s*FAIL\s' "$log" 2>/dev/null | sed 's/.*FAIL[[:space:]]*//' | sed 's/[[:space:]]*$//' || true)
  [ -z "$failed_tests" ] && return 0

  local results="["
  local first=true
  while IFS= read -r test_name; do
    [ -z "$test_name" ] && continue
    $first || results+=","
    first=false
    local escaped_name
    escaped_name=$(echo "$test_name" | jq -Rs '.')
    results+=$(cat <<ITEM
{
  "job_name": "Tests",
  "test_name": ${escaped_name},
  "error_message": "Test failed — see job log for details",
  "root_cause": "Test assertion failed",
  "suggested_fix": "Run \`cargo nextest run --all-targets --all-features\` locally to reproduce",
  "classification": "test_failure"
}
ITEM
)
  done <<< "$failed_tests"
  results+="]"
  echo "$results"
}

# ─── Parse fmt output ────────────────────────────────────────────────
parse_fmt() {
  if ! grep -q "Rustfmt" "$FAILED_JOBS_FILE" 2>/dev/null; then
    echo "[]"
    return 0
  fi

  cat <<'ITEM'
[{
  "job_name": "Rustfmt",
  "error_message": "Code formatting does not match rustfmt style",
  "root_cause": "Unformatted code",
  "suggested_fix": "Run `cargo fmt --all` locally to fix formatting",
  "classification": "lint_violation"
}]
ITEM
}

# ─── Parse integration test failures ─────────────────────────────────
parse_integration() {
  if ! grep -q "Integration" "$FAILED_JOBS_FILE" 2>/dev/null; then
    echo "[]"
    return 0
  fi

  local log="${ARTIFACTS_DIR}/integration-output.log"

  # Check for common flaky patterns
  local is_infra=false
  if [ -f "$log" ]; then
    if grep -qiE '(timed? ?out|connection refused|address already in use|container.*failed|docker)' "$log" 2>/dev/null; then
      is_infra=true
    fi
  fi

  if [ "$is_infra" = true ]; then
    IS_FLAKY=true
    FLAKY_CONFIDENCE=0.7
    cat <<'ITEM'
[{
  "job_name": "Integration Tests",
  "error_message": "Integration test failed — likely infrastructure/timing issue",
  "root_cause": "Container startup or network issue in CI environment",
  "suggested_fix": "Re-run the failed job — this appears to be a flaky infrastructure issue",
  "classification": "infrastructure_flaky"
}]
ITEM
  else
    cat <<'ITEM'
[{
  "job_name": "Integration Tests",
  "error_message": "Integration test failed — see job log for details",
  "root_cause": "Test assertion failed",
  "suggested_fix": "Run integration tests locally with a ScyllaDB container",
  "classification": "test_failure"
}]
ITEM
  fi
}

# ─── Detect recurring issues from failure history ────────────────────
detect_recurring() {
  local history_dir="failure-history"
  [ -d "$history_dir" ] || { echo "[]"; return 0; }

  local count
  count=$(find "$history_dir" -name '*.json' -type f 2>/dev/null | wc -l)
  [ "$count" -gt 0 ] || { echo "[]"; return 0; }

  # Collect all test_name values from history, count occurrences
  if command -v python3 &>/dev/null; then
    python3 -c "
import json, os, sys
from collections import Counter

history_dir = '$history_dir'
test_counts = Counter()
class_map = {}

for fname in sorted(os.listdir(history_dir))[-5:]:
    fpath = os.path.join(history_dir, fname)
    if not fname.endswith('.json'):
        continue
    try:
        with open(fpath) as f:
            data = json.load(f)
        for fail in data.get('failures', []):
            key = fail.get('test_name') or fail.get('job_name', 'unknown')
            test_counts[key] += 1
            class_map[key] = fail.get('classification', 'unknown')
    except Exception:
        pass

recurring = []
for name, count in test_counts.most_common():
    if count >= 2:
        recurring.append({
            'issue': name,
            'occurrences': count,
            'category': class_map.get(name, 'unknown'),
            'recommendation': 'Investigate — this failure has appeared in ' + str(count) + ' recent runs'
        })

print(json.dumps(recurring))
" 2>/dev/null || echo "[]"
  else
    echo "[]"
  fi
}

# ─── Collect passed/failed job names for summary ─────────────────────
build_summary() {
  local failed
  failed=$(cat "$FAILED_JOBS_FILE" | tr '\n' ', ' | sed 's/, $//')
  echo "Failed jobs: ${failed:-none}"
}

# ─── Main ─────────────────────────────────────────────────────────────
main() {
  # Parse each job type
  local clippy_failures build_failures test_failures fmt_failures integration_failures

  clippy_failures=$(parse_clippy)
  build_failures=$(parse_build)
  fmt_failures=$(parse_fmt)
  integration_failures=$(parse_integration)

  # Try XML first, fall back to log
  test_failures=$(parse_tests_xml)
  if [ "$test_failures" = "[]" ] || [ -z "$test_failures" ]; then
    test_failures=$(parse_tests_log)
  fi

  # Detect recurring issues from history
  local recurring
  recurring=$(detect_recurring)

  # Merge all failure arrays into one
  local all_failures
  all_failures=$(jq -s 'add // []' <<EOF
${clippy_failures:-[]}
${build_failures:-[]}
${test_failures:-[]}
${fmt_failures:-[]}
${integration_failures:-[]}
EOF
)

  local summary
  summary=$(build_summary)

  # Output the final JSON
  jq -n \
    --arg summary "$summary" \
    --argjson total_jobs "$TOTAL_JOBS" \
    --argjson failed_jobs "$FAILED_JOBS" \
    --argjson is_flaky "$IS_FLAKY" \
    --argjson flaky_confidence "$FLAKY_CONFIDENCE" \
    --argjson failures "$all_failures" \
    --argjson recurring "${recurring:-[]}" \
    '{
      summary: $summary,
      total_jobs: $total_jobs,
      failed_jobs: $failed_jobs,
      is_flaky: $is_flaky,
      flaky_confidence: $flaky_confidence,
      failures: $failures,
      recurring_issues: $recurring
    }'
}

main
