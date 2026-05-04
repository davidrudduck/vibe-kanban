import { AutoLinkPlugin as LexicalAutoLinkPlugin } from '@lexical/react/LexicalAutoLinkPlugin';
import type { LinkMatcher } from '@lexical/link';

// Matches https:// and www. URLs. http:// intentionally excluded — sanitizer
// only allows https:// and http:// links would render as styled-but-dead anchors.
// Character class capped at 2000 chars via quantifier.
const URL_REGEX =
  /\b(https:\/\/|www\.)[a-zA-Z0-9\-._~:/?#[\]@!$&'()*+,;=%]{1,2000}/;

// Sentence-ending punctuation the regex consumes but that shouldn't appear in
// the href or the visible anchor text. ] included for malformed bracket cases.
const TRAILING_PUNCTUATION = /[.,;)\]]+$/;

// Custom matcher (not createLinkMatcherWithRegExp) so we can control both
// `text` (displayed label) and `url` (href) independently.
// createLinkMatcherWithRegExp only transforms `url` — leaving trailing
// punctuation visibly underlined inside the anchor even when stripped from href.
const URL_MATCHER: LinkMatcher = (text: string) => {
  const match = URL_REGEX.exec(text);
  if (!match) return null;
  const raw = match[0];
  const normalized = raw.startsWith('http') ? raw : `https://${raw}`;
  const url = normalized.replace(TRAILING_PUNCTUATION, '');
  const displayText = raw.replace(TRAILING_PUNCTUATION, '');
  return {
    index: match.index,
    length: raw.length, // consume the full raw match so Lexical advances past any stripped chars
    text: displayText,
    url,
  };
};

const MATCHERS = [URL_MATCHER];

export function AutoLinkPlugin() {
  return <LexicalAutoLinkPlugin matchers={MATCHERS} />;
}
