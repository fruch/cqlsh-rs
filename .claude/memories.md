# General Memory

> Cross-cutting lessons learned in cqlsh-rs development.

## Squash fixup commits before pushing

When developing a feature, squash all fixup commits (CI fixes, formatting, lint fixes) into the relevant development commit before pushing. Each PR should present clean, single-purpose commits — not a trail of fix-ups. Use `git reset --soft` to the base and re-commit, rather than interactive rebase, for simplicity.

## Feedback

- [No fixes during manual testing](memories/feedback_no_fixes_during_testing.md)
