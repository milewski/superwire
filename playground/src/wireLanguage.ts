import { HighlightStyle, LanguageSupport, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import { tags } from '@lezer/highlight';

const keywords = new Set([
  'agent',
  'as',
  'bindings',
  'call',
  'dynamic',
  'for',
  'from',
  'in',
  'input',
  'mcp',
  'model',
  'output',
  'prompt',
  'provider',
  'read',
  'render',
  'resource',
  'schema',
  'secrets',
  'tool',
]);

const types = new Set(['boolean', 'enum', 'float', 'maybe', 'null', 'number', 'string']);
const constants = new Set(['false', 'true']);

const wireStreamLanguage = StreamLanguage.define({
  token(stream) {
    if (stream.match('//')) {
      stream.skipToEnd();

      return 'comment';
    }

    if (stream.match('"""')) {
      while (!stream.eol()) {
        if (stream.match('"""')) {
          break;
        }

        stream.next();
      }

      return 'string';
    }

    if (stream.match('"')) {
      let escaping = false;

      while (!stream.eol()) {
        const character = stream.next();

        if (character === '"' && !escaping) {
          break;
        }

        escaping = character === '\\' && !escaping;
      }

      return 'string';
    }

    if (stream.eatSpace()) {
      return null;
    }

    if (stream.match(/\d(?:[\d_]*\d)?(?:\.\d(?:[\d_]*\d)?)?/)) {
      return 'number';
    }

    if (stream.match(/[{}()[\],:;|]/)) {
      return 'punctuation';
    }

    if (stream.match(/\?\.|\./)) {
      return 'operator';
    }

    const identifier = stream.match(/[A-Za-z_][A-Za-z0-9_]*/);

    if (identifier && typeof identifier !== 'boolean') {
      const value = identifier[0];

      if (keywords.has(value)) {
        return 'keyword';
      }

      if (types.has(value)) {
        return 'typeName';
      }

      if (constants.has(value)) {
        return 'bool';
      }

      return 'variableName';
    }

    stream.next();

    return null;
  },
});

const wireHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--syntax-keyword)', fontWeight: '650' },
  { tag: tags.typeName, color: 'var(--syntax-type)' },
  { tag: tags.variableName, color: 'var(--syntax-variable)' },
  { tag: tags.string, color: 'var(--syntax-string)' },
  { tag: tags.number, color: 'var(--syntax-number)' },
  { tag: tags.bool, color: 'var(--syntax-constant)' },
  { tag: tags.comment, color: 'var(--syntax-comment)', fontStyle: 'italic' },
  { tag: tags.operator, color: 'var(--syntax-operator)' },
  { tag: tags.punctuation, color: 'var(--syntax-punctuation)' },
]);

export function wireLanguage(): LanguageSupport {
  return new LanguageSupport(wireStreamLanguage, [syntaxHighlighting(wireHighlightStyle)]);
}
