// Generated from `providers` by `crates/bindings`. Do not edit.
// Change the Rust type and run `cargo run -p wns-bindings`.

export type CatalogOrigin = "builtIn" | "reference" | "codexDiscovery" | "openAiCompatible"

export type CatalogSnapshot = { schemaVersion: number; models: ModelDescriptor[] }

export type DispatchResolution = { kind: "localMock"; detail: string } | { kind: "codexCli"; detail: string } | { kind: "claudeCli"; detail: string } | { kind: "openAiCompatible"; detail: string } | { kind: "blocked"; detail: string }

/**
 * Persisted endpoint configuration and non-authoritative model discovery.
 */
export type EndpointProfile = { id: string; label: string; baseUrl: string; enabled: boolean; jsonMode?: boolean; configRevision: string; credentialRef?: string | null; manualModelIds?: string[]; cachedModelIds?: string[] }

/**
 * A serializable catalog entry identified by the exact provider and model
 * IDs.  Display labels are descriptive only; dispatch must use the IDs.
 */
export type ModelDescriptor = { key: ModelKey; label: string; providerLabel: string; reasoningLevels: string[]; serviceTiers: ServiceTier[]; contextWindowTokens: string | null; maxOutputTokens: string | null; defaultReasoning?: string | null; defaultServiceTier?: string | null; origin: CatalogOrigin; ready: boolean; statusDetail: string }

export type ModelKey = { providerId: string; modelId: string }

export type ModelSelection = { providerId: string; modelId: string; reasoning?: string | null; serviceTier?: string | null }

export type ModelSettings = { revision: string; active: ModelSelection; favorites: ModelKey[] }

export type ProviderState = { settings: ModelSettings; catalog: CatalogSnapshot; dispatch: DispatchResolution }

export type ServiceTier = { id: string; label: string }

