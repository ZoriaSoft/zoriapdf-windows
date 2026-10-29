import React, { useEffect, useRef } from 'react';
import { Search, ChevronUp, ChevronDown, X, Loader2 } from 'lucide-react';
import { useDocument } from '../context/DocumentContext';
import { shouldNavigateSearch } from '../viewer/documentState';

export const SearchDrawer: React.FC = () => {
  const {
    searchOpen,
    setSearchOpen,
    searchQuery,
    setSearchQuery,
    searchHits,
    currentHitIndex,
    isSearching,
    searchError,
    executeSearch,
    nextSearchHit,
    prevSearchHit,
    setCurrentPage,
  } = useDocument();

  const inputRef = useRef<HTMLInputElement>(null);
  const lastSearchedRef = useRef<string>('');

  useEffect(() => {
    if (searchOpen && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [searchOpen]);


  if (!searchOpen) return null;

  const handleSearchSubmit = () => {
    const trimmed = searchQuery.trim();
    if (!trimmed) return;
    if (isSearching) return;
    lastSearchedRef.current = trimmed;
    executeSearch(trimmed);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    handleSearchSubmit();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      setSearchOpen(false);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (isSearching) return;
      if (shouldNavigateSearch(searchQuery, lastSearchedRef.current, searchHits.length)) {
        if (e.shiftKey) {
          prevSearchHit();
        } else {
          nextSearchHit();
        }
      } else {
        handleSearchSubmit();
      }
    }
  };

  return (
    <div
      style={{
        position: 'absolute',
        top: '48px',
        right: '24px',
        width: '320px',
        maxHeight: '420px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        boxShadow: 'var(--shadow-lg)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 100,
        overflow: 'hidden',
      }}
    >
      {/* Search Header */}
      <form
        onSubmit={handleSubmit}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '6px',
          padding: '8px 10px',
          borderBottom: '1px solid var(--color-border)',
        }}
      >
        <Search size={14} style={{ color: 'var(--color-text-subtle)', flexShrink: 0 }} />
        <input
          ref={inputRef}
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Belgede ara... (Enter)"
          style={{
            flex: 1,
            border: 'none',
            background: 'none',
            outline: 'none',
            fontSize: '12.5px',
            color: 'var(--color-text)',
          }}
        />

        {isSearching && (
          <Loader2
            size={14}
            className="animate-spin"
            style={{ color: 'var(--color-accent)', flexShrink: 0 }}
          />
        )}

        {searchHits.length > 0 && (
          <div
            style={{
              fontSize: '11px',
              color: 'var(--color-text-muted)',
              flexShrink: 0,
              padding: '0 4px',
            }}
          >
            {currentHitIndex + 1}/{searchHits.length}
          </div>
        )}

        <button
          type="button"
          onClick={prevSearchHit}
          disabled={isSearching || searchHits.length === 0}
          className="btn btn-ghost"
          style={{ padding: '2px', height: '24px', width: '24px' }}
          title="Önceki (Shift+Enter)"
        >
          <ChevronUp size={14} />
        </button>

        <button
          type="button"
          onClick={nextSearchHit}
          disabled={isSearching || searchHits.length === 0}
          className="btn btn-ghost"
          style={{ padding: '2px', height: '24px', width: '24px' }}
          title="Sonraki (Enter)"
        >
          <ChevronDown size={14} />
        </button>

        <button
          type="button"
          onClick={() => setSearchOpen(false)}
          className="btn btn-ghost"
          style={{ padding: '2px', height: '24px', width: '24px' }}
          title="Kapat (Esc)"
        >
          <X size={14} />
        </button>
      </form>

      {/* Hits List */}
      {searchError && <div role="alert" style={{ padding: 12, color: 'var(--color-danger)' }}>{searchError}</div>}
      <div style={{ maxHeight: '300px', overflowY: 'auto', padding: '4px' }}>
        {searchHits.length === 0 ? (
          <div
            style={{
              padding: '16px',
              textAlign: 'center',
              fontSize: '12px',
              color: 'var(--color-text-subtle)',
            }}
          >
            {searchQuery.trim()
              ? isSearching
                ? 'Belge taranıyor...'
                : 'Eşleşme bulunamadı.'
              : 'Aramak için bir kelime yazıp Enter tuşuna basın.'}
          </div>
        ) : (
          searchHits.map((hit, idx) => {
            const isSelected = idx === currentHitIndex;
            return (
              <div
                key={idx}
                onClick={() => {
                  setCurrentPage(hit.page);
                }}
                style={{
                  padding: '6px 8px',
                  borderRadius: 'var(--radius-xs)',
                  backgroundColor: isSelected
                    ? 'var(--color-accent-subtle)'
                    : 'transparent',
                  borderLeft: isSelected
                    ? '2px solid var(--color-accent)'
                    : '2px solid transparent',
                  cursor: 'pointer',
                  fontSize: '11.5px',
                  lineHeight: 1.4,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '2px',
                }}
              >
                <div
                  style={{
                    fontWeight: 600,
                    color: isSelected
                      ? 'var(--color-accent)'
                      : 'var(--color-text-muted)',
                  }}
                >
                  Sayfa {hit.page}
                </div>
                <div
                  style={{
                    color: 'var(--color-text)',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {hit.text}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
