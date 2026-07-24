import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('../../../', import.meta.url));
const textMateDirectory = path.join(repositoryRoot, 'editors', 'textmate');
const grammarPath = path.join(textMateDirectory, 'syntaxes', 'wire.tmLanguage.json');
const packagePath = path.join(textMateDirectory, 'package.json');
const keywordSourcePath = path.join(repositoryRoot, 'crates', 'superwire-types', 'src', 'ast', 'keywords.rs');
const parserGrammarPath = path.join(repositoryRoot, 'crates', 'superwire-dsl', 'src', 'dsl', 'grammar.pest');
const fixtureDirectory = path.join(repositoryRoot, 'crates', 'superwire-test-support', 'fixtures');

const grammar = JSON.parse(await readFile(grammarPath, 'utf8'));
const textMatePackage = JSON.parse(await readFile(packagePath, 'utf8'));
const keywordSource = await readFile(keywordSourcePath, 'utf8');
const parserGrammar = await readFile(parserGrammarPath, 'utf8');
const fixturePaths = await collectFilesWithExtension(fixtureDirectory, '.wire');
const fixtureSource = (
  await Promise.all(fixturePaths.map((fixturePath) => readFile(fixturePath, 'utf8')))
).join('\n');

validateBundleContract();
validateRepositoryIncludesAndPatterns();
validateCanonicalTokenPrecedence();
validateCanonicalAgentProperties();

console.log(
  `Validated ${grammarPath}: ${Object.keys(grammar.repository).length} repository rules, ${fixturePaths.length} canonical fixtures, and all local includes.`,
);

function validateBundleContract() {
  assert.equal(grammar.scopeName, 'source.wire', 'TextMate scope name changed');
  assert.deepEqual(grammar.fileTypes, ['wire'], 'TextMate file type contract changed');

  const contributedLanguage = textMatePackage.contributes?.languages?.[0];
  const contributedGrammar = textMatePackage.contributes?.grammars?.[0];

  assert.equal(contributedLanguage?.id, 'superwire', 'language ID changed');
  assert.equal(contributedGrammar?.language, 'superwire', 'grammar language ID changed');
  assert.equal(contributedGrammar?.scopeName, grammar.scopeName, 'package and grammar scopes differ');
  assert.equal(contributedGrammar?.path, './syntaxes/wire.tmLanguage.json', 'grammar resource path changed');

  const languageKeywordPattern = grammar.repository?.['language-keywords']?.patterns?.[0]?.match;
  const typeKeywordPattern = grammar.repository?.['type-keywords']?.patterns?.[0]?.match;

  assert.match(languageKeywordPattern ?? '', /\\b\(([^()]+)\)/, 'playground cannot extract language keywords');
  assert.match(typeKeywordPattern ?? '', /\\b\(([^()]+)\)/, 'playground cannot extract type keywords');
}

function validateRepositoryIncludesAndPatterns() {
  assert.ok(grammar.repository && typeof grammar.repository === 'object', 'grammar repository is missing');

  visitGrammarValue(grammar, 'grammar', (propertyName, propertyValue, propertyPath) => {
    if (propertyName === 'include') {
      assert.equal(typeof propertyValue, 'string', `${propertyPath} must be a string`);

      if (propertyValue.startsWith('#')) {
        const includedRuleName = propertyValue.slice(1);

        assert.ok(grammar.repository[includedRuleName], `${propertyPath} references missing rule ${propertyValue}`);
      } else {
        assert.ok(
          propertyValue === '$self' || propertyValue === '$base',
          `${propertyPath} uses unsupported external include ${propertyValue}`,
        );
      }
    }

    if (propertyName === 'match' || propertyName === 'begin' || propertyName === 'end' || propertyName === 'while') {
      assert.equal(typeof propertyValue, 'string', `${propertyPath} must be a string`);
      assert.doesNotThrow(() => new RegExp(propertyValue, 'u'), `${propertyPath} is not a valid regular expression`);
    }
  });
}

function validateCanonicalTokenPrecedence() {
  const canonicalLiteralKeywords = [
    ...extractPestRuleLiterals(parserGrammar, 'boolean_literal'),
    ...extractPestRuleLiterals(parserGrammar, 'null_literal'),
  ];
  const canonicalExpressionKeywords = extractRustAsStringValues(keywordSource, 'ExpressionKeyword');
  const canonicalAgentFileKeyword = extractPestRuleLiterals(parserGrammar, 'agent_file_property')[0];
  const expressionIncludes = grammar.repository.expressions.patterns
    .map((pattern) => pattern.include)
    .filter((includeName) => typeof includeName === 'string');
  const literalKeywordIndex = expressionIncludes.indexOf('#literal-keywords');
  const languageKeywordIndex = expressionIncludes.indexOf('#language-keywords');
  const referenceIndex = expressionIncludes.indexOf('#references');

  assert.ok(literalKeywordIndex >= 0 && literalKeywordIndex < referenceIndex, 'literal keywords must precede references');
  assert.ok(languageKeywordIndex >= 0 && languageKeywordIndex < referenceIndex, 'language keywords must precede references');

  for (const literalKeyword of canonicalLiteralKeywords) {
    assert.equal(
      firstMatchingExpressionInclude(literalKeyword),
      '#literal-keywords',
      `${literalKeyword} is shadowed by the generic reference fallback`,
    );
  }

  for (const expressionKeyword of canonicalExpressionKeywords) {
    assert.match(fixtureSource, wordPattern(expressionKeyword), `canonical fixtures do not exercise ${expressionKeyword}`);
    assert.equal(
      firstMatchingExpressionInclude(expressionKeyword),
      '#language-keywords',
      `${expressionKeyword} is shadowed by the generic reference fallback`,
    );
  }

  assert.ok(canonicalAgentFileKeyword, 'agent file keyword is missing from the parser grammar');
  assert.match(fixtureSource, wordPattern(canonicalAgentFileKeyword), `canonical fixtures do not exercise ${canonicalAgentFileKeyword}`);

  const languageKeywordPattern = grammar.repository['language-keywords'].patterns[0].match;
  const playgroundKeywords = extractWordRegexAlternatives(languageKeywordPattern);

  for (const expressionKeyword of canonicalExpressionKeywords) {
    assert.ok(playgroundKeywords.includes(expressionKeyword), `playground keyword extraction omits ${expressionKeyword}`);
  }

  assert.ok(playgroundKeywords.includes(canonicalAgentFileKeyword), `playground keyword extraction omits ${canonicalAgentFileKeyword}`);
}

function validateCanonicalAgentProperties() {
  const canonicalAgentPropertyNames = extractRustAsStringValues(keywordSource, 'AgentExpressionPropertyName');
  const agentPropertyPatterns = grammar.repository['agent-properties'].patterns;
  const canonicalAgentPropertyPattern = agentPropertyPatterns.find(
    (pattern) => pattern.name === 'entity.other.attribute-name.agent-property.wire',
  )?.match;
  const highlightedAgentPropertyNames = extractWordRegexAlternatives(canonicalAgentPropertyPattern);

  assert.deepEqual(
    [...highlightedAgentPropertyNames].sort(),
    [...canonicalAgentPropertyNames].sort(),
    'highlighted agent properties differ from AgentExpressionPropertyName',
  );

  for (const agentPropertyName of canonicalAgentPropertyNames) {
    assert.match(fixtureSource, new RegExp(`^\\s*${escapeRegularExpression(agentPropertyName)}\\s*:`, 'm'), `canonical fixtures do not exercise ${agentPropertyName}`);
  }

  for (const staleAgentPropertyName of ['prompt', 'tools', 'inference']) {
    assert.ok(
      !highlightedAgentPropertyNames.includes(staleAgentPropertyName),
      `stale agent property ${staleAgentPropertyName} is still highlighted as canonical`,
    );
  }

  const canonicalAgentFileKeyword = extractPestRuleLiterals(parserGrammar, 'agent_file_property')[0];
  const canonicalAgentOutputKeyword = extractPestRuleLiterals(parserGrammar, 'agent_output_property')[0];
  const fileDirectivePattern = agentPropertyPatterns.find(
    (pattern) => pattern.name === 'keyword.control.directive.file.wire',
  )?.match;
  const outputBlockPattern = grammar.repository['agent-output-block'].begin;

  assert.ok(new RegExp(fileDirectivePattern, 'u').test(`${canonicalAgentFileKeyword} agent.example`), 'file directive is not highlighted');
  assert.ok(new RegExp(outputBlockPattern, 'u').test(`${canonicalAgentOutputKeyword} {`), 'output directive is not highlighted');
  assert.match(fixtureSource, wordPattern(canonicalAgentOutputKeyword), `canonical fixtures do not exercise ${canonicalAgentOutputKeyword}`);
}

function firstMatchingExpressionInclude(token) {
  for (const pattern of grammar.repository.expressions.patterns) {
    const includeName = pattern.include;

    if (typeof includeName !== 'string' || !includeName.startsWith('#')) {
      continue;
    }

    if (repositoryRuleMatchesWholeToken(includeName.slice(1), token, new Set())) {
      return includeName;
    }
  }

  return null;
}

function repositoryRuleMatchesWholeToken(ruleName, token, visitedRuleNames) {
  if (visitedRuleNames.has(ruleName)) {
    return false;
  }

  const nextVisitedRuleNames = new Set(visitedRuleNames);
  nextVisitedRuleNames.add(ruleName);
  const repositoryRule = grammar.repository[ruleName];

  if (typeof repositoryRule.begin === 'string') {
    const beginMatch = new RegExp(repositoryRule.begin, 'u').exec(token);

    return Boolean(beginMatch && beginMatch.index === 0 && beginMatch[0].length === token.length);
  }

  if (typeof repositoryRule.match === 'string') {
    const directMatch = new RegExp(repositoryRule.match, 'u').exec(token);

    return Boolean(directMatch && directMatch.index === 0 && directMatch[0].length === token.length);
  }

  const patterns = repositoryRule.patterns ?? [repositoryRule];

  for (const pattern of patterns) {
    if (typeof pattern.match === 'string') {
      const match = new RegExp(pattern.match, 'u').exec(token);

      if (match && match.index === 0 && match[0].length === token.length) {
        return true;
      }
    }

    if (typeof pattern.include === 'string' && pattern.include.startsWith('#')) {
      if (repositoryRuleMatchesWholeToken(pattern.include.slice(1), token, nextVisitedRuleNames)) {
        return true;
      }
    }
  }

  return false;
}

function extractRustAsStringValues(source, typeName) {
  const implementationMarker = `impl ${typeName}`;
  const implementationStart = source.indexOf(implementationMarker);

  assert.ok(implementationStart >= 0, `missing ${implementationMarker}`);

  const implementationBlock = extractBraceBlock(source, source.indexOf('{', implementationStart));
  const methodStart = implementationBlock.indexOf('pub fn as_str');

  assert.ok(methodStart >= 0, `missing ${typeName}.as_str`);

  const methodBlock = extractBraceBlock(implementationBlock, implementationBlock.indexOf('{', methodStart));
  const stringValues = [...methodBlock.matchAll(/Self::[A-Za-z0-9_]+\s*=>\s*"([^"]+)"/g)]
    .map((match) => match[1]);

  assert.ok(stringValues.length > 0, `${typeName}.as_str has no string values`);

  return stringValues;
}

function extractPestRuleLiterals(source, ruleName) {
  const rulePattern = new RegExp(`^${escapeRegularExpression(ruleName)}\\s*=\\s*\\{`, 'm');
  const ruleMatch = rulePattern.exec(source);

  assert.ok(ruleMatch, `missing parser rule ${ruleName}`);

  const openingBraceIndex = source.indexOf('{', ruleMatch.index);
  const ruleBlock = extractBraceBlock(source, openingBraceIndex);
  const literalValues = [...ruleBlock.matchAll(/"([^"]+)"/g)].map((match) => match[1]);

  assert.ok(literalValues.length > 0, `${ruleName} has no literal values`);

  return literalValues;
}

function extractBraceBlock(source, openingBraceIndex) {
  assert.ok(openingBraceIndex >= 0 && source[openingBraceIndex] === '{', 'brace block does not start with an opening brace');

  let braceDepth = 0;
  let insideString = false;
  let escaping = false;

  for (let characterIndex = openingBraceIndex; characterIndex < source.length; characterIndex += 1) {
    const character = source[characterIndex];

    if (insideString) {
      if (!escaping && character === '"') {
        insideString = false;
      }

      escaping = character === '\\' && !escaping;
      continue;
    }

    if (character === '"') {
      insideString = true;
      escaping = false;
      continue;
    }

    if (character === '{') {
      braceDepth += 1;
    } else if (character === '}') {
      braceDepth -= 1;

      if (braceDepth === 0) {
        return source.slice(openingBraceIndex, characterIndex + 1);
      }
    }
  }

  assert.fail('unterminated brace block');
}

function extractWordRegexAlternatives(regexText) {
  assert.equal(typeof regexText, 'string', 'word regex is missing');

  const alternativeMatch = regexText.match(/\\b\(([^()]+)\)/);

  assert.ok(alternativeMatch, `cannot extract alternatives from ${regexText}`);

  return alternativeMatch[1]
    .split('|')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

async function collectFilesWithExtension(directoryPath, extension) {
  const directoryEntries = await readdir(directoryPath, { withFileTypes: true });
  const collectedPaths = [];

  for (const directoryEntry of directoryEntries.sort((leftEntry, rightEntry) => leftEntry.name.localeCompare(rightEntry.name))) {
    const entryPath = path.join(directoryPath, directoryEntry.name);

    if (directoryEntry.isDirectory()) {
      collectedPaths.push(...await collectFilesWithExtension(entryPath, extension));
    } else if (directoryEntry.isFile() && directoryEntry.name.endsWith(extension)) {
      collectedPaths.push(entryPath);
    }
  }

  return collectedPaths;
}

function visitGrammarValue(value, valuePath, visitor) {
  if (Array.isArray(value)) {
    value.forEach((arrayValue, arrayIndex) => visitGrammarValue(arrayValue, `${valuePath}[${arrayIndex}]`, visitor));
    return;
  }

  if (value === null || typeof value !== 'object') {
    return;
  }

  for (const [propertyName, propertyValue] of Object.entries(value)) {
    const propertyPath = `${valuePath}.${propertyName}`;
    visitor(propertyName, propertyValue, propertyPath);
    visitGrammarValue(propertyValue, propertyPath, visitor);
  }
}

function wordPattern(word) {
  return new RegExp(`\\b${escapeRegularExpression(word)}\\b`);
}

function escapeRegularExpression(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
