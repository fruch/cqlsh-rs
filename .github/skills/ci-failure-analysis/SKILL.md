---
name: ci-failure-analysis
description: >-
  Implement and configure AI-powered CI failure analysis workflows for GitHub
  Actions. Use when setting up CI failure summaries, configuring claude-code-action
  for failure diagnosis, writing workflow_run triggered analysis, creating
  collapsed PR comments with failure diagnostics, implementing flaky test
  detection, or working on recurring issue tracking. Covers the full SP15 plan.
---

# CI Failure Analysis

Implement AI-powered CI failure analysis that automatically diagnoses failing CI jobs and posts structured, collapsed PR comments with root cause analysis, fix suggestions, and recurring issue detection.

## Before Starting

1. Read `docs/plans/15-ai-ci-failure-summaries.md` — the full implementation plan
2. Read the current CI workflow: `.github/workflows/ci.yml`
3. Check `docs/plans/10-testing-strategy.md` for testing context

## Architecture

The system has three components:

1. **Test output collection** — `cargo-nextest` produces JUnit XML for structured test data
2. **Failure analysis workflow** — `ci-failure-analysis.yml` triggers on `workflow_run` completion, invokes `anthropics/claude-code-action` to analyze logs
3. **PR comment posting** — `actions/github-script` formats and posts/updates a collapsed comment

```
CI Workflow Fails
    ├─> Collect JUnit XML (cargo-nextest)
    ├─> Collect raw logs (gh run view --log-failed)
    ├─> Invoke claude-code-action with JSON schema
    │     ├─> Classify each failure
    │     ├─> Identify root cause
    │     ├─> Suggest fixes (file:line references)
    │     └─> Detect flaky tests
    ├─> Post/update collapsed PR comment
    └─> Auto-retry if flaky (confidence > 0.8)
```

## Key Implementation Details

### Workflow Trigger

Use `workflow_run` to trigger analysis after the CI workflow completes:

```yaml
on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]

jobs:
  analyze-failure:
    if: >
      github.event.workflow_run.conclusion == 'failure' &&
      github.event.workflow_run.event == 'pull_request'
```

This avoids re-running CI and only fires when CI actually fails on a PR.

### Required Permissions

```yaml
permissions:
  contents: read
  pull-requests: write
  actions: read
  checks: read
  id-token: write
```

### Required Secrets

| Secret | Purpose |
|--------|---------|
| `ANTHROPIC_API_KEY` | Claude API access |

### claude-code-action Configuration

Use structured JSON output with `--json-schema` to get validated, parseable results:

```yaml
- uses: anthropics/claude-code-action@v1
  with:
    anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
    model: claude-haiku-4-5-20251001
    prompt: |
      Analyze the CI failure...
    claude_args: |
      --allowedTools "Bash(gh run view:*),Bash(gh api:*),Read"
      --model claude-haiku-4-5-20251001
      --json-schema '<schema>'
```

### Failure Classification Taxonomy

Always classify failures into one of these categories:

| Category | Description | Auto-action |
|----------|-------------|-------------|
| `compilation_error` | Code does not compile | None |
| `test_failure` | Test assertion failed | None |
| `lint_violation` | Clippy or rustfmt failure | Suggest exact fix |
| `infrastructure_flaky` | Timeout, container startup, network | Auto-retry if confidence > 0.8 |
| `dependency_issue` | Cargo resolution, version conflict | Suggest `cargo update` |
| `configuration_error` | CI YAML issue, missing secret | Link to docs |
| `unknown` | Cannot classify | Flag for manual review |

### PR Comment Format

Use GitHub `<details>` tags for collapsible sections. Structure:

1. **Summary header** — pass/fail counts, classification labels, flakiness assessment
2. **Failed job sections** (collapsed) — one `<details>` per failed job containing error, root cause, suggested fix, file reference, category
3. **Recurring issues section** (collapsed) — table of issues seen across multiple runs
4. **Footer** — re-run link, attribution

### Comment Deduplication

Always check for an existing bot comment starting with `## CI Failure Summary` and update it rather than creating a duplicate:

```javascript
const existing = comments.data.find(c =>
  c.user.type === 'Bot' && c.body.startsWith('## CI Failure Summary')
);
if (existing) {
  await github.rest.issues.updateComment({ ..., comment_id: existing.id, body });
} else {
  await github.rest.issues.createComment({ ..., body });
}
```

### Recurring Issue Detection

Store failure summaries as workflow artifacts with 30-day retention. On each failure:

1. Download recent `ci-failure-*` artifacts for the same branch
2. Match failures by test name and error message similarity
3. Report patterns when the same failure appears in 2+ of the last 5 runs

### Flaky Test Auto-Retry

```yaml
- name: Auto-retry if flaky
  if: >
    fromJSON(steps.analyze.outputs.structured_output).is_flaky == true &&
    fromJSON(steps.analyze.outputs.structured_output).flaky_confidence > 0.8
  run: gh run rerun ${{ github.event.workflow_run.id }} --failed
```

## cargo-nextest Setup

Replace `cargo test` with `cargo-nextest` in the CI workflow for JUnit XML output:

```yaml
- uses: taiki-e/install-action@nextest
- run: cargo nextest run --all-targets --all-features --message-format junit --output-file test-results.xml
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: test-results
    path: test-results.xml
```

## Model Selection

Use **Claude Haiku 4.5** for cost efficiency (~$0.005 per analysis). Only escalate to Sonnet for complex multi-file failures that need deeper codebase analysis.

## Validation Checklist

After implementing, verify:

- [ ] CI failures trigger the analysis workflow
- [ ] PR comment appears with collapsed sections
- [ ] Each failure has: error message, root cause, suggested fix, classification
- [ ] Comment is updated (not duplicated) on subsequent pushes
- [ ] Recurring issues are detected across runs
- [ ] Flaky tests are auto-retried when detected
- [ ] Cost per analysis stays under $0.01
- [ ] Workflow does not trigger on non-PR failures (push to main)
