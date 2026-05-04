import { AutoLinkPlugin as LexicalAutoLinkPlugin } from '@lexical/react/LexicalAutoLinkPlugin';
import { createLinkMatcherWithRegExp } from '@lexical/link';

const URL_REGEX =
  /\b(https:\/\/|www\.)[a-zA-Z0-9\-._~:/?#[\]@!$&'()*+,;=%]{1,2000}/;

const URL_MATCHER = createLinkMatcherWithRegExp(URL_REGEX, (text) => {
  if (text.length > 2083) return text;
  return text.startsWith('http') ? text : `https://${text}`;
});

const MATCHERS = [URL_MATCHER];

export function AutoLinkPlugin() {
  return <LexicalAutoLinkPlugin matchers={MATCHERS} />;
}
