import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  base: '/',
  publicDir: '../documentation/public',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    proxy: {
      '/execute': 'http://127.0.0.1:3000',
      '/validate': 'http://127.0.0.1:3000',
      '/format': 'http://127.0.0.1:3000',
      '/lsp': {
        target: 'ws://127.0.0.1:3000',
        ws: true,
      },
    },
  },
});
