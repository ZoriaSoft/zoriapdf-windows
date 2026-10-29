import { useEffect, useMemo, useState, type RefObject } from 'react';

/**
 * Windowed layout for a vertical list of pages with known heights.
 * Only pages within one viewport above/below are mounted, so a 10 000-page
 * document costs a few dozen DOM nodes.
 */
export function useVirtualPages(heights: number[], gap: number, container: RefObject<HTMLDivElement | null>) {
  const offsets = useMemo(() => {
    const result = new Array<number>(heights.length);
    let y = 0;
    for (let i = 0; i < heights.length; i++) {
      result[i] = y;
      y += heights[i] + gap;
    }
    return result;
  }, [heights, gap]);
  const total = heights.length ? offsets[heights.length - 1] + heights[heights.length - 1] : 0;

  const [viewport, setViewport] = useState({ top: 0, height: 800 });
  useEffect(() => {
    const el = container.current;
    if (!el) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      setViewport({ top: el.scrollTop, height: el.clientHeight });
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    el.addEventListener('scroll', onScroll, { passive: true });
    const resize = new ResizeObserver(onScroll);
    resize.observe(el);
    return () => {
      el.removeEventListener('scroll', onScroll);
      resize.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [container]);

  const range = useMemo(() => {
    const lo = viewport.top - viewport.height;
    const hi = viewport.top + viewport.height * 2;
    let first = 0;
    let a = 0;
    let b = heights.length - 1;
    while (a <= b) {
      const mid = (a + b) >> 1;
      if (offsets[mid] + heights[mid] < lo) a = mid + 1;
      else {
        first = mid;
        b = mid - 1;
      }
    }
    let last = first;
    while (last + 1 < heights.length && offsets[last + 1] < hi) last++;
    return { first, last };
  }, [viewport, offsets, heights]);

  return { offsets, total, range, viewport };
}
