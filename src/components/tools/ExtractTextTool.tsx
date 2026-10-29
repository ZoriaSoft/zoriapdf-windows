import React, { useState } from 'react';
import { FolderOpen, FileText } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSaveTextPath } from '../../bridge/tauri';

export const ExtractTextTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [pages, setPages] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSaveTextPath('metin_cikti.txt');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputPath.trim()) return;
    await executeOperation('extract-text', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        PDF belgesindeki tüm metin katmanını ayıklayıp UTF-8 .txt dosyası olarak dışa aktarır.
      </div>

      <div className="form-group">
        <label className="form-label">Sayfalar (İsteğe Bağlı)</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Tüm sayfalar (veya örn: 1-5)`}
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
        <label className="form-label">Çıktı Metin Dosyası (.txt)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            placeholder="Kaydedilecek dosya yolu (.txt)..."
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
        style={{ width: '100%', height: '34px', marginTop: '6px', gap: '6px' }}
      >
        <FileText size={15} />
        <span>{isRunning ? 'Metin Ayıklanıyor...' : 'Metni Ayıkla ve Kaydet'}</span>
      </button>
    </div>
  );
};
