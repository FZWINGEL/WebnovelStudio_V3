//! Reading, validating and writing the saved Workshop state.
//!
//! This is the layer under the Workshop, not the Workshop itself. Its records
//! (`WorkshopState` and the rest) are already here in `workshop_vocabulary`,
//! because `wns-conversation`'s chat adoption names them too. The functions
//! that read and validate those records have the same two consumers and
//! therefore the same problem: `wns-workshop` (L5) persists them, and
//! `wns-conversation` (L5) reads them inside its own adoption transaction,
//! where the connection is already borrowed and cannot be handed to a host
//! method. A sibling may not reach sideways, so the layer sits below both.
//!
//! `wns_workshop::workshop` re-exports every item here, so the Workshop's own
//! call sites are unchanged.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::collections::HashSet;
use wns_kernel::{
    CoreError, CoreResult, DocumentRecord, check_id, new_id, parse_stored_version, parse_version,
    sha256_hex, valid_hash,
};
use wns_storage::{read_document, read_revision};

use crate::workshop_metadata::{validate_story_possibilities, validate_text};
use crate::workshop_vocabulary::*;
use std::collections::HashMap;

/// Validation limits for a saved Workshop state. They travelled with the
/// validators that enforce them; `wns_workshop::workshop` re-exports them.
pub const MAX_SESSIONS: usize = 256;
pub const MAX_PREFERENCES: usize = 512;
pub const MAX_DECISIONS: usize = 512;
pub const MAX_RELATIONSHIPS: usize = 512;
pub const MAX_IMPACTS: usize = 1024;
pub const MAX_PRESETS: usize = 128;
pub const MAX_LIST: usize = 512;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_DETAIL_BYTES: usize = 32 * 1024;

pub fn state_json(state: &WorkshopState) -> CoreResult<(String, String)> {
    let value = wns_kernel::canonicalize_value(serde_json::to_value(state)?);
    let json = serde_json::to_string(&value)?;
    Ok((json.clone(), sha256_hex(json.as_bytes())))
}

pub fn parse_state(json: &str, hash: &str) -> CoreResult<WorkshopState> {
    let state: WorkshopState = serde_json::from_str(json).map_err(|error| {
        CoreError::new(
            "InvalidProject",
            &format!("The workshop state is invalid: {error}"),
        )
    })?;
    let (canonical, actual_hash) = state_json(&state)?;
    if canonical != json || actual_hash != hash {
        return Err(CoreError::new(
            "InvalidProject",
            "The workshop state failed its fingerprint check.",
        ));
    }
    validate_state_shape(&state)?;
    Ok(state)
}

pub fn read_state(connection: &Connection) -> CoreResult<(i64, WorkshopState)> {
    let row: Option<(i64, String, String)> = connection
        .query_row(
            "SELECT version,state_json,state_hash FROM workshop_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match row {
        Some((version, state, hash)) if version >= 0 => Ok((version, parse_state(&state, &hash)?)),
        Some(_) => Err(CoreError::new(
            "InvalidProject",
            "The workshop state has a negative version.",
        )),
        None => Ok((0, WorkshopState::default())),
    }
}

pub fn validate_id_list(values: &[String], label: &str) -> CoreResult<()> {
    if values.len() > MAX_LIST {
        return Err(CoreError::new(
            "InvalidRequest",
            &format!("{label} contains too many entries."),
        ));
    }
    let mut seen = HashSet::new();
    for value in values {
        check_id(value)?;
        if !seen.insert(value) {
            return Err(CoreError::new(
                "InvalidRequest",
                &format!("{label} contains a duplicate identifier."),
            ));
        }
    }
    Ok(())
}

pub fn existing_document_ids(connection: &Connection) -> CoreResult<HashSet<String>> {
    let mut statement =
        connection.prepare("SELECT id FROM documents WHERE trashed=0 AND role='ordinary'")?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .collect())
}

pub fn validate_preferences(preferences: &[WorkshopPreference], label: &str) -> CoreResult<()> {
    if preferences.len() > MAX_PREFERENCES {
        return Err(CoreError::new(
            "InvalidRequest",
            "Too many workshop preferences.",
        ));
    }
    let mut ids = HashSet::new();
    for preference in preferences {
        check_id(&preference.id)?;
        if !ids.insert(&preference.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop preference IDs must be unique.",
            ));
        }
        if preference.label.trim().is_empty() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop preference labels must not be blank.",
            ));
        }
        for (value, name) in [
            (&preference.label, "preference label"),
            (&preference.family, "preference family"),
            (&preference.meaning, "preference meaning"),
            (&preference.examples, "preference examples"),
            (&preference.timing, "preference timing"),
        ] {
            validate_text(value, name, MAX_TEXT_BYTES)?;
        }
        if preference.scope == PreferenceScope::Project && preference.target_id.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Project preferences cannot have a target.",
            ));
        }
        if preference.scope != PreferenceScope::Project
            && let Some(target) = &preference.target_id
        {
            check_id(target)?;
        }
    }
    let _ = label;
    Ok(())
}

/// Validate a preset at a file or IPC boundary without applying it to a
/// project. Presets contain project-level preference vocabulary only; the
/// caller still needs to merge the returned value through saveWorkshop.
pub fn validate_workshop_preset(preset: &WorkshopPreset) -> CoreResult<()> {
    check_id(&preset.id)?;
    validate_text(&preset.name, "preset name", MAX_DETAIL_BYTES)?;
    if preset.name.trim().is_empty() {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop preset name must not be blank.",
        ));
    }
    validate_preferences(&preset.preferences, "preset preferences")?;
    if preset
        .preferences
        .iter()
        .any(|preference| preference.scope != PreferenceScope::Project)
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "A workshop preset may contain only project preferences.",
        ));
    }
    Ok(())
}

pub fn validate_hard_preference_conflicts(state: &WorkshopState) -> CoreResult<()> {
    for project in state.preferences.iter().filter(|preference| {
        preference.scope == PreferenceScope::Project
            && preference.strength == PreferenceStrength::Hard
            && preference.confirmed
    }) {
        for other in state
            .preferences
            .iter()
            .filter(|preference| preference.confirmed && preference.id != project.id)
        {
            if project.label.trim().to_lowercase() == other.label.trim().to_lowercase()
                && project.polarity != PreferencePolarity::Neutral
                && other.polarity != PreferencePolarity::Neutral
                && project.polarity != other.polarity
            {
                return Err(CoreError::new(
                    "PreferenceConflict",
                    "A workshop preference conflicts with a hard project preference.",
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_session_branch_graph(sessions: &[WorkshopSession]) -> CoreResult<()> {
    let by_id: HashMap<&str, &WorkshopSession> = sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect();

    for session in sessions {
        match (session.branch_kind, session.parent_session_id.as_deref()) {
            (WorkshopBranchKind::Working, None) => {}
            (WorkshopBranchKind::Working, Some(_)) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A working workshop session cannot have a parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, None) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session must have a parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(parent)) if parent == session.id => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session cannot parent itself.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(parent)) if !by_id.contains_key(parent) => {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A what-if workshop session has an unknown parent.",
                ));
            }
            (WorkshopBranchKind::WhatIf, Some(_)) => {}
        }
    }

    for session in sessions {
        let mut seen = HashSet::new();
        let mut current = Some(session.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop session parents cannot contain a cycle.",
                ));
            }
            current = by_id
                .get(id)
                .and_then(|parent| parent.parent_session_id.as_deref());
        }
    }
    Ok(())
}

pub fn validate_state_shape(state: &WorkshopState) -> CoreResult<()> {
    if state.schema_version != 1 {
        return Err(CoreError::new(
            "UnsupportedSchema",
            "Unsupported workshop state schema.",
        ));
    }
    if state.sessions.len() > MAX_SESSIONS
        || state.decisions.len() > MAX_DECISIONS
        || state.relationships.len() > MAX_RELATIONSHIPS
        || state.impacts.len() > MAX_IMPACTS
        || state.presets.len() > MAX_PRESETS
    {
        return Err(CoreError::new(
            "InvalidRequest",
            "Workshop state exceeds its bounded size.",
        ));
    }
    validate_session_branch_graph(&state.sessions)?;
    let mut sessions = HashSet::new();
    for session in &state.sessions {
        check_id(&session.id)?;
        if !sessions.insert(&session.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop session IDs must be unique.",
            ));
        }
        for (value, name, max) in [
            (&session.title, "session title", MAX_DETAIL_BYTES),
            (&session.brief, "session brief", MAX_TEXT_BYTES),
            (&session.direction, "session direction", MAX_TEXT_BYTES),
            (&session.still_open, "session stillOpen", MAX_TEXT_BYTES),
            (
                &session.focus_question,
                "session focusQuestion",
                MAX_TEXT_BYTES,
            ),
            (&session.focus_reason, "session focusReason", MAX_TEXT_BYTES),
            (&session.working_text, "session workingText", MAX_TEXT_BYTES),
            (
                &session.working_title,
                "session workingTitle",
                MAX_DETAIL_BYTES,
            ),
            (
                &session.working_generation,
                "session workingGeneration",
                128,
            ),
            (&session.composer, "session composer", MAX_TEXT_BYTES),
            (
                &session.selected_scope,
                "session selectedScope",
                MAX_DETAIL_BYTES,
            ),
            (
                &session.original_notes,
                "session originalNotes",
                MAX_TEXT_BYTES,
            ),
        ] {
            validate_text(value, name, max)?;
        }
        if let Some(parent) = &session.parent_session_id {
            check_id(parent)?;
        }
        for id in [
            session.focus_document_id.as_ref(),
            session.anchor_document_id.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            check_id(id)?;
        }
        validate_id_list(&session.included_document_ids, "session included documents")?;
        if session.selected_details.len() > MAX_LIST
            || session.choices.len() > MAX_LIST
            || session.questions.len() > MAX_LIST
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop session contains too many entries.",
            ));
        }
        validate_story_possibilities(&session.story_possibilities)?;
        let mut details = HashSet::new();
        for detail in &session.selected_details {
            check_id(&detail.id)?;
            if !details.insert(&detail.id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Selected detail IDs must be unique.",
                ));
            }
            validate_text(&detail.text, "selected detail", MAX_DETAIL_BYTES)?;
            if let Some(candidate) = &detail.candidate_id {
                check_id(candidate)?;
            }
        }
        let mut choices = HashSet::new();
        for choice in &session.choices {
            check_id(&choice.candidate_id)?;
            if !choices.insert(&choice.candidate_id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Candidate choice IDs must be unique.",
                ));
            }
            validate_text(&choice.rationale, "candidate rationale", MAX_DETAIL_BYTES)?;
        }
        let mut questions = HashSet::new();
        for question in &session.questions {
            check_id(&question.id)?;
            if !questions.insert(&question.id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop question IDs must be unique.",
                ));
            }
            validate_text(&question.text, "workshop question", MAX_DETAIL_BYTES)?;
            validate_text(
                &question.reason,
                "workshop question reason",
                MAX_DETAIL_BYTES,
            )?;
        }
        if let Some(run) = &session.active_run_id {
            check_id(run)?;
        }
    }
    if let Some(current) = &state.current_session_id {
        check_id(current)?;
        if !sessions.contains(current) {
            return Err(CoreError::new(
                "InvalidRequest",
                "The current workshop session does not exist.",
            ));
        }
    }
    validate_preferences(&state.preferences, "preferences")?;
    validate_hard_preference_conflicts(state)?;
    validate_preferences(
        &state
            .presets
            .iter()
            .flat_map(|preset| preset.preferences.clone())
            .collect::<Vec<_>>(),
        "preset preferences",
    )?;
    let mut decision_ids = HashSet::new();
    let mut chosen_documents = HashSet::new();
    for decision in &state.decisions {
        check_id(&decision.id)?;
        if !decision_ids.insert(&decision.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decision IDs must be unique.",
            ));
        }
        if decision.status == WorkshopDecisionStatus::Chosen
            && !chosen_documents.insert(decision.document_id.as_str())
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A document cannot have more than one chosen workshop decision.",
            ));
        }
        check_id(&decision.session_id)?;
        check_id(&decision.document_id)?;
        check_id(&decision.revision_id)?;
        check_id(&decision.head.document_id)?;
        if decision.head.document_id != decision.document_id {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision head targets another document.",
            ));
        }
        if decision.candidate_ids.len() > MAX_LIST {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision has too many candidates.",
            ));
        }
        validate_id_list(&decision.candidate_ids, "decision candidates")?;
        validate_text(&decision.title, "decision title", MAX_DETAIL_BYTES)?;
        validate_text(&decision.rationale, "decision rationale", MAX_TEXT_BYTES)?;
        if decision.access != "authorRoom" {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decisions must use authorRoom access.",
            ));
        }
        for text in &decision.protected_text {
            validate_text(text, "protected decision text", MAX_DETAIL_BYTES)?;
        }
        if let Some(id) = &decision.supersedes_id {
            check_id(id)?;
        }
    }
    let mut relationship_ids = HashSet::new();
    for relationship in &state.relationships {
        check_id(&relationship.id)?;
        if !relationship_ids.insert(&relationship.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop relationship IDs must be unique.",
            ));
        }
        check_id(&relationship.from_document_id)?;
        check_id(&relationship.to_document_id)?;
        for (value, name) in [
            (&relationship.relationship_type, "relationship type"),
            (&relationship.description, "relationship description"),
            (&relationship.uncertainty, "relationship uncertainty"),
        ] {
            validate_text(value, name, MAX_DETAIL_BYTES)?;
        }
        if relationship.source_heads.len() != 2 {
            return Err(CoreError::new(
                "InvalidRequest",
                "A relationship must retain both source heads.",
            ));
        }
        for head in &relationship.source_heads {
            check_id(&head.document_id)?;
            parse_version(&head.version)?;
            if !valid_hash(&head.body_hash) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship has an invalid source hash.",
                ));
            }
        }
    }
    let mut impact_ids = HashSet::new();
    for impact in &state.impacts {
        check_id(&impact.id)?;
        if !impact_ids.insert(&impact.id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop impact IDs must be unique.",
            ));
        }
        check_id(&impact.decision_id)?;
        check_id(&impact.document_id)?;
        validate_text(&impact.reason, "impact reason", MAX_DETAIL_BYTES)?;
        if let Some(candidate_id) = &impact.candidate_id {
            check_id(candidate_id)?;
        }
        if let Some(relationship_id) = &impact.relationship_id {
            check_id(relationship_id)?;
        }
        if impact.candidate_id.is_some() && impact.relationship_id.is_some() {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact cannot mix candidate and relationship provenance.",
            ));
        }
    }
    for preset in &state.presets {
        check_id(&preset.id)?;
        validate_text(&preset.name, "preset name", MAX_DETAIL_BYTES)?;
    }
    Ok(())
}

pub fn validate_session_relationship_references(state: &WorkshopState) -> CoreResult<()> {
    let relationship_ids: HashSet<&str> = state
        .relationships
        .iter()
        .map(|relationship| relationship.id.as_str())
        .collect();
    for session in &state.sessions {
        if let Some(relationship_id) = &session.relationship_id
            && !relationship_ids.contains(relationship_id.as_str())
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop session references an unknown relationship.",
            ));
        }
    }
    Ok(())
}

pub fn validate_state_references(
    connection: &Connection,
    state: &WorkshopState,
    current: Option<&WorkshopState>,
    extra_document_ids: &HashSet<String>,
) -> CoreResult<()> {
    validate_state_shape(state)?;
    let document_ids = existing_document_ids(connection)?;
    let session_ids: HashSet<&str> = state
        .sessions
        .iter()
        .map(|session| session.id.as_str())
        .collect();
    for session in &state.sessions {
        for id in session
            .focus_document_id
            .as_ref()
            .into_iter()
            .chain(session.included_document_ids.iter())
        {
            if !document_ids.contains(id) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A workshop session references an unknown document.",
                ));
            }
        }
        // A new manual exploration starts with a local anchor placeholder.
        // It becomes a real document only when the author explicitly adopts
        // it; every other anchor must already belong to this project.
        if let Some(anchor) = &session.anchor_document_id
            && !document_ids.contains(anchor)
            && !anchor.starts_with("workshop-")
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop session references an unknown anchor document.",
            ));
        }
    }
    validate_session_relationship_references(state)?;
    for preference in &state.preferences {
        match preference.scope {
            PreferenceScope::Project => {
                if preference.target_id.is_some() {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A project preference cannot target a local record.",
                    ));
                }
            }
            PreferenceScope::Element => {
                let Some(target) = preference.target_id.as_ref() else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An element preference needs a document target.",
                    ));
                };
                if !document_ids.contains(target) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An element preference references an unknown document.",
                    ));
                }
            }
            PreferenceScope::Exploration => {
                let Some(target) = preference.target_id.as_ref() else {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An exploration preference needs a session target.",
                    ));
                };
                if !session_ids.contains(target.as_str()) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "An exploration preference references an unknown session.",
                    ));
                }
            }
        }
    }
    for preset in &state.presets {
        validate_workshop_preset(preset)?;
    }
    let decision_map: HashMap<&str, &WorkshopDecision> = state
        .decisions
        .iter()
        .map(|decision| (decision.id.as_str(), decision))
        .collect();
    for decision in &state.decisions {
        if !session_ids.contains(decision.session_id.as_str())
            || !document_ids.contains(&decision.document_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision references an unknown target.",
            ));
        }
        let revision = read_revision(connection, &decision.revision_id)?;
        if revision.head != decision.head || revision.head.document_id != decision.document_id {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop decision provenance is invalid.",
            ));
        }
    }
    for impact in &state.impacts {
        let Some(decision) = decision_map.get(impact.decision_id.as_str()) else {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references an unknown decision.",
            ));
        };
        if !document_ids.contains(&impact.document_id) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references an unknown document.",
            ));
        }
        if let Some(candidate_id) = &impact.candidate_id
            && !decision.candidate_ids.contains(candidate_id)
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop impact references a candidate outside its decision.",
            ));
        }
        if let Some(relationship_id) = &impact.relationship_id {
            let Some(relationship) = state
                .relationships
                .iter()
                .find(|relationship| relationship.id == *relationship_id)
            else {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A workshop impact references an unknown relationship.",
                ));
            };
            if relationship.from_document_id != impact.document_id
                && relationship.to_document_id != impact.document_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship impact targets a document outside the relationship.",
                ));
            }
            if decision.document_id != impact.document_id {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship impact is bound to the wrong adoption decision.",
                ));
            }
        }
    }
    for relationship in &state.relationships {
        if (!document_ids.contains(&relationship.from_document_id)
            && !extra_document_ids.contains(&relationship.from_document_id))
            || (!document_ids.contains(&relationship.to_document_id)
                && !extra_document_ids.contains(&relationship.to_document_id))
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A workshop relationship has a dangling endpoint.",
            ));
        }
        let expected = [
            relationship.from_document_id.as_str(),
            relationship.to_document_id.as_str(),
        ];
        for (head, document_id) in relationship.source_heads.iter().zip(expected) {
            if head.document_id != document_id {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A relationship source head targets the wrong endpoint.",
                ));
            }
        }
    }
    if let Some(previous) = current {
        let previous_sessions: HashMap<&str, &WorkshopSession> = previous
            .sessions
            .iter()
            .map(|session| (session.id.as_str(), session))
            .collect();
        for session in &state.sessions {
            let Some(old) = previous_sessions.get(session.id.as_str()) else {
                continue;
            };
            if old.parent_session_id != session.parent_session_id
                || old.branch_kind != session.branch_kind
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A saved workshop session's branch identity is immutable; create a new fork instead.",
                ));
            }
        }
        let previous_decisions: HashMap<&str, &WorkshopDecision> = previous
            .decisions
            .iter()
            .map(|decision| (decision.id.as_str(), decision))
            .collect();
        for decision in &state.decisions {
            if !previous_decisions.contains_key(decision.id.as_str()) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New workshop decisions can be created only by adoption.",
                ));
            }
            let old = previous_decisions[decision.id.as_str()];
            if old.session_id != decision.session_id
                || old.title != decision.title
                || old.document_id != decision.document_id
                || old.revision_id != decision.revision_id
                || old.head != decision.head
                || old.candidate_ids != decision.candidate_ids
                || old.access != decision.access
                || old.supersedes_id != decision.supersedes_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop decision provenance is immutable; edit rationale, protection, or status instead.",
                ));
            }
        }
        if state.decisions.len() != previous.decisions.len() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop decisions cannot be removed or added by save.",
            ));
        }
        let previous_impacts: HashMap<&str, &WorkshopImpact> = previous
            .impacts
            .iter()
            .map(|impact| (impact.id.as_str(), impact))
            .collect();
        for impact in &state.impacts {
            let Some(old) = previous_impacts.get(impact.id.as_str()) else {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "New workshop impacts can be created only by adoption.",
                ));
            };
            if old.decision_id != impact.decision_id
                || old.document_id != impact.document_id
                || old.kind != impact.kind
                || old.reason != impact.reason
                || old.candidate_id != impact.candidate_id
                || old.relationship_id != impact.relationship_id
            {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "Workshop impact provenance is immutable; edit its review status instead.",
                ));
            }
        }
        if state.impacts.len() != previous.impacts.len() {
            return Err(CoreError::new(
                "InvalidRequest",
                "Workshop impacts cannot be removed or added by save.",
            ));
        }
        let old_details: HashMap<&str, &SelectedDetail> = previous
            .sessions
            .iter()
            .flat_map(|session| session.selected_details.iter())
            .map(|detail| (detail.id.as_str(), detail))
            .collect();
        for detail in state
            .sessions
            .iter()
            .flat_map(|session| session.selected_details.iter())
        {
            if let Some(old) = old_details.get(detail.id.as_str())
                && old.fixed
                && detail.fixed
                && old.text != detail.text
            {
                return Err(CoreError::new(
                    "ProtectedContentChanged",
                    "A fixed selected detail must be explicitly unprotected before editing.",
                ));
            }
        }
    }
    Ok(())
}

pub fn body_text(value: &Value) -> String {
    // Match the renderer's restricted-snapshot projection, including block
    // boundaries and hard breaks. Concatenating text nodes would change the
    // literal the author protected when a selection spans paragraphs.
    value["body"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| {
            block["content"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|node| {
                    if node["type"] == "hardBreak" {
                        "\n"
                    } else {
                        node["text"].as_str().unwrap_or_default()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn validate_protected_text(
    old: Option<&DocumentRecord>,
    new_body: &Value,
    protected: &[String],
    must_exist_in_old: &[String],
) -> CoreResult<()> {
    let new_text = body_text(new_body);
    for text in protected {
        validate_text(text, "protected text", MAX_DETAIL_BYTES)?;
        if text.is_empty() || !new_text.contains(text) {
            return Err(CoreError::new(
                "ProtectedContentChanged",
                "The adoption result removed protected text.",
            ));
        }
        if old.is_some_and(|record| {
            must_exist_in_old.contains(text) && !body_text(&record.body).contains(text)
        }) {
            return Err(CoreError::new(
                "InvalidRequest",
                "Protected text is not present in the source document.",
            ));
        }
    }
    Ok(())
}

/// Apply the existing Workshop protection fence to chat-origin material.
///
/// Chat drafts do not carry Workshop candidate or relationship records, so
/// they must not be routed through the Workshop adoption state machine.  They
/// still have to preserve text the author explicitly marked fixed, including
/// fixed decisions from prior sessions.  This narrow helper lets the chat
/// transaction enforce that invariant without changing the legacy Workshop
/// preview or receipt formats.
pub fn validate_chat_material_targets(
    connection: &Connection,
    targets: &[wns_documents::material_adoption::MaterialTarget],
) -> CoreResult<()> {
    let (_, state) = read_state(connection)?;
    for target in targets {
        let Ok(current) = read_document(connection, &target.document_id) else {
            continue;
        };
        let mut protected = Vec::new();
        if let Some(session_id) = state.current_session_id.as_deref() {
            if let Some(session) = state
                .sessions
                .iter()
                .find(|session| session.id == session_id)
            {
                protected.extend(
                    session
                        .selected_details
                        .iter()
                        .filter(|detail| {
                            detail.fixed
                                && !detail.text.is_empty()
                                && body_text(&current.body).contains(&detail.text)
                        })
                        .map(|detail| detail.text.clone()),
                );
            } else {
                return Err(CoreError::new(
                    "InvalidProject",
                    "The active Workshop session is missing.",
                ));
            }
        }
        for decision in state
            .decisions
            .iter()
            .filter(|decision| decision.fixed && decision.document_id == target.document_id)
        {
            if decision.protected_text.is_empty() {
                protected.push(body_text(
                    &read_revision(connection, &decision.revision_id)?.body,
                ));
            } else {
                protected.extend(decision.protected_text.iter().cloned());
            }
        }
        protected.sort();
        protected.dedup();
        validate_protected_text(Some(&current), &target.body, &protected, &protected)?;
    }
    Ok(())
}

/// Return the relationship records whose endpoints are among a chat
/// adoption's affected documents. The returned endpoint heads are immutable
/// drift fences; chat adoption does not silently rewrite existing edges.
pub fn chat_relationship_dependencies(
    connection: &Connection,
    document_ids: &HashSet<String>,
) -> CoreResult<Vec<WorkshopRelationship>> {
    let (_, state) = read_state(connection)?;
    Ok(state
        .relationships
        .into_iter()
        .filter(|relationship| {
            document_ids.contains(&relationship.from_document_id)
                || document_ids.contains(&relationship.to_document_id)
        })
        .collect())
}

/// Read the fixed text which a chat-origin body proposal must preserve. This
/// is deliberately a projection only; the caller owns the preview digest and
/// the transaction that rechecks it.
pub fn chat_protected_text(connection: &Connection, document_id: &str) -> CoreResult<Vec<String>> {
    let (_, state) = read_state(connection)?;
    let current = read_document(connection, document_id)?;
    let mut protected = Vec::new();
    if let Some(session_id) = state.current_session_id.as_deref() {
        let session = state
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| {
                CoreError::new("InvalidProject", "The active Workshop session is missing.")
            })?;
        protected.extend(
            session
                .selected_details
                .iter()
                .filter(|detail| {
                    detail.fixed
                        && !detail.text.is_empty()
                        && body_text(&current.body).contains(&detail.text)
                })
                .map(|detail| detail.text.clone()),
        );
    }
    for decision in state
        .decisions
        .iter()
        .filter(|decision| decision.fixed && decision.document_id == document_id)
    {
        if decision.protected_text.is_empty() {
            protected.push(body_text(
                &read_revision(connection, &decision.revision_id)?.body,
            ));
        } else {
            protected.extend(decision.protected_text.iter().cloned());
        }
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}

/// Commit newly proposed chat relationships in the caller's transaction.
/// This uses the existing Workshop state/snapshot authority while keeping chat
/// drafts out of the Workshop candidate/session adoption state machine.
pub fn append_chat_relationships(
    connection: &Connection,
    project_id: &str,
    namespace: &str,
    operation_id: &str,
    payload_hash: &str,
    expected_version: i64,
    relationships: &[WorkshopRelationship],
) -> CoreResult<String> {
    let (current_version, mut state) = read_state(connection)?;
    if current_version != expected_version {
        return Err(CoreError::new(
            "WorkshopChanged",
            "Workshop relationships changed after this chat preview.",
        ));
    }
    if relationships.is_empty() {
        return parse_stored_version(current_version);
    }
    let mut existing = state
        .relationships
        .iter()
        .map(|relationship| relationship.id.clone())
        .collect::<HashSet<_>>();
    for relationship in relationships {
        if !existing.insert(relationship.id.clone()) {
            return Err(CoreError::new(
                "InvalidRequest",
                "A chat relationship ID is already in use.",
            ));
        }
        if relationship.source_heads.len() != 2
            || relationship.source_heads[0].document_id != relationship.from_document_id
            || relationship.source_heads[1].document_id != relationship.to_document_id
        {
            return Err(CoreError::new(
                "InvalidRequest",
                "A chat relationship has invalid endpoint provenance.",
            ));
        }
        for (document_id, head) in [
            (
                &relationship.from_document_id,
                &relationship.source_heads[0],
            ),
            (&relationship.to_document_id, &relationship.source_heads[1]),
        ] {
            let document = read_document(connection, document_id)?;
            if !["character", "world"].contains(&document.kind.as_str()) || document.head != *head {
                return Err(CoreError::new(
                    "VersionConflict",
                    "A chat relationship endpoint changed before adoption.",
                ));
            }
        }
        state.relationships.push(relationship.clone());
    }
    let next_version = current_version
        .checked_add(1)
        .ok_or_else(|| CoreError::new("VersionLimit", "The workshop version limit was reached."))?;
    let extra = HashSet::new();
    validate_state_references(connection, &state, None, &extra)?;
    store_state(connection, next_version, &state)?;
    insert_snapshot(
        connection,
        project_id,
        namespace,
        operation_id,
        next_version,
        payload_hash,
        &state,
    )?;
    parse_stored_version(next_version)
}

pub fn insert_snapshot(
    connection: &Connection,
    project_id: &str,
    namespace: &str,
    operation_id: &str,
    version: i64,
    payload_hash: &str,
    state: &WorkshopState,
) -> CoreResult<()> {
    let (json, hash) = state_json(state)?;
    connection.execute(
        "INSERT INTO workshop_snapshots(id,project_id,operation_namespace,operation_id,version,payload_hash,state_json,state_hash) VALUES(?,?,?,?,?,?,?,?)",
        params![new_id(), project_id, namespace, operation_id, version, payload_hash, json, hash],
    )?;
    Ok(())
}

pub fn store_state(connection: &Connection, version: i64, state: &WorkshopState) -> CoreResult<()> {
    let (json, hash) = state_json(state)?;
    let updated = connection.execute(
        "UPDATE workshop_state SET version=?,state_json=?,state_hash=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE singleton=1",
        params![version, json, hash],
    )?;
    if updated == 0 {
        connection.execute(
            "INSERT INTO workshop_state(singleton,version,state_json,state_hash) VALUES(1,?,?,?)",
            params![version, json, hash],
        )?;
    }
    Ok(())
}
