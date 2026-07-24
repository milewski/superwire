import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const documentationDirectory = resolve(scriptDirectory, '..');
const repositoryDirectory = resolve(documentationDirectory, '..', '..');

function readRepositoryFile(relativePath) {
  return readFileSync(resolve(repositoryDirectory, relativePath), 'utf8');
}

function requireText(fileContents, expectedText, contractDescription) {
  if (!fileContents.includes(expectedText)) {
    throw new Error(`${contractDescription}: missing ${JSON.stringify(expectedText)}`);
  }
}

function rejectText(fileContents, rejectedText, contractDescription) {
  if (fileContents.includes(rejectedText)) {
    throw new Error(`${contractDescription}: stale text ${JSON.stringify(rejectedText)}`);
  }
}

function requireOccurrenceCount(fileContents, expectedText, expectedCount, contractDescription) {
  const actualCount = fileContents.split(expectedText).length - 1;

  if (actualCount !== expectedCount) {
    throw new Error(`${contractDescription}: expected ${expectedCount} occurrences of ${JSON.stringify(expectedText)}, found ${actualCount}`);
  }
}

function requirePattern(fileContents, expectedPattern, contractDescription) {
  if (!expectedPattern.test(fileContents)) {
    throw new Error(`${contractDescription}: required pattern ${expectedPattern} was not found`);
  }
}

const executorWorkflow = readRepositoryFile('.github/workflows/superwire-executor.yml');
const pagesWorkflow = readRepositoryFile('.github/workflows/github-pages.yml');
const repositoryIgnoreRules = readRepositoryFile('.gitignore');
const executorApiDocumentation = readRepositoryFile('documentation/docs/api-reference/executor-api.mdx');
const cachingDocumentation = readRepositoryFile('documentation/docs/advanced/caching.mdx');
const troubleshootingDocumentation = readRepositoryFile('documentation/docs/troubleshooting.mdx');
const laravelReadme = readRepositoryFile('integration/superwire-laravel/README.md');
const laravelGuide = readRepositoryFile('documentation/docs/integrations/laravel.mdx');
const documentationPackage = JSON.parse(readRepositoryFile('documentation/docs/package.json'));

requireText(
  executorWorkflow,
  "group: executor-docker-${{ github.event_name != 'pull_request' && github.ref == 'refs/heads/main' && 'publish-main' || github.run_id }}",
  'Docker publish concurrency group',
);
requireText(
  executorWorkflow,
  "cancel-in-progress: ${{ github.event_name != 'pull_request' && github.ref == 'refs/heads/main' }}",
  'Docker publish cancellation condition',
);
requireText(
  executorWorkflow,
  'tags: ${{ steps.pull-request-image.outputs.repository || steps.publish-image.outputs.repository }}:${{ github.sha }}-${{ matrix.arch_tag }}',
  'immutable architecture image tag',
);
requireText(
  executorWorkflow,
  '-t "${{ secrets.DOCKERHUB_USERNAME }}/${{ env.IMAGE_NAME }}:sha-${GITHUB_SHA::7}"',
  'immutable multi-architecture SHA tag',
);
requireOccurrenceCount(
  pagesWorkflow,
  "      - 'documentation/docs/public/**'",
  2,
  'GitHub Pages public-asset path filters',
);
requireText(
  repositoryIgnoreRules,
  '/editors/intellij/.intellijPlatform/',
  'IntelliJ generated cache ignore rule',
);
requireText(
  repositoryIgnoreRules,
  '/documentation/docs/node_modules',
  'documentation dependency ignore rule',
);

for (const [fileContents, contractDescription] of [
  [executorApiDocumentation, 'Executor API cache invalidation contract'],
  [cachingDocumentation, 'Caching guide invalidation contract'],
  [troubleshootingDocumentation, 'Troubleshooting invalidation contract'],
]) {
  requireText(fileContents, 'HTTP 503', contractDescription);
  requireText(fileContents, 'cache_unavailable', contractDescription);
  rejectText(fileContents, 'invalidation-driver failure returns `{ "purged_entries": 0 }`', contractDescription);
  rejectText(fileContents, 'invalidation-driver failure reports zero purged entries', contractDescription);
}

requireText(laravelReadme, 'return exactly two top-level fields', 'Laravel result serialization contract');
for (const [fileContents, contractDescription] of [
  [laravelReadme, 'Laravel package README result contract'],
  [laravelGuide, 'Laravel integration guide result contract'],
]) {
  requireText(fileContents, '$result->transportIdentity', contractDescription);
  requireText(fileContents, '$result->toSensitiveDebugArray()', contractDescription);
  requirePattern(fileContents, /bounded public(?: event)? history/, contractDescription);
  rejectText(fileContents, 'raw provider/tool/MCP event history', contractDescription);
  rejectText(fileContents, 'may contain provider, tool, or MCP arguments and results', contractDescription);
  rejectText(fileContents, '`history`: streamed event history', contractDescription);
  rejectText(fileContents, '`context`: submitted input and secrets', contractDescription);
}

if (documentationPackage.devDependencies?.mint !== '^4.2.734') {
  throw new Error('Mint dependency override contract changed');
}

if (documentationPackage.overrides?.['front-matter']?.['js-yaml'] !== '^3.15.0') {
  throw new Error('front-matter js-yaml override contract changed');
}

if (documentationPackage.overrides?.['js-yaml'] !== '^4.3.0') {
  throw new Error('top-level js-yaml override contract changed');
}

process.stdout.write('checked release workflow, generated-artifact, cache, Laravel, and dependency contracts\n');
