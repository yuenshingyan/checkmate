//! Conservative caller proof: a literal crate-root invocation in a uniquely
//! identified Cargo integration-test target. Never infer SameTarget from names.
use crate::{
    diagnostics::{Error, Kind, Result},
    source::origin::{SourceOrigin, TargetKind},
};
use proc_macro2::{LineColumn, Span};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use syn::visit::Visit;

#[derive(Clone, Debug)]
pub(crate) struct Caller {
    pub file: Option<PathBuf>,
    pub at: LineColumn,
    pub tokens: String,
    pub crate_name: Option<String>,
    pub package_name: Option<String>,
    pub bin_name: Option<String>,
}
impl Caller {
    pub fn capture(tokens: String) -> Self {
        let span = Span::call_site();
        Self {
            file: span.local_file().and_then(|path| {
                if path.is_absolute() {
                    Some(path)
                } else {
                    std::env::current_dir().ok().map(|cwd| cwd.join(path))
                }
            }),
            at: span.start(),
            tokens,
            crate_name: std::env::var("CARGO_CRATE_NAME").ok(),
            package_name: std::env::var("CARGO_PKG_NAME").ok(),
            bin_name: std::env::var("CARGO_BIN_NAME").ok(),
        }
    }
}
pub(super) struct Placement {
    pub occupied: BTreeSet<String>,
    pub caller: PathBuf,
}
fn fail(message: &str) -> Error {
    Error::new(Span::call_site(), message).with_kind(Kind::SourceIdentity)
}
fn normalized(name: &str) -> String {
    name.replace('-', "_")
}

pub(super) fn verify(
    origin: &SourceOrigin,
    caller: &Caller,
    function_name: &str,
) -> Result<Placement> {
    if origin.target_kind != TargetKind::Library {
        return Err(fail(
            "automatic context currently requires a library source root and a package integration-test caller; binary/same-target placement is not enabled",
        ));
    }
    if caller.bin_name.is_some() || caller.at.line == 0 {
        return Err(fail(
            "automatic context requires a verified integration-test invocation with compiler source locations",
        ));
    }
    let file = caller.file.as_ref().ok_or_else(|| {
        fail("compiler invocation file is unavailable; automatic context cannot verify placement")
    })?;
    let file = origin
        .package_root
        .join(file)
        .canonicalize()
        .map_err(|_| fail("cannot locate the compiler invocation file"))?;
    let manifest_text =
        std::fs::read_to_string(&origin.manifest).map_err(|e| fail(&e.to_string()))?;
    let manifest: toml::Value = toml::from_str(&manifest_text).map_err(|e| fail(&e.to_string()))?;
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| fail("missing package manifest"))?;
    let package_name = package
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| fail("missing package name"))?;
    if caller.package_name.as_deref() != Some(package_name) {
        return Err(fail(
            "compiler package identity does not match the source package",
        ));
    }
    if origin.dependency_candidates.contains(&origin.target_name) {
        return Err(fail(
            "a Cargo dependency alias shadows the source library target name",
        ));
    }
    let crate_name = caller
        .crate_name
        .as_deref()
        .ok_or_else(|| fail("compiler crate identity is unavailable"))?;
    // Cargo's custom-build targets are not binary targets (CARGO_BIN_NAME is
    // absent). A build script can include a test-root file, so matching file and
    // crate names would otherwise falsely approve that module as a test root.
    if crate_name.starts_with("build_script_") {
        return Err(fail(
            "caller crate name is reserved for Cargo build scripts; use a distinct integration-test target name",
        ));
    }
    if crate_name == origin.target_name || matches!(crate_name, "std" | "core") {
        return Err(fail(
            "caller target identity is ambiguous with the library or a standard-library root",
        ));
    }

    let tests = targets(
        &manifest,
        &origin.package_root,
        "test",
        "tests",
        enabled(package, "autotests")?,
    )?;
    let matching: Vec<_> = tests
        .iter()
        .filter(|(name, _)| normalized(name) == crate_name)
        .collect();
    if matching.len() != 1 || matching[0].1.canonicalize().ok().as_ref() != Some(&file) {
        return Err(fail(
            "invocation is not the unique declared Cargo integration-test root; modules, includes, and unproven target names are unsupported",
        ));
    }
    if tests
        .iter()
        .filter(|(_, path)| path.canonicalize().ok().as_ref() == Some(&file))
        .count()
        != 1
    {
        return Err(fail(
            "one caller source file belongs to multiple test targets",
        ));
    }
    // Exclude other target classes that can compile a module with the same
    // CARGO_CRATE_NAME. Names alone, without this manifest/layout check, are not proof.
    for (kind, directory, auto) in [
        ("bin", "src/bin", "autobins"),
        ("example", "examples", "autoexamples"),
        ("bench", "benches", "autobenches"),
    ] {
        if targets(
            &manifest,
            &origin.package_root,
            kind,
            directory,
            enabled(package, auto)?,
        )?
        .iter()
        .any(|(name, _)| normalized(name) == crate_name)
        {
            return Err(fail(
                "integration-test crate name overlaps another Cargo target; placement is ambiguous",
            ));
        }
    }
    if enabled(package, "autobins")?
        && origin.package_root.join("src/main.rs").is_file()
        && normalized(package_name) == crate_name
    {
        return Err(fail(
            "integration-test crate name overlaps the default binary target",
        ));
    }

    let text = std::fs::read_to_string(&file).map_err(|e| fail(&e.to_string()))?;
    let syntax =
        // Rustc normalizes CRLF before tokenizing, including inside multiline
        // literals. Normalize the read-only view likewise; lines/columns at
        // invocation boundaries stay unchanged and original bytes are untouched.
        syn::parse_file(&text.replace("\r\n", "\n")).map_err(|e| fail(&format!("cannot parse invocation root: {e}")))?;
    let mut occupied = BTreeSet::new();
    struct Names<'a>(&'a mut BTreeSet<String>);
    impl<'ast> Visit<'ast> for Names<'_> {
        fn visit_ident(&mut self, name: &'ast syn::Ident) {
            self.0
                .insert(name.to_string().trim_start_matches("r#").into());
        }
    }
    Names(&mut occupied).visit_file(&syntax);
    let aliases = macro_aliases(&manifest);
    let mut found = false;
    let mut root_bindings = BTreeSet::new();
    for item in &syntax.items {
        let mut bindings = BTreeSet::new();
        match item {
            syn::Item::Use(item) => use_names(&item.tree, None, &mut bindings)?,
            syn::Item::ExternCrate(_) => {
                return Err(fail(
                    "extern crate declarations in the caller require additional namespace proof",
                ));
            }
            syn::Item::Macro(item) => {
                let names: Vec<_> = item
                    .mac
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string().trim_start_matches("r#").to_owned())
                    .collect();
                if names.len() != 2 || names[1] != "extract_fn" || !aliases.contains(&names[0]) {
                    return Err(fail(
                        "automatic context requires literal fully qualified Checkmate invocations; other root macros may introduce unknown names",
                    ));
                }
                let range = crate::source::ast::SourceRange::of(&item.mac);
                if range.start <= caller.at && caller.at < range.end {
                    let input: proc_macro2::TokenStream = caller
                        .tokens
                        .parse()
                        .map_err(|_| fail("cannot tokenize compiler invocation"))?;
                    if item.mac.tokens.to_string() != input.to_string() {
                        return Err(fail(
                            "invocation tokens do not match the literal caller source; macro-generated invocations are unsupported",
                        ));
                    }
                    found = true;
                }
            }
            syn::Item::Trait(item) => {
                bindings.insert(item.ident.to_string());
            }
            syn::Item::Union(item) => {
                bindings.insert(item.ident.to_string());
            }
            _ => {
                if let Some(name) = crate::resolve::catalog::item_ident(item) {
                    bindings.insert(name.to_string());
                }
            }
        }
        root_bindings.extend(
            bindings
                .iter()
                .map(|name| name.trim_start_matches("r#").to_owned()),
        );
        if bindings.iter().any(|name| {
            matches!(name.trim_start_matches("r#"), "std" | "core")
                || name.trim_start_matches("r#") == origin.target_name
                || aliases.contains(name.trim_start_matches("r#"))
        }) {
            return Err(fail(
                "caller bindings shadow a required library/standard/Checkmate root",
            ));
        }
    }
    if !found {
        return Err(fail(
            "automatic context is limited to literal crate-root macro invocations in verified integration-test targets",
        ));
    }
    // Unknown attributes may synthesize root items, even on a caller function.
    struct Attributes(Option<Error>);
    impl<'ast> Visit<'ast> for Attributes {
        fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
            if ![
                "test", "cfg", "doc", "allow", "warn", "deny", "forbid", "expect",
            ]
            .iter()
            .any(|name| attr.path().is_ident(name))
            {
                self.0.get_or_insert_with(|| fail("caller attributes may alter declarations or the prelude; placement is not proven"));
            }
            // Do not interpret/expand attribute payloads.
        }
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            // Module attributes matter at the parent; child names cannot bind in
            // this crate root. No out-of-line module file is read here.
            for attr in &item.attrs {
                self.visit_attribute(attr);
            }
        }
    }
    let mut attrs = Attributes(None);
    attrs.visit_file(&syntax);
    if let Some(error) = attrs.0 {
        return Err(error);
    }
    // References to the requested function in user tests are expected, not
    // collisions. Keep real root bindings; retain all other identifiers to
    // prevent pre-written impls/references from capturing generated helper names.
    let function_name = function_name.trim_start_matches("r#");
    if !root_bindings.contains(function_name) {
        occupied.remove(function_name);
    }
    Ok(Placement {
        occupied,
        caller: file,
    })
}

fn enabled(package: &toml::Table, key: &str) -> Result<bool> {
    match package.get(key) {
        None => Ok(true),
        Some(toml::Value::Boolean(value)) => Ok(*value),
        _ => Err(fail("Cargo auto-target switches must be booleans")),
    }
}
fn targets(
    manifest: &toml::Value,
    root: &Path,
    kind: &str,
    directory: &str,
    auto: bool,
) -> Result<Vec<(String, PathBuf)>> {
    let mut result = Vec::new();
    let mut explicit = BTreeSet::new();
    if let Some(entries) = manifest.get(kind) {
        for entry in entries
            .as_array()
            .ok_or_else(|| fail("Cargo target entries must be tables"))?
        {
            let name = entry
                .get("name")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| fail("explicit Cargo targets require a name for placement proof"))?;
            let path = if let Some(path) = entry.get("path") {
                root.join(
                    path.as_str()
                        .ok_or_else(|| fail("Cargo target paths must be strings"))?,
                )
            } else {
                let mut candidates = vec![
                    root.join(directory).join(format!("{name}.rs")),
                    root.join(directory).join(name).join("main.rs"),
                ];
                if kind == "bin"
                    && manifest
                        .get("package")
                        .and_then(|p| p.get("name"))
                        .and_then(toml::Value::as_str)
                        == Some(name)
                {
                    candidates.push(root.join("src/main.rs"));
                }
                candidates.retain(|path| path.is_file());
                if candidates.len() != 1 {
                    return Err(fail(
                        "explicit target has ambiguous/missing inferred path; specify its Cargo path",
                    ));
                }
                candidates.remove(0)
            };
            explicit.insert(name.to_owned());
            result.push((name.to_owned(), path));
        }
    }
    let dir = root.join(directory);
    if auto && dir.is_dir() {
        for entry in std::fs::read_dir(&dir).map_err(|e| fail(&e.to_string()))? {
            let path = entry.map_err(|e| fail(&e.to_string()))?.path();
            let (name, source) = if path.is_dir() {
                (path.file_name(), path.join("main.rs"))
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                (path.file_stem(), path.clone())
            } else {
                continue;
            };
            let name = name
                .and_then(|name| name.to_str())
                .ok_or_else(|| fail("non-UTF-8 Cargo target name"))?;
            if source.is_file() && !explicit.contains(name) {
                result.push((name.to_owned(), source));
            }
        }
    }
    Ok(result)
}
fn macro_aliases(manifest: &toml::Value) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    for key in ["dependencies", "dev-dependencies"] {
        if let Some(table) = manifest.get(key).and_then(toml::Value::as_table) {
            for (alias, dependency) in table {
                if dependency.get("workspace").is_some() {
                    continue;
                }
                let package = dependency
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(alias);
                if package == "checkmate" {
                    aliases.insert(normalized(alias));
                }
            }
        }
    }
    aliases
}
fn use_names(
    tree: &syn::UseTree,
    parent: Option<&syn::Ident>,
    names: &mut BTreeSet<String>,
) -> Result<()> {
    match tree {
        syn::UseTree::Path(path) => use_names(&path.tree, Some(&path.ident), names)?,
        syn::UseTree::Name(name) => {
            names.insert(if name.ident == "self" {
                parent
                    .ok_or_else(|| fail("unproven self import"))?
                    .to_string()
            } else {
                name.ident.to_string()
            });
        }
        syn::UseTree::Rename(name) => {
            names.insert(name.rename.to_string());
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_names(tree, parent, names)?;
            }
        }
        syn::UseTree::Glob(_) => {
            return Err(fail(
                "caller glob imports prevent complete generated-name collision checks",
            ));
        }
    }
    Ok(())
}
