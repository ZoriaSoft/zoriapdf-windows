import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSaveDocxPath } from '../../bridge/tauri';

/** PDF → DOCX with automatic OCR for scanned pages. */
export const PdfToWordTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();
  const [outputPath, setOutputPath] = useState<string>('');

  if (!activeTab) {
    return <div className="form-help">Word'e dönüştürmek için lütfen bir PDF belgesi açın.</div>;
  }

  const suggestedName = activeTab.info.name.replace(/\.pdf$/i, '') + '.docx';

  const handlePickOutput = async () => {
    const selected = await pickSaveDocxPath(suggestedName);
    if (selected) setOutputPath(selected);
  };

  const handleRun = async () => {
    if (!outputPath.trim()) return;
    await executeOperation('pdf-to-word', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
    });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Metin sayfaları yazı biçimleri, tablolar ve resimlerle birlikte aktarılır; taranmış sayfalar otomatik algılanıp yerel OCR ile
        tanınır. Seçim yapmanız gerekmez, hiçbir belge bilgisayarınızdan çıkmaz.
        {' '}Küçük pakette taranmış sayfalar için önce Yerel OCR bölümünden OCR bileşenini
        kurun. Metin içeren PDF'ler ek indirme olmadan dönüştürülür.
      </div>

      <div className="form-group">
        <label className="form-label">Kaynak</label>
        <div className="form-static">{activeTab.info.name} · {activeTab.info.pages} sayfa</div>
      </div>

      <div className="form-group">
        <label className="form-label">Çıktı Dosyası (.docx)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            placeholder="Kaydedilecek Word dosyası..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickOutput}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Konum Seç"
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
      >
        Word'e Dönüştür
      </button>
    </div>
  );
};
