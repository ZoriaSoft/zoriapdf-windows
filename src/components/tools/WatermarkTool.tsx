import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const WatermarkTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [text, setText] = useState<string>('GİZLİ');
  const [pages, setPages] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('filigranli_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !text.trim() || !outputPath.trim()) return;
    await executeOperation('watermark', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      text: text.trim(),
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  const presets = ['GİZLİ', 'TASLAK', 'CONFIDENTIAL', 'KOPYA', 'ARŞİV'];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Sayfaların arka planına çapraz metin filigranı ekler.
      </div>

      <div className="form-group">
        <label className="form-label">Filigran Metni</label>
        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Örn: GİZLİ, TASLAK"
          className="form-input"
        />
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '4px', marginTop: '6px' }}>
          {presets.map((p) => (
            <button
              key={p}
              type="button"
              className="btn btn-secondary"
              onClick={() => setText(p)}
              style={{ padding: '2px 6px', fontSize: '11px', height: '22px' }}
            >
              {p}
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
          placeholder={`Tüm sayfalar (veya örn: 1-3)`}
          className="form-input"
        />
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
        disabled={!text.trim() || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Ekleniyor...' : 'Filigran Ekle ve Kaydet'}
      </button>
    </div>
  );
};
