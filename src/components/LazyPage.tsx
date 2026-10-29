import React, { useEffect, useState } from 'react';
import type { RenderedPage } from '../types';
import { cachedRender, cancelLazy, renderLazy } from '../viewer/lazyRender';
import { CancelledRender } from '../viewer/renderQueue';

interface LazyPageProps {
  docId: string;
  page: number;
  renderWidth: number;
  thumb: boolean;
  revision: string;
  style?: React.CSSProperties;
}

/** Renders one page on mount (through the shared queue) and cancels if unmounted first. */
export const LazyPage: React.FC<LazyPageProps> = ({ docId, page, renderWidth, thumb, revision, style }) => {
  const [rendered, setRendered] = useState<RenderedPage | undefined>(() => cachedRender(docId, revision, page, renderWidth, thumb));
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setFailed(false);
    const hit = cachedRender(docId, revision, page, renderWidth, thumb);
    setRendered(hit);
    if (hit) return;
    renderLazy(docId, revision, page, renderWidth, thumb).then(
      (r) => alive && setRendered(r),
      (error) => alive && !(error instanceof CancelledRender) && setFailed(true),
    );
    return () => {
      alive = false;
      cancelLazy(docId, revision, page, renderWidth);
    };
  }, [docId, page, renderWidth, thumb, revision]);

  return (
    <div style={{ background: '#fff', position: 'relative', overflow: 'hidden', ...style }}>
      {rendered ? (
        <img src={rendered.dataUrl} alt={`Sayfa ${page}`} draggable={false}
          style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block', userSelect: 'none' }} />
      ) : (
        <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: thumb ? 10 : 12, color: failed ? 'var(--color-danger, #b42318)' : 'var(--color-text-muted)' }}>
          {failed ? 'Sayfa çizilemedi' : thumb ? page : 'Sayfa hazırlanıyor...'}
        </div>
      )}
    </div>
  );
};
