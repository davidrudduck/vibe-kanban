import type { ScrollModifier } from '@virtuoso.dev/message-list';

export const INITIAL_TOP_ITEM = {
  index: 'LAST' as const,
  align: 'end' as const,
};

// Used on first data load (no purgeItemSizes — sizes are fresh, purging races measurement).
export const InitialDataScrollModifier: ScrollModifier = {
  type: 'item-location',
  location: INITIAL_TOP_ITEM,
};

// Used on subsequent streaming updates when the user is already at the bottom.
// Intentionally identical to InitialDataScrollModifier; kept separate for semantic clarity.
export const ScrollToBottomModifier: ScrollModifier = {
  type: 'item-location',
  location: INITIAL_TOP_ITEM,
};
