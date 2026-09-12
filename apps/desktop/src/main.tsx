import { createRoot } from 'react-dom/client';
import { Workspace } from './shell/Workspace';
import { ProviderSettingsProvider } from './providers/ProviderContext';
import './shell/app.css';

createRoot(document.getElementById('root')!).render(<ProviderSettingsProvider><Workspace /></ProviderSettingsProvider>);
