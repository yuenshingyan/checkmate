//! Executable documentation for the private analysis APIs. These tests read the
//! example source/config used by public or internal-only walkthroughs; they do not
//! add a public API or pretend that `context = "auto"` is available.

use crate::diagnostics::Result;
use std::{fs, path::PathBuf};

use proc_macro2::Span;
use quote::quote;
use syn::{Expr, LitStr, Signature};

use crate::arguments::{Args, Invocation};
use crate::resolve::{ReferenceOrigin, ReferenceTarget, References, collect_references};
use crate::scope::ScopeIndex;
use crate::scope::imports::{ImportId, PathOrigin};
use crate::source::ast::ParsedSource;
use crate::source::origin::SourceOrigin;

struct Example {
    root: PathBuf,
    source: ParsedSource,
    index: ScopeIndex,
    args: Args,
    snippet: String,
}

impl Example {
    fn load(snippet: &str) -> Self {
        Self::load_from("struct-return", snippet)
    }

    fn load_from(example: &str, snippet: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(example);
        let snippet = LitStr::new(snippet, Span::call_site());
        let invocation: Invocation = syn::parse2(quote! {
            config = "checkmate.toml", snippet = #snippet,
        })
        .unwrap();
        let resolved = invocation.resolve(&root).unwrap();
        let text = fs::read_to_string(resolved.root.join(resolved.args.module.value())).unwrap();
        let source = ParsedSource::parse(resolved.args.module.value(), text).unwrap();
        let index = ScopeIndex::build(&source).unwrap();
        Self {
            root,
            source,
            index,
            args: resolved.args,
            snippet: snippet.value(),
        }
    }

    fn analyze(&self, signature: &Signature, output: Option<&Expr>) -> Result<References> {
        let selection = self.source.select(
            self.args.start.base10_parse()?,
            self.args.end.base10_parse()?,
        )?;
        collect_references(&self.source, &self.index, &selection, signature, output)
    }

    fn configured_analysis(&self) -> References {
        self.analyze(
            &self.args.fn_sig.parse().unwrap(),
            self.args.output.as_ref(),
        )
        .unwrap()
    }
}

fn promotion_walkthrough(package: &str, snippet: &str) -> crate::resolve::promote::PromotionPlan {
    use crate::resolve::declarations::{Resolver, ReuseSite};
    use crate::resolve::promote::PromotionPlan;
    use std::collections::BTreeSet;
    let example = Example::load_from(package, snippet);
    let origin = SourceOrigin::discover(&example.root, &example.source.path).unwrap();
    let resolver = Resolver::new(
        &example.source,
        &example.index,
        &origin,
        ReuseSite::PackageIntegrationTest,
    )
    .unwrap();
    let selection = example
        .source
        .select(
            example.args.start.base10_parse().unwrap(),
            example.args.end.base10_parse().unwrap(),
        )
        .unwrap();
    let selected = example
        .source
        .text
        .lines()
        .skip(selection.range.start.line - 1)
        .take(selection.range.end.line - selection.range.start.line)
        .collect::<Vec<_>>();
    let snapshot = fs::read_to_string(
        example
            .root
            .join(example.args.snapshot.as_ref().unwrap().value()),
    )
    .unwrap();
    assert_eq!(
        crate::snapshot::snapshot_difference(&snapshot, &selected, selection.range.start.line),
        None
    );
    let plan = PromotionPlan::build(
        &resolver,
        &example.configured_analysis(),
        &selection,
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(plan.copied_nominal_types.len(), 1);
    assert_eq!(
        fs::read_to_string(&origin.source).unwrap(),
        example.source.text
    );
    println!(
        "Test-gated generated plan (not yet a public extract_fn! mode):\n{}",
        plan.tokens()
    );
    plan
}

#[test]
fn local_type_promotion_discloses_new_identity_and_preserves_source() {
    promotion_walkthrough("local-types", "build_user");
    println!("Copied local User: NEW nominal identity; aliases follow their rewritten targets.");
}

#[test]
fn inherent_implementations_keep_checked_behavior_on_the_new_type() {
    let plan = promotion_walkthrough("inherent-methods", "finish_counter");
    assert_eq!(
        plan.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Impl(_)))
            .count(),
        1
    );
    println!(
        "Copied Counter has NEW nominal identity. Its constructor, mutable borrowed UFCS helper, and owned finish method are checked and retained."
    );
}

#[test]
fn dependency_graph_deduplicates_reused_originals() {
    use crate::resolve::declarations::{Resolver, ReuseSite};
    use crate::resolve::graph::{Graph, NodeKind};
    let example = Example::load("build_user");
    let origin = SourceOrigin::discover(&example.root, &example.source.path).unwrap();
    let resolver = Resolver::new(
        &example.source,
        &example.index,
        &origin,
        ReuseSite::PackageIntegrationTest,
    )
    .unwrap();
    let selection = example
        .source
        .select(
            example.args.start.base10_parse().unwrap(),
            example.args.end.base10_parse().unwrap(),
        )
        .unwrap();
    let graph = Graph::build(&resolver, &example.configured_analysis(), &selection).unwrap();
    assert_eq!(graph.nodes.len(), 3); // User, String, bool; body inputs/locals are not declaration nodes.
    assert_eq!(
        graph
            .nodes
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Original(_)))
            .count(),
        1
    );
    assert!(graph.nodes.values().all(|node| node.edges.is_empty()));
    println!("Graph: 3 unique declaration paths (User, String, bool).");
    println!(
        "User is reused once; its private implementation dependencies are not inspected or copied."
    );
}

#[test]
fn original_declarations_generate_identity_preserving_imports() {
    use crate::resolve::declarations::{Resolved, Resolver, ReuseSite};
    let example = Example::load("build_user");
    let origin = SourceOrigin::discover(&example.root, &example.source.path).unwrap();
    let resolver = Resolver::new(
        &example.source,
        &example.index,
        &origin,
        ReuseSite::PackageIntegrationTest,
    )
    .unwrap();
    let selection = example
        .source
        .select(
            example.args.start.base10_parse().unwrap(),
            example.args.end.base10_parse().unwrap(),
        )
        .unwrap();
    let references = example.configured_analysis();
    let resolved = resolver.references(&references, &selection).unwrap();
    let originals: Vec<_> = resolved
        .iter()
        .filter(|item| matches!(item, Resolved::Original { .. }))
        .collect();
    assert_eq!(originals.len(), 2); // Signature and construction share an identity.
    assert_eq!(originals[0], originals[1]);
    let generated = originals[0]
        .import_as(&syn::parse_str("Account").unwrap())
        .unwrap()
        .to_string();
    assert_eq!(
        generated,
        "use :: checkmate_struct_example :: models :: User as Account ;"
    );
    println!("Reusable original: {generated}");
    println!("No struct copy; the invocation target must be verified before public integration.");
}

#[test]
fn imports_preserve_alias_origins() {
    let example = Example::load("build_user");
    let origin = SourceOrigin::discover(&example.root, &example.source.path).unwrap();
    for (alias, expected) in [
        ("Account", vec!["models", "User"]),
        ("Member", vec!["models", "User"]),
        ("schema", vec!["models"]),
    ] {
        let id = ImportId(
            example
                .index
                .imports
                .iter()
                .position(|import| import.alias == alias)
                .unwrap(),
        );
        let path = example.index.import_origin(id, &origin).unwrap();
        let expected: Vec<String> = expected.into_iter().map(str::to_owned).collect();
        assert_eq!(path, PathOrigin::Crate(expected.clone()));
        println!(
            "{alias} -> crate::{} (source origin; access not inferred)",
            expected.join("::")
        );
    }
}

#[test]
fn references_classify_inputs_and_locals() {
    let example = Example::load("build_user");
    let references = example.configured_analysis();
    assert_eq!(
        references
            .required_inputs
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["active", "name"]
    );
    println!("Required inputs: active, name");
    for reference in &references.occurrences {
        if !["Account", "name", "active", "user"].contains(&reference.root.as_str()) {
            continue;
        }
        let classification = match reference.target {
            ReferenceTarget::Import(_) => "import candidate",
            ReferenceTarget::Input(_) => "explicit input",
            ReferenceTarget::SnippetLocal(_) => "snippet local",
            _ => panic!("unexpected classification for {}", reference.root),
        };
        println!(
            "{:?}: {} -> {classification}",
            reference.site.origin, reference.root
        );
    }
    assert!(
        references
            .occurrences
            .iter()
            .any(|reference| reference.root == "user"
                && reference.site.origin == ReferenceOrigin::Output
                && matches!(reference.target, ReferenceTarget::SnippetLocal(_)))
    );
    assert_eq!(
        fs::read_to_string(example.root.join(&example.source.path)).unwrap(),
        example.source.text
    );
}

#[test]
fn missing_input_has_an_actionable_error() {
    let example = Example::load("build_user");
    let signature = syn::parse_str("fn build_user(active: bool) -> Account").unwrap();
    let error = example
        .analyze(&signature, example.args.output.as_ref())
        .err()
        .expect("name was not supplied");
    assert_eq!(error.kind, crate::diagnostics::Kind::MissingInput);
    assert!(error.to_string().contains("required input `name`"));
    let range = format!(
        "{}:{}-{}",
        example.args.module.value(),
        example.args.start,
        example.args.end
    );
    assert!(error.to_string().contains(&range));
    let report = error.render(
        &crate::diagnostics::Request {
            snippet: &example.snippet,
            path: &example.source.path,
            start: example.args.start.base10_parse().unwrap(),
            end: example.args.end.base10_parse().unwrap(),
        },
        &example.source.text,
        None,
    );
    assert!(report.contains("CM004") && report.contains("snippet: build_user"));
    assert_eq!(
        fs::read_to_string(example.root.join(&example.source.path)).unwrap(),
        example.source.text
    );
    println!("{report}");
}

#[test]
fn output_shadowing_has_a_separate_scope() {
    let example = Example::load("user_fields");
    let output = syn::parse_str("{ let active = false; (user.name, active) }").unwrap();
    let references = example
        .analyze(&example.args.fn_sig.parse().unwrap(), Some(&output))
        .unwrap();
    assert!(
        references
            .occurrences
            .iter()
            .any(|reference| reference.root == "active"
                && reference.site.origin == ReferenceOrigin::Source
                && matches!(reference.target, ReferenceTarget::Input(_)))
    );
    assert!(
        references
            .occurrences
            .iter()
            .any(|reference| reference.root == "active"
                && reference.site.origin == ReferenceOrigin::Output
                && matches!(reference.target, ReferenceTarget::GeneratedLocal(_)))
    );
    println!("active in the snippet -> explicit input");
    println!("active in the output block -> output-local binding, not another input");
}

#[test]
fn methods_stay_deferred_instead_of_becoming_inputs() {
    let example = Example::load("count_values");
    let references = example.configured_analysis();
    assert_eq!(
        references
            .required_inputs
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["values"]
    );
    assert!(
        !references
            .occurrences
            .iter()
            .any(|reference| reference.root == "len")
    );
    let method = references
        .deferred_operations
        .iter()
        .find(|operation| operation.description.contains("method `len`"))
        .unwrap();
    println!("Required input: values (not len)");
    println!("Deferred: {}", method.description);
}
