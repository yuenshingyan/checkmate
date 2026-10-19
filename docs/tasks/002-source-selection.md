# 002 — Parse source files and locate selected syntax

- **Status:** Planned
- **Dependencies:** 001
- **Goal:** Map the selected range to syntax and its enclosing scopes.

## Scope

Extend `crates/checkmate/src/source.rs` with whole-file parsing and source locations
for automatic mode only. Locate complete selected statements/expressions and their
enclosing module/function/block. Keep the existing text-based extractor intact.
Evaluate `proc_macro2` span locations and explicitly map their line/column conventions.

## Non-goals

Resolving identifiers, expanding macros, or changing the requested range silently.

## Acceptance tests

- Locate selections in nested blocks and inline modules.
- Cover Unicode, CRLF, blank lines, comments, and first/last source lines.
- Reject partial syntax or ranges crossing incompatible scopes with source locations.
- Existing non-file statement fixtures still work in ordinary extraction mode.
