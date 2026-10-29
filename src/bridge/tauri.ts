import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { translateEngineError, translateEngineMessage } from '../i18n/engineMessages';

/** Every command error reaches the UI in Turkish. */
async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    throw translateEngineError(error);
  }
}
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open, save } from '@tauri-apps/plugin-dialog';
import type {
  DocumentInfo,
  RenderedPage,
  SearchHit,
  OperationRequest,
  OperationResult,
  Progress,
} from '../types';

export function isTauriAvailable(): boolean {
  if (typeof window === 'undefined') return false;
  return '__TAURI_INTERNALS__' in window || '__TAURI__' in window;
}

function checkBridge() {
  if (!isTauriAvailable()) {
    throw new Error(
      'Yerel Tauri köprüsü kullanılamıyor (Native bridge unavailable). ZoriaPDF masaüstü uygulamasında çalıştırılmalıdır.'
    );
  }
}

export async function apiOpenDocuments(
  paths: string[],
  password?: string
): Promise<DocumentInfo[]> {
  checkBridge();
  return await invoke<DocumentInfo[]>('open_documents', { paths, password });
}

export async function apiCloseDocument(id: string): Promise<void> {
  checkBridge();
  return await invoke<void>('close_document', { id });
}

export async function apiRenderPage(
  id: string,
  page: number,
  width: number
): Promise<RenderedPage> {
  checkBridge();
  return await invoke<RenderedPage>('render_page', { id, page, width });
}

export async function apiPageText(id: string, page: number): Promise<string> {
  checkBridge();
  return await invoke<string>('page_text', { id, page });
}

export async function apiSearchDocument(
  id: string,
  query: string
): Promise<SearchHit[]> {
  checkBridge();
  return await invoke<SearchHit[]>('search_document', { id, query });
}

export async function apiRunOperation(
  request: OperationRequest
): Promise<OperationResult> {
  checkBridge();
  const result = await invoke<OperationResult>('run_operation', { request });
  return result?.message ? { ...result, message: translateEngineMessage(result.message) } : result;
}

export async function apiCancelOperation(id: string): Promise<void> {
  checkBridge();
  return await invoke<void>('cancel_operation', { id });
}

export interface OcrStatus {
  available: boolean;
  enginePath?: string;
  pack: { url: string; sha256: string; sizeMb: number };
}

export async function apiOcrStatus(): Promise<OcrStatus | null> {
  if (!isTauriAvailable()) return null;
  try {
    return await invoke<OcrStatus>('ocr_status');
  } catch {
    return null;
  }
}

export async function apiInitialPaths(): Promise<string[]> {
  if (!isTauriAvailable()) return [];
  try {
    return await invoke<string[]>('initial_paths');
  } catch {
    return [];
  }
}

export async function apiPrintDocument(id: string): Promise<void> {
  checkBridge();
  return await invoke<void>('print_document', { id });
}

export async function apiRevealOutput(path: string): Promise<void> {
  checkBridge();
  return await invoke<void>('reveal_output', { path });
}

export async function pickPdfFiles(multiple = true): Promise<string[] | null> {
  checkBridge();
  const res = await open({
    title: 'PDF Belgesi Seçin',
    multiple,
    filters: [{ name: 'PDF Belgeleri', extensions: ['pdf'] }],
  });
  if (!res) return null;
  return Array.isArray(res) ? res : [res];
}

export async function pickImageFiles(multiple = true): Promise<string[] | null> {
  checkBridge();
  const res = await open({
    title: 'Görselleri Seçin',
    multiple,
    filters: [{ name: 'Görseller', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
  });
  if (!res) return null;
  return Array.isArray(res) ? res : [res];
}

export async function pickSavePdfPath(defaultName = 'cikti.pdf'): Promise<string | null> {
  checkBridge();
  const res = await save({
    title: 'Kaydedilecek PDF Konumunu Seçin',
    defaultPath: defaultName,
    filters: [{ name: 'PDF Belgesi', extensions: ['pdf'] }],
  });
  return res || null;
}

export async function pickSaveTextPath(defaultName = 'cikti.txt'): Promise<string | null> {
  checkBridge();
  const res = await save({
    title: 'Metin Dosyası Konumunu Seçin',
    defaultPath: defaultName,
    filters: [{ name: 'Metin Dosyası', extensions: ['txt'] }],
  });
  return res || null;
}

export async function pickWordFile(): Promise<string | null> {
  checkBridge();
  const res = await open({
    title: 'Word Belgesi Seçin',
    multiple: false,
    filters: [{ name: 'Word Belgeleri', extensions: ['docx'] }],
  });
  if (!res) return null;
  return Array.isArray(res) ? res[0] : res;
}

export async function pickSaveDocxPath(defaultName = 'cikti.docx'): Promise<string | null> {
  checkBridge();
  const res = await save({
    title: 'Kaydedilecek Word Konumunu Seçin',
    defaultPath: defaultName,
    filters: [{ name: 'Word Belgesi', extensions: ['docx'] }],
  });
  return res || null;
}

export async function pickDirectory(title = 'Çıktı Klasörünü Seçin'): Promise<string | null> {
  checkBridge();
  const res = await open({
    title,
    directory: true,
    multiple: false,
  });
  if (!res) return null;
  return Array.isArray(res) ? res[0] : res;
}

export async function listenJobProgress(
  callback: (progress: Progress) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return () => {};
  return await listen<Progress>('job-progress', (event) => {
    const progress = event.payload;
    callback(progress?.message ? { ...progress, message: translateEngineMessage(progress.message) } : progress);
  });
}

export async function listenOpenPaths(
  callback: (paths: string[]) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return () => {};
  return await listen<{ paths: string[] }>('open-paths', (event) => {
    if (event.payload && Array.isArray(event.payload.paths)) {
      callback(event.payload.paths);
    }
  });
}

export async function setupNativeDragDrop(
  callback: (paths: string[]) => void
): Promise<UnlistenFn> {
  if (!isTauriAvailable()) return () => {};
  try {
    const webview = getCurrentWebview();
    const unlisten = await webview.onDragDropEvent((event) => {
      if (event.payload.type === 'drop' && event.payload.paths.length > 0) {
        callback(event.payload.paths);
      }
    });
    return unlisten;
  } catch (err) {
    console.warn('Tauri onDragDropEvent could not be attached:', err);
    return () => {};
  }
}
