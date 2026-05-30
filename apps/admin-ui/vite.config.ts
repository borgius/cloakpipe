/// <reference types="vitest" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The admin UI talks to a CloakPipe server-mode instance. In dev we proxy the
// backend namespaces to VITE_CLOAKPIPE_BASE_URL so the SPA can call relative URLs.
const backend = process.env.VITE_CLOAKPIPE_BASE_URL || 'http://127.0.0.1:8400';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5273,
    proxy: {
      '/admin': backend,
      '/v1': backend,
      '/tree': backend,
      '/sessions': backend,
      '/health': backend,
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: false,
  },
});
