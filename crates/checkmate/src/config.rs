//! Load named TOML definitions and resolve paths relative to their configuration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use syn::{Expr, LitInt, LitStr};

use crate::arguments::{Args, Invocation};
use crate::source::read_text;

pub(crate) struct Resolved {
    pub args: Args,
    pub root: PathBuf,
    pub config: Option<PathBuf>,
    pub context: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    snippets: BTreeMap<String, Definition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    fn_sig: String,
    module: String,
    start: usize,
    end: usize,
    output: Option<String>,
    snapshot: Option<String>,
}

impl Invocation {
    pub fn resolve(self, root: &Path) -> syn::Result<Resolved> {
        match self {
            Self::Inline(args) => Ok(Resolved {
                args: *args,
                root: root.into(),
                config: None,
                context: String::new(),
            }),
            Self::Config { config, snippet } => {
                let path = root.join(config.value());
                let context = format!("{} [snippets.{}]", path.display(), snippet.value());
                let text = read_text(&path, config.span())?;
                let mut definitions: Config = toml::from_str(&text).map_err(|error| {
                    syn::Error::new(
                        config.span(),
                        format!("{}: invalid configuration: {error}", path.display()),
                    )
                })?;
                let definition =
                    definitions
                        .snippets
                        .remove(&snippet.value())
                        .ok_or_else(|| {
                            syn::Error::new(snippet.span(), format!("{context}: snippet not found"))
                        })?;
                let output = definition
                    .output
                    .map(|value| {
                        LitStr::new(&value, config.span())
                            .parse::<Expr>()
                            .map_err(|error| {
                                syn::Error::new(
                                    config.span(),
                                    format!("{context}: invalid output expression: {error}"),
                                )
                            })
                    })
                    .transpose()?;
                Ok(Resolved {
                    args: Args {
                        fn_sig: LitStr::new(&definition.fn_sig, config.span()),
                        module: LitStr::new(&definition.module, config.span()),
                        start: LitInt::new(&definition.start.to_string(), config.span()),
                        end: LitInt::new(&definition.end.to_string(), config.span()),
                        output,
                        snapshot: definition
                            .snapshot
                            .map(|value| LitStr::new(&value, config.span())),
                    },
                    root: path.parent().unwrap_or(root).into(),
                    config: Some(path),
                    context,
                })
            }
        }
    }
}
