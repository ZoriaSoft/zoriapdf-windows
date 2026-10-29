import type { DocumentInfo, TabState } from '../types';

export const renderRevision = (tab: TabState): string => `${tab.info.revision}:${tab.renderGeneration}`;

/**
 * Pure state reducer for closing a document tab.
 * Uses stable document ID and atomically computes the new tab list and active ID,
 * preventing stale-index race conditions when closing multiple tabs rapidly.
 */
export function closeTabState(
  tabs: TabState[],
  activeId: string | null,
  docIdToClose: string
): { tabs: TabState[]; activeId: string | null } {
  const closeIdx = tabs.findIndex((t) => t.info.id === docIdToClose);
  if (closeIdx === -1) {
    return { tabs, activeId };
  }

  const nextTabs = tabs.filter((t) => t.info.id !== docIdToClose);
  let nextActiveId = activeId;

  if (activeId === docIdToClose) {
    if (nextTabs.length === 0) {
      nextActiveId = null;
    } else {
      const nextIdx = Math.min(closeIdx, nextTabs.length - 1);
      nextActiveId = nextTabs[nextIdx].info.id;
    }
  }

  return {
    tabs: nextTabs,
    activeId: nextActiveId,
  };
}

/**
 * Pure helper for merging reopened documents into the existing tab list.
 * If a document path is already open:
 * - If the source revision changed: drafts (redactions, annotations) are reset.
 * - If the source revision is unchanged: drafts are preserved.
 * - Current page is clamped to the new page count.
 */
export function reopenTabState(
  existingTabs: TabState[],
  newDocs: DocumentInfo[],
  password?: string
): TabState[] {
  const updatedTabs: TabState[] = [...existingTabs];

  for (const doc of newDocs) {
    const existingIndex = updatedTabs.findIndex((t) => t.info.path === doc.path);
    if (existingIndex >= 0) {
      const prevTab = updatedTabs[existingIndex];
      const revisionChanged = prevTab.info.revision !== doc.revision;

      updatedTabs[existingIndex] = {
        ...prevTab,
        info: doc,
        renderGeneration: prevTab.renderGeneration + 1,
        sourcePassword: password ?? prevTab.sourcePassword,
        currentPage: Math.max(1, Math.min(doc.pages, prevTab.currentPage)),
        redactions: revisionChanged ? [] : prevTab.redactions,
        annotations: revisionChanged ? [] : prevTab.annotations,
      };
    } else {
      updatedTabs.push({
        info: doc,
        renderGeneration: 0,
        currentPage: 1,
        zoom: 1.0,
        sourcePassword: password,
        redactions: [],
        annotations: [],
      });
    }
  }

  return updatedTabs;
}

/**
 * Pure helper for SearchDrawer keyboard handling.
 * Returns true if pressing Enter/Shift+Enter should cycle through existing hits,
 * or false if it should trigger an initial/new search.
 */
export function shouldNavigateSearch(
  currentQuery: string,
  lastSearchedQuery: string,
  hitCount: number
): boolean {
  const trimmed = currentQuery.trim();
  if (!trimmed || hitCount <= 0) return false;
  return trimmed === lastSearchedQuery.trim();
}
