import React, { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import { useDocument } from '../context/DocumentContext';
import { mostVisiblePage, pageAspect } from '../viewer/renderQueue';
import { useVirtualPages } from '../viewer/useVirtualPages';
import { LazyPage } from './LazyPage';
import { renderRevision } from '../viewer/documentState';

const GAP = 16;
const BASE_WIDTH = 720;

/** Continuous vertical scroll: windowed, pages render lazily as they approach the viewport. */
export const ContinuousView: React.FC = () => {
  const { activeTab, setCurrentPage } = useDocument();
  const scrollRef = useRef<HTMLDivElement>(null);
  const zoom = activeTab?.zoom ?? 1;
  const pages = activeTab?.info.pages ?? 0;
  const sizes = activeTab?.info.pageSizes;
  const cssWidth = Math.round(BASE_WIDTH * zoom);
  const renderWidth = Math.round(Math.min(2400, Math.max(400, 1080 * zoom)));
  const heights = useMemo(
    () => Array.from({ length: pages }, (_, i) => Math.round(cssWidth * pageAspect(sizes, i + 1))),
    [pages, sizes, cssWidth],
  );
  const { offsets, total, range } = useVirtualPages(heights, GAP, scrollRef);
  const reportedRef = useRef<number>(activeTab?.currentPage ?? 1);
  const pendingRestoreRef = useRef<number | null>(null);

  // Restore scroll position before reporting across doc/zoom transitions and on mount (e.g. returning from draw tool)
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || !pages) return;
    const targetPage = activeTab?.currentPage ?? 1;
    const targetOffset = offsets[targetPage - 1] ?? 0;
    el.scrollTop = targetOffset;
    reportedRef.current = targetPage;
    pendingRestoreRef.current = el.scrollTop;
  }, [activeTab?.info.id, activeTab?.renderGeneration, cssWidth, offsets, pages]);

  // Navigation from elsewhere (thumbnails, keys, search) scrolls to the page.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    const page = activeTab?.currentPage ?? 1;
    if (!el || !pages) return;
    if (page !== reportedRef.current) {
      reportedRef.current = page;
      el.scrollTop = offsets[page - 1] ?? 0;
      pendingRestoreRef.current = el.scrollTop;
    }
  }, [activeTab?.currentPage, offsets, pages]);

  // Observe actual scrolling, not the virtualizer's initial zero-height viewport.
  // A programmatic restore is acknowledged without replacing the selected page.
  useEffect(() => {
    const el = scrollRef.current;
    if (!pages || !el) return;
    const report = () => {
      const top = el.scrollTop;
      if (pendingRestoreRef.current !== null) {
        const restored = Math.abs(top - pendingRestoreRef.current) <= 1;
        pendingRestoreRef.current = null;
        if (restored) return;
      }
      if (el.clientHeight <= 0) return;
      const page = mostVisiblePage(offsets, heights, top, top + el.clientHeight);
      if (page !== reportedRef.current) {
        reportedRef.current = page;
        setCurrentPage(page);
      }
    };
    el.addEventListener('scroll', report, { passive: true });
    return () => el.removeEventListener('scroll', report);
  }, [offsets, heights, pages, setCurrentPage]);

  if (!activeTab) return null;
  const items = [];
  for (let i = range.first; i <= range.last && i < pages; i++) {
    items.push(
      <LazyPage
        key={`${activeTab.info.id}-${renderRevision(activeTab)}-${i + 1}`}
        docId={activeTab.info.id}
        page={i + 1}
        renderWidth={renderWidth}
        thumb={false}
        revision={renderRevision(activeTab)}
        style={{
          position: 'absolute',
          top: offsets[i],
          left: '50%',
          transform: 'translateX(-50%)',
          width: cssWidth,
          height: heights[i],
          boxShadow: 'var(--shadow-paper)',
          borderRadius: 2,
        }}
      />,
    );
  }
  return (
    <div ref={scrollRef} style={{ flex: 1, overflow: 'auto', padding: '24px 0', position: 'relative' }}>
      <div style={{ position: 'relative', height: total, minWidth: cssWidth + 48 }}>{items}</div>
    </div>
  );
};
