// Lazy page rendering shared by the thumbnail sidebar and continuous scroll.
// The host serves renders from one reader process, so requests are serialized here,
// prioritized (visible main pages before thumbnails) and dropped when they scroll
// out of view before their turn comes.

export class LruCache<V> {
  private map = new Map<string, V>();
  private readonly capacity: number;
  constructor(capacity: number) {
    this.capacity = capacity;
  }

  get(key: string): V | undefined {
    const value = this.map.get(key);
    if (value !== undefined) {
      this.map.delete(key);
      this.map.set(key, value);
    }
    return value;
  }

  set(key: string, value: V): void {
    this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }

  deleteWhere(predicate: (key: string) => boolean): void {
    for (const key of [...this.map.keys()]) if (predicate(key)) this.map.delete(key);
  }

  get size(): number {
    return this.map.size;
  }
}

export interface RenderJob<T> {
  key: string;
  /** Lower runs first. */
  priority: number;
  run: () => Promise<T>;
}

interface Pending<T> extends RenderJob<T> {
  seq: number;
  waiters: Array<{ resolve: (v: T) => void; reject: (e: unknown) => void }>;
}

export class RenderQueue<T> {
  private pending = new Map<string, Pending<T>>();
  private running = 0;
  private seq = 0;

  private readonly concurrency: number;
  constructor(concurrency = 1) {
    this.concurrency = concurrency;
  }

  /** Queues a job; requesting the same key again shares the result and may raise its priority. */
  request(job: RenderJob<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const existing = this.pending.get(job.key);
      if (existing) {
        existing.priority = Math.min(existing.priority, job.priority);
        existing.waiters.push({ resolve, reject });
        return;
      }
      this.pending.set(job.key, { ...job, seq: this.seq++, waiters: [{ resolve, reject }] });
      this.pump();
    });
  }

  /** Drops a queued (not yet started) job, e.g. when its page leaves the viewport. */
  cancel(key: string): void {
    const job = this.pending.get(key);
    if (!job) return;
    this.pending.delete(key);
    for (const waiter of job.waiters) waiter.reject(new CancelledRender());
  }

  get queued(): number {
    return this.pending.size;
  }

  private next(): Pending<T> | undefined {
    let best: Pending<T> | undefined;
    for (const job of this.pending.values()) {
      if (!best || job.priority < best.priority || (job.priority === best.priority && job.seq < best.seq)) best = job;
    }
    return best;
  }

  private pump(): void {
    while (this.running < this.concurrency) {
      const job = this.next();
      if (!job) return;
      this.pending.delete(job.key);
      this.running++;
      job
        .run()
        .then(
          (value) => job.waiters.forEach((w) => w.resolve(value)),
          (error) => job.waiters.forEach((w) => w.reject(error)),
        )
        .finally(() => {
          this.running--;
          this.pump();
        });
    }
  }
}

export class CancelledRender extends Error {
  constructor() {
    super('render cancelled');
  }
}

/** Page width:height ratio from engine page sizes, defaulting to A4 portrait. */
export function pageAspect(sizes: Array<[number, number]> | undefined, page: number): number {
  const size = sizes?.[page - 1];
  return size && size[0] > 0 && size[1] > 0 ? size[1] / size[0] : 1.4142;
}

/** Finds the page whose box covers the most of the viewport band [top, bottom). */
export function mostVisiblePage(offsets: number[], heights: number[], top: number, bottom: number): number {
  let best = 1;
  let bestCover = -1;
  // Binary search the first page that ends after `top`, then scan forward.
  let lo = 0;
  let hi = offsets.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid] + heights[mid] <= top) lo = mid + 1;
    else hi = mid;
  }
  for (let i = lo; i < offsets.length && offsets[i] < bottom; i++) {
    const cover = Math.min(bottom, offsets[i] + heights[i]) - Math.max(top, offsets[i]);
    if (cover > bestCover) {
      bestCover = cover;
      best = i + 1;
    }
  }
  return best;
}

/** Windows canonical paths carry a `\\?\` prefix; hide it from people. */
export function displayPath(path: string): string {
  if (path.startsWith('\\\\?\\UNC\\')) return '\\\\' + path.slice(8);
  if (path.startsWith('\\\\?\\')) return path.slice(4);
  return path;
}

/** Most-recent-first list without duplicates (case-insensitive on Windows paths). */
export function pushRecent(list: string[], added: string[], limit = 10): string[] {
  const key = (p: string) => displayPath(p).toLowerCase();
  const seen = new Set<string>();
  const out: string[] = [];
  for (const path of [...added, ...list]) {
    const k = key(path);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(displayPath(path));
  }
  return out.slice(0, limit);
}
