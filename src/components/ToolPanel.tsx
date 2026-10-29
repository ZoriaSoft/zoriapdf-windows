import React from 'react';
import { X } from 'lucide-react';
import type { AnnotationKind } from '../types';
import { useOperation } from '../context/OperationContext';
import { MergeTool } from './tools/MergeTool';
import { SplitTool } from './tools/SplitTool';
import { ExtractTool } from './tools/ExtractTool';
import { RemoveTool } from './tools/RemoveTool';
import { ReorderTool } from './tools/ReorderTool';
import { RotateTool } from './tools/RotateTool';
import { WatermarkTool } from './tools/WatermarkTool';
import { NumberTool } from './tools/NumberTool';
import { ProtectTool } from './tools/ProtectTool';
import { UnlockTool } from './tools/UnlockTool';
import { CompressTool } from './tools/CompressTool';
import { ImagesToPdfTool } from './tools/ImagesToPdfTool';
import { PdfToImagesTool } from './tools/PdfToImagesTool';
import { RedactTool } from './tools/RedactTool';
import { AnnotateTool } from './tools/AnnotateTool';
import { ExtractTextTool } from './tools/ExtractTextTool';
import { OcrTool } from './tools/OcrTool';
import { PdfToWordTool } from './tools/PdfToWordTool';
import { WordToPdfTool } from './tools/WordToPdfTool';
import { ImageToWordTool } from './tools/ImageToWordTool';

interface ToolPanelProps {
  annotationKind: AnnotationKind;
  setAnnotationKind: (k: AnnotationKind) => void;
  annotationText: string;
  setAnnotationText: (t: string) => void;
  signatureImagePath: string;
  setSignatureImagePath: (p: string) => void;
}

const TOOL_TITLES: Record<string, string> = {
  'merge': 'PDF Belgelerini Birleştir',
  'split': 'PDF Belgesini Böl',
  'extract': 'Sayfaları Çıkar',
  'remove': 'Sayfaları Sil',
  'reorder': 'Sayfaları Yeniden Sırala',
  'rotate': 'Sayfaları Döndür',
  'watermark': 'Filigran Ekle',
  'number': 'Sayfa Numaralandır',
  'protect': 'Şifre Belirle & Koru',
  'unlock': 'Şifre Korumasını Kaldır',
  'compress': 'Kayıpsız Sıkıştır',
  'images-to-pdf': 'Görsellerden PDF Yap',
  'pdf-to-images': 'PDF\'ten Görsellere',
  'redact': 'Kalıcı Sansür (Redaksiyon)',
  'annotate': 'Anotasyon & İşaretleme',
  'extract-text': 'Metin Katmanını Ayıkla',
  'ocr': 'Yerel OCR Metin Tanıma',
  'pdf-to-word': 'PDF\'ten Word\'e',
  'word-to-pdf': 'Word\'den PDF\'e',
  'image-to-word': 'Görselden Word\'e',
};

export const ToolPanel: React.FC<ToolPanelProps> = ({
  annotationKind,
  setAnnotationKind,
  annotationText,
  setAnnotationText,
  signatureImagePath,
  setSignatureImagePath,
}) => {
  const { activeTool, setActiveTool } = useOperation();

  if (!activeTool) return null;

  const renderToolBody = () => {
    switch (activeTool) {
      case 'merge':
        return <MergeTool />;
      case 'split':
        return <SplitTool />;
      case 'extract':
        return <ExtractTool />;
      case 'remove':
        return <RemoveTool />;
      case 'reorder':
        return <ReorderTool />;
      case 'rotate':
        return <RotateTool />;
      case 'watermark':
        return <WatermarkTool />;
      case 'number':
        return <NumberTool />;
      case 'protect':
        return <ProtectTool />;
      case 'unlock':
        return <UnlockTool />;
      case 'compress':
        return <CompressTool />;
      case 'images-to-pdf':
        return <ImagesToPdfTool />;
      case 'pdf-to-images':
        return <PdfToImagesTool />;
      case 'redact':
        return <RedactTool />;
      case 'annotate':
        return (
          <AnnotateTool
            annotationKind={annotationKind}
            setAnnotationKind={setAnnotationKind}
            annotationText={annotationText}
            setAnnotationText={setAnnotationText}
            signatureImagePath={signatureImagePath}
            setSignatureImagePath={setSignatureImagePath}
          />
        );
      case 'extract-text':
        return <ExtractTextTool />;
      case 'ocr':
        return <OcrTool />;
      case 'pdf-to-word':
        return <PdfToWordTool />;
      case 'word-to-pdf':
        return <WordToPdfTool />;
      case 'image-to-word':
        return <ImageToWordTool />;
      default:
        return null;
    }
  };

  return (
    <aside
      style={{
        width: '320px',
        backgroundColor: 'var(--color-surface)',
        borderLeft: '1px solid var(--color-border)',
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        userSelect: 'none',
        flexShrink: 0,
        overflow: 'hidden',
      }}
    >
      {/* Tool header */}
      <div
        style={{
          height: '42px',
          padding: '0 14px',
          borderBottom: '1px solid var(--color-border)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexShrink: 0,
        }}
      >
        <span style={{ fontWeight: 600, fontSize: '13px', color: 'var(--color-text)' }}>
          {TOOL_TITLES[activeTool] || activeTool}
        </span>
        <button
          type="button"
          onClick={() => setActiveTool(null)}
          className="btn btn-ghost"
          style={{ padding: '2px', height: '24px', width: '24px' }}
          title="Paneli Kapat"
        >
          <X size={15} />
        </button>
      </div>

      {/* Tool Form Content */}
      <div style={{ flex: 1, padding: '16px', overflowY: 'auto' }}>
        {renderToolBody()}
      </div>
    </aside>
  );
};
