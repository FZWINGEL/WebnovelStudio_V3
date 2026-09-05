import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { localModel, readProviderState, saveModelSettings, type ModelKey, type ModelSelection, type ProviderState } from '../ipc/providers';

const isolatedMock: ProviderState = {
  settings: { revision: '0', active: localModel, favorites: [] },
  catalog: { models: [{ key: localModel, label: 'Local test model', providerLabel: 'Local', reasoningLevels: [], serviceTiers: [], contextWindowTokens: null, maxOutputTokens: null, origin: 'builtIn', ready: true, statusDetail: 'No live AI connected' }] },
  dispatch: { kind: 'localMock', detail: 'No live AI connected' },
};
interface ProviderContextValue {
  state: ProviderState | null; busy: boolean; error: string;
  refresh(): Promise<void>;
  save(active: ModelSelection, favorites: ModelKey[]): Promise<boolean>;
}
// Isolated editor/unit-test surfaces use the existing local mock contract.
// The production root always mounts ProviderSettingsProvider and loads Rust state.
const Providers = createContext<ProviderContextValue>({ state: isolatedMock, busy: false, error: '', refresh: async () => {}, save: async () => false });
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
  return <Providers.Provider value={{ state, busy, error, refresh, save }}>{children}</Providers.Provider>;
}
