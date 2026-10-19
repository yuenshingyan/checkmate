//! Resolve an invocation, validate its snippet, and emit test-only Rust items.

use std::path::Path;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Block, Signature, Stmt, Token, Visibility};

use crate::arguments::{Args, Invocation};
use crate::snapshot::snapshot_difference;
use crate::source::{path_literal, preview, read_text};

pub(crate) struct FunctionTemplate {
    visibility: Visibility,
    signature: Signature,
}

impl Parse for FunctionTemplate {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let visibility = input.parse()?;
        let signature = input.parse()?;
        if input.peek(syn::token::Brace) {
            let body: Block = input.parse()?;
            if !body.stmts.is_empty() {
                return Err(input.error("fn_sig must not contain a function implementation"));
            }
        } else if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        Ok(Self {
            visibility,
            signature,
        })
    }
}

pub(crate) fn expand(invocation: Invocation, root: &Path) -> syn::Result<TokenStream> {
    let resolved = invocation.resolve(root)?;
    let span = resolved.args.module.span();
    let result = expand_snippet(resolved.args, &resolved.root).and_then(|items| {
        if let Some(config) = resolved.config {
            let path = path_literal(&config, span)?;
            Ok(quote! {
                #[cfg(test)]
                const _: &str = include_str!(#path);
                #items
            })
        } else {
            Ok(items)
        }
    });
    result.map_err(|error| {
        if resolved.context.is_empty() {
            error
        } else {
            syn::Error::new(error.span(), format!("{}: {error}", resolved.context))
        }
    })
}

fn expand_snippet(args: Args, root: &Path) -> syn::Result<TokenStream> {
    let path = root.join(args.module.value());
    let context = format!("{}:{}-{}", path.display(), args.start, args.end);
    let fail = |span, message: String| syn::Error::new(span, format!("{context}: {message}"));
    let template: FunctionTemplate = args
        .fn_sig
        .parse()
        .map_err(|error| fail(args.fn_sig.span(), format!("invalid fn_sig: {error}")))?;
    let start = args.start.base10_parse::<usize>()?;
    let end = args.end.base10_parse::<usize>()?;
    if start == 0 {
        return Err(fail(args.start.span(), "start must be at least 1".into()));
    }
    if end < start {
        return Err(fail(args.end.span(), "end must be at least start".into()));
    }
    let source = read_text(&path, args.module.span())
        .map_err(|error| fail(args.module.span(), error.to_string()))?;
    let lines: Vec<_> = source.lines().collect();
    if end > lines.len() {
        return Err(fail(
            args.end.span(),
            format!(
                "source has only {} lines; update the selected range",
                lines.len()
            ),
        ));
    }
    let selected = &lines[start - 1..end];
    let snippet = selected.join("\n");
    let mut dependencies = vec![path_literal(&path, args.module.span())?];

    // Check the baseline before parsing, so moved/incomplete code gets a drift
    // diagnostic rather than just an unexpected-token error.
    if let Some(snapshot) = args.snapshot {
        let snapshot_path = root.join(snapshot.value());
        let baseline = read_text(&snapshot_path, snapshot.span()).map_err(|error| {
            fail(snapshot.span(), format!("{error}; create a reviewed snapshot of the selected lines (snapshots are never written automatically)"))
        })?;
        if let Some(difference) = snapshot_difference(&baseline, selected, start) {
            return Err(fail(
                snapshot.span(),
                format!(
                    "snippet differs from snapshot {}\n{difference}\nCheck for moved lines and update start/end; if the change is intentional, review and replace the snapshot. No files were changed.",
                    snapshot_path.display(),
                ),
            ));
        }
        dependencies.push(path_literal(&snapshot_path, snapshot.span())?);
    }

    let mut body: Block = syn::parse_str(&format!("{{\n{snippet}\n}}"))
        .map_err(|error| fail(args.module.span(), format!(
            "not a complete Rust statement/expression sequence: {error}\n{}\nSelect complete statements or an expression, including closing delimiters.",
            preview(selected, start),
        )))?;
    if let Some(output) = args.output {
        body.stmts.push(Stmt::Expr(output, None));
        body = syn::parse2(quote! { #body }).map_err(|error| {
            fail(args.module.span(), format!("cannot append output to snippet; omit output if the snippet already has a tail expression: {error}"))
        })?;
    }

    let FunctionTemplate {
        visibility,
        signature,
    } = template;
    Ok(quote! {
        // Track external files even when they are not Rust modules.
        #(
            #[cfg(test)]
            const _: &str = include_str!(#dependencies);
        )*
        #[cfg(test)]
        #visibility #signature #body
    })
}
