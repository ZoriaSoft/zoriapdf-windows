import React from 'react';
import {
  Loader2,
  XCircle,
  CheckCircle,
  FolderOpen,
  FileText,
  AlertTriangle,
  X,
} from 'lucide-react';
import { useOperation } from '../context/OperationContext';

export const OperationBanner: React.FC = () => {
  const {
    activeJob,
    isRunning,
    cancelActiveJob,
    lastResult,
    clearLastResult,
    revealResult,
    openResultInTab,
    errorMessage,
    clearError,
  } = useOperation();

  if (!isRunning && !lastResult && !errorMessage) {
    return null;
  }

  return (
    <div
      style={{
        position: 'absolute',
        bottom: '16px',
        right: '16px',
        zIndex: 500,
        maxWidth: '420px',
        width: 'calc(100% - 32px)',
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
      }}
    >
      {/* Running Progress Bar */}
      {isRunning && activeJob && (
        <div
          style={{
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-lg)',
            padding: '12px 14px',
            display: 'flex',
            flexDirection: 'column',
            gap: '8px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontWeight: 600, fontSize: '13px' }}>
              <Loader2
                size={16}
                className="animate-spin"
                style={{ color: 'var(--color-accent)' }}
              />
              <span style={{ textTransform: 'capitalize' }}>
                İşlem Yürütülüyor: {activeJob.kind}
              </span>
            </div>
            <button
              type="button"
              onClick={cancelActiveJob}
              className="btn btn-ghost"
              style={{
                padding: '2px 8px',
                height: '24px',
                fontSize: '11.5px',
                color: 'var(--color-danger)',
              }}
              title="İşlemi İptal Et"
            >
              <XCircle size={13} />
              <span>İptal</span>
            </button>
          </div>

          {/* Progress bar */}
          {activeJob.progress ? (
            <>
              <div
                style={{
                  width: '100%',
                  height: '6px',
                  backgroundColor: 'var(--color-bg)',
                  borderRadius: '3px',
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    height: '100%',
                    backgroundColor: 'var(--color-accent)',
                    width: `${
                      activeJob.progress.total > 0
                        ? Math.min(
                            100,
                            Math.round(
                              (activeJob.progress.completed /
                                activeJob.progress.total) *
                                100
                            )
                          )
                        : 50
                    }%`,
                    transition: 'width var(--transition-fast)',
                  }}
                />
              </div>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  fontSize: '11px',
                  color: 'var(--color-text-subtle)',
                }}
              >
                <span>{activeJob.progress.message || 'İşleniyor...'}</span>
                {activeJob.progress.total > 0 && (
                  <span>
                    {activeJob.progress.completed} / {activeJob.progress.total}
                  </span>
                )}
              </div>
            </>
          ) : (
            <div
              style={{
                width: '100%',
                height: '4px',
                backgroundColor: 'var(--color-bg)',
                borderRadius: '2px',
                overflow: 'hidden',
                position: 'relative',
              }}
            >
              <div
                style={{
                  height: '100%',
                  backgroundColor: 'var(--color-accent)',
                  width: '40%',
                  animation: 'pulse 1.5s infinite',
                }}
              />
            </div>
          )}
        </div>
      )}

      {/* Success Result Notification */}
      {!isRunning && lastResult && (
        <div
          style={{
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderLeft: '4px solid var(--color-accent)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-lg)',
            padding: '12px 14px',
            display: 'flex',
            flexDirection: 'column',
            gap: '8px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontWeight: 600, fontSize: '13px', color: 'var(--color-accent)' }}>
              <CheckCircle size={16} />
              <span>İşlem Başarılı</span>
            </div>
            <button
              type="button"
              onClick={clearLastResult}
              className="btn btn-ghost"
              style={{ padding: '2px', height: '22px', width: '22px' }}
            >
              <X size={13} />
            </button>
          </div>

          <p style={{ fontSize: '12.5px', color: 'var(--color-text)', lineHeight: 1.4 }}>
            {lastResult.message}
          </p>

          {/* Action buttons for outputs */}
          {lastResult.outputs && lastResult.outputs.length > 0 && (
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px', marginTop: '4px' }}>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => revealResult(lastResult.outputs[0])}
                style={{ height: '28px', padding: '0 8px', fontSize: '11.5px', gap: '4px' }}
              >
                <FolderOpen size={13} />
                <span>Klasörde Göster</span>
              </button>

              {lastResult.outputs[0].toLowerCase().endsWith('.pdf') && (
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => {
                    openResultInTab(lastResult.outputs[0]);
                    clearLastResult();
                  }}
                  style={{ height: '28px', padding: '0 8px', fontSize: '11.5px', gap: '4px' }}
                >
                  <FileText size={13} />
                  <span>Sekmede Aç</span>
                </button>
              )}
            </div>
          )}
        </div>
      )}

      {/* Error Message Notification */}
      {!isRunning && errorMessage && (
        <div
          style={{
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderLeft: '4px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-lg)',
            padding: '12px 14px',
            display: 'flex',
            flexDirection: 'column',
            gap: '6px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontWeight: 600, fontSize: '13px', color: 'var(--color-danger)' }}>
              <AlertTriangle size={16} />
              <span>Hata</span>
            </div>
            <button
              type="button"
              onClick={clearError}
              className="btn btn-ghost"
              style={{ padding: '2px', height: '22px', width: '22px' }}
            >
              <X size={13} />
            </button>
          </div>
          <p style={{ fontSize: '12px', color: 'var(--color-text)', lineHeight: 1.45 }}>
            {errorMessage}
          </p>
        </div>
      )}
    </div>
  );
};
