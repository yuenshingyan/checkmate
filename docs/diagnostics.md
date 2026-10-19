# Automatic-context diagnostics

Task 010 supplies structured analysis errors; task 011 connects them to public
`context = "auto"` with verified invocation context. Ordinary extraction retains its
existing diagnostics. Parser/configuration errors remain ordinary `syn` errors.

## Runnable example

From the repository root:

```sh
cargo test -p checkmate --lib walkthroughs::missing_input -- --nocapture
cargo test -p checkmate --lib diagnostics
```

The walkthrough reads the existing struct-return example and deliberately omits `name`
from the supplied signature. It prints an expected diagnostic and passes:

```text
Checkmate [CM004] MissingInput: ... required input `name` is not supplied by fn_sig ...
snippet: build_user
source file: src/lib.rs
source selection: lines 17..=20 (1-based)
help: Add the known input as a typed fn_sig parameter; do not copy or run its initializer.
...
subject: name (Local)
source location: 18:8..18:12 (1-based lines, 0-based columns; end exclusive)
source preview:
  18 |         name,
```

The original source stays untouched. Tests compare source, configuration, manifest,
and snapshot bytes before and after failing analysis. The diagnostic renderer itself
performs no IO. It never creates or updates a baseline.

## Categories

These codes identify public automatic-analysis reports; exact message wording is not
a stable machine-readable API.
Categories are assigned at error creation and preserved through context/dependency
wrappers; they are **not inferred by matching error-message strings**.

| Code | Category |
| --- | --- |
| CM001 | Rust syntax |
| CM002 | Invalid or unsupported selection |
| CM003 | Unproven source/target identity |
| CM004 | Missing explicitly typed input |
| CM005 | Unknown item or identifier |
| CM006 | Inaccessible original item or access route |
| CM007 | Unsupported macro expansion, including derives |
| CM008 | Unsupported conditional compilation |
| CM009 | Unsafe or unsupported promotion |
| CM010 | Other unsupported context |
| CM011 | Unproven method/trait resolution |
| CM012 | Internal analysis invariant failure |

A structured error retains a category, message, optional identifier and known declaration
kind, optional location, and dependency subjects with an omission count. Unknown kinds
are left unknown rather than invented. A transitive private/unknown dependency retains
its leaf category even when discovered while promoting another type. Graph ancestry
also survives into later promotion-policy failures, including selected declarations.

## Locations and bounds

- Every rendered report identifies the configured snippet (or a caller-supplied inline
  label), original file, and requested inclusive line range.
- Source positions are distinct from normalized **generated signature/output** positions.
  A generated location never receives a preview from the original file. If its analysis
  text is unavailable, the report says so. Without a precise site, the preview is labeled
  as the selection, not presented as the actual failing declaration.
- Structured positions use one-based lines, zero-based Unicode character columns, and
  exclusive ends. Existing explanatory messages may retain one-based human columns;
  the structured location line explicitly states its convention.
- Rendered output is at most **8,192 UTF-8 bytes**, including user-controlled paths,
  snippet names, messages, identifiers, chains, and previews. UTF-8 code points are never
  split. Controls and invisible directional characters are escaped.
- Previews show at most **three lines**, each with at most **160 payload characters**
  including escapes/truncation markers. Dependency displays retain both ends and state
  how many entries were omitted. Individual fields are independently bounded.
- A real 501-node Unicode alias chain exercises traversal, leaf metadata, bounded output,
  deterministic ordering, and omission accounting—not just a fabricated display string.

## Compiler boundary and limitations

`diagnostics::Error` is the internal result type. Context wrappers preserve its metadata.
`render` requires a `Request` and source text; `into_syn_error` additionally requires the
actual invocation span, anchoring the generated `compile_error!` there rather than at a
synthetic parser location. There is deliberately no implicit conversion back to
`syn::Error` that could discard metadata or bypass bounded rendering. Legacy `Display`
contains the internal explanatory message only; it is **not** the user-facing renderer.

Checkmate's analysis errors are separate from rustc visibility, type, and borrow errors.
The planner can reject an unproven public route; rustc still decides field/method privacy
and typing when generated code is compiled. This feature does not remap those compiler
errors or claim that syntactic analysis proves semantic equivalence.

Whole-file indexing may still reject unsupported syntax outside the selection. Remedies
therefore allow ordinary extraction with explicit test context; they do not promise that
moving a line range will solve every barrier. No remedy widens original visibility,
runs an initializer, guesses an input type, or silently rewrites a signature. Full public
pipeline integration and context snapshots remain tasks 011 and 012.
