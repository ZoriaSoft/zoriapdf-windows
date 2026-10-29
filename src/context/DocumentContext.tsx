import React, {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useState,
  useCallback,
  useRef,
} from 'react';
import type {
  DocumentInfo,
  RenderedPage,
  SearchHit,
  RedactRect,
  Annotation,
  TabState,
} from '../types';
import { displayPath, pushRecent } from '../viewer/renderQueue';
import { forgetDocument } from '../viewer/lazyRender';
import { closeTabState, reopenTabState, renderRevision } from '../viewer/documentState';
import {
  apiOpenDocuments,
  apiCloseDocument,
  apiRenderPage,
  apiPageText,
  apiSearchDocument,
  apiInitialPaths,
  apiPrintDocument,
  pickPdfFiles,
  listenOpenPaths,
  setupNativeDragDrop,
  isTauriAvailable,
} from '../bridge/tauri';

interface PasswordPromptData {
  path: string;
  isOpen: boolean;
  error?: string;
  remainingPaths?: string[];
}

interface DocumentContextValue {
  tabs: TabState[];
  activeTab: TabState | null;
  activeRenderedPage: RenderedPage | null;
  isRendering: boolean;
  recentPaths: string[];
  removeRecentPath: (path: string) => void;
  clearRecentPaths: () => void;
  renderError: string | null;
  retryRender: () => void;
  passwordPrompt: PasswordPromptData;
  searchOpen: boolean;
  searchQuery: string;
  searchHits: SearchHit[];
  currentHitIndex: number;
  isSearching: boolean;
  searchError: string | null;
  bridgeAvailable: boolean;

  // Actions
  openFilesDialog: () => Promise<void>;
  openPaths: (paths: string[], password?: string) => Promise<boolean>;
  closeTab: (docId: string) => Promise<void>;
  selectTab: (docId: string) => void;
  setCurrentPage: (page: number) => void;
  nextPage: () => void;
  prevPage: () => void;
  setZoom: (zoom: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;
  printCurrentDocument: () => Promise<void>;
  setSearchOpen: (open: boolean) => void;
  toggleSearch: () => void;
  setSearchQuery: (query: string) => void;
  executeSearch: (query: string) => Promise<void>;
  nextSearchHit: () => void;
  prevSearchHit: () => void;
  fetchPageText: (page?: number) => Promise<string>;
  submitPasswordPrompt: (password: string) => Promise<void>;
  cancelPasswordPrompt: () => void;

  // Redaction & Annotation
  addRedaction: (rect: RedactRect) => void;
  removeRedaction: (index: number) => void;
  clearRedactions: () => void;
  addAnnotation: (ann: Annotation) => void;
  removeAnnotation: (index: number) => void;
  clearAnnotations: () => void;
}

const DocumentContext = createContext<DocumentContextValue | null>(null);

const MAX_CACHED_PAGES = 6;
const MAX_RECENT_PATHS = 10;

export const DocumentProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [{ tabs, activeId }, setDocuments] = useState<{ tabs: TabState[]; activeId: string | null }>({
    tabs: [], activeId: null,
  });
  const [activeRenderedPage, setActiveRenderedPage] = useState<RenderedPage | null>(null);
  const [isRendering, setIsRendering] = useState<boolean>(false);
  const [renderError, setRenderError] = useState<string | null>(null);
  const [renderRetry, setRenderRetry] = useState(0);
  const retryRender = useCallback(() => setRenderRetry((n) => n + 1), []);
  const [bridgeAvailable] = useState<boolean>(isTauriAvailable());

  const [recentPaths, setRecentPaths] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem('zoriapdf_recent_paths');
      const parsed: unknown = raw ? JSON.parse(raw) : [];
      // Older builds stored Windows verbatim paths (\\?\C:\...); normalize once.
      return Array.isArray(parsed) ? pushRecent([], parsed.filter((p): p is string => typeof p === 'string'), MAX_RECENT_PATHS) : [];
    } catch {
      return [];
    }
  });

  const [passwordPrompt, setPasswordPrompt] = useState<PasswordPromptData>({
    path: '',
    isOpen: false,
  });

  // Search state
  const [searchOpen, setSearchOpen] = useState<boolean>(false);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [currentHitIndex, setCurrentHitIndex] = useState<number>(-1);
  const [isSearching, setIsSearching] = useState<boolean>(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const searchSeqRef = useRef(0);

  // The explicit-open generation prevents old responses/cache entries from
  // crossing a reopen, including replacements that preserve size and mtime.
  const renderCacheRef = useRef<Map<string, RenderedPage>>(new Map());
  const renderSeqRef = useRef<number>(0);

  const activeTab = tabs.find((tab) => tab.info.id === activeId) ?? null;
  const activeRevision = activeTab ? renderRevision(activeTab) : '';
  const updateActiveTab = useCallback((update: (tab: TabState) => TabState) => {
    setDocuments((previous) => {
      if (previous.activeId !== activeId) return previous;
      const index = previous.tabs.findIndex((tab) => tab.info.id === activeId);
      if (index < 0 || renderRevision(previous.tabs[index]) !== activeRevision) return previous;
      const tab = update(previous.tabs[index]);
      if (tab === previous.tabs[index]) return previous;
      const nextTabs = [...previous.tabs];
      nextTabs[index] = tab;
      return { ...previous, tabs: nextTabs };
    });
  }, [activeId, activeRevision]);
  useEffect(() => {
    ++searchSeqRef.current;
    setSearchHits([]);
    setCurrentHitIndex(-1);
    setIsSearching(false);
    setSearchError(null);
  }, [activeId, activeRevision]);

  const storeRecent = (updated: string[]) => {
    try {
      localStorage.setItem('zoriapdf_recent_paths', JSON.stringify(updated));
    } catch (e) {
      console.warn('Failed to save recent paths', e);
    }
  };
  const removeRecentPath = useCallback((path: string) => {
    setRecentPaths((prev) => {
      const target = displayPath(path).toLowerCase();
      const updated = prev.filter((p) => displayPath(p).toLowerCase() !== target);
      storeRecent(updated);
      return updated;
    });
  }, []);
  const clearRecentPaths = useCallback(() => {
    setRecentPaths([]);
    storeRecent([]);
  }, []);

  const saveRecentPath = useCallback((pathsToAdd: string[]) => {
    setRecentPaths((prev) => {
      const updated = pushRecent(prev, pathsToAdd, MAX_RECENT_PATHS);
      try {
        localStorage.setItem('zoriapdf_recent_paths', JSON.stringify(updated));
      } catch (e) {
        console.warn('Failed to save recent paths', e);
      }
      return updated;
    });
  }, []);

  const openPaths = useCallback(
    async (paths: string[], password?: string): Promise<boolean> => {
      if (!paths || paths.length === 0) return false;
      const pdfPaths = paths.filter((p) => p.toLowerCase().endsWith('.pdf') || !p.includes('.'));
      if (pdfPaths.length === 0) return false;

      try {
        const docs = await apiOpenDocuments(pdfPaths, password);
        if (!docs || docs.length === 0) return false;

        ++renderSeqRef.current;
        for (const doc of docs) {
          forgetDocument(doc.id);
          for (const key of renderCacheRef.current.keys()) {
            if (key.startsWith(`${doc.id}_`)) renderCacheRef.current.delete(key);
          }
        }
        setDocuments((previous) => ({
          tabs: reopenTabState(previous.tabs, docs, password),
          activeId: docs[0].id,
        }));

        saveRecentPath(docs.map((d) => d.path));
        return true;
      } catch (err: unknown) {
        const errMessage = String(err || '');
        let failedPath = pdfPaths[0];
        try {
          const detail = JSON.parse(errMessage) as { path?: string; message?: string };
          if (detail.path) failedPath = detail.path;
        } catch { /* Other bridge errors are plain strings. */ }
        if (/bulunamadı|not found/i.test(errMessage)) {
          // A moved or deleted file should not stay in the recent list.
          removeRecentPath(failedPath);
        }
        if (
          errMessage.toLowerCase().includes('password') ||
          errMessage.toLowerCase().includes('şifre') ||
          errMessage.toLowerCase().includes('parola') ||
          errMessage.toLowerCase().includes('encrypted') ||
          errMessage.toLowerCase().includes('kilitli')
        ) {
          // Open password modal for the first failing path
          setPasswordPrompt((previous) => ({
            path: failedPath,
            remainingPaths: pdfPaths.length > 1
              ? pdfPaths.filter((path) => path !== failedPath)
              : previous.path === failedPath ? previous.remainingPaths : [],
            isOpen: true,
            error: password ? 'Hatalı şifre, lütfen tekrar deneyin.' : undefined,
          }));
          return false;
        } else {
          console.error('Failed to open documents:', err);
          alert(`Belge açılamadı: ${errMessage}`);
          return false;
        }
      }
    },
    [saveRecentPath, removeRecentPath]
  );

  const openFilesDialog = useCallback(async () => {
    try {
      const selected = await pickPdfFiles(true);
      if (selected && selected.length > 0) {
        await openPaths(selected);
      }
    } catch (err) {
      console.error('File dialog error:', err);
    }
  }, [openPaths]);

  const closeTab = useCallback(async (docId: string) => {
    try {
      await apiCloseDocument(docId);
    } catch (error) {
      console.warn('Belge kapatılamadı:', error);
      return;
    }
    forgetDocument(docId);
    for (const key of renderCacheRef.current.keys()) {
      if (key.startsWith(`${docId}_`)) renderCacheRef.current.delete(key);
    }
    setDocuments((previous) => closeTabState(previous.tabs, previous.activeId, docId));
  }, []);

  const selectTab = useCallback((docId: string) => {
    setDocuments((previous) => previous.tabs.some((tab) => tab.info.id === docId)
      ? { ...previous, activeId: docId } : previous);
  }, []);

  // Render current page safely
  useLayoutEffect(() => {
    const currentSeq = ++renderSeqRef.current;
    setRenderError(null);
    if (!activeTab) {
      setActiveRenderedPage(null);
      setIsRendering(false);
      return;
    }

    const { id: docId } = activeTab.info;
    const page = activeTab.currentPage;
    const zoom = activeTab.zoom;
    // Base render width ~ 1000px scaled by zoom and bounded to engine limit (100..2400)
    const renderWidth = Math.round(Math.min(2400, Math.max(400, 1080 * zoom)));
    const cacheKey = `${docId}_${activeRevision}_${page}_${renderWidth}`;

    const cachedPage = renderCacheRef.current.get(cacheKey);
    if (cachedPage) {
      // LRU: re-insert so recently viewed pages are evicted last.
      renderCacheRef.current.delete(cacheKey);
      renderCacheRef.current.set(cacheKey, cachedPage);
      setActiveRenderedPage(cachedPage);
      setIsRendering(false);
      return;
    }

    setActiveRenderedPage(null);
    setIsRendering(true);

    apiRenderPage(docId, page, renderWidth)
      .then((rendered) => {
        // Discard stale response
        if (renderSeqRef.current !== currentSeq) return;

        // Manage cache size
        if (renderCacheRef.current.size >= MAX_CACHED_PAGES) {
          const firstKey = renderCacheRef.current.keys().next().value;
          if (firstKey) renderCacheRef.current.delete(firstKey);
        }
        renderCacheRef.current.set(cacheKey, rendered);
        setActiveRenderedPage(rendered);
      })
      .catch((err) => {
        if (renderSeqRef.current !== currentSeq) return;
        console.error(`Page ${page} render error:`, err);
        setRenderError(String(err));
      })
      .finally(() => {
        if (renderSeqRef.current === currentSeq) {
          setIsRendering(false);
        }
      });
    return () => { ++renderSeqRef.current; };
  }, [activeId, activeRevision, activeTab?.currentPage, activeTab?.zoom, renderRetry]);

  const setCurrentPage = useCallback((page: number) => {
    updateActiveTab((tab) => {
      const bounded = Math.max(1, Math.min(tab.info.pages, page));
      return bounded === tab.currentPage ? tab : { ...tab, currentPage: bounded };
    });
  }, [updateActiveTab]);

  const nextPage = useCallback(() => {
    if (activeTab && activeTab.currentPage < activeTab.info.pages) {
      setCurrentPage(activeTab.currentPage + 1);
    }
  }, [activeTab, setCurrentPage]);

  const prevPage = useCallback(() => {
    if (activeTab && activeTab.currentPage > 1) {
      setCurrentPage(activeTab.currentPage - 1);
    }
  }, [activeTab, setCurrentPage]);

  const setZoom = useCallback((zoom: number) => {
    updateActiveTab((tab) => ({ ...tab, zoom: Math.max(0.4, Math.min(3.0, zoom)) }));
  }, [updateActiveTab]);

  const zoomIn = useCallback(() => {
    if (activeTab) setZoom(activeTab.zoom + 0.15);
  }, [activeTab, setZoom]);

  const zoomOut = useCallback(() => {
    if (activeTab) setZoom(activeTab.zoom - 0.15);
  }, [activeTab, setZoom]);

  const resetZoom = useCallback(() => {
    if (activeTab) setZoom(1.0);
  }, [activeTab, setZoom]);

  const printCurrentDocument = useCallback(async () => {
    if (!activeTab) return;
    try {
      await apiPrintDocument(activeTab.info.id);
    } catch (err) {
      alert(`Yazdırma hatası: ${String(err)}`);
    }
  }, [activeTab]);

  // Search
  const toggleSearch = useCallback(() => {
    setSearchOpen((prev) => !prev);
  }, []);

  const executeSearch = useCallback(
    async (query: string) => {
      const sequence = ++searchSeqRef.current;
      setSearchError(null);
      setSearchQuery(query);
      setSearchHits([]);
      setCurrentHitIndex(-1);
      if (!activeTab || !query.trim()) {
        setIsSearching(false);
        return;
      }
      setIsSearching(true);
      try {
        const hits = await apiSearchDocument(activeTab.info.id, query.trim());
        if (sequence !== searchSeqRef.current) return;
        setSearchHits(hits);
        if (hits.length > 0) {
          setCurrentHitIndex(0);
          setCurrentPage(hits[0].page);
        } else {
          setCurrentHitIndex(-1);
        }
      } catch (err) {
        if (sequence !== searchSeqRef.current) return;
        setSearchError(String(err));
        console.error('Search error:', err);
      } finally {
        if (sequence === searchSeqRef.current) setIsSearching(false);
      }
    },
    [activeTab, setCurrentPage]
  );

  const nextSearchHit = useCallback(() => {
    if (searchHits.length === 0) return;
    const nextIdx = (currentHitIndex + 1) % searchHits.length;
    setCurrentHitIndex(nextIdx);
    setCurrentPage(searchHits[nextIdx].page);
  }, [searchHits, currentHitIndex, setCurrentPage]);

  const prevSearchHit = useCallback(() => {
    if (searchHits.length === 0) return;
    const prevIdx = (currentHitIndex - 1 + searchHits.length) % searchHits.length;
    setCurrentHitIndex(prevIdx);
    setCurrentPage(searchHits[prevIdx].page);
  }, [searchHits, currentHitIndex, setCurrentPage]);

  const fetchPageText = useCallback(
    async (page?: number): Promise<string> => {
      if (!activeTab) return '';
      const targetPage = page ?? activeTab.currentPage;
      return apiPageText(activeTab.info.id, targetPage);
    },
    [activeTab]
  );

  // Password Prompt Handlers
  const submitPasswordPrompt = useCallback(
    async (password: string) => {
      if (!passwordPrompt.path) return;
      const success = await openPaths([passwordPrompt.path], password);
      if (success) {
        setPasswordPrompt({ path: '', isOpen: false });
        if (passwordPrompt.remainingPaths?.length) await openPaths(passwordPrompt.remainingPaths);
      }
    },
    [passwordPrompt.path, passwordPrompt.remainingPaths, openPaths]
  );

  const cancelPasswordPrompt = useCallback(() => {
    setPasswordPrompt({ path: '', isOpen: false });
  }, []);

  const addRedaction = useCallback((rect: RedactRect) => {
    updateActiveTab((tab) => ({ ...tab, redactions: [...tab.redactions, rect] }));
  }, [updateActiveTab]);

  const removeRedaction = useCallback((index: number) => {
    updateActiveTab((tab) => ({ ...tab, redactions: tab.redactions.filter((_, i) => i !== index) }));
  }, [updateActiveTab]);

  const clearRedactions = useCallback(() => {
    updateActiveTab((tab) => ({ ...tab, redactions: [] }));
  }, [updateActiveTab]);

  const addAnnotation = useCallback((annotation: Annotation) => {
    updateActiveTab((tab) => ({ ...tab, annotations: [...tab.annotations, annotation] }));
  }, [updateActiveTab]);

  const removeAnnotation = useCallback((index: number) => {
    updateActiveTab((tab) => ({ ...tab, annotations: tab.annotations.filter((_, i) => i !== index) }));
  }, [updateActiveTab]);

  const clearAnnotations = useCallback(() => {
    updateActiveTab((tab) => ({ ...tab, annotations: [] }));
  }, [updateActiveTab]);

  // CLI initial paths and listeners
  useEffect(() => {
    if (!bridgeAvailable) return;

    // Load initial paths passed via command line
    apiInitialPaths().then((paths) => {
      if (paths && paths.length > 0) {
        openPaths(paths);
      }
    });

    let unlistenOpenPaths: (() => void) | undefined;
    let unlistenDragDrop: (() => void) | undefined;

    listenOpenPaths((paths) => {
      openPaths(paths);
    }).then((unlisten) => {
      unlistenOpenPaths = unlisten;
    });

    setupNativeDragDrop((paths) => {
      openPaths(paths);
    }).then((unlisten) => {
      unlistenDragDrop = unlisten;
    });

    return () => {
      if (unlistenOpenPaths) unlistenOpenPaths();
      if (unlistenDragDrop) unlistenDragDrop();
    };
  }, [bridgeAvailable, openPaths]);

  // Global Keyboard Shortcuts (Ctrl+O, Ctrl+F, Ctrl+P)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || document.querySelector('[role="dialog"][aria-modal="true"]')) return;
      const isCtrl = e.ctrlKey || e.metaKey;
      if (e.ctrlKey && e.key === 'Tab' && tabs.length > 0) {
        e.preventDefault();
        const index = tabs.findIndex((tab) => tab.info.id === activeId);
        const next = (index + (e.shiftKey ? -1 : 1) + tabs.length) % tabs.length;
        selectTab(tabs[next].info.id);
        document.getElementById(`document-tab-${tabs[next].info.id}`)?.focus();
        return;
      }
      if (isCtrl && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        openFilesDialog();
      } else if (isCtrl && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        toggleSearch();
      } else if (isCtrl && e.key.toLowerCase() === 'p') {
        e.preventDefault();
        if (activeTab) {
          printCurrentDocument();
        }
      } else if (!isCtrl && !(e.target instanceof HTMLElement && e.target.closest('input,textarea,select,[contenteditable=true],[role=dialog]'))) {
        if (e.key === 'ArrowRight') { e.preventDefault(); nextPage(); }
        if (e.key === 'ArrowLeft') { e.preventDefault(); prevPage(); }
        if (e.key === 'PageDown') { e.preventDefault(); nextPage(); }
        if (e.key === 'PageUp') { e.preventDefault(); prevPage(); }
        if (e.key === 'Home' && activeTab) { e.preventDefault(); setCurrentPage(1); }
        if (e.key === 'End' && activeTab) { e.preventDefault(); setCurrentPage(activeTab.info.pages); }
        if (e.key === 'Escape') setSearchOpen(false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [openFilesDialog, toggleSearch, printCurrentDocument, activeTab, activeId, tabs, selectTab, nextPage, prevPage, setCurrentPage]);

  return (
    <DocumentContext.Provider
      value={{
        tabs,
        activeTab,
        activeRenderedPage,
        isRendering,
        renderError,
        retryRender,
        recentPaths,
        removeRecentPath,
        clearRecentPaths,
        passwordPrompt,
        searchOpen,
        searchQuery,
        searchHits,
        currentHitIndex,
        isSearching,
        searchError,
        bridgeAvailable,
        openFilesDialog,
        openPaths,
        closeTab,
        selectTab,
        setCurrentPage,
        nextPage,
        prevPage,
        setZoom,
        zoomIn,
        zoomOut,
        resetZoom,
        printCurrentDocument,
        setSearchOpen,
        toggleSearch,
        setSearchQuery,
        executeSearch,
        nextSearchHit,
        prevSearchHit,
        fetchPageText,
        submitPasswordPrompt,
        cancelPasswordPrompt,
        addRedaction,
        removeRedaction,
        clearRedactions,
        addAnnotation,
        removeAnnotation,
        clearAnnotations,
      }}
    >
      {children}
    </DocumentContext.Provider>
  );
};

export function useDocument(): DocumentContextValue {
  const ctx = useContext(DocumentContext);
  if (!ctx) {
    throw new Error('useDocument must be used within DocumentProvider');
  }
  return ctx;
}
