use std::collections::BTreeSet;
use std::path::Path;
use syn::{Expr, Fields, Item, Lit, Meta, Token, Type, Visibility, punctuated::Punctuated};

/// This parser checks the generator's hand-maintained lists independently.
/// Attribute layout and enum-field visibility do not affect the result.
pub fn omitted_fields(options: bool) -> BTreeSet<(String, String)> {
    fn walk(dir: &Path, options: bool, out: &mut BTreeSet<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("read Rust source directory") {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                walk(&path, options, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).expect("read Rust source");
                let file = syn::parse_file(&source)
                    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
                collect(&file.items, options, out);
            }
        }
    }

    let mut out = BTreeSet::new();
    let crates = wns_bindings::workspace_root().join("crates");
    for entry in std::fs::read_dir(crates).expect("read crates directory") {
        let source = entry.expect("read crate entry").path().join("src");
        if source.is_dir() {
            walk(&source, options, &mut out);
        }
    }
    out
}

fn collect(items: &[Item], options: bool, out: &mut BTreeSet<(String, String)>) {
    for item in items {
        match item {
            Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                fields(&item.ident.to_string(), &item.fields, false, options, out);
            }
            Item::Enum(item) if matches!(item.vis, Visibility::Public(_)) => {
                for variant in &item.variants {
                    fields(&item.ident.to_string(), &variant.fields, true, options, out);
                }
            }
            Item::Mod(item) => {
                if let Some((_, items)) = &item.content {
                    collect(items, options, out);
                }
            }
            _ => {}
        }
    }
}

fn fields(
    owner: &str,
    fields: &Fields,
    variant: bool,
    options: bool,
    out: &mut BTreeSet<(String, String)>,
) {
    for field in fields {
        let Some(name) = &field.ident else { continue };
        if !variant && !matches!(field.vis, Visibility::Public(_)) {
            continue;
        }
        let option = matches!(&field.ty, Type::Path(path)
            if path.path.segments.last().is_some_and(|segment| segment.ident == "Option"));
        if option != options {
            continue;
        }
        for attribute in field
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("serde"))
        {
            let metadata = attribute
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .expect("parse serde field attributes");
            for meta in metadata {
                if let Meta::NameValue(value) = meta
                    && value.path.is_ident("skip_serializing_if")
                    && let Expr::Lit(value) = value.value
                    && let Lit::Str(predicate) = value.lit
                    && (!option || predicate.value() == "Option::is_none")
                {
                    out.insert((owner.to_owned(), name.to_string()));
                }
            }
        }
    }
}
