import { useEffect, useRef, useState, type FormEvent } from 'react';
import { cancelEndpointDiscovery, discoverEndpointModels, readEndpointSettings, saveEndpointSettings, type EndpointProfile, type EndpointSettings as Settings, type SaveEndpoint } from '../ipc/providers';
import { useProviders } from './ProviderContext';
import './endpoints.css';

const describe = (error: unknown) => error && typeof error === 'object' && 'detail' in error ? String(error.detail) : 'Could not confirm the API connection. Reload saved connections to check it.';
export function EndpointSettings() {
  const providers = useProviders();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [editing, setEditing] = useState<EndpointProfile | 'new' | null>(null);
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const [notice, setNotice] = useState(''); const mounted = useRef(true); const flight = useRef(false);
  const discovery = useRef<string | null>(null); const [discovering, setDiscovering] = useState(false);
  async function reload() {
    if (flight.current) return;
    flight.current = true; setBusy(true);
    try { const value = await readEndpointSettings(); if (mounted.current) { setSettings(value); setError(''); } }
    catch (reason) { if (mounted.current) setError(describe(reason)); }
    finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  useEffect(() => { mounted.current = true; void reload(); return () => { mounted.current = false; if (discovery.current) void cancelEndpointDiscovery(discovery.current).catch(() => {}); }; }, []);
  async function save(request: SaveEndpoint) {
    if (flight.current) return false;
    flight.current = true; setBusy(true); setError(''); setNotice('');
    try {
      const value = await saveEndpointSettings(request);
      if (mounted.current) { setSettings(value); setEditing(null); setNotice('Connection saved. Choose its model from the model picker.'); }
      await providers.refresh(); return true;
    } catch (reason) {
      // Read after a possible lost save acknowledgement; never repeat a key write.
      try { const value = await readEndpointSettings(); if (mounted.current) setSettings(value); } catch { /* Retain the form for explicit recovery. */ }
      if (mounted.current) setError(describe(reason));
      await providers.refresh(); return false;
    } finally { flight.current = false; if (mounted.current) setBusy(false); }
  }
  async function discover(profile: EndpointProfile) {
    if (flight.current) return;
    flight.current = true; setBusy(true); setError(''); setNotice('');
    const id = crypto.randomUUID(); discovery.current = id; setDiscovering(true);
    try {
      const value = await discoverEndpointModels(profile.id, profile.configRevision, id);
      if (mounted.current) { setSettings(value); setNotice('Model list refreshed. You can choose a model from the picker.'); }
      await providers.refresh();
    } catch (reason) {
      if (mounted.current) {
        if (reason && typeof reason === 'object' && 'code' in reason && reason.code === 'DiscoveryCancelled') setNotice('Model search stopped. Your saved model list is unchanged.');
        else setError(`${describe(reason)} You can also enter a model ID in Edit connection.`);
      }
    }
    finally { discovery.current = null; flight.current = false; if (mounted.current) { setBusy(false); setDiscovering(false); } }
  }
  async function cancelDiscovery() {
    if (!discovery.current) return;
    setNotice('Stopping model search…');
    try { await cancelEndpointDiscovery(discovery.current); }
    catch (reason) { if (mounted.current) setError(describe(reason)); }
  }
  return <section className="endpoint-settings" aria-labelledby="api-connections-title">
    <div className="provider-connection-heading"><h3 id="api-connections-title">API connections</h3><button type="button" disabled={busy || !settings} onClick={() => { setEditing('new'); setNotice(''); }}>Add API connection</button></div>
    <p className="provider-note">Use an OpenAI-compatible service or a local model server. Your API keys stay in this computer’s credential store.</p>
    {settings?.profiles.length === 0 && !editing && <p className="provider-note">Add a base URL and a model ID to get started. An API key is optional for local servers.</p>}
    {settings && !editing && <ul className="endpoint-list">{settings.profiles.map(profile => <li key={profile.id}>
      <div><strong>{profile.label}</strong><span>{profile.baseUrl}</span><small>{profile.enabled ? 'Enabled' : 'Disabled'} · {profile.hasApiKey ? 'API key saved' : profile.apiKeyConfigured ? 'Saved key unavailable — enter it again or remove it' : 'No API key'} · {new Set([...profile.manualModelIds, ...profile.cachedModelIds]).size} models</small></div>
      <div className="endpoint-actions"><button type="button" disabled={busy} aria-label={`Edit connection ${profile.label}`} onClick={() => setEditing(profile)}>Edit</button><button type="button" disabled={busy || !profile.enabled} aria-label={`Find models for ${profile.label}`} onClick={() => void discover(profile)}>Find models</button></div>
    </li>)}</ul>}
    {settings && editing && <EndpointForm key={editing === 'new' ? 'new' : editing.id} profile={editing === 'new' ? null : editing} revision={settings.revision} busy={busy} onSave={save} onCancel={() => setEditing(null)} />}
    {busy && <p role="status" className="provider-note">Updating API connections…</p>}
    {discovering && <button type="button" onClick={() => void cancelDiscovery()}>Stop model search</button>}
    {error && <p role="alert" className="provider-error">{error}</p>}
    {notice && <p role="status" className="provider-note">{notice}</p>}
    {error && <button type="button" disabled={busy} onClick={() => void reload()}>Reload saved connections</button>}
  </section>;
}

function EndpointForm({ profile, revision, busy, onSave, onCancel }: { profile: EndpointProfile | null; revision: string; busy: boolean; onSave(request: SaveEndpoint): Promise<boolean>; onCancel(): void }) {
  const [label, setLabel] = useState(profile?.label ?? ''); const [url, setUrl] = useState(profile?.baseUrl ?? '');
  const [models, setModels] = useState(profile?.manualModelIds.join('\n') ?? ''); const [key, setKey] = useState('');
  const [removeKey, setRemoveKey] = useState(false); const [enabled, setEnabled] = useState(profile?.enabled ?? true);
  const [jsonMode, setJsonMode] = useState(profile?.jsonMode ?? false); const nameInput = useRef<HTMLInputElement>(null);
  useEffect(() => { nameInput.current?.focus(); }, []);
  async function submit(event: FormEvent) {
    event.preventDefault();
    const request: SaveEndpoint = { expectedRevision: revision, profileId: profile?.id ?? null,
      label: label.trim(), baseUrl: url.trim(), enabled, jsonMode,
      manualModelIds: [...new Set(models.split('\n').map(id => id.trim()).filter(Boolean))],
      apiKey: removeKey ? { kind: 'remove' } : key ? { kind: 'replace', value: key } : { kind: 'keep' } };
    setKey(''); await onSave(request);
  }
  return <form className="endpoint-form" onSubmit={event => void submit(event)}>
    <h4>{profile ? 'Edit connection' : 'New API connection'}</h4>
    <label htmlFor="endpoint-name">Connection name</label><input ref={nameInput} id="endpoint-name" value={label} onChange={e => setLabel(e.target.value)} required maxLength={128} disabled={busy} placeholder="Local writing model" />
    <label htmlFor="endpoint-url">Base URL</label><input id="endpoint-url" type="url" value={url} onChange={e => setUrl(e.target.value)} required maxLength={2048} disabled={busy} placeholder="http://localhost:1234/v1" />
    {url.startsWith('http:') && <p className="provider-note">This connection uses unencrypted HTTP.</p>}
    <label htmlFor="endpoint-key">API key {profile?.hasApiKey ? '(leave blank to keep saved key)' : '(optional)'}</label><input id="endpoint-key" type="password" value={key} onChange={e => { setKey(e.target.value); setRemoveKey(false); }} autoComplete="off" spellCheck={false} maxLength={5120} disabled={busy || removeKey} />
    {profile?.apiKeyConfigured && <label className="endpoint-check"><input type="checkbox" checked={removeKey} disabled={busy} onChange={e => { setRemoveKey(e.target.checked); setKey(''); }} />Remove saved API key</label>}
    <label htmlFor="endpoint-models">Model IDs <small>(one per line)</small></label><textarea id="endpoint-models" rows={3} value={models} onChange={e => setModels(e.target.value)} disabled={busy} placeholder="Enter the model name from your service" />
    <p className="provider-note">You can enter IDs now or use Find models after saving. Opening the model picker uses the saved list.</p>
    <label className="endpoint-check"><input type="checkbox" checked={enabled} disabled={busy} onChange={e => setEnabled(e.target.checked)} />Enable this connection</label>
    <label className="endpoint-check"><input type="checkbox" checked={jsonMode} disabled={busy} onChange={e => setJsonMode(e.target.checked)} />Request JSON mode for suggestions</label>
    <p className="provider-note">Enable JSON mode only if your service supports it. Suggestions are checked before they can be applied.</p>
    <div className="endpoint-actions"><button type="submit" disabled={busy}>Save connection</button><button type="button" disabled={busy} onClick={onCancel}>Cancel</button></div>
  </form>;
}
