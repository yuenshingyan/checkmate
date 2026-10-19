//! Structured internal errors and a bounded, IO-free diagnostic renderer.

use std::fmt::{self, Write};
use std::ops::Deref;
use std::path::Path;

use proc_macro2::Span;

use crate::scope::DeclarationKind;
use crate::source::ast::SourceRange;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Syntax,
    Selection,
    SourceIdentity,
    MissingInput,
    UnknownItem,
    InaccessibleItem,
    Macro,
    Conditional,
    UnsafePromotion,
    UnsupportedContext,
    MethodResolution,
    InternalInvariant,
    ContextSnapshot,
}

impl Kind {
    fn guidance(self) -> (&'static str, &'static str) {
        match self {
            Self::Syntax => (
                "CM001",
                "Provide a complete, parseable Rust source file and valid supplied signature/output syntax.",
            ),
            Self::Selection => (
                "CM002",
                "Select complete statements within one supported block.",
            ),
            Self::SourceIdentity => (
                "CM003",
                "Verify that the requested source path identifies the intended source file.",
            ),
            Self::MissingInput => (
                "CM004",
                "Add the known input as a typed fn_sig parameter; do not copy or run its initializer.",
            ),
            Self::UnknownItem => (
                "CM005",
                "Check the item path against Checkmate's supported lexical context.",
            ),
            Self::InaccessibleItem => (
                "CM006",
                "Keep original visibility; use an already accessible API or ordinary extraction in an allowed scope.",
            ),
            Self::Macro => (
                "CM007",
                "Use ordinary extraction with explicit test context; automatic analysis does not expand macros.",
            ),
            Self::Conditional => (
                "CM008",
                "Use ordinary extraction with explicit test context; automatic analysis does not evaluate cfg.",
            ),
            Self::UnsafePromotion => (
                "CM009",
                "Use ordinary extraction with explicit test context; this declaration cannot be safely promoted.",
            ),
            Self::UnsupportedContext => (
                "CM010",
                "Use ordinary extraction with explicit test context, or choose a supported selection.",
            ),
            Self::MethodResolution => (
                "CM011",
                "Use ordinary extraction with explicit test context, or select an existing supported qualified call.",
            ),
            Self::InternalInvariant => (
                "CM012",
                "Report this Checkmate failure with a minimal reproduction.",
            ),
            Self::ContextSnapshot => (
                "CM013",
                "Resolve the reported identity/name issue first if present, then inspect context_report and explicitly review/create/update the baseline; compilation never writes it.",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Origin {
    Source,
    Signature,
    Output,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Site {
    pub origin: Origin,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Subject {
    pub name: String,
    pub kind: Option<DeclarationKind>,
}

#[derive(Debug)]
pub(crate) struct Details {
    pub kind: Kind,
    pub message: String,
    pub site: Option<Site>,
    pub subject: Option<Subject>,
    pub chain: Vec<Subject>,
    pub omitted_chain: usize,
    span: Span,
}

/// Keep the error-sized return path to one pointer, even with rich metadata.
#[derive(Debug)]
pub(crate) struct Error(Box<Details>);

impl Deref for Error {
    type Target = Details;

    fn deref(&self) -> &Details {
        &self.0
    }
}

impl Error {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self(Box::new(Details {
            kind: Kind::UnsupportedContext,
            message: message.into(),
            site: None,
            subject: None,
            chain: Vec::new(),
            omitted_chain: 0,
            span,
        }))
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.0.kind = kind;
        self
    }

    pub fn with_site(mut self, origin: Origin, range: SourceRange) -> Self {
        self.0.site = Some(Site { origin, range });
        self
    }

    pub fn with_subject(mut self, name: impl Into<String>, kind: Option<DeclarationKind>) -> Self {
        self.0.subject = Some(Subject {
            name: name.into(),
            kind,
        });
        self
    }

    pub fn with_chain(mut self, chain: Vec<Subject>, omitted: usize) -> Self {
        self.0.chain = chain;
        self.0.omitted_chain = omitted;
        self
    }

    pub fn context(mut self, message: impl Into<String>) -> Self {
        self.0.message = message.into();
        self
    }

    pub fn render(&self, request: &Request<'_>, source: &str, generated: Option<&str>) -> String {
        let mut out = Bounded::new(MAX_BYTES);
        let (code, remedy) = self.kind.guidance();
        let _ = writeln!(
            out,
            "Checkmate [{code}] {:?}: {}",
            self.kind,
            field(&self.message, 1024)
        );
        let _ = writeln!(out, "snippet: {}", field(request.snippet, 384));
        let _ = writeln!(
            out,
            "source file: {}",
            field(&request.path.to_string_lossy(), 512)
        );
        let _ = writeln!(
            out,
            "source selection: lines {}..={} (1-based)",
            request.start, request.end
        );
        let _ = writeln!(out, "help: {remedy}");
        let _ = writeln!(
            out,
            "note: Checkmate context/resolution diagnostics do not remap rustc type, borrow, or privacy errors."
        );
        if let Some(subject) = &self.subject {
            let _ = write!(out, "subject: ");
            render_subject(&mut out, subject);
            let _ = writeln!(out);
        }
        if !self.chain.is_empty() || self.omitted_chain != 0 {
            let _ = writeln!(out, "dependency chain:");
            // Preserve both ends, without traversing or formatting the hidden middle.
            let head = self.chain.len().min(3);
            let tail = self.chain.len().saturating_sub(head).min(3);
            for subject in &self.chain[..head] {
                let _ = write!(out, "  -> ");
                render_subject(&mut out, subject);
                let _ = writeln!(out);
            }
            let omitted = self
                .omitted_chain
                .saturating_add(self.chain.len() - head - tail);
            if omitted != 0 {
                let _ = writeln!(out, "  ... {omitted} dependencies omitted ...");
            }
            for subject in &self.chain[self.chain.len() - tail..] {
                let _ = write!(out, "  -> ");
                render_subject(&mut out, subject);
                let _ = writeln!(out);
            }
        }
        match self.site {
            Some(Site { origin, range }) => {
                let label = match origin {
                    Origin::Source => "source",
                    Origin::Signature => "generated signature",
                    Origin::Output => "generated output",
                };
                let _ = writeln!(
                    out,
                    "{label} location: {}:{}..{}:{} (1-based lines, 0-based columns; end exclusive)",
                    range.start.line, range.start.column, range.end.line, range.end.column
                );
                let text = match origin {
                    Origin::Source => Some(source),
                    Origin::Signature | Origin::Output => generated,
                };
                if let Some(text) = text {
                    // An exclusive end at column zero does not include that line.
                    let end = if range.end.column == 0 && range.end.line > range.start.line {
                        range.end.line - 1
                    } else {
                        range.end.line
                    };
                    preview(&mut out, label, text, range.start.line, end);
                } else {
                    let _ = writeln!(out, "{label} preview unavailable");
                }
            }
            None => preview(
                &mut out,
                "source selection",
                source,
                request.start,
                request.end,
            ),
        }
        out.finish()
    }

    /// Anchor the compile_error! at the invocation, not a synthetic/parser span.
    pub fn into_syn_error(
        self,
        invocation: Span,
        request: &Request<'_>,
        source: &str,
        generated: Option<&str>,
    ) -> syn::Error {
        syn::Error::new(invocation, self.render(request, source, generated))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<syn::Error> for Error {
    fn from(error: syn::Error) -> Self {
        Self::new(error.span(), error.to_string()).with_kind(Kind::Syntax)
    }
}

pub(crate) struct Request<'a> {
    pub snippet: &'a str,
    pub path: &'a Path,
    pub start: usize,
    pub end: usize,
}

const MAX_BYTES: usize = 8192;
const TRUNCATED: &str = "…[truncated]";

/// Reserves space for its truncation marker and never cuts a UTF-8 code point.
struct Bounded {
    text: String,
    capacity: usize,
    truncated: bool,
}

impl Bounded {
    fn new(capacity: usize) -> Self {
        Self {
            text: String::new(),
            capacity,
            truncated: false,
        }
    }

    fn finish(mut self) -> String {
        if self.truncated {
            self.text.push_str(TRUNCATED);
        }
        self.text
    }
}

impl Write for Bounded {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        let remaining = self
            .capacity
            .saturating_sub(TRUNCATED.len() + self.text.len());
        let mut end = remaining.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&text[..end]);
        self.truncated = end < text.len();
        Ok(())
    }
}

fn unsafe_character(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

fn field(text: &str, bytes: usize) -> String {
    let mut out = Bounded::new(bytes);
    for c in text.chars() {
        if unsafe_character(c) {
            let _ = write!(out, "{}", c.escape_default());
        } else {
            let _ = out.write_char(c);
        }
        if out.truncated {
            break;
        }
    }
    out.finish()
}

fn render_subject(out: &mut Bounded, subject: &Subject) {
    let _ = write!(out, "{}", field(&subject.name, 192));
    if let Some(kind) = subject.kind {
        let _ = write!(out, " ({kind:?})");
    }
}

fn preview(out: &mut Bounded, label: &str, text: &str, start: usize, end: usize) {
    let _ = writeln!(out, "{label} preview:");
    if start == 0 || end < start {
        let _ = writeln!(out, "  (invalid line range)");
        return;
    }
    let count = end.saturating_sub(start).saturating_add(1).min(3);
    let mut shown = 0;
    for (offset, line) in text.lines().skip(start - 1).take(count).enumerate() {
        // The preview payload, including escape expansions and truncation marker,
        // is at most 160 Unicode characters, not just 160 input characters.
        let mut payload = String::new();
        let mut chars = 0;
        let mut clipped = false;
        for c in line.chars() {
            let escaped = if unsafe_character(c) {
                c.escape_default().to_string()
            } else {
                c.to_string()
            };
            let width = escaped.chars().count();
            if chars + width > 160 - TRUNCATED.chars().count() {
                clipped = true;
                break;
            }
            payload.push_str(&escaped);
            chars += width;
        }
        if clipped {
            payload.push_str(TRUNCATED);
        }
        let _ = writeln!(out, "  {} | {payload}", start.saturating_add(offset));
        shown += 1;
    }
    if shown == 0 {
        let _ = writeln!(out, "  (line unavailable)");
    } else if end.saturating_sub(start) >= 3 {
        let _ = writeln!(out, "  ... additional lines omitted ...");
    }
}

#[cfg(test)]
mod integration;
#[cfg(test)]
mod tests;
