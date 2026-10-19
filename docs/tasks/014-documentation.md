# 014 — Document the feature and add examples

- **Status:** Completed
- **Dependencies:** 013
- **Goal:** Explain exactly what is automatic and where explicit inputs remain required.

## Scope

Update the usage guide and architecture document. Add a runnable example package for
supported struct-returning extraction without manual context. Document the supported
matrix, opt-in syntax, copied type identity, errors, and context-baseline review workflow.
Retain the basic example as a demonstration of ordinary extraction.

## Non-goals

Advertising full semantic inference, adding IDE integrations, or generating user tests.

## Acceptance

- All documented examples run under workspace tests.
- Documentation clearly separates implemented behavior from deferred work.
- The new example leaves its original snippet untouched and includes user-authored tests.
- Source/config/baseline paths and commands are valid from the documented directory.
- Backlog statuses reflect actual completion, not merely documentation of a plan.

## Implementation

- Added the unpublished workspace package `examples/automatic-context`
  (`checkmate-auto-example`), with an ordinary unannotated library, separate TOML
  metadata, user-authored integration tests, and reviewed snippet/context baselines.
  Its source was frozen before metadata/builds and remained byte-identical throughout.
- The configured extraction returns the original public `Invoice` without an import or
  manual declaration range. Explicit `total: u64`/`quantity: u32` inputs do not execute
  the earlier multiplication. Four tests check concrete original type identity, supplied
  inputs, matching original behavior, and a separate inline tuple-returning extraction.
- An ignored user-authored test explicitly prints `invoice_context()` for review. The
  version-1 baseline was inspected and saved outside compilation; missing/mismatching
  guards never auto-update or fall back. The inline tuple example intentionally uses
  only the snippet guard; the configured extraction uses both guards.
- README, usage, macro rustdoc, architecture, example index, review guide, contributor
  guidance, and task records now distinguish ordinary mode, public automatic discovery,
  and internal walkthroughs. Paths, root-level commands, diagnostic remedies, copied
  nominal identity, transparent aliases, and original-terminal snapshot limits are explicit.
- Existing local-type/inherent-method examples remain the demonstrations of copied
  identities and the narrower method subset. The basic binary remains unchanged.
- The example manifest explicitly allows only Clippy's `let_and_return` style lint for
  the deliberately retained binding; no lint annotation or rewrite was inserted into
  its source. No macro runtime behavior or third-party dependency versions changed.

## Verification and completion boundary

- **291 workspace tests pass**, with three ignored tests: two explicit inspection
  printers (run separately) and the contextual macro doctest. Documentation fragments
  are exercised through workspace consumers, not claimed to be standalone doctests.
- Formatting, configured strict all-target Clippy, warning-denying rustdoc generation,
  the basic binary, the new library build, and both explicit inspection commands pass.
- Relative documentation links/anchors, new source/config code fences, configured line
  range, and baseline/report equality are checked. SHA-256 hashes match for all 26 prior
  example/fixture inputs and the new source frozen before extraction metadata was added.
- Independent read-only review found no concrete inaccurate claim or broken workflow,
  and independently ran the new tests, printer, and library build.
- All task acceptance criteria are now met. This completes the milestone on stacked
  branches only: no automatic merge, release, general compiler inference, arbitrary
  source discovery, or other deferred feature is implied. Test counts are not a new
  coverage measurement or formal correctness guarantee.
