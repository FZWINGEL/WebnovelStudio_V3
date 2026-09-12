import { errorCode } from '../kernel';
import { isTauri } from '@tauri-apps/api/core';
import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { checkClaudeConnection, checkCodexConnection, localModel, readProviderState, saveModelSettings, saveStoryMemoryProvider, type ModelKey, type ModelSelection, type ProviderState } from '../ipc/providers';

const isolatedMock: ProviderState = {
  settings: { revision: '0', active: localModel, favorites: [] },
  catalog: { models: [{ key: localModel, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' }] },
  dispatch: { kind: 'localMock', detail: 'No live AI connected' },
  codexConnection: { ready: false, detail: 'Check Settings to connect Codex.' },
  storyMemory: { revision: '0', providerId: 'mock', providerLabel: 'WebnovelStudio', modelId: 'local-editorial-v1', reasoning: null, serviceTier: null, ready: true, detail: 'Explicit local test provider. No live AI connected.' },
};
const CODEX_CHECK_POLL_MS = 500;
const CODEX_CHECK_JOIN_TIMEOUT_MS = 90_000;
interface ProviderContextValue {
  state: ProviderState | null; busy: boolean; error: string;
  refresh(): Promise<void>;
  checkConnection(): Promise<boolean>;
  checkClaudeConnection(): Promise<boolean>;
  save(active: ModelSelection, favorites: ModelKey[]): Promise<boolean>;
  saveStoryMemory(providerId: string): Promise<boolean>;
}
// Isolated editor/unit-test surfaces use the existing local mock contract.
// The production root always mounts ProviderSettingsProvider and loads Rust state.
const Providers = createContext<ProviderContextValue>({ state: isolatedMock, busy: false, error: '', refresh: async () => {}, checkConnection: async () => false, checkClaudeConnection: async () => false, save: async () => false, saveStoryMemory: async () => false });
export const useProviders = () => useContext(Providers);
function describe(error: unknown): string {
  return error && typeof error === 'object' && 'detail' in error ? String(error.detail) : 'Could not confirm the saved model choice. Check Settings before sending another request.';
}
/**
 * A fresh library starts on the deterministic mock.  Once native Codex has
 * been checked, it is safe to offer the installed Luna profile as the first
 * live default only for that untouched preference revision.  Any saved
 * choice, including an explicit choice of the local mock, is authoritative.
 */
function initialCodexChoice(value: ProviderState): ModelSelection | null {
  if (value.settings.revision !== '0'
    || value.settings.active.providerId !== localModel.providerId
    || value.settings.active.modelId !== localModel.modelId) return null;
  const model = value.catalog.models.find(candidate => candidate.key.providerId === 'codex' && candidate.key.modelId === 'gpt-5.6-luna');
  if (!model?.ready || !model.reasoningLevels.includes('xhigh') || !model.serviceTiers.some(tier => tier.id === 'priority')) return null;
  return { providerId: model.key.providerId, modelId: model.key.modelId, reasoning: 'xhigh', serviceTier: 'priority' };
}
export function ProviderSettingsProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ProviderState | null>(null);
  const [busy, setBusy] = useState(true); const [error, setError] = useState('');
  const current = useRef(state); current.current = state;
  const flight = useRef(false); const mounted = useRef(true);
  const startupProbeAttempted = useRef(false);
  async function joinExistingCodexCheck(initial: ProviderState | null, readImmediately: boolean): Promise<ProviderState | null> {
    const deadline = Date.now() + CODEX_CHECK_JOIN_TIMEOUT_MS;
    let latest = initial;
    while (mounted.current) {
      if (readImmediately || !latest) {
        try { latest = await readProviderState(); }
        catch { latest = null; }
        readImmediately = false;
        if (!mounted.current) return null;
        if (latest && latest.codexConnection?.checking !== true) return latest;
      } else if (latest.codexConnection?.checking !== true) {
        return latest;
      }
      const remaining = deadline - Date.now();
      if (remaining <= 0) break;
      await new Promise(resolve => setTimeout(resolve, Math.min(CODEX_CHECK_POLL_MS, remaining)));
      if (!mounted.current) return null;
      try { latest = await readProviderState(); }
      catch { latest = null; }
      if (!mounted.current) return null;
      if (latest && latest.codexConnection?.checking !== true) return latest;
    }
    if (!mounted.current) return null;
    throw { code: 'ConnectionCheckJoinTimeout', detail: 'The existing Codex connection check did not finish within 90 seconds. Open Settings and try again.' };
  }
  async function adoptInitialCodexChoice(value: ProviderState): Promise<ProviderState> {
    const choice = initialCodexChoice(value);
    if (!choice) return value;
    return saveModelSettings(value.settings.revision, choice, value.settings.favorites);
  }
  async function refresh(startup = false) {
    if (flight.current) return;
    flight.current = true; setBusy(true);
    try {
      let value = await readProviderState();
      if (mounted.current) { setState(value); setError(''); }
      // The native check is a bounded executable/version/auth probe only. It
      // does not create a generation request or send manuscript content.
      const shouldProbeStartup = startup && isTauri() && !value.codexConnection?.ready
        && (value.settings.active.providerId === 'codex' || value.settings.revision === '0');
      const joinedExistingCheck = value.codexConnection?.checking === true;
      if (joinedExistingCheck) {
        try {
          value = await joinExistingCodexCheck(value, false) ?? value;
        } catch (reason) {
          // The initial provider read was valid. Keep its saved model/catalog
          // visible while reporting that the native check did not reconcile.
          if (mounted.current) { setState(value); setError(describe(reason)); }
          return;
        }
        if (!mounted.current) return;
        try { value = await adoptInitialCodexChoice(value); }
        catch (reason) { if (mounted.current) setError(describe(reason)); }
        if (mounted.current) setState(value);
      }
      if (shouldProbeStartup && !joinedExistingCheck) {
        try {
          value = await checkCodexConnection();
          try { value = await adoptInitialCodexChoice(value); }
          catch (reason) { if (mounted.current) setError(describe(reason)); }
          if (mounted.current) setState(value);
        } catch (reason) {
          if (errorCode(reason) === 'ConnectionCheckRunning') {
            try {
              value = await joinExistingCodexCheck(value, true) ?? value;
              if (!mounted.current) return;
              setState(value);
              value = await adoptInitialCodexChoice(value);
              if (mounted.current) { setState(value); setError(''); }
              return;
            } catch (joinReason) {
              if (mounted.current) setError(describe(joinReason));
              return;
            }
          }
          // Keep the saved state visible when the read-only startup probe is
          // unavailable. Settings can retry it without changing preferences.
          if (mounted.current) setError(describe(reason));
        }
      }
    }
    catch (reason) { if (mounted.current) { setError(describe(reason)); setState(null); } }
    finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  async function checkConnection() {
    if (flight.current) return false;
    flight.current = true; setBusy(true); setError('');
    try {
      let value = await checkCodexConnection();
      try { value = await adoptInitialCodexChoice(value); }
      catch (reason) { if (mounted.current) setError(describe(reason)); }
      if (!mounted.current) return false;
      setState(value); return true;
    } catch (reason) {
      if (errorCode(reason) === 'ConnectionCheckRunning') {
        try {
          const joined = await joinExistingCodexCheck(null, true);
          if (!joined || !mounted.current) return false;
          setState(joined);
          const reconciled = await adoptInitialCodexChoice(joined);
          if (!mounted.current) return false;
          setState(reconciled); setError(''); return true;
        } catch (joinReason) {
          if (mounted.current) setError(describe(joinReason));
          return false;
        }
      }
      // A failed probe must not replace the saved choice. Re-read the current
      // state once so a completed native check can still reconcile its status.
      let recovered = current.current;
      try { recovered = await readProviderState(); } catch { /* Keep the last known state. */ }
      if (mounted.current) { setState(recovered); setError(describe(reason)); }
      return false;
    } finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  async function checkClaude() {
    if (flight.current) return false;
    flight.current = true; setBusy(true); setError('');
    try {
      const value = await checkClaudeConnection();
      if (!mounted.current) return false;
      setState(value); return true;
    } catch (reason) {
      let recovered = current.current;
      try { recovered = await readProviderState(); } catch { /* Keep the last known state after a failed explicit probe. */ }
      if (mounted.current) { setState(recovered); setError(describe(reason)); }
      return false;
    } finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  useEffect(() => {
    mounted.current = true;
    if (!startupProbeAttempted.current) {
      startupProbeAttempted.current = true;
      void refresh(true);
    }
    return () => { mounted.current = false; };
  }, []);
  async function save(active: ModelSelection, favorites: ModelKey[]) {
    if (flight.current || !current.current) return false;
    const before = current.current;
    flight.current = true; setBusy(true); setError('');
    try {
      const saved = await saveModelSettings(before.settings.revision, active, favorites);
      if (!mounted.current) return false;
      setState(saved); return true;
    } catch (reason) {
      // Preference writes have no model side effects. Read their current value
      // after a lost acknowledgment or conflict, without replaying the write.
      let recovered: ProviderState | null = null;
      try { recovered = await readProviderState(); } catch { /* Keep sending unavailable until an explicit refresh succeeds. */ }
      if (mounted.current) { setState(recovered); setError(describe(reason)); }
      return false;
    } finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  async function saveStoryMemory(providerId: string) {
    if (flight.current || !current.current?.storyMemory) return false;
    const before = current.current;
    const target = before.storyMemory!;
    flight.current = true; setBusy(true); setError('');
    try {
      const saved = await saveStoryMemoryProvider(target.revision, providerId);
      if (!mounted.current) return false;
      setState(saved); return true;
    } catch (reason) {
      let recovered: ProviderState | null = null;
      try { recovered = await readProviderState(); } catch { /* Keep the prior target until an explicit refresh succeeds. */ }
      if (mounted.current) { setState(recovered); setError(describe(reason)); }
      return false;
    } finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  return <Providers.Provider value={{ state, busy, error, refresh, checkConnection, checkClaudeConnection: checkClaude, save, saveStoryMemory }}>{children}</Providers.Provider>;
}
