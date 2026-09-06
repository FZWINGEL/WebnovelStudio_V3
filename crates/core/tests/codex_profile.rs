use std::path::Path;

use webnovel_core::providers::codex_profile::{
    CODEX_FAST_TIER_LABEL, CODEX_LUNA_MODEL, CODEX_PRIORITY_SERVICE_TIER, CODEX_REASONING_EFFORT,
    CodexLaunchProfile, CodexProfileError,
};

fn profile(version: &str) -> CodexLaunchProfile {
    CodexLaunchProfile::for_version(version, Path::new(r"D:\owned\codex\catalog.json"))
        .expect("valid version should build a profile")
}

#[test]
fn observed_versions_build_the_same_restrictive_luna_catalog() {
    let current = profile("codex-cli 0.153.4\n");
    assert_eq!(current.executable_version, "0.153.4");
    assert_eq!(current.model, CODEX_LUNA_MODEL);
    assert_eq!(current.reasoning_effort, CODEX_REASONING_EFFORT);
    assert_eq!(current.reasoning_effort, "xhigh");
    assert_eq!(current.service_tier, CODEX_PRIORITY_SERVICE_TIER);
    assert!(current.catalog_json.contains("gpt-5.6-luna"));

    let other = profile("codex-cli 9.4.1");
    assert_eq!(other.executable_version, "9.4.1");
    assert_eq!(other.config_overrides, current.config_overrides);

    let model = &current.catalog.models[0];
    assert_eq!(model.slug, CODEX_LUNA_MODEL);
    assert_eq!(model.tool_mode, "direct");
    assert_eq!(model.shell_type, "disabled");
    assert_eq!(model.apply_patch_tool_type, None);
    assert!(model.experimental_supported_tools.is_empty());
    assert_eq!(model.multi_agent_version, None);
    assert!(!model.supports_search_tool);
    assert_eq!(model.context_window, None);
    assert_eq!(model.max_context_window, None);
    assert_eq!(model.service_tiers[0].id, CODEX_PRIORITY_SERVICE_TIER);
    assert_eq!(model.service_tiers[0].name, CODEX_FAST_TIER_LABEL);
    assert_eq!(model.default_service_tier, CODEX_PRIORITY_SERVICE_TIER);
    assert_eq!(model.model_messages["instructions_template"], "");
    for field in [
        "approvals",
        "collaboration_modes",
        "auto_review",
        "permissions",
        "multi_agent",
        "token_budget",
        "guardian_v2",
        "confirmation_policies",
    ] {
        assert_eq!(model.model_messages[field], serde_json::Value::Null);
    }
    assert_eq!(model.availability_nux, None);
    assert_eq!(model.upgrade, None);
}

#[test]
fn malformed_version_output_is_rejected_without_guessing_a_binary_version() {
    for version in [
        "",
        "codex 0.153.4",
        "codex-cli",
        "codex-cli dev",
        "codex-cli 0.153.4 extra",
        "codex-cli 0.153.4\nextra",
        "codex-cli 0.153.4/evil",
        "codex-cli 0_153_4",
    ] {
        let error = CodexLaunchProfile::for_version(version, Path::new("catalog.json"))
            .expect_err("version output must be bounded and parseable");
        assert!(matches!(
            error,
            CodexProfileError::InvalidVersionOutput { .. }
        ));
    }
    let oversized = format!("codex-cli {}", "1".repeat(129));
    assert!(matches!(
        CodexLaunchProfile::for_version(&oversized, Path::new("catalog.json")),
        Err(CodexProfileError::InvalidVersionOutput { .. })
    ));
}

#[test]
fn capability_preflight_reuses_stdin_profile_and_adds_only_validation_inputs() {
    let profile = profile("codex-cli 2026.09");
    let arguments = profile
        .preflight_arguments(
            Path::new(r"D:\owned\cwd"),
            Path::new(r"D:\owned\missing.schema.json"),
            Some("codex_qualification_sentinel=true"),
        )
        .expect("valid preflight paths");
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        arguments
            .windows(2)
            .any(|pair| { pair == ["--output-schema", "D:/owned/missing.schema.json"] })
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| { pair == ["-c", "codex_qualification_sentinel=true"] })
    );
    assert_eq!(arguments.last().map(String::as_str), Some("-"));
    assert!(arguments.contains(&"--strict-config".to_owned()));
}

#[test]
fn launch_arguments_include_strict_profile_and_stdin_without_packet_text() {
    let profile = profile("codex-cli 0.153.4");
    let arguments = profile
        .exec_arguments(Path::new(r"D:\owned\cwd"))
        .expect("valid cwd");
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(arguments[0], "exec");
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["--cd", "D:/owned/cwd"])
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["-m", CODEX_LUNA_MODEL])
    );
    assert_eq!(arguments.last().map(String::as_str), Some("-"));
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("story context"))
    );

    let override_values = arguments
        .windows(2)
        .filter_map(|pair| (pair[0] == "-c").then_some(pair[1].as_str()))
        .collect::<Vec<_>>();
    for expected in [
        "model=\"gpt-5.6-luna\"",
        "model_reasoning_effort=\"xhigh\"",
        "service_tier=\"priority\"",
        "approval_policy=\"never\"",
        "web_search=\"disabled\"",
        "project_doc_max_bytes=0",
        "mcp_servers={}",
        "plugins={}",
        "agents.enabled=false",
        "tools.experimental_request_user_input.enabled=false",
        "tools.update_plan.enabled=false",
        "features.tool_registry.turn_metadata_includes_tool_info=false",
        "features.tool_registry.error_on_tool_collisions=true",
        "features.shell_tool=false",
        "features.unified_exec=false",
        "features.exec_permission_approvals=false",
        "features.request_permissions_tool=false",
        "features.view_image=false",
        "features.sleep_tool=false",
        "features.deferred_executor=false",
        "features.token_budget=false",
        "features.current_time_reminder=false",
        "features.goals=false",
        "features.multi_agent=false",
        "features.multi_agent_v2=false",
        "features.apps=false",
        "features.plugins=false",
        "features.recommended_plugins=false",
        "features.enable_mcp_apps=false",
        "features.mcp_2026_07_28=false",
        "features.mcp_oauth_refresh_coordination=false",
        "features.non_prefixed_mcp_tool_names=false",
        "features.skill_search=false",
        "features.skill_mcp_dependency_install=false",
        "features.skip_host_skill_discovery=true",
        "features.tool_suggest=false",
        "features.memories=false",
        "features.image_generation=false",
        "features.standalone_web_search=false",
        "features.browser_use=false",
        "features.browser_use_full_cdp_access=false",
        "features.browser_use_external=false",
        "features.computer_use=false",
        "features.code_mode=false",
        "features.code_mode_only=false",
        "features.code_mode_host=false",
        "features.code_mode_prewarm=false",
        "features.code_mode_interrupt=false",
        "features.hooks=false",
        "features.guardian_approval=false",
        "features.guardianv2=false",
        "features.remote_plugin=false",
        "features.plugin_sharing=false",
        "features.in_app_browser=false",
        "features.in_app_chat=false",
        "features.in_app_dictation=false",
        "features.in_app_local_automation=false",
        "features.in_app_updates=false",
        "features.workspace_dependencies=false",
        "features.shell_snapshot=false",
        "features.executor_capability_discovery=false",
        "features.unbounded_connection_retries=false",
        "features.tool_call_mcp_elicitation=false",
        "features.auth_elicitation=false",
    ] {
        assert!(
            override_values.contains(&expected),
            "missing override {expected}"
        );
    }
    assert!(
        !override_values
            .iter()
            .any(|value| value.contains("no_tools"))
    );
    assert!(
        !override_values
            .iter()
            .any(|value| value.contains("experimental_use_unified_exec_tool"))
    );
    assert!(
        !override_values
            .iter()
            .any(|value| value == &"features.apply_patch_freeform=false")
    );
    assert!(
        !override_values
            .iter()
            .any(|value| value == &"features.tool_search=false")
    );
}

#[test]
fn paths_are_rendered_without_windows_backslash_or_control_injection() {
    let profile = profile("codex-cli 0.153.4");
    let arguments = profile
        .exec_arguments(Path::new(r"D:\owned\work"))
        .expect("valid path");
    let joined = arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(joined.contains("D:/owned/work"));
    assert!(!joined.contains(r"D:\owned\work"));

    let error = CodexLaunchProfile::for_version(
        "codex-cli 0.153.4",
        Path::new("catalog.json\n-injected=true"),
    )
    .expect_err("control characters must not enter runtime config");
    assert!(matches!(error, CodexProfileError::InvalidPath(_)));
}
