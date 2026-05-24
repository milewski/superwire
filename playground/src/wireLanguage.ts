import { HighlightStyle, LanguageSupport, StreamLanguage, syntaxHighlighting, type StringStream } from '@codemirror/language';
import { tags } from '@lezer/highlight';
import textMateGrammarSource from '../../editors/textmate/syntaxes/wire.tmLanguage.json?raw';

type TextMatePattern = {
  match?: string;
};

type TextMateGrammar = {
  repository?: {
    'language-keywords'?: {
      patterns?: TextMatePattern[];
    };
    'type-keywords'?: {
      patterns?: TextMatePattern[];
    };
  };
};

function extractAlternativesFromWordRegex(regexText: string | undefined): string[] {
  if (!regexText) {
    return [];
  }

  const keywordGroupMatch = regexText.match(/\\b\(([^()]+)\)/);

  if (!keywordGroupMatch) {
    return [];
  }

  return keywordGroupMatch[1]
    .split('|')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

function createTokenSetsFromTextMateGrammar(): {
  keywords: Set<string>;
  types: Set<string>;
} {
  const textMateGrammar = JSON.parse(textMateGrammarSource) as TextMateGrammar;
  const languageKeywordPatterns = textMateGrammar.repository?.['language-keywords']?.patterns ?? [];
  const typeKeywordPatterns = textMateGrammar.repository?.['type-keywords']?.patterns ?? [];

  const keywords = new Set(extractAlternativesFromWordRegex(languageKeywordPatterns[0]?.match));
  const types = new Set(extractAlternativesFromWordRegex(typeKeywordPatterns[0]?.match));

  return { keywords, types };
}

const tokenSets = createTokenSetsFromTextMateGrammar();
const keywords = tokenSets.keywords;
const types = tokenSets.types;
const constants = new Set(['false', 'true']);

class WireStreamState {
  multilineString = false;
  quotedString = false;
  stringInterpolation = false;

  copy() {
    const state = new WireStreamState();
    state.multilineString = this.multilineString;
    state.quotedString = this.quotedString;
    state.stringInterpolation = this.stringInterpolation;

    return state;
  }

  enterMultilineString() {
    this.multilineString = true;
    this.stringInterpolation = false;
  }

  leaveMultilineString() {
    this.multilineString = false;
    this.stringInterpolation = false;
  }

  enterQuotedString() {
    this.quotedString = true;
    this.stringInterpolation = false;
  }

  leaveQuotedString() {
    this.quotedString = false;
    this.stringInterpolation = false;
  }

  enterStringInterpolation() {
    this.stringInterpolation = true;
  }

  leaveStringInterpolation() {
    this.stringInterpolation = false;
  }
}

const wireStreamLanguage = StreamLanguage.define({
  languageData: {
    commentTokens: { line: '//' },
  },

  startState() {
    return new WireStreamState();
  },

  copyState(state) {
    return state.copy();
  },

  token(stream, state) {
    if (state.stringInterpolation) {
      return tokenStringInterpolation(stream, state);
    }

    if (state.multilineString) {
      return tokenMultilineString(stream, state);
    }

    if (state.quotedString) {
      return tokenQuotedString(stream, state);
    }

    if (stream.match('//')) {
      stream.skipToEnd();

      return 'comment';
    }

    if (stream.match('"""')) {
      state.enterMultilineString();

      return 'string';
    }

    if (stream.match('"')) {
      state.enterQuotedString();

      return 'string';
    }

    return tokenDsl(stream);
  },
});

function tokenMultilineString(stream: StringStream, state: WireStreamState) {
  if (stream.match('"""')) {
    state.leaveMultilineString();

    return 'string';
  }

  if (stream.match('{{')) {
    state.enterStringInterpolation();

    return 'operator';
  }

  eatStringText(stream, ['{{', '"""']);

  return 'string';
}

function tokenStringInterpolation(stream: StringStream, state: WireStreamState) {
  if (stream.match('}}')) {
    state.leaveStringInterpolation();

    return 'operator';
  }

  return tokenDsl(stream);
}

function tokenQuotedString(stream: StringStream, state: WireStreamState) {
  if (stream.match('{{')) {
    state.enterStringInterpolation();

    return 'operator';
  }

  if (stream.match('"')) {
    state.leaveQuotedString();

    return 'string';
  }

  let escaping = false;

  while (!stream.eol()) {
    if (!escaping && stream.match('{{', false)) {
      return 'string';
    }

    const character = stream.next();

    if (character === '"' && !escaping) {
      state.leaveQuotedString();

      break;
    }

    escaping = character === '\\' && !escaping;
  }

  state.leaveQuotedString();

  return 'string';
}

function eatStringText(stream: StringStream, terminators: string[]) {
  while (!stream.eol()) {
    if (terminators.some((terminator) => stream.match(terminator, false))) {
      return;
    }

    stream.next();
  }
}

function tokenDsl(stream: StringStream) {
  if (stream.eatSpace()) {
    return null;
  }

  if (stream.match(/\d(?:[\d_]*\d)?(?:\.\d(?:[\d_]*\d)?)?/)) {
    return 'number';
  }

  if (stream.match(/[{}()[\],:;|]/)) {
    return 'punctuation';
  }

  if (stream.match(/\.\*{1,3}\.|\?\.|\?\?|\./)) {
    return 'operator';
  }

  if (stream.match(/#[A-Za-z_][A-Za-z0-9_]*/)) {
    return 'typeName';
  }

  const identifier = stream.match(/[A-Za-z_][A-Za-z0-9_]*/);

  if (identifier && typeof identifier !== 'boolean') {
    const value = identifier[0];

    if (value === '_') {
      return 'keyword';
    }

    const currentPosition = stream.pos;

    stream.eatSpace();

    const isPropertyAssignment = stream.peek() === ':';

    stream.pos = currentPosition;

    if (isPropertyAssignment) {
      return 'propertyName';
    }

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
}

const wireHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--syntax-keyword)', fontWeight: '650' },
  { tag: tags.typeName, color: 'var(--syntax-type)' },
  { tag: tags.propertyName, color: 'var(--syntax-property)' },
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
