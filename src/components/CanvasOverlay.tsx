import React, { useState, useRef, useCallback } from 'react';
import type { RedactRect, Annotation, AnnotationKind } from '../types';
import { useDocument } from '../context/DocumentContext';
import { useOperation } from '../context/OperationContext';
import { X } from 'lucide-react';

interface CanvasOverlayProps {
  pageWidth: number;
  pageHeight: number;
  annotationKind?: AnnotationKind;
  annotationText?: string;
  signatureImagePath?: string;
}

export const CanvasOverlay: React.FC<CanvasOverlayProps> = ({
  pageWidth,
  pageHeight,
  annotationKind = 'highlight',
  annotationText = '',
  signatureImagePath,
}) => {
  const {
    activeTab,
    addRedaction,
    removeRedaction,
    addAnnotation,
    removeAnnotation,
  } = useDocument();
  const { activeTool, isRunning } = useOperation();

  const containerRef = useRef<HTMLDivElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [startPos, setStartPos] = useState<{ x: number; y: number } | null>(null);
  const [currentPos, setCurrentPos] = useState<{ x: number; y: number } | null>(null);
  const [inkPoints, setInkPoints] = useState<[number, number][]>([]);

  const isRedactMode = activeTool === 'redact';
  const isAnnotateMode = activeTool === 'annotate';

  const getNormalizedCoords = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!containerRef.current) return { x: 0, y: 0 };
      const rect = containerRef.current.getBoundingClientRect();
      const rawX = e.clientX - rect.left;
      const rawY = e.clientY - rect.top;
      return {
        x: Math.max(0, Math.min(1, rawX / rect.width)),
        y: Math.max(0, Math.min(1, rawY / rect.height)),
      };
    },
    []
  );

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isRunning || (!isRedactMode && !isAnnotateMode)) return;
    if ((e.target as HTMLElement).closest('button')) return;
    if (e.button !== 0) return; // Left mouse button only
    if (isAnnotateMode && annotationKind === 'signature' && !signatureImagePath) {
      alert('İmza yerleştirmeden önce imza görselini seçin.');
      return;
    }

    const pos = getNormalizedCoords(e);
    setIsDrawing(true);
    setStartPos(pos);
    setCurrentPos(pos);

    if (isAnnotateMode && annotationKind === 'ink') {
      setInkPoints([[pos.x, pos.y]]);
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDrawing) return;
    const pos = getNormalizedCoords(e);
    setCurrentPos(pos);

    if (isAnnotateMode && annotationKind === 'ink') {
      setInkPoints((prev) => {
        const last = prev[prev.length - 1];
        if (prev.length >= 2048 || (last && Math.hypot(last[0] - pos.x, last[1] - pos.y) < 0.001)) return prev;
        return [...prev, [pos.x, pos.y]];
      });
    }
  };

  const handleMouseUp = () => {
    if (!isDrawing || !startPos || !currentPos || !activeTab) {
      setIsDrawing(false);
      setStartPos(null);
      setCurrentPos(null);
      setInkPoints([]);
      return;
    }

    const curPage = activeTab.currentPage;

    if (isRedactMode) {
      const x = Math.min(startPos.x, currentPos.x);
      const y = Math.min(startPos.y, currentPos.y);
      const width = Math.abs(currentPos.x - startPos.x);
      const height = Math.abs(currentPos.y - startPos.y);

      // Minimum dimension threshold to avoid accidental clicks
      if (width > 0.01 && height > 0.01) {
        addRedaction({
          page: curPage,
          x,
          y,
          width,
          height,
        });
      }
    } else if (isAnnotateMode) {
      if (annotationKind === 'ink') {
        if (inkPoints.length > 1) {
          // Calculate bounding box of ink points
          let minX = 1, minY = 1, maxX = 0, maxY = 0;
          inkPoints.forEach(([px, py]) => {
            if (px < minX) minX = px;
            if (py < minY) minY = py;
            if (px > maxX) maxX = px;
            if (py > maxY) maxY = py;
          });
          addAnnotation({
            page: curPage,
            x: minX,
            y: minY,
            width: Math.max(0.01, maxX - minX),
            height: Math.max(0.01, maxY - minY),
            kind: 'ink',
            points: inkPoints,
          });
        }
      } else {
        let x = Math.min(startPos.x, currentPos.x);
        let y = Math.min(startPos.y, currentPos.y);
        let width = Math.max(0.02, Math.abs(currentPos.x - startPos.x));
        let height = Math.max(0.02, Math.abs(currentPos.y - startPos.y));
        if (annotationKind === 'signature' && width < 0.05 && height < 0.05) {
          // A plain click means "put my signature here": use a readable default box
          // around the click instead of a 2% speck, kept inside the page.
          width = 0.28;
          height = 0.09;
          x = Math.min(Math.max(0, startPos.x - width / 2), 1 - width);
          y = Math.min(Math.max(0, startPos.y - height / 2), 1 - height);
        }

        addAnnotation({
          page: curPage,
          x,
          y,
          width,
          height,
          kind: annotationKind,
          text: annotationKind === 'text' ? (annotationText || 'Not') : undefined,
          imagePath: annotationKind === 'signature' ? signatureImagePath : undefined,
        });
      }
    }

    setIsDrawing(false);
    setStartPos(null);
    setCurrentPos(null);
    setInkPoints([]);
  };

  const curPage = activeTab?.currentPage || 1;
  // Drafts belong to their tool: redaction boxes show only while redacting and marks only
  // while annotating, so a draft never looks like it is already part of the document.
  const pageRedactions = isRedactMode ? (activeTab?.redactions || []).filter((r) => r.page === curPage) : [];
  const pageAnnotations = isAnnotateMode ? (activeTab?.annotations || []).filter((a) => a.page === curPage) : [];

  return (
    <div
      ref={containerRef}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        width: `${pageWidth}px`,
        height: `${pageHeight}px`,
        cursor: isRedactMode || isAnnotateMode ? 'crosshair' : 'default',
        pointerEvents: 'auto',
      }}
    >
      {/* Existing Redactions on current page */}
      {pageRedactions.map((rect, idx) => {
        const left = rect.x * pageWidth;
        const top = rect.y * pageHeight;
        const width = rect.width * pageWidth;
        const height = rect.height * pageHeight;

        return (
          <div
            key={`redact_${idx}`}
            style={{
              position: 'absolute',
              left: `${left}px`,
              top: `${top}px`,
              width: `${width}px`,
              height: `${height}px`,
              backgroundColor: '#000000',
              border: '1px solid rgba(255, 255, 255, 0.4)',
              boxShadow: '0 0 2px rgba(0,0,0,0.5)',
              display: 'flex',
              alignItems: 'flex-start',
              justifyContent: 'flex-end',
              zIndex: 10,
            }}
          >
            {isRedactMode && (
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  removeRedaction(activeTab!.redactions.indexOf(rect));
                }}
                style={{
                  backgroundColor: 'var(--color-danger)',
                  color: '#FFFFFF',
                  border: 'none',
                  borderRadius: '2px',
                  width: '16px',
                  height: '16px',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  cursor: 'pointer',
                  transform: 'translate(4px, -4px)',
                }}
                title="Redaksiyonu Kaldır"
              >
                <X size={10} />
              </button>
            )}
          </div>
        );
      })}

      {/* Existing Annotations on current page */}
      {pageAnnotations.map((ann, idx) => {
        const left = ann.x * pageWidth;
        const top = ann.y * pageHeight;
        const width = ann.width * pageWidth;
        const height = ann.height * pageHeight;

        if (ann.kind === 'highlight') {
          return (
            <div
              key={`ann_${idx}`}
              style={{
                position: 'absolute',
                left: `${left}px`,
                top: `${top}px`,
                width: `${width}px`,
                height: `${height}px`,
                backgroundColor: 'rgba(255, 230, 0, 0.35)',
                borderBottom: '2px solid rgba(230, 180, 0, 0.7)',
                zIndex: 8,
              }}
            >
              {isAnnotateMode && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeAnnotation(activeTab!.annotations.indexOf(ann));
                  }}
                  style={{
                    position: 'absolute',
                    top: '-6px',
                    right: '-6px',
                    backgroundColor: 'var(--color-danger)',
                    color: '#fff',
                    border: 'none',
                    borderRadius: '2px',
                    width: '14px',
                    height: '14px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    cursor: 'pointer',
                  }}
                >
                  <X size={9} />
                </button>
              )}
            </div>
          );
        }

        if (ann.kind === 'text') {
          return (
            <div
              key={`ann_${idx}`}
              style={{
                position: 'absolute',
                left: `${left}px`,
                top: `${top}px`,
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border-strong)',
                borderRadius: 'var(--radius-xs)',
                padding: '4px 6px',
                fontSize: '11px',
                color: 'var(--color-text)',
                boxShadow: 'var(--shadow-sm)',
                zIndex: 9,
                maxWidth: '200px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: '4px' }}>
                <span>{ann.text || 'Not'}</span>
                {isAnnotateMode && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeAnnotation(activeTab!.annotations.indexOf(ann));
                    }}
                    style={{
                      border: 'none',
                      background: 'none',
                      color: 'var(--color-danger)',
                      cursor: 'pointer',
                      padding: 0,
                    }}
                  >
                    <X size={10} />
                  </button>
                )}
              </div>
            </div>
          );
        }

        if (ann.kind === 'ink' && ann.points) {
          const svgPoints = ann.points
            .map(([px, py]) => `${px * pageWidth},${py * pageHeight}`)
            .join(' ');
          return (
            <svg
              key={`ann_${idx}`}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: `${pageWidth}px`,
                height: `${pageHeight}px`,
                pointerEvents: 'none',
                zIndex: 8,
              }}
            >
              <polyline
                points={svgPoints}
                fill="none"
                stroke="var(--color-accent)"
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          );
        }

        if (ann.kind === 'signature') {
          return (
            <div
              key={`ann_${idx}`}
              style={{
                position: 'absolute',
                left: `${left}px`,
                top: `${top}px`,
                width: `${width}px`,
                height: `${height}px`,
                border: '1px dashed var(--color-accent)',
                backgroundColor: 'rgba(15, 92, 74, 0.05)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: '12px',
                fontStyle: 'italic',
                color: 'var(--color-accent)',
                zIndex: 8,
              }}
            >
              İmza
              {isAnnotateMode && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeAnnotation(activeTab!.annotations.indexOf(ann));
                  }}
                  style={{
                    position: 'absolute',
                    top: '-6px',
                    right: '-6px',
                    backgroundColor: 'var(--color-danger)',
                    color: '#fff',
                    border: 'none',
                    borderRadius: '2px',
                    width: '14px',
                    height: '14px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    cursor: 'pointer',
                  }}
                >
                  <X size={9} />
                </button>
              )}
            </div>
          );
        }

        return null;
      })}

      {/* Live drawing preview */}
      {isDrawing && startPos && currentPos && (
        <>
          {isRedactMode && (
            <div
              style={{
                position: 'absolute',
                left: `${Math.min(startPos.x, currentPos.x) * pageWidth}px`,
                top: `${Math.min(startPos.y, currentPos.y) * pageHeight}px`,
                width: `${Math.abs(currentPos.x - startPos.x) * pageWidth}px`,
                height: `${Math.abs(currentPos.y - startPos.y) * pageHeight}px`,
                backgroundColor: 'rgba(0, 0, 0, 0.7)',
                border: '1px dashed #ffffff',
                pointerEvents: 'none',
                zIndex: 20,
              }}
            />
          )}

          {isAnnotateMode && annotationKind === 'highlight' && (
            <div
              style={{
                position: 'absolute',
                left: `${Math.min(startPos.x, currentPos.x) * pageWidth}px`,
                top: `${Math.min(startPos.y, currentPos.y) * pageHeight}px`,
                width: `${Math.abs(currentPos.x - startPos.x) * pageWidth}px`,
                height: `${Math.abs(currentPos.y - startPos.y) * pageHeight}px`,
                backgroundColor: 'rgba(255, 230, 0, 0.4)',
                border: '1px dashed #cca000',
                pointerEvents: 'none',
                zIndex: 20,
              }}
            />
          )}

          {isAnnotateMode && annotationKind === 'ink' && inkPoints.length > 0 && (
            <svg
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: `${pageWidth}px`,
                height: `${pageHeight}px`,
                pointerEvents: 'none',
                zIndex: 20,
              }}
            >
              <polyline
                points={inkPoints
                  .map(([px, py]) => `${px * pageWidth},${py * pageHeight}`)
                  .join(' ')}
                fill="none"
                stroke="var(--color-accent)"
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          )}

          {isAnnotateMode && (annotationKind === 'text' || annotationKind === 'signature') && (
            <div
              style={{
                position: 'absolute',
                left: `${Math.min(startPos.x, currentPos.x) * pageWidth}px`,
                top: `${Math.min(startPos.y, currentPos.y) * pageHeight}px`,
                width: `${Math.abs(currentPos.x - startPos.x) * pageWidth}px`,
                height: `${Math.abs(currentPos.y - startPos.y) * pageHeight}px`,
                backgroundColor: 'var(--color-accent-subtle)',
                border: '1px dashed var(--color-accent)',
                pointerEvents: 'none',
                zIndex: 20,
              }}
            />
          )}
        </>
      )}
    </div>
  );
};
