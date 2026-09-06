import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const tauriDebug = process.env.TAURI_ENV_DEBUG === 'true';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  define: { __WNS_EDITOR_TRIAL__: JSON.stringify(tauriDebug) },
  build: { target: 'es2022' },
});
