import { apiRenderPage } from '../bridge/tauri';
import type { RenderedPage } from '../types';
import { LruCache, RenderQueue } from './renderQueue';

/** Priorities: lower runs first. */
export const PRIORITY_MAIN = 0;
export const PRIORITY_THUMB = 10;

const queue = new RenderQueue<RenderedPage>(1);
// Thumbnails are small (~5-15 KB); full pages at zoom can be MBs, so keep few.
const thumbCache = new LruCache<RenderedPage>(400);
const pageCache = new LruCache<RenderedPage>(16);

export const renderKey = (docId: string, revision: string, page: number, width: number) => `${docId}_${revision}_${page}_${width}`;

export function cachedRender(docId: string, revision: string, page: number, width: number, thumb: boolean): RenderedPage | undefined {
  return (thumb ? thumbCache : pageCache).get(renderKey(docId, revision, page, width));
}

export function renderLazy(docId: string, revision: string, page: number, width: number, thumb: boolean): Promise<RenderedPage> {
  const key = renderKey(docId, revision, page, width);
  const cache = thumb ? thumbCache : pageCache;
  const hit = cache.get(key);
  if (hit) return Promise.resolve(hit);
  return queue
    .request({ key, priority: thumb ? PRIORITY_THUMB : PRIORITY_MAIN, run: () => apiRenderPage(docId, page, width) })
    .then((rendered) => {
      cache.set(key, rendered);
      return rendered;
    });
}

export function cancelLazy(docId: string, revision: string, page: number, width: number): void {
  queue.cancel(renderKey(docId, revision, page, width));
}

/** Drops all cached revisions when a document is closed or explicitly reopened. */
export function forgetDocument(docId: string): void {
  const mine = (key: string) => key.startsWith(`${docId}_`);
  thumbCache.deleteWhere(mine);
  pageCache.deleteWhere(mine);
}
