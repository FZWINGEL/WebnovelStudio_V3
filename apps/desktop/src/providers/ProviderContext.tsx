import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { checkClaudeConnection, checkCodexConnection, localModel, readProviderState, saveModelSettings, saveStoryMemoryProvider, type ModelKey, type ModelSelection, type ProviderState } from '../ipc/providers';

const isolatedMock: ProviderState = {
  settings: { revision: '0', active: localModel, favorites: [] },
  catalog: { models: [{ key: localModel, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' }] },
  dispatch: { kind: 'localMock', detail: 'No live AI connected' },
  codexConnection: { ready: false, detail: 'Check Settings to connect Codex.' },
  storyMemory: { revision: '0', providerId: 'mock', providerLabel: 'WebnovelStudio', modelId: 'local-editorial-v1', reasoning: null, serviceTier: null, ready: true, detail: 'Explicit local test provider. No live AI connected.' },
};
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
export function ProviderSettingsProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ProviderState | null>(null);
  const [busy, setBusy] = useState(true); const [error, setError] = useState('');
  const current = useRef(state); current.current = state;
  const flight = useRef(false); const mounted = useRef(true);
  async function refresh() {
    if (flight.current) return;
    flight.current = true; setBusy(true);
    try { const value = await readProviderState(); if (mounted.current) { setState(value); setError(''); } }
    catch (reason) { if (mounted.current) { setError(describe(reason)); setState(null); } }
    finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  async function checkConnection() {
    if (flight.current) return false;
    flight.current = true; setBusy(true); setError('');
    try {
      const value = await checkCodexConnection();
      if (!mounted.current) return false;
      setState(value); return true;
    } catch (reason) {
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
  useEffect(() => { mounted.current = true; void refresh(); return () => { mounted.current = false; }; }, []);
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
