import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const RotateTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [rotation, setRotation] = useState<number>(90);
  const [pages, setPages] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('dondurulmus_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputPath.trim()) return;
    await executeOperation('rotate', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      rotation,
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Sayfaları saat yönünde 90°, 180° veya 270° döndürür.
      </div>

      <div className="form-group">
        <label className="form-label">Döndürme Açısı</label>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '6px' }}>
          {[90, 180, 270].map((deg) => (
            <button
              key={deg}
              type="button"
              className={`btn ${rotation === deg ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setRotation(deg)}
              style={{ height: '32px', fontSize: '12px' }}
            >
              {deg}°
            </button>
          ))}
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">Sayfalar (İsteğe Bağlı)</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Tüm sayfalar (veya örn: 1, 3-5)`}
          className="form-input"
        />
        <div style={{ display: 'flex', gap: '6px', marginTop: '4px' }}>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setPages(String(activeTab.currentPage))}
            style={{ padding: '2px 6px', fontSize: '11px', height: '22px' }}
          >
            Yalnızca Aktif Sayfa ({activeTab.currentPage})
          </button>
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
        disabled={!outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Döndürülüyor...' : 'Sayfaları Döndür ve Kaydet'}
      </button>
    </div>
  );
};
