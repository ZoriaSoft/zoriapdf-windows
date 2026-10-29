import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickDirectory } from '../../bridge/tauri';

export const PdfToImagesTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [dpi, setDpi] = useState<number>(150);
  const [pages, setPages] = useState<string>('');
  const [outputDir, setOutputDir] = useState<string>('');

  const handlePickDir = async () => {
    const selected = await pickDirectory('Görsellerin Kaydedileceği Klasör');
    if (selected) {
      setOutputDir(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputDir.trim()) return;
    await executeOperation('pdf-to-images', {
      inputs: [activeTab.info.path],
      output: outputDir.trim(),
      dpi,
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        PDF sayfalarını yüksek kaliteli görsel dosyalarına (PNG/JPG) dönüştürerek belirtilen klasöre kaydeder.
      </div>

      <div className="form-group">
        <label className="form-label">Çözünürlük (DPI)</label>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '6px' }}>
          {[150, 300].map((d) => (
            <button
              key={d}
              type="button"
              className={`btn ${dpi === d ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setDpi(d)}
              style={{ height: '30px', fontSize: '12px' }}
            >
              {d} DPI {d === 300 ? '(Baskı Kalitesi)' : '(Standart)'}
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
        <label className="form-label">Çıktı Klasörü (Dizin)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={outputDir}
            onChange={(e) => setOutputDir(e.target.value)}
            placeholder="Klasör yolu..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickDir}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Klasör Seç"
          >
            <FolderOpen size={14} />
          </button>
        </div>
      </div>

      <button
        type="button"
        className="btn btn-primary"
        onClick={handleRun}
        disabled={!outputDir.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Dönüştürülüyor...' : 'Görsellere Dönüştür'}
      </button>
    </div>
  );
};
