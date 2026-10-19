//! Load named TOML definitions and resolve paths relative to their configuration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use syn::{Expr, Lit, LitBool, LitInt, LitStr};

use crate::arguments::{Args, Context, Invocation, snapshot_option};
use crate::source::read_text;

pub(crate) struct Resolved {
    pub args: Args,
    pub snippet: String,
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
    snapshot: Option<SnapshotDefinition>,
    context_snapshot: Option<String>,
    context_report: Option<String>,
    context: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SnapshotDefinition {
    Path(String),
    Switch(bool),
}

impl Invocation {
    pub fn resolve(self, root: &Path) -> syn::Result<Resolved> {
        match self {
            Self::Inline(mut args) => {
                args.resolve_snapshot()?;
                Ok(Resolved {
                    snippet: args.fn_sig.value(),
                    args: *args,
                    root: root.into(),
                    config: None,
                    context: String::new(),
                })
            }
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
                let mode = definition
                    .context
                    .as_deref()
                    .map(Context::parse)
                    .transpose()
                    .map_err(|message| {
                        syn::Error::new(config.span(), format!("{context}: {message}"))
                    })?
                    .unwrap_or_default();
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
                let snapshot = definition
                    .snapshot
                    .map(|value| {
                        let literal = match value {
                            SnapshotDefinition::Path(path) => {
                                Lit::Str(LitStr::new(&path, config.span()))
                            }
                            SnapshotDefinition::Switch(value) => {
                                Lit::Bool(LitBool::new(value, config.span()))
                            }
                        };
                        snapshot_option(literal).map_err(|error| {
                            syn::Error::new(config.span(), format!("{context}: {error}"))
                        })
                    })
                    .transpose()?;
                let mut args = Args {
                    context: mode,
                    fn_sig: LitStr::new(&definition.fn_sig, config.span()),
                    module: LitStr::new(&definition.module, config.span()),
                    start: LitInt::new(&definition.start.to_string(), config.span()),
                    end: LitInt::new(&definition.end.to_string(), config.span()),
                    output,
                    snapshot_disabled: matches!(snapshot, Some(None)),
                    snapshot: snapshot.flatten(),
                    context_snapshot: definition
                        .context_snapshot
                        .map(|value| LitStr::new(&value, config.span())),
                    context_report: definition
                        .context_report
                        .map(|value| LitStr::new(&value, config.span())),
                };
                args.validate_context_options()
                    .and_then(|()| args.resolve_snapshot())
                    .map_err(|error| {
                        syn::Error::new(config.span(), format!("{context}: {error}"))
                    })?;
                Ok(Resolved {
                    snippet: snippet.value(),
                    args,
                    root: path.parent().unwrap_or(root).into(),
                    config: Some(path),
                    context,
                })
            }
        }
    }
}
