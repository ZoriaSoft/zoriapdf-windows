import React, { useState } from 'react';
import {
  FileUp,
  FolderOpen,
  Clock,
  Layers,
  Images,
  Shield,
  ArrowRight,
} from 'lucide-react';
import { useDocument } from '../context/DocumentContext';
import { displayPath } from '../viewer/renderQueue';
import { useOperation } from '../context/OperationContext';

export const EmptyState: React.FC = () => {
  const { openFilesDialog, openPaths, recentPaths } = useDocument();
  const { setActiveTool } = useOperation();
  const [isDragOver, setIsDragOver] = useState(false);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = () => {
    setIsDragOver(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    // Standard web drag-drop fallback if files are available
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      // Note: In Tauri native, onDragDropEvent handles native file paths
      console.log('Drop detected in browser viewport');
    }
  };

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        padding: '32px',
        backgroundColor: 'var(--color-bg)',
        overflowY: 'auto',
      }}
    >
      <div
        style={{
          maxWidth: '560px',
          width: '100%',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: '24px',
        }}
      >
        {/* Main Drop Target Box */}
        <div
          onClick={openFilesDialog}
          style={{
            width: '100%',
            padding: '40px 24px',
            backgroundColor: isDragOver
              ? 'var(--color-surface-hover)'
              : 'var(--color-surface)',
            border: `2px dashed ${
              isDragOver ? 'var(--color-accent)' : 'var(--color-border)'
            }`,
            borderRadius: 'var(--radius-lg)',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            textAlign: 'center',
            cursor: 'pointer',
            boxShadow: 'var(--shadow-sm)',
            transition:
              'background-color var(--transition-fast), border-color var(--transition-fast), transform var(--transition-fast)',
          }}
        >
          <div
            style={{
              width: '54px',
              height: '54px',
              borderRadius: '50%',
              backgroundColor: 'var(--color-accent-subtle)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: 'var(--color-accent)',
              marginBottom: '16px',
            }}
          >
            <FileUp size={28} />
          </div>

          <h2
            style={{
              fontSize: '18px',
              fontWeight: 700,
              color: 'var(--color-text)',
              marginBottom: '6px',
              letterSpacing: '-0.3px',
            }}
          >
            PDF Belgesi Seçin veya Buraya Bırakın
          </h2>

          <p
            style={{
              fontSize: '13px',
              color: 'var(--color-text-muted)',
              maxWidth: '360px',
              lineHeight: 1.5,
              marginBottom: '20px',
            }}
          >
            Belgelerinizi görüntüleyin, sayfaları düzenleyin, sansürleyin, şifreleyin veya dönüştürün.
          </p>

          <button
            type="button"
            className="btn btn-primary"
            style={{ padding: '8px 20px', fontSize: '13px', height: '36px' }}
            onClick={(e) => {
              e.stopPropagation();
              openFilesDialog();
            }}
          >
            <FolderOpen size={16} />
            <span>Bilgisayardan PDF Aç</span>
          </button>
        </div>

        {/* Quick Create Tools */}
        <div
          style={{
            width: '100%',
            display: 'grid',
            gridTemplateColumns: '1fr 1fr',
            gap: '12px',
          }}
        >
          <div
            onClick={() => setActiveTool('merge')}
            style={{
              padding: '14px 16px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              boxShadow: 'var(--shadow-sm)',
              transition: 'background-color var(--transition-fast), border-color var(--transition-fast)',
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border-strong)';
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border)';
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
              <div
                style={{
                  width: '32px',
                  height: '32px',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-accent-subtle)',
                  color: 'var(--color-accent)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Layers size={16} />
              </div>
              <div>
                <div style={{ fontWeight: 600, fontSize: '12.5px', color: 'var(--color-text)' }}>
                  PDF Birleştir
                </div>
                <div style={{ fontSize: '11px', color: 'var(--color-text-subtle)' }}>
                  Birden fazla belgeyi sırala
                </div>
              </div>
            </div>
            <ArrowRight size={14} style={{ color: 'var(--color-text-subtle)' }} />
          </div>

          <div
            onClick={() => setActiveTool('images-to-pdf')}
            style={{
              padding: '14px 16px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              boxShadow: 'var(--shadow-sm)',
              transition: 'background-color var(--transition-fast), border-color var(--transition-fast)',
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border-strong)';
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border)';
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
              <div
                style={{
                  width: '32px',
                  height: '32px',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-accent-subtle)',
                  color: 'var(--color-accent)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Images size={16} />
              </div>
              <div>
                <div style={{ fontWeight: 600, fontSize: '12.5px', color: 'var(--color-text)' }}>
                  Görsellerden PDF
                </div>
                <div style={{ fontSize: '11px', color: 'var(--color-text-subtle)' }}>
                  PNG, JPG ile yeni PDF yap
                </div>
              </div>
            </div>
            <ArrowRight size={14} style={{ color: 'var(--color-text-subtle)' }} />
          </div>
        </div>

        {/* Recent local files */}
        {recentPaths.length > 0 && (
          <div
            style={{
              width: '100%',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '12px 16px',
              boxShadow: 'var(--shadow-sm)',
            }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '11.5px',
                fontWeight: 700,
                color: 'var(--color-text-subtle)',
                textTransform: 'uppercase',
                letterSpacing: '0.5px',
                marginBottom: '10px',
              }}
            >
              <Clock size={13} />
              <span>Son Açılan Belgeler</span>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              {recentPaths.slice(0, 10).map((rawPath) => {
                const path = displayPath(rawPath);
                const name = path.split(/[/\\]/).pop() || path;
                return (
                  <div
                    key={rawPath}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openPaths([rawPath]); } }}
                    onClick={() => openPaths([rawPath])}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '6px 8px',
                      borderRadius: 'var(--radius-sm)',
                      cursor: 'pointer',
                      fontSize: '12.5px',
                      transition: 'background-color var(--transition-fast)',
                    }}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLElement).style.backgroundColor =
                        'var(--color-surface-hover)';
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLElement).style.backgroundColor =
                        'transparent';
                    }}
                  >
                    <span
                      style={{
                        fontWeight: 500,
                        color: 'var(--color-text)',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {name}
                    </span>
                    <span
                      style={{
                        fontSize: '11px',
                        color: 'var(--color-text-subtle)',
                        maxWidth: '220px',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {path}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Privacy Note */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            fontSize: '11.5px',
            color: 'var(--color-text-subtle)',
            marginTop: '8px',
          }}
        >
          <Shield size={14} style={{ color: 'var(--color-accent)' }} />
          <span>Tüm işlemler yerel çalışır. Belgeleriniz asla bilgisayarınızdan dışarı çıkmaz.</span>
        </div>
      </div>
    </div>
  );
};
