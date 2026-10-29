import React from 'react';
import {
  Layers,
  Scissors,
  FileMinus,
  FilePlus,
  ArrowUpDown,
  RotateCw,
  EyeOff,
  ShieldCheck,
  Unlock,
  Stamp,
  Highlighter,
  Hash,
  Minimize2,
  Image,
  Images,
  FileCode,
  ScanText,
  FileType2,
  FileText,
  FileImage,
} from 'lucide-react';
import type { OperationKind } from '../types';
import { useOperation } from '../context/OperationContext';

interface ToolItem {
  kind: OperationKind;
  label: string;
  icon: React.ReactNode;
  category: string;
}

const TOOLS: ToolItem[] = [
  // Organize
  { kind: 'merge', label: 'Birleştir', icon: <Layers size={16} />, category: 'Düzenle' },
  { kind: 'split', label: 'Böl', icon: <Scissors size={16} />, category: 'Düzenle' },
  { kind: 'extract', label: 'Sayfa Çıkar', icon: <FilePlus size={16} />, category: 'Düzenle' },
  { kind: 'remove', label: 'Sayfa Sil', icon: <FileMinus size={16} />, category: 'Düzenle' },
  { kind: 'reorder', label: 'Yeniden Sırala', icon: <ArrowUpDown size={16} />, category: 'Düzenle' },
  { kind: 'rotate', label: 'Döndür', icon: <RotateCw size={16} />, category: 'Düzenle' },

  // Privacy & Protection
  { kind: 'redact', label: 'Redaksiyon', icon: <EyeOff size={16} />, category: 'Güvenlik' },
  { kind: 'protect', label: 'Şifre Belirle', icon: <ShieldCheck size={16} />, category: 'Güvenlik' },
  { kind: 'unlock', label: 'Şifre Çöz', icon: <Unlock size={16} />, category: 'Güvenlik' },
  { kind: 'watermark', label: 'Filigran', icon: <Stamp size={16} />, category: 'Güvenlik' },

  // Annotate
  { kind: 'annotate', label: 'Anotasyon', icon: <Highlighter size={16} />, category: 'İşaretle' },
  { kind: 'number', label: 'Numaralandır', icon: <Hash size={16} />, category: 'İşaretle' },

  // Convert & Optimize
  { kind: 'compress', label: 'Sıkıştır', icon: <Minimize2 size={16} />, category: 'Dönüştür' },
  { kind: 'images-to-pdf', label: 'Görsel → PDF', icon: <Images size={16} />, category: 'Dönüştür' },
  { kind: 'pdf-to-images', label: 'PDF → Görsel', icon: <Image size={16} />, category: 'Dönüştür' },
  { kind: 'extract-text', label: 'Metin Çıkar', icon: <FileCode size={16} />, category: 'Dönüştür' },
  { kind: 'ocr', label: 'Yerel OCR', icon: <ScanText size={16} />, category: 'Dönüştür' },
  { kind: 'pdf-to-word', label: 'PDF → Word', icon: <FileType2 size={16} />, category: 'Dönüştür' },
  { kind: 'word-to-pdf', label: 'Word → PDF', icon: <FileText size={16} />, category: 'Dönüştür' },
  { kind: 'image-to-word', label: 'Görsel → Word', icon: <FileImage size={16} />, category: 'Dönüştür' },
];

export const ToolRail: React.FC = () => {
  const { activeTool, setActiveTool } = useOperation();

  const categories = ['Düzenle', 'Güvenlik', 'İşaretle', 'Dönüştür'];

  return (
    <aside
      style={{
        width: '160px',
        backgroundColor: 'var(--color-surface)',
        borderRight: '1px solid var(--color-border)',
        display: 'flex',
        flexDirection: 'column',
        userSelect: 'none',
        flexShrink: 0,
        overflowY: 'auto',
      }}
    >
      <div style={{ padding: '8px 6px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
        {categories.map((cat) => {
          const catTools = TOOLS.filter((t) => t.category === cat);
          return (
            <div key={cat} style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
              <div
                style={{
                  padding: '2px 8px',
                  fontSize: '10px',
                  fontWeight: 700,
                  textTransform: 'uppercase',
                  letterSpacing: '0.6px',
                  color: 'var(--color-text-subtle)',
                }}
              >
                {cat}
              </div>
              {catTools.map((tool) => {
                const isSelected = activeTool === tool.kind;
                return (
                  <button
                    key={tool.kind}
                    type="button"
                    onClick={() => setActiveTool(isSelected ? null : tool.kind)}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                      padding: '6px 8px',
                      borderRadius: 'var(--radius-sm)',
                      border: 'none',
                      backgroundColor: isSelected
                        ? 'var(--color-accent)'
                        : 'transparent',
                      color: isSelected
                        ? 'var(--color-accent-text)'
                        : 'var(--color-text)',
                      fontSize: '12px',
                      fontWeight: isSelected ? 600 : 500,
                      cursor: 'pointer',
                      textAlign: 'left',
                      transition: 'background-color var(--transition-fast), color var(--transition-fast)',
                    }}
                    onMouseEnter={(e) => {
                      if (!isSelected) {
                        (e.currentTarget as HTMLElement).style.backgroundColor =
                          'var(--color-surface-hover)';
                      }
                    }}
                    onMouseLeave={(e) => {
                      if (!isSelected) {
                        (e.currentTarget as HTMLElement).style.backgroundColor =
                          'transparent';
                      }
                    }}
                  >
                    <span
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        color: isSelected ? 'inherit' : 'var(--color-accent)',
                      }}
                    >
                      {tool.icon}
                    </span>
                    <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {tool.label}
                    </span>
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
    </aside>
  );
};
