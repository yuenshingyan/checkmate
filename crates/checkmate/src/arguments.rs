//! Parse the public macro syntax; configuration-file loading lives in `config`.

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitInt, LitStr, Token};

pub(crate) struct Args {
    pub fn_sig: LitStr,
    pub module: LitStr,
    pub start: LitInt,
    pub end: LitInt,
    pub output: Option<Expr>,
    pub snapshot: Option<LitStr>,
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
                "snapshot" => set_once(&mut snapshot, input.parse()?, &key)?,
                "config" => set_once(&mut config, input.parse()?, &key)?,
                "snippet" => set_once(&mut snippet, input.parse()?, &key)?,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown option; expected fn_sig, module, start, end, output, snapshot, config, or snippet",
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
            {
                return Err(input.error("config/snippet cannot be mixed with inline options"));
            }
            return Ok(Self::Config {
                config: config.ok_or_else(|| input.error("missing config"))?,
                snippet: snippet.ok_or_else(|| input.error("missing snippet"))?,
            });
        }
        Ok(Self::Inline(Box::new(Args {
            fn_sig: fn_sig.ok_or_else(|| input.error("missing fn_sig"))?,
            module: module.ok_or_else(|| input.error("missing module"))?,
            start: start.ok_or_else(|| input.error("missing start"))?,
            end: end.ok_or_else(|| input.error("missing end"))?,
            output,
            snapshot,
        })))
    }
}

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
