import React, { useState } from 'react';
import { FolderOpen, CheckCircle2 } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const CompressTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('sikistirilmis_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputPath.trim()) return;
    await executeOperation('compress', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      {/* Honest lossless label */}
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
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, fontSize: '12px', color: 'var(--color-accent)' }}>
          <CheckCircle2 size={15} />
          <span>Dürüst Kayıpsız Sıkıştırma (Lossless)</span>
        </div>
        <p style={{ fontSize: '11.5px', color: 'var(--color-text-muted)', lineHeight: 1.45 }}>
          Bu işlem PDF akışlarını, font tanımlarını ve metadata tablolarını yeniden paketleyerek dosya boyutunu küçültür. Görsellerin piksel kalitesini veya çözünürlüğünü asla düşürmez.
        </p>
      </div>

      <div style={{ fontSize: '12px', color: 'var(--color-text-muted)' }}>
        Mevcut Dosya Boyutu: <strong>{formatSize(activeTab.info.size)}</strong>
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
        disabled={!outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Sıkıştırılıyor...' : 'Belgeyi Sıkıştır ve Kaydet'}
      </button>
    </div>
  );
};
