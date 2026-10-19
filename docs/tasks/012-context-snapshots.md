# 012 — Protect discovered context with snapshots

- **Status:** Planned
- **Dependencies:** 011
- **Goal:** Make relevant context changes reviewable, not just selected-body changes.

## Scope

Design a versioned context-baseline format containing stable source identity and the
resolved declarations/dependency mapping. Keep existing snippet snapshots backward
compatible. Track configuration, source, and baseline files for recompilation.
Separate location-only moves from content changes where the format permits it.

## Non-goals

Writing baselines during compilation, treating hashes as semantic proof, or committing
compiler build artifacts as the baseline.

## Acceptance tests

- Editing a discovered declaration triggers a useful mismatch even if the body is unchanged.
- Dependency additions/removals and alias target changes are visible.
- Unrelated source edits do not needlessly invalidate context baselines.
- Missing baselines require explicit creation/review; existing snapshots still work.
- Baseline file changes trigger recompilation in a real consumer crate.
