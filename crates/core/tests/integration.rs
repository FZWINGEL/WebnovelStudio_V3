// Keep the existing suites in separate modules but compile/link one integration
// executable. Cargo still supplies the process-fixture environment and runs all
// unit, binary, and documentation tests through the normal `cargo test` command.
#[path = "support/schema.rs"]
mod legacy_schema;

macro_rules! suites {
    ($($suite:ident),+ $(,)?) => {
        $(mod $suite;)+

        #[test]
        fn every_integration_suite_is_registered() {
            use std::{collections::BTreeSet, fs, path::Path};
            let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
            let registered: BTreeSet<String> = [$(concat!(stringify!($suite), ".rs")),+]
                .into_iter().map(str::to_owned).collect();
            let discovered: BTreeSet<String> = fs::read_dir(directory).unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
                .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
                .filter(|name| name != "integration.rs")
                .collect();
            assert_eq!(registered, discovered,
                "Register each integration test file in suites! so cargo test cannot silently omit it.");
        }
    };
}

suites! {
    append_scope,
    background_work,
    claude_exec,
    claude_runner,
    codex_catalog,
    codex_exec,
    codex_profile,
    codex_runner,
    context_eligibility,
    context_lookup_protocol,
    context_migration,
    context_packet,
    context_packets,
    continuation_response,
    continuation_storage,
    discussion_lookup,
    discussions,
    evidence_history,
    evidence_queries,
    guidance,
    history,
    http_core,
    library,
    lookup_backup_integrity,
    lookup_boundaries,
    lookup_delivery_summary,
    lookup_memory_storage,
    lookup_restart,
    memory_contract,
    memory_lookup_protocol,
    memory_packet,
    memory_storage,
    metadata,
    model_settings,
    navigation_packet,
    navigation_storage,
    openai_compatible,
    persistence,
    promise_context,
    proposals,
    provider_endpoints,
    recovery_copy,
    reviewed_context,
    reviewed_context_adversarial,
    reviewed_evidence_packet,
    reviewed_export,
    reviewed_knowledge,
    reviewed_knowledge_context,
    reviewed_promises,
    reviewed_records_batch,
    reviewed_story,
    reviewed_summary,
    reviewed_summary_context,
    scope,
    source_pins,
    story_context,
    story_memory_settings,
    structured_contract_golden,
    structured_proposals,
    text_replacement,
    transfer,
    v2_import,
    windows_process,
}
