// Generated from `transfer` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.
import type { Head } from './kernel';

export type DraftExportPreview = { id: string; projectId: string; operationNamespace: string; sourceHead: Head; revisionId: string; format: DraftFormat; formatVersion: number; utf8Bytes: number; sha256: string; formatLoss: string; previewText: string; reviewBundleId?: string | null }

export type DraftFormat = "plainText" | "markdown"

export type V2BodySelection = { kind: "workingProse" } | { kind: "requiresAuthorChoice" }

export type V2ChapterBodyChoice = "empty" | { draft: { sourceDraftId: string } }

export type V2ChapterBodyDecision = { sourceChapterId: string; choice: V2ChapterBodyChoice }

export type V2ChapterPreview = { sourceId: string; chapterNumber: number; title: string; retiredAt: string | null; workingProse: V2WorkingProse; bodySelection: V2BodySelection; workingProseBasedOnDraftId: string | null; approvedDraftId: string | null; draftCount: number; drafts: V2DraftPreview[] }

export type V2DraftPreview = { sourceId: string; version: number; prose: string; isApproved: boolean; createdAt: string }

export type V2ImportPreview = { importFormatVersion: number; source: V2SourceManifest; project: V2ProjectPreview; chapters: V2ChapterPreview[]; legacy: V2LegacyPreview }

export type V2ImportRequest = { operationId: string; sourcePath: string; sourceProjectId: string; title: string; expectedSourceSha256: string; choices: V2ChapterBodyDecision[] }

export type V2LegacyPreview = { recordCounts: { [key: string]: number }; records: V2LegacyRecord[]; totalJsonBytes: number }

export type V2LegacyRecord = { table: string; sourceId: string; payload: any }

export type V2ProjectPreview = { sourceProjectId: string; title: string; slug: string; chapterCount: number }

export type V2ProjectSummary = { sourceProjectId: string; title: string; slug: string; chapterCount: number }

export type V2SourceManifest = { schemaVersion: number; sourceBytes: number; sourceSha256: string; migrationVersions: string[]; projectCount: number }

export type V2WorkingProse = { state: "missing" } | { state: "present"; text: string }

