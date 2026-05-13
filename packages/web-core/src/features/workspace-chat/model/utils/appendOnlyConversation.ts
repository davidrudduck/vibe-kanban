import type { PatchTypeWithKey } from '@/shared/hooks/useConversationHistory/types';

const TRANSIENT_PATCH_KEYS = new Set(['loading', 'next_action']);

export const isTransientConversationItem = (item: PatchTypeWithKey) =>
  TRANSIENT_PATCH_KEYS.has(item.patchKey) || item.patchKey.endsWith(':loading');

const serializeForRender = (value: unknown) =>
  JSON.stringify(value, (_key, itemValue) =>
    typeof itemValue === 'bigint' ? itemValue.toString() : itemValue
  );

const getConversationSignature = (item: PatchTypeWithKey) =>
  serializeForRender({
    ...item,
    patchKey: undefined,
  });

const getItemRenderSignature = (item: PatchTypeWithKey) =>
  serializeForRender(item);

const getPersistentItems = (items: PatchTypeWithKey[]) =>
  items.filter((item) => !isTransientConversationItem(item));

const findInsertionIndex = (
  items: PatchTypeWithKey[],
  nextKeys: Set<string>,
  nextItems: PatchTypeWithKey[],
  newItemIndex: number,
  itemIndexes: Map<string, number>
) => {
  for (let index = newItemIndex + 1; index < nextItems.length; index += 1) {
    const anchorKey = nextItems[index]?.patchKey;
    if (!anchorKey) continue;

    const anchorIndex = itemIndexes.get(anchorKey);
    if (anchorIndex !== undefined) {
      let insertionIndex = anchorIndex;
      while (
        insertionIndex > 0 &&
        !nextKeys.has(items[insertionIndex - 1]?.patchKey ?? '')
      ) {
        insertionIndex -= 1;
      }
      return insertionIndex;
    }
  }

  return items.length;
};

const rebuildItemIndexes = (
  items: PatchTypeWithKey[],
  itemIndexes: Map<string, number>,
  startIndex = 0
) => {
  for (let index = startIndex; index < items.length; index += 1) {
    const item = items[index];
    if (item) itemIndexes.set(item.patchKey, index);
  }
};

export const mergeAppendOnlyConversationItems = (
  previousItems: PatchTypeWithKey[],
  nextItems: PatchTypeWithKey[]
) => {
  const previousPersistentItems = getPersistentItems(previousItems);
  const nextPersistentItems = getPersistentItems(nextItems);
  const nextTransientItems = nextItems.filter(isTransientConversationItem);

  const nextKeys = new Set(nextPersistentItems.map((item) => item.patchKey));
  const includesAllPrevious = previousPersistentItems.every((item) =>
    nextKeys.has(item.patchKey)
  );

  if (includesAllPrevious) {
    return [...nextPersistentItems, ...nextTransientItems];
  }

  const mergedPersistentItems = [...previousPersistentItems];
  const mergedIndexes = new Map(
    mergedPersistentItems.map((item, index) => [item.patchKey, index])
  );

  nextPersistentItems.forEach((item, nextIndex) => {
    const existingIndex = mergedIndexes.get(item.patchKey);

    if (existingIndex !== undefined) {
      mergedPersistentItems[existingIndex] = item;
      return;
    }

    const insertionIndex = findInsertionIndex(
      mergedPersistentItems,
      nextKeys,
      nextPersistentItems,
      nextIndex,
      mergedIndexes
    );
    mergedPersistentItems.splice(insertionIndex, 0, item);
    rebuildItemIndexes(mergedPersistentItems, mergedIndexes, insertionIndex);
  });

  return [...mergedPersistentItems, ...nextTransientItems];
};

export interface RunningAppendOnlyConversationResult {
  acceptedSnapshot: boolean;
  items: PatchTypeWithKey[];
}

export const getRunningAppendOnlyConversationResult = (
  previousItems: PatchTypeWithKey[],
  nextItems: PatchTypeWithKey[],
  previousSnapshotItems: PatchTypeWithKey[] = previousItems
): RunningAppendOnlyConversationResult => {
  const previousPersistentItems = getPersistentItems(previousItems);
  const previousSnapshotPersistentItems = getPersistentItems(
    previousSnapshotItems
  );
  const nextPersistentItems = getPersistentItems(nextItems);
  const nextTransientItems = nextItems.filter(isTransientConversationItem);

  const isObviousStaleReplay =
    nextPersistentItems.length < previousSnapshotPersistentItems.length &&
    nextPersistentItems.every((item, index) => {
      const previousSnapshotItem = previousSnapshotPersistentItems[index];
      return (
        !!previousSnapshotItem &&
        getConversationSignature(previousSnapshotItem) ===
          getConversationSignature(item)
      );
    });

  if (isObviousStaleReplay) {
    return {
      acceptedSnapshot: false,
      items: [...previousPersistentItems, ...nextTransientItems],
    };
  }

  return {
    acceptedSnapshot: true,
    items: mergeAppendOnlyConversationItems(previousItems, nextItems),
  };
};

export const getConversationTailRenderSignature = (items: PatchTypeWithKey[]) =>
  items
    .slice(-2)
    .map((item) => `${item.patchKey}:${getItemRenderSignature(item)}`)
    .join('|');

export const shouldFollowConversationTail = ({
  wasAtBottom,
  previousItems,
  nextItems,
}: {
  wasAtBottom: boolean;
  previousItems: PatchTypeWithKey[];
  nextItems: PatchTypeWithKey[];
}) => {
  if (!wasAtBottom) return false;
  if (nextItems.length > previousItems.length) return true;
  return (
    getConversationTailRenderSignature(nextItems) !==
    getConversationTailRenderSignature(previousItems)
  );
};
