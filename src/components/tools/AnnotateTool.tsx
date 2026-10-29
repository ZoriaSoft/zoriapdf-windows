import React, { useState } from 'react';
import {
  Highlighter,
  Type,
  PenTool,
  Award,
  Trash2,
  FolderOpen,
  Image as ImageIcon,
} from 'lucide-react';
import type { AnnotationKind } from '../../types';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { pickSavePdfPath, pickImageFiles } from '../../bridge/tauri';

interface AnnotateToolProps {
  annotationKind: AnnotationKind;
  setAnnotationKind: (k: AnnotationKind) => void;
  annotationText: string;
  setAnnotationText: (t: string) => void;
  signatureImagePath: string;
  setSignatureImagePath: (p: string) => void;
}

export const AnnotateTool: React.FC<AnnotateToolProps> = ({
  annotationKind,
  setAnnotationKind,
  annotationText,
  setAnnotationText,
  signatureImagePath,
  setSignatureImagePath,
}) => {
  const { activeTab, removeAnnotation, clearAnnotations } = useDocument();
  const { executeOperation, isRunning } = useOperation();

  const [outputPath, setOutputPath] = useState<string>('');

  const handlePickSignatureImage = async () => {
    const selected = await pickImageFiles(false);
    if (selected && selected.length > 0) {
      setSignatureImagePath(selected[0]);
    }
  };

  const handlePickOutput = async () => {
    const selected = await pickSavePdfPath('isaretlenmis_belge.pdf');
    if (selected) {
      setOutputPath(selected);
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputPath.trim() || activeTab.annotations.length === 0) return;
    await executeOperation('annotate', {
      inputs: [activeTab.info.path],
      output: outputPath.trim(),
      annotations: activeTab.annotations,
    });
  };

  if (!activeTab) {
    return <div className="form-help">İşlem yapmak için lütfen bir PDF belgesi açın.</div>;
  }

  const annotations = activeTab.annotations;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Belgeye vurgu, metin notu, el yazısı çizim veya imza ekleyin.
      </div>

      <div className="form-group">
        <label className="form-label">Anotasyon Türü</label>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '6px' }}>
          <button
            type="button"
            className={`btn ${annotationKind === 'highlight' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => setAnnotationKind('highlight')}
            style={{ height: '32px', fontSize: '11.5px', gap: '4px' }}
          >
            <Highlighter size={13} />
            <span>Vurgu</span>
          </button>
          <button
            type="button"
            className={`btn ${annotationKind === 'text' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => setAnnotationKind('text')}
            style={{ height: '32px', fontSize: '11.5px', gap: '4px' }}
          >
            <Type size={13} />
            <span>Metin Notu</span>
          </button>
          <button
            type="button"
            className={`btn ${annotationKind === 'ink' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => setAnnotationKind('ink')}
            style={{ height: '32px', fontSize: '11.5px', gap: '4px' }}
          >
            <PenTool size={13} />
            <span>Serbest Çizim</span>
          </button>
          <button
            type="button"
            className={`btn ${annotationKind === 'signature' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => setAnnotationKind('signature')}
            style={{ height: '32px', fontSize: '11.5px', gap: '4px' }}
          >
            <Award size={13} />
            <span>İmza</span>
          </button>
        </div>
      </div>

      {annotationKind === 'text' && (
        <div className="form-group">
          <label className="form-label">Not Metni</label>
          <input
            type="text"
            value={annotationText}
            onChange={(e) => setAnnotationText(e.target.value)}
            placeholder="Eklenecek notu yazın..."
            className="form-input"
          />
        </div>
      )}

      {annotationKind === 'signature' && (
        <div className="form-group">
          <label className="form-label">İmza Görseli</label>
          <div style={{ display: 'flex', gap: '6px' }}>
            <input
              type="text"
              value={signatureImagePath}
              onChange={(e) => setSignatureImagePath(e.target.value)}
              placeholder="İmza PNG/JPG dosyası..."
              className="form-input"
            />
            <button
              type="button"
              className="btn btn-secondary"
              onClick={handlePickSignatureImage}
              style={{ flexShrink: 0, padding: '0 8px' }}
            >
              <ImageIcon size={14} />
            </button>
          </div>
        </div>
      )}

      <div className="form-group">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '6px' }}>
          <label className="form-label" style={{ margin: 0 }}>
            Eklenen İşaretler ({annotations.length})
          </label>
          {annotations.length > 0 && (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={clearAnnotations}
              style={{ padding: '2px 6px', fontSize: '11px', color: 'var(--color-danger)' }}
            >
              Temizle
            </button>
          )}
        </div>

        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '4px',
            maxHeight: '140px',
            overflowY: 'auto',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-sm)',
            padding: '4px',
            backgroundColor: 'var(--color-bg)',
          }}
        >
          {annotations.length === 0 ? (
            <div style={{ padding: '12px', textAlign: 'center', fontSize: '12px', color: 'var(--color-text-subtle)' }}>
              Belge üzerine tıklayıp sürükleyerek işaret ekleyin.
            </div>
          ) : (
            annotations.map((ann, idx) => (
              <div
                key={idx}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '4px 8px',
                  backgroundColor: 'var(--color-surface)',
                  borderRadius: 'var(--radius-xs)',
                  fontSize: '11.5px',
                  border: '1px solid var(--color-border-subtle)',
                }}
              >
                <span>
                  Sayfa {ann.page} &bull; {ann.kind}
                </span>
                <button
                  type="button"
                  onClick={() => removeAnnotation(idx)}
                  className="btn btn-ghost"
                  style={{ padding: '2px', height: '20px', width: '20px', color: 'var(--color-danger)' }}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))
          )}
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
        disabled={annotations.length === 0 || !outputPath.trim() || isRunning}
        style={{ width: '100%', height: '34px', marginTop: '6px' }}
      >
        {isRunning ? 'Kaydediliyor...' : 'Anotasyonları PDF\'e Kaydet'}
      </button>
    </div>
  );
};
