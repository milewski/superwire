import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const documentationDirectory = resolve(scriptDirectory, '..');
const fixtureDirectory = join(documentationDirectory, 'examples', 'wire');
const superwireCliPath = process.env.SUPERWIRE_CLI ?? resolve(documentationDirectory, '..', '..', 'target', 'debug', 'superwire-cli');
const temporaryDirectory = mkdtempSync(join(tmpdir(), 'superwire-doc-fixtures-'));
let temporaryWorkflowIndex = 0;

function wireCodeBlocks(filePath) {
  const fileContents = readFileSync(filePath, 'utf8');

  return [...fileContents.matchAll(/```wire[^\n]*\n([\s\S]*?)```/g)].map((match) => match[1]);
}

function validateWorkflowSource(workflowSource, displayName) {
  temporaryWorkflowIndex += 1;

  const workflowFileName = `${String(temporaryWorkflowIndex).padStart(3, '0')}-${basename(displayName).replace(/\.mdx?$/, '')}.wire`;
  const workflowPath = join(temporaryDirectory, workflowFileName);
  writeFileSync(workflowPath, workflowSource);

  const validationResult = spawnSync(superwireCliPath, ['workflow', 'check', workflowPath], {
    encoding: 'utf8',
    stdio: 'pipe',
  });

  if (validationResult.status !== 0) {
    process.stderr.write(validationResult.stdout);
    process.stderr.write(validationResult.stderr);
    throw new Error(`workflow validation failed for ${displayName}`);
  }

  process.stdout.write(`checked ${displayName}\n`);
}

function documentationFilePaths(directoryPath) {
  const filePaths = [];

  for (const directoryEntry of readdirSync(directoryPath, { withFileTypes: true })) {
    if (directoryEntry.name === 'node_modules') {
      continue;
    }

    const entryPath = join(directoryPath, directoryEntry.name);

    if (directoryEntry.isDirectory()) {
      filePaths.push(...documentationFilePaths(entryPath));
    } else if (directoryEntry.name.endsWith('.mdx') || directoryEntry.name.endsWith('.md')) {
      filePaths.push(entryPath);
    }
  }

  return filePaths.sort();
}

try {
  const fixtureFileNames = readdirSync(fixtureDirectory)
    .filter((fileName) => fileName.endsWith('.mdx'))
    .sort();

  for (const fixtureFileName of fixtureFileNames) {
    const fixturePath = join(fixtureDirectory, fixtureFileName);
    const fixtureCodeBlocks = wireCodeBlocks(fixturePath);

    if (fixtureCodeBlocks.length !== 1) {
      throw new Error(`${fixturePath} must contain exactly one wire code block, found ${fixtureCodeBlocks.length}`);
    }

    validateWorkflowSource(fixtureCodeBlocks[0], `fixture ${fixtureFileName}`);
  }

  for (const documentationFilePath of documentationFilePaths(documentationDirectory)) {
    if (documentationFilePath.startsWith(`${fixtureDirectory}/`)) {
      continue;
    }

    const relativeDocumentationPath = relative(documentationDirectory, documentationFilePath);

    for (const [codeBlockIndex, workflowSource] of wireCodeBlocks(documentationFilePath).entries()) {
      if (workflowSource.includes('...')) {
        throw new Error(`${relativeDocumentationPath} wire block ${codeBlockIndex + 1} contains a placeholder ellipsis`);
      }

      const isCompleteWorkflow = /^provider\s+/m.test(workflowSource) && /^model\s+/m.test(workflowSource) && /^output\s*\{/m.test(workflowSource);

      if (isCompleteWorkflow) {
        validateWorkflowSource(workflowSource, `${relativeDocumentationPath} block ${codeBlockIndex + 1}`);
      }
    }
  }
} finally {
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
