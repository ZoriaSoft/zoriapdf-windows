import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath } from '../../bridge/tauri';

export const ReorderTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [order, setOrder] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('sirali_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !order.trim() || !outputPath.trim()) return;
    await executeOperation('reorder', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      pages: order.trim(),
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Sayfaların sırasını değiştirir veya ters çevirir (1-tabanlı dizin).
      </div>

      <div className="form-group">
        <label className="form-label">Yeni Sayfa Sırası</label>
        <input
          type="text"
          value={order}
          onChange={(e) => setOrder(e.target.value)}
          placeholder={`Örn: 3,1,2 veya ${activeTab.info.pages}-1`}
          className="form-input"
        />
        <div style={{ display: 'flex', gap: '6px', marginTop: '4px' }}>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setOrder(`${activeTab.info.pages}-1`)}
            style={{ padding: '2px 6px', fontSize: '11px', height: '22px' }}
          >
            Ters Çevir ({activeTab.info.pages}-1)
          </button>
        </div>
        <span className="form-help">
          Belge toplam {activeTab.info.pages} sayfadır. Virgülle ayrılmış sayfa veya aralık (örn: 1-3, 5-4) yazın.
        </span>
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
        disabled={!order.trim() || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Sıralanıyor...' : 'Yeni Sırayla Kaydet'}
      </button>
    </div>
  );
};
