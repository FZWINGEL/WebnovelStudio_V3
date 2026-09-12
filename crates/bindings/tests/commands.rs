use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use wns_bindings::commands::{
    derive_commands_from_main, manifest_from_workspace, normalized_source_sha256,
};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(main: &str, modules: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "wns-binding-command-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.rs"), main).unwrap();
        for (path, source) in modules {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        Self { root }
    }

    fn main(&self) -> PathBuf {
        self.root.join("main.rs")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

struct WorkspaceFixture {
    root: PathBuf,
}

impl WorkspaceFixture {
    fn new(tauri_version: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "wns-binding-workspace-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source_dir = root.join("apps/desktop/src-tauri/src");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(
            source_dir.join("main.rs"),
            "#[tauri::command] fn one(value: String) {} fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![one]); }",
        )
        .unwrap();
        fs::write(
            root.join("apps/desktop/src-tauri/Cargo.toml"),
            format!(
                "[package]\nname = \"webnovel-desktop\"\nversion = \"3.0.0\"\n\n[dependencies]\n# tauri = {{ version = \"=2.11.5\" }}\ntauri = {{ version = \"={tauri_version}\" }}\n"
            ),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "[[package]]\nname = \"tauri\"\nversion = \"2.11.5\"\n\n[[package]]\nname = \"tauri-macros\"\nversion = \"2.6.3\"\n",
        )
        .unwrap();
        Self { root }
    }
}

impl Drop for WorkspaceFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn extracts_registered_wire_names_and_pinned_injection_rules() {
    let fixture = Fixture::new(
        r#"
            mod api;
            fn main() {
                let _decoy = tauri::generate_handler![api::ignored];
                tauri::Builder::default().invoke_handler(tauri::generate_handler![
                    api::hello,
                    api::optional,
                    api::local_state,
                ]);
            }
        "#,
        &[(
            "api.rs",
            r#"
                use tauri::State as ManagedState;
                use std::option::Option as Maybe;
                use core::option::Option as CoreMaybe;
                type AppState = ManagedState<'_, String>;
                struct State<T>(T);
                #[tauri::command]
                fn hello(foo_bar: String, state: AppState, channel: tauri::ipc::Channel<String>) {}
                #[tauri::command(rename_all = "snake_case")]
                fn optional(maybe_value: std::option::Option<String>, another_value: String) {}
                #[tauri::command]
                fn local_state(
                    local_state: State<String>,
                    maybe_alias: Maybe<String>,
                    core_maybe: CoreMaybe<String>,
                ) {}
                #[tauri::command]
                fn ignored(should_not_appear: String) {}
            "#,
        )],
    );
    let commands = derive_commands_from_main(fixture.main()).unwrap();
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[0].name, "hello");
    assert_eq!(commands[0].required, ["channel", "fooBar"]);
    assert!(commands[0].optional.is_empty());
    assert_eq!(commands[1].name, "local_state");
    assert_eq!(commands[1].required, ["localState"]);
    assert_eq!(commands[1].optional, ["coreMaybe", "maybeAlias"]);
    assert_eq!(commands[2].name, "optional");
    assert_eq!(commands[2].required, ["another_value"]);
    assert_eq!(commands[2].optional, ["maybe_value"]);
}

#[test]
fn honors_command_renames_and_raw_identifier_defaults() {
    let fixture = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::r#type, api::renamed]); }",
        &[(
            "api.rs",
            r#"
                #[tauri::command]
                fn r#type(r#value_name: String) {}
                #[tauri::command(rename = "display-name")]
                fn renamed(value: String) {}
            "#,
        )],
    );
    let commands = derive_commands_from_main(fixture.main()).unwrap();
    assert_eq!(
        commands
            .iter()
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>(),
        ["display-name", "r#type"]
    );
    assert_eq!(commands[1].required, ["valueName"]);
}

#[test]
fn resolves_nested_module_files_and_rejects_unregistered_or_duplicate_commands() {
    let fixture = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::nested::one]); }",
        &[
            ("api.rs", "pub mod nested;"),
            (
                "api/nested.rs",
                "#[tauri::command] pub fn one(value: String) {}\n#[tauri::command] pub fn extra(value: String) {}",
            ),
        ],
    );
    let commands = derive_commands_from_main(fixture.main()).unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].rust_path, "api::nested::one");

    let inline = Fixture::new(
        "mod api { pub mod nested; } fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::nested::one]); }",
        &[(
            "api/nested.rs",
            "#[tauri::command] pub fn one(value: String) {}",
        )],
    );
    assert_eq!(derive_commands_from_main(inline.main()).unwrap().len(), 1);

    let inline_attributed = Fixture::new(
        "mod api { #[path = \"custom.rs\"] pub mod nested; } fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::nested::one]); }",
        &[(
            "api/custom.rs",
            "#[tauri::command] pub fn one(value: String) {}",
        )],
    );
    assert!(
        derive_commands_from_main(inline_attributed.main())
            .unwrap_err()
            .contains("under inline modules")
    );

    let attributed = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::nested::one]); }",
        &[
            ("api.rs", "#[path = \"custom.rs\"] pub mod nested;"),
            (
                "custom.rs",
                "#[tauri::command] pub fn one(value: String) {}",
            ),
        ],
    );
    assert_eq!(
        derive_commands_from_main(attributed.main()).unwrap().len(),
        1
    );

    let unknown = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::missing]); }",
        &[("api.rs", "pub fn present() {}")],
    );
    let error = derive_commands_from_main(unknown.main()).unwrap_err();
    assert!(error.contains("not defined"), "{error}");

    let duplicate = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::one, api::two]); }",
        &[(
            "api.rs",
            "#[tauri::command(rename = \"same\")] fn one(value: String) {}\n#[tauri::command(rename = \"same\")] fn two(value: String) {}",
        )],
    );
    let error = derive_commands_from_main(duplicate.main()).unwrap_err();
    assert!(error.contains("duplicate external command name"), "{error}");

    let custom_option = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::custom]); }",
        &[(
            "api.rs",
            "#[tauri::command] fn custom(value: option::Option<String>) {}",
        )],
    );
    let custom = derive_commands_from_main(custom_option.main()).unwrap();
    assert_eq!(custom[0].required, ["value"]);
    assert!(custom[0].optional.is_empty());
}

#[test]
fn rejects_ambiguous_registration_attributes_and_transport_shapes() {
    let decoy = Fixture::new("fn main() { tauri::generate_handler![ignored]; }", &[]);
    let error = derive_commands_from_main(decoy.main()).unwrap_err();
    assert!(error.contains("no tauri::generate_handler"), "{error}");

    let foreign = Fixture::new(
        "mod api; fn main() { Foreign::new().invoke_handler(tauri::generate_handler![api::bad]); }",
        &[("api.rs", "#[tauri::command] fn bad(value: String) {}")],
    );
    assert!(
        derive_commands_from_main(foreign.main())
            .unwrap_err()
            .contains("tauri::Builder")
    );

    let cfg_helper = Fixture::new(
        "mod api; #[cfg(any())] fn helper() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::bad]); } fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::good]); }",
        &[(
            "api.rs",
            "#[tauri::command] fn good(value: String) {} #[tauri::command] fn bad(value: String) {}",
        )],
    );
    let commands = derive_commands_from_main(cfg_helper.main()).unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "good");

    let closure_decoy = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().setup(|_| { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::bad]); }).invoke_handler(tauri::generate_handler![api::good]); }",
        &[(
            "api.rs",
            "#[tauri::command] fn good(value: String) {} #[tauri::command] fn bad(value: String) {}",
        )],
    );
    let commands = derive_commands_from_main(closure_decoy.main()).unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "good");

    let request = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::request]); }",
        &[(
            "api.rs",
            "use tauri::ipc::Request; #[tauri::command] fn request(request: Request<'_>) {}",
        )],
    );
    let error = derive_commands_from_main(request.main()).unwrap_err();
    assert!(error.contains("raw payload"), "{error}");

    let malformed = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::bad]); }",
        &[(
            "api.rs",
            "#[tauri::command(unknown = true)] fn bad(value: String) {}",
        )],
    );
    assert!(
        derive_commands_from_main(malformed.main())
            .unwrap_err()
            .contains("unsupported tauri command")
    );

    let cfg = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::bad]); }",
        &[(
            "api.rs",
            "#[cfg_attr(test, allow(dead_code))] #[tauri::command] fn bad(value: String) {}",
        )],
    );
    assert!(
        derive_commands_from_main(cfg.main())
            .unwrap_err()
            .contains("cfg attributes")
    );

    let multiple = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::one]); tauri::Builder::default().invoke_handler(tauri::generate_handler![api::two]); }",
        &[(
            "api.rs",
            "#[tauri::command] fn one(value: String) {} #[tauri::command] fn two(value: String) {}",
        )],
    );
    assert!(
        derive_commands_from_main(multiple.main())
            .unwrap_err()
            .contains("multiple invoke_handler")
    );

    let extra_handler_argument = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::one], 1); }",
        &[("api.rs", "#[tauri::command] fn one(value: String) {}")],
    );
    assert!(
        derive_commands_from_main(extra_handler_argument.main())
            .unwrap_err()
            .contains("exactly one handler")
    );

    let wildcard_option = Fixture::new(
        "mod api; fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![api::wildcard]); }",
        &[(
            "api.rs",
            "use custom::*; #[tauri::command] fn wildcard(maybe: Option<String>) {}",
        )],
    );
    assert!(
        derive_commands_from_main(wildcard_option.main())
            .unwrap_err()
            .contains("wildcard imports")
    );
}

#[test]
fn normalized_hash_ignores_crlf_but_detects_source_changes() {
    let fixture = Fixture::new("line one\nline two\n", &[]);
    let lf = normalized_source_sha256(fixture.main()).unwrap();
    fs::write(fixture.main(), "line one\r\nline two\r\n").unwrap();
    assert_eq!(normalized_source_sha256(fixture.main()).unwrap(), lf);
    fs::write(fixture.main(), "line one\r\nchanged\r\n").unwrap();
    assert_ne!(normalized_source_sha256(fixture.main()).unwrap(), lf);
}

#[test]
fn pin_guard_parses_active_dependency_and_lock_entries() {
    let valid = WorkspaceFixture::new("2.11.5");
    assert!(manifest_from_workspace(&valid.root).is_ok());

    let invalid = WorkspaceFixture::new("2.11.4");
    let error = manifest_from_workspace(&invalid.root).unwrap_err();
    assert!(error.contains("unsupported tauri dependency"), "{error}");
}
