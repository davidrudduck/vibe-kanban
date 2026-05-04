import { useEffect } from 'react';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { AutoLinkNode, LinkNode } from '@lexical/link';

/**
 * Sanitize href to block dangerous protocols.
 * Returns undefined if the href is blocked.
 */
function sanitizeHref(href?: string): string | undefined {
  if (typeof href !== 'string') return undefined;
  const trimmed = href.trim();
  // Block dangerous protocols
  if (/^(javascript|vbscript|data):/i.test(trimmed)) return undefined;
  // Allow anchors and common relative forms (but they'll be disabled)
  if (
    trimmed.startsWith('#') ||
    trimmed.startsWith('./') ||
    trimmed.startsWith('../') ||
    trimmed.startsWith('/')
  )
    return trimmed;
  // Allow only https
  if (/^https:\/\//i.test(trimmed)) return trimmed;
  // Block everything else by default
  return undefined;
}

/**
 * Check if href is an external HTTPS link.
 */
function isExternalHref(href?: string): boolean {
  if (!href) return false;
  return /^https:\/\//i.test(href);
}

/**
 * Plugin that handles link sanitization and security attributes in read-only mode.
 * - Blocks dangerous protocols (javascript:, vbscript:, data:)
 * - External HTTPS links: clickable with target="_blank" and rel="noopener noreferrer"
 * - Internal/relative links: rendered but not clickable
 */
export function ReadOnlyLinkPlugin() {
  const [editor] = useLexicalComposerContext();

  useEffect(() => {
    function applyLinkAttributes(dom: HTMLAnchorElement) {
      const href = dom.getAttribute('href');
      const safeHref = sanitizeHref(href ?? undefined);

      if (!safeHref) {
        dom.removeAttribute('href');
        dom.style.cursor = 'not-allowed';
        dom.style.pointerEvents = 'none';
        return;
      }

      const isExternal = isExternalHref(safeHref);

      if (isExternal) {
        dom.setAttribute('target', '_blank');
        dom.setAttribute('rel', 'noopener noreferrer');
        dom.onclick = (e) => e.stopPropagation();
      } else {
        dom.removeAttribute('href');
        dom.style.cursor = 'not-allowed';
        dom.style.pointerEvents = 'none';
        dom.setAttribute('role', 'link');
        dom.setAttribute('aria-disabled', 'true');
        dom.title = href ?? '';
      }
    }

    function handleMutations(
      mutations: Map<string, 'created' | 'updated' | 'destroyed'>
    ) {
      for (const [nodeKey, mutation] of mutations) {
        if (mutation === 'destroyed') continue;
        const dom = editor.getElementByKey(nodeKey);
        if (!dom || !(dom instanceof HTMLAnchorElement)) continue;
        applyLinkAttributes(dom);
      }
    }

    const unregisterLink = editor.registerMutationListener(
      LinkNode,
      handleMutations
    );
    const unregisterAutoLink = editor.registerMutationListener(
      AutoLinkNode,
      handleMutations
    );

    // Also handle existing links on mount by triggering a read
    editor.getEditorState().read(() => {
      const root = editor.getRootElement();
      if (!root) return;
      root.querySelectorAll('a').forEach((link) => {
        applyLinkAttributes(link as HTMLAnchorElement);
      });
    });

    return () => {
      unregisterLink();
      unregisterAutoLink();
    };
  }, [editor]);

  return null;
}
