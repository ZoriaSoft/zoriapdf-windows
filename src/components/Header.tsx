import React from 'react';
import {
  FolderOpen,
  Printer,
  Search,
  Sun,
  Moon,
  Plus,
  X,
  FileText,
  Lock,
  ZapOff,
  Zap,
  AlertTriangle,
  History,
} from 'lucide-react';
import { displayPath } from '../viewer/renderQueue';
import { useDocument } from '../context/DocumentContext';
import { useTheme } from '../context/ThemeContext';

export const Header: React.FC = () => {
  const {
    tabs,
    selectTab,
    closeTab,
    openFilesDialog,
    printCurrentDocument,
    toggleSearch,
    activeTab,
    bridgeAvailable,
    recentPaths,
    openPaths,
    clearRecentPaths,
  } = useDocument();
  const [recentOpen, setRecentOpen] = React.useState(false);

  const { theme, toggleTheme, reducedMotion, toggleReducedMotion } = useTheme();

  return (
    <header
      style={{
        height: '46px',
        backgroundColor: 'var(--color-surface)',
        borderBottom: '1px solid var(--color-border)',
        display: 'flex',
        alignItems: 'center',
        padding: '0 12px',
        gap: '12px',
        userSelect: 'none',
        flexShrink: 0,
      }}
    >
      {/* Brand logo & title */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '8px',
          fontWeight: 700,
          fontSize: '14px',
          letterSpacing: '-0.3px',
          color: 'var(--color-text)',
          minWidth: '120px',
        }}
      >
        <div
          style={{
            width: '24px',
            height: '24px',
            backgroundColor: 'var(--color-accent)',
            borderRadius: 'var(--radius-xs)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: 'var(--color-accent-text)',
            fontSize: '12px',
            fontWeight: 800,
          }}
        >
          Z
        </div>
        <span>ZoriaPDF</span>
      </div>

      {/* Tabs list */}
      <div
        role="tablist"
        aria-label="Açık belgeler"
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '4px',
          flex: 1,
          overflowX: 'auto',
          overflowY: 'hidden',
          height: '100%',
          padding: '4px 0',
        }}
      >
        {tabs.map((tab, idx) => {
          const isActive = tab.info.id === activeTab?.info.id;
          return (
            <div
              key={tab.info.id}
              role="presentation"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                padding: '4px 10px',
                backgroundColor: isActive
                  ? 'var(--color-bg)'
                  : 'transparent',
                border: `1px solid ${isActive ? 'var(--color-border)' : 'transparent'}`,
                borderBottom: isActive ? '1px solid transparent' : 'none',
                borderRadius: 'var(--radius-sm)',
                cursor: 'pointer',
                fontSize: '12.5px',
                color: isActive ? 'var(--color-text)' : 'var(--color-text-muted)',
                fontWeight: isActive ? 600 : 400,
                maxWidth: '200px',
                minWidth: '110px',
                transition: 'background-color var(--transition-fast)',
              }}
              title={tab.info.path}
            >
              <button
                id={`document-tab-${tab.info.id}`}
                type="button"
                role="tab"
                aria-selected={isActive}
                aria-controls="document-panel"
                aria-label={tab.info.name}
                tabIndex={isActive ? 0 : -1}
                onClick={() => selectTab(tab.info.id)}
                onKeyDown={(event) => {
                  let next: number;
                  switch (event.key) {
                    case 'ArrowLeft': next = (idx - 1 + tabs.length) % tabs.length; break;
                    case 'ArrowRight': next = (idx + 1) % tabs.length; break;
                    case 'Home': next = 0; break;
                    case 'End': next = tabs.length - 1; break;
                    default: return;
                  }
                  event.preventDefault();
                  event.stopPropagation();
                  selectTab(tabs[next].info.id);
                  document.getElementById(`document-tab-${tabs[next].info.id}`)?.focus();
                }}
                style={{ display: 'flex', alignItems: 'center', gap: 6, flex: 1, minWidth: 0,
                  padding: 0, background: 'none', border: 'none', cursor: 'pointer',
                  color: 'inherit', font: 'inherit' }}
              >
              <FileText size={13} style={{ flexShrink: 0 }} />
              {tab.sourcePassword && (
                <Lock size={11} style={{ flexShrink: 0, color: 'var(--color-accent)' }} />
              )}
              <span
                style={{
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  flex: 1,
                }}
              >
                {tab.info.name}
              </span>
              <span
                style={{
                  fontSize: '10.5px',
                  color: 'var(--color-text-subtle)',
                  padding: '1px 4px',
                  borderRadius: '3px',
                  backgroundColor: 'var(--color-surface-hover)',
                  flexShrink: 0,
                }}
              >
                {tab.info.pages}p
              </span>
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.info.id);
                }}
                style={{
                  background: 'none',
                  border: 'none',
                  padding: '2px',
                  cursor: 'pointer',
                  display: 'flex',
                  alignItems: 'center',
                  borderRadius: '3px',
                  color: 'var(--color-text-subtle)',
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.color = 'var(--color-danger)';
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.color = 'var(--color-text-subtle)';
                }}
                title="Sekmeyi Kapat"
                aria-label={`${tab.info.name} sekmesini kapat`}
              >
                <X size={12} />
              </button>
            </div>
          );
        })}

        <button
          type="button"
          onClick={openFilesDialog}
          className="btn btn-ghost"
          style={{ padding: '4px 6px', height: '28px' }}
          title="Yeni PDF Aç (Ctrl+O)"
        >
          <Plus size={14} />
        </button>
      </div>

      {/* Header controls & tools */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexShrink: 0 }}>
        {!bridgeAvailable && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '4px',
              padding: '3px 8px',
              backgroundColor: 'var(--color-warning-subtle)',
              color: 'var(--color-warning)',
              borderRadius: 'var(--radius-sm)',
              fontSize: '11px',
              fontWeight: 600,
            }}
            title="Tauri yerel masaüstü köprüsü algılanamadı. Native çalışma alanı gerekir."
          >
            <AlertTriangle size={13} />
            <span>Tarayıcı Modu</span>
          </div>
        )}

        <button
          type="button"
          onClick={openFilesDialog}
          className="btn btn-secondary"
          style={{ height: '30px', padding: '0 8px', fontSize: '12px' }}
          title="PDF Aç (Ctrl+O)"
        >
          <FolderOpen size={14} />
          <span>Aç</span>
        </button>

        <div style={{ position: 'relative' }}>
          <button type="button" className="btn btn-ghost" onClick={() => setRecentOpen((v) => !v)}
            disabled={recentPaths.length === 0} aria-haspopup="menu" aria-expanded={recentOpen}
            style={{ height: '30px', padding: '0 6px' }} title="Son Açılanlar" aria-label="Son açılan belgeler">
            <History size={14} />
          </button>
          {recentOpen && recentPaths.length > 0 && (
            <>
              <div style={{ position: 'fixed', inset: 0, zIndex: 90 }} onClick={() => setRecentOpen(false)} />
              <div role="menu" onKeyDown={(e) => e.key === 'Escape' && setRecentOpen(false)}
                style={{ position: 'absolute', right: 0, top: 34, zIndex: 100, width: 360, maxHeight: 420, overflowY: 'auto',
                  background: 'var(--color-surface)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-md)',
                  boxShadow: 'var(--shadow-sm)', padding: 6 }}>
                {recentPaths.map((path) => {
                  const shown = displayPath(path);
                  return (
                    <button key={path} type="button" role="menuitem" className="btn btn-ghost" title={shown}
                      onClick={() => { setRecentOpen(false); openPaths([path]); }}
                      style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', width: '100%', height: 'auto', padding: '6px 8px', textAlign: 'left' }}>
                      <span style={{ fontSize: 12.5, fontWeight: 600, maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{shown.split(/[/\\]/).pop()}</span>
                      <span style={{ fontSize: 11, color: 'var(--color-text-subtle)', maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{shown}</span>
                    </button>
                  );
                })}
                <button type="button" role="menuitem" className="btn btn-ghost" onClick={() => { clearRecentPaths(); setRecentOpen(false); }}
                  style={{ width: '100%', justifyContent: 'center', fontSize: 12, marginTop: 4, borderTop: '1px solid var(--color-border)' }}>
                  Listeyi temizle
                </button>
              </div>
            </>
          )}
        </div>

        <button
          type="button"
          onClick={printCurrentDocument}
          disabled={!activeTab}
          className="btn btn-ghost"
          style={{ height: '30px', padding: '0 8px', fontSize: '12px' }}
          title="Yazdır (Ctrl+P)"
        >
          <Printer size={14} />
        </button>

        <button
          type="button"
          onClick={toggleSearch}
          disabled={!activeTab}
          className="btn btn-ghost"
          style={{ height: '30px', padding: '0 8px', fontSize: '12px' }}
          title="Belgede Ara (Ctrl+F)"
        >
          <Search size={14} />
        </button>

        <div style={{ width: '1px', height: '18px', backgroundColor: 'var(--color-border)', margin: '0 2px' }} />

        <button
          type="button"
          onClick={toggleReducedMotion}
          className="btn btn-ghost"
          style={{ height: '30px', padding: '0 8px' }}
          title={reducedMotion ? 'Animasyonları Etkinleştir' : 'Hareketleri Azalt (Reduced Motion)'}
        >
          {reducedMotion ? <ZapOff size={14} style={{ color: 'var(--color-warning)' }} /> : <Zap size={14} />}
        </button>

        <button
          type="button"
          onClick={toggleTheme}
          className="btn btn-ghost"
          style={{ height: '30px', padding: '0 8px' }}
          title={theme === 'origami' ? 'Folio Noir Temasına Geç' : 'Origami Temasına Geç'}
        >
          {theme === 'origami' ? <Moon size={14} /> : <Sun size={14} />}
        </button>
      </div>
    </header>
  );
};
