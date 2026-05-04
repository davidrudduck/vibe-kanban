import { AutoLinkPlugin as LexicalAutoLinkPlugin } from '@lexical/react/LexicalAutoLinkPlugin';
import { createLinkMatcherWithRegExp } from '@lexical/link';

const URL_REGEX =
  /((https?:\/\/(www\.)?)|(www\.))[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)/;

const URL_MATCHER = createLinkMatcherWithRegExp(URL_REGEX, (text) => {
  return text.startsWith('http') ? text : `https://${text}`;
});

const MATCHERS = [URL_MATCHER];

/**
 * Auto-detects bare URLs in text and converts them to clickable AutoLinkNodes.
 * Works in both edit and read-only mode alongside ReadOnlyLinkPlugin.
 */
export function AutoLinkPlugin() {
  return <LexicalAutoLinkPlugin matchers={MATCHERS} />;
}
