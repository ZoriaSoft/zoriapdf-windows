import React, { useRef, useState, useEffect, useLayoutEffect } from 'react';
import {
  ChevronLeft,
  ChevronRight,
  ZoomIn,
  ZoomOut,
  Maximize2,
  Copy,
  Check,
  FileText,
  PanelLeft,
  Rows3,
} from 'lucide-react';
import { ThumbnailSidebar } from './ThumbnailSidebar';
import { ContinuousView } from './ContinuousView';
import { useOperation } from '../context/OperationContext';
import { useDocument } from '../context/DocumentContext';
import { CanvasOverlay } from './CanvasOverlay';
import type { AnnotationKind } from '../types';

interface ViewerProps {
  annotationKind?: AnnotationKind;
  annotationText?: string;
  signatureImagePath?: string;
}

export const Viewer: React.FC<ViewerProps> = ({
  annotationKind,
  annotationText,
  signatureImagePath,
}) => {
  const {
    activeTab,
    activeRenderedPage,
    isRendering,
    renderError,
    retryRender,
    setCurrentPage,
    nextPage,
    prevPage,
    zoomIn,
    zoomOut,
    resetZoom,
    setZoom,
    fetchPageText,
    passwordPrompt,
  } = useDocument();

  const [pageInput, setPageInput] = useState<string>('1');
  const [textModal, setTextModal] = useState<{ docId: string; page: number; text: string } | null>(null);
  const [isCopied, setIsCopied] = useState<boolean>(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const inspectButtonRef = useRef<HTMLButtonElement | null>(null);
  const modalRef = useRef<HTMLDivElement | null>(null);
  const inspectSeqRef = useRef<number>(0);
  const inspectKey = activeTab ? `${activeTab.info.id}:${activeTab.renderGeneration}:${activeTab.currentPage}` : '';
  const currentInspectKeyRef = useRef(inspectKey);
  currentInspectKeyRef.current = inspectKey;

  const { activeTool } = useOperation();
  const [showThumbs, setShowThumbs] = useState<boolean>(() => localStorage.getItem('zoriapdf_thumbs') !== '0');
  const [continuousPref, setContinuousPref] = useState<boolean>(() => localStorage.getItem('zoriapdf_continuous') === '1');
  // Drawing tools need the single-page canvas overlay.
  const drawingTool = activeTool === 'annotate' || activeTool === 'redact';
  const continuous = continuousPref && !drawingTool;
  const toggleThumbs = () => setShowThumbs((v) => { localStorage.setItem('zoriapdf_thumbs', v ? '0' : '1'); return !v; });
  const toggleContinuous = () => setContinuousPref((v) => { localStorage.setItem('zoriapdf_continuous', v ? '0' : '1'); return !v; });

  useEffect(() => {
    if (activeTab) {
      setPageInput(String(activeTab.currentPage));
    }
  }, [activeTab?.currentPage]);

  useLayoutEffect(() => {
    ++inspectSeqRef.current;
    setTextModal(null);
    return () => { ++inspectSeqRef.current; };
  }, [inspectKey]);

  const handlePageSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const val = parseInt(pageInput, 10);
    if (!isNaN(val) && activeTab) {
      setCurrentPage(val);
    }
  };

  const handleInspectText = async () => {
    if (!activeTab) return;
    const snapDocId = activeTab.info.id;
    const snapPage = activeTab.currentPage;
    const snapshotKey = currentInspectKeyRef.current;
    const seq = ++inspectSeqRef.current;
    try {
      const text = await fetchPageText(snapPage);
      if (seq !== inspectSeqRef.current || snapshotKey !== currentInspectKeyRef.current) return;
      setTextModal({
        docId: snapDocId,
        page: snapPage,
        text: text || '(Bu sayfada metin katmanı bulunamadı)',
      });
    } catch {
      if (seq !== inspectSeqRef.current || snapshotKey !== currentInspectKeyRef.current) return;
      setTextModal({
        docId: snapDocId,
        page: snapPage,
        text: '(Metin katmanı alınırken bir hata oluştu)',
      });
    }
  };

  const closeTextModal = () => {
    ++inspectSeqRef.current;
    setTextModal(null);
  };

  useEffect(() => {
    if (!textModal || passwordPrompt.isOpen) return;
    const modalEl = modalRef.current;
    const returnFocus = inspectButtonRef.current;
    if (modalEl) {
      const focusable = modalEl.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      if (focusable.length > 0) focusable[0].focus();
    }

    const handleModalKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        closeTextModal();
        return;
      }
      if (e.key === 'Tab' && modalEl) {
        const focusable = modalEl.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey) {
          if (document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    };

    window.addEventListener('keydown', handleModalKeyDown, true);
    return () => {
      window.removeEventListener('keydown', handleModalKeyDown, true);
      if (returnFocus?.isConnected && !document.querySelector('[aria-modal="true"]')) returnFocus.focus();
    };
  }, [textModal, passwordPrompt.isOpen]);

  const handleCopyText = () => {
    if (!textModal?.text) return;
    navigator.clipboard.writeText(textModal.text);
    setIsCopied(true);
    setTimeout(() => setIsCopied(false), 2000);
  };

  const handleFitWidth = () => {
    if (!containerRef.current || !activeRenderedPage) return;
    const containerWidth = containerRef.current.clientWidth - 48;
    setZoom(Math.max(0.4, Math.min(3, containerWidth / 720)));
  };

  if (!activeTab) return null;

  const curZoom = activeTab.zoom;
  const renderedWidth = 720 * curZoom;
  const renderedHeight = activeRenderedPage
    ? renderedWidth * activeRenderedPage.height / activeRenderedPage.width
    : 1000 * curZoom;

  return (
    <div
      id="document-panel"
      role="tabpanel"
      aria-labelledby={`document-tab-${activeTab.info.id}`}
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        backgroundColor: 'var(--color-bg)',
        overflow: 'hidden',
      }}
    >
      {/* Top Document Control Bar */}
      <div
        style={{
          height: '42px',
          backgroundColor: 'var(--color-surface)',
          borderBottom: '1px solid var(--color-border)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '0 14px',
          gap: '12px',
          flexShrink: 0,
        }}
      >
        {/* Page navigation */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <button type="button" onClick={toggleThumbs} className="btn btn-ghost" aria-pressed={showThumbs}
            style={{ padding: '4px', height: '28px', opacity: showThumbs ? 1 : 0.6 }} title="Küçük Resimler" aria-label="Küçük resimleri göster/gizle">
            <PanelLeft size={15} />
          </button>
          <button type="button" onClick={toggleContinuous} className="btn btn-ghost" aria-pressed={continuous}
            disabled={drawingTool}
            style={{ padding: '4px', height: '28px', opacity: continuous ? 1 : 0.6 }}
            title={drawingTool ? 'İşaretleme ve karartma tek sayfa görünümünde yapılır' : 'Sürekli Kaydırma'}
            aria-label="Sürekli kaydırma">
            <Rows3 size={15} />
          </button>
          <button
            type="button"
            onClick={prevPage}
            disabled={activeTab.currentPage <= 1}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Önceki Sayfa (Sol Ok)"
          >
            <ChevronLeft size={16} />
          </button>

          <form
            onSubmit={handlePageSubmit}
            style={{ display: 'flex', alignItems: 'center', gap: '4px' }}
          >
            <input
              type="text"
              value={pageInput}
              onChange={(e) => setPageInput(e.target.value)}
              onBlur={handlePageSubmit}
              style={{
                width: '42px',
                height: '26px',
                textAlign: 'center',
                backgroundColor: 'var(--color-bg)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-xs)',
                fontSize: '12px',
                fontWeight: 600,
                color: 'var(--color-text)',
              }}
            />
            <span
              style={{
                fontSize: '12px',
                color: 'var(--color-text-muted)',
                userSelect: 'none',
              }}
            >
              / {activeTab.info.pages}
            </span>
          </form>

          <button
            type="button"
            onClick={nextPage}
            disabled={activeTab.currentPage >= activeTab.info.pages}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Sonraki Sayfa (Sağ Ok)"
          >
            <ChevronRight size={16} />
          </button>
        </div>

        {/* Zoom & Viewport controls */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
          <button
            type="button"
            onClick={zoomOut}
            disabled={curZoom <= 0.4}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Uzaklaş"
          >
            <ZoomOut size={15} />
          </button>

          <button
            type="button"
            onClick={resetZoom}
            className="btn btn-ghost"
            style={{
              padding: '2px 6px',
              height: '26px',
              fontSize: '11.5px',
              fontWeight: 600,
              minWidth: '48px',
            }}
            title="100% Varsayılan Boyut"
          >
            {Math.round(curZoom * 100)}%
          </button>

          <button
            type="button"
            onClick={zoomIn}
            disabled={curZoom >= 3.0}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Yakınlaş"
          >
            <ZoomIn size={15} />
          </button>

          <button
            type="button"
            onClick={handleFitWidth}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Genişliğe Sığdır"
          >
            <Maximize2 size={14} />
          </button>

          <div
            style={{
              width: '1px',
              height: '16px',
              backgroundColor: 'var(--color-border)',
              margin: '0 4px',
            }}
          />


          <button
            ref={inspectButtonRef}
            type="button"
            onClick={handleInspectText}
            className="btn btn-ghost"
            style={{ padding: '4px', height: '28px' }}
            title="Sayfa Metnini İncele"
            aria-label="Sayfa metnini incele"
          >
            <FileText size={14} />
          </button>
        </div>
      </div>

      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
      {showThumbs && <ThumbnailSidebar />}
      {continuous ? <ContinuousView /> : (
      /* Main Canvas Scroll Area */
      <div
        ref={containerRef}
        style={{
          flex: 1,
          overflow: 'auto',
          display: 'flex',
          alignItems: 'flex-start',
          padding: '24px',
          position: 'relative',
        }}
      >
        <div
          style={{
            position: 'relative',
            width: `${renderedWidth}px`,
            height: `${renderedHeight}px`,
            backgroundColor: '#FFFFFF',
            boxShadow: 'var(--shadow-paper)',
            borderRadius: '2px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            margin: '0 auto',
          }}
        >
          {activeRenderedPage ? (
            <>
              <img
                src={activeRenderedPage.dataUrl}
                alt={`Sayfa ${activeTab.currentPage}`}
                style={{
                  width: '100%',
                  height: '100%',
                  objectFit: 'contain',
                  display: 'block',
                  userSelect: 'none',
                  pointerEvents: 'none',
                }}
              />
              <CanvasOverlay
                key={`${activeTab.info.id}-${activeTab.renderGeneration}-${activeTab.currentPage}`}
                pageWidth={renderedWidth}
                pageHeight={renderedHeight}
                annotationKind={annotationKind}
                annotationText={annotationText}
                signatureImagePath={signatureImagePath}
              />
            </>
          ) : (
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: '8px',
                color: 'var(--color-text-muted)',
              }}
            >
              {renderError ? (
                <div role="alert" style={{ padding: 24, maxWidth: 440, textAlign: 'center' }}>
                  <p>{renderError}</p>
                  <button className="btn btn-primary" onClick={retryRender}>Tekrar dene</button>
                </div>
              ) : <span style={{ fontSize: '12px' }}>Sayfa hazırlanıyor...</span>}
            </div>
          )}

          {isRendering && (
            <div
              style={{
                position: 'absolute',
                top: '12px',
                right: '12px',
                backgroundColor: 'rgba(28, 31, 30, 0.75)',
                color: '#FAFAF7',
                padding: '4px 8px',
                borderRadius: 'var(--radius-xs)',
                fontSize: '11px',
                fontWeight: 500,
                zIndex: 50,
                pointerEvents: 'none',
              }}
            >
              İşleniyor...
            </div>
          )}
        </div>
      </div>
      )}
      </div>

      {/* Inspect Page Text Modal */}
      {textModal && !passwordPrompt.isOpen && (
        <div className="modal-backdrop" onClick={closeTextModal}>
          <div
            ref={modalRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="inspect-text-modal-title"
            className="modal-content"
            style={{ maxWidth: '580px', maxHeight: '80vh' }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              style={{
                padding: '12px 16px',
                borderBottom: '1px solid var(--color-border)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}
            >
              <div id="inspect-text-modal-title" style={{ fontWeight: 600, fontSize: '13px' }}>
                Sayfa {textModal.page} Metin Katmanı
              </div>
              <button
                type="button"
                className="btn btn-ghost"
                onClick={handleCopyText}
                style={{ height: '28px', padding: '0 8px', fontSize: '12px' }}
              >
                {isCopied ? <Check size={14} /> : <Copy size={14} />}
                <span>{isCopied ? 'Kopyalandı' : 'Kopyala'}</span>
              </button>
            </div>
            <div
              style={{
                padding: '16px',
                overflowY: 'auto',
                fontSize: '12.5px',
                fontFamily: 'var(--font-mono)',
                whiteSpace: 'pre-wrap',
                lineHeight: 1.6,
                maxHeight: '400px',
                backgroundColor: 'var(--color-bg)',
              }}
            >
              {textModal.text}
            </div>
            <div
              style={{
                padding: '10px 16px',
                borderTop: '1px solid var(--color-border)',
                display: 'flex',
                justifyContent: 'flex-end',
              }}
            >
              <button
                type="button"
                className="btn btn-secondary"
                onClick={closeTextModal}
              >
                Kapat
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
