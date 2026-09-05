import { createRoot } from 'react-dom/client';
import { Workspace } from './shell/Workspace';
import './shell/app.css';

createRoot(document.getElementById('root')!).render(<Workspace />);
