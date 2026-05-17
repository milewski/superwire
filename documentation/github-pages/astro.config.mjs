import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'astro/config';

const repositoryName = process.env.GITHUB_REPOSITORY?.split('/').at(1);
const basePath = process.env.GITHUB_ACTIONS && repositoryName ? `/${repositoryName}` : '/';

export default defineConfig({
  base: basePath,
  integrations: [react()],
  output: 'static',
  vite: {
    plugins: [tailwindcss()],
    server: {
      fs: {
        allow: ['..'],
      },
    },
  },
});
