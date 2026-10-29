import React, { useState } from 'react';
import { FolderOpen, Image as ImageIcon } from 'lucide-react';
import { useOperation } from '../../context/OperationContext';
import { pickImageFiles, pickSaveDocxPath } from '../../bridge/tauri';

/** Image/photo → DOCX using the local OCR engine. */
export const ImageToWordTool: React.FC = () => {
  const { executeOperation, isRunning } = useOperation();
  const [imagePath, setImagePath] = useState<string>('');
  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickImage = async () => {
    const selected = await pickImageFiles(false);
    if (selected && selected.length > 0) {
      setImagePath(selected[0]);
      if (!outputPath) {
        const stem = selected[0].split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') ?? 'belge';
        setOutputPath(`${stem}.docx`);
      }
    }
  };

  const handlePickOutput = async () => {
    const selected = await pickSaveDocxPath('belge.docx');
    if (selected) setOutputPath(selected);
  };

  const handleRun = async () => {
    if (!imagePath.trim() || !outputPath.trim()) return;
    await executeOperation('image-to-word', {
      inputs: [imagePath.trim()],
      imagePaths: [imagePath.trim()],
      output: outputPath.trim(),
    });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Fotoğraf veya taranmış görseldeki yazı yerel OCR motoruyla (Türkçe + İngilizce)
        tanınır ve Word paragraflarına aktarılır. Küçük pakette önce Yerel OCR bölümünden
        bileşeni kurun; tam çevrimdışı pakette hazır gelir. Tanıma sırasında ağ kullanılmaz.
      </div>

      <div className="form-group">
        <label className="form-label">Görsel</label>
        <div style={{ display: 'flex', gap: '6px' }}>
          <input
            type="text"
            value={imagePath}
            onChange={(e) => setImagePath(e.target.value)}
            placeholder="PNG veya JPG seçin..."
            className="form-input"
          />
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handlePickImage}
            style={{ flexShrink: 0, padding: '0 8px' }}
            title="Görsel Seç"
          >
            <ImageIcon size={14} />
          </button>
        </div>
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
        disabled={!imagePath.trim() || !outputPath.trim() || isRunning}
      >
        Word Belgesi Oluştur
      </button>
    </div>
  );
};
