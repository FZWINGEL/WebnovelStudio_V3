PRAGMA foreign_keys = ON;
PRAGMA user_version = 8;

BEGIN;

CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL);
INSERT INTO schema_migrations VALUES
  (1, '001-persistence-floor'), (2, '002-authority-boundary'),
  (3, '003-draft-run'), (4, '004-audit-approval'),
  (5, '005-settlement'), (6, '006-option-model'),
  (7, '007-draft-segments'), (8, '008-local-first-retire');

CREATE TABLE projects (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, slug TEXT NOT NULL UNIQUE,
  genre TEXT NOT NULL, summary TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  terminology_reviewed INTEGER NOT NULL DEFAULT 0, continuity_dirty_from_chapter INTEGER
);
CREATE TABLE story_bibles (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), premise TEXT NOT NULL, core_conflict TEXT NOT NULL, themes TEXT NOT NULL, tone_guidelines TEXT NOT NULL, target_audience TEXT, serialization_hooks TEXT, forbidden_tropes TEXT, version INTEGER NOT NULL, is_active INTEGER NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE canon_entities (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), category TEXT NOT NULL, name TEXT NOT NULL, aliases TEXT NOT NULL, pinyin TEXT, power_level TEXT, power_rank INTEGER, role TEXT, personality_traits TEXT NOT NULL, voice_description TEXT, secrets TEXT NOT NULL, goals TEXT NOT NULL, status TEXT NOT NULL, attributes TEXT NOT NULL, introduced_in_chapter INTEGER, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, active_revision_id TEXT, retired_at TEXT);
CREATE TABLE termbase (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), concept_id TEXT NOT NULL, category TEXT NOT NULL, source_term TEXT, english_translation TEXT NOT NULL, pinyin TEXT, forbidden_substitutions TEXT NOT NULL, required_context_note TEXT, is_locked INTEGER NOT NULL, created_at TEXT NOT NULL, active_revision_id TEXT, retired_at TEXT);
CREATE TABLE plot_threads (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), title TEXT NOT NULL, description TEXT NOT NULL, category TEXT NOT NULL, status TEXT NOT NULL, introduced_chapter INTEGER NOT NULL, target_resolution_chapter INTEGER, resolved_in_chapter INTEGER, notes TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, active_revision_id TEXT, retired_at TEXT);
CREATE TABLE story_arcs (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), arc_number INTEGER NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL, goals TEXT NOT NULL, start_chapter INTEGER NOT NULL, end_chapter INTEGER, created_at TEXT NOT NULL, active_revision_id TEXT, retired_at TEXT);
CREATE TABLE chapters (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), arc_id TEXT REFERENCES story_arcs(id), chapter_number INTEGER NOT NULL,
  title TEXT NOT NULL, summary TEXT NOT NULL, pov_character_id TEXT, target_word_count INTEGER NOT NULL, tension_level INTEGER NOT NULL,
  cliffhanger_hook TEXT, beats TEXT NOT NULL, key_events TEXT NOT NULL, active_canon_ids TEXT NOT NULL, active_plot_thread_ids TEXT NOT NULL,
  status TEXT NOT NULL, approved_draft_id TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  active_plan_revision_id TEXT, accepted_summary_revision_id TEXT, working_plan_json TEXT, working_plan_updated_at TEXT,
  working_prose TEXT, working_prose_updated_at TEXT, working_prose_based_on_draft_id TEXT, retired_at TEXT
);
CREATE TABLE chapter_drafts (
  id TEXT PRIMARY KEY, chapter_id TEXT NOT NULL REFERENCES chapters(id), version INTEGER NOT NULL, prose TEXT NOT NULL,
  word_count INTEGER NOT NULL, model_name TEXT NOT NULL, temperature REAL, generation_time_ms INTEGER,
  audit_summary TEXT, is_approved INTEGER NOT NULL, created_at TEXT NOT NULL, parent_draft_id TEXT,
  content_hash TEXT, origin TEXT, run_id TEXT, is_partial INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE generation_options (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), stage TEXT NOT NULL, reference_id TEXT,
  option_index INTEGER NOT NULL, title TEXT NOT NULL, summary TEXT, payload TEXT NOT NULL, is_selected INTEGER NOT NULL,
  created_at TEXT NOT NULL, source_run_id TEXT
);
CREATE TABLE authoritative_revisions (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), target_type TEXT NOT NULL, target_id TEXT NOT NULL,
  parent_revision_id TEXT, payload TEXT NOT NULL, source_option_id TEXT, source_proposal_id TEXT, created_at TEXT NOT NULL
);
CREATE TABLE context_receipts (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), chapter_id TEXT NOT NULL REFERENCES chapters(id),
  plan_revision_id TEXT, canon_revision_set_hash TEXT, termbase_revision_set_hash TEXT, sources_json TEXT NOT NULL,
  system_message TEXT NOT NULL, user_message TEXT NOT NULL, provider_options TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE generation_runs (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), stage TEXT NOT NULL, reference_id TEXT,
  context_receipt_id TEXT, resumed_from_run_id TEXT, frozen_request TEXT NOT NULL, status TEXT NOT NULL, provider TEXT,
  model TEXT, options TEXT NOT NULL, token_usage TEXT, error TEXT, cancel_requested_at TEXT, created_at TEXT NOT NULL, completed_at TEXT
);
CREATE TABLE audit_runs (
  id TEXT PRIMARY KEY, draft_id TEXT NOT NULL REFERENCES chapter_drafts(id), layer TEXT NOT NULL, status TEXT NOT NULL,
  input_hash TEXT, prose_hash TEXT, plan_revision_id TEXT, canon_revision_set_hash TEXT, termbase_revision_set_hash TEXT,
  rule_set_version TEXT, score REAL, summary TEXT, error TEXT, created_at TEXT NOT NULL
);
CREATE TABLE audit_findings (
  id TEXT PRIMARY KEY, audit_run_id TEXT NOT NULL REFERENCES audit_runs(id), draft_id TEXT NOT NULL,
  layer TEXT NOT NULL, type TEXT NOT NULL, severity TEXT NOT NULL, message TEXT NOT NULL, quoted_span TEXT, suggested_fix TEXT, created_at TEXT NOT NULL
);
CREATE TABLE finding_waivers (id TEXT PRIMARY KEY, finding_id TEXT NOT NULL REFERENCES audit_findings(id), rationale TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE settlement_runs (id TEXT PRIMARY KEY, chapter_id TEXT NOT NULL REFERENCES chapters(id), source_draft_id TEXT, status TEXT NOT NULL, error TEXT, created_at TEXT NOT NULL);
CREATE TABLE settlement_proposals (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), chapter_id TEXT NOT NULL REFERENCES chapters(id), source_draft_id TEXT,
  origin TEXT NOT NULL, kind TEXT NOT NULL, criticality TEXT NOT NULL, payload TEXT NOT NULL, quoted_evidence TEXT, confidence REAL,
  decision TEXT NOT NULL, decision_note TEXT, prior_revision_id TEXT, resulting_revision_id TEXT, decided_at TEXT, created_at TEXT NOT NULL
);
CREATE TABLE draft_segments (
  id TEXT PRIMARY KEY, draft_id TEXT NOT NULL REFERENCES chapter_drafts(id), beat_id TEXT NOT NULL, sequence INTEGER NOT NULL,
  prose TEXT NOT NULL, source_run_id TEXT, parent_segment_id TEXT, created_at TEXT NOT NULL
);

INSERT INTO projects VALUES ('p-alpha', 'Alpha Story', 'alpha', 'cultivation', 'alpha summary', '2026-01-01', '2026-01-02', 1, NULL);
INSERT INTO projects VALUES ('p-beta', 'Beta Story', 'beta', 'fantasy', 'beta summary', '2026-01-01', '2026-01-02', 0, NULL);
INSERT INTO story_bibles VALUES ('b-alpha', 'p-alpha', 'premise', 'conflict', '["duty"]', 'restrained', 'adult', '[]', '[]', 1, 1, '2026-01-01', '2026-01-01');
INSERT INTO story_bibles VALUES ('b-beta', 'p-beta', 'premise', 'conflict', '["hope"]', 'bright', 'adult', '[]', '[]', 1, 1, '2026-01-01', '2026-01-01');
INSERT INTO canon_entities VALUES ('c-alpha', 'p-alpha', 'character', 'Ari', '[]', NULL, NULL, 0, 'hero', '[]', NULL, '[]', '[]', 'alive', '{}', 1, '2026-01-01', '2026-01-01', NULL, NULL);
INSERT INTO canon_entities VALUES ('c-beta', 'p-beta', 'character', 'Bex', '[]', NULL, NULL, 0, 'hero', '[]', NULL, '[]', '[]', 'alive', '{}', 1, '2026-01-01', '2026-01-01', NULL, NULL);
INSERT INTO termbase VALUES ('t-alpha', 'p-alpha', 'qi', 'cultivation_realm', '气', 'Qi', 'qi', '[]', NULL, 1, '2026-01-01', NULL, NULL);
INSERT INTO termbase VALUES ('t-beta', 'p-beta', 'star', 'general', NULL, 'Star', NULL, '[]', NULL, 1, '2026-01-01', NULL, NULL);
INSERT INTO plot_threads VALUES ('pt-alpha', 'p-alpha', 'The Gate', 'open gate', 'main_plot', 'open', 1, NULL, NULL, NULL, '2026-01-01', '2026-01-01', NULL, NULL);
INSERT INTO plot_threads VALUES ('pt-beta', 'p-beta', 'The Road', 'open road', 'main_plot', 'open', 1, NULL, NULL, NULL, '2026-01-01', '2026-01-01', NULL, NULL);
INSERT INTO story_arcs VALUES ('arc-alpha', 'p-alpha', 1, 'Arrival', 'arrival', '[]', 1, 4, '2026-01-01', NULL, NULL);
INSERT INTO story_arcs VALUES ('arc-beta', 'p-beta', 1, 'Departure', 'departure', '[]', 1, 1, '2026-01-01', NULL, NULL);

INSERT INTO chapters VALUES ('a-null', 'p-alpha', 'arc-alpha', 1, 'Null body', 'summary', 'c-alpha', 1000, 5, NULL, '[{"id":"beat-a"}]', '[]', '["c-alpha"]', '["pt-alpha"]', 'outlined', 'a-old', '2026-01-01', '2026-01-01', NULL, NULL, NULL, NULL, NULL, '2026-01-01', 'a-old', NULL);
INSERT INTO chapters VALUES ('a-empty', 'p-alpha', 'arc-alpha', 2, 'Empty body', 'summary', 'c-alpha', 1000, 5, NULL, '[{"id":"beat-b"}]', '[]', '["c-alpha"]', '["pt-alpha"]', 'outlined', 'a-empty-old', '2026-01-01', '2026-01-01', NULL, NULL, NULL, NULL, '', '2026-01-01', 'a-empty-old', NULL);
INSERT INTO chapters VALUES ('a-newer', 'p-alpha', 'arc-alpha', 3, 'Newer body', 'summary', 'c-alpha', 1000, 5, NULL, '[{"id":"beat-c"}]', '[]', '["c-alpha"]', '["pt-alpha"]', 'outlined', 'a-newer-old', '2026-01-01', '2026-01-01', NULL, NULL, NULL, NULL, 'Working newer ' || char(13) || char(10) || 'text', '2026-01-02', 'a-newer-old', NULL);
INSERT INTO chapters VALUES ('a-retired', 'p-alpha', 'arc-alpha', 4, 'Retired body', 'summary', 'c-alpha', 1000, 5, NULL, '[{"id":"beat-d"}]', '[]', '["c-alpha"]', '["pt-alpha"]', 'outlined', 'a-retired-old', '2026-01-01', '2026-01-01', NULL, NULL, NULL, NULL, 'retired prose', '2026-01-01', 'a-retired-old', '2026-01-03');
INSERT INTO chapters VALUES ('b-one', 'p-beta', 'arc-beta', 1, 'Beta body', 'summary', 'c-beta', 1000, 5, NULL, '[{"id":"beat-e"}]', '[]', '["c-beta"]', '["pt-beta"]', 'outlined', 'b-old', '2026-01-01', '2026-01-01', NULL, NULL, NULL, NULL, 'beta prose', '2026-01-01', 'b-old', NULL);

INSERT INTO chapter_drafts VALUES ('a-old', 'a-null', 1, 'Approved null', 2, 'mock', 0.7, 1, NULL, 1, '2026-01-01', NULL, 'hash-a-old', 'seed', NULL, 0);
INSERT INTO chapter_drafts VALUES ('a-empty-old', 'a-empty', 1, 'Approved empty', 2, 'mock', 0.7, 1, NULL, 1, '2026-01-01', NULL, 'hash-a-empty', 'seed', NULL, 0);
INSERT INTO chapter_drafts VALUES ('a-newer-old', 'a-newer', 1, 'Approved old', 2, 'mock', 0.7, 1, NULL, 1, '2026-01-01', NULL, 'hash-a-newer', 'seed', NULL, 0);
INSERT INTO chapter_drafts VALUES ('a-retired-old', 'a-retired', 1, 'Retired old', 2, 'mock', 0.7, 1, NULL, 1, '2026-01-01', NULL, 'hash-a-retired', 'seed', NULL, 0);
INSERT INTO chapter_drafts VALUES ('b-old', 'b-one', 1, 'Beta old', 2, 'mock', 0.7, 1, NULL, 1, '2026-01-01', NULL, 'hash-b', 'seed', NULL, 0);
INSERT INTO generation_options VALUES ('opt-a', 'p-alpha', 'draft', 'a-newer', 1, 'Option', 'option', '{"text":"candidate"}', 0, '2026-01-01', NULL);
INSERT INTO authoritative_revisions VALUES ('rev-a', 'p-alpha', 'chapter_plan', 'a-newer', NULL, '{"title":"Newer body"}', NULL, NULL, '2026-01-01');
INSERT INTO context_receipts VALUES ('ctx-a', 'p-alpha', 'a-newer', 'rev-a', 'canon', 'terms', '["c-alpha"]', 'system', 'user', '{}', '2026-01-01');
INSERT INTO generation_runs VALUES ('run-a', 'p-alpha', 'draft', 'a-newer', 'ctx-a', NULL, '{"request":"draft"}', 'completed', 'mock', 'mock', '{}', '{}', NULL, NULL, '2026-01-01', '2026-01-01');
INSERT INTO audit_runs VALUES ('audit-a', 'a-newer-old', 'L1', 'passed', NULL, NULL, NULL, NULL, NULL, NULL, 1.0, 'ok', NULL, '2026-01-01');
INSERT INTO audit_findings VALUES ('finding-a', 'audit-a', 'a-newer-old', 'L1', 'style', 'info', 'fine', NULL, NULL, '2026-01-01');
INSERT INTO finding_waivers VALUES ('waiver-a', 'finding-a', 'accepted', '2026-01-01');
INSERT INTO settlement_runs VALUES ('settle-a', 'a-newer', 'a-newer-old', 'completed', NULL, '2026-01-01');
INSERT INTO settlement_proposals VALUES ('proposal-a', 'p-alpha', 'a-newer', 'a-newer-old', 'run', 'canon', 'low', '{}', NULL, 1.0, 'pending', NULL, NULL, NULL, NULL, '2026-01-01');
INSERT INTO draft_segments VALUES ('segment-a', 'a-newer-old', 'beat-c', 1, 'Approved old', NULL, NULL, '2026-01-01');

COMMIT;
