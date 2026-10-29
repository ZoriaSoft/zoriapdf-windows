import React, { useCallback, useEffect, useState } from 'react';
import { FolderOpen, ScanText, Info, AlertCircle, Download, CheckCircle2 } from 'lucide-react';
import { useDocument } from '../../context/DocumentContext';
import { useOperation } from '../../context/OperationContext';
import { apiOcrStatus, pickDirectory, type OcrStatus } from '../../bridge/tauri';

export const OcrTool: React.FC = () => {
  const { activeTab } = useDocument();
  const { executeOperation, isRunning, errorMessage } = useOperation();

  const [pages, setPages] = useState<string>('');
  const [outputDir, setOutputDir] = useState<string>('');
  const [status, setStatus] = useState<OcrStatus | null>(null);
  const [installing, setInstalling] = useState(false);

  const refreshStatus = useCallback(async () => {
    const next = await apiOcrStatus();
    setStatus(next);
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const handlePickDir = async () => {
    const selected = await pickDirectory('OCR Çıktılarının Kaydedileceği Klasör');
    if (selected) {
      setOutputDir(selected);
    }
  };

  const handleInstall = async () => {
    if (!status?.pack?.url) return;
    setInstalling(true);
    const result = await executeOperation('install-ocr', {
      inputs: [],
      output: 'ocr-component',
      downloadUrl: status.pack.url,
      sha256: status.pack.sha256,
    });
    setInstalling(false);
    if (result) {
      await refreshStatus();
    }
  };

  const handleRun = async () => {
    if (!activeTab || !outputDir.trim()) return;
    await executeOperation('ocr', {
      inputs: [activeTab.info.path],
      output: outputDir.trim(),
      pages: pages.trim() ? pages.trim() : undefined,
    });
  };

  const ocrReady = status?.available === true;

  const isPrerequisiteError =
    errorMessage &&
    (errorMessage.toLowerCase().includes('ocr') ||
      errorMessage.toLowerCase().includes('tesseract') ||
      errorMessage.toLowerCase().includes('prerequisite') ||
      errorMessage.toLowerCase().includes('önkoşul') ||
      errorMessage.toLowerCase().includes('bulunamadı') ||
      errorMessage.toLowerCase().includes('not found'));

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
      <div className="form-help">
        Taranmış veya görsel tabanlı PDF sayfalarını yerel OCR motoru ile işleyip aranabilir metne ve çıktılara dönüştürür.
        {!activeTab && ' OCR bileşenini belge açmadan da kurabilirsiniz.'}
      </div>

      {!status && <div className="form-help" role="status">OCR durumu alınamadı veya yükleniyor. <button type="button" className="btn btn-secondary" onClick={() => void refreshStatus()}>Yeniden Dene</button></div>}
      {!ocrReady && status && (
        <div
          style={{
            padding: '12px',
            backgroundColor: 'var(--color-accent-subtle)',
            border: '1px solid var(--color-accent)',
            borderRadius: 'var(--radius-sm)',
            display: 'flex',
            flexDirection: 'column',
            gap: '8px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, fontSize: '12.5px' }}>
            <Download size={14} style={{ color: 'var(--color-accent)' }} />
            <span>Yerel OCR Bileşeni Kurulu Değil</span>
          </div>
          <p style={{ fontSize: '11.5px', color: 'var(--color-text-muted)', lineHeight: 1.45 }}>
            OCR motoru isteğe bağlı bir bileşendir; kurulum paketi küçük kalır. Tek seferlik indirme
            (~{status.pack.sizeMb} MB) ile Türkçe + İngilizce tanıma motoru kurulur, sonrasında tamamen
            çevrimdışı çalışır. Tamamen çevrimdışı çalışmak isteyenler "Offline" kurulum paketini seçebilir.
          </p>
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleInstall}
            disabled={installing || isRunning}
            style={{ width: '100%', height: '32px' }}
          >
            <Download size={14} />
            <span>
              {installing ? 'Kuruluyor...' : `OCR Bileşenini İndir ve Kur (${status.pack.sizeMb} MB)`}
            </span>
          </button>
        </div>
      )}

      {ocrReady && (
        <div
          style={{
            padding: '10px 12px',
            backgroundColor: 'var(--color-surface)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-sm)',
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            fontSize: '11.5px',
            color: 'var(--color-text-muted)',
          }}
        >
          <CheckCircle2 size={14} style={{ color: 'var(--color-success)' }} />
          <span>Yerel OCR hazır (Türkçe + İngilizce). Veriler cihazdan çıkmaz.</span>
        </div>
      )}

      {isPrerequisiteError && ocrReady && (
        <div
          style={{
            padding: '10px 12px',
            backgroundColor: 'var(--color-danger-subtle)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-sm)',
            display: 'flex',
            flexDirection: 'column',
            gap: '4px',
            color: 'var(--color-danger)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, fontSize: '12px' }}>
            <AlertCircle size={14} />
            <span>OCR Önkoşul Hatası</span>
          </div>
          <div style={{ fontSize: '11.5px', lineHeight: 1.4 }}>{errorMessage}</div>
        </div>
      )}

      <div
        style={{
          padding: '10px 12px',
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          borderRadius: 'var(--radius-sm)',
          display: 'flex',
          flexDirection: 'column',
          gap: '6px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, fontSize: '12px', color: 'var(--color-text)' }}>
          <Info size={14} style={{ color: 'var(--color-accent)' }} />
          <span>Yerel OCR Önkoşulu</span>
        </div>
        <p style={{ fontSize: '11.5px', color: 'var(--color-text-muted)', lineHeight: 1.45 }}>
          OCR işlemi harici sunucuya veri göndermez; kurulu yerel motoru kullanır.
        </p>
      </div>

      {activeTab && <>
      <div className="form-group">
        <label className="form-label">Sayfalar (İsteğe Bağlı)</label>
        <input
          type="text"
          value={pages}
          onChange={(e) => setPages(e.target.value)}
          placeholder={`Tüm sayfalar (veya örn: 1, 3-5)`}
          className="form-input"
        />
        <div style={{ display: 'flex', gap: '6px', marginTop: '4px' }}>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setPages(String(activeTab.currentPage))}
            style={{ padding: '2px 6px', fontSize: '11px', height: '22px' }}
          >
            Yalnızca Aktif Sayfa ({activeTab.currentPage})
          </button>
        </div>
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
            onClick={handlePickDir}
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
        disabled={!outputDir.trim() || isRunning || !ocrReady}
        style={{ width: '100%', height: '34px', marginTop: '6px', gap: '6px' }}
        title={ocrReady ? undefined : 'Yerel OCR bileşeni kurulmalı'}
      >
        <ScanText size={15} />
        <span>{isRunning ? 'OCR Taranıyor...' : 'Yerel OCR Başlat'}</span>
      </button>
      </>}
    </div>
  );
};
