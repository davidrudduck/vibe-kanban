import { AutoLinkPlugin as LexicalAutoLinkPlugin } from '@lexical/react/LexicalAutoLinkPlugin';
import { createLinkMatcherWithRegExp } from '@lexical/link';

// Matches https:// and www. URLs. http:// intentionally excluded — sanitizer
// only allows https:// and http:// links would render as styled-but-dead anchors.
// Character class capped at 2000 chars via quantifier.
const URL_REGEX =
  /\b(https:\/\/|www\.)[a-zA-Z0-9\-._~:/?#[\]@!$&'()*+,;=%]{1,2000}/;

// Trailing sentence punctuation that the regex consumes but shouldn't be part of the href.
// e.g. "See https://example.com." — strip the trailing period before linking.
const TRAILING_PUNCTUATION = /[.,;)]+$/;

const URL_MATCHER = createLinkMatcherWithRegExp(URL_REGEX, (text) => {
  const normalized = text.startsWith('http') ? text : `https://${text}`;
  return normalized.replace(TRAILING_PUNCTUATION, '');
});

const MATCHERS = [URL_MATCHER];

export function AutoLinkPlugin() {
  return <LexicalAutoLinkPlugin matchers={MATCHERS} />;
}
