import React, { useState } from 'react';
import { Plus, Trash2, ArrowUp, ArrowDown, FolderOpen } from 'lucide-react';
import { useOperation } from '../../context/OperationContext';
import { pickImageFiles, pickSavePdfPath } from '../../bridge/tauri';

export const ImagesToPdfTool: React.FC = () => {
  const { executeOperation, isRunning } = useOperation();

  const [imagePaths, setImagePaths] = useState<string[]>([]);
  const [dpi, setDpi] = useState<number>(150);
  const [outputPath, setOutputPath] = useState<string>('');

  const handleAddImages = async () => {
    const selected = await pickImageFiles(true);
    if (selected && selected.length > 0) {
      setImagePaths((prev) => [...prev, ...selected]);
    }
  };

  const handleMoveUp = (index: number) => {
    if (index === 0) return;
    setImagePaths((prev) => {
      const copy = [...prev];
      const temp = copy[index - 1];
      copy[index - 1] = copy[index];
      copy[index] = temp;
      return copy;
    });
  };

  const handleMoveDown = (index: number) => {
    if (index === imagePaths.length - 1) return;
    setImagePaths((prev) => {
      const copy = [...prev];
      const temp = copy[index + 1];
      copy[index + 1] = copy[index];
      copy[index] = temp;
      return copy;
    });
  };

  const handleRemove = (index: number) => {
    setImagePaths((prev) => prev.filter((_, i) => i !== index));
  };

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('gorsellerden_olusturulan.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (imagePaths.length === 0 || !outputPath.trim()) return;
    await executeOperation('images-to-pdf', {
      inputs: [],
      imagePaths,
      output: outputPath.trim(),
      dpi,
    });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Seçilen görselleri (PNG, JPG, WebP) tek bir PDF belgesine dönüştürür.
      </div>

      <div className="form-group">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
          <label className="form-label" style={{ margin: 0 }}>Görseller ({imagePaths.length})</label>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handleAddImages}
            style={{ padding: '3px 8px', height: '24px', fontSize: '11.5px' }}
          >
            <Plus size={13} />
            <span>Görsel Ekle</span>
          </button>
        </div>

        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '4px',
            maxHeight: '180px',
            overflowY: 'auto',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-sm)',
            padding: '4px',
            backgroundColor: 'var(--color-bg)',
          }}
        >
          {imagePaths.length === 0 ? (
            <div style={{ padding: '12px', textAlign: 'center', fontSize: '12px', color: 'var(--color-text-subtle)' }}>
              Henüz görsel eklenmedi. "Görsel Ekle" butonuna tıklayın.
            </div>
          ) : (
            imagePaths.map((file, idx) => {
              const name = file.split(/[/\\]/).pop() || file;
              return (
                <div
                  key={idx}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    padding: '4px 6px',
                    backgroundColor: 'var(--color-surface)',
                    borderRadius: 'var(--radius-xs)',
                    fontSize: '12px',
                    border: '1px solid var(--color-border-subtle)',
                  }}
                >
                  <span
                    style={{
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                      maxWidth: '180px',
                    }}
                    title={file}
                  >
                    {idx + 1}. {name}
                  </span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '2px' }}>
                    <button
                      type="button"
                      onClick={() => handleMoveUp(idx)}
                      disabled={idx === 0}
                      className="btn btn-ghost"
                      style={{ padding: '2px', height: '20px', width: '20px' }}
                    >
                      <ArrowUp size={11} />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleMoveDown(idx)}
                      disabled={idx === imagePaths.length - 1}
                      className="btn btn-ghost"
                      style={{ padding: '2px', height: '20px', width: '20px' }}
                    >
                      <ArrowDown size={11} />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleRemove(idx)}
                      className="btn btn-ghost"
                      style={{ padding: '2px', height: '20px', width: '20px', color: 'var(--color-danger)' }}
                    >
                      <Trash2 size={11} />
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      <div className="form-group">
        <label className="form-label">Çözünürlük (DPI)</label>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '6px' }}>
          {[72, 150, 300].map((d) => (
            <button
              key={d}
              type="button"
              className={`btn ${dpi === d ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setDpi(d)}
              style={{ height: '30px', fontSize: '12px' }}
            >
              {d} DPI
            </button>
          ))}
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
        disabled={imagePaths.length === 0 || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'PDF Oluşturuluyor...' : 'Görselleri PDF Yap'}
      </button>
    </div>
  );
};
