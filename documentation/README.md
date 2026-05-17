# Documentation workspace

This directory is split into two parts:

- `github-pages/`: marketing-style landing page for GitHub Pages with a link to `https://docs.superwire.dev`
- `docs/`: Mintlify documentation source for `docs.superwire.dev`

## Local development

Mintlify docs:

```bash
cd documentation/docs
npx mintlify dev
```

GitHub Pages landing page:

```bash
cd documentation/github-pages
npm install
npm run dev
```

Production build:

```bash
cd documentation/github-pages
npm run build
```
