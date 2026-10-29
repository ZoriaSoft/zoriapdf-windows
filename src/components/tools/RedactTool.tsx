import React, { useState } from 'react';
import { Trash2, FolderOpen, EyeOff } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const RedactTool: React.FC = () => {
  const { activeTab, removeRedaction, clearRedactions } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('redakte_edilmis_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputPath.trim() || activeTab.redactions.length === 0) return;
    await executeOperation('redact', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      rects: activeTab.redactions,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  const redactions = activeTab.redactions;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div
        style={{
          padding: '10px 12px',
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          borderRadius: 'var(--radius-sm)',
          display: 'flex',
          flexDirection: 'column',
          gap: '6px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, fontSize: '12px', color: 'var(--color-text)' }}>
          <EyeOff size={15} style={{ color: 'var(--color-accent)' }} />
          <span>Kalıcı Metin & Alan Sansürleme</span>
        </div>
        <p style={{ fontSize: '11.5px', color: 'var(--color-text-muted)', lineHeight: 1.45 }}>
          Sansürlemek istediğiniz alanları farenizle belgenin üzerine tıklayıp sürükleyerek seçin. Seçilen alanlar siyah kutu ile kalıcı olarak örtülecektir.
        </p>
      </div>

      <div className="form-group">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
          <label className="form-label" style={{ margin: 0 }}>
            Sansür Alanları ({redactions.length})
          </label>
          {redactions.length > 0 && (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={clearRedactions}
              style={{ padding: '2px 6px', fontSize: '11px', color: 'var(--color-danger)' }}
            >
              Tümünü Temizle
            </button>
          )}
        </div>

        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '4px',
            maxHeight: '160px',
            overflowY: 'auto',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-sm)',
            padding: '4px',
            backgroundColor: 'var(--color-bg)',
          }}
        >
          {redactions.length === 0 ? (
            <div style={{ padding: '12px', textAlign: 'center', fontSize: '12px', color: 'var(--color-text-subtle)' }}>
              Henüz sansür alanı seçilmedi. Belge üzerine kutu çizin.
            </div>
          ) : (
            redactions.map((rect, idx) => (
              <div
                key={idx}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '4px 8px',
                  backgroundColor: 'var(--color-surface)',
                  borderRadius: 'var(--radius-xs)',
                  fontSize: '11.5px',
                  border: '1px solid var(--color-border-subtle)',
                }}
              >
                <span>
                  Sayfa {rect.page} &bull; (x: {Math.round(rect.x * 100)}%, y: {Math.round(rect.y * 100)}%)
                </span>
                <button
                  type="button"
                  onClick={() => removeRedaction(idx)}
                  className="btn btn-ghost"
                  style={{ padding: '2px', height: '20px', width: '20px', color: 'var(--color-danger)' }}
                  title="Alanı Kaldır"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))
          )}
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">Çıktı Dosyası (.pdf)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            placeholder="Kaydedilecek dosya yolu..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickOutput}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Dosya Seç"
          >
            <FolderOpen size={14} />
          </button>
        </div>
      </div>

      <button
        type="button"
        className="btn btn-primary"
        onClick={handleRun}
        disabled={redactions.length === 0 || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Sansürleniyor...' : 'Sansürleri Kalıcı Olarak Uygula'}
      </button>
    </div>
  );
};
