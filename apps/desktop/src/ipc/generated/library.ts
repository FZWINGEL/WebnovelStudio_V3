// Generated from `library` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.

export type CodexTransport = "exec" | "appServer"

export type CodexTransportSettings = { revision: string; transport: CodexTransport }

/**
 * Identity of the library operation that installed this independent folder.
 * Kept beside the database so registry recovery does not require a schema upgrade.
 */
export type CreationOrigin = { operationNamespace: string; operationId: string }

export type LibraryEntry = { projectId: string; title: string; path: string; archived: boolean; lastOpened: string; missing: boolean }

export type PendingProject = { origin: CreationOrigin; kind: string; title: string; stagingPath: string; finalPath: string; sourcePath: string | null; sourceFingerprint: string | null; completed: boolean }

