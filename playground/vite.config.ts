import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';
import { defineConfig } from 'vite';

const executionProxyTimeoutMilliseconds = 20 * 60 * 1000;
const executorServerTarget = 'http://127.0.0.1:3000';

export default defineConfig({
  base: '/playground/',
  publicDir: '../documentation/docs/public',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    proxy: {
      '/execute': {
        target: executorServerTarget,
        timeout: executionProxyTimeoutMilliseconds,
        proxyTimeout: executionProxyTimeoutMilliseconds,
      },
      '/validate': executorServerTarget,
      '/graph': executorServerTarget,
      '/format': executorServerTarget,
      '/lsp': {
        target: 'ws://127.0.0.1:3000',
        ws: true,
      },
    },
  },
});
