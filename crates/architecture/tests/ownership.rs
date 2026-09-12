//! Regression fence for Workshop's conversation-query boundary.
//!
//! This recognizes table identifiers in Rust string literals, including macro
//! inputs. It is not a SQL parser or a sandbox against constructed table names.

use proc_macro2::{TokenStream, TokenTree};
use std::path::Path;
use syn::visit::Visit;
use wns_architecture::workspace_root;

fn mentions_run_table(value: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|word| word.eq_ignore_ascii_case("discussion_runs"))
}

#[derive(Default)]
struct TableReferences(Vec<String>);

impl TableReferences {
    fn check(&mut self, value: String) {
        if mentions_run_table(&value) {
            self.0.push(value);
        }
    }

    fn macro_literals(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Literal(literal) => {
                    if let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                        self.check(value.value());
                    }
                }
                TokenTree::Group(group) => self.macro_literals(group.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for TableReferences {
    // Documentation and serde/configuration attributes are not query code.
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.check(literal.value());
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        self.macro_literals(invocation.tokens.clone());
        if invocation.path.is_ident("concat") {
            // concat!("discussion_", "runs") is a literal table name too.
            let parser =
                syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated;
            if let Ok(parts) = syn::parse::Parser::parse2(parser, invocation.tokens.clone()) {
                self.check(parts.iter().map(syn::LitStr::value).collect::<String>());
            }
        }
    }
}

fn table_references(source: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut references = TableReferences::default();
    references.visit_file(&file);
    references.0.sort();
    references.0.dedup();
    Ok(references.0)
}

fn scan_sources(directory: &Path, violations: &mut Vec<String>, count: &mut usize) {
    for entry in std::fs::read_dir(directory).expect("read Workshop source directory") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            scan_sources(&path, violations, count);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            *count += 1;
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            for query in table_references(&source).expect("parse Rust source") {
                violations.push(format!("{}: {query}", path.display()));
            }
        }
    }
}

#[test]
fn workshop_uses_the_conversation_reader_for_run_tables() {
    let mut violations = Vec::new();
    let mut count = 0;
    scan_sources(
        &workspace_root().join("crates/workshop/src"),
        &mut violations,
        &mut count,
    );
    assert!(count > 0, "the ownership guard must inspect actual sources");
    assert!(
        violations.is_empty(),
        "Workshop must use its conversation-owned run reader:\n{}",
        violations.join("\n")
    );
}

#[test]
fn ordinary_raw_escaped_and_macro_literals_cannot_hide_direct_table_references() {
    for source in [
        r#"fn query() { let sql = "SELECT id FROM discussion_runs"; }"#,
        r##"fn query() { let sql = r#"SELECT id FROM "DISCUSSION_RUNS""#; }"##,
        "fn query() { let sql = \"SELECT id\nFROM [discussion_runs]\"; }",
        r#"fn query() { let sql = "SELECT id FROM discussion\x5fruns"; }"#,
        r#"fn query() { format!("SELECT id FROM {}", "discussion_runs"); }"#,
        r#"fn query() { concat!("SELECT id FROM ", "discussion_runs"); }"#,
        r#"fn query() { concat!("SELECT id FROM discussion_", "runs"); }"#,
    ] {
        assert!(
            !table_references(source).unwrap().is_empty(),
            "missed reference in {source}",
        );
    }
}

#[test]
fn comments_documentation_and_other_identifiers_are_not_table_references() {
    let source = r#"
        //! Conversation owns `discussion_runs`.
        // connection.prepare("SELECT id FROM discussion_runs")
        /* SELECT id FROM discussion_runs */
        /// Read through the discussion_runs owner.
        fn query() {
            let discussion_runs = "SELECT id FROM workshop_state";
            let other = "old_discussion_runs and discussion_runs_archive";
        }
    "#;
    assert!(table_references(source).unwrap().is_empty());
}

#[test]
fn invalid_rust_fails_instead_of_silently_skipping_a_file() {
    assert!(table_references("fn broken( {").is_err());
}
