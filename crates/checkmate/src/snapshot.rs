//! Read-only comparison against an explicitly reviewed snippet baseline.

pub(crate) mod context;
mod identity;

use crate::source::bounded_line;

pub(crate) fn snapshot_difference(baseline: &str, actual: &[&str], start: usize) -> Option<String> {
    // Normalize CRLF and the presence of one final newline only. Whitespace and
    // comments remain significant, avoiding silent acceptance of source edits.
    let expected: Vec<_> = baseline.lines().collect();
    (0..expected.len().max(actual.len())).find_map(|index| {
        let before = expected.get(index);
        let after = actual.get(index);
        (before != after).then(|| format!(
            "first difference at source line {} (snapshot line {}):\n  expected: {}\n    actual: {}",
            start + index, index + 1,
            before.map_or_else(|| "<end of snapshot>".into(), |line| format!("{:?}", bounded_line(line))),
            after.map_or_else(|| "<end of selection>".into(), |line| format!("{:?}", bounded_line(line))),
        ))
    })
}
