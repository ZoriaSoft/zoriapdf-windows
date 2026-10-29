import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const ExtractTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [pages, setPages] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('cikarilan_sayfalar.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !pages.trim() || !outputPath.trim()) return;
    await executeOperation('extract', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      pages: pages.trim(),
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Belirtilen sayfaları çıkarıp yeni bir PDF belgesi oluşturur.
      </div>

      <div className="form-group">
        <label className="form-label">Çıkarılacak Sayfalar</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Örn: 1,3-5 (Mevcut: ${activeTab.currentPage})`}
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
        disabled={!pages.trim() || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Çıkarılıyor...' : 'Sayfaları Çıkar ve Kaydet'}
      </button>
    </div>
  );
};
