import React, { useState } from 'react';
import { Plus, Trash2, ArrowUp, ArrowDown, FolderOpen } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickPdfFiles, pickSavePdfPath } from '../../bridge/tauri';

export const MergeTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [files, setFiles] = useState<string[]>(() => {
    return activeTab ? [activeTab.info.path] : [];
  });
  const [outputPath, setOutputPath] = useState<string>('');

  const handleAddFiles = async () => {
    const selected = await pickPdfFiles(true);
    if (selected && selected.length > 0) {
      setFiles((prev) => [...prev, ...selected]);
    }
  };

  const handleMoveUp = (index: number) => {
    if (index === 0) return;
    setFiles((prev) => {
      const copy = [...prev];
      const temp = copy[index - 1];
      copy[index - 1] = copy[index];
      copy[index] = temp;
      return copy;
    });
  };

  const handleMoveDown = (index: number) => {
    if (index === files.length - 1) return;
    setFiles((prev) => {
      const copy = [...prev];
      const temp = copy[index + 1];
      copy[index + 1] = copy[index];
      copy[index] = temp;
      return copy;
    });
  };

  const handleRemove = (index: number) => {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  };

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('birlestirilmis.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (files.length < 2 || !outputPath.trim()) return;
    await executeOperation('merge', {
      inputs: files,
      output: outputPath.trim(),
    });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Birden çok PDF belgesini sırayla birleştirir. Orijinal dosyalar korunur.
      </div>

      <div className="form-group">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
          <label className="form-label" style={{ margin: 0 }}>Birleştirilecek Belgeler ({files.length})</label>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={handleAddFiles}
            style={{ padding: '3px 8px', height: '24px', fontSize: '11.5px' }}
          >
            <Plus size={13} />
            <span>Belge Ekle</span>
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
          {files.length === 0 ? (
            <div style={{ padding: '12px', textAlign: 'center', fontSize: '12px', color: 'var(--color-text-subtle)' }}>
              Henüz belge eklenmedi.
            </div>
          ) : (
            files.map((file, idx) => {
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
                      disabled={idx === files.length - 1}
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

      {/* Output path picker */}
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
        disabled={files.length < 2 || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Birleştiriliyor...' : 'Belgeleri Birleştir'}
      </button>
    </div>
  );
};
