import React, { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickDirectory } from '../../bridge/tauri';

export const SplitTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [outputDir, setOutputDir] = useState<string>('');
  const [pages, setPages] = useState<string>('');

  const handlePickDirectory = async () => {
    const selected = await pickDirectory('Bölünen Sayfaların Kaydedileceği Klasör');
    if (selected) {
      setOutputDir(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputDir.trim()) return;
    await executeOperation('split', {
      inputs: [activeTab.info.path],
      output: outputDir.trim(),
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Belgeyi her sayfası ayrı bir PDF olacak şekilde belirtilen klasöre böler.
      </div>

      <div className="form-group">
        <label className="form-label">Sayfa Aralığı (İsteğe Bağlı)</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Örn: 1-5, 8 (Boş bırakılırsa tüm ${activeTab.info.pages} sayfa)`}
          className="form-input"
        />
        <span className="form-help">Boş bırakılırsa tüm sayfalar ayrı ayrı kaydedilir.</span>
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
            onClick={handlePickDirectory}
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
        {isRunning ? 'Bölünüyor...' : 'Belgeyi Sayfalara Böl'}
      </button>
    </div>
  );
};
