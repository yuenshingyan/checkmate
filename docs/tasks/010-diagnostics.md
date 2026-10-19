# 010 — Report unsupported context precisely

- **Status:** Complete (internal/test-gated; public integration remains task 011)
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

## Implementation and evidence

`diagnostics.rs` supplies a boxed structured error, typed categories/codes, optional
source/generated sites, identifier/declaration subjects, dependency chains, remedies,
and a pure bounded renderer. Internal AST, origin, scope, resolver, graph, implementation,
and promotion APIs now carry that error type instead of discarding metadata into strings.

Reference and dependency wrappers preserve the originating category and subject. Graph
errors retain bounded root/leaf chain entries with an exact omitted count. Discovery
ancestry is retained after graph success so promotion-policy failures also report chains.
Unknown type/
const arguments and transitive inaccessible originals retain their leaf category, rather
than becoming generic promotion errors. Source locations are distinguished from supplied
signature/output positions, including failures while indexing the analysis shell.

Reports require configured snippet identity, original file, and requested inclusive range.
They are limited to 8,192 UTF-8 bytes; previews have at most three lines and 160 payload
characters per line. Controls/directional characters are escaped. Invocation-anchored
`into_syn_error` is explicit; there is no implicit conversion that loses metadata.
Legacy internal `Display` messages are not the public reporting boundary.

24 new tests cover categories, metadata preservation, macro/cfg/derive barriers, missing
inputs, privacy/unknown targets, unsafe promotion, actual dependency failures, a 501-node
Unicode alias chain, extreme ranges, oversized fields, generated/source preview separation,
determinism, compile-error anchoring, and byte-for-byte source/config/manifest/snapshot
preservation on failure. Existing compiler-backed privacy tests remain unchanged; rustc
errors are not reclassified as Checkmate analysis failures.

The existing missing-input walkthrough now renders a structured report using the actual
struct-return example configuration. See the [diagnostic guide](../diagnostics.md).

## Follow-up boundary

Task 011 must supply the verified invocation span, configured/inline identity, and source
text to this reporting boundary. No public option or automatic pipeline is enabled here.
Existing public extraction/configuration error behavior remains unchanged. Context
baselines remain task 012; diagnostics never create or approve baselines.
