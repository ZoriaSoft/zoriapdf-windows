import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LruCache, RenderQueue, CancelledRender, mostVisiblePage, displayPath, pushRecent, pageAspect } from '../src/viewer/renderQueue.ts';

test('LRU evicts least recently used', () => {
  const c = new LruCache<number>(2);
  c.set('a', 1); c.set('b', 2); c.get('a'); c.set('c', 3);
  assert.equal(c.get('b'), undefined);
  assert.equal(c.get('a'), 1);
  assert.equal(c.size, 2);
});

test('queue runs by priority, dedupes and cancels', async () => {
  const q = new RenderQueue<string>(1);
  const order: string[] = [];
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  const job = (key: string, priority: number) => ({ key, priority, run: async () => { order.push(key); if (key === 'first') await gate; return key; } });
  const first = q.request(job('first', 5));
  const thumb = q.request(job('thumb', 10));
  const main = q.request(job('main', 0));
  const dup = q.request(job('main', 3));
  const dropped = q.request(job('gone', 1));
  q.cancel('gone');
  await assert.rejects(dropped, CancelledRender);
  release();
  assert.deepEqual(await Promise.all([first, thumb, main, dup]), ['first', 'thumb', 'main', 'main']);
  assert.deepEqual(order, ['first', 'main', 'thumb']);
});

test('most visible page and helpers', () => {
  const heights = [100, 100, 100, 100];
  const offsets = [0, 110, 220, 330];
  assert.equal(mostVisiblePage(offsets, heights, 0, 80), 1);
  assert.equal(mostVisiblePage(offsets, heights, 120, 260), 2);
  assert.equal(mostVisiblePage(offsets, heights, 240, 360), 3);
  assert.equal(displayPath('\\\\?\\C:\\Belgeler\\a.pdf'), 'C:\\Belgeler\\a.pdf');
  assert.deepEqual(pushRecent(['C:\\a.pdf', 'C:\\b.pdf'], ['\\\\?\\C:\\B.pdf'], 10), ['C:\\B.pdf', 'C:\\a.pdf']);
  assert.equal(pushRecent(Array.from({ length: 20 }, (_, i) => `${i}.pdf`), ['x.pdf']).length, 10);
  assert.deepEqual(pushRecent([], ['\\\\?\\C:\\a.pdf', 'C:\\A.pdf', 'C:\\b.pdf']), ['C:\\a.pdf', 'C:\\b.pdf']);
  assert.equal(pageAspect([[600, 300]], 1), 0.5);
  assert.equal(pageAspect(undefined, 3), 1.4142);
});
