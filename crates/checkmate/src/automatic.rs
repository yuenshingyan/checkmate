//! Opt-in orchestration. File analysis uses the fallback parser, whose spans
//! refer to the parsed text rather than the compiler invocation. The macro entry
//! serializes expansion while this process-global backend switch is active.
mod placement;
#[cfg(test)]
mod tests;

use crate::{
    config,
    diagnostics::{Error, Kind, Origin, Request, Result},
    generate::FunctionTemplate,
    resolve::{
        collect_references,
        declarations::{Resolver, ReuseSite},
        promote::PromotionPlan,
    },
    scope::ScopeIndex,
    source::{
        ast::{ParsedSource, SourceRange},
        origin::SourceOrigin,
    },
};
pub(crate) use placement::Caller;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

struct Input {
    package: PathBuf,
    source: PathBuf,
    config: Option<PathBuf>,
    snapshot: Option<PathBuf>,
    snippet: String,
    signature: String,
    output: Option<String>,
    start: usize,
    end: usize,
    caller: Caller,
}

pub(crate) fn expand(
    resolved: config::Resolved,
    package: &Path,
    caller: Option<Caller>,
) -> syn::Result<TokenStream> {
    let span = resolved.args.module.span();
    let source = resolved.root.join(resolved.args.module.value());
    let start = resolved.args.start.base10_parse()?;
    let end = resolved.args.end.base10_parse()?;
    let Some(caller) = caller else {
        return Err(Error::new(
            Span::call_site(),
            "automatic context requires compiler invocation metadata; placement is never guessed",
        )
        .with_kind(Kind::SourceIdentity)
        .into_syn_error(
            span,
            &Request {
                snippet: &resolved.snippet,
                path: &source,
                start,
                end,
            },
            "",
            None,
        ));
    };
    let snippet_label = resolved.config.as_ref().map_or_else(
        || resolved.snippet.clone(),
        |path| format!("{} (configuration {})", resolved.snippet, path.display()),
    );
    let input = Input {
        package: package.to_owned(),
        source,
        config: resolved.config,
        snapshot: resolved
            .args
            .snapshot
            .map(|path| resolved.root.join(path.value())),
        snippet: snippet_label,
        signature: resolved.args.fn_sig.value(),
        output: resolved
            .args
            .output
            .map(|expr| expr.to_token_stream().to_string()),
        start,
        end,
        caller,
    };
    // Only plain data crosses the backend boundary. Compiler and fallback
    // tokens/spans must never mix. Restore even on unwind before constructing
    // the invocation-anchored compiler error or reparsing emitted tokens.
    let result = std::panic::catch_unwind(|| {
        let _backend = FileBackend::enter();
        run(input)
    })
    .map_err(|_| {
        syn::Error::new(
            span,
            "Checkmate [CM012]: automatic analysis failed; no partial expansion was emitted",
        )
    })?;
    let text = result.map_err(|message| syn::Error::new(span, message))?;
    text.parse().map_err(|_| syn::Error::new(span, "Checkmate [CM012]: generated automatic plan is not a token stream; no partial expansion was emitted"))
}

// proc_macro2 explicitly exposes this narrowly supported parser switch. Pin its
// version and serialize ALL macro entry paths, including ordinary extraction;
// a worker thread alone is insufficient because backend detection is global.
struct FileBackend;
impl FileBackend {
    fn enter() -> Self {
        proc_macro2::fallback::force();
        Self
    }
}
impl Drop for FileBackend {
    fn drop(&mut self) {
        proc_macro2::fallback::unforce();
    }
}

fn run(input: Input) -> std::result::Result<String, String> {
    let request = Request {
        snippet: &input.snippet,
        path: &input.source,
        start: input.start,
        end: input.end,
    };
    let mut source = String::new();
    let mut generated = None;
    let result = (|| -> Result<TokenStream> {
        source = read(&input.source)?;
        let lines: Vec<_> = source.lines().collect();
        if input.start == 0 || input.end < input.start || input.end > lines.len() {
            return Err(Error::new(
                Span::call_site(),
                "select a valid inclusive source line range",
            )
            .with_kind(Kind::Selection));
        }
        let mut dependencies: BTreeSet<_> = [input.source.clone()].into_iter().collect();
        if let Some(path) = &input.config {
            dependencies.insert(path.clone());
        }
        if let Some(snapshot) = &input.snapshot {
            let baseline = read(snapshot).map_err(|err| {
                let message = format!("{err}; create a reviewed snippet snapshot explicitly; compilation never writes baselines");
                err.context(message)
            })?;
            if let Some(difference) = crate::snapshot::snapshot_difference(
                &baseline,
                &lines[input.start - 1..input.end],
                input.start,
            ) {
                return Err(Error::new(Span::call_site(), format!("snippet differs from snapshot {}: {difference}; review changed source/ranges explicitly; no files were changed", snapshot.display())).with_kind(Kind::Selection));
            }
            dependencies.insert(snapshot.clone());
        }
        let template: FunctionTemplate = syn::parse_str(&input.signature).map_err(|err| {
            let range = SourceRange::from_span(err.span());
            Error::from(err).with_site(Origin::Signature, range)
        })?;
        let parsed = ParsedSource::parse(&input.source, source.clone())?;
        let selection = parsed.select(input.start, input.end)?;
        let origin = SourceOrigin::discover(&input.package, &input.source)?;
        let placement = placement::verify(
            &origin,
            &input.caller,
            &template.signature.ident.to_string(),
        )?;
        dependencies.extend([
            origin.source.clone(),
            origin.manifest.clone(),
            placement.caller,
        ]);
        if let Some(path) = &input.caller.file {
            dependencies.insert(input.package.join(path));
        }
        let index = ScopeIndex::build(&parsed)?;
        let output = input
            .output
            .as_ref()
            .map(|text| syn::parse_str(text))
            .transpose()?;
        let references = collect_references(
            &parsed,
            &index,
            &selection,
            &template.signature,
            output.as_ref(),
        )?;
        generated = Some(references.generated.text.clone());
        let resolver = Resolver::new(&parsed, &index, &origin, ReuseSite::PackageIntegrationTest)?;
        let mut plan =
            PromotionPlan::build(&resolver, &references, &selection, &placement.occupied)?;
        let Some(syn::Item::Fn(function)) = plan.items.last_mut() else {
            return Err(Error::new(
                Span::call_site(),
                "automatic plan has no extracted function",
            )
            .with_kind(Kind::InternalInvariant));
        };
        function.vis = template.visibility;
        if !plan.copied_nominal_types.is_empty() {
            function.attrs.push(syn::parse_quote!(#[doc = "Checkmate automatic context: copied function-local structs/enums have NEW nominal identity; they are not interchangeable with the original types."]));
        }
        let tokens = plan.tokens();
        let paths = dependencies
            .iter()
            .map(|path| crate::source::path_literal(path, Span::call_site()))
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(quote! {
            #( #[cfg(test)] const _: &str = ::core::include_str!(#paths); )*
            #tokens
        })
    })();
    result
        .map(|tokens| tokens.to_string())
        .map_err(|err| err.render(&request, &source, generated.as_deref()))
}
fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|err| {
        Error::new(
            Span::call_site(),
            format!("cannot read {}: {err}", path.display()),
        )
        .with_kind(Kind::SourceIdentity)
    })
}
