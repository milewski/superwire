# Engine AI Documentation

This directory contains the complete Mintlify documentation for Engine AI.

## Structure

```
documentation/
├── mint.json                    # Mintlify configuration
├── introduction.mdx             # Introduction and overview
├── quickstart.mdx               # Quick start guide
├── installation.mdx             # Installation instructions
├── core-concepts/               # Core concepts documentation
│   ├── overview.mdx
│   ├── providers.mdx
│   ├── agents.mdx
│   ├── schemas.mdx
│   ├── workflows.mdx
│   └── dependencies.mdx
├── syntax/                      # Language syntax reference
│   ├── variables.mdx
│   ├── data-types.mdx
│   ├── operators.mdx
│   ├── control-flow.mdx
│   ├── string-interpolation.mdx
│   └── comments.mdx
├── advanced/                    # Advanced features
│   ├── parallel-execution.mdx
│   ├── context-sharing.mdx
│   ├── tool-calling.mdx
│   ├── error-handling.mdx
│   └── caching.mdx
├── examples/                    # Comprehensive examples
│   ├── basic-agent.mdx
│   ├── multi-agent.mdx
│   ├── structured-output.mdx
│   ├── parallel-processing.mdx
│   └── context-management.mdx
├── api-reference/               # API documentation
│   ├── rust-api.mdx
│   ├── javascript-api.mdx
│   └── cli.mdx
├── integrations/                # Provider integrations
│   ├── ollama.mdx
│   ├── openai.mdx
│   └── anthropic.mdx
└── guides/                      # How-to guides
    ├── building-workflows.mdx
    ├── testing.mdx
    ├── deployment.mdx
    └── best-practices.mdx
```

## Running Locally

### Prerequisites

- Node.js 16 or later
- npm or yarn

### Installation

```bash
cd documentation
npm install -g mintlify
```

### Development Server

```bash
mintlify dev
```

The documentation will be available at `http://localhost:3000`.

## Building for Production

```bash
mintlify build
```

## Deployment

The documentation can be deployed to:

- **Mintlify Cloud**: Push to GitHub and connect your repository
- **Vercel**: Deploy as a static site
- **Netlify**: Deploy as a static site
- **Self-hosted**: Build and serve the static files

### Mintlify Cloud Deployment

1. Push this repository to GitHub
2. Go to [Mintlify Dashboard](https://dashboard.mintlify.com)
3. Connect your repository
4. Configure the documentation path: `/documentation`
5. Deploy

## Documentation Guidelines

### Writing Style

- Use clear, concise language
- Include code examples for every concept
- Add comments to explain complex code
- Use callouts (Info, Warning, Tip) to highlight important information
- Keep examples practical and realistic

### Code Examples

- Always include complete, runnable examples
- Add comments explaining key parts
- Show expected output
- Include error cases when relevant

### Structure

- Start with simple concepts, build to complex
- Each page should be self-contained
- Link to related pages for deeper dives
- Use consistent formatting and terminology

## Contributing

To add or update documentation:

1. Create/edit `.mdx` files in the appropriate directory
2. Update `mint.json` navigation if adding new pages
3. Test locally with `mintlify dev`
4. Submit a pull request

## Components Available

Mintlify provides these components:

- `<Card>` - Clickable cards with icons
- `<CardGroup>` - Group multiple cards
- `<Accordion>` - Collapsible content sections
- `<AccordionGroup>` - Group multiple accordions
- `<Tabs>` - Tabbed content
- `<Tab>` - Individual tab
- `<CodeGroup>` - Group code blocks
- `<Info>` - Info callout
- `<Warning>` - Warning callout
- `<Tip>` - Tip callout
- `<Note>` - Note callout
- `<ParamField>` - API parameter documentation

## Customization

### Branding

Update `mint.json` to customize:

- Logo and favicon
- Color scheme
- Navigation structure
- Footer links
- Social media links

### Custom Components

Add custom React components in `/components` directory and import them in MDX files.

## Support

For questions or issues:

- GitHub Issues: https://github.com/yourusername/engine-ai/issues
- Documentation: https://docs.example.com
- Community: https://community.example.com
