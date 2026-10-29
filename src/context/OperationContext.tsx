import React, {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  useRef,
} from 'react';
import type {
  OperationKind,
  OperationRequest,
  OperationResult,
  Progress,
} from '../types';
import {
  apiRunOperation,
  apiCancelOperation,
  apiRevealOutput,
  listenJobProgress,
  isTauriAvailable,
} from '../bridge/tauri';
import { useDocument } from './DocumentContext';

interface ActiveJob {
  id: string;
  kind: OperationKind;
  request: OperationRequest;
  progress?: Progress;
  startTime: number;
}

interface LastResultInfo {
  outputs: string[];
  message: string;
  kind: OperationKind;
  timestamp: number;
}

interface OperationContextValue {
  activeTool: OperationKind | null;
  setActiveTool: (tool: OperationKind | null) => void;
  activeJob: ActiveJob | null;
  isRunning: boolean;
  lastResult: LastResultInfo | null;
  clearLastResult: () => void;
  errorMessage: string | null;
  clearError: () => void;

  executeOperation: (
    kind: OperationKind,
    params: Omit<OperationRequest, 'id' | 'kind'>
  ) => Promise<OperationResult | null>;
  cancelActiveJob: () => Promise<void>;
  revealResult: (path: string) => Promise<void>;
  openResultInTab: (path: string) => Promise<void>;
}

const OperationContext = createContext<OperationContextValue | null>(null);

export const OperationProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { openPaths } = useDocument();
  const [activeTool, setActiveTool] = useState<OperationKind | null>(null);
  const [activeJob, setActiveJob] = useState<ActiveJob | null>(null);
  const [lastResult, setLastResult] = useState<LastResultInfo | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const activeJobRef = useRef<ActiveJob | null>(null);

  useEffect(() => {
    if (!isTauriAvailable()) return;

    let unlisten: (() => void) | undefined;
    listenJobProgress((progress: Progress) => {
      if (activeJobRef.current && activeJobRef.current.id === progress.id) {
        setActiveJob((prev) => (prev ? { ...prev, progress } : null));
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const clearLastResult = useCallback(() => setLastResult(null), []);
  const clearError = useCallback(() => setErrorMessage(null), []);

  const executeOperation = useCallback(
    async (
      kind: OperationKind,
      params: Omit<OperationRequest, 'id' | 'kind'>
    ): Promise<OperationResult | null> => {
      if (activeJobRef.current) {
        setErrorMessage('Zaten çalışan bir işlem var. Lütfen tamamlanmasını veya iptal edilmesini bekleyin.');
        return null;
      }

      setErrorMessage(null);
      setLastResult(null);

      // Invariant Check: Output collision prevention
      const outputNormalized = (params.output || '').trim().toLowerCase();
      const inputCollision = (params.inputs || []).some(
        (inp) => inp.trim().toLowerCase() === outputNormalized
      );

      if (inputCollision) {
        const collisionErr =
          'Orijinal girdi dosyasının üzerine yazılamaz. Lütfen farklı bir çıktı dosya adı veya konumu seçin.';
        setErrorMessage(collisionErr);
        return null;
      }

      const jobId = `job_${Date.now()}_${Math.random().toString(36).substring(2, 6)}`;
      const request: OperationRequest = {
        id: jobId,
        kind,
        ...params,
      };

      const job: ActiveJob = {
        id: jobId,
        kind,
        request,
        startTime: Date.now(),
      };

      activeJobRef.current = job;
      setActiveJob(job);

      try {
        const result = await apiRunOperation(request);
        setLastResult({
          outputs: result.outputs,
          message: result.message || 'İşlem başarıyla tamamlandı.',
          kind,
          timestamp: Date.now(),
        });
        return result;
      } catch (err: unknown) {
        const msg = String(err || '');
        if (msg.includes('cancelled') || msg.includes('iptal')) {
          setErrorMessage('İşlem kullanıcı tarafından iptal edildi.');
        } else {
          setErrorMessage(`İşlem başarısız: ${msg}`);
        }
        return null;
      } finally {
        if (activeJobRef.current?.id === jobId) {
          activeJobRef.current = null;
          setActiveJob(null);
        }
      }
    },
    []
  );

  const cancelActiveJob = useCallback(async () => {
    if (!activeJob) return;
    try {
      await apiCancelOperation(activeJob.id);
      setErrorMessage('İşlem iptal edildi.');
    } catch (err) {
      console.error('Cancel operation failed:', err);
    }
  }, [activeJob]);

  const revealResult = useCallback(async (path: string) => {
    try {
      await apiRevealOutput(path);
    } catch (err) {
      console.error('Reveal output failed:', err);
    }
  }, []);

  const openResultInTab = useCallback(
    async (path: string) => {
      if (path.toLowerCase().endsWith('.pdf')) {
        await openPaths([path]);
      }
    },
    [openPaths]
  );

  return (
    <OperationContext.Provider
      value={{
        activeTool,
        setActiveTool,
        activeJob,
        isRunning: !!activeJob,
        lastResult,
        clearLastResult,
        errorMessage,
        clearError,
        executeOperation,
        cancelActiveJob,
        revealResult,
        openResultInTab,
      }}
    >
      {children}
    </OperationContext.Provider>
  );
};

export function useOperation(): OperationContextValue {
  const ctx = useContext(OperationContext);
  if (!ctx) {
    throw new Error('useOperation must be used within OperationProvider');
  }
  return ctx;
}
