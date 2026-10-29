import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const RemoveTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [pages, setPages] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('sayfalar_silindi.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !pages.trim() || !outputPath.trim()) return;
    await executeOperation('remove', {
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
        İstemediğiniz sayfaları belgeden silerek yeni bir PDF oluşturur.
      </div>

      <div className="form-group">
        <label className="form-label">Silinecek Sayfalar</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Örn: 2, 4-6 (Aktif: ${activeTab.currentPage})`}
          className="form-input"
        />
        <div style={{ display: 'flex', gap: '6px', marginTop: '4px' }}>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setPages(String(activeTab.currentPage))}
            style={{ padding: '2px 6px', fontSize: '11px', height: '22px' }}
          >
            Aktif Sayfayı Ekle ({activeTab.currentPage})
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
        className="btn btn-danger"
        onClick={handleRun}
        disabled={!pages.trim() || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Siliniyor...' : 'Sayfaları Sil ve Kaydet'}
      </button>
    </div>
  );
};
