//! Derive the renderer's Tauri command contract from the application source.
//!
//! This deliberately follows the small, pinned subset of Tauri's command
//! macro used by the desktop crate.  It resolves the command paths in the
//! actual `generate_handler!` invocation before looking at signatures, so a
//! command with a valid attribute but no registration cannot accidentally be
//! considered callable.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use heck::{ToLowerCamelCase, ToSnakeCase};
use serde::{Deserialize, Serialize};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ExprLit, FnArg, Item, Lit, Meta, Pat, PathArguments, Token, Type};

const DESKTOP_SOURCE: &str = "apps/desktop/src-tauri/src";
const DESKTOP_CARGO: &str = "apps/desktop/src-tauri/Cargo.toml";
const LOCKFILE: &str = "Cargo.lock";
const DESKTOP_MAIN: &str = "apps/desktop/src-tauri/src/main.rs";
const TAURI_VERSION: &str = "2.11.5";
const TAURI_MACROS_VERSION: &str = "2.6.3";

/// Version of the on-disk command manifest format.
pub const MANIFEST_VERSION: u8 = 1;

/// One command's externally visible name and top-level wire arguments.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContract {
    pub name: String,
    pub rust_path: String,
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

/// A normalized source input used to derive the command contract.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceHash {
    pub path: String,
    pub sha256: String,
}

/// The generated source-hashed command contract consumed by frontend checks.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandManifest {
    pub version: u8,
    pub sources: Vec<SourceHash>,
    pub commands: Vec<CommandContract>,
}

/// Derive the command list from the desktop `main.rs` and its registered
/// module paths.  This is useful for fixtures; production callers should use
/// [`manifest_from_workspace`] so the version guard and source hashes run too.
pub fn derive_commands_from_main(
    main_path: impl AsRef<Path>,
) -> Result<Vec<CommandContract>, String> {
    let main_path = main_path.as_ref();
    let source_root = main_path
        .parent()
        .ok_or_else(|| format!("main source has no parent: {}", main_path.display()))?;
    let mut parser = ParserState::new(source_root);
    parser.derive(main_path)
}

/// Build the complete manifest for a workspace checkout.
pub fn manifest_from_workspace(root: &Path) -> Result<CommandManifest, String> {
    verify_pinned_tauri(root)?;
    let main = root.join(DESKTOP_MAIN);
    let source_root = root.join(DESKTOP_SOURCE);
    let mut parser = ParserState::new(&source_root);
    let commands = parser.derive(&main)?;

    // This manifest records the input version declaration as well as the Rust
    // files traversed for registered handlers.  A dependency version change
    // therefore requires regeneration and review even when Rust signatures
    // happen to remain unchanged.
    parser.include_source(&root.join(DESKTOP_CARGO))?;
    parser.include_source(&root.join(LOCKFILE))?;
    let sources = parser.source_hashes(root)?;
    let manifest = CommandManifest {
        version: MANIFEST_VERSION,
        sources,
        commands,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Build the production manifest using the current workspace root.
pub fn manifest() -> Result<CommandManifest, String> {
    manifest_from_workspace(&workspace_root())
}

/// Write a validated manifest as deterministic pretty JSON.
pub fn write_manifest(path: &Path, manifest: &CommandManifest) -> Result<(), String> {
    validate_manifest(manifest)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("manifest has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create manifest directory: {error}"))?;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("serialize command manifest: {error}"))?;
    fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("write command manifest {}: {error}", path.display()))
}

/// Compare a committed manifest with the independently derived manifest.
pub fn manifest_differences(
    path: &Path,
    expected: &CommandManifest,
) -> Result<Vec<String>, String> {
    validate_manifest(expected)?;
    let actual = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<CommandManifest>(&text)
            .map_err(|error| format!("parse command manifest {}: {error}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(vec![format!("missing {}", path.display())]);
        }
        Err(error) => return Err(format!("read command manifest {}: {error}", path.display())),
    };
    validate_manifest(&actual)?;
    if actual == *expected {
        Ok(Vec::new())
    } else {
        Ok(vec![format!("changed {}", path.display())])
    }
}

/// Validate the schema and deterministic ordering independently of generation.
pub fn validate_manifest(manifest: &CommandManifest) -> Result<(), String> {
    if manifest.version != MANIFEST_VERSION {
        return Err(format!(
            "unsupported command manifest version {}; expected {}",
            manifest.version, MANIFEST_VERSION
        ));
    }
    let source_paths: Vec<&str> = manifest
        .sources
        .iter()
        .map(|source| source.path.as_str())
        .collect();
    if source_paths.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("command manifest sources must be sorted and unique".to_owned());
    }
    for source in &manifest.sources {
        if source.path.is_empty()
            || source.path.contains('\\')
            || Path::new(&source.path).is_absolute()
            || source.sha256.len() != 64
            || !source
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(format!("invalid command manifest source {:?}", source.path));
        }
    }
    let command_names: Vec<&str> = manifest
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    if command_names.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("command manifest commands must be sorted by unique external name".to_owned());
    }
    for command in &manifest.commands {
        if command.name.is_empty() || command.rust_path.is_empty() {
            return Err("command manifest command names and paths must not be empty".to_owned());
        }
        validate_names("required", &command.required)?;
        validate_names("optional", &command.optional)?;
        if command
            .required
            .iter()
            .any(|name| command.optional.contains(name))
        {
            return Err(format!(
                "command {} has required and optional key overlap",
                command.name
            ));
        }
    }
    Ok(())
}

fn validate_names(kind: &str, names: &[String]) -> Result<(), String> {
    if names.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(format!(
            "command manifest {kind} arguments must be sorted and unique"
        ));
    }
    if names.iter().any(String::is_empty) {
        return Err(format!(
            "command manifest {kind} arguments must not be empty"
        ));
    }
    Ok(())
}

/// Hash source bytes after normalizing CRLF to LF.
pub fn normalized_source_sha256(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    let bytes = normalized_source_bytes(path)?;
    Ok(wns_kernel::sha256_hex(&bytes))
}

/// Resolve the workspace root from this crate's manifest directory.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

struct ParserState {
    source_root: PathBuf,
    sources: BTreeSet<PathBuf>,
}

struct ModuleLocation {
    source_path: PathBuf,
    child_dir: PathBuf,
    path_attribute_dir: PathBuf,
    allow_path_attribute: bool,
}

impl ModuleLocation {
    fn root(path: &Path) -> Self {
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        Self {
            source_path: path.to_owned(),
            child_dir: parent.to_owned(),
            path_attribute_dir: parent.to_owned(),
            allow_path_attribute: true,
        }
    }

    fn external(path: &Path, allow_path_attribute: bool) -> Self {
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let child_dir = if path.file_name().is_some_and(|name| name == "mod.rs") {
            parent.to_owned()
        } else {
            parent.join(path.file_stem().unwrap_or_default())
        };
        Self {
            source_path: path.to_owned(),
            child_dir,
            path_attribute_dir: parent.to_owned(),
            allow_path_attribute,
        }
    }

    fn inline(&self, ident: &syn::Ident) -> Self {
        Self {
            source_path: self.source_path.clone(),
            child_dir: self.child_dir.join(ident.unraw().to_string()),
            path_attribute_dir: self.path_attribute_dir.clone(),
            allow_path_attribute: false,
        }
    }
}

impl ParserState {
    fn new(source_root: &Path) -> Self {
        Self {
            source_root: source_root.to_owned(),
            sources: BTreeSet::new(),
        }
    }

    fn derive(&mut self, main_path: &Path) -> Result<Vec<CommandContract>, String> {
        let main_path = main_path
            .canonicalize()
            .map_err(|error| format!("resolve main source {}: {error}", main_path.display()))?;
        let root = self.load(&main_path)?;
        let registrations = registrations(&root)?;
        if registrations.is_empty() {
            return Err(format!(
                "{} has no tauri::generate_handler! registration",
                main_path.display()
            ));
        }

        let mut commands = Vec::with_capacity(registrations.len());
        let mut seen_paths = BTreeSet::new();
        for registration in registrations {
            let path_text = path_text(&registration);
            if !seen_paths.insert(path_text.clone()) {
                return Err(format!(
                    "duplicate generate_handler registration: {path_text}"
                ));
            }
            let command = self.resolve_registration(&main_path, &root.items, &registration)?;
            commands.push(command);
        }
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        for pair in commands.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(format!("duplicate external command name: {}", pair[0].name));
            }
        }
        Ok(commands)
    }

    fn resolve_registration(
        &mut self,
        main_path: &Path,
        main_items: &[Item],
        registration: &syn::Path,
    ) -> Result<CommandContract, String> {
        let mut segments: Vec<String> = registration
            .segments
            .iter()
            .map(|segment| segment.ident.unraw().to_string())
            .collect();
        if segments.first().is_some_and(|segment| segment == "crate") {
            segments.remove(0);
        }
        if segments
            .iter()
            .any(|segment| segment == "self" || segment == "super")
        {
            return Err(format!(
                "unsupported relative command registration: {}",
                path_text(registration)
            ));
        }
        let function = segments
            .pop()
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| format!("registration has no function: {}", path_text(registration)))?;
        if segments.is_empty() {
            return self.command_from_items(
                main_path,
                main_items,
                &function,
                &path_text(registration),
            );
        }

        let location = ModuleLocation::root(main_path);
        let (module_path, module_items) = self.resolve_module(&location, main_items, &segments)?;
        self.command_from_items(
            &module_path,
            &module_items,
            &function,
            &path_text(registration),
        )
    }

    fn resolve_module(
        &mut self,
        current_location: &ModuleLocation,
        current_items: &[Item],
        segments: &[String],
    ) -> Result<(PathBuf, Vec<Item>), String> {
        let segment = segments
            .first()
            .ok_or_else(|| "module path unexpectedly empty".to_owned())?;
        let module = current_items
            .iter()
            .find_map(|item| match item {
                Item::Mod(module) if module.ident.unraw() == segment => Some(module),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "module {segment:?} is not declared in {}",
                    current_location.source_path.display()
                )
            })?;
        if has_cfg(&module.attrs) {
            return Err(format!("registered module {segment:?} has cfg attributes"));
        }
        let (module_path, module_items) = if let Some((_, items)) = &module.content {
            (current_location.inline(&module.ident), items.clone())
        } else {
            let module_path = resolve_module_file(current_location, module)?;
            let file = self.load(&module_path)?;
            (
                ModuleLocation::external(&module_path, current_location.allow_path_attribute),
                file.items,
            )
        };
        if segments.len() == 1 {
            Ok((module_path.source_path, module_items))
        } else {
            self.resolve_module(&module_path, &module_items, &segments[1..])
        }
    }

    fn command_from_items(
        &self,
        source_path: &Path,
        items: &[Item],
        function_name: &str,
        registration: &str,
    ) -> Result<CommandContract, String> {
        let function = items
            .iter()
            .find_map(|item| match item {
                Item::Fn(function) if function.sig.ident.unraw() == function_name => Some(function),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "registered command {registration} is not defined in {}",
                    source_path.display()
                )
            })?;
        if has_cfg(&function.attrs) {
            return Err(format!(
                "registered command {registration} has cfg attributes"
            ));
        }
        let options = command_options(&function.attrs)?.ok_or_else(|| {
            format!("registered command {registration} is missing #[tauri::command]")
        })?;
        let environment = TypeEnvironment::from_items(items);
        options.contract(function, registration, &environment)
    }

    fn load(&mut self, path: &Path) -> Result<syn::File, String> {
        let path = path
            .canonicalize()
            .map_err(|error| format!("resolve Rust source {}: {error}", path.display()))?;
        let source_root = self.source_root.canonicalize().map_err(|error| {
            format!(
                "resolve Rust source root {}: {error}",
                self.source_root.display()
            )
        })?;
        if !path.starts_with(source_root) {
            return Err(format!(
                "Rust module escapes source root: {}",
                path.display()
            ));
        }
        self.include_source(&path)?;
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("read Rust source {}: {error}", path.display()))?;
        syn::parse_file(&source)
            .map_err(|error| format!("parse Rust source {}: {error}", path.display()))
    }

    fn include_source(&mut self, path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|error| format!("resolve source {}: {error}", path.display()))?;
        self.sources.insert(path);
        Ok(())
    }

    fn source_hashes(&self, root: &Path) -> Result<Vec<SourceHash>, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("resolve workspace root {}: {error}", root.display()))?;
        let mut sources = Vec::with_capacity(self.sources.len());
        for path in &self.sources {
            let relative = path.strip_prefix(&root).map_err(|_| {
                format!(
                    "source {} is outside workspace {}",
                    path.display(),
                    root.display()
                )
            })?;
            let relative = relative
                .to_str()
                .ok_or_else(|| format!("source path is not UTF-8: {}", path.display()))?
                .replace('\\', "/");
            sources.push(SourceHash {
                path: relative,
                sha256: normalized_source_sha256(path)?,
            });
        }
        sources.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(sources)
    }
}

fn registrations(file: &syn::File) -> Result<Vec<syn::Path>, String> {
    struct FindRegistrations {
        paths: Vec<syn::Path>,
        error: Option<String>,
        found_handler: bool,
    }
    impl<'ast> Visit<'ast> for FindRegistrations {
        fn visit_item_fn(&mut self, _function: &'ast syn::ItemFn) {
            // The caller starts at the real top-level `main` body.  Nested
            // helper functions are not application registration sites.
        }

        fn visit_expr_closure(&mut self, _closure: &'ast syn::ExprClosure) {
            // A generate_handler! inside setup or another callback is a
            // decoy; only the builder chain in main registers commands.
        }

        fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
            if expression.method == "invoke_handler" {
                if self.error.is_some() {
                    return;
                }
                if self.found_handler {
                    self.error = Some("multiple invoke_handler registrations".to_owned());
                    return;
                }
                if !tauri_builder_receiver(&expression.receiver) {
                    self.error =
                        Some("invoke_handler must be called on a tauri::Builder chain".to_owned());
                    return;
                }
                self.found_handler = true;
                if expression.args.len() != 1 {
                    self.error =
                        Some("invoke_handler must have exactly one handler argument".to_owned());
                    return;
                }
                let Some(argument) = expression.args.first() else {
                    self.error = Some("invoke_handler has no handler argument".to_owned());
                    return;
                };
                let syn::Expr::Macro(argument) = argument else {
                    self.error = Some(
                        "invoke_handler must use tauri::generate_handler! directly".to_owned(),
                    );
                    return;
                };
                let path = &argument.mac.path;
                if path.segments.len() != 2
                    || path.segments[0].ident != "tauri"
                    || path.segments[1].ident != "generate_handler"
                {
                    self.error =
                        Some("invoke_handler must use tauri::generate_handler!".to_owned());
                    return;
                }
                let parser = Punctuated::<syn::Path, Token![,]>::parse_terminated;
                match parser.parse2(argument.mac.tokens.clone()) {
                    Ok(paths) => self.paths.extend(paths),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            visit::visit_expr_method_call(self, expression);
        }
    }
    let main_functions: Vec<&syn::ItemFn> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(function) if function.sig.ident == "main" => Some(function),
            _ => None,
        })
        .collect();
    let Some(main) = (match main_functions.as_slice() {
        [main] => Some(*main),
        [] => None,
        _ => return Err("multiple top-level main functions".to_owned()),
    }) else {
        return Err("no top-level main function".to_owned());
    };
    if has_cfg(&main.attrs) {
        return Err("top-level main has cfg attributes".to_owned());
    }
    let mut visitor = FindRegistrations {
        paths: Vec::new(),
        error: None,
        found_handler: false,
    };
    visitor.visit_block(&main.block);
    if let Some(error) = visitor.error {
        return Err(format!(
            "malformed tauri::generate_handler! registration: {error}"
        ));
    }
    Ok(visitor.paths)
}

fn tauri_builder_receiver(expression: &Expr) -> bool {
    match expression {
        Expr::MethodCall(call) => tauri_builder_receiver(&call.receiver),
        Expr::Call(call) => match call.func.as_ref() {
            Expr::Path(path) => {
                let segments = &path.path.segments;
                path.qself.is_none()
                    && segments.len() == 3
                    && segments[0].ident == "tauri"
                    && segments[1].ident == "Builder"
                    && (segments[2].ident == "default" || segments[2].ident == "new")
            }
            _ => false,
        },
        Expr::Paren(paren) => tauri_builder_receiver(&paren.expr),
        Expr::Group(group) => tauri_builder_receiver(&group.expr),
        _ => false,
    }
}

fn resolve_module_file(
    location: &ModuleLocation,
    module: &syn::ItemMod,
) -> Result<PathBuf, String> {
    if !location.allow_path_attribute && module_path_attribute(&module.attrs)?.is_some() {
        return Err("#[path] modules under inline modules are unsupported".to_owned());
    }
    if let Some(path) = module_path_attribute(&module.attrs)? {
        let candidate = location.path_attribute_dir.join(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "module {} path does not exist: {}",
            module.ident,
            candidate.display()
        ));
    }
    let flat = location
        .child_dir
        .join(format!("{}.rs", module.ident.unraw()));
    let nested = location
        .child_dir
        .join(module.ident.unraw().to_string())
        .join("mod.rs");
    match (flat.is_file(), nested.is_file()) {
        (true, false) => Ok(flat),
        (false, true) => Ok(nested),
        (false, false) => Err(format!("module {} has no source file", module.ident)),
        (true, true) => Err(format!(
            "module {} has ambiguous source files",
            module.ident
        )),
    }
}

fn module_path_attribute(attributes: &[Attribute]) -> Result<Option<PathBuf>, String> {
    let paths: Vec<&Attribute> = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("path"))
        .collect();
    let Some(attribute) = paths.first().copied() else {
        return Ok(None);
    };
    if paths.len() > 1 {
        return Err("duplicate module path attributes".to_owned());
    }
    let Meta::NameValue(value) = &attribute.meta else {
        return Err("module path attribute must be a string literal".to_owned());
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &value.value
    else {
        return Err("module path attribute must be a string literal".to_owned());
    };
    Ok(Some(PathBuf::from(value.value())))
}

fn has_cfg(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr"))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ArgumentCase {
    Camel,
    Snake,
}

struct CommandOptions {
    argument_case: ArgumentCase,
    rename: Option<String>,
}

enum CommandAttribute {
    Meta(Box<Meta>),
    Async,
}

impl Parse for CommandAttribute {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Token![async]) {
            input.parse::<Token![async]>()?;
            Ok(Self::Async)
        } else {
            input.parse().map(|meta| Self::Meta(Box::new(meta)))
        }
    }
}

fn command_options(attributes: &[Attribute]) -> Result<Option<CommandOptions>, String> {
    let command_attributes: Vec<&Attribute> = attributes
        .iter()
        .filter(|attribute| {
            let segments = &attribute.path().segments;
            segments.len() == 2 && segments[0].ident == "tauri" && segments[1].ident == "command"
        })
        .collect();
    let Some(attribute) = command_attributes.first().copied() else {
        return Ok(None);
    };
    if command_attributes.len() > 1 {
        return Err("duplicate #[tauri::command] attributes".to_owned());
    }
    let parser = Punctuated::<CommandAttribute, Token![,]>::parse_terminated;
    let attributes = match &attribute.meta {
        Meta::Path(_) => Punctuated::new(),
        Meta::List(list) => parser
            .parse2(list.tokens.clone())
            .map_err(|error| format!("malformed #[tauri::command] attributes: {error}"))?,
        Meta::NameValue(_) => return Err("malformed #[tauri::command] attribute".to_owned()),
    };
    let mut options = CommandOptions {
        argument_case: ArgumentCase::Camel,
        rename: None,
    };
    let mut seen_case = false;
    let mut seen_rename = false;
    for attribute in attributes {
        match attribute {
            CommandAttribute::Async => {}
            CommandAttribute::Meta(meta) if matches!(*meta, Meta::NameValue(ref value) if value.path.is_ident("rename_all")) =>
            {
                let Meta::NameValue(value) = *meta else {
                    unreachable!()
                };
                if seen_case {
                    return Err("duplicate tauri command rename_all attribute".to_owned());
                }
                seen_case = true;
                let value = string_literal(&value.value, "rename_all")?;
                options.argument_case = match value.as_str() {
                    "camelCase" => ArgumentCase::Camel,
                    "snake_case" => ArgumentCase::Snake,
                    other => return Err(format!("unsupported tauri rename_all value {other:?}")),
                };
            }
            CommandAttribute::Meta(meta) if matches!(*meta, Meta::NameValue(ref value) if value.path.is_ident("rename")) =>
            {
                let Meta::NameValue(value) = *meta else {
                    unreachable!()
                };
                if seen_rename {
                    return Err("duplicate tauri command rename attribute".to_owned());
                }
                seen_rename = true;
                options.rename = Some(string_literal(&value.value, "rename")?);
            }
            CommandAttribute::Meta(meta) if matches!(*meta, Meta::NameValue(ref value) if value.path.is_ident("root")) =>
            {
                let Meta::NameValue(value) = *meta else {
                    unreachable!()
                };
                let root = string_literal(&value.value, "root")?;
                if root != "crate" && !is_ident(&root) {
                    return Err(format!("unsupported tauri root value {root:?}"));
                }
            }
            CommandAttribute::Meta(_) => {
                return Err("unsupported tauri command attribute".to_owned());
            }
        }
    }
    Ok(Some(options))
}

fn string_literal(value: &Expr, name: &str) -> Result<String, String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = value
    else {
        return Err(format!("tauri command {name} must be a string literal"));
    };
    Ok(value.value())
}

fn is_ident(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

impl CommandOptions {
    fn contract(
        &self,
        function: &syn::ItemFn,
        registration: &str,
        environment: &TypeEnvironment,
    ) -> Result<CommandContract, String> {
        let mut required = Vec::new();
        let mut optional = Vec::new();
        for argument in &function.sig.inputs {
            let FnArg::Typed(argument) = argument else {
                return Err(format!(
                    "command {registration} uses unsupported self receiver"
                ));
            };
            let Pat::Ident(pattern) = argument.pat.as_ref() else {
                return Err(format!(
                    "command {registration} arguments must use named patterns"
                ));
            };
            let rust_name = pattern.ident.unraw().to_string();
            let wire_name = match self.argument_case {
                ArgumentCase::Camel => rust_name.to_lower_camel_case(),
                ArgumentCase::Snake => rust_name.to_snake_case(),
            };
            if wire_name.is_empty() {
                return Err(format!(
                    "command {registration} has an empty wire argument name"
                ));
            }
            match environment.tauri_argument_kind(&argument.ty)? {
                Some(TauriType::Injected) => continue,
                Some(TauriType::Request) => {
                    return Err(format!(
                        "command {registration} uses tauri::Request, whose raw payload cannot be represented as named wire keys"
                    ));
                }
                Some(TauriType::Wire) | None => {}
            }
            let names = if environment
                .is_option(&argument.ty)
                .map_err(|error| format!("command {registration}: {error}"))?
            {
                &mut optional
            } else {
                &mut required
            };
            if names.iter().any(|name| name == &wire_name) {
                return Err(format!(
                    "command {registration} has duplicate wire argument {wire_name:?}"
                ));
            }
            names.push(wire_name);
        }
        required.sort();
        optional.sort();
        // Tauri uses stringify!(ident) for command names.  Unlike argument
        // keys, that retains the `r#` prefix on raw identifiers.
        let name = self
            .rename
            .clone()
            .unwrap_or_else(|| function.sig.ident.to_string());
        if name.is_empty() {
            return Err(format!("command {registration} has an empty external name"));
        }
        Ok(CommandContract {
            name,
            rust_path: registration.to_owned(),
            required,
            optional,
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TauriType {
    Injected,
    Request,
    Wire,
}

struct TypeEnvironment {
    aliases: BTreeMap<String, Vec<String>>,
    type_aliases: BTreeMap<String, Type>,
    local_types: BTreeSet<String>,
    wildcard_imports: BTreeSet<String>,
}

impl TypeEnvironment {
    fn from_items(items: &[Item]) -> Self {
        let mut environment = Self {
            aliases: BTreeMap::new(),
            type_aliases: BTreeMap::new(),
            local_types: BTreeSet::new(),
            wildcard_imports: BTreeSet::new(),
        };
        for item in items {
            match item {
                Item::Use(item) => collect_use_tree(
                    &item.tree,
                    &[],
                    &mut environment.aliases,
                    &mut environment.wildcard_imports,
                ),
                Item::Type(item) => {
                    environment
                        .local_types
                        .insert(item.ident.unraw().to_string());
                    environment
                        .type_aliases
                        .insert(item.ident.unraw().to_string(), (*item.ty).clone());
                }
                Item::Struct(item) => {
                    environment
                        .local_types
                        .insert(item.ident.unraw().to_string());
                }
                Item::Enum(item) => {
                    environment
                        .local_types
                        .insert(item.ident.unraw().to_string());
                }
                Item::Union(item) => {
                    environment
                        .local_types
                        .insert(item.ident.unraw().to_string());
                }
                _ => {}
            }
        }
        environment
    }

    fn tauri_argument_kind(&self, ty: &Type) -> Result<Option<TauriType>, String> {
        self.tauri_argument_kind_inner(ty, &mut BTreeSet::new())
    }

    fn tauri_argument_kind_inner(
        &self,
        ty: &Type,
        seen: &mut BTreeSet<String>,
    ) -> Result<Option<TauriType>, String> {
        let Type::Path(path) = ty else {
            return Ok(None);
        };
        if path.qself.is_some() {
            return Ok(None);
        }
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.unraw().to_string())
            .collect();
        if let Some(kind) = tauri_type(&segments, &self.aliases, seen)? {
            return Ok(Some(kind));
        }
        if segments.len() == 1 {
            let alias = &segments[0];
            if seen.insert(alias.clone())
                && let Some(target) = self.type_aliases.get(alias)
            {
                return self.tauri_argument_kind_inner(target, seen);
            }
        }
        Ok(None)
    }

    fn is_option(&self, ty: &Type) -> Result<bool, String> {
        self.is_option_inner(ty, &mut BTreeSet::new())
    }

    fn is_option_inner(&self, ty: &Type, seen: &mut BTreeSet<String>) -> Result<bool, String> {
        let Type::Path(path) = ty else {
            return Ok(false);
        };
        if path.qself.is_some() {
            return Ok(false);
        }
        let has_type_arguments = matches!(
            path.path.segments.last().map(|segment| &segment.arguments),
            Some(PathArguments::AngleBracketed(_))
        );
        let mut segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.unraw().to_string())
            .collect();
        if segments.len() == 1
            && let Some(alias) = self.aliases.get(&segments[0])
        {
            let alias_name = segments[0].clone();
            if !seen.insert(format!("use::{alias_name}")) {
                return Err(format!("cyclic Rust import alias involving {alias_name:?}"));
            }
            segments = alias.clone();
        }
        let is_option_name =
            segments.last().is_some_and(|name| name == "Option") && has_type_arguments;
        if !is_option_name {
            if segments.len() == 1
                && let Some(target) = self.type_aliases.get(&segments[0])
            {
                let alias_name = segments[0].clone();
                if !seen.insert(format!("type::{alias_name}")) {
                    return Err(format!("cyclic Rust type alias involving {alias_name:?}"));
                }
                return self.is_option_inner(target, seen);
            }
            return Ok(false);
        }
        if segments.len() == 1 {
            if let Some(target) = self.type_aliases.get("Option") {
                if !seen.insert("type::Option".to_owned()) {
                    return Err("cyclic Rust type alias involving \"Option\"".to_owned());
                }
                return self.is_option_inner(target, seen);
            }
            if self.local_types.contains("Option") {
                return Err("ambiguous local Option type; qualify std::option::Option".to_owned());
            }
            if self
                .wildcard_imports
                .iter()
                .any(|path| !known_wildcard_import(path))
            {
                return Err(
                    "ambiguous bare Option with wildcard imports; qualify std::option::Option"
                        .to_owned(),
                );
            }
            return Ok(true);
        }
        Ok(segments.len() == 3
            && (segments[0] == "std" || segments[0] == "core")
            && segments[1] == "option")
    }
}

fn collect_use_tree(
    tree: &syn::UseTree,
    prefix: &[String],
    aliases: &mut BTreeMap<String, Vec<String>>,
    wildcard_imports: &mut BTreeSet<String>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let mut next = prefix.to_vec();
            next.push(path.ident.unraw().to_string());
            collect_use_tree(&path.tree, &next, aliases, wildcard_imports);
        }
        syn::UseTree::Name(name) => {
            let mut full = prefix.to_vec();
            full.push(name.ident.unraw().to_string());
            aliases.insert(name.ident.unraw().to_string(), full);
        }
        syn::UseTree::Rename(rename) => {
            let mut full = prefix.to_vec();
            full.push(rename.ident.unraw().to_string());
            aliases.insert(rename.rename.unraw().to_string(), full);
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                collect_use_tree(tree, prefix, aliases, wildcard_imports);
            }
        }
        syn::UseTree::Glob(_) => {
            wildcard_imports.insert(prefix.join("::"));
        }
    }
}

fn known_wildcard_import(path: &str) -> bool {
    matches!(
        path.split("::").next(),
        Some("crate" | "self" | "super" | "serde" | "tauri" | "webnovel_core")
    )
}

fn tauri_type(
    segments: &[String],
    aliases: &BTreeMap<String, Vec<String>>,
    seen: &mut BTreeSet<String>,
) -> Result<Option<TauriType>, String> {
    let mut resolved = segments.to_vec();
    loop {
        let Some(first) = resolved.first() else {
            return Ok(None);
        };
        let Some(prefix) = aliases.get(first) else {
            break;
        };
        if !seen.insert(first.clone()) {
            return Err(format!("cyclic Rust import alias involving {first:?}"));
        }
        let mut next = prefix.clone();
        next.extend_from_slice(&resolved[1..]);
        resolved = next;
    }
    let is_tauri = resolved.first().is_some_and(|segment| segment == "tauri");
    let Some(name) = resolved.last().map(String::as_str) else {
        return Ok(None);
    };
    if !is_tauri {
        return Ok(None);
    }
    let kind = if name == "Request" {
        TauriType::Request
    } else if matches!(
        name,
        "State"
            | "AppHandle"
            | "Window"
            | "Webview"
            | "WebviewWindow"
            | "CommandScope"
            | "GlobalScope"
    ) {
        TauriType::Injected
    } else {
        // Channel<T> is intentionally wire input. It is deserialized from the
        // named invoke payload even though it has a CommandArg impl.
        TauriType::Wire
    };
    Ok(Some(kind))
}

fn path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.unraw().to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn normalized_source_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("read source {}: {error}", path.display()))?;
    Ok(source.replace("\r\n", "\n").into_bytes())
}

fn verify_pinned_tauri(root: &Path) -> Result<(), String> {
    let cargo_path = root.join(DESKTOP_CARGO);
    let cargo_source = fs::read_to_string(&cargo_path)
        .map_err(|error| format!("read desktop manifest {}: {error}", cargo_path.display()))?;
    let cargo: toml::Value = cargo_source
        .replace("\r\n", "\n")
        .parse()
        .map_err(|error| format!("parse desktop manifest {}: {error}", cargo_path.display()))?;
    let mut tauri_versions = Vec::new();
    collect_tauri_dependency_versions(&cargo, &mut tauri_versions);
    let expected_tauri_version = format!("={TAURI_VERSION}");
    if tauri_versions.len() != 1
        || tauri_versions[0].as_deref() != Some(expected_tauri_version.as_str())
    {
        return Err(format!(
            "unsupported tauri dependency; expected pinned {TAURI_VERSION} in {}",
            cargo_path.display()
        ));
    }
    let lock_path = root.join("Cargo.lock");
    let lock_source = fs::read_to_string(&lock_path)
        .map_err(|error| format!("read lockfile {}: {error}", lock_path.display()))?;
    let lock: toml::Value = lock_source
        .replace("\r\n", "\n")
        .parse()
        .map_err(|error| format!("parse lockfile {}: {error}", lock_path.display()))?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("lockfile {} has no package table", lock_path.display()))?;
    let tauri = package_versions(packages, "tauri");
    let tauri_macros = package_versions(packages, "tauri-macros");
    if tauri.len() != 1
        || tauri[0] != TAURI_VERSION
        || tauri_macros.len() != 1
        || tauri_macros[0] != TAURI_MACROS_VERSION
    {
        return Err(format!(
            "unsupported locked tauri dependency; expected tauri {TAURI_VERSION} and tauri-macros {TAURI_MACROS_VERSION} in {}",
            lock_path.display()
        ));
    }
    Ok(())
}

fn collect_tauri_dependency_versions(value: &toml::Value, versions: &mut Vec<Option<String>>) {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = value.get(section).and_then(toml::Value::as_table)
            && let Some(dependency) = dependencies.get("tauri")
        {
            versions.push(dependency_version(dependency));
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            collect_tauri_dependency_versions(target, versions);
        }
    }
}

fn dependency_version(value: &toml::Value) -> Option<String> {
    value.as_str().map(str::to_owned).or_else(|| {
        value
            .get("version")
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    })
}

fn package_versions(packages: &[toml::Value], name: &str) -> Vec<String> {
    packages
        .iter()
        .filter_map(|package| {
            let table = package.as_table()?;
            (table.get("name")?.as_str()? == name)
                .then(|| table.get("version")?.as_str().map(str::to_owned))
                .flatten()
        })
        .collect()
}
