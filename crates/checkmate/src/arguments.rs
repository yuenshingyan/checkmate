//! Parse the public macro syntax; configuration-file loading lives in `config`.

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Lit, LitInt, LitStr, Token};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Context {
    Explicit,
    #[default]
    Auto,
}

impl Context {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "auto" => Ok(Self::Auto),
            "explicit" => Ok(Self::Explicit),
            _ => Err("invalid context; expected \"auto\" or \"explicit\""),
        }
    }
}

pub(crate) struct Args {
    pub context: Context,
    pub fn_sig: LitStr,
    pub module: LitStr,
    pub start: LitInt,
    pub end: LitInt,
    pub output: Option<Expr>,
    /// Explicit path, or the effective default after invocation resolution.
    pub snapshot: Option<LitStr>,
    /// Only an explicit `snapshot = false` disables the guard.
    pub snapshot_disabled: bool,
    pub context_snapshot: Option<LitStr>,
    /// Requested test-only review accessor identifier, not a path or write option.
    pub context_report: Option<LitStr>,
}

impl Args {
    pub(crate) fn resolve_snapshot(&mut self) -> syn::Result<()> {
        if self.snapshot.is_none() && !self.snapshot_disabled {
            let template: crate::generate::FunctionTemplate =
                self.fn_sig.parse().map_err(|error| {
                    syn::Error::new(self.fn_sig.span(), format!("invalid fn_sig: {error}"))
                })?;
            let name = template.signature.ident.to_string();
            let name = name.trim_start_matches("r#");
            self.snapshot = Some(LitStr::new(
                &format!("snapshots/{name}.rs"),
                self.fn_sig.span(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_context_options(&self) -> syn::Result<()> {
        for (name, value) in [
            ("context_snapshot", &self.context_snapshot),
            ("context_report", &self.context_report),
        ] {
            if let Some(value) = value
                && self.context != Context::Auto
            {
                return Err(syn::Error::new(
                    value.span(),
                    format!("{name} requires context = \"auto\""),
                ));
            }
        }
        if let Some(value) = &self.context_report
            && !value.parse::<Ident>().is_ok_and(|name| name != "_")
        {
            return Err(syn::Error::new(
                value.span(),
                "context_report must be one Rust identifier other than `_`",
            ));
        }
        Ok(())
    }
}

pub(crate) enum Invocation {
    Inline(Box<Args>),
    Config { config: LitStr, snippet: LitStr },
}

impl Invocation {
    pub fn span(&self) -> Span {
        match self {
            Self::Inline(args) => args.module.span(),
            Self::Config { config, .. } => config.span(),
        }
    }
}

impl Parse for Invocation {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut fn_sig = None;
        let mut module = None;
        let mut start = None;
        let mut end = None;
        let mut output = None;
        let mut snapshot = None;
        let mut context_snapshot = None;
        let mut context_report = None;
        let mut context = None;
        let mut config = None;
        let mut snippet = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "fn_sig" => set_once(&mut fn_sig, input.parse()?, &key)?,
                "module" => set_once(&mut module, input.parse()?, &key)?,
                "start" => set_once(&mut start, input.parse()?, &key)?,
                "end" => set_once(&mut end, input.parse()?, &key)?,
                "output" => set_once(&mut output, input.parse()?, &key)?,
                "snapshot" => set_once(&mut snapshot, snapshot_option(input.parse()?)?, &key)?,
                "context_snapshot" => set_once(&mut context_snapshot, input.parse()?, &key)?,
                "context_report" => set_once(&mut context_report, input.parse()?, &key)?,
                "context" => {
                    let value: LitStr = input.parse()?;
                    let mode = Context::parse(&value.value())
                        .map_err(|message| syn::Error::new(value.span(), message))?;
                    set_once(&mut context, mode, &key)?;
                }
                "config" => set_once(&mut config, input.parse()?, &key)?,
                "snippet" => set_once(&mut snippet, input.parse()?, &key)?,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown option; expected fn_sig, module, start, end, output, snapshot, context_snapshot, context_report, context, config, or snippet",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        if config.is_some() || snippet.is_some() {
            if fn_sig.is_some()
                || module.is_some()
                || start.is_some()
                || end.is_some()
                || output.is_some()
                || snapshot.is_some()
                || context_snapshot.is_some()
                || context_report.is_some()
                || context.is_some()
            {
                return Err(input.error("config/snippet cannot be mixed with inline options"));
            }
            return Ok(Self::Config {
                config: config.ok_or_else(|| input.error("missing config"))?,
                snippet: snippet.ok_or_else(|| input.error("missing snippet"))?,
            });
        }
        let args = Args {
            fn_sig: fn_sig.ok_or_else(|| input.error("missing fn_sig"))?,
            module: module.ok_or_else(|| input.error("missing module"))?,
            start: start.ok_or_else(|| input.error("missing start"))?,
            end: end.ok_or_else(|| input.error("missing end"))?,
            output,
            snapshot_disabled: matches!(snapshot, Some(None)),
            snapshot: snapshot.flatten(),
            context_snapshot,
            context_report,
            context: context.unwrap_or_default(),
        };
        args.validate_context_options()?;
        Ok(Self::Inline(Box::new(args)))
    }
}

/// Shared inline/TOML validation. Omission is handled separately from false.
pub(crate) fn snapshot_option(value: Lit) -> syn::Result<Option<LitStr>> {
    match value {
        Lit::Str(path) if !path.value().is_empty() => Ok(Some(path)),
        Lit::Bool(flag) if !flag.value => Ok(None),
        other => Err(syn::Error::new(
            other.span(),
            "snapshot must be a non-empty path string or false; omit it to use snapshots/<function-name>.rs",
        )),
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
#[cfg(test)]
#[path = "snapshot_options_tests.rs"]
mod snapshot_options_tests;

fn set_once<T>(slot: &mut Option<T>, value: T, key: &Ident) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            key.span(),
            format!("duplicate option `{key}`"),
        ));
    }
    *slot = Some(value);
    Ok(())
}
