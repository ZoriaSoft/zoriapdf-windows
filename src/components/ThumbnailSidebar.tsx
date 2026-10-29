import React, { useEffect, useMemo, useRef } from 'react';
import { useDocument } from '../context/DocumentContext';
import { pageAspect } from '../viewer/renderQueue';
import { useVirtualPages } from '../viewer/useVirtualPages';
import { LazyPage } from './LazyPage';
import { renderRevision } from '../viewer/documentState';

const THUMB_CSS_WIDTH = 112;
const LABEL_HEIGHT = 18;
const GAP = 10;

/** Lazily rendered page thumbnails; only the visible window is mounted and rendered. */
export const ThumbnailSidebar: React.FC = () => {
  const { activeTab, setCurrentPage } = useDocument();
  const scrollRef = useRef<HTMLDivElement>(null);
  const sizes = activeTab?.info.pageSizes;
  const pages = activeTab?.info.pages ?? 0;
  const imageHeights = useMemo(
    () => Array.from({ length: pages }, (_, i) => Math.round(THUMB_CSS_WIDTH * pageAspect(sizes, i + 1))),
    [pages, sizes],
  );
  const heights = useMemo(() => imageHeights.map((h) => h + LABEL_HEIGHT), [imageHeights]);
  const { offsets, total, range, viewport } = useVirtualPages(heights, GAP, scrollRef);
  const renderWidth = Math.max(100, Math.min(240, Math.round(THUMB_CSS_WIDTH * (window.devicePixelRatio || 1))));
  const current = activeTab?.currentPage ?? 1;

  // Keep the current page's thumbnail in view.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !pages) return;
    const top = offsets[current - 1] ?? 0;
    const bottom = top + (heights[current - 1] ?? 0);
    if (top < viewport.top || bottom > viewport.top + viewport.height) {
      el.scrollTop = Math.max(0, top - viewport.height / 3);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, activeTab?.info.id]);

  if (!activeTab) return null;
  const items = [];
  for (let i = range.first; i <= range.last && i < pages; i++) {
    const page = i + 1;
    const selected = page === current;
    const key = `${activeTab.info.id}-${renderRevision(activeTab)}-${page}`;
    items.push(
      <button key={key} type="button" onClick={() => setCurrentPage(page)} aria-label={`Sayfa ${page}`}
        aria-current={selected ? 'page' : undefined}
        style={{ position: 'absolute', top: offsets[i], left: '50%', transform: 'translateX(-50%)', width: THUMB_CSS_WIDTH + 8,
          padding: 4, border: 'none', background: 'transparent', cursor: 'pointer' }}>
        <LazyPage docId={activeTab.info.id} page={page} renderWidth={renderWidth} thumb revision={renderRevision(activeTab)}
          style={{ width: THUMB_CSS_WIDTH, height: imageHeights[i], borderRadius: 2,
            outline: selected ? '2px solid var(--color-accent, #2f6f5e)' : '1px solid var(--color-border)' }} />
        <div style={{ fontSize: 11, height: LABEL_HEIGHT - 4, marginTop: 4, fontWeight: selected ? 700 : 500,
          color: selected ? 'var(--color-text)' : 'var(--color-text-muted)' }}>{page}</div>
      </button>,
    );
  }
  return (
    <aside ref={scrollRef} aria-label="Sayfa küçük resimleri"
      style={{ width: 150, flexShrink: 0, overflowY: 'auto', overflowX: 'hidden', borderRight: '1px solid var(--color-border)',
        backgroundColor: 'var(--color-surface)', position: 'relative' }}>
      <div style={{ position: 'relative', height: total + 2 * GAP, marginTop: GAP }}>{items}</div>
    </aside>
  );
};
