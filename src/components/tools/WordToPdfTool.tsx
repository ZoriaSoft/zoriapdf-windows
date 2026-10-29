import React, { useState } from 'react';
import { FolderOpen, FileText } from 'lucide-react';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath, pickWordFile } from '../../bridge/tauri';

/** DOCX → PDF with embedded Unicode font; text, headings and page breaks are kept. */
export const WordToPdfTool: React.FC = () => {
  const { executeOperation, isRunning } = useOperation();
  const [inputPath, setInputPath] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickInput = async () => {
    const selected = await pickWordFile();
    if (selected) {
      setInputPath(selected);
      if (!outputPath) {
        const stem = selected.split(/[\\/]/).pop()?.replace(/\.docx$/i, '') ?? 'belge';
        setOutputPath(`${stem}.pdf`);
      }
    }
  };

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('belge.pdf');
    if (selected) setOutputPath(selected);
  };

  const handleRun = async () => {
    if (!inputPath.trim() || !outputPath.trim()) return;
    await executeOperation('word-to-pdf', {
      inputs: [inputPath.trim()],
      output: outputPath.trim(),
    });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Yazı biçimleri, başlıklar, listeler, tablolar, resimler (PNG/JPEG) ve sayfa sonları
        PDF'e aktarılır.
      </div>

      <div className="form-group">
        <label className="form-label">Word Belgesi (.docx)</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={inputPath}
            onChange={(e) => setInputPath(e.target.value)}
            placeholder="Word dosyası seçin..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickInput}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Belge Seç"
          >
            <FileText size={14} />
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
            placeholder="Kaydedilecek PDF..."
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
        disabled={!inputPath.trim() || !outputPath.trim() || isRunning}
      >
        PDF'e Dönüştür
      </button>
    </div>
  );
};
