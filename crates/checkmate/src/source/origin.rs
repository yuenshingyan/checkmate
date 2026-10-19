//! Verify source-to-crate identity from a package manifest, never from a filename
//! guess alone. Initial support is deliberately limited to library/binary roots.
//! No out-of-line module files or dependency source trees are loaded.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use proc_macro2::Span;

use super::read_text;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TargetKind {
    Library,
    Binary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceOrigin {
    pub package_root: PathBuf,
    pub manifest: PathBuf,
    pub source: PathBuf,
    pub target_kind: TargetKind,
    pub target_name: String,
    // Candidates, not assertions that an optional/target-specific crate is active.
    pub extern_candidates: BTreeSet<String>,
    // Keep manifest aliases separate so std/core cannot masquerade as a proven
    // standard-library mapping in the later declaration resolver.
    pub dependency_candidates: BTreeSet<String>,
}

impl SourceOrigin {
    pub fn discover(package_root: &Path, source: &Path) -> syn::Result<Self> {
        let fail = |message: String| syn::Error::new(Span::call_site(), message);
        let package_root = package_root.canonicalize().map_err(|e| {
            fail(format!(
                "cannot locate package {}: {e}",
                package_root.display()
            ))
        })?;
        let manifest = package_root.join("Cargo.toml");
        let text = read_text(&manifest, Span::call_site())?;
        let document: toml::Value = toml::from_str(&text).map_err(|e| {
            fail(format!(
                "{}: invalid Cargo manifest: {e}",
                manifest.display()
            ))
        })?;
        let package = document
            .get("package")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| {
                fail(format!(
                    "{}: expected a package manifest, not a virtual workspace",
                    manifest.display()
                ))
            })?;
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fail(format!("{}: missing package.name", manifest.display())))?;
        match package.get("edition").and_then(toml::Value::as_str) {
            Some("2018" | "2021" | "2024") => {}
            _ => {
                return Err(fail(format!(
                    "{}: source-path analysis requires an explicit supported edition (2018, 2021, or 2024); inherited/default editions need further manifest resolution",
                    manifest.display()
                )));
            }
        }
        let source = package_root
            .join(source)
            .canonicalize()
            .map_err(|e| fail(format!("cannot locate source: {e}")))?;
        let mut targets: Vec<(TargetKind, String, PathBuf)> = Vec::new();
        let enabled = |key: &str| -> syn::Result<bool> {
            match package.get(key) {
                None => Ok(true),
                Some(toml::Value::Boolean(enabled)) => Ok(*enabled),
                Some(_) => Err(fail(format!(
                    "{}: package.{key} must be a boolean",
                    manifest.display()
                ))),
            }
        };
        if let Some(lib) = document.get("lib") {
            let lib = lib
                .as_table()
                .ok_or_else(|| fail("[lib] must be a table".into()))?;
            let lib_name = string(lib, "name")?.unwrap_or_else(|| name.replace('-', "_"));
            let path = string(lib, "path")?.unwrap_or_else(|| "src/lib.rs".into());
            targets.push((TargetKind::Library, lib_name, package_root.join(path)));
        } else if enabled("autolib")? && package_root.join("src/lib.rs").exists() {
            targets.push((
                TargetKind::Library,
                name.replace('-', "_"),
                package_root.join("src/lib.rs"),
            ));
        }

        let mut bins = BTreeMap::new();
        if let Some(entries) = document.get("bin") {
            for entry in entries
                .as_array()
                .ok_or_else(|| fail("[[bin]] must be an array of tables".into()))?
            {
                let entry = entry
                    .as_table()
                    .ok_or_else(|| fail("invalid [[bin]] entry".into()))?;
                let bin_name =
                    string(entry, "name")?.ok_or_else(|| fail("[[bin]] requires a name".into()))?;
                let path = if let Some(path) = string(entry, "path")? {
                    package_root.join(path)
                } else {
                    let mut candidates = vec![
                        package_root.join(format!("src/bin/{bin_name}.rs")),
                        package_root.join(format!("src/bin/{bin_name}/main.rs")),
                    ];
                    if bin_name == name {
                        candidates.push(package_root.join("src/main.rs"));
                    }
                    let candidates: Vec<_> = candidates
                        .into_iter()
                        .filter(|path| path.is_file())
                        .collect();
                    if candidates.len() != 1 {
                        return Err(fail(format!(
                            "binary {bin_name}: ambiguous or missing inferred root; specify its Cargo target path"
                        )));
                    }
                    candidates[0].clone()
                };
                if bins.insert(bin_name.clone(), path).is_some() {
                    return Err(fail(format!("duplicate binary target {bin_name}")));
                }
            }
        }
        let explicit_bins: BTreeSet<_> = bins.keys().cloned().collect();
        if enabled("autobins")? {
            if package_root.join("src/main.rs").is_file() {
                bins.entry(name.to_owned())
                    .or_insert_with(|| package_root.join("src/main.rs"));
            }
            let directory = package_root.join("src/bin");
            if directory.is_dir() {
                let mut inferred = BTreeMap::new();
                for entry in std::fs::read_dir(&directory)
                    .map_err(|e| fail(format!("{}: {e}", directory.display())))?
                {
                    let path = entry.map_err(|e| fail(e.to_string()))?.path();
                    let root = if path.is_dir() {
                        path.join("main.rs")
                    } else {
                        path.clone()
                    };
                    if !root.is_file() || root.extension().is_none_or(|ext| ext != "rs") {
                        continue;
                    }
                    let target = if path.is_dir() {
                        path.file_name()
                    } else {
                        path.file_stem()
                    }
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| fail("non-UTF-8 target name".into()))?
                    .to_owned();
                    if explicit_bins.contains(&target) {
                        continue;
                    } // Explicit targets take precedence.
                    if bins.contains_key(&target) {
                        return Err(fail(format!("ambiguous inferred binary root for {target}")));
                    }
                    if inferred.insert(target.clone(), root).is_some() {
                        return Err(fail(format!("ambiguous inferred binary root for {target}")));
                    }
                }
                bins.extend(inferred);
            }
        }
        targets.extend(
            bins.into_iter()
                .map(|(name, path)| (TargetKind::Binary, name, path)),
        );
        let mut matches = Vec::new();
        for (kind, name, path) in targets {
            let path = path
                .canonicalize()
                .map_err(|e| fail(format!("cannot locate target {}: {e}", path.display())))?;
            if path == source {
                matches.push((kind, name));
            }
        }
        if matches.len() != 1 {
            return Err(fail(format!(
                "{}: ambiguous or unsupported source origin; expected exactly one library/binary root, not an arbitrary module, test, or external file",
                source.display()
            )));
        }
        let (target_kind, target_name) = matches.remove(0);
        let mut dependency_candidates = BTreeSet::new();
        dependencies(&document, &mut dependency_candidates)?;
        if let Some(targets) = document.get("target").and_then(toml::Value::as_table) {
            for target in targets.values() {
                dependencies(target, &mut dependency_candidates)?;
            }
        }
        let mut extern_candidates = dependency_candidates.clone();
        extern_candidates.extend(["std".into(), "core".into()]);
        Ok(Self {
            package_root,
            manifest,
            source,
            target_kind,
            target_name,
            extern_candidates,
            dependency_candidates,
        })
    }
}

fn string(table: &toml::Table, key: &str) -> syn::Result<Option<String>> {
    table
        .get(key)
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                syn::Error::new(Span::call_site(), format!("{key} must be a string"))
            })
        })
        .transpose()
}

fn dependencies(document: &toml::Value, names: &mut BTreeSet<String>) -> syn::Result<()> {
    for key in ["dependencies", "dev-dependencies"] {
        if let Some(table) = document.get(key) {
            let table = table.as_table().ok_or_else(|| {
                syn::Error::new(Span::call_site(), format!("{key} must be a table"))
            })?;
            // Dependency keys are the local aliases (including workspace-inherited
            // and renamed dependencies). Availability is checked later, not here.
            names.extend(table.keys().map(|name| name.replace('-', "_")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
