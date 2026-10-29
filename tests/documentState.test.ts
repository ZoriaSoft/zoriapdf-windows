import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { DocumentInfo, TabState } from '../src/types.ts';
import { closeTabState, reopenTabState, shouldNavigateSearch } from '../src/viewer/documentState.ts';

function createMockDoc(id: string, name: string, revision = 'rev1', pages = 5): DocumentInfo {
  return {
    id,
    path: `C:\\docs\\${name}.pdf`,
    name: `${name}.pdf`,
    pages,
    size: 1024,
    revision,
  };
}

function createMockTab(id: string, name: string, revision = 'rev1', pages = 5): TabState {
  return {
    info: createMockDoc(id, name, revision, pages),
    renderGeneration: 0,
    currentPage: 1,
    zoom: 1.0,
    redactions: [],
    annotations: [],
  };
}

test('closeTabState removes by stable docId and maintains correct activeId', () => {
  const tabA = createMockTab('doc-a', 'DocA');
  const tabB = createMockTab('doc-b', 'DocB');
  const tabC = createMockTab('doc-c', 'DocC');
  const tabs = [tabA, tabB, tabC];

  // Active is A (index 0). Closing A activates B.
  const state1 = closeTabState(tabs, 'doc-a', 'doc-a');
  assert.equal(state1.tabs.length, 2);
  assert.equal(state1.tabs[0].info.id, 'doc-b');
  assert.equal(state1.tabs[1].info.id, 'doc-c');
  assert.equal(state1.activeId, 'doc-b');

  // Rapid close: now close B from the resulting state.
  const state2 = closeTabState(state1.tabs, state1.activeId, 'doc-b');
  assert.equal(state2.tabs.length, 1);
  assert.equal(state2.tabs[0].info.id, 'doc-c');
  assert.equal(state2.activeId, 'doc-c');

  // C is preserved (not incorrectly removed like in captured-index race).
});

test('closeTabState preserves activeId when a non-active tab is closed', () => {
  const tabA = createMockTab('doc-a', 'DocA');
  const tabB = createMockTab('doc-b', 'DocB');
  const tabC = createMockTab('doc-c', 'DocC');

  // Active is C. Closing A keeps C active.
  const state = closeTabState([tabA, tabB, tabC], 'doc-c', 'doc-a');
  assert.equal(state.tabs.length, 2);
  assert.equal(state.tabs[0].info.id, 'doc-b');
  assert.equal(state.tabs[1].info.id, 'doc-c');
  assert.equal(state.activeId, 'doc-c');
});

test('closeTabState handles closing the last remaining tab', () => {
  const tabA = createMockTab('doc-a', 'DocA');
  const state = closeTabState([tabA], 'doc-a', 'doc-a');
  assert.equal(state.tabs.length, 0);
  assert.equal(state.activeId, null);
});

test('closeTabState handles non-existent docId gracefully', () => {
  const tabA = createMockTab('doc-a', 'DocA');
  const state = closeTabState([tabA], 'doc-a', 'doc-unknown');
  assert.equal(state.tabs.length, 1);
  assert.equal(state.activeId, 'doc-a');
});

test('reopenTabState preserves drafts when source revision is unchanged', () => {
  const tabA = createMockTab('doc-a', 'DocA', 'rev-hash-1', 10);
  tabA.redactions = [{ page: 1, x: 0.1, y: 0.1, width: 0.2, height: 0.2 }];
  tabA.annotations = [{ page: 1, x: 0.2, y: 0.2, width: 0.1, height: 0.1, kind: 'highlight' }];
  tabA.currentPage = 4;

  const reopenedDoc = createMockDoc('doc-a', 'DocA', 'rev-hash-1', 10);
  const nextTabs = reopenTabState([tabA], [reopenedDoc]);

  assert.equal(nextTabs.length, 1);
  assert.equal(nextTabs[0].info.revision, 'rev-hash-1');
  assert.equal(nextTabs[0].currentPage, 4);
  assert.equal(nextTabs[0].redactions.length, 1);
  assert.equal(nextTabs[0].annotations.length, 1);
});

test('reopenTabState resets drafts when source revision changed', () => {
  const tabA = createMockTab('doc-a', 'DocA', 'rev-old', 10);
  tabA.redactions = [{ page: 1, x: 0.1, y: 0.1, width: 0.2, height: 0.2 }];
  tabA.annotations = [{ page: 1, x: 0.2, y: 0.2, width: 0.1, height: 0.1, kind: 'highlight' }];
  tabA.currentPage = 8;

  // New doc with different revision and fewer pages (e.g. 5)
  const reopenedDoc = createMockDoc('doc-a', 'DocA', 'rev-new', 5);
  const nextTabs = reopenTabState([tabA], [reopenedDoc]);

  assert.equal(nextTabs.length, 1);
  assert.equal(nextTabs[0].info.revision, 'rev-new');
  // Drafts must be reset
  assert.equal(nextTabs[0].redactions.length, 0);
  assert.equal(nextTabs[0].annotations.length, 0);
  // Current page must be clamped to new page count (5)
  assert.equal(nextTabs[0].currentPage, 5);
});

test('shouldNavigateSearch determines navigation vs initial search correctly', () => {
  // Empty query -> no navigation
  assert.equal(shouldNavigateSearch('', '', 0), false);
  assert.equal(shouldNavigateSearch('', 'test', 5), false);

  // Initial search (query entered but no hits yet or not searched yet)
  assert.equal(shouldNavigateSearch('hello', '', 0), false);
  assert.equal(shouldNavigateSearch('hello', '', 5), false);

  // Search performed and hits exist
  assert.equal(shouldNavigateSearch('hello', 'hello', 3), true);
  assert.equal(shouldNavigateSearch(' hello ', 'hello', 3), true);

  // Query modified by user -> must search instead of navigate
  assert.equal(shouldNavigateSearch('hello world', 'hello', 3), false);

  // Query matches but 0 hits -> must search
  assert.equal(shouldNavigateSearch('hello', 'hello', 0), false);
});
