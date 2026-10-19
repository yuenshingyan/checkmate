# 010 — Report unsupported context precisely

- **Status:** Planned
- **Dependencies:** 006, 008, 009
- **Goal:** Explain automatic-resolution failures without suggesting that context was guessed.

## Scope

Add structured resolver errors with source range, identifier/declaration kind, dependency
chain, and a short remedy. Include configured snippet identity. Keep previews bounded.
Separate extraction/resolution errors from ordinary rustc visibility, type, and borrow errors.

## Non-goals

Remapping every rustc diagnostic, silently falling back to a different transformation,
or changing signatures automatically to make an error disappear.

## Acceptance tests

- Missing input, inaccessible/unknown item, unsupported macro/cfg, and unsafe promotion
  each have distinct readable diagnostics.
- Error messages identify the originating file and selected range.
- Long dependency chains and Unicode source produce bounded, valid output.
- Error paths never write source, configuration, or snapshots.
